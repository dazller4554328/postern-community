//! Standalone raw IMAP client used only for the Gmail label rescan.
//!
//! The `imap` 2.4 crate routes every byte from the server through
//! imap-proto's parser. imap-proto 0.10 has no branch for
//! `X-GM-LABELS`, so any FETCH response carrying that attribute fails
//! `msg_att` parsing and the whole read is surfaced as
//! "Unable to parse status response". Neither `run_command_and_read_response`
//! nor `uid_fetch` is usable here.
//!
//! So we open our own TLS socket, speak IMAP by hand for the three
//! commands the rescan actually needs (LOGIN, EXAMINE, UID FETCH, plus
//! LOGOUT for manners), handle literal continuations ourselves, and
//! feed the raw FETCH-block bytes into the existing gmail_labels
//! parsers. No imap-proto, no tag drift.

use std::{
    io::{Read, Write},
    net::TcpStream,
    time::Duration,
};

use native_tls::{TlsConnector, TlsStream};
use tracing::{debug, info, warn};

use crate::{
    error::{Error, Result},
    storage::{Account, Db},
    sync::gmail_labels,
};

const RESCAN_BATCH: u32 = 500;
const READ_CHUNK: usize = 8192;

/// Open a TLS socket, read the untagged greeting, and send LOGIN.
/// Returns the authenticated stream ready for command issuance.
/// Both the rescan and the category-purge entry points wrap this —
/// keeping the TCP-timeout + TLS-handshake boilerplate in one place.
///
/// The caller is expected to drive its own tag counter afterwards
/// (starting from 2, since LOGIN used `a1`). That keeps this helper
/// agnostic about how the rest of the session wants to be structured.
pub(crate) fn connect_and_login(
    host: &str,
    port: u16,
    email: &str,
    password: &str,
    bind_iface: Option<&str>,
) -> Result<TlsStream<TcpStream>> {
    let tcp = crate::net::open_tcp(host, port, bind_iface)?;
    tcp.set_read_timeout(Some(Duration::from_secs(300))).ok();
    tcp.set_write_timeout(Some(Duration::from_secs(60))).ok();
    let tls_conn = TlsConnector::builder()
        .build()
        .map_err(|e| Error::Imap(format!("tls init: {e}")))?;
    let mut stream: TlsStream<TcpStream> = tls_conn
        .connect(host, tcp)
        .map_err(|e| Error::Imap(format!("tls handshake: {e}")))?;

    read_response(&mut stream, None)?;

    write_line(
        &mut stream,
        &format!("a1 LOGIN {} {}", quote(email), quote(password)),
    )?;
    expect_ok(&mut stream, "a1", "LOGIN")?;

    Ok(stream)
}

