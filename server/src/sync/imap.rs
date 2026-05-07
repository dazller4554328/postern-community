use std::{collections::HashSet, net::TcpStream, time::Duration};

use imap::extensions::idle::WaitOutcome;
use imap::Session;
use native_tls::{TlsConnector, TlsStream};
use serde::Serialize;
use tracing::{debug, info, warn};

use crate::{
    error::{Error, Result},
    storage::{Account, AccountKind, BlobStore, Db},
    sync::{label_policy, parser},
};

const FETCH_ATTRS: &str = "(UID INTERNALDATE RFC822.SIZE BODY.PEEK[] FLAGS)";
const BATCH: u32 = 200;

// All Mail goes LAST. A Gmail message carries multiple labels, but we
// import it under exactly the one folder Postern happens to be syncing
// at the time — and if delete_after_sync is on, we MOVE that UID to
// Trash right after the import, so no other folder can ever re-label
// it. Putting specific folders (Sent Mail, Drafts, Spam, Trash) first
// means sent/draft/spam messages get the *correct* label before All
// Mail sweeps them up. All Mail is the catch-all for anything else.
const GMAIL_PRIORITY: &[&str] = &[
    "INBOX",
    "[Gmail]/Sent Mail",
    "[Gmail]/Drafts",
    "[Gmail]/Important",
    "[Gmail]/Starred",
    "[Gmail]/Spam",
    "[Gmail]/Trash",
    "[Gmail]/All Mail",
];

/// Personal-namespace prefix + delimiter as returned by NAMESPACE
/// (RFC 2342). Dovecot typically advertises `("INBOX" ".")`, Gmail
/// `("" "/")`, Exchange varies. When the server doesn't advertise
/// NAMESPACE we derive a reasonable default from folder names we see
/// on first LIST.
#[derive(Debug, Clone)]
pub struct Namespace {
    pub prefix: String,
    pub delimiter: String,
}

impl Default for Namespace {
    fn default() -> Self {
        Self {
            prefix: String::new(),
            delimiter: "/".into(),
        }
    }
}

pub struct ImapClient {
    session: Session<TlsStream<TcpStream>>,
    namespace: Namespace,
}

impl ImapClient {
    /// Connect to the IMAP server. When `bind_iface` is set, the underlying
    /// TCP socket is pinned to that network interface via SO_BINDTODEVICE —
    /// that's how we route IMAP through `wg0` when the VPN is enabled.
    /// If the interface is missing, connect() fails hard — kill-switch.
    pub fn connect(
        host: &str,
        port: u16,
        email: &str,
        password: &str,
        bind_iface: Option<&str>,
    ) -> Result<Self> {
        let tcp = open_tcp(host, port, bind_iface)?;
        let tls_conn = TlsConnector::builder()
            .build()
            .map_err(|e| Error::Imap(format!("tls init: {e}")))?;
        let tls = tls_conn
            .connect(host, tcp)
            .map_err(|e| Error::Imap(format!("tls handshake: {e}")))?;

        let client = imap::Client::new(tls);
        let mut session = client
            .login(email, password)
            .map_err(|(e, _)| Error::Imap(format!("login: {e}")))?;

        // Derive the personal-namespace (prefix + delimiter) from the
        // INBOX's LIST response. Originally this used raw NAMESPACE
        // via run_command_and_read_response, but imap-proto's handling
        // of NAMESPACE in that crate version is flaky on real servers
        // — when parsing fails mid-response, leftover bytes poison the
        // stream for the next structured command and panic with a tag-
        // match assert.
        //
        // INBOX's delimiter is the same as every other folder in the
        // personal namespace. For prefix: we sniff the first non-INBOX
        // mailbox name and pull everything up to (and including) the
        // first delimiter. Dovecot → "INBOX", Gmail → "" / "[Gmail]"
        // (we just use "" since Gmail folders aren't all under [Gmail]
        // anyway).
        let namespace = probe_namespace(&mut session).unwrap_or_default();
        info!(%email, %host, iface = ?bind_iface, prefix = %namespace.prefix, delim = %namespace.delimiter, "imap connected");
        Ok(Self { session, namespace })
    }

    pub fn namespace(&self) -> &Namespace {
        &self.namespace
    }

    pub fn logout(mut self) {
        let _ = self.session.logout();
    }

    /// True when the server's CAPABILITIES advertise the IDLE
    /// extension (RFC 2177). The IDLE listener consults this before
    /// committing to a long-lived connection per account; servers
    /// without IDLE fall back to the polling scheduler.
    pub fn supports_idle(&mut self) -> bool {
        match self.session.capabilities() {
            Ok(caps) => caps.has_str("IDLE"),
            Err(e) => {
                debug!(error = %e, "capability probe failed; assuming no IDLE");
                false
            }
        }
    }

