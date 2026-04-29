//! On-disk vault metadata file.
//!
//! Why a sidecar and not the DB? Phase B encrypts the DB with SQLCipher,
//! so the salt + verifier can't live inside it — without reading them
//! first there's no way to know *which* key to try for the DB. The
//! sidecar keeps those bits alongside the DB but outside the encrypted
//! blob.
//!
//! Format: JSON, UTF-8, mode 0600. Atomic writes via temp + rename.

use std::{
    fs,
    io::Write,
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

pub const VAULT_FILE: &str = "vault.json";
const CURRENT_VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
pub struct VaultFile {
    pub version: u32,
    /// Base64-encoded 16-byte Argon2id salt.
    pub salt: String,
    /// Base64-encoded AEAD-wrapped known plaintext.
    pub verifier: String,
    pub params: VaultParams,
    pub created_at: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct VaultParams {
    pub algo: String,
    pub m_cost: u32,
    pub t_cost: u32,
    pub p_cost: u32,
}

impl VaultFile {
    pub fn new(salt: &[u8], verifier: &[u8], m_cost: u32, t_cost: u32, p_cost: u32) -> Self {
        Self {
            version: CURRENT_VERSION,
            salt: B64.encode(salt),
            verifier: B64.encode(verifier),
            params: VaultParams {
                algo: "argon2id".into(),
                m_cost,
                t_cost,
                p_cost,
            },
            created_at: chrono::Utc::now().timestamp(),
        }
    }

    pub fn salt_bytes(&self) -> Result<Vec<u8>> {
        B64.decode(&self.salt)
            .map_err(|e| Error::Other(anyhow::anyhow!("decode salt: {e}")))
    }

    pub fn verifier_bytes(&self) -> Result<Vec<u8>> {
        B64.decode(&self.verifier)
            .map_err(|e| Error::Other(anyhow::anyhow!("decode verifier: {e}")))
    }
}

pub fn sidecar_path(data_dir: &Path) -> PathBuf {
    data_dir.join(VAULT_FILE)
}

pub fn read(path: &Path) -> Result<Option<VaultFile>> {
    match fs::read(path) {
        Ok(bytes) => {
            let v: VaultFile = serde_json::from_slice(&bytes)
                .map_err(|e| Error::Other(anyhow::anyhow!("parse {path:?}: {e}")))?;
            Ok(Some(v))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Atomic write: temp file in same dir + rename. Mode 0600.
pub fn write(path: &Path, vf: &VaultFile) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(vf)
        .map_err(|e| Error::Other(anyhow::anyhow!("serialize vault: {e}")))?;
    let dir = path
        .parent()
        .ok_or_else(|| Error::Other(anyhow::anyhow!("no parent dir for {path:?}")))?;
    let tmp = dir.join(format!("{VAULT_FILE}.tmp"));
    {
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&tmp)?;
        f.write_all(&bytes)?;
        f.sync_all()?;
    }
    fs::rename(&tmp, path)?;
    Ok(())
}
