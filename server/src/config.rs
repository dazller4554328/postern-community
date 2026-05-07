use std::{env, path::PathBuf};

use anyhow::Context;

#[derive(Debug, Clone)]
pub struct Config {
    /// `host:port` for the HTTP server. Defaults to `127.0.0.1:8080` — we
    /// rely on Cloudflare Tunnel (`cloudflared`) for public ingress, so the
    /// app must never bind a public interface directly.
    pub bind: String,

    /// Data root — holds the DB, blobs, and vault sidecar.
    pub data_dir: PathBuf,

    /// Absolute path to the SQLite (eventually SQLCipher) database file.
    pub db_path: PathBuf,

    /// Directory holding raw RFC822 message blobs, addressed by SHA-256.
    pub blob_dir: PathBuf,

    /// Directory holding static assets (the built SvelteKit SPA).
    /// When empty, static serving is disabled (useful for API-only dev).
    pub static_dir: Option<PathBuf>,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let bind = env::var("POSTERN_BIND").unwrap_or_else(|_| "127.0.0.1:8080".into());

        let data_dir = env::var("POSTERN_DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("./data"));

        let db_path = env::var("POSTERN_DB_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| data_dir.join("postern.db"));

        let blob_dir = env::var("POSTERN_BLOB_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| data_dir.join("blobs"));

        let static_dir = env::var("POSTERN_STATIC_DIR").ok().map(PathBuf::from);

        std::fs::create_dir_all(&blob_dir).context("ensure blob_dir")?;
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).context("ensure db_path parent")?;
        }

        Ok(Self {
            bind,
            data_dir,
            db_path,
            blob_dir,
            static_dir,
        })
    }
}
