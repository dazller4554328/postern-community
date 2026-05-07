//! Backfill-purge of server copies for messages Postern already holds.
//!
//! The streaming sync in `imap.rs` only deletes UIDs in the *current
//! batch*. UIDs already accepted by the local DB on a previous sync
//! pass never come back through the deletion check. So a user who
//! starts an account in "keep on server" mode, downloads years of
//! mail, then flips to "delete after sync" gets no retroactive purge.
//! This module is the catch-up: walk every UID currently on the
//! server, and for any whose Message-ID is in the local DB, MOVE-to-
//! Trash (Gmail) or STORE \Deleted + EXPUNGE (plain IMAP).
//!
//! Trust boundary: we authorise a delete only when the server-reported
//! Message-ID matches a row in `messages` for the same account. That's
//! the same boundary the streaming sync uses to skip re-downloading
//! (`upsert_message` returning `Ok(false)`); reusing it keeps the
//! decision auditable from one place.

use std::{
    collections::HashSet,
    fmt::Write,
    net::TcpStream,
    sync::{Arc, Mutex},
};

use native_tls::TlsStream;
use serde::Serialize;
use tracing::{debug, info, warn};

use crate::{
    error::Result,
    storage::{Account, AccountKind, Db},
    sync::{gmail_labels, gmail_rescan},
};

/// Cap on UIDs queried per FETCH and per MOVE/STORE batch. Big enough
/// to amortise round-trip cost; small enough that one bad UID range
/// doesn't take out a 10-minute purge.
const PURGE_BATCH: u32 = 256;

/// Whether to actually delete or just count. The "Run safety scan"
/// button maps to `Precheck`; the toggle-driven auto-purge and the
/// confirmed UI flow map to `Execute`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PurgeMode {
    Precheck,
    Execute,
}

/// What triggered this purge run. Threaded into the audit log so
/// support can distinguish "user flipped the dropdown" from "user
/// clicked the manual button". Not load-bearing for behaviour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PurgeTrigger {
    /// Set when `delete_after_sync` was just toggled false→true.
    PolicyChange,
    /// Set when the operator pressed "Run safety scan" / "Run purge".
    Manual,
}

/// Per-account purge progress + final report. Persisted in-memory by
/// `PurgeJobs` and surfaced via `GET /accounts/:id/purge-status`.
#[derive(Debug, Clone, Serialize)]
pub struct PurgeReport {
    pub account_id: i64,
    pub mode: PurgeMode,
    pub trigger: PurgeTrigger,
    pub state: PurgeState,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    /// UIDs walked on the server across all folders.
    pub scanned: u32,
    /// UIDs whose Message-ID matched a local row.
    pub verified_safe: u32,
    /// UIDs the server returned but we don't have locally — never
    /// deleted regardless of mode.
    pub skipped_no_local_copy: u32,
    /// UIDs MOVEd to Trash (Gmail) or STORE+EXPUNGEd (plain IMAP).
    /// Always zero in `Precheck` mode.
    pub moved_or_deleted: u32,
    /// Trash messages permanently expunged (Gmail with skip_trash).
    pub expunged_from_trash: u32,
    /// Per-folder error strings (best-effort; never fails the whole job).
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PurgeState {
    Running,
    Success,
    Failed,
}

impl PurgeReport {
    /// Public constructor used by the HTTP layer to seed a "Running"
    /// placeholder before the blocking thread has done any IMAP work.
    /// Internally identical to `new`; exposed so callers in other
    /// modules don't need to construct the struct field-by-field.
    pub fn new_initial(account_id: i64, mode: PurgeMode, trigger: PurgeTrigger) -> Self {
        Self::new(account_id, mode, trigger)
    }

    /// Mark the report as terminally failed. Used by the HTTP spawn
    /// helper when `purge_synced_server_copies` returns an error
    /// before producing a report of its own.
    pub fn finish_failed(&mut self) {
        self.finish(PurgeState::Failed);
    }

