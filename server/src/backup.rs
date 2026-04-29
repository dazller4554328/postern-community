//! Local backup: snapshot the SQLCipher DB + blob store into a
//! timestamped tar.gz in the backups volume. The DB is copied via
//! SQLite's online backup API (consistent even while writers are
//! active); blobs are hard-linked or copied as on-disk files.
//!
//! End-to-end encryption properties of the resulting tarball:
//!   - `postern.db` — SQLCipher (AES-256-CBC + HMAC). Key derived
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
    path::Path,
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
/// (single shared backup_dir + backup serially writes a tarball), so
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
        self.inner.lock().expect("backup_jobs lock poisoned").clone()
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

/// Create a timestamped backup. Blocking — run via spawn_blocking.
pub fn create_backup(db: &Db, data_dir: &Path, backup_dir: &Path) -> Result<BackupReport> {
    fs::create_dir_all(backup_dir)
        .map_err(|e| Error::Other(anyhow::anyhow!("create backup dir: {e}")))?;

    let ts = Utc::now().format("%Y%m%d-%H%M%S");
    let name = format!("postern-backup-{ts}");
    let staging = backup_dir.join(&name);
    fs::create_dir_all(&staging)
        .map_err(|e| Error::Other(anyhow::anyhow!("create staging dir: {e}")))?;

    // 1. Copy the DB via SQLite online backup API — consistent snapshot
    //    even with concurrent readers/writers.
    let _db_src = data_dir.join("postern.db");
    let db_dst = staging.join("postern.db");
    info!("backup: copying DB via sqlite backup API");
    {
        let conn = db.pool().get()?;
        conn.execute_batch(&format!("VACUUM INTO '{}';", db_dst.display()))
            .map_err(|e| Error::Other(anyhow::anyhow!("VACUUM INTO: {e}")))?;
    }
    let db_bytes = fs::metadata(&db_dst).map(|m| m.len()).unwrap_or(0);

    // 2. Copy the vault sidecar.
    let vault_src = data_dir.join("vault.json");
    if vault_src.exists() {
        let _ = fs::copy(&vault_src, staging.join("vault.json"));
    }

    // 3. Copy blobs directory (hard-link where possible for speed).
    let blob_src = data_dir.join("blobs");
    let blob_dst = staging.join("blobs");
    let blob_count = if blob_src.is_dir() {
        copy_blobs(&blob_src, &blob_dst)?
    } else {
        0
    };

    // 4. Compress into a .tar.gz alongside the staging dir.
    let tarball = backup_dir.join(format!("{name}.tar.gz"));
    info!(path = ?tarball, "backup: compressing");
    let status = Command::new("tar")
        .args([
            "-czf",
            &tarball.to_string_lossy(),
            "-C",
            &backup_dir.to_string_lossy(),
            &name,
        ])
        .status()
        .map_err(|e| Error::Other(anyhow::anyhow!("tar: {e}")))?;
    if !status.success() {
        return Err(Error::Other(anyhow::anyhow!("tar exited with {status}")));
    }

    // 5. Clean up staging dir.
    let _ = fs::remove_dir_all(&staging);

    let size_bytes = fs::metadata(&tarball).map(|m| m.len()).unwrap_or(0);
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
        let size_bytes = meta.as_ref().map(|m| m.len()).unwrap_or(0);
        let created_at = meta
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
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
            Err(e) => tracing::warn!(filename = %old.filename, error = %e, "retention: prune failed"),
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

fn copy_blobs(src: &Path, dst: &Path) -> Result<usize> {
    fs::create_dir_all(dst).map_err(|e| Error::Other(anyhow::anyhow!("create blob dst: {e}")))?;
    let mut count = 0usize;
    let mut stack = vec![src.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries {
            let Ok(entry) = entry else { continue };
            let ft = entry
                .file_type()
                .unwrap_or_else(|_| fs::metadata(entry.path()).unwrap().file_type());
            let rel = entry
                .path()
                .strip_prefix(src)
                .unwrap_or(&entry.path())
                .to_path_buf();
            if ft.is_dir() {
                let sub = dst.join(&rel);
                let _ = fs::create_dir_all(&sub);
                stack.push(entry.path());
            } else if ft.is_file() {
                let dest = dst.join(&rel);
                if let Some(p) = dest.parent() {
                    let _ = fs::create_dir_all(p);
                }
                // Try hard-link first (instant, same filesystem); fall back to copy.
                if fs::hard_link(entry.path(), &dest).is_err() {
                    let _ = fs::copy(entry.path(), &dest);
                }
                count += 1;
            }
        }
    }
    Ok(count)
}
