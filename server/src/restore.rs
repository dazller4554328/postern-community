//! Restore from a backup tarball.
//!
//! Three phases, mirroring the Settings → Backups UI:
//!
//!   1. **Stage** — caller writes the uploaded tarball under
//!      `<data_dir>/restore-staging/<id>/backup.tar.gz`.
//!
//!   2. **Validate** — `validate_staged_backup` extracts the tarball,
//!      reads its embedded `vault.json`, derives the DB key from the
//!      master password the user just typed, opens the embedded
//!      `postern.db` with that key, and counts rows. Returns a
//!      `ValidationSummary` on success; nothing on disk is touched
//!      yet beyond the staging dir.
//!
//!   3. **Apply** — `mark_for_boot_restore` writes a `.restore-on-boot`
//!      marker that points at the staging dir, and the server exits.
//!      On the next process start, `consume_pending_restore` (called
//!      from `main`) runs *before* the live DB pool opens: it moves
//!      the existing `postern.db`, `vault.json`, and `blobs/` aside
//!      to `.pre-restore-<ts>/` for one-restart undo, then untars the
//!      staged backup into `<data_dir>/`.
//!
//! Failure modes:
//!   - Wrong master password → `BadRequest("wrong master password")`,
//!     same surface as `Vault::unlock`. Staging dir is preserved so
//!     the user can retry without re-uploading.
//!   - Tarball missing files (no `vault.json` or `postern.db`) →
//!     `BadRequest("not a Postern backup ...")`.
//!   - Boot-time untar fails halfway → the previous live data is
//!     still in `.pre-restore-<ts>/`; operator can recover by hand.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use serde::Serialize;
use tracing::{info, warn};

use crate::{
    error::{Error, Result},
    vault::{derive_db_key_from_sidecar, sidecar},
};

/// Subdirectory under `data_dir` that holds in-flight uploads. One
/// child per staging session, identified by a random id.
pub const STAGING_DIRNAME: &str = "restore-staging";

/// Marker file the boot consumer looks for. Contents = the full path
/// of the staging tarball to untar over the data dir. One line, UTF-8.
pub const BOOT_MARKER: &str = ".restore-on-boot";

/// Where displaced live data lands during a successful boot-time
/// restore — kept for one-restart manual undo.
fn pre_restore_dir(data_dir: &Path) -> PathBuf {
    let ts = chrono::Utc::now().format("%Y%m%dT%H%M%SZ");
    data_dir.join(format!(".pre-restore-{ts}"))
}

#[derive(Debug, Clone, Serialize)]
pub struct ValidationSummary {
    /// Random id assigned to this staging session. The frontend
    /// includes this in subsequent /validate and /apply calls so
    /// multiple uploads don't collide.
    pub staging_id: String,
    pub backup_filename: String,
    pub size_bytes: u64,
    /// Number of mailbox accounts in the backup's DB.
    pub accounts: u32,
    /// Number of message rows.
    pub messages: u32,
    /// Number of blob files (RFC822 bodies + attachments).
    pub blobs: u32,
    /// Mtime of the tarball — closest cheap proxy for "when was this
    /// backup made". Embedded backup timestamps would need a metadata
    /// file inside the tarball; not worth the format change for this.
    pub created_at: i64,
}

/// Random hex id for a staging session. Cheap, collision-resistant
/// enough for one-user-typing-fast scenarios; we don't need crypto
/// strength because anything in `restore-staging/` is already inside
/// `data_dir` and only readable by the postern process.
pub fn new_staging_id() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

pub fn staging_root(data_dir: &Path) -> PathBuf {
    data_dir.join(STAGING_DIRNAME)
}

pub fn staging_dir(data_dir: &Path, id: &str) -> PathBuf {
    staging_root(data_dir).join(id)
}

pub fn staged_tarball(data_dir: &Path, id: &str) -> PathBuf {
    staging_dir(data_dir, id).join("backup.tar.gz")
}

fn extracted_dir(data_dir: &Path, id: &str) -> PathBuf {
    staging_dir(data_dir, id).join("extracted")
}

