//! Tiny cookie helpers. The standard library doesn't ship one and we
//! only need parse-single-pair + set-single-pair, so pulling in a
//! full cookie crate would be overkill.

use axum::http::HeaderMap;

/// Fish a single cookie value out of the request's `Cookie` header.
/// Returns None if the header is absent or the name isn't present.
pub fn get(headers: &HeaderMap, name: &str) -> Option<String> {
    let raw = headers.get(axum::http::header::COOKIE)?.to_str().ok()?;
    for pair in raw.split(';') {
        let mut parts = pair.trim().splitn(2, '=');
        let k = parts.next()?.trim();
        let v = parts.next()?.trim();
        if k == name {
            return Some(v.to_string());
        }
    }
    None
}

/// Build a Set-Cookie value. Sensible defaults — HttpOnly, Secure,
/// SameSite=Strict, Path=/. Caller provides the TTL and value.
pub fn build(name: &str, value: &str, max_age_secs: i64) -> String {
    format!("{name}={value}; Max-Age={max_age_secs}; Path=/; HttpOnly; Secure; SameSite=Strict")
}

/// Build a Set-Cookie value with no `Max-Age` / `Expires` attribute —
/// the browser keeps it for the lifetime of the tab/window only and
/// deletes it on close. Used for the default unlock flow when the
/// user has *not* ticked "Remember this device", so closing the
/// browser is a clean logout.
pub fn build_session(name: &str, value: &str) -> String {
    format!("{name}={value}; Path=/; HttpOnly; Secure; SameSite=Strict")
}

/// Build a Set-Cookie value that immediately invalidates the cookie.
pub fn expire(name: &str) -> String {
    format!("{name}=; Max-Age=0; Path=/; HttpOnly; Secure; SameSite=Strict")
}
