//! Off-site backup destinations (Settings → Backups → "Off-site
//! destinations"). One row per place the operator wants the backup
//! tarball pushed after a successful local backup.
//!
//! Two-level secret storage to keep the at-rest threat model simple:
//!   - Public config (host, port, username, remote dir) → plain JSON
//!     in this table.
//!   - Credentials (password OR private key + passphrase) → vault-
//!     encrypted blob in the `secrets` table, referenced by
//!     `credential_ref`. So a stolen DB without the master password
//!     leaks the destinations a user has *configured*, not the
//!     credentials needed to use them.

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use super::Db;
use crate::{
    error::{Error, Result},
    vault::Vault,
};

/// What the operator stores publicly. Auth credentials live in the
/// vault-wrapped secret blob, not here.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SftpPublicConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    /// Remote directory the tarball is uploaded into. The server
    /// expects this to already exist and be writable by `username`.
    pub remote_dir: String,
}

/// Encrypted credential blob. Persisted via `Vault::encrypt`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "auth", rename_all = "lowercase")]
pub enum SftpCredential {
    /// Plain SSH password.
    Password { password: String },
    /// OpenSSH private key, optionally protected by a passphrase.
    Key {
        key_pem: String,
        passphrase: Option<String>,
    },
}

/// Public (non-secret) GDrive config — folder we upload into and the
/// owning Google account email so the operator can tell two
/// destinations apart in the UI.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GDrivePublicConfig {
    /// Drive folder id where backups are uploaded. Created on first
    /// auth (`Postern Backups` in the user's Drive root).
    pub folder_id: String,
    pub folder_name: String,
    /// User's Google address — display only.
    pub account_email: String,
}

/// Encrypted GDrive credential blob — long-lived `refresh_token` plus
/// a cached short-lived `access_token`. Refreshed on demand.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GDriveCredential {
    pub refresh_token: String,
    pub access_token: String,
    /// Unix seconds when the cached `access_token` expires. We refresh
    /// preemptively a minute before this.
    pub expires_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct BackupDestination {
    pub id: i64,
    pub kind: String,
    pub label: String,
    pub enabled: bool,
    pub public_config: serde_json::Value,
    /// TOFU hostkey fingerprint pinned on first successful connect.
    /// `None` = no fingerprint pinned yet (next connect captures and
    /// persists). Format: `SHA256:<base64-no-pad>`.
    pub server_fingerprint: Option<String>,
    pub last_push_at: Option<i64>,
    pub last_push_filename: Option<String>,
    pub last_push_status: Option<String>,
    pub last_push_error: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone)]
pub struct NewBackupDestination {
    pub kind: String,
    pub label: String,
    pub public_config: serde_json::Value,
    /// Already-serialised credential JSON. Caller (HTTP handler)
    /// builds this from the per-kind credential type so this struct
    /// stays kind-agnostic at the storage layer.
    pub credential_json: Vec<u8>,
}

impl Db {
    pub fn list_backup_destinations(&self) -> Result<Vec<BackupDestination>> {
        let conn = self.pool().get()?;
        let mut stmt = conn.prepare(
            "SELECT id, kind, label, enabled, public_config_json,
                    server_fingerprint,
                    last_push_at, last_push_filename, last_push_status,
                    last_push_error, created_at
             FROM backup_destinations
             ORDER BY id ASC",
        )?;
        let rows = stmt.query_map([], row_to_destination)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn list_enabled_backup_destinations(&self) -> Result<Vec<BackupDestination>> {
        Ok(self
            .list_backup_destinations()?
            .into_iter()
            .filter(|d| d.enabled)
            .collect())
    }

    pub fn get_backup_destination(&self, id: i64) -> Result<BackupDestination> {
        let conn = self.pool().get()?;
        conn.query_row(
            "SELECT id, kind, label, enabled, public_config_json,
                    server_fingerprint,
                    last_push_at, last_push_filename, last_push_status,
                    last_push_error, created_at
             FROM backup_destinations WHERE id = ?1",
            params![id],
            row_to_destination,
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Error::NotFound,
            other => other.into(),
        })
    }

