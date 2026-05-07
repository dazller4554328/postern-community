use std::{sync::Arc, time::Duration};

use serde::Serialize;
use tokio::{
    sync::{mpsc, Mutex},
    task::JoinHandle,
    time,
};
use tracing::{error, info, instrument, warn};

use crate::{
    error::Result,
    storage::{Account, AccountKind, BlobStore, Db},
    sync::imap::{FolderSyncReport, ImapClient},
    vault::Vault,
    vpn::VpnManager,
};

/// Default polling interval. Overridden at runtime via
/// POST /api/settings/sync-interval.
const DEFAULT_POLL_SECS: u64 = 60;

/// Max folders synced per cycle per account. Set high enough to cover
/// Gmail categories (Promotions, Updates, Social, Forums) plus user
/// labels. If a mailbox has more than this, the remainder gets picked
/// up on the next cycle.
const FOLDERS_PER_CYCLE: usize = 25;

#[derive(Debug, Clone, Serialize)]
pub struct SyncReport {
    pub account_id: i64,
    pub folders: Vec<FolderSyncReport>,
    pub started_at: i64,
    pub finished_at: i64,
    pub error: Option<String>,
}

/// Shared mutable interval so the HTTP API can adjust without restart.
pub type SyncIntervalRef = Arc<std::sync::RwLock<u64>>;

#[derive(Clone)]
pub struct Scheduler {
    db: Arc<Db>,
    blobs: Arc<BlobStore>,
    vpn: VpnManager,
    vault: Vault,
    locks: Arc<Mutex<std::collections::HashMap<i64, Arc<Mutex<()>>>>>,
    tx: mpsc::Sender<i64>,
    last_report: Arc<Mutex<std::collections::HashMap<i64, SyncReport>>>,
    interval_secs: SyncIntervalRef,
    /// Per-account "skip Gmail purge until this unix-timestamp" map.
    /// Set when Gmail returns [THROTTLED]; regular folder sync keeps
    /// running normally. Cleared silently once the timestamp passes.
    /// Uses std::sync::Mutex (not tokio) because the read/write sites
    /// straddle sync_account_blocking's spawn_blocking boundary —
    /// tokio's async Mutex doesn't work from inside blocking code.
    purge_cooldowns: Arc<std::sync::Mutex<std::collections::HashMap<i64, i64>>>,
}

/// How long to back off category-purge for one account after hitting
/// Gmail throttling. Five minutes is enough to let their per-account
/// quota refresh without waiting so long that the user notices a
/// purge delay.
const PURGE_COOLDOWN_SECS: i64 = 300;

impl Scheduler {
    pub fn start(
        db: Arc<Db>,
        blobs: Arc<BlobStore>,
        vpn: VpnManager,
        vault: Vault,
    ) -> (Self, JoinHandle<()>) {
        let (tx, rx) = mpsc::channel::<i64>(64);
        let interval_secs = Arc::new(std::sync::RwLock::new(DEFAULT_POLL_SECS));
        let me = Self {
            db: db.clone(),
            blobs: blobs.clone(),
            vpn,
            vault,
            locks: Arc::new(Mutex::new(Default::default())),
            tx: tx.clone(),
            last_report: Arc::new(Mutex::new(Default::default())),
            interval_secs,
            purge_cooldowns: Arc::new(std::sync::Mutex::new(Default::default())),
        };

        let poll = me.clone();
        let handle = tokio::spawn(async move {
            poll.run(rx).await;
        });

        (me, handle)
    }

    pub async fn trigger(&self, account_id: i64) {
        if let Err(e) = self.tx.send(account_id).await {
            warn!(error = %e, account_id, "trigger channel closed");
        }
    }

    /// Clone of the trigger sender, for components that need to push
    /// account ids onto the same queue without holding a Scheduler.
    /// The IDLE supervisor uses this to wake a sync the moment its
    /// IDLE handle returns "mailbox changed".
    pub fn trigger_sender(&self) -> mpsc::Sender<i64> {
        self.tx.clone()
    }

    pub async fn last_report(&self, account_id: i64) -> Option<SyncReport> {
        self.last_report.lock().await.get(&account_id).cloned()
    }

    pub fn set_interval(&self, secs: u64) {
        let clamped = secs.clamp(30, 3600);
        if let Ok(mut v) = self.interval_secs.write() {
            *v = clamped;
        }
        info!(interval_secs = clamped, "sync interval updated");
    }

    pub fn get_interval(&self) -> u64 {
        self.interval_secs
            .read()
            .map(|v| *v)
            .unwrap_or(DEFAULT_POLL_SECS)
    }

