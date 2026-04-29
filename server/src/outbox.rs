//! Background worker that drains the outbox.
//!
//! Ticks every WORKER_TICK_SECS. On each tick:
//!   1. If the vault is locked, skip. Queued rows wait safely — they
//!      stay 'pending' and will be picked up after the next unlock.
//!   2. Atomically claim up to BATCH rows whose scheduled_at has passed.
//!   3. For each claimed row, deserialize the payload and hand it to
//!      send_blocking. Mark sent on success, failed on error.
//!
//! Undo-send is an upstream concern — it never reaches the worker
//! because the UI cancels the row before scheduled_at arrives.

use std::{sync::Arc, time::Duration};

use tokio::{task::JoinHandle, time};
use tracing::{error, info, warn};

use crate::{
    send::{send_blocking, SendRequest},
    storage::Db,
    vault::Vault,
    vpn::VpnManager,
};

const WORKER_TICK_SECS: u64 = 2;
const BATCH: usize = 10;
/// Stale-row rescue: anything that claims 'sending' but hasn't finished
/// within this many seconds gets returned to 'pending' so the next
/// worker loop tries again. This covers crashes or process restarts
/// mid-dispatch.
const STALE_SENDING_SECS: i64 = 120;

pub fn spawn(db: Arc<Db>, vpn: VpnManager, vault: Vault) -> JoinHandle<()> {
    tokio::spawn(async move {
        // Stale-row rescue (rows stuck in 'sending' from a crashed
        // previous process) must wait until the vault unlocks — the
        // outbox table lives in the encrypted DB. Do it once on the
        // first unlocked tick so an unlock after crash still heals.
        let mut rescued = false;

        let mut tick = time::interval(Duration::from_secs(WORKER_TICK_SECS));
        // Drop the immediate-first-tick so we don't fire during startup
        // before everything else has finished initialising.
        tick.tick().await;
        loop {
            tick.tick().await;
            if !vault.is_unlocked() {
                continue;
            }
            // Lockdown hold: skip dispatch entirely while the
            // install is in lockdown mode. Rows stay in 'pending'
            // and resume on the next tick after lockdown is
            // disabled — no rows fail, no rows get cancelled.
            if matches!(db.lockdown_enabled(), Ok(true)) {
                continue;
            }
            if !rescued {
                match db.outbox_requeue_stale_sending(STALE_SENDING_SECS) {
                    Ok(0) => {}
                    Ok(n) => info!(requeued = n, "rescued stale outbox rows"),
                    Err(e) => {
                        warn!(error = %e, "outbox stale-row sweep failed");
                        continue;
                    }
                }
                rescued = true;
            }
            let now = chrono::Utc::now().timestamp();
            let claimed = match db.outbox_claim_due(now, BATCH) {
                Ok(v) => v,
                Err(e) => {
                    warn!(error = %e, "outbox claim failed");
                    continue;
                }
            };
            if claimed.is_empty() {
                continue;
            }

            for entry in claimed {
                let id = entry.id;
                let payload = entry.payload_json.clone();
                let db2 = db.clone();
                let vpn2 = vpn.clone();
                let vault2 = vault.clone();
                // Dispatch in a blocking task — smtp + IMAP APPEND are
                // sync and we don't want to block the runtime. Awaiting
                // each one in turn keeps ordering sane and avoids
                // hammering the mail server with parallel SMTP sessions
                // from the same account.
                let result: std::result::Result<
                    anyhow::Result<(String, String)>,
                    tokio::task::JoinError,
                > = tokio::task::spawn_blocking(move || {
                    let req: SendRequest = serde_json::from_str(&payload)
                        .map_err(|e| anyhow::anyhow!("bad outbox payload: {e}"))?;
                    let report = send_blocking(&db2, &vpn2, &vault2, req)
                        .map_err(|e| anyhow::anyhow!("{e}"))?;
                    let forensics = serde_json::to_string(&report.forensics)
                        .unwrap_or_else(|_| "{}".to_string());
                    Ok((report.message_id, forensics))
                })
                .await;

                match result {
                    Ok(Ok((message_id, forensics))) => {
                        if let Err(e) = db.outbox_mark_sent(id, &message_id, &forensics) {
                            warn!(error = %e, id, "outbox mark_sent failed");
                        }
                    }
                    Ok(Err(e)) => {
                        let msg = format!("{e}");
                        error!(error = %msg, id, "outbox dispatch failed");
                        if let Err(e2) = db.outbox_mark_failed(id, &msg) {
                            warn!(error = %e2, id, "outbox mark_failed failed");
                        }
                    }
                    Err(join_err) => {
                        let msg = format!("worker task panicked: {join_err}");
                        error!(error = %msg, id, "outbox dispatch panic");
                        let _ = db.outbox_mark_failed(id, &msg);
                    }
                }
            }
        }
    })
}
