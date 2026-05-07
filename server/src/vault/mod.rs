//! Master-password-derived encryption.
//!
//! Two keys from one password:
//!   1. Argon2id(password, salt)   → 32-byte master
//!   2. HKDF-SHA256(master, "kek") → KEK for secrets (ChaCha20-Poly1305)
//!   3. HKDF-SHA256(master, "db")  → DB key for SQLCipher (32 bytes → hex → PRAGMA key)
//!
//! Metadata (salt + verifier + Argon params) lives in a sidecar file
//! `<data_dir>/vault.json` so the SQLCipher-encrypted DB bootstrap
//! doesn't fight itself: we can read the sidecar without touching the
//! DB, derive both keys, then open the DB with the right PRAGMA key.
//!
//! Migration from Phase A: if the sidecar is missing but the legacy
//! `kek_config` table has a row (Phase A shipped state-in-DB), read it,
//! write the sidecar, and proceed. The DB row stays until the DB itself
//! is encrypted — no point chasing cleanup when the next step wipes
//! it anyway.

mod crypto;
mod session;
pub mod sidecar;

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, RwLock},
};

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use rand::RngCore;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};
use zeroize::Zeroizing;

use crate::{
    error::{Error, Result},
    storage::{BlobStore, Db},
    vpn::VpnManager,
};

use crypto::{
    aead_decrypt, aead_encrypt, argon2_derive, hkdf_split, ARGON_M_COST_KIB, ARGON_P_COST,
    ARGON_T_COST,
};
use session::{
    evaluate_session, is_tailnet_cgnat, DeviceSession, SessionVerdict, SESSION_HARD_CAP_SECS,
    SESSION_IDLE_SECS,
};
use sidecar::VaultFile;

const VERIFIER_PLAINTEXT: &[u8] = b"postern-verify-v1";
const SALT_LEN: usize = 16;

#[derive(Clone)]
pub struct Vault {
    db: Arc<Db>,
    vpn: Option<VpnManager>,
    /// Blob store whose ChaCha20-Poly1305 key we install at unlock
    /// time. `None` when Vault is constructed standalone (tests);
    /// real wiring happens via `set_blob_store` from `main::serve`.
    blobs: Option<Arc<BlobStore>>,
    sidecar_path: PathBuf,
    inner: Arc<RwLock<VaultInner>>,
}

#[derive(Default)]
struct VaultInner {
    kek: Option<Zeroizing<[u8; 32]>>,
    db_key: Option<Zeroizing<[u8; 32]>>,
    /// ChaCha20-Poly1305 key for at-rest blob encryption. Held alongside
    /// the other subkeys so every blob read/write while the vault is
    /// unlocked can fetch it without a password re-derivation. Cleared
    /// on lock via `lock()` → `install_keys` is the installer.
    blob_key: Option<Zeroizing<[u8; 32]>>,
    failed_attempts: u32,
    last_failed_at: i64,
    /// IP address at time of unlock. If a subsequent request arrives
    /// from a different IP, the vault auto-locks.
    unlock_ip: Option<String>,
    /// Per-device authenticated sessions. Keyed by SHA-256 of the
    /// cookie token (matching the `trusted_devices.token_hash` column).
    /// Presence of an entry means "this device successfully entered
    /// the master password and hasn't been idle/expired/locked since."
    /// The map is the gate for HTTP requests — `is_unlocked()` (which
    /// reflects the global SQLCipher mount) keeps returning true for
    /// background workers regardless of who's currently authenticated.
    sessions: HashMap<String, DeviceSession>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum VaultStatus {
    Uninitialized,
    Locked,
    Unlocked,
}

impl Vault {
    pub fn new(db: Arc<Db>, data_dir: PathBuf) -> Self {
        let sidecar_path = sidecar::sidecar_path(&data_dir);
        Self {
            db,
            vpn: None,
            blobs: None,
            sidecar_path,
            inner: Arc::new(RwLock::new(VaultInner::default())),
        }
    }

