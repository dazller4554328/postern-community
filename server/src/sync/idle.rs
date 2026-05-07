//! IMAP IDLE supervisor — one persistent task per account that watches
//! INBOX for server-side changes and triggers an immediate sync when
//! anything moves. Replaces "wait up to 60 seconds for the next poll
//! tick" with "wait until the server tells us something happened",
//! which closes the latency gap between Postern and push-IMAP clients
//! like BlueMail.
//!
//! Design constraints:
//!
//! - The sync `imap` crate's IDLE handle is sync + blocking, so the
//!   per-account loop runs on a `spawn_blocking` task. Tokio is the
//!   coordinator: it owns the cancellation channel, the trigger
//!   sender, and the task join handles.
//!
//! - Servers that don't advertise the IDLE capability fall back to the
//!   polling scheduler. We check capabilities once at connect time;
//!   capability stability is fine because servers don't downgrade
//!   their IDLE support mid-session.
//!
//! - We re-issue IDLE every ~25 minutes to stay well under the
//!   RFC-2177 29-minute server timeout. The crate's `wait_keepalive`
//!   would handle that internally, but going through `idle_inbox`
//!   with a bounded timeout lets us also bail out cleanly when the
//!   vault locks (we drop the loop and the next unlock re-spawns).
//!
//! - Reconnect is exponential-backoff up to 5 minutes. A flapping
//!   server doesn't become a CPU hog; a recovering server gets reach
//!   within seconds of the next backoff window.

use std::{sync::Arc, time::Duration};

use tokio::{
    sync::{mpsc, Mutex},
    task::JoinHandle,
};
use tracing::{debug, info, warn};

use imap::extensions::idle::WaitOutcome;

use crate::{
    storage::{Account, Db},
    sync::imap::ImapClient,
    vault::Vault,
    vpn::VpnManager,
};

/// Result of one IDLE cycle, surfaced from the blocking task to the
/// async supervisor so it can decide whether to log+trigger a sync
/// (`Pushed`), quietly re-IDLE (`KeepaliveExpired`), or fall back to
/// long sleeps because the server doesn't speak IDLE (`Unsupported`).
enum IdleResult {
    Pushed,
    KeepaliveExpired,
    Unsupported,
}

/// Backoff bounds: start at 2s, double up to 5 minutes. We never sit
/// silently waiting longer than five minutes after a failure — at
/// that point the polling fallback would have already covered the
/// gap, but a quick retry still buys us push-style latency once the
/// server recovers.
const BACKOFF_MIN: Duration = Duration::from_secs(2);
const BACKOFF_MAX: Duration = Duration::from_secs(300);

/// Per-IDLE keepalive. Real-world push clients (BlueMail, K-9,
/// Thunderbird) re-issue IDLE every 5–9 minutes rather than the
/// RFC-2177 ceiling of 29 because Gmail (and NATs) silently half-close
/// sockets long before the 29-minute mark. With a 5-minute window,
/// a half-open IDLE that's missing notifications gets reaped within
/// 5 minutes of going dead instead of 25, and the next reconnect
/// catches up via a fresh sync trigger.
const KEEPALIVE: Duration = Duration::from_secs(5 * 60);

#[derive(Clone)]
pub struct IdleSupervisor {
    db: Arc<Db>,
    vpn: VpnManager,
    vault: Vault,
    /// Sends account-ids into the polling scheduler's trigger
    /// channel. Each IDLE wakeup pushes one id, the scheduler runs a
    /// regular sync (folder list, fetch, etc) — same code path as a
    /// manual "Sync now" click. Reusing it avoids duplicating the
    /// fetch logic and keeps the IDLE path narrowly responsible for
    /// "tell the scheduler something happened".
    trigger: mpsc::Sender<i64>,
    /// account_id → JoinHandle for the per-account IDLE task.
    /// Tracked so a stop() request can abort cleanly and the
    /// supervisor doesn't leak handles when accounts are re-keyed.
    handles: Arc<Mutex<std::collections::HashMap<i64, JoinHandle<()>>>>,
}

impl IdleSupervisor {
    pub fn new(
        db: Arc<Db>,
        vpn: VpnManager,
        vault: Vault,
        trigger: mpsc::Sender<i64>,
    ) -> Self {
        Self {
            db,
            vpn,
            vault,
            trigger,
            handles: Arc::new(Mutex::new(Default::default())),
        }
    }