pub fn rescan(
    host: &str,
    port: u16,
    email: &str,
    password: &str,
    account: &Account,
    db: &Db,
    bind_iface: Option<&str>,
) -> Result<(u32, u32)> {
    let mut stream = connect_and_login(host, port, email, password, bind_iface)?;

    // LOGIN consumed `a1`, so the session-local tag counter continues
    // from `a2`.
    let mut tag_n: u32 = 1;
    let mut next_tag = || {
        tag_n += 1;
        format!("a{tag_n}")
    };

    // EXAMINE "[Gmail]/All Mail".
    let tag = next_tag();
    write_line(&mut stream, &format!("{tag} EXAMINE \"[Gmail]/All Mail\""))?;
    let examine_resp = read_response(&mut stream, Some(&tag))?;
    let uid_next = parse_uid_next(&examine_resp).unwrap_or(1);
    if uid_next <= 1 {
        logout(&mut stream, &next_tag());
        return Ok((0, 0));
    }
    info!(email, uid_next, "rescan: examined All Mail");

    let mut scanned = 0u32;
    let mut updated = 0u32;
    let mut cursor = 1u32;
    while cursor < uid_next {
        let end = cursor.saturating_add(RESCAN_BATCH - 1).min(uid_next - 1);
        let range = format!("{cursor}:{end}");
        let tag = next_tag();
        write_line(
            &mut stream,
            &format!(
                "{tag} UID FETCH {range} (UID X-GM-LABELS BODY.PEEK[HEADER.FIELDS (MESSAGE-ID)])"
            ),
        )?;
        let bytes = match read_response(&mut stream, Some(&tag)) {
            Ok(b) => b,
            Err(e) => {
                debug!(error = %e, %range, "rescan batch fetch failed, skipping");
                cursor = end.saturating_add(1);
                continue;
            }
        };

        // One-shot raw dump of the first non-empty batch so we can see
        // what Gmail is actually sending before any parsing. Capped at
        // 4 KiB to keep the log readable.
        static DUMPED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        if !bytes.is_empty() && !DUMPED.swap(true, std::sync::atomic::Ordering::Relaxed) {
            let preview: String = bytes
                .iter()
                .take(4096)
                .map(|&b| {
                    if b == b'\r' {
                        '␍'
                    } else if b == b'\n' {
                        '␊'
                    } else if (32..127).contains(&b) {
                        b as char
                    } else {
                        '·'
                    }
                })
                .collect();
            info!(
                bytes_len = bytes.len(),
                preview = %preview,
                "rescan: raw batch dump"
            );
        }

        let mut sample_logged = false;
        let mut batch_empty = 0u32;
        let mut batch_labelled = 0u32;
        for block in gmail_labels::split_fetch_blocks(&bytes) {
            scanned += 1;

            let message_id_opt = gmail_labels::extract_message_id(&block);
            let raw_labels = gmail_labels::extract_labels(&block);

            if raw_labels.is_empty() {
                batch_empty += 1;
            } else {
                batch_labelled += 1;
            }

            // Log the very first block per batch *with* non-empty
            // labels. If every sample shows empty across the whole
            // scan, labels aren't reaching us from Gmail; if even one
            // shows non-empty, the parser works and specific messages
            // just happen to be unlabelled.
            if !sample_logged && !raw_labels.is_empty() {
                sample_logged = true;
                let preview: String = block
                    .iter()
                    .take(500)
                    .map(|&b| {
                        if b == b'\r' {
                            '␍'
                        } else if b == b'\n' {
                            '␊'
                        } else {
                            b as char
                        }
                    })
                    .collect();
                info!(
                    preview = %preview,
                    mid = ?message_id_opt,
                    labels = ?raw_labels,
                    "rescan: block with labels"
                );
            }

            let Some(message_id) = message_id_opt else {
                continue;
            };
            if raw_labels.is_empty() {
                continue;
            }
            let translated: Vec<String> = raw_labels
                .iter()
                .filter_map(|raw| gmail_labels::normalise_label(raw))
                .collect();
            if translated.is_empty() {
                continue;
            }
            for name in &translated {
                let kind = gmail_labels::kind_for_label(name);
                let _ = db.upsert_label(account.id, name, kind);
            }
            match db.add_labels_by_message_id(account.id, &message_id, &translated) {
                Ok(true) => updated += 1,
                Ok(false) => {
                    // Message isn't in our local DB. Log once per
                    // rescan so we know if this is the dominant reason
                    // updated stays at 0.
                    debug!(mid = %message_id, "rescan: message not found locally");
                }
                Err(e) => debug!(mid = %message_id, error = %e, "rescan: label add errored"),
            }
        }

        if batch_empty + batch_labelled > 0 {
            info!(
                %range,
                batch_empty,
                batch_labelled,
                "rescan: batch summary"
            );
        }

        cursor = end.saturating_add(1);
    }

    // Categories (Updates / Promotions / Social / Forums / Purchases)
    // don't appear in X-GM-LABELS on accounts where Google has locked
    // the category out of IMAP — which is every account, by design.
    // `X-GM-RAW "category:X"` bypasses that by running a Gmail-web-
    // style search over IMAP: we get back the UIDs of every message
    // in that category even though the label itself is invisible.
    // Tag the locals with CATEGORY_X so the sidebar can surface them.
    let category_updates = paint_category(
        &mut stream,
        &mut next_tag,
        account,
        db,
        "category:updates",
        "CATEGORY_UPDATES",
    )?;
    let category_promotions = paint_category(
        &mut stream,
        &mut next_tag,
        account,
        db,
        "category:promotions",
        "CATEGORY_PROMOTIONS",
    )?;
    let category_social = paint_category(
        &mut stream,
        &mut next_tag,
        account,
        db,
        "category:social",
        "CATEGORY_SOCIAL",
    )?;
    let category_forums = paint_category(
        &mut stream,
        &mut next_tag,
        account,
        db,
        "category:forums",
        "CATEGORY_FORUMS",
    )?;
    let category_purchases = paint_category(
        &mut stream,
        &mut next_tag,
        account,
        db,
        "category:purchases",
        "CATEGORY_PURCHASES",
    )?;
    let category_total = category_updates
        + category_promotions
        + category_social
        + category_forums
        + category_purchases;

    logout(&mut stream, &next_tag());
    info!(
        email,
        scanned,
        updated,
        category_updates,
        category_promotions,
        category_social,
        category_forums,
        category_purchases,
        "rescan: complete"
    );
    Ok((scanned, updated + category_total))
}