    fn new(account_id: i64, mode: PurgeMode, trigger: PurgeTrigger) -> Self {
        Self {
            account_id,
            mode,
            trigger,
            state: PurgeState::Running,
            started_at: chrono::Utc::now().timestamp(),
            finished_at: None,
            scanned: 0,
            verified_safe: 0,
            skipped_no_local_copy: 0,
            moved_or_deleted: 0,
            expunged_from_trash: 0,
            errors: Vec::new(),
        }
    }

    fn finish(&mut self, state: PurgeState) {
        self.state = state;
        self.finished_at = Some(chrono::Utc::now().timestamp());
    }
}

/// Process-wide registry of the most recent purge per account.
///
/// Uses `std::sync::Mutex` rather than `tokio::sync::Mutex`: the lock
/// is held only for clone-or-insert, never across an IMAP round-trip
/// or any other await point. The `spawn_blocking` thread that runs
/// the purge updates the registry directly without re-entering the
/// async runtime — simpler and removes the `block_on(jobs.set(...))`
/// anti-pattern.
#[derive(Default, Clone)]
pub struct PurgeJobs {
    inner: Arc<Mutex<std::collections::HashMap<i64, PurgeReport>>>,
}

impl PurgeJobs {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, account_id: i64) -> Option<PurgeReport> {
        self.inner
            .lock()
            .expect("purge_jobs lock poisoned")
            .get(&account_id)
            .cloned()
    }

    pub fn set(&self, report: PurgeReport) {
        self.inner
            .lock()
            .expect("purge_jobs lock poisoned")
            .insert(report.account_id, report);
    }

    /// Returns `true` only when a Running report is currently recorded
    /// for this account. Used to reject overlapping triggers.
    pub fn is_running(&self, account_id: i64) -> bool {
        matches!(
            self.inner
                .lock()
                .expect("purge_jobs lock poisoned")
                .get(&account_id)
                .map(|r| r.state),
            Some(PurgeState::Running)
        )
    }
}

/// Pure logic: split a batch's `(UID, Message-ID)` pairs into the
/// UIDs safe to delete and the UIDs that must be skipped because we
/// either don't have the message locally or the server didn't give us
/// a Message-ID.
///
/// Kept as a free function so we can unit-test the trust boundary
/// without a TLS socket: the precheck/execute split, the cross-account
/// safety guarantee from `messages_present`, and the missing-mid case
/// all flow through here.
pub(crate) fn decide_purgeable_uids(
    uid_to_mid: &[(u32, Option<String>)],
    present: &HashSet<String>,
) -> (Vec<u32>, u32) {
    let mut safe = Vec::with_capacity(uid_to_mid.len());
    let mut skipped = 0u32;
    for (uid, mid) in uid_to_mid {
        match mid {
            Some(m) if present.contains(m) => safe.push(*uid),
            _ => skipped += 1,
        }
    }
    (safe, skipped)
}

/// Top-level entry point. Dispatches to the Gmail or plain-IMAP path
/// based on the account's IMAP host kind.
pub fn purge_synced_server_copies(
    account: &Account,
    password: &str,
    db: &Db,
    bind_iface: Option<&str>,
    mode: PurgeMode,
    trigger: PurgeTrigger,
) -> Result<PurgeReport> {
    let mut report = PurgeReport::new(account.id, mode, trigger);

    let result = match account.kind {
        AccountKind::Gmail => purge_gmail(account, password, db, bind_iface, mode, &mut report),
        AccountKind::Imap => {
            purge_plain_imap(account, password, db, bind_iface, mode, &mut report)
        }
    };

    match result {
        Ok(()) => report.finish(PurgeState::Success),
        Err(e) => {
            report.errors.push(e.to_string());
            report.finish(PurgeState::Failed);
        }
    }

    Ok(report)
}

// ---------------------------------------------------------------------
// Gmail flow — single pass over [Gmail]/All Mail.
//
// Gmail's labels are virtual: a single message can show in INBOX,
// Sent, and a custom label simultaneously. STORE \Deleted + EXPUNGE
// only removes the *current label*. UID MOVE … [Gmail]/Trash strips
// every label and starts the 30-day purge clock. So one walk of All
// Mail covers every label every UID currently has.
// ---------------------------------------------------------------------

