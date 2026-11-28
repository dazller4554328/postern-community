use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    sync::{Arc, Mutex, RwLock},
    time::{Duration, Instant},
};

use rand::{rngs::OsRng, RngCore};
use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT};

use crate::error::{Error, Result};

pub type ProxyToken = String;

const TOKEN_TTL: Duration = Duration::from_secs(60 * 60 * 24);
const MAX_FETCH_BYTES: usize = 20 * 1024 * 1024;
const FETCH_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_REDIRECTS: usize = 3;

struct Entry {
    url: String,
    created_at: Instant,
}

/// Ephemeral URL→token registry plus the HTTP client that actually
/// fetches remote resources. Tokens live in memory — if the server
/// restarts, images need to be shown again (which re-mints tokens).
///
/// Privacy properties preserved by the fetcher:
/// - Never sends cookies (reqwest without the `cookies` feature).
/// - Never sends Referer (we don't set one and default is empty).
/// - Never sends authorization (we don't propagate inbound headers).
/// - When VPN is enabled, egress is bound to `wg0` via `SO_BINDTODEVICE`
///   (reqwest `.interface()`). If the interface is missing, fetches fail
///   closed — kill-switch semantics.
#[derive(Clone)]
pub struct ImageProxy {
    tokens: Arc<Mutex<HashMap<ProxyToken, Entry>>>,
    bound_iface: Arc<RwLock<Option<String>>>,
}

impl ImageProxy {
    pub fn new() -> Self {
        Self {
            tokens: Arc::new(Mutex::new(HashMap::new())),
            bound_iface: Arc::new(RwLock::new(None)),
        }
    }

    pub fn mint(&self, url: &str) -> ProxyToken {
        let mut bytes = [0u8; 16];
        OsRng.fill_bytes(&mut bytes);
        let token = hex::encode(bytes);
        let mut map = self
            .tokens
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        map.insert(
            token.clone(),
            Entry {
                url: url.to_owned(),
                created_at: Instant::now(),
            },
        );
        token
    }

    pub fn resolve(&self, token: &str) -> Option<String> {
        let mut map = self
            .tokens
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        map.retain(|_, e| e.created_at.elapsed() < TOKEN_TTL);
        map.get(token).map(|e| e.url.clone())
    }

    /// Bind future fetches' egress to the named interface, or restore
    /// default-route egress when `None`. Fetch clients are built per
    /// request (they pin the vetted IP), so this just records the name;
    /// a missing interface still fails closed at connect time.
    pub fn bind_interface(&self, iface: Option<&str>) -> Result<()> {
        *self
            .bound_iface
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = iface.map(str::to_owned);
        tracing::info!(iface = ?iface, "image proxy egress rebind");
        Ok(())
    }

    #[must_use]
    pub fn bound_interface(&self) -> Option<String> {
        self.bound_iface
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    pub async fn fetch(&self, url: &str) -> Result<FetchedImage> {
        let mut headers = HeaderMap::new();
        headers.insert("accept", HeaderValue::from_static("image/*,*/*;q=0.5"));
        headers.insert(
            USER_AGENT,
            HeaderValue::from_static("Postern/0.1 (+privacy-proxy)"),
        );

        // SSRF guard: email content chooses these URLs, so every hop —
        // including each redirect target — must resolve to a public
        // address before we connect. The connection is pinned to the
        // vetted IP (reqwest `.resolve()`), so a DNS rebind between our
        // check and reqwest's own lookup can't redirect the request into
        // the container network. Redirects are followed manually for the
        // same reason; the shared client has redirects disabled.
        let iface = self.bound_interface();
        let mut current: reqwest::Url = url
            .parse()
            .map_err(|e| Error::BadRequest(format!("proxy url: {e}")))?;
        let mut resp = None;
        for _hop in 0..=MAX_REDIRECTS {
            let (host, addr) = vet_proxy_target(&current).await?;
            let client = build_pinned_client(iface.as_deref(), &host, addr)?;
            let r = client
                .get(current.clone())
                .headers(headers.clone())
                .send()
                .await
                .map_err(|e| Error::Other(anyhow::anyhow!("proxy fetch {current}: {e}")))?;
            if r.status().is_redirection() {
                let loc = r
                    .headers()
                    .get("location")
                    .and_then(|v| v.to_str().ok())
                    .ok_or_else(|| Error::Other(anyhow::anyhow!("proxy redirect without location")))?;
                current = current
                    .join(loc)
                    .map_err(|e| Error::Other(anyhow::anyhow!("proxy redirect url: {e}")))?;
                continue;
            }
            resp = Some(r);
            break;
        }
        let resp =
            resp.ok_or_else(|| Error::Other(anyhow::anyhow!("proxy: too many redirects")))?;

        if !resp.status().is_success() {
            return Err(Error::Other(anyhow::anyhow!(
                "proxy fetch {url}: status {}",
                resp.status()
            )));
        }

        let ctype = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .map_or_else(|| "application/octet-stream".into(), str::to_owned);

        if !ctype.starts_with("image/") {
            return Err(Error::Other(anyhow::anyhow!(
                "proxy refused non-image content-type: {ctype}"
            )));
        }

        let bytes = resp
            .bytes()
            .await
            .map_err(|e| Error::Other(anyhow::anyhow!("proxy read body: {e}")))?;
        if bytes.len() > MAX_FETCH_BYTES {
            return Err(Error::Other(anyhow::anyhow!(
                "proxy body too large: {}",
                bytes.len()
            )));
        }
        Ok(FetchedImage {
            bytes: bytes.to_vec(),
            content_type: ctype,
        })
    }
}

impl Default for ImageProxy {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse + vet a proxy target: http/https only, hostname resolved, and
/// every resolved address must be public. Returns the hostname and the
/// vetted address the connection must be pinned to.
async fn vet_proxy_target(url: &reqwest::Url) -> Result<(String, SocketAddr)> {
    match url.scheme() {
        "http" | "https" => {}
        s => return Err(Error::BadRequest(format!("proxy refused scheme: {s}"))),
    }
    let host = url
        .host_str()
        .ok_or_else(|| Error::BadRequest("proxy url has no host".into()))?
        .to_owned();
    let port = url
        .port_or_known_default()
        .ok_or_else(|| Error::BadRequest("proxy url has no port".into()))?;
    let addrs: Vec<SocketAddr> = match host.trim_matches(['[', ']']).parse::<IpAddr>() {
        Ok(ip) => vec![SocketAddr::new(ip, port)],
        Err(_) => tokio::net::lookup_host((host.as_str(), port))
            .await
            .map_err(|e| Error::Other(anyhow::anyhow!("proxy dns {host}: {e}")))?
            .collect(),
    };
    // ALL addresses must be public — a mixed record set is exactly what a
    // rebinding/split-horizon attack looks like.
    if addrs.is_empty() || !addrs.iter().all(|a| is_public_ip(a.ip())) {
        return Err(Error::BadRequest(format!(
            "proxy refused non-public address for {host}"
        )));
    }
    Ok((host, addrs[0]))
}

/// Reject loopback, RFC-1918, link-local (incl. 169.254 metadata), CGNAT,
/// ULA, documentation/benchmark ranges, multicast, and unspecified.
fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            !(v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_multicast()
                || v4.is_unspecified()
                || v4.is_documentation()
                || o[0] == 0
                || (o[0] == 100 && (o[1] & 0xc0) == 64) // 100.64.0.0/10 CGNAT
                || (o[0] == 192 && o[1] == 0 && o[2] == 0) // 192.0.0.0/24
                || (o[0] == 198 && (o[1] & 0xfe) == 18) // 198.18.0.0/15
                || o[0] >= 240) // 240.0.0.0/4 reserved
        }
        IpAddr::V6(v6) => {
            if let Some(mapped) = v6.to_ipv4_mapped() {
                return is_public_ip(IpAddr::V4(mapped));
            }
            let seg = v6.segments();
            !(v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || (seg[0] & 0xfe00) == 0xfc00 // fc00::/7 unique-local
                || (seg[0] & 0xffc0) == 0xfe80 // fe80::/10 link-local
                || (seg[0] == 0x2001 && seg[1] == 0xdb8)) // documentation
        }
    }
}