/// Create a staging directory and return the tarball path the HTTP
/// upload handler should stream into. The caller is responsible for
/// writing the bytes.
pub fn prepare_staging(data_dir: &Path) -> Result<(String, PathBuf)> {
    let id = new_staging_id();
    let dir = staging_dir(data_dir, &id);
    fs::create_dir_all(&dir)
        .map_err(|e| Error::Other(anyhow::anyhow!("create staging dir: {e}")))?;
    let tarball = staged_tarball(data_dir, &id);
    Ok((id, tarball))
}

/// Stage an existing on-server backup so the validate+apply pipeline
/// can pick it up without a fresh upload. We hard-link rather than
/// copy when possible — the tarball can be hundreds of MB and a true
/// copy would double disk usage just to validate.
///
/// `backup_filename` is the bare filename (e.g.
/// `postern-backup-20260425-130000.tar.gz`) within `backup_dir`. The
/// caller MUST sanity-check it against directory traversal before
/// calling — see `crate::backup::delete_backup` for the same pattern.
pub fn stage_existing_backup(
    data_dir: &Path,
    backup_dir: &Path,
    backup_filename: &str,
) -> Result<(String, PathBuf)> {
    crate::backup::validate_backup_filename(backup_filename)?;
    let source = backup_dir.join(backup_filename);
    if !source.exists() {
        return Err(Error::NotFound);
    }
    let (id, dest) = prepare_staging(data_dir)?;
    // Try hard-link first (instant, same filesystem). Fall back to
    // copy when the source and staging dir are on different mounts —
    // common when the operator put backups on a separate disk.
    if fs::hard_link(&source, &dest).is_err() {
        fs::copy(&source, &dest)
            .map_err(|e| Error::Other(anyhow::anyhow!("copy {source:?} → staging: {e}")))?;
    }
    info!(?source, ?dest, "restore: staged existing backup");
    Ok((id, dest))
}

/// Validate a staged tarball against `password`.
///
/// 1. Extract the tarball into `<staging_dir>/extracted/`.
/// 2. Locate the wrapper directory inside (`postern-backup-*`).
/// 3. Read `vault.json` and verify the password unlocks it.
/// 4. Open the embedded `postern.db` with the derived SQLCipher key.
/// 5. Count rows + blobs.
///
/// On wrong password we return BadRequest and leave the staging dir
/// in place so the user can retry without re-uploading.
pub fn validate_staged_backup(
    data_dir: &Path,
    staging_id: &str,
    password: &str,
) -> Result<ValidationSummary> {
    let tarball = staged_tarball(data_dir, staging_id);
    if !tarball.exists() {
        return Err(Error::BadRequest(
            "no staged backup with that id — re-upload the tarball".into(),
        ));
    }
    let extract_to = extracted_dir(data_dir, staging_id);
    if extract_to.exists() {
        let _ = fs::remove_dir_all(&extract_to);
    }
    fs::create_dir_all(&extract_to)
        .map_err(|e| Error::Other(anyhow::anyhow!("create extract dir: {e}")))?;

    // Use the system `tar` rather than a Rust crate: backup.rs writes
    // the tarballs with the same binary, so there's no risk of a
    // format mismatch, and we already shell out for compression on
    // create. Pure-Rust tar/gz parsing would just add a dependency.
    let status = Command::new("tar")
        .args([
            "-xzf",
            &tarball.to_string_lossy(),
            "-C",
            &extract_to.to_string_lossy(),
        ])
        .status()
        .map_err(|e| Error::Other(anyhow::anyhow!("spawn tar: {e}")))?;
    if !status.success() {
        return Err(Error::BadRequest(
            "could not extract tarball — file may be corrupt or not a tar.gz".into(),
        ));
    }

    let inner = locate_backup_root(&extract_to)?;
    let summary =
        verify_extracted_backup(&inner, password, staging_id, &tarball)?;
    Ok(summary)
}

