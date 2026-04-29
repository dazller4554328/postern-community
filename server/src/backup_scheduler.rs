//! Backup scheduler tick task.
//!
//! Runs once a minute. Reads `backup_schedule`, asks
//! `storage::should_fire_now` whether the current local-time minute
//! matches a scheduled run, and if so triggers a backup via the same
//! `BackupJob` registry the manual button uses.
//!
//! No retention work happens here — the post-backup success path
//! inside `http::backup::create` does that, so manual + scheduled
//! backups share one prune codepath.

use std::sync::Arc;

use tokio::task::JoinHandle;
use tracing::{info, warn};

use crate::{
    backup::{BackupJob, BackupJobs},
    backup_orchestrator,
    storage::{should_fire_now, Db},
    vault::Vault,
};

/// Spawn the tick loop. Called once from `main::serve`. The
/// `JoinHandle` is held by the caller — dropping it doesn't abort
/// the task (tokio detaches on drop), but holding it keeps the
/// shutdown story consistent with other long-running tasks.
pub fn spawn(
    db: Arc<Db>,
    vault: Vault,
    backup_jobs: BackupJobs,
    data_dir: std::path::PathBuf,
    backup_dir: std::path::PathBuf,
    vpn: crate::vpn::VpnManager,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            tick(&db, &vault, &backup_jobs, &data_dir, &backup_dir, &vpn).await;
        }
    })
}

async fn tick(
    db: &Arc<Db>,
    vault: &Vault,
    jobs: &BackupJobs,
    data_dir: &std::path::Path,
    backup_dir: &std::path::Path,
    vpn: &crate::vpn::VpnManager,
) {
    // Vault locked → no derived DB key, no backup. The schedule will
    // skip cleanly until the operator unlocks; no missed-run noise.
    if vault.require_unlocked().is_err() {
        return;
    }
    let sched = match db.get_backup_schedule() {
        Ok(s) => s,
        Err(e) => {
            warn!(error = %e, "scheduler: read schedule failed");
            return;
        }
    };
    let now = chrono::Local::now();
    if !should_fire_now(&sched, now) {
        return;
    }
    if jobs.is_running() {
        info!("scheduler: skipping (a backup is already in flight)");
        return;
    }

    info!(
        frequency = sched.frequency.as_str(),
        hour = sched.hour,
        minute = sched.minute,
        "scheduler: firing scheduled backup"
    );
    let initial = BackupJob::running();
    jobs.set(initial);

    if let Err(e) = db.record_backup_schedule_fired(now.timestamp()) {
        warn!(error = %e, "scheduler: failed to stamp last_run_at");
    }

    let db = db.clone();
    let vault = vault.clone();
    let jobs = jobs.clone();
    let data_dir = data_dir.to_path_buf();
    let backup_dir = backup_dir.to_path_buf();
    let bind_iface = vpn.bind_iface();
    tokio::spawn(backup_orchestrator::run_backup(
        db, vault, jobs, bind_iface, data_dir, backup_dir, "scheduled",
    ));
}