    /// Wire the VPN manager so reconcile runs automatically after unlock.
    /// Called once from main after both Vault and VpnManager are created.
    pub fn set_vpn(&mut self, vpn: VpnManager) {
        self.vpn = Some(vpn);
    }

    /// Wire the blob store so every unlock installs the blob key and
    /// runs the first-time migration pass. Called once from `main::serve`.
    pub fn set_blob_store(&mut self, blobs: Arc<BlobStore>) {
        self.blobs = Some(blobs);
    }

    pub fn status(&self) -> VaultStatus {
        if self
            .inner
            .read()
            .ok()
            .and_then(|g| g.kek.as_ref().map(|_| ()))
            .is_some()
        {
            return VaultStatus::Unlocked;
        }
        if self.sidecar_path.exists() {
            return VaultStatus::Locked;
        }
        // Sidecar missing — check legacy Phase A in-DB state.
        match self.read_legacy_config() {
            Ok(Some(_)) => VaultStatus::Locked,
            _ => VaultStatus::Uninitialized,
        }
    }

    pub fn init(&self, password: &str) -> Result<()> {
        if password.is_empty() {
            return Err(Error::BadRequest("master password cannot be empty".into()));
        }
        if sidecar::read(&self.sidecar_path)?.is_some() || self.read_legacy_config()?.is_some() {
            return Err(Error::BadRequest(
                "vault already initialised; use /unlock".into(),
            ));
        }

        let mut salt = [0u8; SALT_LEN];
        rand::thread_rng().fill_bytes(&mut salt);
        let master = argon2_derive(password, &salt)?;
        let (kek, db_key, blob_key) = hkdf_split(&master);
        let verifier_blob = aead_encrypt(&kek, VERIFIER_PLAINTEXT)?;

        let vf = VaultFile::new(
            &salt,
            &verifier_blob,
            ARGON_M_COST_KIB,
            ARGON_T_COST,
            ARGON_P_COST,
        );
        sidecar::write(&self.sidecar_path, &vf)?;

        self.install_keys(kek, db_key, blob_key);
        // Order matters: apply_db_encryption runs the schema migrations
        // that create the `secrets` table. migrate_legacy_secrets queries
        // that table, so on a fresh install (no DB file, no tables yet)
        // it must run *after* encryption+migrations have applied.
        // Pre-existing sidecar/legacy state is rejected above, so on this
        // path legacy_secrets always finds 0 rows — but keep the call as
        // defence-in-depth if an admin hand-copies an old DB in place.
        self.apply_db_encryption()?;
        self.migrate_legacy_secrets()?;
        info!(path = ?self.sidecar_path, "vault initialised");
        Ok(())
    }

