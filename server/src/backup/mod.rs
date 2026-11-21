//! Backup + restore subsystem.
//!
//! Folded under `server/src/backup/` from six top-level siblings
//! (`backup.rs`, `backup_destinations.rs`, `backup_orchestrator.rs`,
//! `backup_scheduler.rs`, `gdrive.rs`, `restore.rs`) so the related
//! pieces are visible together. Public surface is re-exported here
//! so external callers keep using `crate::backup::<thing>` paths
//! they already had — only `gdrive` and `restore` get a new prefix.

pub mod destinations;
pub mod gdrive;
pub mod orchestrator;
pub mod restore;
pub mod scheduler;

mod local;

// Re-export the local-backup surface so external callers continue
// to use `crate::backup::create_backup` etc. The `local` submodule
// stays private — its name was historically just `backup`, so a
// `crate::backup::local::create_backup` call site reads worse than
// the bare re-export.
pub use local::{
    delete_backup, list_backups, validate_backup_filename, BackupJob, BackupJobs, BackupReport,
};
