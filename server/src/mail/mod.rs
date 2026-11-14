//! Mail-server orchestration above the raw `sync::ImapClient`.
//!
//! HTTP handlers used to open IMAP sessions inline — connecting,
//! logging in, ensuring folders, moving messages, logging out — all
//! mixed in with vault checks, response shaping, and local DB
//! updates. This module holds the IMAP plumbing so the handlers can
//! stay focused on HTTP-shape concerns.
//!
//! Functions here are fire-and-forget by convention: the caller has
//! already updated local state optimistically, and the IMAP move /
//! expunge runs in the background. Failures are logged but not
//! propagated; the next sync reconciles divergence.

use crate::{
    error::Result,
    storage::{Account, AccountKind},
    sync::ImapClient,
    vpn::VpnManager,
};

/// Move a single message between folders on the IMAP server.
/// Optimistic local DB update is the caller's responsibility; this
/// only handles the remote side. Spawns a background task and
/// returns immediately so the HTTP request stays fast.
///
/// `ensure_folder` should be true when the destination might not
/// exist yet (e.g. archive bucketing creates `Archive/2026/03` on
/// the first archive of the month).
pub fn spawn_move(
    vpn: VpnManager,
    account: Account,
    password: String,
    msg_id: i64,
    message_id: String,
    from_folder: String,
    to_folder: String,
    ensure_folder: bool,
    jobs: MoveJobs,
) {
    let account_id = account.id;
    jobs.inc(account_id);
    tokio::task::spawn(async move {
        let bind_iface = vpn.bind_iface();
        let to_folder_for_log = to_folder.clone();
        let result = tokio::task::spawn_blocking(move || -> Result<bool> {
            let mut client = ImapClient::connect(
                &account.imap_host,
                account.imap_port,
                &account.email,
                &password,
                bind_iface.as_deref(),
            )?;
            if ensure_folder {
                if let Err(e) = client.ensure_folder(&to_folder) {
                    tracing::warn!(folder = %to_folder, error = %e, "failed to ensure destination folder");
                }
            }
            let moved = client.move_message(&message_id, &from_folder, &to_folder)?;
            client.logout();
            Ok(moved)
        })
        .await;
        match result {
            Ok(Ok(true)) => tracing::info!(msg_id, to = %to_folder_for_log, "imap move succeeded"),
            Ok(Ok(false)) => tracing::warn!(
                msg_id,
                to = %to_folder_for_log,
                "imap move: message not found in source folder"
            ),
            Ok(Err(e)) => tracing::warn!(msg_id, error = %e, "imap move failed"),
            Err(e) => tracing::warn!(msg_id, error = %e, "imap move task panicked"),
        }
        jobs.dec(account_id);
    });
}

/// In-flight IMAP MOVE registry, keyed by `account_id`. Used by
/// empty-folder to wait for any pending bulk-trash MOVEs to land
/// before purging — otherwise the user trashes a message, hits
/// Empty Trash before the server finishes the MOVE, the local row
/// is deleted prematurely, and the next sync re-imports the message
/// from server-Trash. This isn't a queue (we don't reorder work),
/// just a per-account counter the caller can poll.
#[derive(Default, Clone)]
pub struct MoveJobs {
    inner: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<i64, u32>>>,
}

impl MoveJobs {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn inc(&self, account_id: i64) {
        let mut map = self.inner.lock().expect("move_jobs lock poisoned");
        *map.entry(account_id).or_insert(0) += 1;
    }

    pub fn dec(&self, account_id: i64) {
        let mut map = self.inner.lock().expect("move_jobs lock poisoned");
        if let Some(c) = map.get_mut(&account_id) {
            *c = c.saturating_sub(1);
            if *c == 0 {
                map.remove(&account_id);
            }
        }
    }

    pub fn pending(&self, account_id: i64) -> u32 {
        self.inner
            .lock()
            .expect("move_jobs lock poisoned")
            .get(&account_id)
            .copied()
            .unwrap_or(0)
    }

    /// Block until pending count for this account hits zero, or `timeout`
    /// elapses. Returns true if the queue drained, false on timeout —
    /// the caller decides whether to proceed anyway (empty-folder does:
    /// a partial wait is better than no wait, and the IMAP-side expunge
    /// will catch stragglers on the next user action either way).
    pub async fn await_idle(&self, account_id: i64, timeout: std::time::Duration) -> bool {
        let start = std::time::Instant::now();
        loop {
            if self.pending(account_id) == 0 {
                return true;
            }
            if start.elapsed() >= timeout {
                return false;
            }
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        }
    }
}