    pub fn unlock(&self, password: &str) -> Result<()> {
        // Rate-limit: after 3 failures, enforce exponential backoff
        // (2^failures seconds, capped at 60s).
        if let Ok(guard) = self.inner.read() {
            if guard.failed_attempts >= 3 {
                let now = chrono::Utc::now().timestamp();
                let wait = (1i64 << guard.failed_attempts.min(6)).min(60);
                if now - guard.last_failed_at < wait {
                    return Err(Error::BadRequest(format!(
                        "too many failed attempts — wait {} seconds",
                        wait - (now - guard.last_failed_at)
                    )));
                }
            }
        }

        if let Some(vf) = sidecar::read(&self.sidecar_path)? {
            let salt = vf.salt_bytes()?;
            let verifier = vf.verifier_bytes()?;
            let master = argon2_derive(password, &salt)?;
            let (kek, db_key, blob_key) = hkdf_split(&master);
            if aead_decrypt(&kek, &verifier).is_err() {
                self.record_failed_attempt();
                return Err(Error::BadRequest("wrong master password".into()));
            }
            self.reset_attempts();
            self.install_keys(kek, db_key, blob_key);
            self.apply_db_encryption()?;
            info!("vault unlocked");
            return Ok(());
        }

        // Legacy (Phase A): kek_config row exists, verifier + secrets
        // were AEAD'd directly with the raw Argon2 output. We verify
        // with that old key, then re-wrap every secret and the
        // verifier under the new HKDF subkey before promoting to sidecar.
        let legacy = self
            .read_legacy_config()?
            .ok_or_else(|| Error::BadRequest("vault not initialised".into()))?;
        let master = argon2_derive(password, &legacy.salt)?;
        if aead_decrypt(&master, &legacy.verifier).is_err() {
            self.record_failed_attempt();
            return Err(Error::BadRequest("wrong master password".into()));
        }
        self.reset_attempts();

        let (new_kek, db_key, blob_key) = hkdf_split(&master);
        self.rewrap_legacy_secrets(&master, &new_kek)?;
        let new_verifier = aead_encrypt(&new_kek, VERIFIER_PLAINTEXT)?;
        let vf = VaultFile::new(
            &legacy.salt,
            &new_verifier,
            ARGON_M_COST_KIB,
            ARGON_T_COST,
            ARGON_P_COST,
        );
        sidecar::write(&self.sidecar_path, &vf)?;
        info!("vault upgraded Phase A → Phase B (sidecar + HKDF)");

        self.install_keys(new_kek, db_key, blob_key);
        self.apply_db_encryption()?;
        info!("vault unlocked");
        Ok(())
    }

    /// Migrate `secrets` rows from Phase A (encrypted under raw Argon2)
    /// to Phase B (encrypted under HKDF-derived KEK). Any row that
    /// doesn't decrypt with the old key falls back to base64 decode
    /// (pre-vault rows that skipped Phase A re-wrap entirely).
    fn rewrap_legacy_secrets(&self, old_kek: &[u8; 32], new_kek: &[u8; 32]) -> Result<()> {
        let conn = self.db.pool().get()?;
        let mut stmt = conn.prepare("SELECT ref, ciphertext FROM secrets")?;
        let rows: Vec<(String, Vec<u8>)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(stmt);

        let mut moved = 0usize;
        for (ref_name, blob) in rows {
            let plain = match aead_decrypt(old_kek, &blob) {
                Ok(p) => p,
                Err(_) => match B64.decode(&blob) {
                    Ok(p) => p,
                    Err(_) => {
                        warn!(%ref_name, "legacy secret neither old-KEK nor base64, skipping");
                        continue;
                    }
                },
            };
            let wrapped = aead_encrypt(new_kek, &plain)?;
            conn.execute(
                "UPDATE secrets SET ciphertext = ?1 WHERE ref = ?2",
                params![wrapped, ref_name],
            )?;
            moved += 1;
        }
        info!(rewrapped = moved, "secrets migrated old-KEK → new-KEK");
        Ok(())
    }

    pub fn lock(&self) {
        let mut g = self.expect_write();
        g.kek = None;
        g.db_key = None;
        g.blob_key = None;
        g.unlock_ip = None;
        g.sessions.clear();
        drop(g);
        if let Some(ref blobs) = self.blobs {
            blobs.clear_key();
        }
        info!("vault locked");
    }

    pub fn is_unlocked(&self) -> bool {
        matches!(self.status(), VaultStatus::Unlocked)
    }

    /// Directory holding the vault sidecar — same dir we keep
    /// other unencrypted bookkeeping files in. Used by the HTTP
    /// layer to drop tiny marker files that need to be readable
    /// before unlock (e.g. "TOTP is enabled" so the unlock screen
    /// can render the 2FA field without first having to decrypt
    /// the auth_totp table that lives in SQLCipher).
    pub fn data_dir(&self) -> std::path::PathBuf {
        self.sidecar_path
            .parent()
            .map(std::path::Path::to_path_buf)
            .unwrap_or_else(|| std::path::PathBuf::from("."))
    }

    /// Record the IP at unlock time. Call from the HTTP handler that
    /// processes the unlock request.
    pub fn set_unlock_ip(&self, ip: String) {
        self.expect_write().unlock_ip = Some(ip);
    }

