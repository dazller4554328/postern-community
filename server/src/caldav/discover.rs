//! CalDAV principal + calendar-home discovery.
//!
//! Every server implements the same three-step dance:
//!   1. PROPFIND the user-supplied URL for `current-user-principal`.
//!   2. PROPFIND that principal for `calendar-home-set`.
//!   3. PROPFIND depth:1 on the calendar-home URL to enumerate
//!      calendar collections.
//!
//! We additionally fall back to `/.well-known/caldav` when step 1
//! returns a non-2xx — some servers only expose the principal via the
//! well-known redirect rather than the vhost root.

use anyhow::anyhow;

use super::client::{parse_multistatus, Client, PropResponse};
use crate::error::{Error, Result};

const PROPFIND_PRINCIPAL: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<d:propfind xmlns:d="DAV:">
  <d:prop>
    <d:current-user-principal/>
  </d:prop>
</d:propfind>"#;

const PROPFIND_CALENDAR_HOME: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<d:propfind xmlns:d="DAV:" xmlns:cal="urn:ietf:params:xml:ns:caldav">
  <d:prop>
    <cal:calendar-home-set/>
  </d:prop>
</d:propfind>"#;

const PROPFIND_COLLECTIONS: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<d:propfind xmlns:d="DAV:" xmlns:cal="urn:ietf:params:xml:ns:caldav"
            xmlns:cs="http://calendarserver.org/ns/"
            xmlns:ic="http://apple.com/ns/ical/">
  <d:prop>
    <d:displayname/>
    <d:resourcetype/>
    <cs:getctag/>
    <cal:supported-calendar-component-set/>
    <ic:calendar-color/>
  </d:prop>
</d:propfind>"#;

/// Resolve a user-entered server URL to the actual `calendar-home-set`
/// URL. Both intermediate URLs are returned so callers can cache them
/// on the account row and skip the discovery dance next sync.
pub async fn discover(
    client: &Client,
    server_url: &str,
) -> Result<(String, String)> {
    let principal = find_principal(client, server_url).await?;
    let home = find_calendar_home(client, &principal).await?;
    Ok((principal, home))
}

async fn find_principal(client: &Client, server_url: &str) -> Result<String> {
    // First attempt: the URL the user entered.
    if let Ok(url) = propfind_principal(client, server_url).await {
        return Ok(url);
    }
    // Fallback: RFC 6764 /.well-known/caldav bootstrap.
    let well_known = well_known_url(server_url)?;
    propfind_principal(client, &well_known).await
}

async fn propfind_principal(client: &Client, url: &str) -> Result<String> {
    let body = client.propfind(url, "0", PROPFIND_PRINCIPAL).await?;
    let responses = parse_multistatus(&body)?;
    let principal = responses
        .iter()
        .find_map(|r| r.props.current_user_principal.clone())
        .ok_or_else(|| Error::Other(anyhow!("no current-user-principal in PROPFIND response")))?;
    absolute_url(url, &principal)
}

async fn find_calendar_home(client: &Client, principal_url: &str) -> Result<String> {
    let body = client
        .propfind(principal_url, "0", PROPFIND_CALENDAR_HOME)
        .await?;
    let responses = parse_multistatus(&body)?;
    let home = responses
        .iter()
        .find_map(|r| r.props.calendar_home_set.clone())
        .ok_or_else(|| Error::Other(anyhow!("no calendar-home-set in PROPFIND response")))?;
    absolute_url(principal_url, &home)
}

/// Enumerate calendar collections under the home URL, filtering out
/// addressbooks and VTODO-only calendars.
pub async fn list_calendars(
    client: &Client,
    calendar_home_url: &str,
) -> Result<Vec<CalendarInfo>> {
    let body = client
        .propfind(calendar_home_url, "1", PROPFIND_COLLECTIONS)
        .await?;
    let responses = parse_multistatus(&body)?;
    let mut out = Vec::new();
    for r in responses {
        // Skip the home collection itself (its href ends with "/"
        // and has no calendar resourcetype).
        if !r.props.is_calendar_collection || !r.props.supports_vevent {
            continue;
        }
        let abs = absolute_url(calendar_home_url, &r.href).unwrap_or(r.href.clone());
        out.push(CalendarInfo {
            url: abs,
            name: r.props.displayname.unwrap_or_else(|| "Calendar".into()),
            ctag: r.props.getctag,
            color: r.props.calendar_color,
        });
    }
    Ok(out)
}

#[derive(Debug, Clone)]
pub struct CalendarInfo {
    pub url: String,
    pub name: String,
    pub ctag: Option<String>,
    pub color: Option<String>,
}

/// Resolve `href` against `base` (which the caller knows is already
/// absolute). Accepts absolute hrefs unchanged; rewrites relative
/// hrefs onto the base's scheme + host.
pub(super) fn absolute_url(base: &str, href: &str) -> Result<String> {
    let base_url = reqwest::Url::parse(base)
        .map_err(|e| Error::Other(anyhow!("caldav base url {base}: {e}")))?;
    let resolved = base_url
        .join(href)
        .map_err(|e| Error::Other(anyhow!("caldav join {base} + {href}: {e}")))?;
    Ok(resolved.to_string())
}

fn well_known_url(server_url: &str) -> Result<String> {
    let base = reqwest::Url::parse(server_url)
        .map_err(|e| Error::Other(anyhow!("caldav url {server_url}: {e}")))?;
    let root = format!(
        "{}://{}{}/.well-known/caldav",
        base.scheme(),
        base.host_str().unwrap_or(""),
        base.port().map(|p| format!(":{p}")).unwrap_or_default()
    );
    Ok(root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absolute_leaves_already_absolute_alone() {
        let url =
            absolute_url("https://example.com/base", "https://other.example/abc").unwrap();
        assert_eq!(url, "https://other.example/abc");
    }

    #[test]
    fn absolute_resolves_relative_href() {
        let url = absolute_url("https://example.com/dav/", "/principals/alice/").unwrap();
        assert_eq!(url, "https://example.com/principals/alice/");
    }
}