/// Flag every message in `folder` as \Deleted and EXPUNGE. Drives
/// the empty-folder path; after this call the remote folder is
/// empty (modulo races with concurrent IMAP clients).
pub fn spawn_expunge(vpn: VpnManager, account: Account, password: String, folder: String) {
    let folder_for_log = folder.clone();
    tokio::task::spawn(async move {
        let result = tokio::task::spawn_blocking(move || -> Result<()> {
            let bind_iface = vpn.bind_iface();
            let mut client = ImapClient::connect(
                &account.imap_host,
                account.imap_port,
                &account.email,
                &password,
                bind_iface.as_deref(),
            )?;
            client.select_and_purge(&folder)?;
            client.logout();
            Ok(())
        })
        .await;
        match result {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                tracing::warn!(folder = %folder_for_log, error = %e, "empty-folder: imap expunge failed; remote copies linger until next sync");
            }
            Err(e) => {
                tracing::warn!(folder = %folder_for_log, error = %e, "empty-folder: blocking task panicked");
            }
        }
    });
}

/// Compute the source folder, destination folder, and new local
/// label set for a smart-move target (`spam` / `trash` / `archive` /
/// other). The source-folder guess comes from the message's current
/// labels; if we guess wrong the IMAP Message-ID search just returns
/// empty and the move no-ops rather than misdelivering.
pub fn resolve_smart_move(
    target: &str,
    account: &Account,
    current_labels: &[String],
    message_date_utc: i64,
) -> (String, String, Vec<String>) {
    let is_gmail = account.kind == AccountKind::Gmail;
    let spam = if is_gmail { "[Gmail]/Spam" } else { "Spam" };
    let trash = if is_gmail { "[Gmail]/Trash" } else { "Trash" };
    let archive_base = account.archive_folder_base().to_owned();
    let archive_target = account.archive_folder_for(message_date_utc);

    // Pick the most specific "from" folder we can tell from current labels.
    // Worst case we aim at INBOX — IMAP search is forgiving, it just won't
    // find the UID and the move no-ops rather than misdelivering.
    let from = if current_labels.iter().any(|l| l == spam || l == "Junk") {
        spam.to_string()
    } else if current_labels.iter().any(|l| l == trash) {
        trash.to_string()
    } else if current_labels.iter().any(|l| l.starts_with(&archive_base)) {
        // Already archived — use whichever archive label we see first.
        current_labels
            .iter()
            .find(|l| l.starts_with(&archive_base))
            .cloned()
            .unwrap_or_else(|| archive_base.clone())
    } else {
        "INBOX".to_string()
    };

    match target {
        "spam" => (from, spam.to_string(), vec![spam.to_string()]),
        "trash" => (from, trash.to_string(), vec![trash.to_string()]),
        "archive" => (from, archive_target.clone(), vec![archive_target]),
        _ => (from, "INBOX".to_string(), vec!["INBOX".to_string()]),
    }
}