    /// SELECT INBOX and enter IDLE for up to `keepalive`. Returns the
    /// outcome so callers can distinguish a real push (`MailboxChanged`)
    /// from a half-open socket that just sat there (`TimedOut`). The
    /// IDLE handle's Drop sends DONE.
    pub fn idle_inbox(&mut self, keepalive: std::time::Duration) -> Result<WaitOutcome> {
        self.session
            .select("INBOX")
            .map_err(|e| Error::Imap(format!("select INBOX for idle: {e}")))?;
        let handle = self
            .session
            .idle()
            .map_err(|e| Error::Imap(format!("idle: {e}")))?;
        handle
            .wait_with_timeout(keepalive)
            .map_err(|e| Error::Imap(format!("idle wait: {e}")))
    }

    /// Move a message identified by its RFC822 Message-ID from one folder
    /// to another. Opens the source folder in read-write mode, searches
    /// for the Message-ID, and uses COPY + STORE \Deleted + EXPUNGE
    /// (safer than MOVE which not all servers support). Returns true
    /// when the message was located and moved; false if the search
    /// returned no UIDs (message wasn't in the source folder).
    pub fn move_message(
        &mut self,
        message_id: &str,
        from_folder: &str,
        to_folder: &str,
    ) -> Result<bool> {
        self.session
            .select(from_folder)
            .map_err(|e| Error::Imap(format!("select {from_folder}: {e}")))?;

        // Search by Message-ID header.
        let query = format!("HEADER Message-ID \"{message_id}\"");
        let uids = self
            .session
            .uid_search(&query)
            .map_err(|e| Error::Imap(format!("uid search: {e}")))?;

        if uids.is_empty() {
            return Ok(false);
        }

        let uid_set = parser::uid_set(&uids.iter().copied().collect::<Vec<_>>());

        // Copy to destination, then flag as deleted in source, then expunge.
        self.session
            .uid_copy(&uid_set, to_folder)
            .map_err(|e| Error::Imap(format!("uid copy to {to_folder}: {e}")))?;
        self.session
            .uid_store(&uid_set, "+FLAGS (\\Deleted)")
            .map_err(|e| Error::Imap(format!("uid store deleted: {e}")))?;
        self.session
            .expunge()
            .map_err(|e| Error::Imap(format!("expunge: {e}")))?;

        Ok(true)
    }

    /// APPEND a raw RFC822 message to the given folder. Used by the send
    /// path to file a copy in Sent on non-Gmail accounts. Gmail auto-files
    /// sent mail from SMTP so callers skip this for Gmail.
    pub fn append_raw(&mut self, folder: &str, raw: &[u8]) -> Result<()> {
        self.session
            .append(folder, raw)
            .map_err(|e| Error::Imap(format!("append {folder}: {e}")))?;
        Ok(())
    }

    /// Empty a folder: select it, flag every UID as \Deleted, EXPUNGE.
    /// Used by the empty-folder path for Trash / Spam. Safe to call on
    /// an empty folder (the UID search returns nothing and we no-op).
    pub fn select_and_purge(&mut self, folder: &str) -> Result<()> {
        self.session
            .select(folder)
            .map_err(|e| Error::Imap(format!("select {folder}: {e}")))?;
        let uids = self
            .session
            .uid_search("ALL")
            .map_err(|e| Error::Imap(format!("uid search {folder}: {e}")))?;
        if uids.is_empty() {
            return Ok(());
        }
        let uid_set = parser::uid_set(&uids.iter().copied().collect::<Vec<_>>());
        self.session
            .uid_store(&uid_set, "+FLAGS (\\Deleted)")
            .map_err(|e| Error::Imap(format!("uid store \\Deleted on {folder}: {e}")))?;
        self.session
            .expunge()
            .map_err(|e| Error::Imap(format!("expunge {folder}: {e}")))?;
        Ok(())
    }