/// Look up messages matching a Gmail search operator (e.g.
/// `category:updates`) via X-GM-RAW, then apply `label_name` to every
/// local message that matches by Message-ID. Returns how many local
/// rows were touched.
fn paint_category(
    stream: &mut TlsStream<TcpStream>,
    next_tag: &mut impl FnMut() -> String,
    account: &Account,
    db: &Db,
    search_term: &str,
    label_name: &str,
) -> Result<u32> {
    // Step 1 — SEARCH to find UIDs in the category.
    let tag = next_tag();
    write_line(
        stream,
        &format!("{tag} UID SEARCH X-GM-RAW {}", quote(search_term)),
    )?;
    let resp = read_response(stream, Some(&tag))?;
    let uids = parse_search_uids(&resp);
    if uids.is_empty() {
        debug!(%search_term, "rescan: category search returned zero uids");
        return Ok(0);
    }
    info!(%search_term, uid_count = uids.len(), "rescan: category search");

    // Step 2 — register the category label up front so subsequent
    // add_labels_by_message_id calls can resolve its id.
    let _ = db.upsert_label(
        account.id,
        label_name,
        gmail_labels::kind_for_label(label_name),
    );

    // Step 3 — resolve UID → Message-ID via batched FETCH, then paint
    // the label onto each local row. Big categories (e.g. 1,500
    // messages in Updates) need batching to keep the command line
    // under any server-side limits.
    let mut painted = 0u32;
    const UID_BATCH: usize = 200;
    for chunk in uids.chunks(UID_BATCH) {
        let uid_set = crate::sync::uid_set(chunk);
        let tag = next_tag();
        write_line(
            stream,
            &format!("{tag} UID FETCH {uid_set} (UID BODY.PEEK[HEADER.FIELDS (MESSAGE-ID)])"),
        )?;
        let bytes = read_response(stream, Some(&tag))?;
        for block in gmail_labels::split_fetch_blocks(&bytes) {
            let Some(mid) = gmail_labels::extract_message_id(&block) else {
                continue;
            };
            if let Ok(true) = db.add_labels_by_message_id(
                account.id,
                &mid,
                std::slice::from_ref(&label_name.to_owned()),
            ) {
                painted += 1;
            }
        }
    }
    Ok(painted)
}