/// Whether a smart-move target's destination needs `ensure_folder`
/// before the move. Archive buckets by date (Archive/2026/03) and
/// the bucket may not exist server-side yet; spam / trash / other
/// land in well-known mailboxes.
pub fn smart_move_needs_ensure(target: &str) -> bool {
    target == "archive"
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::ArchiveStrategy;

    /// Build an Account with just the fields `resolve_smart_move`
    /// reads. The vast majority of fields don't affect routing —
    /// this keeps test cases readable.
    fn account(kind: AccountKind) -> Account {
        Account {
            id: 1,
            kind,
            email: "user@example.com".into(),
            display_name: None,
            imap_host: "imap.example.com".into(),
            imap_port: 993,
            smtp_host: None,
            smtp_port: None,
            vpn_required: false,
            delete_after_sync: false,
            created_at: 0,
            archive_folder: None,
            archive_strategy: ArchiveStrategy::Monthly,
            archive_enabled: true,
            auto_archive_enabled: false,
            auto_archive_age_days: 30,
            auto_archive_read_only: false,
            avatar_seed: None,
            avatar_set: "set1".into(),
            retention_enabled: false,
            retention_days: 0,
            purge_gmail_categories: false,
            skip_gmail_trash: false,
            signature_html: None,
            signature_plain: None,
            sync_enabled: true,
            send_enabled: true,
            include_in_unified: true,
            color: None,
        }
    }

    // Mid-March 2026 — picks a clearly bucketed Monthly target so
    // the year/month substring is testable independent of "today".
    const MARCH_2026: i64 = 1_773_316_800; // 2026-03-12T12:00:00Z

    // ── Spam target ──────────────────────────────────────────────────

    #[test]
    fn imap_spam_routes_to_plain_spam() {
        let acc = account(AccountKind::Imap);
        let (from, to, labels) = resolve_smart_move("spam", &acc, &["INBOX".into()], MARCH_2026);
        assert_eq!(from, "INBOX");
        assert_eq!(to, "Spam");
        assert_eq!(labels, vec!["Spam"]);
    }

    #[test]
    fn gmail_spam_uses_bracketed_namespace() {
        let acc = account(AccountKind::Gmail);
        let (_from, to, labels) = resolve_smart_move("spam", &acc, &["INBOX".into()], MARCH_2026);
        assert_eq!(to, "[Gmail]/Spam");
        assert_eq!(labels, vec!["[Gmail]/Spam"]);
    }

    // ── Trash target ─────────────────────────────────────────────────

    #[test]
    fn imap_trash_routes_to_plain_trash() {
        let acc = account(AccountKind::Imap);
        let (_, to, labels) = resolve_smart_move("trash", &acc, &["INBOX".into()], MARCH_2026);
        assert_eq!(to, "Trash");
        assert_eq!(labels, vec!["Trash"]);
    }

    #[test]
    fn gmail_trash_uses_bracketed_namespace() {
        let acc = account(AccountKind::Gmail);
        let (_, to, _) = resolve_smart_move("trash", &acc, &["INBOX".into()], MARCH_2026);
        assert_eq!(to, "[Gmail]/Trash");
    }

    // ── Archive target — the bucketing path ──────────────────────────

    #[test]
    fn archive_monthly_buckets_by_year_month() {
        let acc = account(AccountKind::Imap);
        let (_, to, labels) = resolve_smart_move("archive", &acc, &["INBOX".into()], MARCH_2026);
        assert_eq!(to, "Archive/2026/03");
        assert_eq!(labels, vec!["Archive/2026/03"]);
    }

    #[test]
    fn archive_yearly_buckets_by_year_only() {
        let mut acc = account(AccountKind::Imap);
        acc.archive_strategy = ArchiveStrategy::Yearly;
        let (_, to, _) = resolve_smart_move("archive", &acc, &["INBOX".into()], MARCH_2026);
        assert_eq!(to, "Archive/2026");
    }

    #[test]
    fn archive_single_uses_flat_archive() {
        let mut acc = account(AccountKind::Imap);
        acc.archive_strategy = ArchiveStrategy::Single;
        let (_, to, _) = resolve_smart_move("archive", &acc, &["INBOX".into()], MARCH_2026);
        assert_eq!(to, "Archive");
    }

    #[test]
    fn archive_honours_user_archive_folder_override() {
        let mut acc = account(AccountKind::Imap);
        acc.archive_folder = Some("My Archive".into());
        let (_, to, _) = resolve_smart_move("archive", &acc, &["INBOX".into()], MARCH_2026);
        assert!(to.starts_with("My Archive/"));
    }

    // ── Source-folder inference ──────────────────────────────────────

    #[test]
    fn source_inferred_from_spam_label() {
        let acc = account(AccountKind::Imap);
        let labels = vec!["Spam".into()];
        let (from, _, _) = resolve_smart_move("trash", &acc, &labels, MARCH_2026);
        assert_eq!(from, "Spam");
    }

    #[test]
    fn source_inferred_from_junk_legacy_label() {
        // "Junk" maps to spam-source on plain IMAP — Outlook-era servers
        // historically used that name.
        let acc = account(AccountKind::Imap);
        let labels = vec!["Junk".into()];
        let (from, _, _) = resolve_smart_move("trash", &acc, &labels, MARCH_2026);
        assert_eq!(from, "Spam");
    }

    #[test]
    fn source_inferred_from_trash_label() {
        let acc = account(AccountKind::Imap);
        let labels = vec!["Trash".into()];
        let (from, _, _) = resolve_smart_move("spam", &acc, &labels, MARCH_2026);
        assert_eq!(from, "Trash");
    }

    /// When the message is already under any archive bucket
    /// (Archive/2026 or Archive/2026/03), the source should be
    /// that exact bucket — the IMAP move targets need the precise
    /// folder, not the base. Getting this wrong = move searches the
    /// wrong folder = move silently no-ops.
    #[test]
    fn source_inferred_from_existing_archive_bucket() {
        let acc = account(AccountKind::Imap);
        let labels = vec!["Archive/2025/11".into()];
        let (from, _, _) = resolve_smart_move("spam", &acc, &labels, MARCH_2026);
        assert_eq!(from, "Archive/2025/11");
    }

    /// No identifiable label → fall back to INBOX. The IMAP search
    /// is forgiving (returns empty rather than misdelivering) but
    /// INBOX is the right default for the common case.
    #[test]
    fn source_falls_back_to_inbox_when_no_match() {
        let acc = account(AccountKind::Imap);
        let (from, _, _) = resolve_smart_move("spam", &acc, &["UserLabel".into()], MARCH_2026);
        assert_eq!(from, "INBOX");
    }

    // ── Unknown target ───────────────────────────────────────────────

    #[test]
    fn unknown_target_routes_to_inbox() {
        let acc = account(AccountKind::Imap);
        let (_, to, labels) = resolve_smart_move("???", &acc, &["INBOX".into()], MARCH_2026);
        assert_eq!(to, "INBOX");
        assert_eq!(labels, vec!["INBOX"]);
    }

    // ── smart_move_needs_ensure ──────────────────────────────────────

    #[test]
    fn ensure_folder_only_needed_for_archive() {
        assert!(smart_move_needs_ensure("archive"));
        assert!(!smart_move_needs_ensure("spam"));
        assert!(!smart_move_needs_ensure("trash"));
        assert!(!smart_move_needs_ensure(""));
    }
}