    /// Ensure a folder path exists on the server, creating each segment
    /// along the way. Idempotent — CREATE on an existing folder fails
    /// with an "already exists" style error that we swallow.
    ///
    /// Uses `/` as the hierarchy separator, which matches Gmail, Dovecot,
    /// and most modern IMAP servers. We deliberately avoid LIST for
    /// existence checks here: the imap crate doesn't quote arbitrary
    /// mailbox names for LIST (patterns can contain `*`/`%` wildcards),
    /// so anything with a space like "Conversation History" would blow
    /// up with a "Bad Response: Could not parse command" on Gmail.
    /// CREATE receives the name as a proper quoted string and is fine
    /// with spaces and other punctuation.
    pub fn ensure_folder(&mut self, path: &str) -> Result<()> {
        let path = path.trim_matches('/');
        if path.is_empty() {
            return Ok(());
        }
        let segments: Vec<&str> = path.split('/').collect();
        let mut acc = String::new();
        for seg in segments {
            if !acc.is_empty() {
                acc.push('/');
            }
            acc.push_str(seg);

            match self.session.create(&acc) {
                Ok(_) => info!(folder = %acc, "created imap folder"),
                Err(e) => {
                    // Servers vary in how they spell the "already exists"
                    // case. Gmail: "folder already exists", Dovecot:
                    // "mailbox already exists", Courier: "ALREADYEXISTS".
                    // Swallow anything that clearly says exists/already.
                    let msg = e.to_string().to_lowercase();
                    if msg.contains("exists") || msg.contains("already") {
                        debug!(folder = %acc, "folder already exists");
                    } else {
                        return Err(Error::Imap(format!("create {acc}: {e}")));
                    }
                }
            }
        }
        Ok(())
    }

    /// Rename `from` to `to`. The IMAP RENAME command creates any
    /// missing parent segments in `to` automatically.
    pub fn rename_folder(&mut self, from: &str, to: &str) -> Result<()> {
        self.session
            .rename(from, to)
            .map_err(|e| Error::Imap(format!("rename {from} -> {to}: {e}")))?;
        info!(from = %from, to = %to, "imap folder renamed");
        Ok(())
    }

    /// Delete a folder on the server. Many servers refuse if the folder
    /// has children or is non-empty — the error bubbles up so callers
    /// can surface a useful message.
    pub fn delete_folder(&mut self, name: &str) -> Result<()> {
        self.session
            .delete(name)
            .map_err(|e| Error::Imap(format!("delete {name}: {e}")))?;
        info!(folder = %name, "imap folder deleted");
        Ok(())
    }

    pub fn folders(&mut self, is_gmail: bool) -> Result<Vec<String>> {
        let mut names: Vec<String> = self
            .session
            .list(None, Some("*"))
            .map_err(|e| Error::Imap(format!("list: {e}")))?
            .iter()
            .filter(|m| {
                !m.attributes()
                    .iter()
                    .any(|a| matches!(a, imap::types::NameAttribute::NoSelect))
            })
            .map(|m| m.name().to_owned())
            .collect();

        if is_gmail {
            let set: HashSet<_> = names.iter().cloned().collect();
            let mut ordered: Vec<String> = GMAIL_PRIORITY
                .iter()
                .filter(|p| set.contains(**p))
                .map(|s| (*s).to_owned())
                .collect();
            for n in names.drain(..) {
                if !ordered.contains(&n) {
                    ordered.push(n);
                }
            }
            names = ordered;
        }
        Ok(names)
    }

