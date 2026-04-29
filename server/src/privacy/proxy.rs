use std::{
    collections::HashMap,
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
/// - When VPN is enabled, egress is bound to `wg0` via SO_BINDTODEVICE
///   (reqwest `.interface()`). If the interface is missing, fetches fail
///   closed — kill-switch semantics.
#[derive(Clone)]
pub struct ImageProxy {
    tokens: Arc<Mutex<HashMap<ProxyToken, Entry>>>,
    client: Arc<RwLock<reqwest::Client>>,
    bound_iface: Arc<RwLock<Option<String>>>,
}

impl ImageProxy {
    pub fn new() -> Self {
        let client = Arc::new(RwLock::new(build_client(None).expect("reqwest builds")));
        Self {
            tokens: Arc::new(Mutex::new(HashMap::new())),
            client,
            bound_iface: Arc::new(RwLock::new(None)),
        }
    }

    pub fn mint(&self, url: &str) -> ProxyToken {
        let mut bytes = [0u8; 16];
        OsRng.fill_bytes(&mut bytes);
        let token = hex::encode(bytes);
        let mut map = self.tokens.lock().unwrap();
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
        let mut map = self.tokens.lock().unwrap();
        map.retain(|_, e| e.created_at.elapsed() < TOKEN_TTL);
        map.get(token).map(|e| e.url.clone())
    }

    /// Rebuild the HTTP client to bind egress to the named interface, or
    /// restore default-route egress when `None`.
    pub fn bind_interface(&self, iface: Option<&str>) -> Result<()> {
        let new_client = build_client(iface)?;
        *self.client.write().unwrap() = new_client;
        *self.bound_iface.write().unwrap() = iface.map(str::to_owned);
        tracing::info!(iface = ?iface, "image proxy egress rebind");
        Ok(())
    }

    #[must_use]
    pub fn bound_interface(&self) -> Option<String> {
        self.bound_iface.read().unwrap().clone()
    }

    pub async fn fetch(&self, url: &str) -> Result<FetchedImage> {
        let mut headers = HeaderMap::new();
        headers.insert("accept", HeaderValue::from_static("image/*,*/*;q=0.5"));
        headers.insert(
            USER_AGENT,
            HeaderValue::from_static("Postern/0.1 (+privacy-proxy)"),
        );

        let client = self.client.read().unwrap().clone();
        let resp = client
            .get(url)
            .headers(headers)
            .send()
            .await
            .map_err(|e| Error::Other(anyhow::anyhow!("proxy fetch {url}: {e}")))?;

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
            .map(str::to_owned)
            .unwrap_or_else(|| "application/octet-stream".into());

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

fn build_client(iface: Option<&str>) -> Result<reqwest::Client> {
    let mut b = reqwest::Client::builder()
        .user_agent("Postern/0.1 (+privacy-proxy)")
        .timeout(FETCH_TIMEOUT)
        .redirect(reqwest::redirect::Policy::limited(3));

    // SO_BINDTODEVICE on Linux via reqwest `interface()`. When the named
    // interface doesn't exist, connect() fails — exactly the kill-switch
    // behavior we want (fail closed rather than leak traffic to eth0).
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
}