fn purge_gmail(
    account: &Account,
    password: &str,
    db: &Db,
    bind_iface: Option<&str>,
    mode: PurgeMode,
    report: &mut PurgeReport,
) -> Result<()> {
    let mut stream = gmail_rescan::connect_and_login(
        &account.imap_host,
        account.imap_port,
        &account.email,
        password,
        bind_iface,
    )?;

    // Tag counter starts at 2 — LOGIN consumed `a1`. Closure captures
    // it so the tag string is always one expression.
    let mut tag_n: u32 = 1;
    let mut next_tag = || {
        tag_n += 1;
        format!("a{tag_n}")
    };

    // SELECT (read-write) so MOVE is permitted. EXAMINE would 403 us.
    let select_keyword = if mode == PurgeMode::Execute {
        "SELECT"
    } else {
        "EXAMINE"
    };
    let tag = next_tag();
    gmail_rescan::write_line(
        &mut stream,
        &format!("{tag} {select_keyword} \"[Gmail]/All Mail\""),
    )?;
    let select_resp = gmail_rescan::read_response(&mut stream, Some(&tag))?;
    let uid_next = gmail_rescan::parse_uid_next(&select_resp).unwrap_or(1);
    if uid_next <= 1 {
        gmail_rescan::logout(&mut stream, &next_tag());
        return Ok(());
    }
    info!(email = %account.email, uid_next, mode = ?mode, "purge: examined All Mail");

    purge_uids_in_range(
        &mut stream,
        &mut next_tag,
        account.id,
        db,
        1,
        uid_next,
        mode,
        report,
        |stream, next_tag, uid_set| {
            // Gmail: UID MOVE → Trash. Strips every label.
            let tag = next_tag();
            gmail_rescan::write_line(
                stream,
                &format!("{tag} UID MOVE {uid_set} \"[Gmail]/Trash\""),
            )?;
            gmail_rescan::expect_ok(stream, &tag, "UID MOVE")
        },
    )?;

    // Optional: skip Gmail's 30-day Trash lifecycle.
    if mode == PurgeMode::Execute && account.skip_gmail_trash && report.moved_or_deleted > 0 {
        match gmail_rescan::empty_trash(&mut stream, &mut next_tag) {
            Ok(n) => {
                report.expunged_from_trash = n;
                info!(email = %account.email, n, "purge: trash emptied");
            }
            Err(e) => {
                warn!(error = %e, "purge: empty-trash failed");
                report.errors.push(format!("empty trash: {e}"));
            }
        }
    }

    gmail_rescan::logout(&mut stream, &next_tag());
    Ok(())
}

// ---------------------------------------------------------------------
// Plain IMAP flow — walk every selectable folder, STORE \Deleted +
// EXPUNGE for matching UIDs in each.
//
// Unlike Gmail, plain IMAP folders are real (one message lives in one
// folder per inbox/sent/etc), so we have to visit each folder in turn.
// `skip_gmail_trash` is irrelevant here because EXPUNGE already
// removes the message permanently — there's no Trash equivalent.
// ---------------------------------------------------------------------

fn purge_plain_imap(
    account: &Account,
    password: &str,
    db: &Db,
    bind_iface: Option<&str>,
    mode: PurgeMode,
    report: &mut PurgeReport,
) -> Result<()> {
    let mut stream = gmail_rescan::connect_and_login(
        &account.imap_host,
        account.imap_port,
        &account.email,
        password,
        bind_iface,
    )?;
    let mut tag_n: u32 = 1;
    let mut next_tag = || {
        tag_n += 1;
        format!("a{tag_n}")
    };

    let folders = list_folders(&mut stream, &mut next_tag)?;
    info!(email = %account.email, folder_count = folders.len(), "purge: listed folders");

    for folder in folders {
        if let Err(e) =
            purge_one_imap_folder(&mut stream, &mut next_tag, account.id, db, &folder, mode, report)
        {
            warn!(folder = %folder, error = %e, "purge: folder failed (continuing)");
            report.errors.push(format!("{folder}: {e}"));
        }
    }

    gmail_rescan::logout(&mut stream, &next_tag());
    Ok(())
}