/// Per-fetch client: pins `host` to the vetted address so the connection
/// can only reach what `vet_proxy_target` approved, and binds the VPN
/// interface when one is set (SO_BINDTODEVICE — a missing interface fails
/// at connect, kill-switch semantics).
fn build_pinned_client(
    iface: Option<&str>,
    host: &str,
    addr: SocketAddr,
) -> Result<reqwest::Client> {
    let mut b = reqwest::Client::builder()
        .user_agent("Postern/0.1 (+privacy-proxy)")
        .timeout(FETCH_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .resolve(host, addr);

    if let Some(name) = iface {
        b = b.interface(name);
    }

    b.build()
        .map_err(|e| Error::Other(anyhow::anyhow!("reqwest client build: {e}")))
}

pub struct FetchedImage {
    pub bytes: Vec<u8>,
    pub content_type: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mint_and_resolve_roundtrip() {
        let p = ImageProxy::new();
        let t = p.mint("https://example.com/a.png");
        assert_eq!(p.resolve(&t).as_deref(), Some("https://example.com/a.png"));
    }

    #[test]
    fn unknown_token_returns_none() {
        let p = ImageProxy::new();
        assert!(p.resolve("not-a-real-token").is_none());
    }

    #[test]
    fn bind_then_unbind() {
        let p = ImageProxy::new();
        assert_eq!(p.bound_interface(), None);
        // Can't actually bind to a nonexistent iface without caps in unit
        // tests — just assert the unbind path works.
        p.bind_interface(None).unwrap();
        assert_eq!(p.bound_interface(), None);
    }

    #[test]
    fn public_ip_check_blocks_internal_ranges() {
        let blocked = [
            "127.0.0.1",
            "10.0.0.5",
            "172.16.0.1",
            "192.168.1.1",
            "169.254.169.254", // cloud metadata
            "100.75.82.52",    // CGNAT / Tailscale
            "0.0.0.0",
            "255.255.255.255",
            "198.18.0.1",
            "::1",
            "fe80::1",
            "fd00::1",
            "::ffff:192.168.1.1", // v4-mapped must unwrap
        ];
        for s in blocked {
            assert!(!is_public_ip(s.parse().unwrap()), "should block {s}");
        }
        for s in ["93.184.216.34", "8.8.8.8", "2606:4700::6810:84e5"] {
            assert!(is_public_ip(s.parse().unwrap()), "should allow {s}");
        }
    }

    #[tokio::test]
    async fn vet_refuses_non_http_and_ip_literals_in_private_space() {
        for url in [
            "file:///etc/passwd",
            "ftp://example.com/x.png",
            "http://127.0.0.1/x.png",
            "http://[::1]/x.png",
            "http://169.254.169.254/latest/meta-data",
            "http://192.168.0.10:8080/a.gif",
        ] {
            let u: reqwest::Url = url.parse().unwrap();
            assert!(vet_proxy_target(&u).await.is_err(), "should refuse {url}");
        }
    }
}
