//! Pure cryptographic primitives for the vault.
//!
//! Lifted out of `vault/mod.rs` so the security-critical building
//! blocks live in one auditable file. **No state**, no IO, no DB —
//! just key derivation + AEAD wrappers. The vault module composes
//! these into the unlock/encrypt/decrypt flow.
//!
//! Algorithm choices (frozen — versioned via the constants below):
//!   * Argon2id, m=19 MiB, t=2, p=1 → 32-byte master key
//!   * HKDF-SHA256(master, info) → 32-byte subkey
//!   * ChaCha20-Poly1305 with a fresh 12-byte random nonce
//!
//! Three subkeys are derived per unlock:
//!   * KEK     — wraps in-DB secrets (license keys, app passwords, …)
//!   * DB key  — passed to SQLCipher's `PRAGMA key`
//!   * Blob key — at-rest blob encryption for the message store

use argon2::{Argon2, Params, Version};
use chacha20poly1305::{
    aead::{Aead, KeyInit},
    ChaCha20Poly1305, Key, Nonce,
};
use hkdf::Hkdf;
use rand::RngCore;
use sha2::Sha256;
use zeroize::Zeroizing;

use crate::error::{Error, Result};

/// Argon2id work factor. Sized so a modern laptop spends ~250ms per
/// derivation, which is comfortable for an interactive unlock and
/// painful for an attacker grinding the dictionary.
///
/// `pub(super)` because mod.rs writes them into the sidecar so future
/// unlocks know which parameters to re-derive against.
pub(super) const ARGON_M_COST_KIB: u32 = 19_456;
pub(super) const ARGON_T_COST: u32 = 2;
pub(super) const ARGON_P_COST: u32 = 1;

/// 12 bytes — the standard size for ChaCha20-Poly1305. Each `aead_encrypt`
/// call generates a fresh random nonce, prepended to the ciphertext.
pub const NONCE_LEN: usize = 12;

/// HKDF info strings — version-namespaced so a future v2 derivation
/// can coexist with v1 secrets in the same on-disk layout.
const HKDF_INFO_KEK: &[u8] = b"postern-kek-v1";
const HKDF_INFO_DB: &[u8] = b"postern-db-v1";
const HKDF_INFO_BLOB: &[u8] = b"postern-blob-v1";

/// Derive a 32-byte master key from a password + salt via Argon2id.
/// Output is zeroed on drop.
pub fn argon2_derive(password: &str, salt: &[u8]) -> Result<Zeroizing<[u8; 32]>> {
    let params = Params::new(ARGON_M_COST_KIB, ARGON_T_COST, ARGON_P_COST, Some(32))
        .map_err(|e| Error::Other(anyhow::anyhow!("argon2 params: {e}")))?;
    let argon = Argon2::new(argon2::Algorithm::Argon2id, Version::V0x13, params);
    let mut out = Zeroizing::new([0u8; 32]);
    argon
        .hash_password_into(password.as_bytes(), salt, out.as_mut_slice())
        .map_err(|e| Error::Other(anyhow::anyhow!("argon2 hash: {e}")))?;
    Ok(out)
}

/// One Argon2 output → three independent 32-byte keys via HKDF-SHA256.
/// Cost stays at a single Argon2 call per unlock. Returns
/// `(KEK, DB key, blob key)`.
pub fn hkdf_split(
    master: &[u8; 32],
) -> (
    Zeroizing<[u8; 32]>,
    Zeroizing<[u8; 32]>,
    Zeroizing<[u8; 32]>,
) {
    let hk = Hkdf::<Sha256>::new(None, master);
    let mut kek = Zeroizing::new([0u8; 32]);
    hk.expand(HKDF_INFO_KEK, kek.as_mut_slice())
        .expect("hkdf expand: kek length is valid");
    let mut db_key = Zeroizing::new([0u8; 32]);
    hk.expand(HKDF_INFO_DB, db_key.as_mut_slice())
        .expect("hkdf expand: db_key length is valid");
    let mut blob_key = Zeroizing::new([0u8; 32]);
    hk.expand(HKDF_INFO_BLOB, blob_key.as_mut_slice())
        .expect("hkdf expand: blob_key length is valid");
    (kek, db_key, blob_key)
}

