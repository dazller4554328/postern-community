use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
};

use anyhow::{Context, Result};
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::params;
use tracing::info;

/// Embedded migration files. Numeric prefix determines order.
const MIGRATIONS: &[(&str, &str)] = &[
    (
        "0001_initial",
        include_str!("../../migrations/0001_initial.sql"),
    ),
    (
        "0002_fts_body_text",
        include_str!("../../migrations/0002_fts_body_text.sql"),
    ),
    (
        "0003_pgp_keyring",
        include_str!("../../migrations/0003_pgp_keyring.sql"),
    ),
    (
        "0004_thread_index",
        include_str!("../../migrations/0004_thread_index.sql"),
    ),
    (
        "0005_vault",
        include_str!("../../migrations/0005_vault.sql"),
    ),
    (
        "0006_msg_encrypted",
        include_str!("../../migrations/0006_msg_encrypted.sql"),
    ),
    (
        "0007_rules",
        include_str!("../../migrations/0007_rules.sql"),
    ),
    (
        "0008_audit_log",
        include_str!("../../migrations/0008_audit_log.sql"),
    ),
    (
        "0009_account_delete_policy",
        include_str!("../../migrations/0009_account_delete_policy.sql"),
    ),
    (
        "0010_fix_rule_label_kind",
        include_str!("../../migrations/0010_fix_rule_label_kind.sql"),
    ),
    (
        "0011_audit_category",
        include_str!("../../migrations/0011_audit_category.sql"),
    ),
    (
        "0012_account_archive_folder",
        include_str!("../../migrations/0012_account_archive_folder.sql"),
    ),
    (
        "0013_account_archive_strategy",
        include_str!("../../migrations/0013_account_archive_strategy.sql"),
    ),
    (
        "0014_account_archive_enabled",
        include_str!("../../migrations/0014_account_archive_enabled.sql"),
    ),
    (
        "0015_auto_archive",
        include_str!("../../migrations/0015_auto_archive.sql"),
    ),
    (
        "0016_account_avatar",
        include_str!("../../migrations/0016_account_avatar.sql"),
    ),
    (
        "0017_trusted_devices",
        include_str!("../../migrations/0017_trusted_devices.sql"),
    ),
    (
        "0018_message_subject_key",
        include_str!("../../migrations/0018_message_subject_key.sql"),
    ),
    (
        "0019_vpn_country_id",
        include_str!("../../migrations/0019_vpn_country_id.sql"),
    ),
    (
        "0020_vpn_server_load",
        include_str!("../../migrations/0020_vpn_server_load.sql"),
    ),
    (
        "0021_vpn_server_detail",
        include_str!("../../migrations/0021_vpn_server_detail.sql"),
    ),
    (
        "0022_retention",
        include_str!("../../migrations/0022_retention.sql"),
    ),
    (
        "0023_purge_gmail_categories",
        include_str!("../../migrations/0023_purge_gmail_categories.sql"),
    ),
    (
        "0024_skip_gmail_trash",
        include_str!("../../migrations/0024_skip_gmail_trash.sql"),
    ),
    (
        "0025_account_signature",
        include_str!("../../migrations/0025_account_signature.sql"),
    ),
    (
        "0026_outbox",
        include_str!("../../migrations/0026_outbox.sql"),
    ),
    (
        "0027_calendar",
        include_str!("../../migrations/0027_calendar.sql"),
    ),
    (
        "0028_reminders",
        include_str!("../../migrations/0028_reminders.sql"),
    ),
    (
        "0029_app_meta",
        include_str!("../../migrations/0029_app_meta.sql"),
    ),
    (
        "0030_account_enabled",
        include_str!("../../migrations/0030_account_enabled.sql"),
    ),
    (
        "0031_account_include_in_unified",
        include_str!("../../migrations/0031_account_include_in_unified.sql"),
    ),
    (
        "0032_backup_destinations",
        include_str!("../../migrations/0032_backup_destinations.sql"),
    ),
    (
        "0033_backup_destination_fingerprint",
        include_str!("../../migrations/0033_backup_destination_fingerprint.sql"),
    ),
    (
        "0034_backup_schedule",
        include_str!("../../migrations/0034_backup_schedule.sql"),
    ),
    (
        "0035_strip_stale_trash_labels",
        include_str!("../../migrations/0035_strip_stale_trash_labels.sql"),
    ),
    (
        "0036_ai",
        include_str!("../../migrations/0036_ai.sql"),
    ),
    (
        "0037_message_receipt_to",
        include_str!("../../migrations/0037_message_receipt_to.sql"),
    ),
    (
        "0038_ai_settings",
        include_str!("../../migrations/0038_ai_settings.sql"),
    ),
    (
        "0039_ai_embed_provider",
        include_str!("../../migrations/0039_ai_embed_provider.sql"),
    ),
    (
        "0040_ai_activity_log",
        include_str!("../../migrations/0040_ai_activity_log.sql"),
    ),
    (
        "0041_ai_auto_start",
        include_str!("../../migrations/0041_ai_auto_start.sql"),
    ),
    (
        "0042_lockdown_mode",
        include_str!("../../migrations/0042_lockdown_mode.sql"),
    ),
    (
        "0043_ai_user_rules",
        include_str!("../../migrations/0043_ai_user_rules.sql"),
    ),
    (
        "0044_ai_index_exclusions",
        include_str!("../../migrations/0044_ai_index_exclusions.sql"),
    ),
    (
        "0045_ai_polish_model",
        include_str!("../../migrations/0045_ai_polish_model.sql"),
    ),
    (
        "0046_auth_totp",
        include_str!("../../migrations/0046_auth_totp.sql"),
    ),
    (
        "0047_ai_freedom_mode",
        include_str!("../../migrations/0047_ai_freedom_mode.sql"),
    ),
    (
        "0048_ai_chat_max_tokens",
        include_str!("../../migrations/0048_ai_chat_max_tokens.sql"),
    ),
    (
        "0049_contacts",
        include_str!("../../migrations/0049_contacts.sql"),
    ),
];

