use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
};

use chacha20poly1305::{
    aead::{Aead, KeyInit},
    ChaCha20Poly1305, Key, Nonce,
};
use rand::RngCore;
use sha2::{Digest, Sha256};
use tracing::info;
use zeroize::Zeroizing;

use crate::error::{Error, Result};

/// SHA-256 addressed blob store for raw RFC822 messages.
///
/// Layout: `<root>/ab/cd/<full-hex>` — two-level fan-out to keep any
/// one directory from growing unbounded. Writes are atomic via rename.
///
/// ## At-rest encryption
///
/// Once the vault unlocks, callers install a 32-byte ChaCha20-Poly1305
/// key via `set_key`. From that point on `put` encrypts before writing
/// and `get` decrypts after reading.
///
/// On-disk layout of an encrypted blob:
///
/// ```text
/// [1 byte: version = 0x01] [12 bytes: nonce] [ciphertext || 16-byte tag]
/// ```
///
/// Version byte `0x01` is the "file is encrypted" discriminator used
/// during migration: RFC822 bodies always start with an ASCII header
/// character (`R`, `F`, `T`, …), so the leading `0x01` can't occur
/// naturally. A file that doesn't start with `0x01` is treated as
/// legacy plaintext — read-through still works so no message is ever
/// orphaned by a half-done migration, and `migrate_encrypt_all()`
/// re-encrypts any such files idempotently.
#[derive(Clone)]
pub struct BlobStore {
    root: PathBuf,
    key: Arc<RwLock<Option<Zeroizing<[u8; 32]>>>>,
}

const BLOB_VERSION: u8 = 0x01;
const NONCE_LEN: usize = 12;

impl BlobStore {
    pub fn new(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        fs::create_dir_all(&root)?;
        Ok(Self {
            root,
            key: Arc::new(RwLock::new(None)),
        })
    }

    /// Install the blob-encryption key. Called from `Vault::apply_blob_encryption`
    /// right after unlock. `None` here means subsequent writes land in
    /// plaintext — which only happens if the vault is locked, and in
    /// normal flow that never happens because we never write to the
    /// blob store without a key installed.
    pub fn set_key(&self, key: Zeroizing<[u8; 32]>) {
        if let Ok(mut g) = self.key.write() {
            *g = Some(key);
        }
    }

    pub fn clear_key(&self) {
        if let Ok(mut g) = self.key.write() {
            *g = None;
        }
    }

    fn current_key(&self) -> Option<Zeroizing<[u8; 32]>> {
        self.key
            .read()
            .ok()
            .and_then(|g| g.as_ref().map(|k| Zeroizing::new(**k)))
    }

    /// Hash and store `data`. Returns the hex-encoded SHA-256 of the
    /// plaintext (stable across migrations — the filename is the
    /// content hash, never changes with re-encryption).
    pub fn put(&self, data: &[u8]) -> Result<String> {
        let hex = hash_hex(data);
        let path = self.path_for(&hex);

        if path.exists() {
            // Dedup: same content already on disk. If it's plaintext and
            // we have a key, future migration will re-encrypt it.
            return Ok(hex);
        }

        let parent = path.parent().expect("blob path always has parent");
        fs::create_dir_all(parent)?;

        let bytes = match self.current_key() {
            Some(k) => encrypt_blob(&k, data)?,
            None => data.to_vec(),
        };

        let tmp = parent.join(format!(".tmp-{hex}"));
        fs::write(&tmp, &bytes)?;
        fs::rename(&tmp, &path)?;
        Ok(hex)
    }

    pub fn get(&self, hex: &str) -> Result<Vec<u8>> {
        let path = self.path_for(hex);
        let raw = fs::read(path)?;
        // Encrypted files start with 0x01. Anything else is legacy
        // plaintext from before the migration ran — return as-is so
        // existing messages keep rendering during the migration window.
        if raw.first() == Some(&BLOB_VERSION) {
            let key = self.current_key().ok_or_else(|| {
                Error::Locked("vault locked; cannot decrypt blob".into())
            })?;
            decrypt_blob(&key, &raw)
        } else {
            Ok(raw)
        }
    }

    fn path_for(&self, hex: &str) -> PathBuf {
        debug_assert_eq!(hex.len(), 64, "expected 64-char sha256 hex");
        self.root.join(&hex[0..2]).join(&hex[2..4]).join(hex)
    }

