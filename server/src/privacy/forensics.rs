//! Deep inspection of an RFC822 message for the UI forensics pane.
//!
//! What we surface:
//!   - All headers (order-preserving)
//!   - Parsed `Received:` chain (deliveryhop path)
//!   - SPF / DKIM / DMARC verdicts from `Authentication-Results`
//!   - Spam score when the receiving MTA added one
//!   - PGP / S/MIME encryption + signature detection
//!   - MIME tree (recursive content-type breakdown)
//!   - Attachment list with filename + mime + size
//!
//! This is best-effort — wild-west real-world email is full of broken
//! headers. We tolerate it and return what we can.

use std::net::IpAddr;

use mail_parser::{HeaderValue, MessageParser, MimeHeaders, PartType};
use serde::Serialize;

use super::geoip;

#[derive(Debug, Serialize)]
pub struct Forensics {
    pub headers: Vec<Header>,
    pub received_chain: Vec<ReceivedHop>,
    pub auth: AuthResults,
    pub is_pgp_encrypted: bool,
    pub is_pgp_signed: bool,
    pub is_smime_signed: bool,
    pub is_smime_encrypted: bool,
    pub spam_score: Option<f64>,
    pub spam_status: Option<String>,
    pub size_bytes: usize,
    pub attachments: Vec<AttachmentMeta>,
    pub mime_tree: Vec<MimeNode>,
}

#[derive(Debug, Serialize)]
pub struct Header {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Serialize)]
pub struct ReceivedHop {
    pub from: Option<String>,
    pub by: Option<String>,
    pub with: Option<String>,
    /// First public IP we found in the header (usually the upstream
    /// peer's address). `None` for private hops or when no IP appears.
    pub ip: Option<String>,
    /// ISO 3166-1 alpha-2 country code from the offline `GeoIP` DB.
    /// `None` if no DB is installed, the IP is private, or unmatched.
    pub country_code: Option<String>,
    /// Human-readable country name (English).
    pub country_name: Option<String>,
    pub raw: String,
}

#[derive(Debug, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Verdict {
    Pass,
    Fail,
    SoftFail,
    Neutral,
    TempError,
    PermError,
    None,
    Unknown,
}

