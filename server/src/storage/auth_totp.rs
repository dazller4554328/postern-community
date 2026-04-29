//! Storage layer for the optional TOTP second factor at vault unlock.
//!
//! Two tables: `auth_totp` is a singleton row tracking enabled state +
//! a pointer to the encrypted secret in the existing `secrets` table;
//! `auth_recovery_codes` holds SHA-256 hashes of single-use recovery
//! strings the user gets shown once during enrollment.
//!
//! Why store the secret in the existing `secrets` table rather than
//! inline: an offline DB read shouldn't be able to compute valid TOTP
//! codes, so the secret needs to be encrypted under the vault KEK
//! like every other long-lived sensitive value. The KEK is only
//! available when the vault is unlocked, which is exactly when we
//! need to read it (during the unlock that just verified the
//! password).

use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use sha2::{Digest, Sha256};

use super::Db;
use crate::error::{Error, Result};

/// Stable ref-name in the `secrets` table for the TOTP base32 secret.
const TOTP_SECRET_REF: &str = "auth:totp";

/// Public state surface for the Settings UI.
#[derive(Debug, Clone, Serialize)]
pub struct AuthTotpStatus {
    pub enabled: bool,
    /// True when an enrollment has been started (secret exists) but
    /// the user hasn't yet confirmed with a code. UI uses this to
    /// resume an in-progress setup vs offering a fresh one.
    pub pending: bool,
    pub recovery_codes_remaining: i64,
}

/// SHA-256 hash of a recovery code, hex-encoded. Recovery codes
/// themselves are short enough that brute-forcing the hash is
/// effectively the same as brute-forcing the code (16 random bytes
/// = 128 bits → 5e38 combinations), so a plain SHA-256 is fine —
/// no Argon2 needed, and a fast hash keeps unlock latency low.
fn hash_recovery_code(raw: &str) -> String {
    let mut h = Sha256::new();
    h.update(raw.as_bytes());
    hex::encode(h.finalize())
}

impl Db {
    /// Read the (singleton) TOTP row. Always returns Ok once the
    /// migration has run — the migration seeds the row with id=1.
    pub fn get_auth_totp_status(&self) -> Result<AuthTotpStatus> {
        let conn = self.pool().get()?;
        let (enabled, secret_ref): (i64, Option<String>) = conn.query_row(
            "SELECT enabled, secret_ref FROM auth_totp WHERE id = 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let remaining: i64 = conn.query_row(
            "SELECT COUNT(*) FROM auth_recovery_codes WHERE used_at IS NULL",
            [],
            |r| r.get(0),
        )?;
        Ok(AuthTotpStatus {
            enabled: enabled != 0,
            pending: enabled == 0 && secret_ref.is_some(),
            recovery_codes_remaining: remaining,
        })
    }

    /// Write a freshly-generated TOTP secret into the secrets table
    /// + point auth_totp at it. enabled stays 0 — the user still has
    /// to confirm a 6-digit code before it counts. Replaces any
    /// existing pending enrollment.
    pub fn store_auth_totp_secret(
        &self,
        vault: &crate::vault::Vault,
        secret_b32: &str,
    ) -> Result<()> {
        let mut conn = self.pool().get()?;
        let tx = conn.transaction()?;
        let wrapped = vault.encrypt(secret_b32.as_bytes())?;
        tx.execute(
            "INSERT INTO secrets(ref, ciphertext) VALUES (?1, ?2)
             ON CONFLICT(ref) DO UPDATE SET ciphertext = excluded.ciphertext",
            params![TOTP_SECRET_REF, wrapped],
        )?;
        let now = chrono::Utc::now().timestamp();
        tx.execute(
            "UPDATE auth_totp
                SET secret_ref = ?1, enabled = 0, updated_at = ?2
              WHERE id = 1",
            params![TOTP_SECRET_REF, now],
        )?;
        // Drop any leftover recovery codes from a previous enrollment
        // — they belong to the old secret, not this one.
        tx.execute("DELETE FROM auth_recovery_codes", [])?;
        tx.commit()?;
        Ok(())
    }

    /// Read the decrypted base32 TOTP secret. None when no enrollment
    /// has been started, or when the secrets row is missing (DB
    /// out-of-sync with auth_totp — defensive).
    pub fn read_auth_totp_secret(
        &self,
        vault: &crate::vault::Vault,
    ) -> Result<Option<String>> {
        let conn = self.pool().get()?;
        let secret_ref: Option<String> = conn
            .query_row(
                "SELECT secret_ref FROM auth_totp WHERE id = 1",
                [],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        let Some(ref_name) = secret_ref else {
            return Ok(None);
        };
        let ciphertext: Option<Vec<u8>> = conn
            .query_row(
                "SELECT ciphertext FROM secrets WHERE ref = ?1",
                params![ref_name],
                |r| r.get(0),
            )
            .optional()?;
        let Some(ct) = ciphertext else {
            return Ok(None);
        };
        let plain = vault.decrypt(&ct)?;
        let s = String::from_utf8(plain)
            .map_err(|_| Error::Other(anyhow::anyhow!("totp secret not utf-8")))?;
        Ok(Some(s))
    }

    /// Mark TOTP enabled. Call after a successful confirm-code check.
    pub fn enable_auth_totp(&self) -> Result<()> {
        let conn = self.pool().get()?;
        let now = chrono::Utc::now().timestamp();
        conn.execute(
            "UPDATE auth_totp SET enabled = 1, updated_at = ?1 WHERE id = 1",
            params![now],
        )?;
        Ok(())
    }

    /// Tear down all TOTP state — secret row, secret_ref pointer,
    /// recovery codes — and flip enabled back to 0. Used by the
    /// "Disable two-factor" flow.
    pub fn disable_auth_totp(&self) -> Result<()> {
        let mut conn = self.pool().get()?;
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM secrets WHERE ref = ?1", params![TOTP_SECRET_REF])?;
        tx.execute("DELETE FROM auth_recovery_codes", [])?;
        let now = chrono::Utc::now().timestamp();
        tx.execute(
            "UPDATE auth_totp
                SET enabled = 0, secret_ref = NULL, updated_at = ?1
              WHERE id = 1",
            params![now],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Insert a fresh batch of recovery codes (typically 10) hashed.
    /// Called once during the enrollment confirm step. Replaces any
    /// existing codes — store_auth_totp_secret already cleared them
    /// at the start of enrollment, but we do it again here so a
    /// "regenerate codes" flow can reuse this entry-point safely.
    pub fn store_recovery_codes(&self, raw_codes: &[String]) -> Result<()> {
        let mut conn = self.pool().get()?;
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM auth_recovery_codes", [])?;
        let now = chrono::Utc::now().timestamp();
        for code in raw_codes {
            let h = hash_recovery_code(code);
            tx.execute(
                "INSERT INTO auth_recovery_codes(code_hash, created_at) VALUES (?1, ?2)",
                params![h, now],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Try to consume a recovery code. Returns Ok(true) if the code
    /// matched an unused row and was just marked used. Constant-time
    /// in the matched-vs-mismatched path because both paths run a
    /// single SQL UPDATE keyed on the hash — no early-exit.
    pub fn consume_recovery_code(&self, raw: &str) -> Result<bool> {
        let h = hash_recovery_code(raw);
        let conn = self.pool().get()?;
        let now = chrono::Utc::now().timestamp();
        let n = conn.execute(
            "UPDATE auth_recovery_codes
                SET used_at = ?1
              WHERE code_hash = ?2 AND used_at IS NULL",
            params![now, h],
        )?;
        Ok(n > 0)
    }
}