    /// Walk every blob file and re-encrypt any that are still in the
    /// legacy plaintext format. Called from `Vault::apply_blob_encryption`
    /// on the first post-unlock that finds the blob key installed but
    /// legacy blobs on disk. Idempotent: safe to re-run; already-encrypted
    /// files are skipped on the first-byte probe.
    ///
    /// Returns the count of files that were re-encrypted on this pass.
    /// A crash partway through leaves a mixed plaintext/ciphertext tree
    /// — the next invocation resumes.
    pub fn migrate_encrypt_all(&self) -> Result<usize> {
        let key = self
            .current_key()
            .ok_or_else(|| Error::Locked("blob key not installed".into()))?;
        let mut migrated = 0usize;
        let mut scanned = 0usize;
        walk_blobs(&self.root, &mut |path| {
            scanned += 1;
            // Peek first byte. Already-encrypted files skip.
            let mut first = [0u8; 1];
            if let Ok(mut f) = fs::File::open(path) {
                use std::io::Read;
                if f.read_exact(&mut first).is_err() {
                    return Ok(());
                }
            } else {
                return Ok(());
            }
            if first[0] == BLOB_VERSION {
                return Ok(());
            }

            // Plaintext — re-encrypt in place.
            let plain = fs::read(path)?;
            let enc = encrypt_blob(&key, &plain)?;
            let tmp = path.with_extension("mig");
            fs::write(&tmp, &enc)?;
            fs::rename(&tmp, path)?;
            migrated += 1;
            if migrated % 500 == 0 {
                info!(migrated, scanned, "blob migration progress");
            }
            Ok(())
        })?;
        if migrated > 0 {
            info!(migrated, scanned, "blob store encrypted at rest");
        }
        Ok(migrated)
    }

    /// Re-encrypt every blob from `old_key` to `new_key`. Used by the
    /// change-password path so the blob store keeps matching the
    /// freshly-rotated vault subkey. A crash partway leaves a mix —
    /// safe because `get()` would then fail to decrypt old blobs under
    /// the new key and refuse to silently corrupt. The caller is
    /// responsible for ordering: install the new key *before* calling
    /// rewrap so any inbound writes land under the new key.
    pub fn rewrap(
        &self,
        old_key: &Zeroizing<[u8; 32]>,
        new_key: &Zeroizing<[u8; 32]>,
    ) -> Result<usize> {
        let mut rewrapped = 0usize;
        walk_blobs(&self.root, &mut |path| {
            let raw = fs::read(path)?;
            if raw.first() != Some(&BLOB_VERSION) {
                // Legacy plaintext — migrate_encrypt_all() takes care
                // of these during normal unlock. Skip here to avoid
                // double-handling.
                return Ok(());
            }
            // Decrypt under old, re-encrypt under new.
            let plain = decrypt_blob(old_key, &raw)?;
            let fresh = encrypt_blob(new_key, &plain)?;
            let tmp = path.with_extension("rwr");
            fs::write(&tmp, &fresh)?;
            fs::rename(&tmp, path)?;
            rewrapped += 1;
            Ok(())
        })?;
        if rewrapped > 0 {
            info!(rewrapped, "blob store rewrapped to new key");
        }
        Ok(rewrapped)
    }
}

fn hash_hex(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    hex::encode(h.finalize())
}

fn encrypt_blob(key: &[u8; 32], plain: &[u8]) -> Result<Vec<u8>> {
    let cipher = ChaCha20Poly1305::new(Key::from_slice(key));
    let mut nonce = [0u8; NONCE_LEN];
    rand::thread_rng().fill_bytes(&mut nonce);
    let ct = cipher
        .encrypt(Nonce::from_slice(&nonce), plain)
        .map_err(|e| Error::Other(anyhow::anyhow!("blob encrypt: {e}")))?;
    let mut out = Vec::with_capacity(1 + NONCE_LEN + ct.len());
    out.push(BLOB_VERSION);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ct);
    Ok(out)
}

fn decrypt_blob(key: &[u8; 32], raw: &[u8]) -> Result<Vec<u8>> {
    if raw.len() < 1 + NONCE_LEN + 16 {
        return Err(Error::Other(anyhow::anyhow!(
            "blob too short to be encrypted"
        )));
    }
    if raw[0] != BLOB_VERSION {
        return Err(Error::Other(anyhow::anyhow!(
            "unexpected blob version byte"
        )));
    }
    let nonce = &raw[1..1 + NONCE_LEN];
    let ct = &raw[1 + NONCE_LEN..];
    let cipher = ChaCha20Poly1305::new(Key::from_slice(key));
    cipher
        .decrypt(Nonce::from_slice(nonce), ct)
        .map_err(|e| Error::Other(anyhow::anyhow!("blob decrypt: {e}")))
}