#[derive(Debug, Serialize)]
pub struct AuthResults {
    pub spf: Verdict,
    pub dkim: Verdict,
    pub dmarc: Verdict,
    /// Raw text of the Authentication-Results header(s) we parsed.
    pub raw: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct AttachmentMeta {
    pub filename: Option<String>,
    pub content_type: String,
    pub size_bytes: usize,
}

#[derive(Debug, Serialize)]
pub struct MimeNode {
    pub content_type: String,
    pub size_bytes: usize,
    pub is_attachment: bool,
    pub filename: Option<String>,
}

pub fn analyze(raw: &[u8]) -> Forensics {
    let parser = MessageParser::default();
    let Some(msg) = parser.parse(raw) else {
        return Forensics {
            headers: vec![],
            received_chain: vec![],
            auth: AuthResults {
                spf: Verdict::Unknown,
                dkim: Verdict::Unknown,
                dmarc: Verdict::Unknown,
                raw: vec![],
            },
            is_pgp_encrypted: false,
            is_pgp_signed: false,
            is_smime_signed: false,
            is_smime_encrypted: false,
            spam_score: None,
            spam_status: None,
            size_bytes: raw.len(),
            attachments: vec![],
            mime_tree: vec![],
        };
    };

    // Headers — mail-parser's root part is part 0. For Received
    // headers we slice the original bytes ourselves: `mail-parser`
    // parses them into a structured `HeaderValue::Received` and our
    // text-form serializer flattens that to an empty string, so the
    // UI was showing blank `Received:` rows and the chain parser had
    // nothing to chew on.
    let headers: Vec<Header> = msg.parts[0]
        .headers
        .iter()
        .map(|h| Header {
            name: h.name.as_str().to_owned(),
            value: if matches!(h.value, HeaderValue::Received(_)) {
                header_raw_text(raw, h)
            } else {
                header_value_to_string(&h.value)
            },
        })
        .collect();

    let received_chain: Vec<ReceivedHop> = msg.parts[0]
        .headers
        .iter()
        .filter(|h| h.name.as_str().eq_ignore_ascii_case("received"))
        .map(|h| parse_received(&header_raw_text(raw, h)))
        .collect();

    let auth_lines: Vec<String> = msg.parts[0]
        .headers
        .iter()
        .filter(|h| {
            h.name
                .as_str()
                .eq_ignore_ascii_case("authentication-results")
        })
        .map(|h| header_value_to_string(&h.value))
        .collect();
    let auth = parse_auth_results(&auth_lines);

    let (spam_score, spam_status) = parse_spam(&msg);
    let (is_pgp_encrypted, is_pgp_signed) = detect_pgp(&msg);
    let (is_smime_encrypted, is_smime_signed) = detect_smime(&msg);

    let mut mime_tree = Vec::new();
    let mut attachments = Vec::new();
    collect_mime(&msg, &mut mime_tree, &mut attachments);

    Forensics {
        headers,
        received_chain,
        auth,
        is_pgp_encrypted,
        is_pgp_signed,
        is_smime_signed,
        is_smime_encrypted,
        spam_score,
        spam_status,
        size_bytes: raw.len(),
        attachments,
        mime_tree,
    }
}

/// Slice the original RFC822 bytes for a single header value. Used
/// for headers (like `Received:`) where mail-parser's structured
/// representation strips information that downstream code needs.
/// Folded multi-line headers come through with their CRLF + leading
/// whitespace intact — collapse them so the line-based parser can
/// find `from`/`by`/`with`/IP tokens regardless of how the original
/// MTA wrapped the header.
fn header_raw_text(raw: &[u8], h: &mail_parser::Header<'_>) -> String {
    if h.offset_start >= h.offset_end || h.offset_end > raw.len() {
        return String::new();
    }
    let slice = &raw[h.offset_start..h.offset_end];
    let text = std::str::from_utf8(slice).unwrap_or("");
    let mut out = String::with_capacity(text.len());
    let mut prev_space = false;
    for c in text.chars() {
        if c == '\r' || c == '\n' || c == '\t' || c == ' ' {
            // Collapse any run of whitespace to a single space.
            if !prev_space {
                out.push(' ');
                prev_space = true;
            }
        } else {
            out.push(c);
            prev_space = false;
        }
    }
    out.trim().to_owned()
}

fn header_value_to_string(v: &HeaderValue<'_>) -> String {
    match v {
        HeaderValue::Text(s) => s.to_string(),
        HeaderValue::TextList(list) => list
            .iter()
            .map(std::string::ToString::to_string)
            .collect::<Vec<_>>()
            .join(", "),
        HeaderValue::Address(addr) => addr
            .first()
            .map(|a| {
                let name = a.name().unwrap_or("").to_owned();
                let email = a.address().unwrap_or("").to_owned();
                if name.is_empty() {
                    email
                } else {
                    format!("{name} <{email}>")
                }
            })
            .unwrap_or_default(),
        HeaderValue::DateTime(d) => d.to_rfc3339(),
        HeaderValue::ContentType(ct) => ct.c_type.to_string(),
        HeaderValue::Received(_) => String::new(),
        HeaderValue::Empty => String::new(),
    }
}

fn parse_received(line: &str) -> ReceivedHop {
    // Received: from HOST (...) by HOST (...) with PROTO ...
    let from = after_token(line, "from ");
    let by = after_token(line, "by ");
    let with = after_token(line, "with ");
    let ip = first_public_ip(line);
    let (country_code, country_name) = match ip.as_deref().and_then(|s| s.parse::<IpAddr>().ok()) {
        Some(addr) => match geoip::lookup(addr) {
            Some((c, n)) => (Some(c), Some(n)),
            None => (None, None),
        },
        None => (None, None),
    };
    ReceivedHop {
        from,
        by,
        with,
        ip,
        country_code,
        country_name,
        raw: line.to_owned(),
    }
}

/// Pull the first public, parseable IP (v4 or v6) out of a `Received:`
/// header. Common shapes:
///
///   from mail.example.com ([192.0.2.1]) by ...
///   from mail.example.com (mail.example.com [192.0.2.1]) by ...
///   from [`2001:db8::1`] (helo=...) by ...
///   from mail.example.com (192.0.2.1) by ...
///
/// We scan bracketed candidates first (most specific), then fall back to
/// any standalone v4/v6 token in the line. Private/loopback addresses
/// are skipped — they're never the originating internet hop.
fn first_public_ip(line: &str) -> Option<String> {
    for cand in extract_ip_candidates(line) {
        if let Ok(ip) = cand.parse::<IpAddr>() {
            if is_public_ip(ip) {
                return Some(ip.to_string());
            }
        }
    }
    None
}

fn extract_ip_candidates(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    // Bracketed: [1.2.3.4] or [2001:db8::1]
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'[' {
            if let Some(end) = line[i + 1..].find(']') {
                let inner = &line[i + 1..i + 1 + end];
                if !inner.is_empty() {
                    out.push(inner.to_owned());
                }
                i += end + 2;
                continue;
            }
        }
        i += 1;
    }
    // Bare IPv4 tokens — split on common delimiters and try each.
    for tok in line
        .split(|c: char| c.is_whitespace() || matches!(c, '(' | ')' | ',' | ';' | '<' | '>' | '"'))
    {
        let t = tok.trim_matches(|c: char| !c.is_ascii_hexdigit() && c != '.' && c != ':');
        if t.is_empty() {
            continue;
        }
        if t.contains('.') || t.contains(':') {
            out.push(t.to_owned());
        }
    }
    out
}

fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            !(v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4.is_unspecified()
                || v4.octets()[0] == 0)
        }
        IpAddr::V6(v6) => {
            if v6.is_loopback() || v6.is_unspecified() || v6.is_multicast() {
                return false;
            }
            let s = v6.segments();
            // fc00::/7 — Unique-Local Addresses (the IPv6 RFC1918 equivalent).
            if (s[0] & 0xfe00) == 0xfc00 {
                return false;
            }
            // fe80::/10 — link-local.
            if (s[0] & 0xffc0) == 0xfe80 {
                return false;
            }
            // 2001:db8::/32 — documentation.
            if s[0] == 0x2001 && s[1] == 0x0db8 {
                return false;
            }
            // 2002::/16 — 6to4 transition prefix. Deprecated for routed
            // traffic and Google reuses this shape as an internal SMTP
            // server identifier in `Received: by 2002:…` headers, so
            // it's never a useful "originating IP" for forensics.
            if s[0] == 0x2002 {
                return false;
            }
            true
        }
    }
}

fn after_token(line: &str, token: &str) -> Option<String> {
    let idx = line
        .to_ascii_lowercase()
        .find(&token.to_ascii_lowercase())?;
    let rest = &line[idx + token.len()..];
    // Take up to the next space, semicolon, or open-paren.
    let end = rest.find([' ', ';', '(']).unwrap_or(rest.len());
    let val = rest[..end]
        .trim()
        .trim_matches(|c: char| c == ',' || c == '"');
    if val.is_empty() {
        None
    } else {
        Some(val.to_owned())
    }
}

fn parse_auth_results(lines: &[String]) -> AuthResults {
    let mut spf = Verdict::Unknown;
    let mut dkim = Verdict::Unknown;
    let mut dmarc = Verdict::Unknown;

    for line in lines {
        let l = line.to_ascii_lowercase();
        if let Some(v) = extract_verdict(&l, "spf=") {
            spf = v;
        }
        if let Some(v) = extract_verdict(&l, "dkim=") {
            dkim = v;
        }
        if let Some(v) = extract_verdict(&l, "dmarc=") {
            dmarc = v;
        }
    }

    AuthResults {
        spf,
        dkim,
        dmarc,
        raw: lines.to_vec(),
    }
}

fn extract_verdict(line: &str, key: &str) -> Option<Verdict> {
    let idx = line.find(key)?;
    let rest = &line[idx + key.len()..];
    let end = rest
        .find(|c: char| !c.is_alphanumeric())
        .unwrap_or(rest.len());
    let verdict = &rest[..end];
    Some(match verdict {
        "pass" => Verdict::Pass,
        "fail" => Verdict::Fail,
        "softfail" => Verdict::SoftFail,
        "neutral" => Verdict::Neutral,
        "temperror" => Verdict::TempError,
        "permerror" => Verdict::PermError,
        "none" => Verdict::None,
        _ => Verdict::Unknown,
    })
}

fn parse_spam(msg: &mail_parser::Message<'_>) -> (Option<f64>, Option<String>) {
    let mut score: Option<f64> = None;
    let mut status: Option<String> = None;
    for h in &msg.parts[0].headers {
        let name = h.name.as_str().to_ascii_lowercase();
        let value = header_value_to_string(&h.value);
        if name == "x-spam-score" {
            score = value.trim().parse().ok();
        } else if name == "x-spam-status" {
            if score.is_none() {
                // Some MTAs embed score in status: "Yes, score=5.1 required=5.0 ..."
                if let Some(s) = value.to_ascii_lowercase().split("score=").nth(1) {
                    let n: String = s
                        .chars()
                        .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
                        .collect();
                    score = n.parse().ok();
                }
            }
            status = Some(value);
        } else if name == "x-spamd-result" || name == "x-rspamd-score" {
            score = value.split_whitespace().find_map(|tok| tok.parse().ok());
        }
    }
    (score, status)
}