/// Nuclear-option purge: after a normal sync, walk each of Gmail's
/// five hidden categories, download any message Postern doesn't yet
/// have locally, and MOVE every matched UID into [Gmail]/Trash so the
/// provider copy leaves every label and enters Gmail's 30-day purge
/// cycle. Returns (downloaded, moved).
///
/// Requires that the caller's account has both `delete_after_sync`
/// and `purge_gmail_categories` set — the scheduler enforces that
/// gate before calling in.
pub fn purge_categories(
    host: &str,
    port: u16,
    email: &str,
    password: &str,
    account: &Account,
    db: &Db,
    blobs: &crate::storage::BlobStore,
    vault: &crate::vault::Vault,
    bind_iface: Option<&str>,
) -> Result<(u32, u32)> {
    let mut stream = connect_and_login(host, port, email, password, bind_iface)?;

    // LOGIN consumed `a1`, session continues from `a2`.
    let mut tag_n: u32 = 1;
    let mut next_tag = || {
        tag_n += 1;
        format!("a{tag_n}")
    };

    // SELECT (not EXAMINE) because we need write access to MOVE.
    let tag = next_tag();
    write_line(&mut stream, &format!("{tag} SELECT \"[Gmail]/All Mail\""))?;
    expect_ok(&mut stream, &tag, "SELECT")?;

    let mut total_downloaded = 0u32;
    let mut total_moved = 0u32;
    for (search, label) in [
        ("category:updates", "CATEGORY_UPDATES"),
        ("category:promotions", "CATEGORY_PROMOTIONS"),
        ("category:social", "CATEGORY_SOCIAL"),
        ("category:forums", "CATEGORY_FORUMS"),
        ("category:purchases", "CATEGORY_PURCHASES"),
    ] {
        let (d, m) = purge_one_category(
            &mut stream,
            &mut next_tag,
            account,
            db,
            blobs,
            vault,
            search,
            label,
        )?;
        total_downloaded += d;
        total_moved += m;
    }

    // Optional: skip Gmail's 30-day trash lifecycle entirely.
    let expunged = if account.skip_gmail_trash {
        match empty_trash(&mut stream, &mut next_tag) {
            Ok(n) => n,
            Err(e) => {
                if is_throttled_error(&e) {
                    debug!(error = %e, "purge: empty-trash throttled this cycle");
                } else {
                    warn!(error = %e, "purge: empty-trash failed (leaving messages in Trash)");
                }
                0
            }
        }
    } else {
        0
    };

    logout(&mut stream, &next_tag());
    info!(
        email,
        downloaded = total_downloaded,
        moved = total_moved,
        expunged,
        "purge: complete"
    );
    Ok((total_downloaded, total_moved))
}

/// SELECT [Gmail]/Trash, flag every message there as \Deleted, then
/// EXPUNGE. This permanently removes everything currently in Trash —
/// including mail the user trashed manually via Gmail's web UI. The
/// UI makes that scope clear before the user enables it.
pub(crate) fn empty_trash(
    stream: &mut TlsStream<TcpStream>,
    next_tag: &mut impl FnMut() -> String,
) -> Result<u32> {
    let tag = next_tag();
    write_line(stream, &format!("{tag} SELECT \"[Gmail]/Trash\""))?;
    expect_ok(stream, &tag, "SELECT Trash")?;

    let tag = next_tag();
    write_line(stream, &format!("{tag} UID SEARCH ALL"))?;
    let resp = read_response(stream, Some(&tag))?;
    let uids = parse_search_uids(&resp);
    if uids.is_empty() {
        return Ok(0);
    }

    // One STORE + EXPUNGE for the whole set. Gmail handles thousands
    // of UIDs in a single UID SET without complaint.
    let uid_set = crate::sync::uid_set(&uids);
    let tag = next_tag();
    write_line(
        stream,
        &format!("{tag} UID STORE {uid_set} +FLAGS (\\Deleted)"),
    )?;
    expect_ok(stream, &tag, "STORE \\Deleted")?;

    let tag = next_tag();
    write_line(stream, &format!("{tag} EXPUNGE"))?;
    expect_ok(stream, &tag, "EXPUNGE")?;
    info!(count = uids.len(), "purge: emptied Trash");
    Ok(uids.len() as u32)
}