/// Walk every blob file under `root` (two-level fan-out) and run
/// `cb` on each. Ignores transient `.tmp-*` / `.mig` / `.rwr` files
/// left by crashed writes; a subsequent `put` on the same content
/// hash will just overwrite them.
fn walk_blobs<F>(root: &Path, cb: &mut F) -> Result<()>
where
    F: FnMut(&Path) -> Result<()>,
{
    let fan1 = match fs::read_dir(root) {
        Ok(d) => d,
        Err(_) => return Ok(()),
    };
    for a in fan1.flatten() {
        if !a.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let fan2 = match fs::read_dir(a.path()) {
            Ok(d) => d,
            Err(_) => continue,
        };
        for b in fan2.flatten() {
            if !b.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                continue;
            }
            let files = match fs::read_dir(b.path()) {
                Ok(d) => d,
                Err(_) => continue,
            };
            for entry in files.flatten() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if name_str.starts_with('.') || name_str.len() != 64 {
                    continue;
                }
                cb(&entry.path())?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_key() -> Zeroizing<[u8; 32]> {
        let mut k = [0u8; 32];
        for (i, v) in k.iter_mut().enumerate() {
            *v = i as u8;
        }
        Zeroizing::new(k)
    }

    #[test]
    fn plaintext_roundtrip_when_key_absent() {
        let dir = tempfile::tempdir().unwrap();
        let store = BlobStore::new(dir.path()).unwrap();
        let payload = b"From: alice\r\nSubject: hi\r\n\r\nbody";
        let hex = store.put(payload).unwrap();
        let got = store.get(&hex).unwrap();
        assert_eq!(got, payload);
    }

    #[test]
    fn encrypted_roundtrip_under_key() {
        let dir = tempfile::tempdir().unwrap();
        let store = BlobStore::new(dir.path()).unwrap();
        store.set_key(dummy_key());
        let payload = b"From: bob\r\nSubject: secret\r\n\r\nencrypted body";
        let hex = store.put(payload).unwrap();

        // Raw file on disk must start with the version byte and NOT
        // contain the plaintext.
        let on_disk = std::fs::read(
            dir.path()
                .join(&hex[0..2])
                .join(&hex[2..4])
                .join(&hex),
        )
        .unwrap();
        assert_eq!(on_disk[0], BLOB_VERSION);
        assert!(!on_disk.windows(6).any(|w| w == b"secret"));

        let got = store.get(&hex).unwrap();
        assert_eq!(got, payload);
    }

    #[test]
    fn plaintext_still_readable_after_key_install() {
        let dir = tempfile::tempdir().unwrap();
        let store = BlobStore::new(dir.path()).unwrap();

        // Simulate a legacy plaintext blob on disk.
        let payload = b"From: legacy\r\n\r\nold body";
        let hex = store.put(payload).unwrap();

        // Now install a key. get() must still return plaintext via the
        // first-byte fallback.
        store.set_key(dummy_key());
        let got = store.get(&hex).unwrap();
        assert_eq!(got, payload);
    }

    #[test]
    fn migrate_encrypts_plaintext_idempotently() {
        let dir = tempfile::tempdir().unwrap();
        let store = BlobStore::new(dir.path()).unwrap();
        // Write three legacy plaintext blobs.
        let h1 = store.put(b"plain one").unwrap();
        let _h2 = store.put(b"plain two with slightly more bytes").unwrap();
        let _h3 = store.put(b"plain three").unwrap();
        // Now install key and migrate.
        store.set_key(dummy_key());
        let migrated = store.migrate_encrypt_all().unwrap();
        assert_eq!(migrated, 3);
        // On-disk header is version byte.
        let on_disk = std::fs::read(
            dir.path().join(&h1[0..2]).join(&h1[2..4]).join(&h1),
        )
        .unwrap();
        assert_eq!(on_disk[0], BLOB_VERSION);
        // Second run is a no-op.
        let again = store.migrate_encrypt_all().unwrap();
        assert_eq!(again, 0);
        // Reads still return plaintext.
        assert_eq!(store.get(&h1).unwrap(), b"plain one");
    }

    #[test]
    fn rewrap_rotates_key_without_corruption() {
        let dir = tempfile::tempdir().unwrap();
        let store = BlobStore::new(dir.path()).unwrap();
        let k1 = dummy_key();
        store.set_key(k1.clone());
        let payload = b"secret message body";
        let hex = store.put(payload).unwrap();

        // Rotate.
        let k2 = {
            let mut k = [0u8; 32];
            for (i, v) in k.iter_mut().enumerate() {
                *v = (i as u8).wrapping_add(99);
            }
            Zeroizing::new(k)
        };
        store.set_key(k2.clone());
        let rewrapped = store.rewrap(&k1, &k2).unwrap();
        assert_eq!(rewrapped, 1);
        assert_eq!(store.get(&hex).unwrap(), payload);
    }
}
