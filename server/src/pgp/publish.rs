//! Publish a public key to keys.openpgp.org (Hagrid).
//!
//! Flow (two HTTP calls):
//!   1. `POST /vks/v1/upload` — hands Hagrid the armored key, gets
//!      back a token + a per-email-address status map. Fresh uploads
//!      come back with every address in state "unpublished" — Hagrid
//!      won't serve the key to external lookups until the owner
//!      proves each address via a verify link.
//!   2. `POST /vks/v1/request-verify` — asks Hagrid to mail the
//!      owner a verification link for each address we care about.
//!      Clicking the link (done out-of-band, in their mail client)
//!      flips the address to "published" and the key becomes
//!      retrievable by that email.
//!
//! Why this matters for Postern: Proton (and other non-Autocrypt
//! clients) look up external correspondents on keys.openpgp.org
//! when deciding whether to encrypt a reply. Without this step a
//! Postern → Proton thread stays one-way encrypted.
//!
//! API reference: <https://keys.openpgp.org/about/api>.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

const HAGRID_BASE: &str = "https://keys.openpgp.org";
const HTTP_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Serialize, Deserialize)]
pub struct PublishResult {
    /// Fingerprint Hagrid reports back — cross-check against the
    /// local key's fingerprint, protects against mistakes during
    /// armor handling.
    pub key_fpr: String,
    /// Addresses Hagrid just mailed verification links to.
    pub verification_sent: Vec<String>,
    /// Addresses Hagrid already considered published before this
    /// call — no new verify email needed.
    pub already_published: Vec<String>,
    /// Browser URL where the user (or anyone) can fetch the key.
    pub key_url: String,
}

/// Two-step publish to Hagrid. Returns when both the upload and the
/// request-verify have returned 200. The user still has to click
/// the verify link that lands in their inbox — this function just
/// gets the link sent.
///
/// `addresses` should be the email identities the user wants public
/// on the keyserver. Pass an empty slice to skip verification and
/// only upload (key sits in an "unpublished" state).
///
/// `bind_iface` should be the VPN-bound interface when active —
/// same pattern as the WKD discover path, so the lookup goes
/// through the tunnel.
pub async fn publish_to_hagrid(
    armored_public: &str,
    addresses: &[String],
    bind_iface: Option<&str>,
) -> Result<PublishResult> {
    let client = build_client(bind_iface)?;

    // ── 1. Upload ─────────────────────────────────────────────────
    let upload_req = UploadRequest {
        keytext: armored_public.to_owned(),
    };
    let upload_url = format!("{HAGRID_BASE}/vks/v1/upload");
    let upload: UploadResponse = client
        .post(&upload_url)
        .json(&upload_req)
        .send()
        .await
        .map_err(|e| Error::Other(anyhow::anyhow!("hagrid upload send: {e}")))?
        .error_for_status()
        .map_err(|e| Error::Other(anyhow::anyhow!("hagrid upload status: {e}")))?
        .json()
        .await
        .map_err(|e| Error::Other(anyhow::anyhow!("hagrid upload decode: {e}")))?;

    // Classify the addresses Hagrid already knows about vs ones we
    // still need to prove. Everything that comes back as "published"
    // or "revoked" is already public; "pending" is mid-verify from
    // an earlier run; "unpublished" is the fresh case we just
    // created with the upload and need to ask Hagrid to mail links
    // for.
    let mut already_published: Vec<String> = Vec::new();
    let mut needs_verify: Vec<String> = Vec::new();
    for addr in addresses {
        let key = addr.to_ascii_lowercase();
        match upload.status.get(&key).map(String::as_str) {
            Some("published") => already_published.push(addr.clone()),
            Some("revoked") => already_published.push(addr.clone()),
            _ => needs_verify.push(addr.clone()),
        }
    }

    let key_url = format!("{HAGRID_BASE}/search?q={}", upload.key_fpr);

    // ── 2. Request verify (only if needed) ────────────────────────
    let mut verification_sent: Vec<String> = Vec::new();
    if !needs_verify.is_empty() {
        let verify_req = VerifyRequest {
            token: upload.token.clone(),
            addresses: needs_verify.clone(),
        };
        let verify_url = format!("{HAGRID_BASE}/vks/v1/request-verify");
        let resp = client
            .post(&verify_url)
            .json(&verify_req)
            .send()
            .await
            .map_err(|e| Error::Other(anyhow::anyhow!("hagrid verify send: {e}")))?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(Error::Other(anyhow::anyhow!(
                "hagrid request-verify returned {status}: {}",
                body.chars().take(200).collect::<String>()
            )));
        }
        verification_sent = needs_verify;
    }

    Ok(PublishResult {
        key_fpr: upload.key_fpr,
        verification_sent,
        already_published,
        key_url,
    })
}

#[derive(Serialize)]
struct UploadRequest {
    keytext: String,
}

#[derive(Deserialize)]
struct UploadResponse {
    key_fpr: String,
    token: String,
    /// `email -> ("pending"|"published"|"revoked"|"unpublished"|"unknown")`
    #[serde(default)]
    status: std::collections::HashMap<String, String>,
}

#[derive(Serialize)]
struct VerifyRequest {
    token: String,
    addresses: Vec<String>,
}

fn build_client(bind_iface: Option<&str>) -> Result<reqwest::Client> {
    let mut b = reqwest::Client::builder()
        .user_agent("Postern/0.1 (+pgp-publish)")
        .timeout(HTTP_TIMEOUT);
    if let Some(name) = bind_iface {
        b = b.interface(name);
    }
    b.build()
        .map_err(|e| Error::Other(anyhow::anyhow!("hagrid client: {e}")))
}

/// Ask Hagrid whether a given email address has a published key.
/// Maps Hagrid's responses onto a tight enum the UI can render:
///   - 200 with body → Published
///   - 404 / empty → NotFound
///   - anything else → Unknown (network / server error — UI treats
///     it as "couldn't check right now" instead of "no key")
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum KeyserverPresence {
    Published,
    NotFound,
    Unknown,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct KeyserverStatus {
    pub email: String,
    pub presence: KeyserverPresence,
}

pub async fn check_keyserver(emails: &[String], bind_iface: Option<&str>) -> Vec<KeyserverStatus> {
    let Ok(client) = build_client(bind_iface) else {
        return emails
            .iter()
            .map(|e| KeyserverStatus {
                email: e.clone(),
                presence: KeyserverPresence::Unknown,
            })
            .collect();
    };

    let mut out = Vec::with_capacity(emails.len());
    // Serial rather than parallel — it's a handful of addresses and
    // Hagrid is single-machine; hitting it with many in flight is
    // rude. A 3-account scan takes well under 2 seconds.
    for email in emails {
        let url = format!("{HAGRID_BASE}/vks/v1/by-email/{}", urlencode_path(email),);
        let presence = match client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => KeyserverPresence::Published,
            Ok(resp) if resp.status().as_u16() == 404 => KeyserverPresence::NotFound,
            _ => KeyserverPresence::Unknown,
        };
        out.push(KeyserverStatus {
            email: email.clone(),
            presence,
        });
    }
    out
}

use crate::net::urlencode as urlencode_path;