    /// Start watchers for every sync-enabled account. Idempotent —
    /// re-starting an account that's already running is a no-op.
    /// Called once at boot and again whenever the vault unlocks (so
    /// fresh credentials get picked up).
    pub async fn refresh(&self) {
        let accounts = match self.db.list_accounts() {
            Ok(a) => a,
            Err(e) => {
                warn!(error = %e, "idle: list_accounts failed");
                return;
            }
        };
        let mut handles = self.handles.lock().await;
        // Drop watchers for accounts that have been deleted or
        // disabled since the last refresh.
        let live_ids: std::collections::HashSet<i64> = accounts
            .iter()
            .filter(|a| a.sync_enabled)
            .map(|a| a.id)
            .collect();
        let stale: Vec<i64> = handles
            .keys()
            .copied()
            .filter(|id| !live_ids.contains(id))
            .collect();
        for id in stale {
            if let Some(h) = handles.remove(&id) {
                h.abort();
                info!(account_id = id, "idle: stopped watcher");
            }
        }
        // Spawn watchers for accounts that don't yet have one.
        for account in accounts.into_iter().filter(|a| a.sync_enabled) {
            if handles.contains_key(&account.id) {
                continue;
            }
            let me = self.clone();
            let id = account.id;
            let handle = tokio::spawn(async move { me.run_account(account).await });
            handles.insert(id, handle);
            info!(account_id = id, "idle: started watcher");
        }
    }

    /// Cancel every watcher. Used at vault-lock so we stop holding
    /// long-lived authenticated sockets to upstream servers; the next
    /// unlock calls refresh() to re-spawn.
    #[allow(dead_code)]
    pub async fn stop_all(&self) {
        let mut handles = self.handles.lock().await;
        for (_, h) in handles.drain() {
            h.abort();
        }
        info!("idle: all watchers stopped");
    }

    /// Spawn a long-lived task that polls the account list every
    /// `period` and reconciles watchers. Keeps the supervisor self-
    /// healing — newly added accounts pick up an IDLE listener
    /// within `period`, and deleted/disabled accounts have their
    /// listeners reaped on the same cadence. Returns the JoinHandle
    /// for shutdown.
    pub fn spawn_reconciler(self, period: Duration) -> JoinHandle<()> {
        tokio::spawn(async move {
            // Initial pass — gives accounts that exist at boot a
            // listener as soon as the vault is unlocked.
            self.refresh().await;
            let mut tick = tokio::time::interval(period);
            tick.tick().await;
            loop {
                tick.tick().await;
                self.refresh().await;
            }
        })
    }

    async fn run_account(self, account: Account) {
        let mut backoff = BACKOFF_MIN;
        loop {
            // The vault's password decryption requires it to be
            // unlocked. If we're locked, sit tight in a short loop —
            // the vault unlock path calls refresh() which doesn't
            // disturb us, but a re-lock would have aborted us anyway.
            if !self.vault.is_unlocked() {
                tokio::time::sleep(Duration::from_secs(15)).await;
                continue;
            }
            match self.idle_once(&account).await {
                Ok(IdleResult::Pushed) => {
                    info!(
                        account_id = account.id,
                        email = %account.email,
                        "idle: mailbox changed, triggering sync"
                    );
                    if let Err(e) = self.trigger.send(account.id).await {
                        warn!(account_id = account.id, error = %e, "idle: trigger send failed");
                    }
                    backoff = BACKOFF_MIN;
                }
                Ok(IdleResult::KeepaliveExpired) => {
                    debug!(
                        account_id = account.id,
                        email = %account.email,
                        "idle: keepalive expired, re-issuing"
                    );
                    backoff = BACKOFF_MIN;
                }
                Ok(IdleResult::Unsupported) => {
                    backoff = BACKOFF_MIN;
                }
                Err(e) => {
                    warn!(
                        account_id = account.id,
                        email = %account.email,
                        error = %e,
                        backoff_ms = backoff.as_millis() as u64,
                        "idle: cycle failed; backing off"
                    );
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(BACKOFF_MAX);
                }
            }
        }
    }

    async fn idle_once(&self, account: &Account) -> crate::error::Result<IdleResult> {
        // Decrypt the account password on the async side, then move
        // the IMAP client into a blocking task. Same pattern as the
        // polling scheduler — keeps the tokio runtime free of
        // blocking IO.
        let password = self.db.account_password(account.id, &self.vault)?;
        let host = account.imap_host.clone();
        let port = account.imap_port;
        let email = account.email.clone();
        let bind_iface = self.vpn.bind_iface();

        let result = tokio::task::spawn_blocking(move || -> crate::error::Result<IdleResult> {
            let mut client = ImapClient::connect(
                &host,
                port,
                &email,
                &password,
                bind_iface.as_deref(),
            )?;
            if !client.supports_idle() {
                debug!(host = %host, "imap server does not advertise IDLE");
                client.logout();
                return Ok(IdleResult::Unsupported);
            }
            let outcome = client.idle_inbox(KEEPALIVE)?;
            client.logout();
            Ok(match outcome {
                WaitOutcome::MailboxChanged => IdleResult::Pushed,
                WaitOutcome::TimedOut => IdleResult::KeepaliveExpired,
            })
        })
        .await
        .map_err(|e| crate::error::Error::Other(anyhow::anyhow!("idle join: {e}")))??;

        if matches!(result, IdleResult::Unsupported) {
            // No IDLE support: nap so we don't burn the loop.
            // Polling scheduler covers correctness.
            tokio::time::sleep(BACKOFF_MAX).await;
        }
        Ok(result)
    }
}