    pub fn sync_folder(
        &mut self,
        account: &Account,
        folder: &str,
        db: &Db,
        blobs: &BlobStore,
        vault: &crate::vault::Vault,
    ) -> Result<FolderSyncReport> {
        // Use SELECT (read-write) when delete-after-sync is enabled so
        // we can STORE \Deleted + EXPUNGE after downloading. Otherwise
        // EXAMINE (read-only) to avoid any accidental modifications.
        let mbox = if account.delete_after_sync {
            self.session
                .select(folder)
                .map_err(|e| Error::Imap(format!("select {folder}: {e}")))?
        } else {
            self.session
                .examine(folder)
                .map_err(|e| Error::Imap(format!("examine {folder}: {e}")))?
        };

        let uid_validity = mbox.uid_validity.unwrap_or(0);
        let uid_next = mbox.uid_next.unwrap_or(1);

        let label_id = db.upsert_label(account.id, folder, classify_label(folder))?;
        let (prev_validity, prev_next) = db.sync_state(account.id, label_id)?;

        let start_uid = match prev_validity {
            Some(v) if v == uid_validity => prev_next.unwrap_or(1),
            _ => 1,
        };
        if prev_validity.is_some_and(|v| v != uid_validity) {
            warn!(%folder, old = prev_validity, new = uid_validity, "UIDVALIDITY changed — full resync");
        }

        if start_uid >= uid_next {
            db.update_sync_state(account.id, label_id, uid_validity, uid_next)?;
            return Ok(FolderSyncReport {
                folder: folder.to_owned(),
                new: 0,
                scanned: 0,
            });
        }

        let mut new = 0u32;
        let mut scanned = 0u32;
        let mut cursor = start_uid;
        while cursor < uid_next {
            let end = cursor.saturating_add(BATCH - 1).min(uid_next - 1);
            let range = format!("{cursor}:{end}");
            debug!(%folder, range = %range, "uid fetch");

            let msgs = self
                .session
                .uid_fetch(&range, FETCH_ATTRS)
                .map_err(|e| Error::Imap(format!("uid fetch {range}: {e}")))?;

            // Live sync is folder-only. Gmail's hidden labels
            // (categories, unexposed user labels) are picked up by
            // the manual `POST /api/accounts/:id/rescan-gmail-labels`
            // endpoint, which owns its own all-raw IMAP flow and
            // doesn't mix tag-generating calls the way this batch
            // loop does.
            let mut imported_uids: Vec<u32> = Vec::new();
            // Parallel collection of Message-IDs for the UIDs we
            // import — used by the delete_after_sync MOVE batch.
            let mut imported_message_ids: Vec<String> = Vec::new();
            // Trusted-sender rescue queue. New arrivals in the spam
            // folder whose From: matches an entry in trusted_senders
            // get auto-moved back to INBOX after the per-batch parse
            // loop, with their local label rewritten to INBOX.
            let folder_is_spam = is_spam_folder(folder);
            let mut rescue_uids: Vec<u32> = Vec::new();
            let mut rescue_msg_rows: Vec<i64> = Vec::new();
            for m in msgs.iter() {
                scanned += 1;
                let Some(body) = m.body() else { continue };
                let parsed = parser::parse(body);
                let hash = blobs.put(body)?;
                let thread_id = m.uid.map(|u| format!("imap-{folder}-{u}"));
                let is_read = m
                    .flags()
                    .iter()
                    .any(|f| matches!(f, imap::types::Flag::Seen));
                let is_encrypted = parser::is_pgp_encrypted(body);

                // Decide which local label(s) to attach via the
                // single-source-of-truth label policy. See
                // sync::label_policy + docs/STORAGE_INVARIANTS.md.
                // The presence check is the bit policy needs from
                // the DB; the rest is pure logic.
                let presence = if !parsed.message_id.is_empty()
                    && matches!(
                        db.message_row_id(account.id, &parsed.message_id),
                        Ok(Some(_))
                    )
                {
                    label_policy::MessagePresence::AlreadyKnown
                } else {
                    label_policy::MessagePresence::New
                };
                let label_for_this_sync =
                    label_policy::decide_mirror(account.kind, folder, presence).add;

                let nm = parser::into_new_message(
                    account.id,
                    parsed,
                    hash,
                    body.len(),
                    label_for_this_sync,
                    thread_id,
                    is_read,
                    is_encrypted,
                );
                match db.upsert_message(&nm) {
                    Ok(true) => {
                        new += 1;
                        if let Some(uid) = m.uid {
                            imported_uids.push(uid);
                            imported_message_ids.push(nm.message_id.clone());
                        }
                        crate::pgp::harvest_autocrypt(body, db, vault);
                        let msg_row_id = match db.message_row_id(account.id, &nm.message_id) {
                            Ok(id) => id,
                            Err(e) => {
                                warn!(error = %e, message_id = %nm.message_id, "lookup new message row for rule-apply failed");
                                None
                            }
                        };
                        if let Some(msg_row) = msg_row_id {
                            crate::rules::apply_rules(
                                db,
                                account.id,
                                msg_row,
                                nm.from_addr.as_deref().unwrap_or(""),
                                nm.to_addrs.as_deref().unwrap_or(""),
                                nm.cc_addrs.as_deref().unwrap_or(""),
                                nm.subject.as_deref().unwrap_or(""),
                            );
                            // Trusted-sender rescue: a brand-new
                            // arrival in a Spam/Junk folder gets
                            // moved back to INBOX (and locally
                            // relabelled) when its sender is on the
                            // user's allowlist. Misclassifying a
                            // bulk sender once shouldn't sentence
                            // every future message to Spam.
                            if folder_is_spam {
                                let from = nm.from_addr.as_deref().unwrap_or("");
                                match db.is_trusted_sender(account.id, from) {
                                    Ok(true) => {
                                        if let Some(uid) = m.uid {
                                            rescue_uids.push(uid);
                                            rescue_msg_rows.push(msg_row);
                                        }
                                    }
                                    Ok(false) => {}
                                    Err(e) => warn!(error = %e, sender = from, "trusted-sender lookup failed"),
                                }
                            }
                        }
                    }
                    Ok(false) => {
                        // Already existed — still mark for deletion since
                        // Postern has it and the server copy is redundant.
                        if account.delete_after_sync {
                            if let Some(uid) = m.uid {
                                imported_uids.push(uid);
                                imported_message_ids.push(nm.message_id.clone());
                            }
                        }
                    }
                    Err(e) => warn!(error = %e, "skip message"),
                }
            }

            // Trusted-sender rescue. Run BEFORE delete_after_sync so a
            // rescued UID never gets routed to Trash on the same cycle
            // it was rescued. Server-side MOVE to INBOX, then rewrite
            // local labels so Postern's view matches.
            //
            // delete_after_sync is *for the current spam folder*. A
            // rescued message is no longer in that folder by the time
            // the delete batch runs, so we strip rescued UIDs out of
            // imported_uids defensively even though the IMAP move
            // already removed them from the source mailbox.
            if !rescue_uids.is_empty() {
                let uid_set = parser::uid_set(&rescue_uids);
                // Gmail supports MOVE; everything else gets COPY +
                // STORE \Deleted + EXPUNGE so we don't depend on the
                // optional MOVE extension.
                let move_result = if account.kind == AccountKind::Gmail {
                    self.session.uid_mv(&uid_set, "INBOX").map(|_| ())
                } else {
                    self.session
                        .uid_copy(&uid_set, "INBOX")
                        .and_then(|_| self.session.uid_store(&uid_set, "+FLAGS (\\Deleted)"))
                        .and_then(|_| self.session.expunge())
                        .map(|_| ())
                };
                match move_result {
                    Ok(()) => {
                        for &row in &rescue_msg_rows {
                            if let Err(e) = db.relabel_message(row, account.id, &["INBOX"]) {
                                warn!(error = %e, msg_row = row, "rescue relabel failed");
                            }
                        }
                        info!(
                            %folder,
                            count = rescue_uids.len(),
                            "trusted-sender rescue: moved spam to INBOX"
                        );
                        let detail = format!(
                            "{}: {} rescued from {}",
                            account.email,
                            rescue_uids.len(),
                            folder
                        );
                        let _ = db.log_activity("trusted_sender_rescue", Some(&detail));
                        // Don't double-process rescued UIDs in the
                        // delete_after_sync batch below — the IMAP
                        // MOVE has already taken them out of the
                        // source mailbox.
                        let rescued: HashSet<u32> = rescue_uids.iter().copied().collect();
                        imported_uids.retain(|u| !rescued.contains(u));
                    }
                    Err(e) => {
                        warn!(%folder, error = %e, "trusted-sender rescue MOVE failed");
                    }
                }
            }

            // Delete successfully-imported messages from the server.
            // Gmail's \Deleted + EXPUNGE only removes the current
            // label, not the message — the copy lives on in All Mail
            // and under every other applied label, still counting
            // against quota. MOVE to [Gmail]/Trash is the only way to
            // actually let the message go (Trash auto-purges after 30
            // days and strips every label on the way in). Plain IMAP
            // keeps the classic STORE \Deleted + EXPUNGE flow.
            if account.delete_after_sync && !imported_uids.is_empty() {
                let uid_set = parser::uid_set(&imported_uids);
                let result = if account.kind == AccountKind::Gmail {
                    // Don't move when we're ALREADY in Trash —
                    // MOVE-to-self fails and we'd just lose the label.
                    if folder == "[Gmail]/Trash" {
                        self.session.expunge().map(|_| ())
                    } else {
                        self.session.uid_mv(&uid_set, "[Gmail]/Trash").map(|_| ())
                    }
                } else {
                    self.session
                        .uid_store(&uid_set, "+FLAGS (\\Deleted)")
                        .and_then(|_| self.session.expunge())
                        .map(|_| ())
                };
                match result {
                    Ok(()) => {
                        info!(%folder, count = imported_uids.len(), "delete_after_sync: purged server copy");
                        // Note: we do NOT strip the source-folder label
                        // locally. Postern's view is independent of
                        // server folder state — a message synced from
                        // INBOX stays in the user's local Inbox view
                        // even after the server copy moves to Trash
                        // for quota reasons. The complementary half of
                        // this is in the [Gmail]/Trash sync path,
                        // which skips re-tagging known messages with
                        // Trash so they don't appear in two views.
                    }
                    Err(e) => {
                        warn!(%folder, error = %e, "delete_after_sync failed (messages kept locally)")
                    }
                }
            }

            cursor = end.saturating_add(1);
        }

        db.update_sync_state(account.id, label_id, uid_validity, uid_next)?;
        info!(%folder, new, scanned, "folder synced");
        Ok(FolderSyncReport {
            folder: folder.to_owned(),
            new,
            scanned,
        })
    }

