//! Local backup: snapshot the `SQLCipher` DB + blob store into a
//! timestamped tar.gz in the backups volume. The DB is copied via
//! `SQLite`'s online backup API (consistent even while writers are
//! active); blobs are streamed into the tarball straight from the data dir.
//!
//! End-to-end encryption properties of the resulting tarball:
//!   - `postern.db` — `SQLCipher` (AES-256-CBC + HMAC). Key derived
//!     from the user's master password via the vault.
//!   - `blobs/<ab>/<cd>/<sha>` — ChaCha20-Poly1305 AEAD, per-blob
//!     random nonce, 16-byte tag. Key derived from the vault. See
//!     `storage::blobs` for the on-disk layout (version byte 0x01 +
//!     12-byte nonce + ciphertext + tag).
//!   - `vault.json` — KEK wrapped with master-password-derived key.
//!
//! So a stolen backup file is opaque end-to-end without the master
//! password. The on-machine backup directory therefore inherits the
//! same threat model as the live data dir: filesystem access alone
//! gives nothing without the password. (Off-site copies should still
//! travel over an authenticated channel — confidentiality at rest
//! does not imply integrity in transit.)

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex},
};

use chrono::Utc;
use serde::Serialize;
use tracing::info;

use crate::{
    error::{Error, Result},
    storage::Db,
};

#[derive(Debug, Clone, Serialize)]
pub struct BackupReport {
    pub filename: String,
    pub path: String,
    pub size_bytes: u64,
    pub db_bytes: u64,
    pub blob_count: usize,
    pub created_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BackupJobState {
    Running,
    Success,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
pub struct BackupJob {
    pub state: BackupJobState,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    /// Set when state == Success.
    pub report: Option<BackupReport>,
    /// Set when state == Failed.
    pub error: Option<String>,
}

impl BackupJob {
    pub fn running() -> Self {
        Self {
            state: BackupJobState::Running,
            started_at: Utc::now().timestamp(),
            finished_at: None,
            report: None,
            error: None,
        }
    }

    pub fn finish_success(&mut self, report: BackupReport) {
        self.state = BackupJobState::Success;
        self.finished_at = Some(Utc::now().timestamp());
        self.report = Some(report);
    }

    pub fn finish_failed(&mut self, error: String) {
        self.state = BackupJobState::Failed;
        self.finished_at = Some(Utc::now().timestamp());
        self.error = Some(error);
    }
}

/// Process-wide registry. There's only one active backup at a time
/// (single shared `backup_dir` + backup serially writes a tarball), so
/// we don't need a per-id map — just the last/current job.
///
/// Uses `std::sync::Mutex` rather than `tokio::sync::Mutex`: the lock
/// is only held for the duration of a clone-or-copy, never across an
/// await point. The `spawn_blocking` thread that runs `create_backup`
/// updates the registry without re-entering the async runtime, which
/// is the simpler + safer pattern.
#[derive(Default, Clone)]
pub struct BackupJobs {
    inner: Arc<Mutex<Option<BackupJob>>>,
}

impl BackupJobs {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn current(&self) -> Option<BackupJob> {
        self.inner
            .lock()
            .expect("backup_jobs lock poisoned")
            .clone()
    }

    pub fn is_running(&self) -> bool {
        matches!(
            self.inner
                .lock()
                .expect("backup_jobs lock poisoned")
                .as_ref()
                .map(|j| j.state),
            Some(BackupJobState::Running)
        )
    }

    pub fn set(&self, job: BackupJob) {
        *self.inner.lock().expect("backup_jobs lock poisoned") = Some(job);
    }
}

/// Headroom multiplier on the estimated backup footprint before we
/// agree to start. Covers tar/gzip framing on the (already-encrypted,
/// so incompressible) blobs plus whatever the live app writes while
/// the backup runs.
const PREFLIGHT_HEADROOM_PERCENT: u64 = 10;

/// Removes the staging dir and any half-written tarball when dropped,
/// so an error anywhere in `create_backup` can't strand gigabytes of
/// debris. (A failed weekly backup once left an 8 GB staging copy and
/// a truncated tarball behind, filled the disk, and took the whole
/// app down with "database or disk is full".)
struct StagingGuard {
    staging: PathBuf,
    partial: PathBuf,
}

impl Drop for StagingGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.staging);
        let _ = fs::remove_file(&self.partial);
    }
}

