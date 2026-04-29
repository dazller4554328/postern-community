//! Low-level CalDAV HTTP: PROPFIND and REPORT with a tiny multistatus
//! parser. We use reqwest with basic auth and rustls.

use std::time::Duration;

use anyhow::{anyhow, Context};
use quick_xml::events::Event;
use quick_xml::Reader;
use reqwest::{header, Method, Response};

use crate::error::{Error, Result};

const REQ_TIMEOUT: Duration = Duration::from_secs(30);

/// Authenticated CalDAV client bound to one account's credentials.
/// Cheap to clone — the inner reqwest::Client is Arc'd.
#[derive(Clone)]
pub struct Client {
    http: reqwest::Client,
    username: String,
    password: String,
}

impl Client {
    pub fn new(username: &str, password: &str) -> Result<Self> {
        let http = reqwest::Client::builder()
            .user_agent("Postern/0.1 (+caldav)")
            .timeout(REQ_TIMEOUT)
            .redirect(reqwest::redirect::Policy::limited(5))
            .build()
            .map_err(|e| Error::Other(anyhow!("caldav reqwest build: {e}")))?;
        Ok(Self {
            http,
            username: username.to_owned(),
            password: password.to_owned(),
        })
    }

    /// PROPFIND with the supplied Depth header. `body` is the request
    /// XML; `props` is purely advisory for tracing.
    pub async fn propfind(&self, url: &str, depth: &str, body: &str) -> Result<String> {
        let method = Method::from_bytes(b"PROPFIND")
            .map_err(|e| Error::Other(anyhow!("bad method: {e}")))?;
        let resp = self
            .http
            .request(method, url)
            .basic_auth(&self.username, Some(&self.password))
            .header("Depth", depth)
            .header(header::CONTENT_TYPE, "application/xml; charset=utf-8")
            .body(body.to_owned())
            .send()
            .await
            .map_err(|e| Error::Other(anyhow!("caldav PROPFIND {url}: {e}")))?;
        text_of(resp, "PROPFIND", url).await
    }

    /// REPORT — calendar-query or calendar-multiget.
    pub async fn report(&self, url: &str, depth: &str, body: &str) -> Result<String> {
        let method =
            Method::from_bytes(b"REPORT").map_err(|e| Error::Other(anyhow!("bad method: {e}")))?;
        let resp = self
            .http
            .request(method, url)
            .basic_auth(&self.username, Some(&self.password))
            .header("Depth", depth)
            .header(header::CONTENT_TYPE, "application/xml; charset=utf-8")
            .body(body.to_owned())
            .send()
            .await
            .map_err(|e| Error::Other(anyhow!("caldav REPORT {url}: {e}")))?;
        text_of(resp, "REPORT", url).await
    }
}

async fn text_of(resp: Response, op: &str, url: &str) -> Result<String> {
    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|e| Error::Other(anyhow!("caldav {op} {url} body: {e}")))?;
    // 207 Multi-Status is the normal success code for PROPFIND/REPORT.
    // Some servers return 200 for single-resource PROPFINDs.
    if !(status.as_u16() == 207 || status.is_success()) {
        return Err(Error::Other(anyhow!(
            "caldav {op} {url} status {}: {}",
            status,
            truncate_for_log(&body)
        )));
    }
    Ok(body)
}

fn truncate_for_log(s: &str) -> String {
    const MAX: usize = 400;
    if s.len() <= MAX {
        s.to_owned()
    } else {
        format!("{}…[truncated {}b]", &s[..MAX], s.len() - MAX)
    }
}

// --- Multistatus parsing ---------------------------------------------------
//
// CalDAV XML is heavily namespaced but we only care about a small
// vocabulary. We walk the tag stream and look for local names; if we
// need to disambiguate namespaces later we can check the URI.

/// One parsed `<response>` from a multistatus payload.
#[derive(Debug, Default, Clone)]
pub struct PropResponse {
    pub href: String,
    pub props: PropSet,
}