    /// Delete all INBOX messages older than `cutoff_days`, skipping any
    /// that are flagged (starred). Gmail semantics require moving to
    /// `[Gmail]/Trash` to actually free quota — setting \Deleted on
    /// INBOX alone only removes the label, leaving the copy in All Mail
    /// and quota unchanged. Plain IMAP uses STORE \Deleted + EXPUNGE.
    ///
    /// Returns the number of UIDs the server reported deleted. Any
    /// failure is non-fatal — the next sweep retries.
    pub fn sweep_retention_inbox(&mut self, is_gmail: bool, cutoff_days: i32) -> Result<u32> {
        let days = cutoff_days.max(1);
        // IMAP SEARCH BEFORE takes a RFC 3501 date like "19-Apr-2026";
        // messages whose INTERNALDATE is strictly earlier than the given
        // day match. Compute the cutoff in UTC.
        let cutoff = chrono::Utc::now() - chrono::Duration::days(days as i64);
        let before = cutoff.format("%d-%b-%Y").to_string();

        self.session
            .select("INBOX")
            .map_err(|e| Error::Imap(format!("select INBOX: {e}")))?;

        let uids = self
            .session
            .uid_search(format!("BEFORE {before} NOT FLAGGED"))
            .map_err(|e| Error::Imap(format!("retention search: {e}")))?;

        if uids.is_empty() {
            return Ok(0);
        }

        let uid_vec: Vec<u32> = uids.iter().copied().collect();
        let uid_set = parser::uid_set(&uid_vec);
        let count = uid_vec.len() as u32;

        if is_gmail {
            // Move to Trash so the copy in All Mail gets unlinked and
            // Gmail stops counting the message against quota.
            self.session
                .uid_mv(&uid_set, "[Gmail]/Trash")
                .map_err(|e| Error::Imap(format!("retention move: {e}")))?;
            info!(count, "gmail retention — moved INBOX to Trash");
        } else {
            self.session
                .uid_store(&uid_set, "+FLAGS (\\Deleted)")
                .map_err(|e| Error::Imap(format!("retention store: {e}")))?;
            self.session
                .expunge()
                .map_err(|e| Error::Imap(format!("retention expunge: {e}")))?;
            info!(count, "imap retention — expunged from INBOX");
        }

        Ok(count)
    }
}

