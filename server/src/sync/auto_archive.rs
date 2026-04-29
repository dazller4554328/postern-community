//! Age-based auto-archive — sweep old INBOX messages into the
//! per-account Archive folder, respecting the configured strategy.
//!
//! Runs synchronously inside a tokio::task::spawn_blocking so it can
//! own its IMAP connection. Scope is capped per cycle (MAX_PER_RUN) so
//! big backlogs get processed over multiple scheduler ticks instead of
//! pinning the IMAP session.

use tracing::{info, warn};

use crate::{
    error::Result,
    storage::{Account, Db},
    sync::ImapClient,
};

/// Don't move more than this per cycle. The rest gets picked up next tick.
const MAX_PER_RUN: i64 = 200;

#[derive(Debug, Default, Clone, Copy)]
pub struct AutoArchiveOutcome {
    pub eligible: i64,
    pub moved: u32,
    pub failed: u32,
}

pub fn run(
    account: &Account,
    password: &str,
    bind_iface: Option<&str>,
    db: &Db,
) -> Result<AutoArchiveOutcome> {
    if !account.archive_enabled || !account.auto_archive_enabled {
        return Ok(AutoArchiveOutcome::default());
    }

    let cutoff =
        chrono::Utc::now().timestamp() - (account.auto_archive_age_days.max(1) as i64) * 86_400;
    let base = account.archive_folder_base().to_owned();
    let candidates = db.list_auto_archive_candidates(
        account.id,
        cutoff,
        account.auto_archive_read_only,
        &base,
        MAX_PER_RUN,
    )?;

    if candidates.is_empty() {
        return Ok(AutoArchiveOutcome::default());
    }

    let mut client = ImapClient::connect(
        &account.imap_host,
        account.imap_port,
        &account.email,
        password,
        bind_iface,
    )?;

    let mut outcome = AutoArchiveOutcome {
        eligible: candidates.len() as i64,
        moved: 0,
        failed: 0,
    };

    for c in candidates {
        let target = account.archive_folder_for(c.date_utc);
        if let Err(e) = client.ensure_folder(&target) {
            warn!(account = %account.email, folder = %target, error = %e, "ensure_folder failed");
            outcome.failed += 1;
            continue;
        }
        match client.move_message(&c.message_id, "INBOX", &target) {
            Ok(true) => {
                outcome.moved += 1;
                // Mirror the server move locally — relabel drops INBOX
                // and sets the archive bucket label so the UI shows the
                // same state without waiting for the next IMAP sync.
                if let Err(e) = db.relabel_message(c.id, account.id, &[target.as_str()]) {
                    warn!(msg_id = c.id, error = %e, "local relabel failed");
                }
            }
            Ok(false) => {
                // Message-ID wasn't found in INBOX on the server — likely
                // already moved by another client or rule. Skip quietly.
                outcome.failed += 1;
            }
            Err(e) => {
                warn!(msg_id = c.id, error = %e, "imap move failed");
                outcome.failed += 1;
            }
        }
    }

    client.logout();

    info!(
        account = %account.email,
        moved = outcome.moved,
        failed = outcome.failed,
        "auto-archive cycle done"
    );
    Ok(outcome)
}
