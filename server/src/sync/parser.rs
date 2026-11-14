use mail_parser::{HeaderValue, MessageParser};

use crate::storage::NewMessage;

const SNIPPET_LEN: usize = 200;
const BODY_INDEX_CAP: usize = 256 * 1024; // 256 KiB of body text is plenty for FTS

/// Render a UID list as an IMAP UID-set literal, e.g. `12,34,567`.
/// Centralised so every sync / purge / retention path formats the
/// wire-level set the same way.
pub fn uid_set(uids: &[u32]) -> String {
    // Pre-sizing avoids reallocs for the common "few hundred UIDs" case.
    let mut out = String::with_capacity(uids.len() * 7);
    for (i, uid) in uids.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        use std::fmt::Write;
        let _ = write!(out, "{uid}");
    }
    out
}

/// Quick-and-dirty sniff for PGP-encrypted content — matches either
/// an ASCII-armored block (`-----BEGIN PGP MESSAGE`) or an RFC 3156
/// `multipart/encrypted` Content-Type. Duplicated across the sync
/// imports and the category purge; one source of truth here.
pub fn is_pgp_encrypted(body: &[u8]) -> bool {
    body.windows(22)
        .any(|w| w.starts_with(b"-----BEGIN PGP MESSAGE"))
        || body
            .windows(19)
            .any(|w| w.starts_with(b"multipart/encrypted"))
}

pub struct Parsed {
    pub message_id: String,
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
    pub subject: Option<String>,
    pub from_addr: Option<String>,
    pub to_addrs: Option<String>,
    pub cc_addrs: Option<String>,
    pub date_utc: i64,
    pub snippet: Option<String>,
    pub body_text: Option<String>,
    pub has_attachments: bool,
    /// Address from `Disposition-Notification-To:` (RFC 8098) — set
    /// when the sender requested a read receipt. We never auto-send
    /// the MDN; the read view shows a manual "Send receipt" banner.
    pub receipt_to: Option<String>,
}

/// Parse RFC822 bytes into our flat `Parsed` shape.
/// Falls back to safe defaults for missing fields — real-world mail
/// is messy and we never want a single bad message to stop a sync.
pub fn parse(raw: &[u8]) -> Parsed {
    let parser = MessageParser::default();
    let Some(msg) = parser.parse(raw) else {
        return Parsed {
            // Length + content hash: length alone collides across distinct
            // unparseable messages, and a collision feeds the
            // delete_after_sync dedup path — the second message would be
            // judged "already stored" and its server copy deleted.
            message_id: format!("<postern-unparseable-{}-{}>", raw.len(), blake_like(raw)),
            in_reply_to: None,
            references: Vec::new(),
            subject: None,
            from_addr: None,
            to_addrs: None,
            cc_addrs: None,
            date_utc: 0,
            snippet: None,
            body_text: None,
            has_attachments: false,
            receipt_to: None,
        };
    };

    let message_id = msg.message_id().map_or_else(
        || format!("<postern-synth-{}>", blake_like(raw)),
        normalize_mid,
    );

    // In-Reply-To + References are Message-ID lists. mail-parser returns
    // them on `HeaderValue::TextList` when multiple IDs are present and
    // `Text` when just one — handle both shapes.
    let mut in_reply_to: Option<String> = None;
    let mut references: Vec<String> = Vec::new();
    let mut receipt_to: Option<String> = None;
    for h in &msg.parts[0].headers {
        let name = h.name.as_str();
        if name.eq_ignore_ascii_case("in-reply-to") {
            match &h.value {
                HeaderValue::Text(s) => in_reply_to = Some(normalize_mid(s)),
                HeaderValue::TextList(list) => in_reply_to = list.first().map(|s| normalize_mid(s)),
                _ => {}
            }
        } else if name.eq_ignore_ascii_case("references") {
            match &h.value {
                HeaderValue::Text(s) => {
                    references = split_message_ids(s);
                }
                HeaderValue::TextList(list) => {
                    references = list.iter().map(|s| normalize_mid(s)).collect();
                }
                _ => {}
            }
        } else if name.eq_ignore_ascii_case("disposition-notification-to") {
            // RFC 8098 §2.1. Value can be a plain address or
            // "Name <addr@host>" — extract just the address part.
            // Mail-parser doesn't structurally parse this header, so
            // we read it as raw text and pull the address ourselves.
            if let HeaderValue::Text(s) = &h.value {
                receipt_to = parse_receipt_addr(s);
            }
        }
    }

    let subject = msg.subject().map(str::to_owned);

    let from_addr = msg
        .from()
        .and_then(|a| a.first())
        .and_then(|addr| addr.address().map(str::to_owned));

    let to_addrs = msg.to().map(|list| {
        list.iter()
            .filter_map(|a| a.address())
            .collect::<Vec<_>>()
            .join(", ")
    });
    let cc_addrs = msg.cc().map(|list| {
        list.iter()
            .filter_map(|a| a.address())
            .collect::<Vec<_>>()
            .join(", ")
    });

    let date_utc = msg.date().map_or(0, mail_parser::DateTime::to_timestamp);

    // Prefer text/plain body for indexing; fall back to stripped HTML.
    let body_text = msg
        .body_text(0)
        .map(std::borrow::Cow::into_owned)
        .or_else(|| msg.body_html(0).map(|s| strip_html(&s)));

    let snippet = body_text.as_ref().map(|b| make_snippet(b));

    let has_attachments = msg.attachments().next().is_some();

    let indexed_body = body_text
        .as_ref()
        .map(|b| truncate_chars(b, BODY_INDEX_CAP));

    Parsed {
        message_id,
        in_reply_to,
        references,
        subject,
        from_addr,
        to_addrs,
        cc_addrs,
        date_utc,
        snippet,
        body_text: indexed_body,
        has_attachments,
        receipt_to,
    }
}

