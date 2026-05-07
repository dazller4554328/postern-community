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

impl Parsed {
    /// Canonical thread root: first References → In-Reply-To → self.
    /// That's the JWZ algorithm in its simplest form: the earliest
    /// Message-ID any replies in the chain reference is the thread root.
    pub fn thread_id(&self) -> String {
        if let Some(root) = self.references.first() {
            return root.clone();
        }
        if let Some(irt) = &self.in_reply_to {
            return irt.clone();
        }
        self.message_id.clone()
    }

    /// True when this message has no parent header reference — i.e.,
    /// `thread_id()` defaulted to the message's own ID. Used by the
    /// storage layer to decide whether to attempt the subject-based
    /// fallback merge (JWZ step 4).
    pub fn is_thread_orphan(&self) -> bool {
        self.references.is_empty() && self.in_reply_to.is_none()
    }

    /// Normalized subject key for cross-message clustering when header
    /// threading fails. Lowercased, with repeated `Re:`/`Fwd:` and
    /// leading `[list-tag]` prefixes stripped. `None` for messages
    /// whose subject is empty after normalization.
    pub fn subject_key(&self) -> Option<String> {
        self.subject.as_deref().and_then(normalize_subject)
    }

    /// True when the raw subject carried a reply-ish prefix (Re:, Fwd:,
    /// list bracket, …). We only cluster by subject_key when this is
    /// true — otherwise two unrelated mails titled "Hi" would get
    /// glued together.
    pub fn has_reply_prefix(&self) -> bool {
        self.subject
            .as_deref()
            .map(|s| has_reply_prefix(s))
            .unwrap_or(false)
    }
}