    async fn run(self, mut rx: mpsc::Receiver<i64>) {
        let current_secs = *self.interval_secs.read().unwrap();
        let mut tick = time::interval(Duration::from_secs(current_secs));
        tick.tick().await;

        loop {
            // Re-check if interval changed; reset the tick if so.
            let desired = *self.interval_secs.read().unwrap();
            if tick.period() != Duration::from_secs(desired) {
                tick = time::interval(Duration::from_secs(desired));
                tick.tick().await;
            }

            tokio::select! {
                _ = tick.tick() => {
                    match self.db.list_accounts() {
                        Ok(accounts) => {
                            // Drop disabled accounts before batching — a
                            // paused mailbox contributes nothing to the
                            // Gmail-connection count either way.
                            let accounts: Vec<_> = accounts
                                .into_iter()
                                .filter(|a| a.sync_enabled)
                                .collect();
                            // Sync in batches of 3 to avoid hitting Gmail's
                            // per-IP IMAP connection limit (~15). A 2s gap
                            // between batches keeps us well under the threshold.
                            const BATCH: usize = 3;
                            for chunk in accounts.chunks(BATCH) {
                                let futs: Vec<_> = chunk
                                    .iter()
                                    .map(|a| {
                                        let me = self.clone();
                                        let a = a.clone();
                                        tokio::spawn(async move { me.run_sync(a).await })
                                    })
                                    .collect();
                                for f in futs {
                                    let _ = f.await;
                                }
                                time::sleep(Duration::from_secs(2)).await;
                            }
                        }
                        Err(e) => error!(error = %e, "list accounts failed"),
                    }
                }
                Some(id) = rx.recv() => {
                    match self.db.get_account(id) {
                        Ok(a) if !a.sync_enabled => {
                            info!(account_id = id, email = %a.email, "trigger skipped: sync disabled");
                        }
                        Ok(a) => self.run_sync(a).await,
                        Err(e) => warn!(error = %e, account_id = id, "skip trigger"),
                    }
                }
                else => break,
            }
        }
    }

    #[instrument(skip(self), fields(account_id = account.id, email = %account.email))]
    async fn run_sync(&self, account: Account) {
        let lock = {
            let mut locks = self.locks.lock().await;
            locks
                .entry(account.id)
                .or_insert_with(|| Arc::new(Mutex::new(())))
                .clone()
        };
        let Ok(_guard) = lock.try_lock_owned() else {
            info!("sync already running, skipping");
            return;
        };

        let started_at = chrono::Utc::now().timestamp();
        let _ = self.db.log_activity("sync_started", Some(&account.email));

        let result = self.sync_account_blocking(account.clone()).await;
        let finished_at = chrono::Utc::now().timestamp();
        let duration = finished_at - started_at;

        let report = match result {
            Ok(folders) => {
                let total_new: u32 = folders.iter().map(|f| f.new).sum();
                let total_scanned: u32 = folders.iter().map(|f| f.scanned).sum();
                let detail = format!(
                    "{} — {} folders, {} new, {} scanned in {}s",
                    account.email,
                    folders.len(),
                    total_new,
                    total_scanned,
                    duration
                );
                let _ = self.db.log_activity("sync_completed", Some(&detail));

                // Run auto-archive AFTER a successful sync so we're
                // working against a fresh inbox snapshot. Skipped when
                // the account has archive disabled or auto-archive off.
                if crate::tier::ALLOW_AUTO_ARCHIVE
                    && account.archive_enabled
                    && account.auto_archive_enabled
                {
                    if let Err(e) = self.run_auto_archive(account.clone()).await {
                        warn!(error = %e, "auto-archive failed");
                        let _ = self.db.log_activity(
                            "auto_archive_error",
                            Some(&format!("{}: {}", account.email, e)),
                        );
                    }
                }

                SyncReport {
                    account_id: account.id,
                    folders,
                    started_at,
                    finished_at,
                    error: None,
                }
            }
            Err(e) => {
                warn!(error = %e, "sync failed");
                let detail = format!("{}: {}", account.email, e);
                let _ = self.db.log_activity("sync_error", Some(&detail));
                SyncReport {
                    account_id: account.id,
                    folders: vec![],
                    started_at,
                    finished_at,
                    error: Some(e.to_string()),
                }
            }
        };
        self.last_report.lock().await.insert(account.id, report);
    }

    async fn run_auto_archive(&self, account: Account) -> Result<()> {
        self.vault.require_unlocked()?;
        let password = self.db.account_password(account.id, &self.vault)?;
        let db = self.db.clone();
        let bind_iface = self.vpn.bind_iface();
        let account_email = account.email.clone();

        let outcome = tokio::task::spawn_blocking(move || {
            crate::sync::auto_archive::run(&account, &password, bind_iface.as_deref(), &db)
        })
        .await
        .map_err(|e| crate::error::Error::Other(anyhow::anyhow!("join: {e}")))??;

        if outcome.moved > 0 || outcome.failed > 0 {
            let detail = format!(
                "{}: {} moved, {} failed (of {} eligible this cycle)",
                account_email, outcome.moved, outcome.failed, outcome.eligible
            );
            let _ = self
                .db
                .log_activity("auto_archive_completed", Some(&detail));
        }
        Ok(())
    }