    /// Check the request IP against the unlock IP. If they differ,
    /// auto-lock and return Err(Locked). Callers should extract the
    /// IP from `Cf-Connecting-Ip` (tunnel) or the socket peer.
    ///
    /// Tailnet exception: requests coming from the Tailscale CGNAT
    /// range (`100.64.0.0/10`, RFC 6598) skip the pin check. Since the
    /// 2026-04-19 migration, Postern is reachable only via the
    /// Tailscale sidecar — every request is already authenticated at
    /// the network layer by Tailscale, and each device on the tailnet
    /// has its own 100.x address. Pinning to a single tailnet IP would
    /// make the desktop and phone repeatedly lock each other out. We
    /// keep the pin active for any non-CGNAT IP so that if public
    /// exposure ever comes back, the protection does too.
    pub fn check_ip(&self, current_ip: &str) -> Result<()> {
        if is_tailnet_cgnat(current_ip) {
            return Ok(());
        }
        let should_lock = {
            let guard = self.inner.read().map_err(|_| poisoned())?;
            match &guard.unlock_ip {
                Some(stored) if stored != current_ip => {
                    // If we unlocked from a tailnet device, we never
                    // stored a "real" public IP — the pin value is a
                    // 100.x address, and matching a non-CGNAT request
                    // against it is meaningless. Let it through.
                    if is_tailnet_cgnat(stored) {
                        return Ok(());
                    }
                    warn!(old = %stored, new = %current_ip, "IP changed — auto-locking vault");
                    true
                }
                _ => false,
            }
        };
        if should_lock {
            self.lock();
            let _ = self.db.log_event("ip_change_lock", Some(current_ip), None);
            return Err(Error::Locked(
                "IP address changed since unlock — re-enter master password".into(),
            ));
        }
        Ok(())
    }

    /// Open a per-device session. Called from the HTTP unlock handler
    /// after the master password (and any 2FA) has been verified.
    /// `token_hash` must already be the SHA-256 of the cookie token —
    /// callers should use `storage::devices::hash_token`. Replaces any
    /// existing session under the same hash so a re-unlock from the
    /// same device resets the idle/hard-cap clocks.
    pub fn session_open(&self, token_hash: String, ip: String) {
        let now = chrono::Utc::now().timestamp();
        let mut g = self.expect_write();
        g.sessions.insert(
            token_hash,
            DeviceSession {
                unlock_ip: ip,
                opened_at: now,
                last_seen_at: now,
            },
        );
    }

    /// Close a single device's session. Called from the lock endpoint
    /// when the user clicks "Lock" — this device's authenticated state
    /// is gone, but the global SQLCipher mount stays up so background
    /// sync, the outbox sender, and any other still-active devices
    /// keep working. Use `lock()` for the global wipe.
    pub fn session_close(&self, token_hash: &str) {
        self.expect_write().sessions.remove(token_hash);
    }