/// Encrypt under a 32-byte AEAD key. Output is `nonce ‖ ciphertext`,
/// where the 12-byte nonce is freshly random per call. Decryption with
/// `aead_decrypt` is the inverse.
pub fn aead_encrypt(kek: &[u8; 32], plaintext: &[u8]) -> Result<Vec<u8>> {
    let cipher = ChaCha20Poly1305::new(Key::from_slice(kek));
    let mut nonce = [0u8; NONCE_LEN];
    rand::thread_rng().fill_bytes(&mut nonce);
    let ct = cipher
        .encrypt(Nonce::from_slice(&nonce), plaintext)
        .map_err(|e| Error::Other(anyhow::anyhow!("aead encrypt: {e}")))?;
    let mut out = Vec::with_capacity(NONCE_LEN + ct.len());
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ct);
    Ok(out)
}

/// Inverse of `aead_encrypt`. Rejects blobs shorter than the nonce
/// header (which would always be a logic bug) before reaching the
/// AEAD which would surface a generic decryption error instead.
pub fn aead_decrypt(kek: &[u8; 32], blob: &[u8]) -> Result<Vec<u8>> {
    if blob.len() <= NONCE_LEN {
        return Err(Error::Other(anyhow::anyhow!("aead blob too short")));
    }
    let (nonce, ct) = blob.split_at(NONCE_LEN);
    let cipher = ChaCha20Poly1305::new(Key::from_slice(kek));
    cipher
        .decrypt(Nonce::from_slice(nonce), ct)
        .map_err(|e| Error::Other(anyhow::anyhow!("aead decrypt: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn argon2_is_deterministic_for_same_inputs() {
        let salt = b"sixteen-bytes!!!";
        let a = argon2_derive("hunter2", salt).unwrap();
        let b = argon2_derive("hunter2", salt).unwrap();
        assert_eq!(a.as_slice(), b.as_slice());
    }

    #[test]
    fn argon2_diverges_on_different_password() {
        let salt = b"sixteen-bytes!!!";
        let a = argon2_derive("hunter2", salt).unwrap();
        let b = argon2_derive("hunter3", salt).unwrap();
        assert_ne!(a.as_slice(), b.as_slice());
    }

    #[test]
    fn hkdf_subkeys_are_distinct() {
        let master = [7u8; 32];
        let (kek, db, blob) = hkdf_split(&master);
        assert_ne!(kek.as_slice(), db.as_slice());
        assert_ne!(kek.as_slice(), blob.as_slice());
        assert_ne!(db.as_slice(), blob.as_slice());
    }

    #[test]
    fn aead_round_trip() {
        let key = [9u8; 32];
        let pt = b"Postern test plaintext";
        let blob = aead_encrypt(&key, pt).unwrap();
        let out = aead_decrypt(&key, &blob).unwrap();
        assert_eq!(out, pt);
    }

    #[test]
    fn aead_decrypt_rejects_tampered_ciphertext() {
        let key = [9u8; 32];
        let mut blob = aead_encrypt(&key, b"hello").unwrap();
        // Flip the last byte (inside the AEAD tag region) — must error.
        let last = blob.len() - 1;
        blob[last] ^= 0x01;
        assert!(aead_decrypt(&key, &blob).is_err());
    }

    #[test]
    fn aead_decrypt_rejects_short_blob() {
        let key = [9u8; 32];
        assert!(aead_decrypt(&key, &[]).is_err());
        assert!(aead_decrypt(&key, &[0u8; NONCE_LEN]).is_err());
    }
}