fn purge_one_imap_folder(
    stream: &mut TlsStream<TcpStream>,
    next_tag: &mut dyn FnMut() -> String,
    account_id: i64,
    db: &Db,
    folder: &str,
    mode: PurgeMode,
    report: &mut PurgeReport,
) -> Result<()> {
    // SELECT (read-write) for STORE+EXPUNGE; EXAMINE for precheck.
    let select_keyword = if mode == PurgeMode::Execute {
        "SELECT"
    } else {
        "EXAMINE"
    };
    let tag = next_tag();
    gmail_rescan::write_line(
        stream,
        &format!("{tag} {select_keyword} {}", gmail_rescan::quote(folder)),
    )?;
    let select_resp = gmail_rescan::read_response(stream, Some(&tag))?;
    let uid_next = gmail_rescan::parse_uid_next(&select_resp).unwrap_or(1);
    if uid_next <= 1 {
        return Ok(());
    }
    debug!(%folder, uid_next, "purge folder: examined");

    purge_uids_in_range(
        stream,
        next_tag,
        account_id,
        db,
        1,
        uid_next,
        mode,
        report,
        |stream, next_tag, uid_set| {
            // Plain IMAP: STORE \Deleted then EXPUNGE the folder.
            let tag = next_tag();
            gmail_rescan::write_line(
                stream,
                &format!("{tag} UID STORE {uid_set} +FLAGS (\\Deleted)"),
            )?;
            gmail_rescan::expect_ok(stream, &tag, "UID STORE")?;
            let tag = next_tag();
            gmail_rescan::write_line(stream, &format!("{tag} EXPUNGE"))?;
            gmail_rescan::expect_ok(stream, &tag, "EXPUNGE")
        },
    )
}

// ---------------------------------------------------------------------
// Shared range-walker used by both flows.
// ---------------------------------------------------------------------

fn purge_uids_in_range(
    stream: &mut TlsStream<TcpStream>,
    next_tag: &mut dyn FnMut() -> String,
    account_id: i64,
    db: &Db,
    start_uid: u32,
    uid_next: u32,
    mode: PurgeMode,
    report: &mut PurgeReport,
    mut delete_batch: impl FnMut(
        &mut TlsStream<TcpStream>,
        &mut dyn FnMut() -> String,
        &str,
    ) -> Result<()>,
) -> Result<()> {
    let mut cursor = start_uid;
    while cursor < uid_next {
        let end = cursor.saturating_add(PURGE_BATCH - 1).min(uid_next - 1);
        let range = format!("{cursor}:{end}");

        let tag = next_tag();
        gmail_rescan::write_line(
            stream,
            &format!("{tag} UID FETCH {range} (UID BODY.PEEK[HEADER.FIELDS (MESSAGE-ID)])"),
        )?;
        let bytes = match gmail_rescan::read_response(stream, Some(&tag)) {
            Ok(b) => b,
            Err(e) => {
                warn!(%range, error = %e, "purge: batch FETCH failed, skipping");
                report.errors.push(format!("fetch {range}: {e}"));
                cursor = end.saturating_add(1);
                continue;
            }
        };

        // Parse (UID, Message-ID) pairs.
        let mut pairs: Vec<(u32, Option<String>)> = Vec::new();
        let mut mids_in_batch: Vec<String> = Vec::new();
        for block in gmail_labels::split_fetch_blocks(&bytes) {
            let Some(uid) = gmail_labels::extract_uid(&block) else {
                continue;
            };
            let mid = gmail_labels::extract_message_id(&block);
            if let Some(m) = mid.as_deref() {
                mids_in_batch.push(m.to_owned());
            }
            pairs.push((uid, mid));
            report.scanned += 1;
        }

        // One bulk DB lookup per batch — far cheaper than per-UID.
        let present = if mids_in_batch.is_empty() {
            HashSet::new()
        } else {
            db.messages_present(account_id, &mids_in_batch)?
        };

        let (safe_uids, skipped) = decide_purgeable_uids(&pairs, &present);
        report.verified_safe += safe_uids.len() as u32;
        report.skipped_no_local_copy += skipped;

        if mode == PurgeMode::Execute && !safe_uids.is_empty() {
            // Chunk MOVE/STORE so each command stays under a sane wire
            // size. PURGE_BATCH is the chunk size for both fetch and
            // delete; in practice safe_uids.len() ≤ PURGE_BATCH so this
            // is a single command, but defensive against future tuning.
            for chunk in safe_uids.chunks(PURGE_BATCH as usize) {
                let uid_set = format_uid_set(chunk);
                match delete_batch(stream, next_tag, &uid_set) {
                    Ok(()) => report.moved_or_deleted += chunk.len() as u32,
                    Err(e) => {
                        warn!(error = %e, "purge: delete batch failed (continuing)");
                        report.errors.push(format!("delete: {e}"));
                    }
                }
            }
        }

        cursor = end.saturating_add(1);
    }
    Ok(())
}