    /// Validate a request's session and refresh `last_seen_at` if it's
    /// still good. Returns `Ok(())` on success; `Err(Locked(_))` when
    /// no session exists, the idle window has elapsed since the last
    /// request, the hard cap has elapsed since unlock, or the request
    /// IP fails the same pin check the vault-wide pin uses (tailnet
    /// exempt, non-tailnet must match).
    ///
    /// Idle/hard-cap expiry also evicts the session so subsequent
    /// requests with the same cookie 401 immediately rather than
    /// re-checking timestamps.
    pub fn session_check(&self, token_hash: &str, current_ip: &str) -> Result<()> {
        let now = chrono::Utc::now().timestamp();
        let mut g = self.expect_write();
        let session = g.sessions.get(token_hash).cloned().ok_or_else(|| {
            Error::Locked("no active session for this device — re-enter master password".into())
        })?;
        match evaluate_session(now, &session, current_ip) {
            SessionVerdict::Ok => {
                if let Some(s) = g.sessions.get_mut(token_hash) {
                    s.last_seen_at = now;
                }
                Ok(())
            }
            SessionVerdict::IdleExpired => {
                g.sessions.remove(token_hash);
                drop(g);
                let _ = self
                    .db
                    .log_event("session_idle_lock", Some(current_ip), None);
                Err(Error::Locked(
                    "session idle too long — re-enter master password".into(),
                ))
            }
            SessionVerdict::HardCapExpired => {
                g.sessions.remove(token_hash);
                drop(g);
                let _ = self
                    .db
                    .log_event("session_hardcap_lock", Some(current_ip), None);
                Err(Error::Locked(
                    "session expired — re-enter master password".into(),
                ))
            }
            SessionVerdict::IpMismatch => {
                g.sessions.remove(token_hash);
                drop(g);
                let _ = self
                    .db
                    .log_event("session_ip_lock", Some(current_ip), None);
                Err(Error::Locked(
                    "IP address changed since unlock — re-enter master password".into(),
                ))
            }
        }
    }

    /// True iff this device has an active, non-idle, within-hard-cap
    /// session. Read-only (does not refresh `last_seen_at`) so the
    /// pre-unlock status probe doesn't accidentally extend a session
    /// that's about to time out.
    pub fn session_is_valid(&self, token_hash: &str) -> bool {
        let now = chrono::Utc::now().timestamp();
        let Ok(g) = self.inner.read() else {
            return false;
        };
        let Some(session) = g.sessions.get(token_hash) else {
            return false;
        };
        now - session.last_seen_at <= SESSION_IDLE_SECS
            && now - session.opened_at <= SESSION_HARD_CAP_SECS
    }

    /// Copy of the DB key, hex-encoded for `PRAGMA key = "x'<hex>'"`.
    /// Available only while unlocked; caller must zeroize after use.
    pub fn db_key_hex(&self) -> Option<Zeroizing<String>> {
        let guard = self.inner.read().ok()?;
        guard
            .db_key
            .as_ref()
            .map(|k| Zeroizing::new(hex::encode(k.as_slice())))
    }

    pub fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>> {
        let guard = self.inner.read().map_err(|_| poisoned())?;
        let kek = guard
            .kek
            .as_ref()
            .ok_or_else(|| Error::Locked("vault is locked".into()))?;
        aead_encrypt(kek, plaintext)
    }

    pub fn decrypt(&self, blob: &[u8]) -> Result<Vec<u8>> {
        let guard = self.inner.read().map_err(|_| poisoned())?;
        let kek = guard
            .kek
            .as_ref()
            .ok_or_else(|| Error::Locked("vault is locked".into()))?;
        match aead_decrypt(kek, blob) {
            Ok(v) => Ok(v),
            Err(_) => {
                // Legacy base64-only rows (pre-vault).
                if let Ok(decoded) = B64.decode(blob) {
                    warn!("decrypted legacy base64 secret — not yet re-wrapped");
                    return Ok(decoded);
                }
                Err(Error::Other(anyhow::anyhow!("secret decrypt failed")))
            }
        }
    }

    pub fn require_unlocked(&self) -> Result<()> {
        if !self.is_unlocked() {
            return Err(Error::Locked("vault is locked".into()));
        }
        Ok(())
    }