#[allow(clippy::too_many_arguments)]
fn purge_one_category(
    stream: &mut TlsStream<TcpStream>,
    next_tag: &mut impl FnMut() -> String,
    account: &Account,
    db: &Db,
    blobs: &crate::storage::BlobStore,
    vault: &crate::vault::Vault,
    search_term: &str,
    label_name: &str,
) -> Result<(u32, u32)> {
    // 1. Discover UIDs for this category.
    let tag = next_tag();
    write_line(
        stream,
        &format!("{tag} UID SEARCH X-GM-RAW {}", quote(search_term)),
    )?;
    let resp = read_response(stream, Some(&tag))?;
    let uids = parse_search_uids(&resp);
    if uids.is_empty() {
        return Ok((0, 0));
    }
    info!(%search_term, uid_count = uids.len(), "purge: category");

    let _ = db.upsert_label(
        account.id,
        label_name,
        gmail_labels::kind_for_label(label_name),
    );

    // 2. For each UID chunk, fetch full body + label mapping, upsert
    //    any new local rows, then MOVE the chunk to Trash.
    let mut downloaded = 0u32;
    let mut moved = 0u32;
    const CHUNK: usize = 200;
    for chunk in uids.chunks(CHUNK) {
        let uid_set = crate::sync::uid_set(chunk);

        // Full bodies for messages that may not exist locally.
        let tag = next_tag();
        write_line(
            stream,
            &format!("{tag} UID FETCH {uid_set} (UID INTERNALDATE RFC822.SIZE BODY.PEEK[] FLAGS)"),
        )?;
        let bytes = read_response(stream, Some(&tag))?;

        for block in gmail_labels::split_fetch_blocks(&bytes) {
            if let Some((nm, was_new)) =
                import_from_block(account, db, blobs, vault, label_name, &block)
            {
                if was_new {
                    downloaded += 1;
                }
                // If the message was already local, ensure the label
                // sticks anyway so the category folder populates.
                let _ = db.add_labels_by_message_id(
                    account.id,
                    &nm.message_id,
                    std::slice::from_ref(&label_name.to_owned()),
                );
            }
        }

        // 3. MOVE the entire chunk to Trash. This strips every label
        //    on the Gmail side in one shot.
        let tag = next_tag();
        write_line(
            stream,
            &format!("{tag} UID MOVE {uid_set} \"[Gmail]/Trash\""),
        )?;
        if let Err(e) = expect_ok(stream, &tag, "UID MOVE") {
            if is_throttled_error(&e) {
                // Gmail is rate-limiting this account. Skip the rest
                // of this purge cycle — the next sync tick retries.
                // Returning an error (vs. `continue`) stops us from
                // hammering the throttled socket with the remaining
                // categories, which just burns quota.
                debug!(error = %e, %search_term, "purge: Gmail throttled, aborting this cycle");
                return Err(e);
            }
            warn!(error = %e, %search_term, "purge: MOVE failed, leaving chunk on server");
            continue;
        }
        moved += chunk.len() as u32;
    }
    Ok((downloaded, moved))
}

/// Parse a single FETCH block with full body attributes and upsert it
/// into Postern. Returns the imported message + whether it was new.
fn import_from_block(
    account: &Account,
    db: &Db,
    blobs: &crate::storage::BlobStore,
    vault: &crate::vault::Vault,
    label_name: &str,
    block: &[u8],
) -> Option<(crate::storage::NewMessage, bool)> {
    let body = extract_body_literal(block)?;
    let parsed = crate::sync::parser::parse(body);
    let hash = blobs.put(body).ok()?;
    let is_read = block.windows(5).any(|w| w.eq_ignore_ascii_case(b"\\seen"));
    let is_encrypted = crate::sync::parser::is_pgp_encrypted(body);
    let nm = crate::sync::parser::into_new_message(
        account.id,
        parsed,
        hash,
        body.len(),
        vec![label_name.to_owned()],
        None,
        is_read,
        is_encrypted,
    );
    // Make sure the target label exists, then upsert the message.
    let _ = db.upsert_label(
        account.id,
        label_name,
        gmail_labels::kind_for_label(label_name),
    );
    match db.upsert_message(&nm) {
        Ok(is_new) => {
            if is_new {
                crate::pgp::harvest_autocrypt(body, db, vault);
            }
            Some((nm, is_new))
        }
        Err(e) => {
            warn!(error = %e, "purge: upsert failed");
            None
        }
    }
}