/// Pure helper: given the *already-extracted* `postern-backup-*` directory
/// and a candidate password, run the verification + counting steps.
/// Split out so unit tests can call it without going through the
/// full staging tarball pipeline.
fn verify_extracted_backup(
    backup_root: &Path,
    password: &str,
    staging_id: &str,
    tarball: &Path,
) -> Result<ValidationSummary> {
    let vault_path = backup_root.join("vault.json");
    let db_path = backup_root.join("postern.db");
    if !vault_path.exists() || !db_path.exists() {
        return Err(Error::BadRequest(
            "not a Postern backup — missing vault.json or postern.db".into(),
        ));
    }

    let vf = sidecar::read(&vault_path)?
        .ok_or_else(|| Error::BadRequest("vault.json unreadable".into()))?;
    let db_key = derive_db_key_from_sidecar(&vf, password)?;
    let key_hex = hex::encode(db_key.as_slice());

    // Open the backup's DB with rusqlite directly — bypasses the live
    // connection pool entirely. PRAGMA key MUST be the first statement.
    let conn = rusqlite::Connection::open(&db_path)
        .map_err(|e| Error::Other(anyhow::anyhow!("open backup DB: {e}")))?;
    conn.execute_batch(&format!("PRAGMA key = \"x'{key_hex}'\";"))
        .map_err(|e| Error::Other(anyhow::anyhow!("set PRAGMA key: {e}")))?;
    // Touching schema is the cheapest "is the key right?" probe.
    let accounts: i64 = conn
        .query_row("SELECT COUNT(*) FROM accounts", [], |r| r.get(0))
        .map_err(|e| {
            // SQLCipher with the wrong key surfaces as
            // "file is not a database" — translate to friendly copy.
            if e.to_string().contains("not a database") {
                Error::BadRequest(
                    "wrong master password (DB rejected the derived key)".into(),
                )
            } else {
                Error::Other(anyhow::anyhow!("query accounts: {e}"))
            }
        })?;
    let messages: i64 = conn
        .query_row("SELECT COUNT(*) FROM messages", [], |r| r.get(0))
        .unwrap_or(0);

    let blob_count = backup_root.join("blobs");
    let blobs = if blob_count.is_dir() {
        count_files_recursive(&blob_count)
    } else {
        0
    };

    let metadata = fs::metadata(tarball)
        .map_err(|e| Error::Other(anyhow::anyhow!("stat tarball: {e}")))?;
    let size_bytes = metadata.len();
    let created_at = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let backup_filename = tarball
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "backup.tar.gz".into());

    info!(
        accounts,
        messages, blobs, size_bytes, "restore: validated staged backup"
    );

    Ok(ValidationSummary {
        staging_id: staging_id.to_owned(),
        backup_filename,
        size_bytes,
        accounts: accounts as u32,
        messages: messages as u32,
        blobs: blobs as u32,
        created_at,
    })
}

/// `tar -xzf` produces `<extract_to>/postern-backup-*/{postern.db,...}`.
/// Find that single child directory.
fn locate_backup_root(extract_to: &Path) -> Result<PathBuf> {
    let mut found: Option<PathBuf> = None;
    for entry in fs::read_dir(extract_to)
        .map_err(|e| Error::Other(anyhow::anyhow!("read extract dir: {e}")))?
    {
        let Ok(entry) = entry else { continue };
        if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            if found.is_some() {
                return Err(Error::BadRequest(
                    "tarball has multiple top-level directories — not a Postern backup".into(),
                ));
            }
            found = Some(entry.path());
        }
    }
    found.ok_or_else(|| {
        Error::BadRequest(
            "tarball is empty or has no top-level directory — not a Postern backup".into(),
        )
    })
}

fn count_files_recursive(dir: &Path) -> u32 {
    let mut count = 0u32;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = fs::read_dir(&d) else {
            continue;
        };
        for entry in entries.flatten() {
            match entry.file_type() {
                Ok(ft) if ft.is_dir() => stack.push(entry.path()),
                Ok(ft) if ft.is_file() => count += 1,
                _ => {}
            }
        }
    }
    count
}