    /// Verify old password, derive new keys, re-wrap every secret,
    /// re-key the SQLCipher DB, and write a fresh sidecar.
    pub fn change_password(&self, old_password: &str, new_password: &str) -> Result<()> {
        if new_password.is_empty() || new_password.len() < 8 {
            return Err(Error::BadRequest(
                "new password must be at least 8 characters".into(),
            ));
        }
        // Verify old password first.
        let vf = sidecar::read(&self.sidecar_path)?
            .ok_or_else(|| Error::BadRequest("vault not initialised".into()))?;
        let old_salt = vf.salt_bytes()?;
        let old_verifier = vf.verifier_bytes()?;
        let old_master = argon2_derive(old_password, &old_salt)?;
        let (old_kek, _, _) = hkdf_split(&old_master);
        aead_decrypt(&old_kek, &old_verifier)
            .map_err(|_| Error::BadRequest("old password is wrong".into()))?;

        // Derive new keys.
        let mut new_salt = [0u8; SALT_LEN];
        rand::thread_rng().fill_bytes(&mut new_salt);
        let new_master = argon2_derive(new_password, &new_salt)?;
        let (new_kek, new_db_key, new_blob_key) = hkdf_split(&new_master);

        // Re-wrap every secret: old-KEK → new-KEK.
        self.rewrap_legacy_secrets(&old_kek, &new_kek)?;

        // Write new sidecar.
        let new_verifier = aead_encrypt(&new_kek, VERIFIER_PLAINTEXT)?;
        let new_vf = sidecar::VaultFile::new(
            &new_salt,
            &new_verifier,
            ARGON_M_COST_KIB,
            ARGON_T_COST,
            ARGON_P_COST,
        );
        sidecar::write(&self.sidecar_path, &new_vf)?;

        // Install new keys. Blob key also rotates here, but existing
        // ciphertext blobs on disk are still under the OLD blob key;
        // rewrap_blobs() re-encrypts them in place immediately after
        // the DB is re-keyed.
        let old_blob_key_for_rewrap = self
            .inner
            .read()
            .ok()
            .and_then(|g| g.blob_key.as_ref().map(|k| Zeroizing::new(**k)));
        self.install_keys(new_kek, new_db_key, new_blob_key.clone());

        // Re-key the SQLCipher DB in place.
        // PRAGMA rekey atomically re-encrypts the DB under the new key.
        let hex = self
            .db_key_hex()
            .ok_or_else(|| Error::Other(anyhow::anyhow!("no db key after install")))?;
        {
            let conn = self.db.pool().get()?;
            conn.execute_batch(&format!("PRAGMA rekey = \"x'{}'\";", *hex))
                .map_err(|e| Error::Other(anyhow::anyhow!("PRAGMA rekey: {e}")))?;
        }
        // Rebuild the pool so future connections use the new key.
        self.db.rekey_pool((*hex).clone())?;

        // Re-encrypt every blob from the old blob key to the new one.
        // Skipped silently when there's no old key on file (fresh
        // install that had never unlocked before the change-password
        // call — impossible today, future-proofing).
        if let (Some(old_bk), Some(ref blobs)) = (old_blob_key_for_rewrap, self.blobs.as_ref()) {
            blobs
                .rewrap(&old_bk, &new_blob_key)
                .map_err(|e| Error::Other(anyhow::anyhow!("blob rewrap: {e}")))?;
        }

        info!("master password changed, secrets re-wrapped, DB re-keyed, blobs re-wrapped");
        Ok(())
    }