    async fn sync_account_blocking(&self, account: Account) -> Result<Vec<FolderSyncReport>> {
        // Don't hammer the IMAP server when we can't decrypt credentials.
        // Returning Locked just pauses the scheduler for this cycle.
        self.vault.require_unlocked()?;
        let password = self.db.account_password(account.id, &self.vault)?;
        let db = self.db.clone();
        let blobs = self.blobs.clone();
        let vault = self.vault.clone();
        let purge_cooldowns = self.purge_cooldowns.clone();
        // Snapshot the VPN state at sync start. If the tunnel is up we bind
        // IMAP to wg0; if it goes down mid-sync the socket just fails and
        // the next tick picks up where we left off.
        let bind_iface = self.vpn.bind_iface();
        tokio::task::spawn_blocking(move || {
            let mut client = ImapClient::connect(
                &account.imap_host,
                account.imap_port,
                &account.email,
                &password,
                bind_iface.as_deref(),
            )?;
            let is_gmail = account.kind == AccountKind::Gmail;
            let folders = client.folders(is_gmail)?;
            // Cap folders per cycle so big mailboxes still make progress.
            // FOLDERS_PER_CYCLE is tuned to cover Gmail categories
            // (Promotions, Updates, Social, Forums) plus user labels.
            let mut reports = Vec::new();
            for name in folders.iter().take(FOLDERS_PER_CYCLE) {
                match client.sync_folder(&account, name, &db, &blobs, &vault) {
                    Ok(r) => reports.push(r),
                    Err(e) => {
                        tracing::warn!(folder = %name, error = %e, "folder sync failed");
                        let detail = format!("{} / {}: {}", account.email, name, e);
                        let _ = db.log_activity("folder_sync_error", Some(&detail));
                    }
                }
            }

            // Nuclear-option category purge. Gmail-only, and only
            // when the user has *also* opted into delete_after_sync —
            // this is the explicit "download + delete" mode. The
            // call is best-effort; failure is logged but doesn't
            // fail the sync report.
            if crate::tier::ALLOW_GMAIL_CATEGORIES_PURGE
                && is_gmail
                && account.delete_after_sync
                && account.purge_gmail_categories
            {
                // Skip the purge entirely if we've recently hit Gmail
                // throttling for this account. Regular folder sync is
                // unaffected — only the rescan loop pauses.
                let now = chrono::Utc::now().timestamp();
                let skip_purge = {
                    let mut cd = purge_cooldowns.lock().expect("purge cooldown lock poisoned");
                    match cd.get(&account.id).copied() {
                        Some(until) if until > now => true,
                        Some(_) => {
                            cd.remove(&account.id);
                            false
                        }
                        None => false,
                    }
                };

                if !skip_purge {
                    // Password clone because the existing connection
                    // above is dropped at end of this block; the purge
                    // opens a fresh TLS socket via its own client.
                    match crate::sync::gmail_rescan::purge_categories(
                        &account.imap_host,
                        account.imap_port,
                        &account.email,
                        &password,
                        &account,
                        &db,
                        &blobs,
                        &vault,
                        bind_iface.as_deref(),
                    ) {
                        Ok((0, 0)) => {}
                        Ok((d, m)) => {
                            let detail = format!(
                                "{}: category purge — downloaded {} new, moved {} to Trash",
                                account.email, d, m
                            );
                            tracing::info!(target: "purge", "{detail}");
                            let _ = db.log_activity("gmail_categories_purged", Some(&detail));
                        }
                        Err(e) => {
                            if crate::sync::gmail_rescan::is_throttled_error(&e) {
                                // Gmail is rate-limiting. Back off for
                                // a few minutes so we don't keep
                                // feeding the throttle counter.
                                purge_cooldowns
                                    .lock()
                                    .expect("purge cooldown lock poisoned")
                                    .insert(account.id, now + PURGE_COOLDOWN_SECS);
                                tracing::info!(
                                    email = %account.email,
                                    cooldown_secs = PURGE_COOLDOWN_SECS,
                                    "category purge throttled by Gmail, pausing purge cycle"
                                );
                            } else {
                                tracing::warn!(error = %e, email = %account.email, "category purge failed");
                                let detail =
                                    format!("{}: category purge failed: {}", account.email, e);
                                let _ = db
                                    .log_activity("gmail_categories_purge_error", Some(&detail));
                            }
                        }
                    }
                }
            }

            // Retention sweep runs after sync so we've just pulled any
            // messages the server still holds. Any failure is logged but
            // doesn't poison the sync report — next cycle retries.
            if crate::tier::ALLOW_SERVER_RETENTION && account.retention_enabled {
                match client.sweep_retention_inbox(is_gmail, account.retention_days) {
                    Ok(0) => {}
                    Ok(n) => {
                        let detail = format!(
                            "{}: {} messages older than {}d removed from INBOX",
                            account.email, n, account.retention_days
                        );
                        tracing::info!(target: "retention", "{detail}");
                        let _ = db.log_activity("retention_swept", Some(&detail));
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, email = %account.email, "retention sweep failed");
                        let detail = format!("{}: retention sweep failed: {}", account.email, e);
                        let _ = db.log_activity("retention_error", Some(&detail));
                    }
                }
            }

            client.logout();
            Ok(reports)
        })
        .await
        .map_err(|e| crate::error::Error::Other(anyhow::anyhow!("join: {e}")))?
    }
}