fn open_tcp(host: &str, port: u16, bind_iface: Option<&str>) -> Result<TcpStream> {
    let tcp = crate::net::open_tcp(host, port, bind_iface)?;
    tcp.set_read_timeout(Some(Duration::from_secs(300))).ok();
    tcp.set_write_timeout(Some(Duration::from_secs(60))).ok();
    Ok(tcp)
}

/// Fast credential check: connect + LOGIN + LIST + LOGOUT with short
/// timeouts, so the UI can tell the user whether the mailbox is reachable
/// before we persist anything. Runs LIST because Gmail will happily
/// accept LOGIN with a stale/regular password and only tear the socket
/// when the first real command arrives — the "Connection Lost on list:"
/// symptom in the sync error toast. Making LIST part of the probe catches
/// that before a row hits the database.
///
/// Errors are returned as friendly strings (caller surfaces via 400),
/// not Error enums, because the taxonomy we want ("bad password", "IMAP
/// disabled", "network") doesn't map cleanly to existing variants.
pub fn probe(
    host: &str,
    port: u16,
    email: &str,
    password: &str,
    bind_iface: Option<&str>,
) -> std::result::Result<(), String> {
    let tcp = crate::net::open_tcp(host, port, bind_iface)
        .map_err(|e| format!("cannot reach {host}:{port} — {e}"))?;
    // Shorter than the sync-time budgets: users are waiting on the
    // probe response, so 20s per leg is plenty for a healthy server
    // and saves them from staring at a spinner for five minutes on a
    // dead host.
    tcp.set_read_timeout(Some(Duration::from_secs(20))).ok();
    tcp.set_write_timeout(Some(Duration::from_secs(20))).ok();

    let tls_conn = TlsConnector::builder()
        .build()
        .map_err(|e| format!("tls init: {e}"))?;
    let tls = tls_conn
        .connect(host, tcp)
        .map_err(|e| format!("tls handshake to {host}: {e}"))?;

    let client = imap::Client::new(tls);
    let mut session = client
        .login(email, password)
        .map_err(|(e, _)| classify_login_error(&e.to_string()))?;

    // LIST is the cheapest sanity check post-login; everything else
    // (SELECT, STATUS) requires a folder name we don't want to guess.
    session
        .list(None, Some("*"))
        .map_err(|e| classify_post_login_error(host, &e.to_string()))?;

    let _ = session.logout();
    Ok(())
}