    /// Apply the DB subkey to the pool. If the on-disk DB is still plain
    /// SQLite (pre-SQLCipher install), run the one-shot export migration
    /// first. Idempotent: safe to call on already-encrypted DBs.
    fn apply_db_encryption(&self) -> Result<()> {
        let hex = match self.db_key_hex() {
            Some(h) => h,
            None => return Err(Error::Locked("vault locked; no db key available".into())),
        };

        if self.db.is_plain_sqlite()? {
            info!("migrating plain SQLite → SQLCipher");
            let bytes = self.db.migrate_plain_to_sqlcipher(&hex)?;
            info!(bytes, "DB encryption migration complete");
        }
        self.db.rekey_pool((*hex).clone())?;

        // Run any migrations we skipped at startup because the DB was
        // still encrypted (idempotent — applied rows are skipped).
        self.db
            .migrate()
            .map_err(|e| Error::Other(anyhow::anyhow!("post-rekey migrate: {e}")))?;

        // Sanity check — hit the now-keyed pool with a trivial query
        // to confirm the key works.
        let conn = self.db.pool().get()?;
        conn.query_row("SELECT 1", [], |r| r.get::<_, i64>(0))
            .map_err(|e| Error::Other(anyhow::anyhow!("db verify after rekey: {e}")))?;
        drop(conn);

        // Reconcile any accounts whose stored kind disagrees with the
        // kind derived from their IMAP host. First-time unlock on an
        // upgraded install walks existing rows and fixes
        // mis-classifications (e.g. a cPanel account saved with
        // kind=gmail by a buggy setup flow).
        match self.db.reconcile_account_kinds() {
            Ok(0) => {}
            Ok(n) => info!(corrected = n, "account kinds reconciled from IMAP hosts"),
            Err(e) => tracing::warn!(error = %e, "account kind reconcile failed"),
        }

        // Sweep the messages corpus into the contacts table on every
        // unlock. Idempotent (ON CONFLICT IGNORE) so the cost on
        // repeat unlocks is just the message-table scan + hash
        // aggregate + cheap no-op inserts. Always log the breakdown
        // (scanned / unique / inserted) so a "0 new" result can be
        // distinguished from "0 messages found" or "0 addresses
        // parsed" — the previous always-zero log made it impossible
        // to tell which of those was happening.
        match self.db.backfill_contacts_diag() {
            Ok((scanned, unique, inserted)) => info!(
                scanned,
                unique_addresses = unique,
                inserted,
                "contacts: backfill swept messages corpus"
            ),
            Err(e) => tracing::warn!(error = %e, "contacts backfill failed (non-fatal)"),
        }

        // Install the blob-encryption key into the blob store and
        // run the one-shot migration pass for any pre-encryption
        // plaintext blobs left from before this feature shipped.
        // Idempotent — `migrate_encrypt_all` skips files that already
        // carry the version byte.
        self.apply_blob_encryption();

        // VPN reconcile failed at boot because the DB was still locked.
        // Now that we have a working keyed connection, retry so wg0
        // comes up without the user manually re-installing.
        if let Some(ref vpn) = self.vpn {
            if let Err(e) = vpn.reconcile_on_boot() {
                warn!(error = %e, "vpn post-unlock reconcile failed (non-fatal)");
            } else {
                info!("vpn reconciled after vault unlock");
            }
        }
        Ok(())
    }

    /// Push the blob key into the blob store and run the plaintext
    /// → ciphertext migration if any legacy blobs remain. Never
    /// returns an error: blob encryption is additive safety, not a
    /// correctness gate; a broken migration logs and is retried on
    /// next unlock rather than stopping the server from starting.
    fn apply_blob_encryption(&self) {
        let Some(ref blobs) = self.blobs else {
            warn!("blob store not wired into vault; skipping key install");
            return;
        };
        let key = match self.inner.read().ok().and_then(|g| g.blob_key.as_ref().map(|k| Zeroizing::new(**k))) {
            Some(k) => k,
            None => {
                warn!("no blob key after unlock; skipping");
                return;
            }
        };
        blobs.set_key(key);
        match blobs.migrate_encrypt_all() {
            Ok(0) => {}
            Ok(n) => info!(n, "migrated legacy plaintext blobs"),
            Err(e) => warn!(error = %e, "blob migration failed; will retry next unlock"),
        }
    }

    /// A poisoned inner RwLock means a previous holder panicked while
    /// mutating vault state — continuing with nil writes would silently
    /// leave the vault in an inconsistent state (keys not installed,
    /// failure counters frozen). Prefer a hard process exit so systemd
    /// restarts us into a clean state.
    fn expect_write(&self) -> std::sync::RwLockWriteGuard<'_, VaultInner> {
        self.inner
            .write()
            .expect("vault inner RwLock poisoned — a prior panic left state inconsistent")
    }

    fn record_failed_attempt(&self) {
        let mut g = self.expect_write();
        g.failed_attempts += 1;
        g.last_failed_at = chrono::Utc::now().timestamp();
    }