/// Pull out the BODY[] literal payload from a raw FETCH block. Gmail
/// returns it as `BODY[] {N}\r\n<N bytes>` — we find the size marker
/// and slice the N bytes after the CRLF.
fn extract_body_literal(block: &[u8]) -> Option<&[u8]> {
    let idx = find_subsequence(block, b"BODY[] ")?;
    let after = &block[idx + b"BODY[] ".len()..];
    let brace = after.iter().position(|&b| b == b'{')?;
    let close = after[brace..].iter().position(|&b| b == b'}')? + brace;
    let size_str = std::str::from_utf8(&after[brace + 1..close]).ok()?;
    let n: usize = size_str.parse().ok()?;
    // Skip the `}` and CRLF (or LF).
    let mut start = close + 1;
    if after.get(start) == Some(&b'\r') && after.get(start + 1) == Some(&b'\n') {
        start += 2;
    } else if after.get(start) == Some(&b'\n') {
        start += 1;
    }
    let end = start + n;
    if end > after.len() {
        return None;
    }
    Some(&after[start..end])
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// Parse `* SEARCH 1 2 3 ...` lines out of a response and return the
/// full UID list. Multiple untagged SEARCH responses accumulate.
fn parse_search_uids(raw: &[u8]) -> Vec<u32> {
    let mut out = Vec::new();
    let text = String::from_utf8_lossy(raw);
    for line in text.lines() {
        let trimmed = line.trim();
        let Some(rest) = trimmed.strip_prefix("* SEARCH") else {
            continue;
        };
        for tok in rest.split_ascii_whitespace() {
            if let Ok(n) = tok.parse::<u32>() {
                out.push(n);
            }
        }
    }
    out
}

pub(crate) fn logout(stream: &mut TlsStream<TcpStream>, tag: &str) {
    let _ = write_line(stream, &format!("{tag} LOGOUT"));
    // Drain whatever the server sends back; failure to log out cleanly
    // doesn't matter — the TLS connection is about to be dropped.
    let _ = read_response(stream, Some(tag));
}

pub(crate) fn write_line(stream: &mut TlsStream<TcpStream>, line: &str) -> Result<()> {
    stream
        .write_all(line.as_bytes())
        .and_then(|()| stream.write_all(b"\r\n"))
        .and_then(|()| stream.flush())
        .map_err(|e| Error::Imap(format!("imap write: {e}")))?;
    // Don't log the raw line — LOGIN carries the password.
    debug!("imap C: {}", sanitise_for_log(line));
    Ok(())
}

fn sanitise_for_log(line: &str) -> String {
    if let Some(rest) = line.strip_prefix_preserve_tag("LOGIN") {
        return format!("{rest} LOGIN ***");
    }
    line.to_owned()
}

trait StripPrefixPreserveTag {
    fn strip_prefix_preserve_tag(&self, cmd: &str) -> Option<String>;
}

impl StripPrefixPreserveTag for str {
    fn strip_prefix_preserve_tag(&self, cmd: &str) -> Option<String> {
        let mut parts = self.splitn(2, ' ');
        let tag = parts.next()?;
        let rest = parts.next()?;
        if rest.starts_with(cmd) {
            Some(tag.to_owned())
        } else {
            None
        }
    }
}

/// Read lines from the TLS stream until we see the tagged completion
/// (`<tag> OK|NO|BAD ...`) — or, when `tag` is None, return after the
/// first untagged OK (used for the initial greeting).
///
/// Handles IMAP literal continuations: a line ending in `{N}\r\n` is
/// followed by exactly N bytes of raw data that are part of the same
/// logical response. We append those bytes and keep reading until a
/// plain CRLF-terminated line is found.
pub(crate) fn read_response(stream: &mut TlsStream<TcpStream>, tag: Option<&str>) -> Result<Vec<u8>> {
    let mut buf: Vec<u8> = Vec::new();
    loop {
        let line_start = buf.len();
        read_line_with_literals(stream, &mut buf)?;
        let line = &buf[line_start..];
        // Match outcome.
        match tag {
            Some(t) => {
                if is_tagged_completion(line, t) {
                    return Ok(buf);
                }
            }
            None => {
                // The greeting is a single untagged line — `* OK ...`.
                if line.starts_with(b"* OK") || line.starts_with(b"* PREAUTH") {
                    return Ok(buf);
                }
                if line.starts_with(b"* BYE") {
                    return Err(Error::Imap("server hung up during greeting".into()));
                }
            }
        }
    }
}

/// Gmail's IMAP hits us with `[THROTTLED]` (a response code) when we
/// push too many ops per account per minute. The surrounding text is
/// often misleading — e.g. a throttled UID MOVE comes back as
/// "No folder [Gmail]/Trash (Failure) [THROTTLED]" even though the
/// folder obviously exists. Classify these so callers can log them
/// at debug level and back off instead of warning loudly about a
/// "missing" folder that's nothing of the sort.
pub(crate) fn is_throttled_error(err: &crate::error::Error) -> bool {
    matches!(err, crate::error::Error::Imap(msg) if msg.contains("[THROTTLED]"))
}

pub(crate) fn expect_ok(stream: &mut TlsStream<TcpStream>, tag: &str, op: &str) -> Result<()> {
    let buf = read_response(stream, Some(tag))?;
    // Find the tagged line and inspect the status word.
    for line in buf.split(|&b| b == b'\n') {
        if is_tagged_completion(line, tag) {
            let text = String::from_utf8_lossy(line);
            if text.contains(&format!("{tag} OK")) || text.contains(&format!("{tag} ok")) {
                return Ok(());
            }
            return Err(Error::Imap(format!(
                "{op} refused: {}",
                text.trim().to_owned()
            )));
        }
    }
    Err(Error::Imap(format!("{op}: no tagged completion")))
}

fn is_tagged_completion(line: &[u8], tag: &str) -> bool {
    let text = std::str::from_utf8(line).unwrap_or("");
    let trimmed = text.trim_start();
    if !trimmed.starts_with(tag) {
        return false;
    }
    let after = &trimmed[tag.len()..];
    // Must be followed by whitespace and a status word.
    if !after.starts_with(' ') {
        return false;
    }
    let rest = after.trim_start();
    rest.starts_with("OK ")
        || rest.starts_with("NO ")
        || rest.starts_with("BAD ")
        || rest.starts_with("ok ")
        || rest.starts_with("no ")
        || rest.starts_with("bad ")
}

/// Read one logical IMAP line into `buf`, including any literal
/// continuations the server sends. Each `{N}\r\n` at the tail of a
/// physical line is followed by exactly N bytes; we pull those bytes
/// and then keep reading physical lines until the logical line ends
/// with a plain CRLF (no literal).
fn read_line_with_literals(stream: &mut TlsStream<TcpStream>, buf: &mut Vec<u8>) -> Result<()> {
    loop {
        read_until_lf(stream, buf)?;
        if let Some(n) = trailing_literal_len(buf) {
            let start = buf.len();
            buf.resize(start + n, 0);
            let mut filled = 0;
            while filled < n {
                let got = stream
                    .read(&mut buf[start + filled..start + n])
                    .map_err(|e| Error::Imap(format!("imap read literal: {e}")))?;
                if got == 0 {
                    return Err(Error::Imap("imap read literal: eof".into()));
                }
                filled += got;
            }
            // Continue reading; the logical line isn't done yet.
            continue;
        }
        return Ok(());
    }
}

/// If `buf` ends with `{N}\r\n` (possibly just `{N}\n`), return N.
fn trailing_literal_len(buf: &[u8]) -> Option<usize> {
    let end = if buf.ends_with(b"\r\n") {
        buf.len() - 2
    } else if buf.ends_with(b"\n") {
        buf.len() - 1
    } else {
        return None;
    };
    if end == 0 || buf[end - 1] != b'}' {
        return None;
    }
    let mut i = end - 2;
    while i > 0 && buf[i].is_ascii_digit() {
        i -= 1;
    }
    if buf[i] != b'{' {
        return None;
    }
    let digits = std::str::from_utf8(&buf[i + 1..end - 1]).ok()?;
    digits.parse().ok()
}

fn read_until_lf(stream: &mut TlsStream<TcpStream>, buf: &mut Vec<u8>) -> Result<()> {
    let mut chunk = [0u8; READ_CHUNK];
    loop {
        // Read byte-by-byte so we stop exactly at LF without over-reading
        // into the next logical line / literal. TLS stream already
        // buffers under the hood, so this isn't as slow as it sounds.
        let got = stream
            .read(&mut chunk[..1])
            .map_err(|e| Error::Imap(format!("imap read: {e}")))?;
        if got == 0 {
            return Err(Error::Imap("imap read: eof".into()));
        }
        buf.push(chunk[0]);
        if chunk[0] == b'\n' {
            return Ok(());
        }
    }
}

pub(crate) fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        if ch == '"' || ch == '\\' {
            out.push('\\');
        }
        out.push(ch);
    }
    out.push('"');
    out
}

/// Extract `UIDNEXT <n>` from an EXAMINE response.
pub(crate) fn parse_uid_next(resp: &[u8]) -> Option<u32> {
    let text = String::from_utf8_lossy(resp);
    let idx = text.find("UIDNEXT ")?;
    let rest = &text[idx + "UIDNEXT ".len()..];
    let end = rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(rest.len());
    rest[..end].parse().ok()
}