/// Create a timestamped backup. Blocking — run via `spawn_blocking`.
///
/// Only the DB snapshot and `vault.json` are staged on disk; blobs are
/// streamed straight from the data dir into the tarball, so the peak
/// footprint is roughly one copy of the data rather than two.
pub fn create_backup(db: &Db, data_dir: &Path, backup_dir: &Path) -> Result<BackupReport> {
    fs::create_dir_all(backup_dir)
        .map_err(|e| Error::Other(anyhow::anyhow!("create backup dir: {e}")))?;
    sweep_stale_staging(backup_dir);

    let db_src = data_dir.join("postern.db");
    let blob_src = data_dir.join("blobs");
    let db_live_bytes = file_len(&db_src) + file_len(&data_dir.join("postern.db-wal"));
    let (blob_count, blob_bytes) = if blob_src.is_dir() {
        measure_blobs(&blob_src)
    } else {
        (0, 0)
    };
    preflight_free_space(backup_dir, db_live_bytes, blob_bytes)?;

    let ts = Utc::now().format("%Y%m%d-%H%M%S");
    let name = format!("postern-backup-{ts}");
    let staging = backup_dir.join(&name);
    let tarball = backup_dir.join(format!("{name}.tar.gz"));
    // Written under a name `list_backups` ignores, then renamed into
    // place, so a crash mid-compress never shows up as a "backup".
    let partial = backup_dir.join(format!("{name}.tar.gz.partial"));
    let _guard = StagingGuard {
        staging: staging.clone(),
        partial: partial.clone(),
    };
    fs::create_dir_all(&staging)
        .map_err(|e| Error::Other(anyhow::anyhow!("create staging dir: {e}")))?;

    // 1. Copy the DB via SQLite online backup API — consistent snapshot
    //    even with concurrent readers/writers.
    let db_dst = staging.join("postern.db");
    info!("backup: copying DB via sqlite backup API");
    {
        let conn = db.pool().get()?;
        conn.execute_batch(&format!("VACUUM INTO '{}';", db_dst.display()))
            .map_err(|e| Error::Other(anyhow::anyhow!("VACUUM INTO: {e}")))?;
    }
    let db_bytes = file_len(&db_dst);

    // 2. Copy the vault sidecar.
    let vault_src = data_dir.join("vault.json");
    if vault_src.exists() {
        let _ = fs::copy(&vault_src, staging.join("vault.json"));
    }

    // 3. Compress. The staged `<name>/` dir comes from backup_dir; blobs
    //    are read in place from data_dir and renamed under `<name>/blobs`
    //    so the archive layout is unchanged for `restore`.
    info!(path = ?tarball, "backup: compressing");
    let partial_s = partial.to_string_lossy().to_string();
    let backup_dir_s = backup_dir.to_string_lossy().to_string();
    let data_dir_s = data_dir.to_string_lossy().to_string();
    let transform = format!("s,^blobs,{name}/blobs,");
    let mut args: Vec<&str> = vec!["-czf", &partial_s, "-C", &backup_dir_s, &name];
    if blob_src.is_dir() {
        args.extend(["-C", &data_dir_s, "--transform", &transform, "blobs"]);
    }
    let status = Command::new("tar")
        .args(&args)
        .status()
        .map_err(|e| Error::Other(anyhow::anyhow!("tar: {e}")))?;
    if !status.success() {
        return Err(Error::Other(anyhow::anyhow!("tar exited with {status}")));
    }
    fs::rename(&partial, &tarball)
        .map_err(|e| Error::Other(anyhow::anyhow!("finalise tarball: {e}")))?;

    let size_bytes = file_len(&tarball);
    let created_at = Utc::now().timestamp();

    info!(
        filename = %tarball.file_name().unwrap_or_default().to_string_lossy(),
        size_mb = size_bytes / (1024 * 1024),
        db_mb = db_bytes / (1024 * 1024),
        blob_count,
        "backup complete"
    );

    Ok(BackupReport {
        filename: format!("{name}.tar.gz"),
        path: tarball.to_string_lossy().to_string(),
        size_bytes,
        db_bytes,
        blob_count,
        created_at,
    })
}