/// Pull a bare email address out of a `Disposition-Notification-To`
/// header value. Accepts "user@host" or "Name <user@host>"; returns
/// None when neither shape matches. Trims surrounding whitespace.
fn parse_receipt_addr(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let (Some(lt), Some(gt)) = (trimmed.find('<'), trimmed.rfind('>')) {
        if lt < gt {
            let inner = trimmed[lt + 1..gt].trim();
            return if inner.contains('@') {
                Some(inner.to_owned())
            } else {
                None
            };
        }
    }
    if trimmed.contains('@') {
        Some(trimmed.to_owned())
    } else {
        None
    }
}

/// Strip any surrounding angle brackets and whitespace, then re-wrap in
/// `<...>`. Keeps our internal Message-ID convention uniform regardless
/// of whether the header arrived with the brackets or not.
fn normalize_mid(s: &str) -> String {
    let cleaned = s.trim().trim_matches(|c: char| c == '<' || c == '>').trim();
    format!("<{cleaned}>")
}

/// Split a raw `References:` value into individual Message-IDs. The
/// format is whitespace-separated `<...>`-wrapped IDs per RFC 5322.
fn split_message_ids(raw: &str) -> Vec<String> {
    let mut out = Vec::new();
    for tok in raw.split_whitespace() {
        let n = normalize_mid(tok);
        if n != "<>" {
            out.push(n);
        }
    }
    out
}

/// Public shortcut for the `body_text` backfill path: parse, return the
/// indexed body text (capped at `BODY_INDEX_CAP`) or None if we can't
/// pull anything useful.
pub fn body_text_of(raw: &[u8]) -> Option<String> {
    parse(raw).body_text
}

pub fn into_new_message(
    account_id: i64,
    p: Parsed,
    blob_sha256: String,
    size_bytes: usize,
    label_names: Vec<String>,
    is_read: bool,
    is_encrypted: bool,
) -> NewMessage {
    NewMessage {
        account_id,
        message_id: p.message_id,
        subject: p.subject,
        from_addr: p.from_addr,
        to_addrs: p.to_addrs,
        cc_addrs: p.cc_addrs,
        date_utc: p.date_utc,
        blob_sha256,
        size_bytes: size_bytes as i64,
        snippet: p.snippet,
        body_text: p.body_text,
        has_attachments: p.has_attachments,
        is_read,
        is_encrypted,
        receipt_to: p.receipt_to,
        label_names,
    }
}

fn make_snippet(body: &str) -> String {
    let cleaned: String = body
        .chars()
        .map(|c| if c.is_control() && c != ' ' { ' ' } else { c })
        .collect();
    let collapsed: String = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    truncate_chars(&collapsed, SNIPPET_LEN)
}

fn truncate_chars(s: &str, max: usize) -> String {
    let mut out = String::with_capacity(s.len().min(max));
    for (i, ch) in s.chars().enumerate() {
        if i >= max {
            break;
        }
        out.push(ch);
    }
    out
}

/// Crude HTML-to-text: strip tags, decode a handful of common entities.
/// Good enough for FTS and snippets. Never served to the UI.
fn strip_html(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
}

