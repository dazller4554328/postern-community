//! Sender avatars via Gravatar → domain favicon → 404.
//!
//! Both upstream fetches go through `ImageProxy`, so they bind to
//! `wg0` (when the VPN is up) and honour the same kill-switch the
//! remote-image path uses. The endpoint returns either the upstream
//! image bytes or a 404 — the client falls back to initials on 404.
//!
//! Caching: we don't have a persistent cache yet, so the only cache
//! is the browser's. We emit `Cache-Control: public, max-age=86400`
//! on hits and `max-age=3600` on misses so repeat views of the same
//! thread don't re-fetch on every render.
//!
//! Privacy note: a successful avatar fetch leaks "this sender is
//! being looked at" to Gravatar (or the sender's domain). That
//! leak was already present for every inline image; the VPN routing
//! matches their threat model.
use axum::{
    body::Body,
    extract::{Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
};
use md5::{Digest, Md5};
use serde::Deserialize;

use super::AppState;

#[derive(Debug, Deserialize)]
pub struct AvatarQuery {
    email: String,
    #[serde(default = "default_size")]
    size: u32,
}

const fn default_size() -> u32 {
    64
}

pub async fn get_avatar(State(s): State<AppState>, Query(q): Query<AvatarQuery>) -> Response {
    let email = q.email.trim().to_ascii_lowercase();
    if email.is_empty() || !email.contains('@') {
        return not_found();
    }
    let size = q.size.clamp(16, 256);

    // 1. Gravatar. `d=404` tells Gravatar to return 404 when no
    //    avatar is registered, instead of serving a default mystery
    //    silhouette. We treat 404 as "try the next source".
    let md5_hex = {
        let mut h = Md5::new();
        h.update(email.as_bytes());
        hex::encode(h.finalize())
    };
    let gravatar_url = format!("https://www.gravatar.com/avatar/{md5_hex}?d=404&s={size}");
    if let Ok(fetched) = s.proxy.fetch(&gravatar_url).await {
        return image_response(fetched.content_type, fetched.bytes, 86_400);
    }

    // 2-4. Company senders (anthropic.com, github.com, …) when the
    //   individual doesn't have Gravatar. We try the domain's own
    //   icon assets BEFORE DuckDuckGo's icon service — DDG's
    //   `/ip3/<domain>.ico` returns 200 with a generic placeholder
    //   for less-frequently-crawled domains, which we'd otherwise
    //   serve as if it were the brand mark. Direct fetches give us
    //   the real logo for any company that ships an `apple-touch-icon`
    //   (most do — it's part of standard SEO/social-share hygiene).
    //
    //   Order: apple-touch-icon (best quality, 180×180 PNG with
    //   alpha) → favicon.ico (universal but often 16×16) → DDG.
    //   Each call is cheap on a cache hit; the proxy handles the
    //   wg0 binding so we still go through the VPN if it's up.
    if let Some(domain) = email.split('@').nth(1) {
        let candidates = [
            format!("https://{domain}/apple-touch-icon.png"),
            format!("https://{domain}/favicon.ico"),
            format!("https://icons.duckduckgo.com/ip3/{domain}.ico"),
        ];
        for url in &candidates {
            if let Ok(fetched) = s.proxy.fetch(url).await {
                // Skip suspiciously tiny payloads — most "icon not
                // found" pages from misconfigured servers come back
                // as a few bytes of placeholder. A real favicon is
                // ≥ ~600 bytes; touch icons are ≥ ~3 KB.
                if fetched.bytes.len() >= 256 {
                    return image_response(fetched.content_type, fetched.bytes, 86_400);
                }
            }
        }
    }

    // Nothing matched. Let the client fall back to initials.
    not_found()
}

fn image_response(content_type: String, bytes: Vec<u8>, cache_secs: u32) -> Response {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(
            header::CACHE_CONTROL,
            format!("public, max-age={cache_secs}"),
        )
        .body(Body::from(bytes))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

fn not_found() -> Response {
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .header(header::CACHE_CONTROL, "public, max-age=3600")
        .body(Body::empty())
        .unwrap_or_else(|_| StatusCode::NOT_FOUND.into_response())
}