/// Parse RFC822 bytes into our flat `Parsed` shape.
/// Falls back to safe defaults for missing fields — real-world mail
/// is messy and we never want a single bad message to stop a sync.
pub fn parse(raw: &[u8]) -> Parsed {
    let parser = MessageParser::default();
    let Some(msg) = parser.parse(raw) else {
        return Parsed {
            message_id: format!("<postern-unparseable-{}>", raw.len()),
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

    let message_id = msg
        .message_id()
        .map(|s| normalize_mid(s))
        .unwrap_or_else(|| format!("<postern-synth-{}>", blake_like(raw)));

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

    let date_utc = msg.date().map(|d| d.to_timestamp()).unwrap_or(0);

    // Prefer text/plain body for indexing; fall back to stripped HTML.
    let body_text = msg
        .body_text(0)
        .map(|s| s.into_owned())
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

/// Public shortcut for the body_text backfill path: parse, return the
/// indexed body text (capped at `BODY_INDEX_CAP`) or None if we can't
/// pull anything useful.
pub fn body_text_of(raw: &[u8]) -> Option<String> {
    parse(raw).body_text
}

/// Public shortcut for the threading backfill: compute the thread root
/// for a raw blob without going through upsert_message.
pub fn thread_id_of(raw: &[u8]) -> String {
    parse(raw).thread_id()
}

/// Public shortcut for the subject_key backfill: pull the normalized
/// subject clustering key out of a raw blob.
pub fn subject_key_of(raw: &[u8]) -> Option<String> {
    parse(raw).subject_key()
}

/// Reply-prefix tokens Mailpile and Thunderbird both strip when
/// normalizing for threading. Case-insensitive match. German (`Aw:`,
/// `Wg:`) and Spanish (`Rv:`) are common enough in public mail to be
/// worth covering.
const REPLY_PREFIXES: &[&str] = &["re:", "fwd:", "fw:", "aw:", "wg:", "rv:"];

/// Strip all leading reply and list-tag prefixes, collapse whitespace,
/// lowercase, and return the result. Returns `None` when the
/// normalized subject is empty (whitespace-only inputs, or a subject
/// that was nothing *but* reply prefixes).
fn normalize_subject(raw: &str) -> Option<String> {
    let mut cur = raw.trim();
    loop {
        let before = cur.len();

        // Mailing list tag(s) like `[ops]` or `[postern-dev]`. Strip
        // repeatedly so `[ops][urgent] Re: foo` collapses cleanly.
        while cur.starts_with('[') {
            if let Some(end) = cur.find(']') {
                cur = cur[end + 1..].trim_start();
            } else {
                break;
            }
        }

        // Reply-style prefix. Case-insensitive match on ASCII prefixes —
        // those are always ASCII-only so slicing by byte length is safe.
        let lower = cur.to_ascii_lowercase();
        for pfx in REPLY_PREFIXES {
            if lower.starts_with(pfx) {
                cur = cur[pfx.len()..].trim_start();
                break;
            }
        }

        if cur.len() == before {
            break;
        }
    }

    let collapsed: String = cur.split_whitespace().collect::<Vec<_>>().join(" ");
    let normalized = collapsed.to_ascii_lowercase();
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

fn has_reply_prefix(raw: &str) -> bool {
    let trimmed = raw.trim_start();
    // List tag on its own doesn't count as "reply" — it's just a list
    // bucket. We only merge by subject when we see an actual Re/Fwd.
    let after_brackets = {
        let mut cur = trimmed;
        while cur.starts_with('[') {
            if let Some(end) = cur.find(']') {
                cur = cur[end + 1..].trim_start();
            } else {
                break;
            }
        }
        cur
    };
    let lower = after_brackets.to_ascii_lowercase();
    REPLY_PREFIXES.iter().any(|p| lower.starts_with(p))
}

pub fn into_new_message(
    account_id: i64,
    p: Parsed,
    blob_sha256: String,
    size_bytes: usize,
    label_names: Vec<String>,
    _imap_fallback_thread_id: Option<String>,
    is_read: bool,
    is_encrypted: bool,
) -> NewMessage {
    let thread_id = Some(p.thread_id());
    let subject_key = p.subject_key();
    let has_reply_prefix = p.has_reply_prefix();
    let is_thread_orphan = p.is_thread_orphan();
    NewMessage {
        account_id,
        message_id: p.message_id,
        thread_id,
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
        subject_key,
        has_reply_prefix,
        is_thread_orphan,
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
        // No References/In-Reply-To → the message is its own thread root.
        assert_eq!(p.thread_id(), "<abc@example.com>");
    }

    #[test]
    fn reply_uses_references_root_as_thread_id() {
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
        assert_eq!(p.thread_id(), "<abc@example.com>");
    }

    #[test]
    fn reply_without_references_falls_back_to_in_reply_to() {
        let raw = b"From: bob@example.com\r\n\
                    Subject: Re: Hello\r\n\
                    Message-ID: <xyz@example.com>\r\n\
                    In-Reply-To: <abc@example.com>\r\n\
                    \r\n\
                    Body.";
        let p = parse(raw);
        assert_eq!(p.thread_id(), "<abc@example.com>");
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
    fn snippet_collapses_whitespace() {
        let raw = b"Subject: s\r\n\r\nline one\r\n\r\n  line   two  \r\n";
        let p = parse(raw);
        assert_eq!(p.snippet.as_deref(), Some("line one line two"));
    }

    #[test]
    fn subject_key_strips_reply_prefixes() {
        assert_eq!(normalize_subject("Re: Hello"), Some("hello".into()));
        assert_eq!(normalize_subject("RE: FWD: Hello"), Some("hello".into()));
        assert_eq!(
            normalize_subject("fwd: Re: Re: Invoice #42"),
            Some("invoice #42".into())
        );
    }

    #[test]
    fn subject_key_strips_list_tags() {
        assert_eq!(
            normalize_subject("[ops] Re: deploy broke"),
            Some("deploy broke".into())
        );
        assert_eq!(
            normalize_subject("[postern-dev][urgent] Re: Re: build"),
            Some("build".into()),
        );
    }

    #[test]
    fn subject_key_handles_plain_subject() {
        assert_eq!(normalize_subject("Invoice #42"), Some("invoice #42".into()));
    }

    #[test]
    fn subject_key_returns_none_for_empty() {
        assert_eq!(normalize_subject(""), None);
        assert_eq!(normalize_subject("   "), None);
        assert_eq!(normalize_subject("Re: "), None);
    }

    #[test]
    fn reply_prefix_detected_on_raw_subject() {
        assert!(has_reply_prefix("Re: hello"));
        assert!(has_reply_prefix("FWD: hello"));
        assert!(has_reply_prefix("[ops] Re: deploy"));
        assert!(!has_reply_prefix("[ops] deploy"));
        assert!(!has_reply_prefix("Hello"));
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

    #[test]
    fn orphan_detection_requires_no_threading_headers() {
        let orphan = parse(b"From: a@x\r\nSubject: Re: x\r\nMessage-ID: <m@x>\r\n\r\nbody");
        assert!(orphan.is_thread_orphan());
        let reply = parse(
            b"From: a@x\r\nSubject: Re: x\r\nMessage-ID: <m@x>\r\nIn-Reply-To: <p@x>\r\n\r\nbody",
        );
        assert!(!reply.is_thread_orphan());
    }
}