/// Refuse to start a backup that can't fit. Estimated footprint is the
/// DB snapshot (staged) plus the tarball (DB again + blobs), with
/// headroom. Running out of space halfway is far worse than skipping
/// a run: SQLite starts failing every write in the live app.
fn preflight_free_space(backup_dir: &Path, db_bytes: u64, blob_bytes: u64) -> Result<()> {
    let footprint = db_bytes.saturating_mul(2).saturating_add(blob_bytes);
    let needed = footprint.saturating_add(footprint / 100 * PREFLIGHT_HEADROOM_PERCENT);
    let Some(available) = available_bytes(backup_dir) else {
        // Can't measure — don't block the backup on it.
        return Ok(());
    };
    if available < needed {
        let mib = 1024 * 1024;
        return Err(Error::Other(anyhow::anyhow!(
            "not enough free disk space for backup: need ~{} MB, {} MB available in {}",
            needed / mib,
            available / mib,
            backup_dir.display()
        )));
    }
    Ok(())
}

/// Bytes available to an unprivileged writer on the filesystem that
/// holds `path`. `None` if the query fails.
fn available_bytes(path: &Path) -> Option<u64> {
    let st = rustix::fs::statvfs(path).ok()?;
    Some(st.f_bavail.saturating_mul(st.f_frsize))
}

/// Remove leftovers from earlier runs that died before cleaning up:
/// `postern-backup-*` staging *directories* and `*.tar.gz.partial`
/// files. Finished tarballs are never touched.
pub fn sweep_stale_staging(backup_dir: &Path) {
    let Ok(entries) = fs::read_dir(backup_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with("postern-backup-") {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            info!(dir = %name, "backup: removing stale staging dir");
            let _ = fs::remove_dir_all(&path);
        } else if name.ends_with(".tar.gz.partial") {
            info!(file = %name, "backup: removing half-written tarball");
            let _ = fs::remove_file(&path);
        }
    }
}

fn file_len(path: &Path) -> u64 {
    fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

/// List existing backups in the backup directory.
pub fn list_backups(backup_dir: &Path) -> Result<Vec<BackupReport>> {
    let mut out = Vec::new();
    let entries = match fs::read_dir(backup_dir) {
        Ok(e) => e,
        Err(_) => return Ok(out),
    };
    for entry in entries {
        let Ok(entry) = entry else { continue };
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with("postern-backup-") || !name.ends_with(".tar.gz") {
            continue;
        }
        let meta = entry.metadata().ok();
        let size_bytes = meta.as_ref().map_or(0, std::fs::Metadata::len);
        let created_at = meta
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_secs() as i64);
        out.push(BackupReport {
            filename: name,
            path: entry.path().to_string_lossy().to_string(),
            size_bytes,
            db_bytes: 0,
            blob_count: 0,
            created_at,
        });
    }
    out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(out)
}

/// Keep only the `keep` most recent backup tarballs in `backup_dir`,
/// deleting everything older. `keep == 0` is a no-op (treat as
/// "unlimited"). Returns the number of files actually deleted.
///
/// `list_backups` already returns newest-first; we just slice past
/// the cutoff and delete.
pub fn prune_to_keep(backup_dir: &Path, keep: usize) -> Result<usize> {
    if keep == 0 {
        return Ok(0);
    }
    let backups = list_backups(backup_dir)?;
    if backups.len() <= keep {
        return Ok(0);
    }
    let mut deleted = 0;
    for old in backups.iter().skip(keep) {
        match delete_backup(backup_dir, &old.filename) {
            Ok(()) => {
                info!(filename = %old.filename, "retention: pruned old backup");
                deleted += 1;
            }
            Err(e) => {
                tracing::warn!(filename = %old.filename, error = %e, "retention: prune failed");
            }
        }
    }
    Ok(deleted)
}