/// Phase 3a: write the boot marker. After this returns, the caller
/// should arrange for the process to exit; on the next start
/// `consume_pending_restore` will swap the data dir.
pub fn mark_for_boot_restore(data_dir: &Path, staging_id: &str) -> Result<()> {
    let tarball = staged_tarball(data_dir, staging_id);
    if !tarball.exists() {
        return Err(Error::BadRequest("no such staged backup".into()));
    }
    let marker = data_dir.join(BOOT_MARKER);
    fs::write(&marker, tarball.to_string_lossy().as_bytes())
        .map_err(|e| Error::Other(anyhow::anyhow!("write marker: {e}")))?;
    info!(?marker, "restore: marked for boot consumption");
    Ok(())
}

/// Discard a staged backup. Called by the cancel button or after the
/// boot consumer finishes (the staging dir is no longer useful once
/// restoration is complete).
pub fn discard_staging(data_dir: &Path, staging_id: &str) -> Result<()> {
    let dir = staging_dir(data_dir, staging_id);
    if dir.exists() {
        fs::remove_dir_all(&dir)
            .map_err(|e| Error::Other(anyhow::anyhow!("remove staging: {e}")))?;
    }
    Ok(())
}

/// Phase 3b: called from `main` *before* the DB pool opens. If the
/// boot marker exists, swap the data dir contents with the staged
/// backup. Returns `Ok(true)` when a restore was applied.
///
/// Idempotent against partial completion: if untar succeeded but the
/// marker delete failed, the next boot would untar again over the
/// (already-restored) data dir. That's wasteful but not destructive,
/// and unlikely in practice — fs::remove_file is reliable.
pub fn consume_pending_restore(data_dir: &Path) -> Result<bool> {
    let marker = data_dir.join(BOOT_MARKER);
    let Ok(tarball_path_bytes) = fs::read_to_string(&marker) else {
        return Ok(false);
    };
    let tarball_path = PathBuf::from(tarball_path_bytes.trim());
    if !tarball_path.exists() {
        warn!(?tarball_path, "restore marker points at missing tarball — clearing");
        let _ = fs::remove_file(&marker);
        return Ok(false);
    }

    info!(?tarball_path, "restore: marker found, applying");

    // Untar to a temp dir adjacent to data_dir. If untar fails, abort
    // before touching live data.
    let tmp_extract = data_dir.join(".restore-tmp");
    if tmp_extract.exists() {
        let _ = fs::remove_dir_all(&tmp_extract);
    }
    fs::create_dir_all(&tmp_extract)
        .map_err(|e| Error::Other(anyhow::anyhow!("create restore tmp: {e}")))?;
    let status = Command::new("tar")
        .args([
            "-xzf",
            &tarball_path.to_string_lossy(),
            "-C",
            &tmp_extract.to_string_lossy(),
        ])
        .status()
        .map_err(|e| Error::Other(anyhow::anyhow!("spawn tar: {e}")))?;
    if !status.success() {
        let _ = fs::remove_dir_all(&tmp_extract);
        return Err(Error::Other(anyhow::anyhow!(
            "tar exited with {status} — leaving live data untouched"
        )));
    }

    let new_root = locate_backup_root(&tmp_extract)?;

    // Move the live `postern.db`, `vault.json`, `blobs/` aside under
    // `<data_dir>/.pre-restore-<ts>/`. Operator can recover by hand
    // by swapping back if the restore turns out to be wrong.
    let preserve_dir = pre_restore_dir(data_dir);
    fs::create_dir_all(&preserve_dir)
        .map_err(|e| Error::Other(anyhow::anyhow!("create preserve dir: {e}")))?;
    for name in ["postern.db", "postern.db-shm", "postern.db-wal", "vault.json", "blobs"] {
        let live = data_dir.join(name);
        if live.exists() {
            let dest = preserve_dir.join(name);
            if let Err(e) = fs::rename(&live, &dest) {
                warn!(name, error = %e, "preserve-aside failed; trying copy-then-remove");
                copy_then_remove(&live, &dest)?;
            }
        }
    }

    // Move every file from new_root into data_dir.
    move_dir_contents(&new_root, data_dir)?;

    // Cleanup: extract dir, marker, staged tarball.
    let _ = fs::remove_dir_all(&tmp_extract);
    let _ = fs::remove_file(&marker);
    let _ = fs::remove_file(&tarball_path);

    info!(?preserve_dir, "restore: applied; previous data preserved");
    Ok(true)
}