    fn reset_attempts(&self) {
        let mut g = self.expect_write();
        g.failed_attempts = 0;
        g.last_failed_at = 0;
    }

    fn install_keys(
        &self,
        kek: Zeroizing<[u8; 32]>,
        db_key: Zeroizing<[u8; 32]>,
        blob_key: Zeroizing<[u8; 32]>,
    ) {
        let mut g = self.expect_write();
        g.kek = Some(kek);
        g.db_key = Some(db_key);
        g.blob_key = Some(blob_key);
    }

    /// Borrow the blob key. Used by `BlobStore` via the `blob_key()`
    /// accessor when putting/getting blobs. None when vault is locked.
    pub fn blob_key(&self) -> Option<Zeroizing<[u8; 32]>> {
        self.inner
            .read()
            .ok()
            .and_then(|g| g.blob_key.as_ref().map(|k| Zeroizing::new(**k)))
    }

    fn read_legacy_config(&self) -> Result<Option<LegacyCfg>> {
        let conn = match self.db.pool().get() {
            Ok(c) => c,
            Err(_) => return Ok(None),
        };
        // The kek_config table may not even exist after Phase B2 drops
        // it; treat a missing table as "no legacy row".
        let row = conn
            .query_row(
                "SELECT salt, verifier FROM kek_config WHERE id = 1",
                [],
                |r| Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, Vec<u8>>(1)?)),
            )
            .optional();
        match row {
            Ok(Some((salt, verifier))) => Ok(Some(LegacyCfg { salt, verifier })),
            Ok(None) => Ok(None),
            Err(_) => Ok(None),
        }
    }

    fn migrate_legacy_secrets(&self) -> Result<()> {
        let conn = self.db.pool().get()?;
        let mut stmt = conn.prepare("SELECT ref, ciphertext FROM secrets")?;
        let rows: Vec<(String, Vec<u8>)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(stmt);

        let mut migrated = 0usize;
        for (ref_name, blob) in rows {
            if let Ok(guard) = self.inner.read() {
                if let Some(kek) = guard.kek.as_ref() {
                    if aead_decrypt(kek, &blob).is_ok() {
                        continue;
                    }
                }
            }
            let Ok(plain) = B64.decode(&blob) else {
                warn!(%ref_name, "secret is neither KEK-wrapped nor valid base64, skipping");
                continue;
            };
            let wrapped = self.encrypt(&plain)?;
            conn.execute(
                "UPDATE secrets SET ciphertext = ?1 WHERE ref = ?2",
                params![wrapped, ref_name],
            )?;
            migrated += 1;
        }
        if migrated > 0 {
            info!(migrated, "re-wrapped legacy secrets");
        }
        Ok(())
    }
}

/// Verify a master password against an arbitrary `VaultFile` and return
/// the derived DB key on success, without touching any installed vault
/// state. Used by the backup-restore flow to check a tarball's
/// embedded `vault.json` against the password the user just typed —
/// the live vault may be unlocked with a *different* password (the
/// current install's), and that one is irrelevant for opening the
/// backup's SQLCipher DB.
///
/// Wrong password returns `BadRequest("wrong master password")` —
/// same surface as `Vault::unlock` so the UI can show identical copy.
pub(crate) fn derive_db_key_from_sidecar(
    vf: &VaultFile,
    password: &str,
) -> Result<Zeroizing<[u8; 32]>> {
    let salt = vf.salt_bytes()?;
    let verifier = vf.verifier_bytes()?;
    let master = argon2_derive(password, &salt)?;
    let (kek, db_key, _blob_key) = hkdf_split(&master);
    aead_decrypt(&kek, &verifier)
        .map_err(|_| Error::BadRequest("wrong master password".into()))?;
    Ok(db_key)
}

struct LegacyCfg {
    salt: Vec<u8>,
    verifier: Vec<u8>,
}

fn poisoned() -> Error {
    Error::Other(anyhow::anyhow!("vault state lock poisoned"))
}