/// Tiny non-cryptographic fingerprint used only to synthesize a
/// Message-ID when one is missing. Not security-relevant.
fn blake_like(data: &[u8]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in data.iter().take(4096) {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    format!("{h:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uid_set_empty_and_single() {
        assert_eq!(uid_set(&[]), "");
        assert_eq!(uid_set(&[42]), "42");
    }

    #[test]
    fn uid_set_joins_with_commas_no_trailing() {
        assert_eq!(uid_set(&[1, 2, 3]), "1,2,3");
        assert_eq!(uid_set(&[10, 20, 30, 40]), "10,20,30,40");
    }

    #[test]
    fn detects_pgp_armor() {
        assert!(is_pgp_encrypted(b"foo\n-----BEGIN PGP MESSAGE-----\nbar"));
        assert!(is_pgp_encrypted(
            b"Content-Type: multipart/encrypted; protocol=\"application/pgp-encrypted\""
        ));
    }

    #[test]
    fn ignores_unrelated_bodies() {
        assert!(!is_pgp_encrypted(b"plain old text"));
        assert!(!is_pgp_encrypted(b"-----BEGIN PGP PUBLIC KEY BLOCK-----"));
        assert!(!is_pgp_encrypted(b""));
    }

    #[test]
    fn parses_plain_message() {
        let raw = b"From: alice@example.com\r\n\
                    To: bob@example.com\r\n\
                    Subject: Hello\r\n\
                    Message-ID: <abc@example.com>\r\n\
                    Date: Tue, 14 Apr 2026 12:00:00 +0000\r\n\
                    \r\n\
                    Hi there, this is the body.";
        let p = parse(raw);
        assert_eq!(p.subject.as_deref(), Some("Hello"));
        assert_eq!(p.from_addr.as_deref(), Some("alice@example.com"));
        assert_eq!(p.to_addrs.as_deref(), Some("bob@example.com"));
        assert_eq!(p.message_id, "<abc@example.com>");
        assert!(p.body_text.as_deref().unwrap().contains("this is the body"));
        assert!(p.snippet.as_deref().unwrap().contains("this is the body"));
    }

    #[test]
    fn reply_exposes_in_reply_to_and_references() {
        let raw = b"From: bob@example.com\r\n\
                    Subject: Re: Hello\r\n\
                    Message-ID: <xyz@example.com>\r\n\
                    In-Reply-To: <abc@example.com>\r\n\
                    References: <abc@example.com> <mid@example.com>\r\n\
                    Date: Tue, 14 Apr 2026 13:00:00 +0000\r\n\
                    \r\n\
                    Reply body.";
        let p = parse(raw);
        assert_eq!(p.message_id, "<xyz@example.com>");
        assert_eq!(p.in_reply_to.as_deref(), Some("<abc@example.com>"));
        assert_eq!(p.references, vec!["<abc@example.com>", "<mid@example.com>"]);
    }

    #[test]
    fn synthesizes_message_id_when_missing() {
        let raw = b"From: x\r\nSubject: s\r\n\r\nbody";
        let p = parse(raw);
        assert!(
            p.message_id.starts_with("<postern-synth-"),
            "got: {}",
            p.message_id
        );
    }

    #[test]
    fn synthetic_ids_differ_for_distinct_content_of_same_length() {
        // Same length, different content must get distinct synthetic
        // Message-IDs — a collision feeds delete_after_sync's "already
        // stored" check and would purge an unstored server copy.
        let a = parse(&[0u8, 1, 2, 3]);
        let b = parse(&[0u8, 9, 8, 7]);
        assert_ne!(a.message_id, b.message_id);
    }

    #[test]
    fn snippet_collapses_whitespace() {
        let raw = b"Subject: s\r\n\r\nline one\r\n\r\n  line   two  \r\n";
        let p = parse(raw);
        assert_eq!(p.snippet.as_deref(), Some("line one line two"));
    }

    #[test]
    fn extracts_disposition_notification_to() {
        let raw = b"From: alice@example.com\r\n\
                    To: bob@example.com\r\n\
                    Subject: ping\r\n\
                    Disposition-Notification-To: Alice <alice@example.com>\r\n\
                    Message-ID: <r@x>\r\n\
                    \r\n\
                    body";
        let p = parse(raw);
        assert_eq!(p.receipt_to.as_deref(), Some("alice@example.com"));
    }

    #[test]
    fn no_receipt_when_header_absent() {
        let raw = b"From: a@x\r\nSubject: s\r\nMessage-ID: <m@x>\r\n\r\nbody";
        assert!(parse(raw).receipt_to.is_none());
    }

    #[test]
    fn receipt_addr_handles_bare_address() {
        assert_eq!(
            parse_receipt_addr("alice@example.com"),
            Some("alice@example.com".to_owned())
        );
    }

    #[test]
    fn receipt_addr_rejects_garbage() {
        assert!(parse_receipt_addr("").is_none());
        assert!(parse_receipt_addr("not an address").is_none());
    }
}
