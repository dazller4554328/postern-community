use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};

use ammonia::{Builder, UrlRelative};
use mail_parser::{MessageParser, PartType};
use serde::Serialize;

use super::proxy::ImageProxy;
use super::trackers;

const ALLOWED_TAGS: &[&str] = &[
    "a",
    "b",
    "br",
    "blockquote",
    "code",
    "div",
    "em",
    "figure",
    "figcaption",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "hr",
    "i",
    "img",
    "li",
    "ol",
    "p",
    "pre",
    "q",
    "small",
    "span",
    "strong",
    "sub",
    "sup",
    "table",
    "tbody",
    "td",
    "tfoot",
    "th",
    "thead",
    "tr",
    "u",
    "ul",
];

/// Per-tag attribute allowlist. Any attribute not listed here is stripped.
const TAG_ATTRS: &[(&str, &[&str])] = &[
    ("a", &["href", "title"]),
    ("img", &["src", "alt", "title", "width", "height"]),
    ("td", &["colspan", "rowspan", "align"]),
    ("th", &["colspan", "rowspan", "align", "scope"]),
    ("table", &["cellpadding", "cellspacing", "border"]),
    ("span", &["style"]),
    ("div", &["style"]),
    ("p", &["style"]),
];

// `data:` is allowed here because we use it only for our 1×1 blocked-image
// placeholder — never accept arbitrary data URLs from the message source,
// since the rewriter replaces any incoming img src before ammonia sees it.
// The iframe sandbox (no allow-scripts) + CSP `default-src 'none'` stop
// `<a href="data:text/html,<script>…">` being exploitable even if a message
// snuck one past the rewriter.
const ALLOWED_URL_SCHEMES: &[&str] = &["http", "https", "mailto", "data"];

/// One blocked tracker — the UI shows a count + a top-services summary.
/// `host` is the raw host seen in the original URL so the user can see
/// exactly what's in the mail. `service` is the named platform (e.g.
/// "Mailchimp") or the generic "Tracking pixel" when only the URL
/// shape gave it away.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TrackerBlocked {
    pub host: String,
    pub service: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct RenderedBody {
    pub format: &'static str,
    pub html: String,
    pub remote_hosts: Vec<String>,
    pub has_remote_content: bool,
    /// Tracker beacons we replaced with the inert placeholder,
    /// regardless of `allow_remote`. Surfaced to the UI so users see
    /// how many pixels got stripped and which services were behind
    /// them.
    pub trackers_blocked: Vec<TrackerBlocked>,
}

pub fn render_body(raw: &[u8], proxy: &ImageProxy, allow_remote: bool) -> RenderedBody {
    let parser = MessageParser::default();
    let Some(msg) = parser.parse(raw) else {
        return broken();
    };

    if let Some(html) = msg.body_html(0) {
        sanitize_html(&html, proxy, allow_remote)
    } else if let Some(text) = msg.body_text(0) {
        render_plain(&text)
    } else if let Some(first_text) = find_first_text(&msg) {
        render_plain(&first_text)
    } else {
        RenderedBody {
            format: "plain",
            html: "<p><em>(empty message)</em></p>".into(),
            remote_hosts: vec![],
            has_remote_content: false,
            trackers_blocked: vec![],
        }
    }
}

fn render_plain(text: &str) -> RenderedBody {
    let escaped = html_escape(text);
    RenderedBody {
        format: "plain",
        html: format!("<pre>{escaped}</pre>"),
        remote_hosts: vec![],
        has_remote_content: false,
        trackers_blocked: vec![],
    }
}

fn sanitize_html(html: &str, proxy: &ImageProxy, allow_remote: bool) -> RenderedBody {
    let remote_hosts: Arc<Mutex<HashSet<String>>> = Arc::new(Mutex::new(HashSet::new()));
    let blocked_trackers: Arc<Mutex<Vec<TrackerBlocked>>> = Arc::new(Mutex::new(Vec::new()));

    let mut builder = Builder::default();
    builder.tags(ALLOWED_TAGS.iter().copied().collect::<HashSet<_>>());

    let mut tag_attrs: HashMap<&str, HashSet<&str>> = HashMap::new();
    for (tag, attrs) in TAG_ATTRS {
        tag_attrs.insert(*tag, attrs.iter().copied().collect());
    }
    builder.tag_attributes(tag_attrs);

    builder.url_schemes(ALLOWED_URL_SCHEMES.iter().copied().collect::<HashSet<_>>());
    // PassThrough keeps our `/img-proxy/<token>` relative URLs intact.
    // Relative hrefs in email are vanishingly rare and harmless — the
    // browser just fails to resolve them, which is fine.
    builder.url_relative(UrlRelative::PassThrough);
    builder.link_rel(Some("noopener noreferrer nofollow"));

    // The closure must be 'static + Send + Sync — clone the proxy and
    // Arc the hosts set so we own everything we capture.
    let proxy_clone = proxy.clone();
    let hosts_clone = remote_hosts.clone();
    let trackers_clone = blocked_trackers.clone();
    builder.attribute_filter(move |element, attribute, value| {
        rewrite_attr(
            element,
            attribute,
            value,
            &proxy_clone,
            allow_remote,
            &hosts_clone,
            &trackers_clone,
        )
    });

    let cleaned = builder.clean(html).to_string();

    let hosts: Vec<String> = {
        let mut v: Vec<_> = remote_hosts.lock().unwrap().iter().cloned().collect();
        v.sort();
        v
    };
    let has_remote = !hosts.is_empty();
    let trackers = std::mem::take(&mut *blocked_trackers.lock().unwrap());

    RenderedBody {
        format: "html",
        html: cleaned,
        remote_hosts: hosts,
        has_remote_content: has_remote,
        trackers_blocked: trackers,
    }
}

