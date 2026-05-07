//! Recipient key discovery. Order: local keyring → WKD → public keyserver.
//!
//! WKD (Web Key Directory) is preferred — it's under the recipient's own
//! domain authority, so a key served there is cryptographically
//! attested by the domain owner. The keyserver fallback (keys.openpgp.org)
//! is best-effort.

use std::time::Duration;

use serde::Serialize;
use sha1::{Digest, Sha1};

use crate::error::{Error, Result};

const HTTP_TIMEOUT: Duration = Duration::from_secs(10);
const KEYSERVER_BASE: &str = "https://keys.openpgp.org";

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DiscoverySource {
    Wkd,
    Keyserver,
    NotFound,
}

#[derive(Debug, Serialize)]
pub struct DiscoveryResult {
    pub source: DiscoverySource,
    pub armored_public_key: Option<String>,
    pub url_tried: Vec<String>,
}

pub async fn discover_key(email: &str, bind_iface: Option<&str>) -> Result<DiscoveryResult> {
    let normalized = email.trim().to_ascii_lowercase();
    let Some((local, domain)) = normalized.split_once('@') else {
        return Err(Error::BadRequest(format!("not a valid email: {email}")));
    };

    let mut tried: Vec<String> = Vec::new();
    let client = build_client(bind_iface)?;

    // 1. WKD — advanced (subdomain) then direct.
    for url in wkd_candidates(local, domain) {
        tried.push(url.clone());
        match fetch_key(&client, &url).await {
            Ok(Some(armored)) => {
                return Ok(DiscoveryResult {
                    source: DiscoverySource::Wkd,
                    armored_public_key: Some(armored),
                    url_tried: tried,
                });
            }
            Ok(None) => continue,
            Err(_e) => continue,
        }
    }

    // 2. keys.openpgp.org (Verifying Keyserver).
    let ks = format!(
        "{KEYSERVER_BASE}/vks/v1/by-email/{}",
        urlencode(&normalized)
    );
    tried.push(ks.clone());
    if let Ok(Some(armored)) = fetch_key(&client, &ks).await {
        return Ok(DiscoveryResult {
            source: DiscoverySource::Keyserver,
            armored_public_key: Some(armored),
            url_tried: tried,
        });
    }

    Ok(DiscoveryResult {
        source: DiscoverySource::NotFound,
        armored_public_key: None,
        url_tried: tried,
    })
}

fn wkd_candidates(local: &str, domain: &str) -> Vec<String> {
    let hashed = wkd_local_part_hash(local);
    let encoded_local = urlencode(local);
    vec![
        // Advanced method — well-known under openpgpkey subdomain.
        format!(
            "https://openpgpkey.{domain}/.well-known/openpgpkey/{domain}/hu/{hashed}?l={encoded_local}"
        ),
        // Direct method — well-known under the domain root.
        format!(
            "https://{domain}/.well-known/openpgpkey/hu/{hashed}?l={encoded_local}"
        ),
    ]
}

/// z-base-32 SHA-1 of the lowercased local part, per
/// <https://wiki.gnupg.org/WKD>.
fn wkd_local_part_hash(local: &str) -> String {
    let digest = {
        let mut h = Sha1::new();
        h.update(local.to_ascii_lowercase().as_bytes());
        h.finalize()
    };
    zbase32_encode(&digest)
}

const ZBASE32_ALPHABET: &[u8] = b"ybndrfg8ejkmcpqxot1uwisza345h769";

fn zbase32_encode(bytes: &[u8]) -> String {
    let mut out = String::new();
    let mut buffer: u32 = 0;
    let mut bits: u32 = 0;
    for b in bytes {
        buffer = (buffer << 8) | u32::from(*b);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            let idx = ((buffer >> bits) & 0x1f) as usize;
            out.push(ZBASE32_ALPHABET[idx] as char);
        }
    }
    if bits > 0 {
        let idx = ((buffer << (5 - bits)) & 0x1f) as usize;
        out.push(ZBASE32_ALPHABET[idx] as char);
    }
    out
}

use crate::net::urlencode;

fn build_client(bind_iface: Option<&str>) -> Result<reqwest::Client> {
    let mut b = reqwest::Client::builder()
        .user_agent("Postern/0.1 (+pgp-discover)")
        .timeout(HTTP_TIMEOUT)
        .redirect(reqwest::redirect::Policy::limited(3));
    // When the VPN is active we MUST bind to wg0. Otherwise DNS
    // resolution hits /etc/resolv.conf pointing at the in-tunnel
    // resolver (10.2.0.1 etc.) and reqwest's default-route socket
    // can't reach it — the request silently fails and we surface
    // "NotFound" even for addresses that have perfectly discoverable
    // keys. Matches what ImageProxy / open_tcp already do.
    if let Some(name) = bind_iface {
        b = b.interface(name);
    }
    b.build()
        .map_err(|e| Error::Other(anyhow::anyhow!("pgp discover client: {e}")))
}

/// Fetch a URL that should return either:
///   - application/pgp-keys (binary) — we armor it
///   - text/plain (already armored) — passthrough
///   - application/octet-stream — try both
async fn fetch_key(client: &reqwest::Client, url: &str) -> Result<Option<String>> {
    let resp = client.get(url).send().await.ok();
    let Some(resp) = resp else { return Ok(None) };
    if !resp.status().is_success() {
        return Ok(None);
    }

    let ctype = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    let body = match resp.bytes().await {
        Ok(b) => b,
        Err(_) => return Ok(None),
    };

    // Already armored
    if ctype.starts_with("text/") || starts_with_armor_header(&body) {
        if let Ok(s) = std::str::from_utf8(&body) {
            if s.contains("BEGIN PGP PUBLIC KEY BLOCK") {
                return Ok(Some(s.to_owned()));
            }
        }
    }

    // Binary → armor
    let armored = armor_public_key(&body);
    Ok(Some(armored))
}

fn starts_with_armor_header(b: &[u8]) -> bool {
    b.windows("-----BEGIN PGP".len())
        .any(|w| w == b"-----BEGIN PGP")
}

fn armor_public_key(raw: &[u8]) -> String {
    use base64::{engine::general_purpose::STANDARD as B64, Engine};
    let b64 = B64.encode(raw);
    let mut out = String::with_capacity(b64.len() + 128);
    out.push_str("-----BEGIN PGP PUBLIC KEY BLOCK-----\n");
    out.push_str("Comment: Postern — discovered via WKD/keyserver\n\n");
    for chunk in b64.as_bytes().chunks(64) {
        out.push_str(std::str::from_utf8(chunk).unwrap_or(""));
        out.push('\n');
    }
    out.push_str("-----END PGP PUBLIC KEY BLOCK-----\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wkd_hash_matches_reference() {
        // "Joe.Doe" → "iy9q119eutrkn8s1mk4r39qejnbu3n5q" (GnuPG test vector).
        assert_eq!(
            wkd_local_part_hash("Joe.Doe"),
            "iy9q119eutrkn8s1mk4r39qejnbu3n5q"
        );
    }

    #[test]
    fn wkd_candidates_have_correct_shape() {
        let urls = wkd_candidates("alice", "example.com");
        assert!(urls[0].contains("openpgpkey.example.com"));
        assert!(urls[0].contains(".well-known/openpgpkey/example.com/hu/"));
        assert!(urls[1].contains("example.com/.well-known/openpgpkey/hu/"));
        for u in &urls {
            assert!(u.contains("l=alice"));
        }
    }
}