/// The SQLite magic at offset 0 of a plain database. Any other bytes
/// indicate SQLCipher-encrypted (or corrupt, but we'll let the engine
/// figure that out).
const SQLITE_MAGIC: &[u8; 16] = b"SQLite format 3\0";

/// Runtime-settable DB key. Set via `Db::rekey_pool` after vault
/// unlock; read by the pool's connection-init callback which applies
/// `PRAGMA key` to each new connection.
#[derive(Default)]
pub struct KeyState {
    pub key_hex: Option<String>,
}

pub type KeyStateRef = Arc<RwLock<KeyState>>;

#[derive(Clone)]
pub struct Db {
    pool: Arc<RwLock<Pool<SqliteConnectionManager>>>,
    key_state: KeyStateRef,
    path: PathBuf,
}

impl Db {
    pub fn open(path: &Path) -> Result<Self> {
        let key_state: KeyStateRef = Arc::new(RwLock::new(KeyState::default()));
        let pool = Self::build_pool(path, key_state.clone())?;
        Ok(Self {
            pool: Arc::new(RwLock::new(pool)),
            key_state,
            path: path.to_path_buf(),
        })
    }

    fn build_pool(path: &Path, key_state: KeyStateRef) -> Result<Pool<SqliteConnectionManager>> {
        let ks = key_state.clone();
        let manager = SqliteConnectionManager::file(path).with_init(move |c| {
            // PRAGMA key MUST run before any other SQL on an encrypted
            // database. Reading the state under a read-lock is cheap.
            if let Ok(state) = ks.read() {
                if let Some(hex) = state.key_hex.as_ref() {
                    c.execute_batch(&format!("PRAGMA key = \"x'{hex}'\";"))?;
                }
            }
            c.execute_batch(
                "PRAGMA journal_mode = WAL;
                 PRAGMA synchronous = NORMAL;
                 PRAGMA foreign_keys = ON;
                 PRAGMA busy_timeout = 5000;",
            )
        });
        // `build_unchecked` + `min_idle(Some(0))` matter: an encrypted DB
        // can't be opened until the vault hands us the key, and any
        // pre-unlock connection attempt hits "file is not a database".
        // Letting r2d2 pre-fill the pool blocks Db::open for ~30s. With
        // min_idle=0, r2d2 never tries to warm connections; build_unchecked
        // skips the synchronous probe. Connections are created lazily on
        // the first `get()` after rekey.
        Ok(Pool::builder()
            .max_size(8)
            .min_idle(Some(0))
            .connection_timeout(std::time::Duration::from_secs(5))
            .build_unchecked(manager))
    }