fn detect_pgp(msg: &mail_parser::Message<'_>) -> (bool, bool) {
    let mut encrypted = false;
    let mut signed = false;
    for part in &msg.parts {
        if let Some(ct) = part.content_type() {
            let typ = ct.c_type.to_ascii_lowercase();
            let subtype = ct
                .c_subtype
                .as_ref()
                .map(|s| s.to_ascii_lowercase())
                .unwrap_or_default();
            if typ == "multipart"
                && subtype == "encrypted"
                && ct
                    .attribute("protocol")
                    .is_some_and(|p| p.eq_ignore_ascii_case("application/pgp-encrypted"))
            {
                encrypted = true;
            }
            if typ == "multipart"
                && subtype == "signed"
                && ct
                    .attribute("protocol")
                    .is_some_and(|p| p.eq_ignore_ascii_case("application/pgp-signature"))
            {
                signed = true;
            }
        }
        // Inline PGP armor in a text part — less common but real.
        if let PartType::Text(text) = &part.body {
            if text.contains("-----BEGIN PGP MESSAGE-----") {
                encrypted = true;
            }
            if text.contains("-----BEGIN PGP SIGNED MESSAGE-----") {
                signed = true;
            }
        }
    }
    (encrypted, signed)
}

fn detect_smime(msg: &mail_parser::Message<'_>) -> (bool, bool) {
    let mut encrypted = false;
    let mut signed = false;
    for part in &msg.parts {
        if let Some(ct) = part.content_type() {
            let typ = ct.c_type.to_ascii_lowercase();
            let subtype = ct
                .c_subtype
                .as_ref()
                .map(|s| s.to_ascii_lowercase())
                .unwrap_or_default();
            if typ == "application" && (subtype == "pkcs7-mime" || subtype == "x-pkcs7-mime") {
                // smime-type=enveloped-data → encrypted
                // smime-type=signed-data    → signed (opaque; rare vs multipart/signed)
                let smime_type = ct
                    .attribute("smime-type")
                    .map(str::to_ascii_lowercase)
                    .unwrap_or_default();
                if smime_type == "enveloped-data" {
                    encrypted = true;
                } else if smime_type == "signed-data" {
                    signed = true;
                } else {
                    // No explicit type → assume encrypted (the more common case).
                    encrypted = true;
                }
            }
            if typ == "multipart" && subtype == "signed" {
                let protocol = ct
                    .attribute("protocol")
                    .map(str::to_ascii_lowercase)
                    .unwrap_or_default();
                if protocol == "application/pkcs7-signature"
                    || protocol == "application/x-pkcs7-signature"
                {
                    signed = true;
                }
            }
        }
    }
    (encrypted, signed)
}

fn collect_mime(
    msg: &mail_parser::Message<'_>,
    tree: &mut Vec<MimeNode>,
    attachments: &mut Vec<AttachmentMeta>,
) {
    for part in &msg.parts {
        let content_type = part
            .content_type()
            .map(|ct| {
                let sub = ct
                    .c_subtype
                    .as_ref()
                    .map_or("", std::convert::AsRef::as_ref);
                if sub.is_empty() {
                    ct.c_type.to_string()
                } else {
                    format!("{}/{}", ct.c_type, sub)
                }
            })
            .unwrap_or_else(|| "unknown".into());

        let size_bytes = match &part.body {
            PartType::Text(t) => t.len(),
            PartType::Html(h) => h.len(),
            PartType::Binary(b) | PartType::InlineBinary(b) => b.len(),
            _ => 0,
        };

        let is_attachment = matches!(part.body, PartType::Binary(_) | PartType::InlineBinary(_))
            || part.attachment_name().is_some();
        let filename = part.attachment_name().map(str::to_owned);

        // Banks + a lot of corporate mail gateways tag real PDFs / images
        // as application/octet-stream. Sniff from the filename so the
        // inline-preview whitelist doesn't refuse a legitimate PDF.
        let normalized_type = if is_attachment {
            sniff_type(&content_type, filename.as_deref())
        } else {
            content_type.clone()
        };

        tree.push(MimeNode {
            content_type: normalized_type.clone(),
            size_bytes,
            is_attachment,
            filename: filename.clone(),
        });

        if is_attachment {
            attachments.push(AttachmentMeta {
                filename,
                content_type: normalized_type,
                size_bytes,
            });
        }
    }
}