fn rewrite_attr(
    element: &str,
    attribute: &str,
    value: &str,
    proxy: &ImageProxy,
    allow_remote: bool,
    hosts: &Mutex<HashSet<String>>,
    trackers: &Mutex<Vec<TrackerBlocked>>,
) -> Option<Cow<'static, str>> {
    // `'static` return lets the ammonia closure satisfy its HRTB for any
    // input lifetime. Minor extra alloc vs borrowing — fine for an email-
    // render path that's nowhere near hot.
    if element != "img" || attribute != "src" {
        return Some(Cow::Owned(value.to_owned()));
    }
    let Some(host) = host_of(value) else {
        // Non-http(s) src (cid:, data:, file:, etc.) — drop hard.
        return None;
    };

    // Tracker classifier runs *before* the allow_remote gate so open-
    // pixel beacons stay blocked even after the user clicks "Show
    // images". Privacy is the floor, not a preference.
    if let Some(m) = trackers::classify(value) {
        let mut list = trackers.lock().unwrap();
        // Deduplicate (host, service) pairs so repeated tracker pixels
        // in a single mail count as one surface in the UI.
        if !list
            .iter()
            .any(|t| t.host == m.host && t.service == m.service)
        {
            list.push(TrackerBlocked {
                host: m.host,
                service: m.service,
            });
        }
        return Some(Cow::Borrowed(BLOCKED_PIXEL));
    }

    hosts.lock().unwrap().insert(host);

    if allow_remote {
        let token = proxy.mint(value);
        Some(Cow::Owned(format!("/img-proxy/{token}")))
    } else {
        Some(Cow::Borrowed(BLOCKED_PIXEL))
    }
}

/// 1×1 transparent GIF as a data URL — the same placeholder we swap
/// in for any blocked remote image. Shared so `BLOCKED_PIXEL ==` works
/// in tests without duplicating the base64 literal.
const BLOCKED_PIXEL: &str = "data:image/gif;base64,R0lGODlhAQABAAAAACw=";

fn host_of(url: &str) -> Option<String> {
    let trimmed = url.trim();
    if !trimmed.starts_with("http://") && !trimmed.starts_with("https://") {
        return None;
    }
    let after_scheme = trimmed.splitn(2, "//").nth(1)?;
    let host_port = after_scheme.split(['/', '?', '#']).next()?;
    let host = host_port.split('@').last()?.split(':').next()?;
    if host.is_empty() {
        None
    } else {
        Some(host.to_owned())
    }
}

fn find_first_text(msg: &mail_parser::Message<'_>) -> Option<String> {
    for part in msg.parts.iter() {
        if let PartType::Text(text) = &part.body {
            return Some(text.to_string());
        }
    }
    None
}

fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