    /// Read + decrypt the raw credential JSON blob for a destination.
    /// Caller picks the per-kind type to deserialise into.
    pub fn get_destination_credential_blob(
        &self,
        destination_id: i64,
        vault: &Vault,
    ) -> Result<Vec<u8>> {
        let conn = self.pool().get()?;
        let cred_ref: String = conn
            .query_row(
                "SELECT credential_ref FROM backup_destinations WHERE id = ?1",
                params![destination_id],
                |r| r.get(0),
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Error::NotFound,
                other => other.into(),
            })?;
        let wrapped: Vec<u8> = conn
            .query_row(
                "SELECT ciphertext FROM secrets WHERE ref = ?1",
                params![cred_ref],
                |r| r.get(0),
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Error::NotFound,
                other => other.into(),
            })?;
        vault.decrypt(&wrapped)
    }

    pub fn get_sftp_credential(
        &self,
        destination_id: i64,
        vault: &Vault,
    ) -> Result<SftpCredential> {
        let plaintext = self.get_destination_credential_blob(destination_id, vault)?;
        serde_json::from_slice(&plaintext)
            .map_err(|e| Error::Other(anyhow::anyhow!("decode sftp credential: {e}")))
    }

    pub fn get_gdrive_credential(
        &self,
        destination_id: i64,
        vault: &Vault,
    ) -> Result<GDriveCredential> {
        let plaintext = self.get_destination_credential_blob(destination_id, vault)?;
        serde_json::from_slice(&plaintext)
            .map_err(|e| Error::Other(anyhow::anyhow!("decode gdrive credential: {e}")))
    }

    /// Replace the destination's credential blob with a freshly-
    /// encrypted one. Used by the GDrive driver after a token refresh
    /// to persist the new access_token + expires_at without disturbing
    /// any other column.
    pub fn update_destination_credential(
        &self,
        destination_id: i64,
        new_credential_json: &[u8],
        vault: &Vault,
    ) -> Result<()> {
        let conn = self.pool().get()?;
        let cred_ref: String = conn
            .query_row(
                "SELECT credential_ref FROM backup_destinations WHERE id = ?1",
                params![destination_id],
                |r| r.get(0),
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Error::NotFound,
                other => other.into(),
            })?;
        let wrapped = vault.encrypt(new_credential_json)?;
        conn.execute(
            "UPDATE secrets SET ciphertext = ?1 WHERE ref = ?2",
            params![wrapped, cred_ref],
        )?;
        Ok(())
    }

    pub fn insert_backup_destination(
        &self,
        new: &NewBackupDestination,
        vault: &Vault,
    ) -> Result<BackupDestination> {
        let mut conn = self.pool().get()?;
        let tx = conn.transaction()?;

        let public_json = serde_json::to_string(&new.public_config)
            .map_err(|e| Error::Other(anyhow::anyhow!("encode public_config: {e}")))?;

        let credential_ref = format!(
            "backup-dest:{}:{}",
            new.kind,
            chrono::Utc::now().timestamp_micros()
        );
        let wrapped = vault.encrypt(&new.credential_json)?;
        tx.execute(
            "INSERT INTO secrets(ref, ciphertext) VALUES (?1, ?2)",
            params![credential_ref, wrapped],
        )?;

        let created_at = chrono::Utc::now().timestamp();
        tx.execute(
            "INSERT INTO backup_destinations
                (kind, label, enabled, public_config_json, credential_ref, created_at)
             VALUES (?1, ?2, 1, ?3, ?4, ?5)",
            params![new.kind, new.label, public_json, credential_ref, created_at],
        )?;
        let id = tx.last_insert_rowid();
        tx.commit()?;
        self.get_backup_destination(id)
    }

    pub fn update_backup_destination_enabled(&self, id: i64, enabled: bool) -> Result<()> {
        let conn = self.pool().get()?;
        let n = conn.execute(
            "UPDATE backup_destinations SET enabled = ?1 WHERE id = ?2",
            params![i32::from(enabled), id],
        )?;
        if n == 0 {
            return Err(Error::NotFound);
        }
        Ok(())
    }

    pub fn update_backup_destination_label(&self, id: i64, label: &str) -> Result<()> {
        let conn = self.pool().get()?;
        let n = conn.execute(
            "UPDATE backup_destinations SET label = ?1 WHERE id = ?2",
            params![label, id],
        )?;
        if n == 0 {
            return Err(Error::NotFound);
        }
        Ok(())
    }

    pub fn delete_backup_destination(&self, id: i64) -> Result<()> {
        let conn = self.pool().get()?;
        let cred_ref: Option<String> = conn
            .query_row(
                "SELECT credential_ref FROM backup_destinations WHERE id = ?1",
                params![id],
                |r| r.get(0),
            )
            .optional()?;
        let n = conn.execute(
            "DELETE FROM backup_destinations WHERE id = ?1",
            params![id],
        )?;
        if n == 0 {
            return Err(Error::NotFound);
        }
        if let Some(r) = cred_ref {
            conn.execute("DELETE FROM secrets WHERE ref = ?1", params![r])?;
        }
        Ok(())
    }

    /// Pin the server's hostkey fingerprint. Used after a TOFU
    /// first-connect captures the live fingerprint, and as the
    /// "remember this key" effect of the Test action when the row
    /// previously had no fingerprint stored.
    pub fn set_backup_destination_fingerprint(
        &self,
        id: i64,
        fingerprint: &str,
    ) -> Result<()> {
        let conn = self.pool().get()?;
        let n = conn.execute(
            "UPDATE backup_destinations SET server_fingerprint = ?1 WHERE id = ?2",
            params![fingerprint, id],
        )?;
        if n == 0 {
            return Err(Error::NotFound);
        }
        Ok(())
    }

    /// Clear the pinned fingerprint. Used when the operator legitimately
    /// rotated the SSH host key on the destination box and needs the
    /// next connect to re-TOFU rather than refuse with a mismatch.
    pub fn forget_backup_destination_fingerprint(&self, id: i64) -> Result<()> {
        let conn = self.pool().get()?;
        let n = conn.execute(
            "UPDATE backup_destinations SET server_fingerprint = NULL WHERE id = ?1",
            params![id],
        )?;
        if n == 0 {
            return Err(Error::NotFound);
        }
        Ok(())
    }

    /// Stamp a successful push.
    pub fn record_destination_push_ok(
        &self,
        id: i64,
        filename: &str,
    ) -> Result<()> {
        let conn = self.pool().get()?;
        conn.execute(
            "UPDATE backup_destinations
             SET last_push_at = ?1,
                 last_push_filename = ?2,
                 last_push_status = 'ok',
                 last_push_error = NULL
             WHERE id = ?3",
            params![chrono::Utc::now().timestamp(), filename, id],
        )?;
        Ok(())
    }

    /// Stamp a failed push.
    pub fn record_destination_push_err(&self, id: i64, error: &str) -> Result<()> {
        let conn = self.pool().get()?;
        conn.execute(
            "UPDATE backup_destinations
             SET last_push_at = ?1,
                 last_push_status = 'error',
                 last_push_error = ?2
             WHERE id = ?3",
            params![chrono::Utc::now().timestamp(), error, id],
        )?;
        Ok(())
    }
}