fn copy_then_remove(src: &Path, dst: &Path) -> Result<()> {
    if src.is_dir() {
        copy_dir_recursive(src, dst)?;
        fs::remove_dir_all(src)
            .map_err(|e| Error::Other(anyhow::anyhow!("rm dir {src:?}: {e}")))?;
    } else {
        fs::copy(src, dst).map_err(|e| Error::Other(anyhow::anyhow!("copy {src:?}: {e}")))?;
        fs::remove_file(src).map_err(|e| Error::Other(anyhow::anyhow!("rm {src:?}: {e}")))?;
    }
    Ok(())
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst).map_err(|e| Error::Other(anyhow::anyhow!("mkdir {dst:?}: {e}")))?;
    for entry in
        fs::read_dir(src).map_err(|e| Error::Other(anyhow::anyhow!("read {src:?}: {e}")))?
    {
        let entry = entry.map_err(|e| Error::Other(anyhow::anyhow!("entry: {e}")))?;
        let path = entry.path();
        let target = dst.join(entry.file_name());
        if path.is_dir() {
            copy_dir_recursive(&path, &target)?;
        } else {
            fs::copy(&path, &target)
                .map_err(|e| Error::Other(anyhow::anyhow!("copy {path:?}: {e}")))?;
        }
    }
    Ok(())
}

