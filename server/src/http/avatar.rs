//! Sender avatars via contact photo → Libravatar → domain icon → 404.
//!
//! Lookup chain, queried in order; first hit wins:
//!
//! 1. **Contact photo** — if the user has stored a photo for this
//!    address in their address book, serve that. Zero remote
//!    leakage; cheapest case. Mirrors Evolution's strongest source.
//! 2. **Libravatar** — `https://seccdn.libravatar.org/avatar/<md5>?d=404`.
//!    Federated, privacy-respecting alternative to Gravatar (which
//!    Evolution chose for the same reason). Libravatar falls
//!    through to Gravatar internally when no Libravatar entry
//!    exists, so it's a strict superset.
//! 3. **Domain assets** — `apple-touch-icon.png`, then `favicon.svg`,
//!    then `favicon.ico` on the sender's own host. Catches the
//!    company-sender case (anthropic.com, github.com) without hitting
//!    a third-party icon service that returns generic placeholders.
//!    `favicon.svg` is included because it's now the default favicon
//!    format on modern sites that ship no `.ico` or touch icon.
//! 4. **`DuckDuckGo` icons** — final fallback for less common domains.
//!
//! Remote fetches go through `ImageProxy`, so they bind to `wg0`
//! when the VPN is up and honour the same kill-switch the
//! remote-image path uses. The endpoint returns either the chosen
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
    Json,
};
use md5::{Digest, Md5};
use serde::Deserialize;

