//! Outbound email — SMTP send + optional PGP encrypt + Sent-folder APPEND.
//!
//! Matches the IMAP side's design: sync lettre + imap crates driven from
//! tokio via spawn_blocking, reusing per-account app-password credentials
//! already in the secrets table. VPN binding for SMTP egress is a
//! follow-up — for now SMTP goes out the default route.

use lettre::message::{
    header::{ContentDisposition, ContentType},
    Attachment, Mailbox, Message, MessageBuilder, MultiPart, SinglePart,
};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::{
    error::{Error, Result},
    smtp,
    storage::{Account, AccountKind, Db},
    vault::Vault,
    vpn::VpnManager,
};

/// Incoming payload from `POST /api/send`. Also serialized into the
/// outbox row so the background worker can reconstruct the request
/// verbatim at dispatch time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendRequest {
    pub account_id: i64,
    pub to: Vec<String>,
    #[serde(default)]
    pub cc: Vec<String>,
    #[serde(default)]
    pub bcc: Vec<String>,
    pub subject: String,
    /// Plain-text body. For v1, HTML bodies are opt-in via `body_html`.
    pub body: String,
    #[serde(default)]
    pub body_html: Option<String>,
    #[serde(default)]
    pub attachments: Vec<SendAttachment>,
    #[serde(default)]
    pub in_reply_to: Option<String>,
    #[serde(default)]
    pub references: Option<String>,
    /// When true and all recipients have public keys in the PGP keyring,
    /// encrypt the body before sending. When false, always send plain.
    #[serde(default)]
    pub pgp_encrypt: bool,
    /// When true, attach `Disposition-Notification-To: <from-address>`
    /// so receiving clients are prompted to send a read receipt.
    /// Postern itself never auto-sends MDNs back; the receiver's
    /// client decides whether to honour the request.
    #[serde(default)]
    pub request_receipt: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendAttachment {
    pub filename: String,
    pub content_type: String,
    /// Base64-encoded attachment bytes.
    pub data_base64: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SendReport {
    pub ok: bool,
    pub message_id: String,
    pub encrypted: bool,
    pub appended_to_sent: bool,
    pub details: Option<String>,
    /// Forensics shown in the compose success card. All best-effort —
    /// Postern's SMTP is sync/blocking so these values are captured at
    /// call time. Not all fields populate on every send (e.g. VPN
    /// metadata is empty when the tunnel is down).
    pub forensics: SendForensics,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct SendForensics {
    /// UTC epoch seconds at dispatch.
    pub sent_at_utc: i64,
    /// SMTP host + port we handed the message to.
    pub smtp_host: String,
    pub smtp_port: u16,
    /// Number of envelope recipients (To + Cc + Bcc).
    pub recipient_count: usize,
    /// Size of the raw message bytes handed to SMTP.
    pub raw_size_bytes: usize,
    /// `"wg0"` when SMTP was routed through the tunnel, null otherwise.
    pub bind_iface: Option<String>,
    /// VPN snapshot at dispatch — included even when the tunnel was
    /// *not* up, so the forensics card can explicitly say "VPN off".
    pub vpn_enabled: bool,
    pub vpn_interface_up: bool,
    pub vpn_exit_ip: Option<String>,
    pub vpn_provider: Option<String>,
    pub vpn_region_label: Option<String>,
    pub vpn_server_country_code: Option<String>,
    pub vpn_server_city: Option<String>,
    pub vpn_server_number: Option<u32>,
    pub killswitch_enabled: bool,
    /// Autocrypt header was attached to the outer envelope.
    pub autocrypt_attached: bool,
    /// Name of the folder the Sent copy landed in (or a description
    /// when the server auto-files, e.g. Gmail). `None` when the
    /// APPEND wasn't even attempted (pre-flight error).
    pub sent_folder: Option<String>,
}

/// Top-level entry. Blocking (our custom smtp + imap are sync). Callers
/// should wrap in `tokio::task::spawn_blocking`.
pub fn send_blocking(
    db: &Db,
    vpn: &VpnManager,
    vault: &Vault,
    req: SendRequest,
) -> Result<SendReport> {
    if req.to.is_empty() {
        return Err(Error::BadRequest(
            "at least one recipient (To) required".into(),
        ));
    }
    vault.require_unlocked()?;
    let account = db.get_account(req.account_id)?;
    if !account.send_enabled {
        return Err(Error::BadRequest(format!(
            "sending is disabled for {}. Re-enable in Settings → Mailboxes.",
            account.email
        )));
    }
    let password = db.account_password(account.id, vault)?;

    let (smtp_host, smtp_port) = resolve_smtp(&account)?;
    let from_mbox = parse_mailbox(&format!(
        "{} <{}>",
        account
            .display_name
            .clone()
            .unwrap_or_else(|| account.email.clone()),
        account.email
    ))?;

    // PGP encrypt when the user asked AND every recipient has a public
    // key in our keyring. Missing keys surface as a 400 so the UI can
    // tell the user exactly who is unreachable rather than silently
    // sending plain.
    let encrypted = req.pgp_encrypt;
    if encrypted && !all_recipients_have_keys(db, &req) {
        return Err(Error::BadRequest(
            "PGP encrypt requested but not every recipient has a public key in the keyring".into(),
        ));
    }

    let message_id = format!(
        "<{}@{}>",
        uuid::Uuid::new_v4(),
        hostname_from(&account.email)
    );

    let email = build_message(db, vault, &from_mbox, &req, &message_id, encrypted)?;
    let mut raw = email.formatted();
    // Read-receipt request. The recipient's client decides whether
    // to honour `Disposition-Notification-To`; we just declare the
    // address it should reply to (always the sender). Injected via
    // raw header rewrite — lettre has no typed support for this
    // header, and `inject_header` is already proven on the Autocrypt
    // path below.
    if req.request_receipt {
        let header_line = format!("Disposition-Notification-To: <{}>", account.email);
        raw = inject_header(&raw, &header_line);
    }
    // Teach every recipient what our public key looks like. If we
    // have a keypair for this from-address in the local keyring, we
    // attach an Autocrypt header to the OUTER envelope (even on
    // encrypted sends) so Autocrypt-aware clients auto-import on
    // receipt. Silently skip if there's no key — not every user
    // wants PGP, and a missing Autocrypt header is the correct
    // signal for "I'm not inviting encryption yet".
    let mut autocrypt_attached = false;
    if let Ok(Some(our_pub)) = db.pgp_public_armored_for_email(&account.email) {
        if let Some(header) = crate::pgp::build_autocrypt_header(&account.email, &our_pub) {
            raw = inject_header(&raw, &header);
            autocrypt_attached = true;
        }
    }
    let rcpts: Vec<String> = all_addresses(&req);
    let envelope_from = extract_email(&account.email);

    // Pin SMTP to wg0 when the VPN is up, just like IMAP — otherwise
    // the kill-switch drops the connection (ENETUNREACH). When VPN is
    // down we fall back to the default route.
    let bind_iface = vpn.bind_iface();
    if let Err(e) = smtp::send(
        smtp_host,
        smtp_port,
        &account.email,
        &password,
        bind_iface.as_deref(),
        &envelope_from,
        &rcpts,
        &raw,
    ) {
        let detail = format!("{} → {}: {}", account.email, rcpts.join(","), e);
        let _ = db.log_activity("smtp_error", Some(&detail));
        return Err(e);
    }

    info!(account = %account.email, to = ?req.to, bind = ?bind_iface.as_deref(), encrypted, "email sent");
    let detail = format!(
        "{} → {}{}",
        account.email,
        rcpts.join(","),
        if encrypted { " (pgp)" } else { "" }
    );
    let _ = db.log_activity("smtp_send", Some(&detail));

    let sent_outcome = append_to_sent(&account, &password, &raw, bind_iface.as_deref()).unwrap_or_else(|e| {
        // APPEND failure is not fatal — non-Gmail users can still
        // resend or manually file, and we don't want to hide a
        // successful SMTP send behind a secondary IMAP glitch.
        warn!(error = %e, "sent-folder APPEND failed (non-fatal)");
        SentAppendOutcome {
            appended: false,
            folder: None,
        }
    });
    let appended_to_sent = sent_outcome.appended;
    let sent_folder = sent_outcome.folder.clone();

    let vpn_status = vpn.status();
    let forensics = SendForensics {
        sent_at_utc: chrono::Utc::now().timestamp(),
        smtp_host: smtp_host.to_string(),
        smtp_port,
        recipient_count: rcpts.len(),
        raw_size_bytes: raw.len(),
        bind_iface: bind_iface.clone(),
        vpn_enabled: vpn_status.enabled,
        vpn_interface_up: vpn_status.interface_up,
        vpn_exit_ip: vpn_status.exit_ip.clone(),
        vpn_provider: vpn_status.provider.clone(),
        vpn_region_label: vpn_status.region_label.clone(),
        vpn_server_country_code: vpn_status.server_country_code.clone(),
        vpn_server_city: vpn_status.server_city.clone(),
        vpn_server_number: vpn_status.server_number,
        killswitch_enabled: vpn_status.killswitch_enabled,
        autocrypt_attached,
        sent_folder,
    };

    Ok(SendReport {
        ok: true,
        message_id,
        encrypted,
        appended_to_sent,
        details: None,
        forensics,
    })
}

fn resolve_smtp(account: &Account) -> Result<(&str, u16)> {
    let host = account.smtp_host.as_deref().ok_or_else(|| {
        Error::BadRequest(format!("no smtp host configured for {}", account.email))
    })?;
    let port = account.smtp_port.unwrap_or(587);
    Ok((host, port))
}

fn hostname_from(email: &str) -> &str {
    email
        .rsplit_once('@')
        .map(|(_, d)| d)
        .unwrap_or("localhost")
}

fn parse_mailbox(spec: &str) -> Result<Mailbox> {
    spec.parse::<Mailbox>()
        .map_err(|e| Error::BadRequest(format!("invalid mailbox {spec}: {e}")))
}

fn build_message(
    db: &Db,
    vault: &Vault,
    from: &Mailbox,
    req: &SendRequest,
    message_id: &str,
    encrypted: bool,
) -> Result<Message> {
    let builder = envelope_builder(from, req, message_id)?;

    if encrypted {
        // RFC 3156: build the plaintext MIME body first (incl. attachments
        // and alt-parts), render it to bytes, encrypt to every recipient,
        // then wrap the ciphertext in a multipart/encrypted envelope
        // whose outer headers stay in the clear.
        let inner_part = build_body(&req.body, req.body_html.as_deref(), &req.attachments)?;
        // lettre needs a full Message to serialize — build a header-less
        // inner message, then strip the synthetic top headers lettre adds.
        let inner_msg = Message::builder()
            .from(from.clone())
            .to(parse_mailbox(&req.to[0])?)
            .subject("inner")
            .multipart(inner_part)
            .map_err(|e| Error::Other(anyhow::anyhow!("inner build: {e}")))?;
        let rendered = inner_msg.formatted();
        let mime_entity = strip_outer_envelope(&rendered);

        let rcpts = all_addresses(req);
        let armored = crate::pgp::encrypt_for_recipients(db, vault, &rcpts, &mime_entity)?;

        let outer = pgp_mime_envelope(armored);
        builder
            .multipart(outer)
            .map_err(|e| Error::Other(anyhow::anyhow!("build encrypted: {e}")))
    } else {
        let inner = build_body(&req.body, req.body_html.as_deref(), &req.attachments)?;
        builder
            .multipart(inner)
            .map_err(|e| Error::Other(anyhow::anyhow!("build message: {e}")))
    }
}

/// All the From/To/Cc/Bcc/Subject/Message-ID/threading headers — shared
/// between the plaintext and PGP-encrypted paths so the outer envelope
/// of an encrypted mail still carries correct routing info.
fn envelope_builder(from: &Mailbox, req: &SendRequest, message_id: &str) -> Result<MessageBuilder> {
    let mut builder = Message::builder()
        .from(from.clone())
        .subject(req.subject.clone());
    for t in &req.to {
        builder = builder.to(parse_mailbox(t)?);
    }
    for c in &req.cc {
        builder = builder.cc(parse_mailbox(c)?);
    }
    for b in &req.bcc {
        builder = builder.bcc(parse_mailbox(b)?);
    }
    if let Some(irt) = &req.in_reply_to {
        builder = builder.in_reply_to(irt.clone());
    }
    if let Some(refs) = &req.references {
        builder = builder.references(refs.clone());
    }
    Ok(builder.message_id(Some(message_id.to_string())))
}

/// Insert a new header line (must already be in "Name: value" form,
/// may contain folded continuation lines with leading whitespace) at
/// the top of a formatted message, before the existing headers.
/// Splits at the first CRLF-CRLF header/body separator and reassembles
/// with the new header prepended. Used to inject the Autocrypt
/// header — keeping it high in the block makes it easier to find for
/// tooling that doesn't parse the whole envelope.
fn inject_header(raw: &[u8], header_line: &str) -> Vec<u8> {
    let separator = b"\r\n\r\n";
    let Some(sep_pos) = raw.windows(separator.len()).position(|w| w == separator) else {
        // No header terminator — safe fallback is to return the
        // original unchanged rather than corrupt a malformed message.
        return raw.to_vec();
    };
    let (headers, body_with_sep) = raw.split_at(sep_pos);
    let mut out = Vec::with_capacity(raw.len() + header_line.len() + 2);
    out.extend_from_slice(header_line.as_bytes());
    out.extend_from_slice(b"\r\n");
    out.extend_from_slice(headers);
    out.extend_from_slice(body_with_sep);
    out
}

/// Given a fully-formatted lettre message, return just the MIME entity
/// that goes inside the PGP ciphertext — i.e. the `Content-Type: …`
/// header plus the body, without any of the user-visible routing
/// headers (From/To/Subject). RFC 3156 says the encrypted part is a
/// MIME entity, not a full message.
fn strip_outer_envelope(raw: &[u8]) -> Vec<u8> {
    // Find the first CRLF CRLF separator between headers and body.
    let header_end = raw.windows(4).position(|w| w == b"\r\n\r\n").unwrap_or(0);
    let (headers, body) = raw.split_at(header_end);

    // Keep only headers that describe the MIME entity itself. Everything
    // else (From/To/Subject/Date/Message-ID) belongs on the outer envelope.
    let mut kept: Vec<&[u8]> = Vec::new();
    for line in split_unfolded_headers(headers) {
        let lower = line.to_ascii_lowercase();
        if lower.starts_with(b"content-type:")
            || lower.starts_with(b"content-transfer-encoding:")
            || lower.starts_with(b"mime-version:")
        {
            kept.push(line);
        }
    }
    let mut out = Vec::with_capacity(raw.len());
    for line in kept {
        out.extend_from_slice(line);
        out.extend_from_slice(b"\r\n");
    }
    out.extend_from_slice(b"\r\n");
    // Skip the "\r\n\r\n" separator we split on.
    if body.len() >= 4 {
        out.extend_from_slice(&body[4..]);
    }
    out
}

/// Split a header block on CRLF while respecting RFC 5322 folded
/// continuations (leading whitespace on a wrapped line).
fn split_unfolded_headers(block: &[u8]) -> Vec<&[u8]> {
    let mut out: Vec<&[u8]> = Vec::new();
    let mut start = 0usize;
    let mut i = 0usize;
    while i < block.len() {
        // Find CRLF.
        if block[i] == b'\r' && i + 1 < block.len() && block[i + 1] == b'\n' {
            // Peek: if the next byte starts with whitespace, the header
            // continues (folded). Keep scanning.
            let next = i + 2;
            if next < block.len() && (block[next] == b' ' || block[next] == b'\t') {
                i = next;
                continue;
            }
            out.push(&block[start..i]);
            start = next;
            i = next;
        } else {
            i += 1;
        }
    }
    if start < block.len() {
        out.push(&block[start..]);
    }
    out
}

/// RFC 3156 multipart/encrypted wrapper: a Version part + the ciphertext
/// part, with the outer Content-Type parameters indicating OpenPGP.
fn pgp_mime_envelope(armored_cipher: String) -> MultiPart {
    let version = SinglePart::builder()
        .header(ContentType::parse("application/pgp-encrypted").unwrap())
        .header(ContentDisposition::attachment("Version: 1.txt"))
        .body(String::from("Version: 1\r\n"));
    let cipher = SinglePart::builder()
        .header(ContentType::parse("application/octet-stream").unwrap())
        .header(ContentDisposition::attachment("encrypted.asc"))
        .body(armored_cipher);
    MultiPart::encrypted(String::from("application/pgp-encrypted"))
        .singlepart(version)
        .singlepart(cipher)
}

fn build_body(
    plain: &str,
    html: Option<&str>,
    attachments: &[SendAttachment],
) -> Result<MultiPart> {
    let text_part = if let Some(h) = html {
        MultiPart::alternative()
            .singlepart(SinglePart::plain(plain.to_string()))
            .singlepart(SinglePart::html(h.to_string()))
    } else {
        MultiPart::alternative().singlepart(SinglePart::plain(plain.to_string()))
    };

    if attachments.is_empty() {
        return Ok(text_part);
    }

    let mut mixed = MultiPart::mixed().multipart(text_part);
    for a in attachments {
        let bytes =
            base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &a.data_base64)
                .map_err(|e| Error::BadRequest(format!("bad attachment base64: {e}")))?;
        let ct: ContentType = a
            .content_type
            .parse()
            .map_err(|e| Error::BadRequest(format!("bad content type {}: {e}", a.content_type)))?;
        mixed = mixed.singlepart(Attachment::new(a.filename.clone()).body(bytes, ct));
        // (`Attachment::new` already sets a sensible Content-Disposition;
        // no need to set it again.)
    }
    Ok(mixed)
}

fn all_addresses(req: &SendRequest) -> Vec<String> {
    req.to
        .iter()
        .chain(req.cc.iter())
        .chain(req.bcc.iter())
        .map(|s| extract_email(s))
        .collect()
}

fn extract_email(addr: &str) -> String {
    // "Name <user@host>" -> "user@host"; plain "user@host" stays as-is.
    if let (Some(lt), Some(gt)) = (addr.find('<'), addr.rfind('>')) {
        if lt < gt {
            return addr[lt + 1..gt].trim().to_string();
        }
    }
    addr.trim().to_string()
}

fn all_recipients_have_keys(db: &Db, req: &SendRequest) -> bool {
    for addr in all_addresses(req) {
        match db.pgp_find_by_email(&addr) {
            Ok(Some(_)) => {}
            _ => return false,
        }
    }
    !req.to.is_empty()
}

/// Manually send a read-receipt MDN (RFC 8098) for a previously
/// received message. The original message must carry a non-null
/// `receipt_to`; we never auto-derive one from the From line. The
/// MDN is a `multipart/report` with a human-readable text part plus
/// the machine-parseable `message/disposition-notification` part.
///
/// Returns the address the receipt was sent to. Errors out if the
/// message has no receipt request, the account is missing/disabled,
/// or SMTP refuses the send.
pub fn send_read_receipt_blocking(
    db: &Db,
    vpn: &VpnManager,
    vault: &Vault,
    message_id: i64,
) -> Result<String> {
    vault.require_unlocked()?;
    let detail = db.get_message_detail(message_id)?;
    let receipt_to = detail.message.receipt_to.clone().ok_or_else(|| {
        Error::BadRequest("message did not request a read receipt".into())
    })?;
    let original_msg_id = detail.message.message_id.clone();
    let original_subject = detail.message.subject.clone().unwrap_or_default();
    let account = db.get_account(detail.message.account_id)?;
    if !account.send_enabled {
        return Err(Error::BadRequest(format!(
            "sending is disabled for {}. Re-enable in Settings → Mailboxes.",
            account.email
        )));
    }
    let password = db.account_password(account.id, vault)?;
    let (smtp_host, smtp_port) = resolve_smtp(&account)?;

    let mdn_msg_id = format!(
        "<{}@{}>",
        uuid::Uuid::new_v4(),
        hostname_from(&account.email)
    );
    let raw = build_mdn_message(
        &account.email,
        account.display_name.as_deref(),
        &receipt_to,
        &original_msg_id,
        &original_subject,
        &mdn_msg_id,
    );

    let bind_iface = vpn.bind_iface();
    let envelope_from = extract_email(&account.email);
    let rcpts = vec![extract_email(&receipt_to)];
    crate::smtp::send(
        smtp_host,
        smtp_port,
        &account.email,
        &password,
        bind_iface.as_deref(),
        &envelope_from,
        &rcpts,
        &raw,
    )?;
    let _ = db.log_activity(
        "read_receipt_sent",
        Some(&format!("{} → {}", account.email, receipt_to)),
    );
    info!(account = %account.email, to = %receipt_to, "read receipt MDN dispatched");
    Ok(receipt_to)
}

/// Construct an RFC 8098 MDN. Built as raw CRLF-delimited bytes
/// because lettre has no typed helper for `message/disposition-
/// notification` and the multipart envelope is small enough to
/// hand-write without losing readability.
fn build_mdn_message(
    from_addr: &str,
    from_name: Option<&str>,
    to_addr: &str,
    original_message_id: &str,
    original_subject: &str,
    mdn_message_id: &str,
) -> Vec<u8> {
    let date = chrono::Utc::now()
        .format("%a, %d %b %Y %H:%M:%S +0000")
        .to_string();
    let from_header = match from_name {
        Some(name) if !name.trim().is_empty() => format!("{name} <{from_addr}>"),
        _ => from_addr.to_owned(),
    };
    let boundary = format!("postern-mdn-{}", uuid::Uuid::new_v4().simple());
    let from_email = extract_email(from_addr);

    let mut out = String::with_capacity(2048);
    use std::fmt::Write;
    let _ = writeln!(out, "From: {from_header}\r");
    let _ = writeln!(out, "To: {to_addr}\r");
    let _ = writeln!(out, "Subject: Read: {original_subject}\r");
    let _ = writeln!(out, "Date: {date}\r");
    let _ = writeln!(out, "Message-ID: {mdn_message_id}\r");
    let _ = writeln!(out, "MIME-Version: 1.0\r");
    let _ = writeln!(
        out,
        "Content-Type: multipart/report; report-type=disposition-notification; boundary=\"{boundary}\"\r"
    );
    let _ = writeln!(out, "\r");
    let _ = writeln!(out, "--{boundary}\r");
    let _ = writeln!(out, "Content-Type: text/plain; charset=utf-8\r");
    let _ = writeln!(out, "Content-Transfer-Encoding: 7bit\r");
    let _ = writeln!(out, "\r");
    let _ = writeln!(
        out,
        "Your message titled \"{original_subject}\" was displayed by the recipient on {date}.\r"
    );
    let _ = writeln!(out, "\r");
    let _ = writeln!(out, "--{boundary}\r");
    let _ = writeln!(
        out,
        "Content-Type: message/disposition-notification\r"
    );
    let _ = writeln!(out, "\r");
    let _ = writeln!(out, "Reporting-UA: Postern; Postern Mail Client\r");
    let _ = writeln!(out, "Original-Recipient: rfc822;{from_email}\r");
    let _ = writeln!(out, "Final-Recipient: rfc822;{from_email}\r");
    let _ = writeln!(out, "Original-Message-ID: {original_message_id}\r");
    let _ = writeln!(
        out,
        "Disposition: manual-action/MDN-sent-manually; displayed\r"
    );
    let _ = writeln!(out, "\r");
    let _ = writeln!(out, "--{boundary}--\r");
    out.into_bytes()
}

/// APPEND the sent message to the account's Sent folder via IMAP. Gmail
/// auto-files sent mail on its side, so for Gmail this is a no-op. For
/// conventional IMAP, we append to "Sent" (the standard IMAP folder
/// name). Returns Ok(true) if the message landed.
/// Outcome of the Sent-folder APPEND. The folder name is reported
/// back so the send's forensics card can show the user exactly where
/// the copy ended up (or why it didn't happen).
pub struct SentAppendOutcome {
    pub appended: bool,
    pub folder: Option<String>,
}

fn append_to_sent(
    account: &Account,
    password: &str,
    raw: &[u8],
    bind_iface: Option<&str>,
) -> Result<SentAppendOutcome> {
    info!(account = %account.email, kind = ?account.kind, "append_to_sent: entering");
    if matches!(account.kind, AccountKind::Gmail) {
        // Gmail auto-files sent messages via SMTP. Manual APPEND would
        // just duplicate the message in [Gmail]/All Mail.
        return Ok(SentAppendOutcome {
            appended: false,
            folder: Some("[Gmail]/Sent Mail (auto-filed by Gmail)".to_owned()),
        });
    }
    info!(host = %account.imap_host, port = account.imap_port, "append_to_sent: connecting IMAP");
    let mut client = crate::sync::ImapClient::connect(
        &account.imap_host,
        account.imap_port,
        &account.email,
        password,
        bind_iface.as_deref(),
    )?;
    info!(
        prefix = %client.namespace().prefix,
        delim = %client.namespace().delimiter,
        "append_to_sent: resolving Sent role"
    );

    // Ask the server itself where the Sent folder lives — \Sent
    // SPECIAL-USE first, heuristic walk second. Works on Gmail,
    // Dovecot (INBOX.Sent), Exchange (Sent Items), Fastmail, etc.
    let resolved = client
        .resolve_role_folder(crate::sync::FolderRole::Sent)
        .unwrap_or_else(|e| {
            warn!(error = %e, "resolve Sent role failed");
            None
        });

    let target = match resolved {
        Some(name) => {
            info!(account = %account.email, folder = %name, "append_to_sent: resolved Sent");
            name
        }
        None => {
            // Nothing matches. Create a sensible one under the server's
            // advertised personal namespace — `INBOX.Sent` on Dovecot,
            // `Sent` on a flat Maildir server — so the next send lands
            // somewhere predictable.
            let ns = client.namespace();
            let created = if ns.prefix.is_empty() {
                "Sent".to_owned()
            } else {
                format!("{}{}{}", ns.prefix, ns.delimiter, "Sent")
            };
            if let Err(e) = client.ensure_folder(&created) {
                warn!(error = %e, folder = %created, "append_to_sent: CREATE fallback failed");
            }
            info!(account = %account.email, folder = %created, "append_to_sent: no existing Sent, using namespace default");
            created
        }
    };

    let ok = match client.append_raw(&target, raw) {
        Ok(()) => true,
        Err(e) => {
            warn!(
                account = %account.email,
                folder = %target,
                error = %e,
                "append_to_sent: APPEND failed"
            );
            false
        }
    };
    client.logout();
    Ok(SentAppendOutcome {
        appended: ok,
        folder: Some(target),
    })
}