fn move_dir_contents(src: &Path, dst: &Path) -> Result<()> {
    for entry in
        fs::read_dir(src).map_err(|e| Error::Other(anyhow::anyhow!("read {src:?}: {e}")))?
    {
        let entry = entry.map_err(|e| Error::Other(anyhow::anyhow!("entry: {e}")))?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if to.exists() {
            // Should never happen — caller moved the live files aside
            // first — but defensively remove rather than fail.
            if to.is_dir() {
                let _ = fs::remove_dir_all(&to);
            } else {
                let _ = fs::remove_file(&to);
            }
        }
        if let Err(e) = fs::rename(&from, &to) {
            warn!(?from, error = %e, "rename failed, falling back to copy");
            copy_then_remove(&from, &to)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locate_backup_root_rejects_empty_tarball() {
        let dir = tempfile::tempdir().unwrap();
        let err = locate_backup_root(dir.path()).unwrap_err();
        match err {
            Error::BadRequest(msg) => assert!(
                msg.contains("not a Postern backup"),
                "unexpected: {msg}"
            ),
            e => panic!("expected BadRequest, got {e:?}"),
        }
    }

    #[test]
    fn locate_backup_root_finds_single_child() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("postern-backup-20260101-000000")).unwrap();
        let found = locate_backup_root(dir.path()).unwrap();
        assert_eq!(
            found.file_name().unwrap(),
            "postern-backup-20260101-000000"
        );
    }

    #[test]
    fn locate_backup_root_rejects_multiple_top_level_dirs() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("a")).unwrap();
        fs::create_dir(dir.path().join("b")).unwrap();
        let err = locate_backup_root(dir.path()).unwrap_err();
        match err {
            Error::BadRequest(msg) => assert!(msg.contains("multiple top-level"), "{msg}"),
            e => panic!("expected BadRequest, got {e:?}"),
        }
    }

    #[test]
    fn count_files_recursive_walks_nested_dirs() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("ab/cd")).unwrap();
        fs::write(dir.path().join("ab/cd/file1"), b"x").unwrap();
        fs::write(dir.path().join("ab/cd/file2"), b"y").unwrap();
        fs::create_dir_all(dir.path().join("ef")).unwrap();
        fs::write(dir.path().join("ef/file3"), b"z").unwrap();
        // also a top-level file
        fs::write(dir.path().join("top"), b"w").unwrap();
        assert_eq!(count_files_recursive(dir.path()), 4);
    }

    #[test]
    fn count_files_recursive_returns_zero_for_missing() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("does-not-exist");
        assert_eq!(count_files_recursive(&missing), 0);
    }

    #[test]
    fn new_staging_id_is_24_hex_chars() {
        let id = new_staging_id();
        assert_eq!(id.len(), 24);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn new_staging_id_is_unique_per_call() {
        let a = new_staging_id();
        let b = new_staging_id();
        assert_ne!(a, b, "ids must not collide");
    }

    #[test]
    fn consume_pending_restore_returns_false_when_no_marker() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(consume_pending_restore(dir.path()).unwrap(), false);
    }

    #[test]
    fn stage_existing_rejects_directory_traversal() {
        let dir = tempfile::tempdir().unwrap();
        let backup_dir = dir.path().join("backups");
        fs::create_dir_all(&backup_dir).unwrap();
        let data_dir = dir.path().join("data");
        fs::create_dir_all(&data_dir).unwrap();

        for bad in [
            "../etc/passwd",
            "postern-backup-../oops.tar.gz",
            "..",
            "/etc/passwd",
            "subdir/postern-backup-x.tar.gz",
        ] {
            let result = stage_existing_backup(&data_dir, &backup_dir, bad);
            assert!(matches!(result, Err(Error::BadRequest(_))), "should reject {bad}");
        }
    }

    #[test]
    fn stage_existing_rejects_wrong_prefix_or_suffix() {
        let dir = tempfile::tempdir().unwrap();
        let backup_dir = dir.path().join("backups");
        fs::create_dir_all(&backup_dir).unwrap();
        let data_dir = dir.path().join("data");
        fs::create_dir_all(&data_dir).unwrap();

        for bad in [
            "evil.tar.gz",                          // wrong prefix
            "postern-backup-thing.zip",             // wrong suffix
            "postern-backup-",                      // both
        ] {
            let result = stage_existing_backup(&data_dir, &backup_dir, bad);
            assert!(matches!(result, Err(Error::BadRequest(_))), "should reject {bad}");
        }
    }

    #[test]
    fn stage_existing_returns_not_found_for_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let backup_dir = dir.path().join("backups");
        fs::create_dir_all(&backup_dir).unwrap();
        let data_dir = dir.path().join("data");
        fs::create_dir_all(&data_dir).unwrap();

        let result = stage_existing_backup(
            &data_dir,
            &backup_dir,
            "postern-backup-20260101-000000.tar.gz",
        );
        assert!(matches!(result, Err(Error::NotFound)));
    }

    #[test]
    fn stage_existing_hardlinks_or_copies_real_file() {
        let dir = tempfile::tempdir().unwrap();
        let backup_dir = dir.path().join("backups");
        fs::create_dir_all(&backup_dir).unwrap();
        let data_dir = dir.path().join("data");
        fs::create_dir_all(&data_dir).unwrap();

        let backup_name = "postern-backup-20260101-000000.tar.gz";
        let source = backup_dir.join(backup_name);
        fs::write(&source, b"fake tarball content").unwrap();

        let (id, dest) = stage_existing_backup(&data_dir, &backup_dir, backup_name).unwrap();
        assert!(dest.exists(), "staged tarball must exist");
        assert_eq!(fs::read(&dest).unwrap(), b"fake tarball content");
        // Original is untouched (hard-link or copy doesn't move).
        assert!(source.exists());
        // staging_id is in the dest path so the validate endpoint can find it.
        assert!(dest.to_string_lossy().contains(&id));
    }

    #[test]
    fn consume_pending_restore_clears_dangling_marker() {
        // Marker pointing at a file that no longer exists — happens
        // if the operator manually deleted the staging tarball
        // between scheduling and reboot.
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(BOOT_MARKER), "/nonexistent/path.tar.gz").unwrap();
        assert_eq!(consume_pending_restore(dir.path()).unwrap(), false);
        assert!(
            !dir.path().join(BOOT_MARKER).exists(),
            "dangling marker should be cleared"
        );
    }
}
