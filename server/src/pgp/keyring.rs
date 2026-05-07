use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use super::crypto::KeyInfo;
use crate::{error::Result, storage::Db, vault::Vault};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum KeySource {
    Generated,
    Imported,
    Autocrypt,
    Wkd,
    Keyserver,
}

impl KeySource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Generated => "generated",
            Self::Imported => "imported",
            Self::Autocrypt => "autocrypt",
            Self::Wkd => "wkd",
            Self::Keyserver => "keyserver",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "generated" => Some(Self::Generated),
            "imported" => Some(Self::Imported),
            "autocrypt" => Some(Self::Autocrypt),
            "wkd" => Some(Self::Wkd),
            "keyserver" => Some(Self::Keyserver),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct NewKey<'a> {
    pub info: &'a KeyInfo,
    pub armored_public: &'a str,
    /// When present, this key's private material — stored encrypted.
    pub armored_secret: Option<&'a str>,
    pub source: KeySource,
}

#[derive(Debug, Clone, Serialize)]
pub struct KeyRow {
    pub id: i64,
    pub fingerprint: String,
    pub user_id: String,
    pub primary_email: Option<String>,
    pub is_secret: bool,
    pub created_at: i64,
    pub expires_at: Option<i64>,
    pub source: String,
    pub last_used_at: Option<i64>,
}

impl Db {
    pub fn pgp_upsert(&self, new: &NewKey<'_>, vault: &Vault) -> Result<i64> {
        let mut conn = self.pool().get()?;
        let tx = conn.transaction()?;

        let existing: Option<i64> = tx
            .query_row(
                "SELECT id FROM pgp_keys WHERE fingerprint = ?1",
                params![new.info.fingerprint],
                |r| r.get(0),
            )
            .optional()?;

        let secret_ref = if let Some(armored_secret) = new.armored_secret {
            let reff = format!("pgp:secret:{}", new.info.fingerprint);
            let wrapped = vault.encrypt(armored_secret.as_bytes())?;
            tx.execute(
                "INSERT INTO secrets(ref, ciphertext) VALUES (?1, ?2)
                 ON CONFLICT(ref) DO UPDATE SET ciphertext = excluded.ciphertext",
                params![reff, wrapped],
            )?;
            Some(reff)
        } else {
            None
        };

        let id = if let Some(id) = existing {
            tx.execute(
                "UPDATE pgp_keys SET
                    user_id = ?1,
                    primary_email = ?2,
                    is_secret = is_secret OR ?3,
                    armored_public = ?4,
                    secret_ref = COALESCE(?5, secret_ref),
                    expires_at = ?6,
                    source = CASE WHEN is_secret THEN source ELSE ?7 END
                 WHERE id = ?8",
                params![
                    new.info.user_id,
                    new.info.primary_email,
                    i32::from(new.info.has_secret || secret_ref.is_some()),
                    new.armored_public,
                    secret_ref,
                    new.info.expires_at,
                    new.source.as_str(),
                    id,
                ],
            )?;
            id
        } else {
            tx.execute(
                "INSERT INTO pgp_keys
                   (fingerprint, user_id, primary_email, is_secret, armored_public,
                    secret_ref, created_at, expires_at, source)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    new.info.fingerprint,
                    new.info.user_id,
                    new.info.primary_email,
                    i32::from(new.info.has_secret || secret_ref.is_some()),
                    new.armored_public,
                    secret_ref,
                    new.info.created_at,
                    new.info.expires_at,
                    new.source.as_str(),
                ],
            )?;
            tx.last_insert_rowid()
        };