// LIST every selectable folder. Skips \Noselect / \Nonexistent so we
// don't try to SELECT a parent placeholder.
fn list_folders(
    stream: &mut TlsStream<TcpStream>,
    next_tag: &mut dyn FnMut() -> String,
) -> Result<Vec<String>> {
    let tag = next_tag();
    gmail_rescan::write_line(stream, &format!("{tag} LIST \"\" \"*\""))?;
    let bytes = gmail_rescan::read_response(stream, Some(&tag))?;
    Ok(parse_list_folders(&bytes))
}

/// Pure parser for the response to `LIST "" "*"`. Each untagged
/// `* LIST (flags) "delim" "name"` line yields one folder name; rows
/// flagged `\Noselect` or `\Nonexistent` are dropped because SELECT
/// would fail on them.
fn parse_list_folders(bytes: &[u8]) -> Vec<String> {
    let text = String::from_utf8_lossy(bytes);
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim_start_matches(['\r', '\n']);
        if !line.starts_with("* LIST ") {
            continue;
        }
        let Some(open) = line.find('(') else { continue };
        let Some(close) = line[open..].find(')') else {
            continue;
        };
        let flags = &line[open + 1..open + close];
        let flags_lc = flags.to_ascii_lowercase();
        if flags_lc.contains("\\noselect") || flags_lc.contains("\\nonexistent") {
            continue;
        }
        // Folder name is the last token; quoted form takes precedence.
        let rest = &line[open + close + 1..];
        if let Some(name) = parse_last_quoted_or_atom(rest) {
            out.push(name);
        }
    }
    out
}

fn parse_last_quoted_or_atom(s: &str) -> Option<String> {
    let s = s.trim();
    // Find the LAST quoted token: scan right-to-left for the closing
    // `"`, then for the matching opening `"` before it. The hierarchy
    // delimiter (e.g. `"/"`) is itself quoted, so a naive `strip_prefix`
    // would swallow it.
    if let Some(close) = s.rfind('"') {
        if let Some(open) = s[..close].rfind('"') {
            return Some(s[open + 1..close].to_owned());
        }
    }
    s.split_whitespace().last().map(str::to_owned)
}