fn classify_login_error(raw: &str) -> String {
    let lower = raw.to_ascii_lowercase();
    if lower.contains("authenticationfailed")
        || lower.contains("invalid credentials")
        || lower.contains("authentication failed")
        || lower.contains("login failed")
        || lower.contains("bad username")
    {
        "login rejected — check the password. Gmail/Outlook/iCloud require an \
         App Password, not your regular account password."
            .to_owned()
    } else if lower.contains("timed out") || lower.contains("timeout") {
        "login timed out — server is slow or unreachable".to_owned()
    } else {
        format!("login: {raw}")
    }
}

fn classify_post_login_error(host: &str, raw: &str) -> String {
    let lower = raw.to_ascii_lowercase();
    let is_gmail = host.eq_ignore_ascii_case("imap.gmail.com")
        || host.eq_ignore_ascii_case("imap.googlemail.com");
    if lower.contains("connection lost") || lower.contains("unexpected eof") {
        if is_gmail {
            "Gmail closed the connection right after login. Three common fixes: \
             (1) paste the 16-char App Password with no spaces, \
             (2) enable IMAP in Gmail settings (Forwarding and POP/IMAP → Enable IMAP), \
             (3) check myaccount.google.com/notifications for a blocked sign-in \
             and approve this device."
                .to_owned()
        } else {
            format!("{host} closed the connection after login — check IMAP is enabled \
                and the account isn't rate-limited")
        }
    } else if lower.contains("timed out") || lower.contains("timeout") {
        format!("{host} timed out listing folders — server slow or network flaky")
    } else {
        format!("list folders: {raw}")
    }
}

/// Pull the bare Message-ID (no angle brackets) out of a
/// `BODY[HEADER.FIELDS (MESSAGE-ID)]` response. Returns None when the
/// header is absent or malformed; the rescan path skips those messages
/// rather than assigning mystery labels.
fn extract_message_id(header: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(header).ok()?;
    for line in text.lines() {
        let mut parts = line.splitn(2, ':');
        let Some(name) = parts.next() else { continue };
        if !name.trim().eq_ignore_ascii_case("message-id") {
            continue;
        }
        let Some(value) = parts.next() else { continue };
        let trimmed = value
            .trim()
            .trim_start_matches('<')
            .trim_end_matches('>')
            .trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_owned());
        }
    }
    None
}

/// Infer the personal namespace (prefix + delimiter) from LIST
/// results. The delimiter comes straight from INBOX's LIST response —
/// servers are required to return it. The prefix comes from looking
/// at a non-INBOX mailbox's path and taking everything before the
/// first delimiter:
///
///   Dovecot:    INBOX.Sent   → prefix="INBOX", delim="."
///   Gmail:      [Gmail]/Sent → delim="/", prefix="" (no shared root)
///   Fastmail:   Sent         → delim=".", prefix=""
///
/// Any single-step failure returns None; the caller falls back to
/// the default ("", "/") which works for flat hierarchies.
fn probe_namespace(session: &mut Session<TlsStream<TcpStream>>) -> Option<Namespace> {
    let names = session.list(None, Some("*")).ok()?;

    // Delimiter: trust whatever LIST returns on the first name.
    let mut delimiter = None;
    for name in names.iter() {
        if let Some(d) = name.delimiter() {
            delimiter = Some(d.to_owned());
            break;
        }
    }
    let delimiter = delimiter?;

    // Prefix: if every non-INBOX mailbox starts with the same prefix
    // (e.g. "INBOX." on Dovecot), that's our personal-namespace root.
    // Otherwise the prefix is empty.
    let mut candidate: Option<String> = None;
    for name in names.iter() {
        let n = name.name();
        if n.eq_ignore_ascii_case("INBOX") {
            continue;
        }
        let Some((head, _tail)) = n.split_once(&delimiter) else {
            candidate = None;
            break;
        };
        match &candidate {
            None => candidate = Some(head.to_owned()),
            Some(prev) if prev == head => {}
            Some(_) => {
                candidate = None;
                break;
            }
        }
    }
    // Only accept non-empty prefixes if they're the classic INBOX root;
    // Gmail's `[Gmail]` isn't actually a prefix every folder shares.
    let prefix = match candidate {
        Some(p) if p.eq_ignore_ascii_case("INBOX") => p,
        _ => String::new(),
    };

    Some(Namespace { prefix, delimiter })
}