/// Delete a backup file.
pub fn delete_backup(backup_dir: &Path, filename: &str) -> Result<()> {
    validate_backup_filename(filename)?;
    let path = backup_dir.join(filename);
    if !path.exists() {
        return Err(Error::NotFound);
    }
    fs::remove_file(&path).map_err(|e| Error::Other(anyhow::anyhow!("delete: {e}")))?;
    Ok(())
}

/// Reject anything that isn't a Postern-format backup filename: must
/// start with `postern-backup-`, end with `.tar.gz`, and contain no
/// path separators or `..` segments. The same check is needed by
/// every endpoint that joins user-supplied names onto the backup
/// directory — without it, a request for `../etc/passwd` could escape
/// the directory.
pub fn validate_backup_filename(filename: &str) -> Result<()> {
    if !filename.starts_with("postern-backup-")
        || !filename.ends_with(".tar.gz")
        || filename.contains('/')
        || filename.contains('\\')
        || filename.contains("..")
    {
        return Err(Error::BadRequest("invalid backup filename".into()));
    }
    Ok(())
}

/// Walk the blob store and return `(file_count, total_bytes)`. Entries
/// that vanish mid-walk are skipped rather than failing the backup.
fn measure_blobs(src: &Path) -> (usize, u64) {
    let mut count = 0usize;
    let mut bytes = 0u64;
    let mut stack = vec![src.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            if meta.is_dir() {
                stack.push(entry.path());
            } else if meta.is_file() {
                count += 1;
                bytes += meta.len();
            }
        }
    }
    (count, bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sweep_removes_stale_staging_but_keeps_finished_tarballs() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("postern-backup-20260101-000000/blobs")).unwrap();
        fs::write(
            root.join("postern-backup-20260102-000000.tar.gz.partial"),
            b"x",
        )
        .unwrap();
        fs::write(root.join("postern-backup-20260103-000000.tar.gz"), b"x").unwrap();
        fs::write(root.join("unrelated.txt"), b"x").unwrap();

        sweep_stale_staging(root);

        assert!(!root.join("postern-backup-20260101-000000").exists());
        assert!(!root
            .join("postern-backup-20260102-000000.tar.gz.partial")
            .exists());
        assert!(root.join("postern-backup-20260103-000000.tar.gz").exists());
        assert!(root.join("unrelated.txt").exists());
    }

    #[test]
    fn measure_blobs_counts_files_and_bytes_recursively() {
        let dir = tempfile::tempdir().unwrap();
        let blobs = dir.path().join("blobs");
        fs::create_dir_all(blobs.join("ab/cd")).unwrap();
        fs::write(blobs.join("ab/cd/one"), [0u8; 10]).unwrap();
        fs::write(blobs.join("ab/two"), [0u8; 5]).unwrap();

        assert_eq!(measure_blobs(&blobs), (2, 15));
        assert_eq!(measure_blobs(&dir.path().join("missing")), (0, 0));
    }

    #[test]
    fn preflight_rejects_when_estimate_exceeds_available_space() {
        let dir = tempfile::tempdir().unwrap();
        // Nothing to back up always fits.
        assert!(preflight_free_space(dir.path(), 0, 0).is_ok());
        // An absurd estimate never fits.
        let err = preflight_free_space(dir.path(), u64::MAX / 8, 0).unwrap_err();
        assert!(err.to_string().contains("not enough free disk space"));
    }

    #[test]
    fn staging_guard_cleans_up_on_drop() {
        let dir = tempfile::tempdir().unwrap();
        let staging = dir.path().join("postern-backup-X");
        let partial = dir.path().join("postern-backup-X.tar.gz.partial");
        fs::create_dir_all(&staging).unwrap();
        fs::write(&partial, b"half").unwrap();

        drop(StagingGuard {
            staging: staging.clone(),
            partial: partial.clone(),
        });

        assert!(!staging.exists());
        assert!(!partial.exists());
    }
}