/// Compress a sorted UID list into IMAP's `1,3:5,9` set syntax.
/// We could ship one UID per character but RFC-friendly ranges keep
/// the line length under control on huge mailboxes.
pub(crate) fn format_uid_set(uids: &[u32]) -> String {
    if uids.is_empty() {
        return String::new();
    }
    let mut sorted: Vec<u32> = uids.to_vec();
    sorted.sort_unstable();
    sorted.dedup();

    let mut out = String::with_capacity(sorted.len() * 6);
    let mut i = 0;
    while i < sorted.len() {
        let start = sorted[i];
        let mut end = start;
        while i + 1 < sorted.len() && sorted[i + 1] == end + 1 {
            end = sorted[i + 1];
            i += 1;
        }
        if !out.is_empty() {
            out.push(',');
        }
        if start == end {
            out.write_fmt(format_args!("{start}")).unwrap();
        } else {
            out.write_fmt(format_args!("{start}:{end}")).unwrap();
        }
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mid_set(items: &[&str]) -> HashSet<String> {
        items.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn decide_returns_only_uids_with_local_match() {
        let pairs = vec![
            (10, Some("<a@local>".into())),
            (11, Some("<missing@local>".into())),
            (12, Some("<b@local>".into())),
        ];
        let present = mid_set(&["<a@local>", "<b@local>"]);
        let (safe, skipped) = decide_purgeable_uids(&pairs, &present);
        assert_eq!(safe, vec![10, 12]);
        assert_eq!(skipped, 1);
    }

    #[test]
    fn decide_skips_uids_without_message_id() {
        // A message with no parseable Message-ID header. Even if the
        // user wants delete-after-sync, we can't prove we have it, so
        // we MUST NOT delete.
        let pairs = vec![(1, None), (2, Some("<x@local>".into()))];
        let present = mid_set(&["<x@local>"]);
        let (safe, skipped) = decide_purgeable_uids(&pairs, &present);
        assert_eq!(safe, vec![2]);
        assert_eq!(skipped, 1);
    }

    #[test]
    fn decide_returns_empty_when_present_set_is_empty() {
        let pairs = vec![(1, Some("<a@local>".into())), (2, Some("<b@local>".into()))];
        let (safe, skipped) = decide_purgeable_uids(&pairs, &HashSet::new());
        assert!(safe.is_empty());
        assert_eq!(skipped, 2);
    }

    #[test]
    fn decide_returns_empty_for_empty_input() {
        let (safe, skipped) = decide_purgeable_uids(&[], &HashSet::new());
        assert!(safe.is_empty());
        assert_eq!(skipped, 0);
    }

    #[test]
    fn format_uid_set_compresses_runs_into_ranges() {
        assert_eq!(format_uid_set(&[1, 2, 3, 5, 7, 8, 9]), "1:3,5,7:9");
    }

    #[test]
    fn format_uid_set_handles_singletons() {
        assert_eq!(format_uid_set(&[42]), "42");
    }

    #[test]
    fn format_uid_set_dedups_and_sorts() {
        assert_eq!(format_uid_set(&[5, 1, 2, 1, 3]), "1:3,5");
    }

    #[test]
    fn format_uid_set_empty() {
        assert_eq!(format_uid_set(&[]), "");
    }

    #[test]
    fn parse_list_drops_noselect() {
        let raw = b"* LIST (\\HasChildren \\Noselect) \"/\" \"[Gmail]\"\r\n\
                    * LIST (\\HasNoChildren) \"/\" \"INBOX\"\r\n\
                    * LIST (\\HasNoChildren) \"/\" \"Sent\"\r\n\
                    a3 OK LIST completed.\r\n";
        let folders = parse_list_folders(raw);
        assert_eq!(folders, vec!["INBOX".to_string(), "Sent".to_string()]);
    }

    #[test]
    fn parse_list_handles_multiword_quoted_names() {
        let raw = b"* LIST () \"/\" \"My Custom Folder\"\r\n\
                    a3 OK LIST completed.\r\n";
        let folders = parse_list_folders(raw);
        assert_eq!(folders, vec!["My Custom Folder".to_string()]);
    }

    #[test]
    fn report_starts_running_and_finishes_to_state() {
        let mut r = PurgeReport::new(7, PurgeMode::Execute, PurgeTrigger::Manual);
        assert_eq!(r.state, PurgeState::Running);
        assert!(r.finished_at.is_none());
        r.finish(PurgeState::Success);
        assert_eq!(r.state, PurgeState::Success);
        assert!(r.finished_at.is_some());
    }
}