fn row_to_destination(r: &rusqlite::Row) -> rusqlite::Result<BackupDestination> {
    let public_json: String = r.get(4)?;
    let public_config: serde_json::Value = serde_json::from_str(&public_json).unwrap_or_else(|_| {
        // Best-effort: don't fail the whole list query because one
        // row's JSON went weird. Surface as empty object so the UI
        // shows the row is broken instead of the panel collapsing.
        serde_json::json!({})
    });
    Ok(BackupDestination {
        id: r.get(0)?,
        kind: r.get(1)?,
        label: r.get(2)?,
        enabled: r.get::<_, i64>(3)? != 0,
        public_config,
        server_fingerprint: r.get(5)?,
        last_push_at: r.get(6)?,
        last_push_filename: r.get(7)?,
        last_push_status: r.get(8)?,
        last_push_error: r.get(9)?,
        created_at: r.get(10)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sftp_credential_roundtrips_via_json() {
        let original = SftpCredential::Password {
            password: "swordfish".into(),
        };
        let s = serde_json::to_string(&original).unwrap();
        // Tag-based discrimination — the JSON shape must include "auth"
        // so a future variant added in the wrong order doesn't silently
        // mis-decode older blobs.
        assert!(s.contains("\"auth\":\"password\""), "{s}");
        let back: SftpCredential = serde_json::from_str(&s).unwrap();
        match back {
            SftpCredential::Password { password } => assert_eq!(password, "swordfish"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn gdrive_credential_roundtrips_via_json() {
        let original = GDriveCredential {
            refresh_token: "1//refresh".into(),
            access_token: "ya29.access".into(),
            expires_at: 1_777_000_000,
        };
        let s = serde_json::to_string(&original).unwrap();
        let back: GDriveCredential = serde_json::from_str(&s).unwrap();
        assert_eq!(back.refresh_token, "1//refresh");
        assert_eq!(back.access_token, "ya29.access");
        assert_eq!(back.expires_at, 1_777_000_000);
    }

    #[test]
    fn sftp_credential_key_variant_carries_optional_passphrase() {
        let with = SftpCredential::Key {
            key_pem: "-----BEGIN ...".into(),
            passphrase: Some("p".into()),
        };
        let without = SftpCredential::Key {
            key_pem: "-----BEGIN ...".into(),
            passphrase: None,
        };
        let s1 = serde_json::to_string(&with).unwrap();
        let s2 = serde_json::to_string(&without).unwrap();
        let _: SftpCredential = serde_json::from_str(&s1).unwrap();
        let r2: SftpCredential = serde_json::from_str(&s2).unwrap();
        if let SftpCredential::Key { passphrase, .. } = r2 {
            assert!(passphrase.is_none());
        } else {
            panic!("wrong variant");
        }
    }
}