/// Logical folder role Postern cares about. The `ImapClient` resolves
/// these to real server paths at connect time via SPECIAL-USE + the
/// NAMESPACE prefix + a case-insensitive candidate walk.
#[derive(Debug, Clone, Copy)]
pub enum FolderRole {
    Sent,
    Drafts,
    Trash,
    Spam,
    Archive,
}

impl FolderRole {
    fn special_use_flag(self) -> &'static str {
        match self {
            FolderRole::Sent => "\\Sent",
            FolderRole::Drafts => "\\Drafts",
            FolderRole::Trash => "\\Trash",
            FolderRole::Spam => "\\Junk",
            FolderRole::Archive => "\\Archive",
        }
    }

    /// Common names a server is likely to use when SPECIAL-USE isn't
    /// advertised. Ordered roughly by convention — first match wins.
    /// Every candidate is compared case-insensitively and combined
    /// with every known namespace prefix.
    fn candidate_tails(self) -> &'static [&'static str] {
        match self {
            FolderRole::Sent => &["Sent", "Sent Mail", "Sent Messages", "Sent Items"],
            FolderRole::Drafts => &["Drafts", "Draft"],
            FolderRole::Trash => &[
                "Trash",
                "Deleted",
                "Deleted Items",
                "Deleted Messages",
                "Bin",
            ],
            FolderRole::Spam => &["Spam", "Junk", "Junk E-mail", "Junk Mail"],
            FolderRole::Archive => &["Archive", "Archives", "All Mail"],
        }
    }
}

impl ImapClient {
    /// Find the real server folder for a logical role. Prefers RFC
    /// 6154 SPECIAL-USE; falls back to a case-insensitive walk of
    /// candidate tails under each advertised namespace prefix; gives
    /// up with `Ok(None)` when nothing plausible exists so the caller
    /// can decide whether to CREATE the folder or skip.
    pub fn resolve_role_folder(&mut self, role: FolderRole) -> Result<Option<String>> {
        let names = self
            .session
            .list(None, Some("*"))
            .map_err(|e| Error::Imap(format!("list: {e}")))?;

        // 1. SPECIAL-USE — authoritative when advertised.
        let want_flag = role.special_use_flag();
        for name in names.iter() {
            let hit = name.attributes().iter().any(|a| match a {
                imap::types::NameAttribute::Custom(s) => s.eq_ignore_ascii_case(want_flag),
                _ => false,
            });
            if hit {
                return Ok(Some(name.name().to_owned()));
            }
        }

        // 2. Heuristic — match case-insensitively against known tails,
        //    considering each name's last segment. This works across
        //    hierarchies (`INBOX.Sent`, `INBOX/Sent`, `Sent`,
        //    `[Gmail]/Sent Mail`) without having to know the delimiter
        //    up front.
        let tails = role.candidate_tails();
        // We want the SHORTEST matching name (prefer `Sent` over
        // `Archive/Sent-2020`), so collect then sort.
        let mut matches: Vec<String> = Vec::new();
        for name in names.iter() {
            // Skip \Noselect containers — can't hold messages anyway.
            let no_select = name
                .attributes()
                .iter()
                .any(|a| matches!(a, imap::types::NameAttribute::NoSelect));
            if no_select {
                continue;
            }
            let full = name.name();
            // Split on any separator the server might use. LIST
            // actually tells us the delimiter per entry, but the
            // imap 2.4 crate's `Name` doesn't expose it — so compare
            // the last path component across both `.` and `/`.
            let last = full.rsplit_once(['.', '/']).map_or(full, |(_, r)| r);
            if tails.iter().any(|t| last.eq_ignore_ascii_case(t)) {
                matches.push(full.to_owned());
            }
        }
        matches.sort_by_key(|s| (s.len(), s.clone()));
        Ok(matches.into_iter().next())
    }
}

/// True when a folder name maps to "Spam" / "Junk" semantics. Used by
/// the trusted-sender rescue path to decide whether to inspect the
/// new arrivals' senders against the allowlist. Case-insensitive
/// because plain-IMAP servers vary on capitalisation, and matches
/// Gmail's bracketed namespace (`[Gmail]/Spam`).
pub fn is_spam_folder(folder: &str) -> bool {
    let lower = folder.to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "spam" | "junk" | "junk e-mail" | "junk mail" | "[gmail]/spam"
    )
}

fn classify_label(name: &str) -> &'static str {
    match name {
        "INBOX" | "Sent" | "Drafts" | "Spam" | "Trash" | "Archive" => "system",
        n if n.starts_with("[Gmail]/") => "system",
        n if n.starts_with("CATEGORY_") => "gmail_category",
        _ => "user",
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct FolderSyncReport {
    pub folder: String,
    pub new: u32,
    pub scanned: u32,
}