    /// Apply the given key to future pool connections. Callers that
    /// need existing connections re-keyed too should call `rebuild_pool`.
    pub fn rekey_pool(&self, key_hex: String) -> Result<()> {
        if let Ok(mut ks) = self.key_state.write() {
            ks.key_hex = Some(key_hex);
        }
        let new_pool = Self::build_pool(&self.path, self.key_state.clone())?;
        if let Ok(mut guard) = self.pool.write() {
            *guard = new_pool;
        }
        Ok(())
    }

    /// True iff the DB file on disk is a plain SQLite (pre-SQLCipher).
    pub fn is_plain_sqlite(&self) -> Result<bool> {
        let mut f = match fs::File::open(&self.path) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(e) => return Err(e.into()),
        };
        let mut header = [0u8; 16];
        match f.read_exact(&mut header) {
            Ok(()) => Ok(&header == SQLITE_MAGIC),
            // Empty file, brand new install — treat as "not plain", caller
            // will create fresh encrypted DB.
            Err(_) => Ok(false),
        }
    }

    /// One-shot migration: exports the current plain DB into a new
    /// encrypted copy alongside it, atomically renames over the
    /// original, and purges the old WAL/SHM files which are
    /// incompatible with SQLCipher. Returns the size of the new file.
    ///
    /// Caller must hold the DB locked against concurrent writes while
    /// this runs — easiest is to invoke it immediately after vault
    /// unlock before the scheduler can start hammering IMAP.
    pub fn migrate_plain_to_sqlcipher(&self, key_hex: &str) -> Result<u64> {
        let tmp = self.path.with_file_name(format!(
            "{}.enc",
            self.path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("postern.db")
        ));
        // Remove any leftover from a previous aborted attempt.
        let _ = fs::remove_file(&tmp);

        // Fresh, unkeyed connection to the plain DB. Not from the pool
        // — that lets us guarantee no PRAGMA key has been applied.
        let conn = rusqlite::Connection::open(&self.path)?;
        conn.execute_batch("PRAGMA wal_checkpoint(FULL);")
            .context("flush WAL before export")?;
        conn.execute_batch(&format!(
            "ATTACH DATABASE '{}' AS enc KEY \"x'{}'\";
             SELECT sqlcipher_export('enc');
             DETACH DATABASE enc;",
            tmp.display(),
            key_hex
        ))
        .context("sqlcipher_export")?;
        drop(conn);

        let size = fs::metadata(&tmp).context("stat encrypted tmp")?.len();
        fs::rename(&tmp, &self.path).context("install encrypted DB")?;

        // Plain-era WAL frames can't be read by SQLCipher; wipe them.
        let _ = fs::remove_file(self.path.with_extension("db-wal"));
        let _ = fs::remove_file(self.path.with_extension("db-shm"));

        Ok(size)
    }

    pub fn migrate(&self) -> Result<()> {
        let mut conn = self.pool().get().context("acquire migration conn")?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_migrations (
                name       TEXT PRIMARY KEY,
                applied_at INTEGER NOT NULL
            );",
        )?;

        for (name, sql) in MIGRATIONS {
            let already: bool = conn
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE name = ?1)",
                    params![name],
                    |r| r.get(0),
                )
                .context("check migration")?;

            if already {
                continue;
            }

            let tx = conn.transaction().context("begin migration tx")?;
            tx.execute_batch(sql)
                .with_context(|| format!("apply migration {name}"))?;
            tx.execute(
                "INSERT INTO schema_migrations(name, applied_at) VALUES (?1, ?2)",
                params![name, chrono::Utc::now().timestamp()],
            )?;
            tx.commit()?;
            info!(%name, "migration applied");
        }

        Ok(())
    }

    #[must_use]
    pub fn pool(&self) -> Pool<SqliteConnectionManager> {
        self.pool.read().expect("db pool rwlock poisoned").clone()
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_apply_cleanly_and_are_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.db");

        let db = Db::open(&path).unwrap();
        db.migrate().unwrap();
        db.migrate().unwrap(); // second run is a no-op

        let conn = db.pool().get().unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, MIGRATIONS.len() as i64);

        // Sanity-check a few expected tables exist.
        for table in ["accounts", "messages", "labels", "messages_fts"] {
            let exists: bool = conn
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type IN ('table','view') AND name=?1)",
                    params![table],
                    |r| r.get(0),
                )
                .unwrap();
            assert!(exists, "expected table {table} to exist");
        }
    }
}