/// Return a best-guess content-type. If the declared type is one of
/// the generic fallbacks (octet-stream, unknown, empty), derive from
/// the filename extension. Otherwise trust what the sender said —
/// they know better than our extension table for niche formats.
pub fn sniff_type(declared: &str, filename: Option<&str>) -> String {
    let d = declared.trim().to_ascii_lowercase();
    let needs_sniff = d.is_empty()
        || d == "unknown"
        || d == "application/octet-stream"
        || d == "application/x-download"
        || d == "application/force-download"
        || d == "application/binary"
        || d == "binary/octet-stream";
    if !needs_sniff {
        return declared.to_string();
    }
    let Some(name) = filename else {
        return declared.to_string();
    };
    let ext = std::path::Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase);
    let Some(ext) = ext else {
        return declared.to_string();
    };
    match ext.as_str() {
        "pdf" => "application/pdf",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "tif" | "tiff" => "image/tiff",
        "svg" => "image/svg+xml",
        "heic" | "heif" => "image/heic",
        "bmp" => "image/bmp",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" | "oga" => "audio/ogg",
        "mp4" | "m4v" => "video/mp4",
        "webm" => "video/webm",
        "mpeg" | "mpg" => "video/mpeg",
        "txt" | "log" | "md" => "text/plain",
        "csv" => "text/csv",
        "json" => "application/json",
        "xml" => "application/xml",
        "zip" => "application/zip",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        "doc" => "application/msword",
        "xls" => "application/vnd.ms-excel",
        "ppt" => "application/vnd.ms-powerpoint",
        "rtf" => "application/rtf",
        "odt" => "application/vnd.oasis.opendocument.text",
        "ods" => "application/vnd.oasis.opendocument.spreadsheet",
        _ => return declared.to_string(),
    }
    .to_string()
}

/// Extracted attachment bytes ready to hand to the browser. Kept
/// separate from `AttachmentMeta` so the forensics payload stays
/// small — we only copy the bytes out when the user actually asks
/// for one.
#[derive(Debug, Clone)]
pub struct ExtractedAttachment {
    pub filename: Option<String>,
    pub content_type: String,
    pub bytes: Vec<u8>,
}

/// Pull attachment number `index` out of a raw RFC822 message.
/// Indexing matches the order attachments appear in the `Forensics`
/// struct — the UI carries the index back verbatim, no name matching.
///
/// Only binary parts (or parts with a `Content-Disposition: attachment`
/// filename) count; text bodies, HTML alternatives, and decorative
/// inline images in signatures are skipped so the index space stays
/// stable with what the user sees in the attachment list.
pub fn extract_attachment(raw: &[u8], index: usize) -> Option<ExtractedAttachment> {
    use mail_parser::{MessageParser, PartType};
    let parser = MessageParser::default();
    let msg = parser.parse(raw)?;

    let mut seen = 0usize;
    for part in &msg.parts {
        let is_attachment = matches!(part.body, PartType::Binary(_) | PartType::InlineBinary(_))
            || part.attachment_name().is_some();
        if !is_attachment {
            continue;
        }
        if seen != index {
            seen += 1;
            continue;
        }

        let declared_type = part
            .content_type()
            .map(|ct| {
                let sub = ct
                    .c_subtype
                    .as_ref()
                    .map_or("", std::convert::AsRef::as_ref);
                if sub.is_empty() {
                    ct.c_type.to_string()
                } else {
                    format!("{}/{}", ct.c_type, sub)
                }
            })
            .unwrap_or_else(|| "application/octet-stream".into());
        let filename = part.attachment_name().map(str::to_owned);
        // Same normalisation the forensics pass applies — keeps the
        // Content-Type header we serve consistent with the whitelist
        // check and the filename the user sees in the UI.
        let content_type = sniff_type(&declared_type, filename.as_deref());

        let bytes = match &part.body {
            PartType::Binary(b) | PartType::InlineBinary(b) => b.to_vec(),
            PartType::Text(t) => t.as_bytes().to_vec(),
            PartType::Html(h) => h.as_bytes().to_vec(),
            _ => Vec::new(),
        };

        return Some(ExtractedAttachment {
            filename,
            content_type,
            bytes,
        });
    }
    None
}