use super::AppState;
use crate::error::Result;

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

    // 1. Locally-stored contact photo. Wins when present — never
    //    leaks the email address to a third party.
    if let Ok(Some((bytes, mime))) = s.db.get_contact_photo_by_address(&email) {
        return image_response(mime, bytes, 86_400);
    }

    // Everything past this point contacts a third party (Libravatar/
    // Gravatar, the sender's domain, DuckDuckGo). That's a privacy
    // egress — each fetch reveals "this mailbox is viewing this sender"
    // — so it's opt-in. When disabled, stop after the local-only
    // contact-photo check above and let the client render initials.
    if !s.db.remote_avatars_enabled().unwrap_or(false) {
        return not_found();
    }

    // 2. Libravatar. `d=404` returns 404 when no avatar is
    //    registered, instead of a generic silhouette. Libravatar
    //    falls through to Gravatar internally, so we cover both
    //    services with one request.
    let md5_hex = {
        let mut h = Md5::new();
        h.update(email.as_bytes());
        hex::encode(h.finalize())
    };
    let libravatar_url = format!("https://seccdn.libravatar.org/avatar/{md5_hex}?d=404&s={size}");
    if let Ok(fetched) = s.proxy.fetch(&libravatar_url).await {
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
    //   alpha) → favicon.svg (crisp vector, now the default favicon
    //   format on modern sites) → favicon.ico (universal but often
    //   16×16) → DDG. Each call is cheap on a cache hit; the proxy
    //   handles the wg0 binding so we still go through the VPN if up.
    if let Some(domain) = email.split('@').nth(1) {
        // Big senders often use marketing subdomains for outgoing mail
        // (`ecomm.lenovo.com`, `messaging.squareup.com`) that don't
        // host the brand icon themselves. Walk from the most-specific
        // domain to the registrable parent and probe each level —
        // `messaging.squareup.com` → `squareup.com` rescues the icon
        // for everyone who uses a CDN/ESP subdomain. The walk stops
        // before stripping past the eTLD+1 to avoid probing bare TLDs.
        for candidate_domain in domain_walk(domain) {
            let candidates = [
                format!("https://{candidate_domain}/apple-touch-icon.png"),
                format!("https://{candidate_domain}/favicon.svg"),
                format!("https://{candidate_domain}/favicon.ico"),
                format!("https://icons.duckduckgo.com/ip3/{candidate_domain}.ico"),
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
    }

    // Nothing matched. Let the client fall back to initials.
    not_found()
}

/// GET /api/settings/remote-avatars — read the opt-in flag.
pub async fn get_remote_avatars_setting(
    State(s): State<AppState>,
) -> Result<Json<serde_json::Value>> {
    Ok(Json(serde_json::json!({
        "enabled": s.db.remote_avatars_enabled()?
    })))
}

#[derive(Debug, Deserialize)]
pub struct RemoteAvatarsBody {
    enabled: bool,
}

/// POST /api/settings/remote-avatars — flip the opt-in flag.
pub async fn set_remote_avatars_setting(
    State(s): State<AppState>,
    Json(body): Json<RemoteAvatarsBody>,
) -> Result<Json<serde_json::Value>> {
    s.db.set_remote_avatars_enabled(body.enabled)?;
    let _ = s.db.log_event(
        "remote_avatars_setting_changed",
        Some(if body.enabled { "enabled" } else { "disabled" }),
        None,
    );
    Ok(Json(serde_json::json!({ "enabled": body.enabled })))
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

/// Two-letter labels that commonly act as second-level "TLDs" under a
/// country code — `co.uk`, `com.au`, `gov.in`, etc. We treat any
/// domain whose last two labels match one of these as a 3-label
/// registrable parent, so we don't accidentally strip past it.
///
/// This is a pragmatic subset of the Mozilla Public Suffix List — the
/// real PSL has thousands of entries, but those add up to roughly
/// 200 KB of build-time data for a feature whose miss-rate just
/// causes one redundant 404. The list below covers the common cases
/// users actually encounter in email.
const KNOWN_CCTLD_SECONDARIES: &[&str] =
    &["co", "com", "net", "org", "gov", "edu", "ac", "sch", "mil"];

/// Walk a domain from the most-specific form down to the registrable
/// parent, emitting each label-level along the way. Stops before
/// stripping past the eTLD+1 so we don't probe bare TLDs.
///
/// Examples:
///   "ecomm.lenovo.com"        → ["ecomm.lenovo.com", "lenovo.com"]
///   "messaging.squareup.com"  → ["messaging.squareup.com", "squareup.com"]
///   "foo.bar.lenovo.com"      → ["foo.bar.lenovo.com", "bar.lenovo.com", "lenovo.com"]
///   "lenovo.com"              → ["lenovo.com"]
///   "co.uk"                   → ["co.uk"]
///   "foo.example.co.uk"       → ["foo.example.co.uk", "example.co.uk"]
fn domain_walk(domain: &str) -> Vec<String> {
    let trimmed = domain.trim().trim_end_matches('.');
    if trimmed.is_empty() {
        return Vec::new();
    }
    let labels: Vec<&str> = trimmed.split('.').filter(|s| !s.is_empty()).collect();
    if labels.len() < 2 {
        return vec![trimmed.to_string()];
    }

    // The "minimum keep" is the eTLD+1. For `co.uk`-style ccTLD
    // domains, that's 3 labels; otherwise 2.
    let last_two = format!("{}.{}", labels[labels.len() - 2], labels[labels.len() - 1]);
    let min_labels = if looks_like_cctld_secondary(&last_two) {
        3
    } else {
        2
    };

    let mut out = Vec::new();
    for start in 0..=labels.len().saturating_sub(min_labels) {
        out.push(labels[start..].join("."));
    }
    out
}

fn looks_like_cctld_secondary(last_two: &str) -> bool {
    let parts: Vec<&str> = last_two.split('.').collect();
    if parts.len() != 2 {
        return false;
    }
    let (second_level, tld) = (parts[0], parts[1]);
    // ccTLDs are exactly two ASCII letters (us, uk, au, in, jp, …).
    if tld.len() != 2 || !tld.chars().all(|c| c.is_ascii_lowercase()) {
        return false;
    }
    KNOWN_CCTLD_SECONDARIES.contains(&second_level)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walks_subdomain_down_to_parent() {
        assert_eq!(
            domain_walk("ecomm.lenovo.com"),
            vec!["ecomm.lenovo.com", "lenovo.com"]
        );
        assert_eq!(
            domain_walk("messaging.squareup.com"),
            vec!["messaging.squareup.com", "squareup.com"]
        );
    }

    #[test]
    fn walks_through_multiple_subdomain_levels() {
        assert_eq!(
            domain_walk("foo.bar.lenovo.com"),
            vec!["foo.bar.lenovo.com", "bar.lenovo.com", "lenovo.com"]
        );
    }

    #[test]
    fn stops_at_etld_plus_one() {
        // `lenovo.com` is already the registrable parent — don't
        // strip to `com`.
        assert_eq!(domain_walk("lenovo.com"), vec!["lenovo.com"]);
    }

    #[test]
    fn handles_known_cctld_secondaries() {
        // `example.co.uk` is the parent — don't strip to `co.uk`.
        assert_eq!(domain_walk("example.co.uk"), vec!["example.co.uk"]);
        assert_eq!(
            domain_walk("foo.example.co.uk"),
            vec!["foo.example.co.uk", "example.co.uk"]
        );
    }

    #[test]
    fn handles_edge_cases() {
        // Single-label garbage — pass through unchanged.
        assert_eq!(domain_walk("localhost"), vec!["localhost"]);
        // Trailing dot trimmed.
        assert_eq!(domain_walk("lenovo.com."), vec!["lenovo.com"]);
        // Empty stays empty.
        assert!(domain_walk("").is_empty());
    }
}