/// Loose bag of property values keyed by local name. We only pluck
/// what we need downstream — extending this is a matter of adding a
/// tag name to the parser's allowlist.
#[derive(Debug, Default, Clone)]
pub struct PropSet {
    pub displayname: Option<String>,
    pub getctag: Option<String>,
    pub getetag: Option<String>,
    pub calendar_color: Option<String>,
    pub current_user_principal: Option<String>,
    pub calendar_home_set: Option<String>,
    /// Raw <calendar-data> body. Only populated on calendar-multiget.
    pub calendar_data: Option<String>,
    /// True when we saw a resourcetype containing `<calendar/>`. Used
    /// to tell calendar collections apart from sibling collections
    /// (e.g. addressbooks on the same principal).
    pub is_calendar_collection: bool,
    /// True when we saw supported-calendar-component-set/VEVENT. We
    /// skip collections that are task-only.
    pub supports_vevent: bool,
}

pub fn parse_multistatus(xml: &str) -> Result<Vec<PropResponse>> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut responses: Vec<PropResponse> = Vec::new();
    let mut current: Option<PropResponse> = None;
    // Stack of local tag names so we can tell which element's text we
    // are about to see.
    let mut stack: Vec<String> = Vec::new();
    // Buffer used when a property value is a nested <href>.
    let mut capturing_href_into: Option<&'static str> = None;

    let mut buf = Vec::new();
    loop {
        match reader
            .read_event_into(&mut buf)
            .with_context(|| "caldav xml parse")
            .map_err(|e| Error::Other(anyhow!("{e}")))?
        {
            Event::Start(e) | Event::Empty(e) => {
                let local = local_name(e.name().as_ref());
                match local.as_str() {
                    "response" => {
                        current = Some(PropResponse::default());
                    }
                    "calendar" => {
                        if let Some(r) = current.as_mut() {
                            // Only count this when we're inside a resourcetype.
                            if stack.iter().any(|s| s == "resourcetype") {
                                r.props.is_calendar_collection = true;
                            }
                        }
                    }
                    "comp" => {
                        if let Some(r) = current.as_mut() {
                            if stack.iter().any(|s| s == "supported-calendar-component-set") {
                                for attr in e.attributes().flatten() {
                                    if attr.key.as_ref() == b"name"
                                        && attr.value.as_ref() == b"VEVENT"
                                    {
                                        r.props.supports_vevent = true;
                                    }
                                }
                            }
                        }
                    }
                    "current-user-principal" => capturing_href_into = Some("current_user_principal"),
                    "calendar-home-set" => capturing_href_into = Some("calendar_home_set"),
                    _ => {}
                }
                stack.push(local);
            }
            Event::End(e) => {
                let local = local_name(e.name().as_ref());
                if local == "response" {
                    if let Some(r) = current.take() {
                        responses.push(r);
                    }
                }
                if stack.last().map(String::as_str) == Some(local.as_str()) {
                    stack.pop();
                }
                if capturing_href_into.is_some()
                    && (local == "current-user-principal" || local == "calendar-home-set")
                {
                    capturing_href_into = None;
                }
            }
            Event::Text(t) => {
                let text = t
                    .unescape()
                    .map_err(|e| Error::Other(anyhow!("xml unescape: {e}")))?
                    .to_string();
                let Some(top) = stack.last() else { continue };
                let Some(r) = current.as_mut() else { continue };
                match top.as_str() {
                    "href" => {
                        if let Some(target) = capturing_href_into {
                            match target {
                                "current_user_principal" => {
                                    r.props.current_user_principal = Some(text.clone());
                                }
                                "calendar_home_set" => {
                                    r.props.calendar_home_set = Some(text.clone());
                                }
                                _ => {}
                            }
                        } else if r.href.is_empty() {
                            // Top-level <href> inside <response>.
                            r.href = text;
                        }
                    }
                    "displayname" => r.props.displayname = Some(text),
                    "getctag" => r.props.getctag = Some(text),
                    "getetag" => r.props.getetag = Some(text),
                    "calendar-color" => r.props.calendar_color = Some(text),
                    "calendar-data" => r.props.calendar_data = Some(text),
                    _ => {}
                }
            }
            Event::CData(c) => {
                // calendar-data sometimes arrives as CDATA to preserve
                // its iCalendar CRLF framing.
                let Some(top) = stack.last() else { continue };
                let Some(r) = current.as_mut() else { continue };
                if top == "calendar-data" {
                    r.props.calendar_data =
                        Some(String::from_utf8_lossy(c.as_ref()).into_owned());
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    Ok(responses)
}

/// Strip any `ns:` prefix — CalDAV mixes DAV:, urn:ietf:params:…,
/// http://calendarserver.org/ns/ and we only care about local names.
fn local_name(qname: &[u8]) -> String {
    let s = std::str::from_utf8(qname).unwrap_or("");
    match s.rfind(':') {
        Some(idx) => s[idx + 1..].to_ascii_lowercase(),
        None => s.to_ascii_lowercase(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_principal_discovery() {
        let xml = r#"<?xml version="1.0"?>
<d:multistatus xmlns:d="DAV:">
  <d:response>
    <d:href>/dav.php/</d:href>
    <d:propstat>
      <d:prop>
        <d:current-user-principal><d:href>/dav.php/principals/alice/</d:href></d:current-user-principal>
      </d:prop>
      <d:status>HTTP/1.1 200 OK</d:status>
    </d:propstat>
  </d:response>
</d:multistatus>"#;
        let out = parse_multistatus(xml).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(
            out[0].props.current_user_principal.as_deref(),
            Some("/dav.php/principals/alice/")
        );
    }

    #[test]
    fn parses_calendar_collection_list() {
        let xml = r#"<?xml version="1.0"?>
<d:multistatus xmlns:d="DAV:" xmlns:cal="urn:ietf:params:xml:ns:caldav"
               xmlns:cs="http://calendarserver.org/ns/">
  <d:response>
    <d:href>/dav/calendars/alice/personal/</d:href>
    <d:propstat>
      <d:prop>
        <d:displayname>Personal</d:displayname>
        <d:resourcetype><d:collection/><cal:calendar/></d:resourcetype>
        <cs:getctag>"abc123"</cs:getctag>
        <cal:supported-calendar-component-set>
          <cal:comp name="VEVENT"/>
          <cal:comp name="VTODO"/>
        </cal:supported-calendar-component-set>
      </d:prop>
    </d:propstat>
  </d:response>
  <d:response>
    <d:href>/dav/calendars/alice/tasks/</d:href>
    <d:propstat>
      <d:prop>
        <d:displayname>Tasks</d:displayname>
        <d:resourcetype><d:collection/><cal:calendar/></d:resourcetype>
        <cal:supported-calendar-component-set>
          <cal:comp name="VTODO"/>
        </cal:supported-calendar-component-set>
      </d:prop>
    </d:propstat>
  </d:response>
</d:multistatus>"#;
        let out = parse_multistatus(xml).unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].props.displayname.as_deref(), Some("Personal"));
        assert!(out[0].props.is_calendar_collection);
        assert!(out[0].props.supports_vevent);
        assert_eq!(out[0].props.getctag.as_deref(), Some("\"abc123\""));
        assert!(out[1].props.is_calendar_collection);
        assert!(!out[1].props.supports_vevent);
    }

    #[test]
    fn parses_calendar_multiget() {
        let xml = r#"<?xml version="1.0"?>
<d:multistatus xmlns:d="DAV:" xmlns:cal="urn:ietf:params:xml:ns:caldav">
  <d:response>
    <d:href>/dav/calendars/alice/personal/abc.ics</d:href>
    <d:propstat>
      <d:prop>
        <d:getetag>"etag-1"</d:getetag>
        <cal:calendar-data>BEGIN:VCALENDAR
VERSION:2.0
BEGIN:VEVENT
UID:abc
SUMMARY:Test
DTSTART:20260101T090000Z
END:VEVENT
END:VCALENDAR</cal:calendar-data>
      </d:prop>
    </d:propstat>
  </d:response>
</d:multistatus>"#;
        let out = parse_multistatus(xml).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].href, "/dav/calendars/alice/personal/abc.ics");
        assert_eq!(out[0].props.getetag.as_deref(), Some("\"etag-1\""));
        assert!(out[0]
            .props
            .calendar_data
            .as_deref()
            .unwrap()
            .contains("UID:abc"));
    }
}