        tx.commit()?;
        Ok(id)
    }

    pub fn pgp_list(&self) -> Result<Vec<KeyRow>> {
        let conn = self.pool().get()?;
        let mut stmt = conn.prepare(
            "SELECT id, fingerprint, user_id, primary_email, is_secret,
                    created_at, expires_at, source, last_used_at
             FROM pgp_keys
             ORDER BY is_secret DESC, created_at DESC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(KeyRow {
                id: r.get(0)?,
                fingerprint: r.get(1)?,
                user_id: r.get(2)?,
                primary_email: r.get(3)?,
                is_secret: r.get::<_, i64>(4)? != 0,
                created_at: r.get(5)?,
                expires_at: r.get(6)?,
                source: r.get(7)?,
                last_used_at: r.get(8)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn pgp_delete(&self, id: i64) -> Result<()> {
        let conn = self.pool().get()?;
        let secret_ref: Option<String> = conn
            .query_row(
                "SELECT secret_ref FROM pgp_keys WHERE id = ?1",
                params![id],
                |r| r.get(0),
            )
            .optional()?;
        let n = conn.execute("DELETE FROM pgp_keys WHERE id = ?1", params![id])?;
        if n == 0 {
            return Err(crate::error::Error::NotFound);
        }
        if let Some(r) = secret_ref {
            conn.execute("DELETE FROM secrets WHERE ref = ?1", params![r])?;
        }
        Ok(())
    }

    pub fn pgp_export_public(&self, id: i64) -> Result<String> {
        let conn = self.pool().get()?;
        conn.query_row(
            "SELECT armored_public FROM pgp_keys WHERE id = ?1",
            params![id],
            |r| r.get(0),
        )
        .optional()?
        .ok_or(crate::error::Error::NotFound)
    }

    /// Concatenate every armored public key in the ring, optionally
    /// followed by every secret key (when the vault is unlocked).
    /// Produces a single file that `gpg --import` and Postern's own
    /// paste flow both accept — the canonical OpenPGP "bundle of
    /// blocks" format.
    ///
    /// Used by the Settings → PGP "Download backup" button. When
    /// include_secret is true the output contains PRIVATE KEY BLOCK
    /// sections; treat the returned bytes as sensitive.
    pub fn pgp_export_all(&self, vault: &Vault, include_secret: bool) -> Result<String> {
        let conn = self.pool().get()?;
        // All publics are stored directly; secrets need to go through
        // the vault decrypt path so we pull them separately.
        let mut publics = conn.prepare(
            "SELECT armored_public FROM pgp_keys ORDER BY is_secret DESC, created_at ASC",
        )?;
        let pub_rows = publics.query_map([], |r| r.get::<_, String>(0))?;
        let mut buf = String::new();
        for row in pub_rows {
            let armored = row?;
            if !buf.is_empty() && !buf.ends_with('\n') {
                buf.push('\n');
            }
            buf.push_str(&armored);
        }

        if include_secret {
            // Reuses the existing decrypt-on-read path so we don't
            // reinvent the vault wrapping.
            for armored_secret in self.pgp_all_secrets(vault)? {
                if !buf.is_empty() && !buf.ends_with('\n') {
                    buf.push('\n');
                }
                buf.push_str(&armored_secret);
            }
        }

        Ok(buf)
    }

    /// All armored secret-key blobs we own. Used for decrypt attempts.
    /// Requires the vault to be unlocked; returns an empty list if a
    /// single row fails to decrypt (and logs it).
    pub fn pgp_all_secrets(&self, vault: &Vault) -> Result<Vec<String>> {
        let conn = self.pool().get()?;
        let mut stmt = conn.prepare(
            "SELECT s.ciphertext FROM pgp_keys k
             JOIN secrets s ON s.ref = k.secret_ref
             WHERE k.is_secret = 1 AND k.secret_ref IS NOT NULL",
        )?;
        let rows = stmt.query_map([], |r| r.get::<_, Vec<u8>>(0))?;
        let mut out = Vec::new();
        for row in rows {
            let wrapped = row?;
            match vault.decrypt(&wrapped) {
                Ok(bytes) => {
                    if let Ok(armored) = String::from_utf8(bytes) {
                        out.push(armored);
                    }
                }
                Err(e) => {
                    tracing::warn!(error = %e, "pgp secret row decrypt failed");
                }
            }
        }
        Ok(out)
    }

    /// Armored public-key blob for the first key we have matching this
    /// email address. None if the keyring has no key for them.
    pub fn pgp_public_armored_for_email(&self, email: &str) -> Result<Option<String>> {
        let conn = self.pool().get()?;
        conn.query_row(
            "SELECT armored_public FROM pgp_keys
             WHERE primary_email = ?1
             ORDER BY is_secret DESC, created_at DESC
             LIMIT 1",
            params![email.to_ascii_lowercase()],
            |r| r.get::<_, String>(0),
        )
        .optional()
        .map_err(Into::into)
    }

    /// Look up any key we have for a given email address.
    pub fn pgp_find_by_email(&self, email: &str) -> Result<Option<KeyRow>> {
        let conn = self.pool().get()?;
        conn.query_row(
            "SELECT id, fingerprint, user_id, primary_email, is_secret,
                    created_at, expires_at, source, last_used_at
             FROM pgp_keys
             WHERE primary_email = ?1
             ORDER BY is_secret DESC, created_at DESC
             LIMIT 1",
            params![email.to_ascii_lowercase()],
            |r| {
                Ok(KeyRow {
                    id: r.get(0)?,
                    fingerprint: r.get(1)?,
                    user_id: r.get(2)?,
                    primary_email: r.get(3)?,
                    is_secret: r.get::<_, i64>(4)? != 0,
                    created_at: r.get(5)?,
                    expires_at: r.get(6)?,
                    source: r.get(7)?,
                    last_used_at: r.get(8)?,
                })
            },
        )
        .optional()
        .map_err(Into::into)
    }
}