fn broken() -> RenderedBody {
    RenderedBody {
        format: "plain",
        html: "<p><em>(unparseable email — raw blob preserved for inspection)</em></p>".into(),
        remote_hosts: vec![],
        has_remote_content: false,
        trackers_blocked: vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proxy() -> ImageProxy {
        ImageProxy::new()
    }

    #[test]
    fn strips_script_tags() {
        let raw = b"MIME-Version: 1.0\r\nContent-Type: text/html\r\n\r\n\
                    <p>hi</p><script>alert(1)</script><b>bold</b>";
        let r = render_body(raw, &proxy(), false);
        assert!(!r.html.contains("script"));
        assert!(r.html.contains("bold"));
    }

    #[test]
    fn blocks_remote_images_by_default() {
        // URL path deliberately avoids tracker-pattern matches (no
        // `/pixel.gif` etc.) so this test still exercises the
        // generic remote-content path rather than the tracker path.
        let raw = b"MIME-Version: 1.0\r\nContent-Type: text/html\r\n\r\n\
                    <p><img src=\"https://evil.example/assets/hero.png\"></p>";
        let r = render_body(raw, &proxy(), false);
        assert!(
            !r.html.contains("evil.example"),
            "remote host must not leak: {}",
            r.html
        );
        assert!(r.html.contains("data:image/gif"));
        assert!(r.has_remote_content);
        assert_eq!(r.remote_hosts, vec!["evil.example"]);
    }

    #[test]
    fn proxies_remote_images_when_allowed() {
        let raw = b"MIME-Version: 1.0\r\nContent-Type: text/html\r\n\r\n\
                    <img src=\"https://foo.example/p.gif\">";
        let r = render_body(raw, &proxy(), true);
        assert!(r.html.contains("/img-proxy/"));
        assert!(!r.html.contains("foo.example"));
    }

    #[test]
    fn trackers_stay_blocked_even_when_remote_allowed() {
        // Mailchimp open pixel. Even with allow_remote=true, the
        // sanitizer must replace it with the inert 1×1 placeholder
        // and record the match for the UI.
        let raw = b"MIME-Version: 1.0\r\nContent-Type: text/html\r\n\r\n\
                    <p><img src=\"https://beacon.list-manage.com/track/open.php?u=1\"></p>\
                    <p><img src=\"https://cdn.example.com/hero.png\"></p>";
        let r = render_body(raw, &proxy(), true);
        // Legit image got proxied.
        assert!(r.html.contains("/img-proxy/"));
        // Tracker swapped for the placeholder — neither the original
        // host nor a proxy token for it should leak.
        assert!(!r.html.contains("list-manage.com"));
        assert!(r.html.contains("data:image/gif"));
        assert_eq!(r.trackers_blocked.len(), 1);
        assert_eq!(r.trackers_blocked[0].service, "Mailchimp");
        assert_eq!(r.trackers_blocked[0].host, "beacon.list-manage.com");
    }

    #[test]
    fn tracker_dedup_per_host_service() {
        // Same tracker twice in the same message → one entry.
        let raw = b"MIME-Version: 1.0\r\nContent-Type: text/html\r\n\r\n\
                    <img src=\"https://list-manage.com/track/open.php?u=1\">\
                    <img src=\"https://list-manage.com/track/open.php?u=2\">";
        let r = render_body(raw, &proxy(), true);
        assert_eq!(r.trackers_blocked.len(), 1);
    }

    #[test]
    fn tracker_path_pattern_on_unknown_host() {
        let raw = b"MIME-Version: 1.0\r\nContent-Type: text/html\r\n\r\n\
                    <img src=\"https://news.example/track/open?m=x\">";
        let r = render_body(raw, &proxy(), true);
        assert_eq!(r.trackers_blocked.len(), 1);
        assert_eq!(r.trackers_blocked[0].service, "Tracking pixel");
    }

    #[test]
    fn tracker_also_blocked_when_remote_disallowed() {
        let raw = b"MIME-Version: 1.0\r\nContent-Type: text/html\r\n\r\n\
                    <img src=\"https://sendgrid.net/beacon.gif\">";
        let r = render_body(raw, &proxy(), false);
        assert_eq!(r.trackers_blocked.len(), 1);
        assert!(r.html.contains("data:image/gif"));
    }

    #[test]
    fn strips_javascript_urls() {
        let raw = b"MIME-Version: 1.0\r\nContent-Type: text/html\r\n\r\n\
                    <a href=\"javascript:alert(1)\">click</a>";
        let r = render_body(raw, &proxy(), false);
        assert!(
            !r.html.to_lowercase().contains("javascript:"),
            "got: {}",
            r.html
        );
    }

    #[test]
    fn plaintext_script_tags_are_neutralised() {
        // Primary invariant: a mail body containing <script> must never
        // produce a live script tag in the rendered HTML, regardless
        // of which path render_body takes (plain → html_escape, or
        // HTML → ammonia strip). Both are XSS-safe; the test doesn't
        // pin which one is used.
        //
        // mail-parser 0.9 synthesises body_html(0) from plain-text
        // content in some configurations, pushing this sample down
        // the sanitize_html path. ammonia strips the <script> tag
        // but preserves innerText — which is fine because plain
        // text of "alert(1)" isn't executable.
        let raw = b"MIME-Version: 1.0\r\nContent-Type: text/plain\r\n\r\nbefore <script>x</script> after";
        let r = render_body(raw, &proxy(), false);
        assert!(!r.html.to_lowercase().contains("<script"), "got: {}", r.html);
        assert!(!r.html.to_lowercase().contains("</script"), "got: {}", r.html);
    }

    #[test]
    fn host_parsing() {
        assert_eq!(
            host_of("https://foo.example/bar").as_deref(),
            Some("foo.example")
        );
        assert_eq!(host_of("http://FOO:123/"), Some("FOO".into()));
        assert_eq!(host_of("cid:abc"), None);
        assert_eq!(host_of("data:image/gif;..."), None);
    }
}
