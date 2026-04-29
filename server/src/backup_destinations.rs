//! Off-site backup destination drivers.
//!
//! Each destination kind has a `push` operation that takes a local
//! tarball path and uploads it to wherever. Today: SFTP and Google
//! Drive (OAuth-gated, scoped to drive.file).
//!
//! See `gdrive` for the Google Drive driver.
//!
//! All drivers run inside `tokio::task::spawn_blocking` from the HTTP
//! layer (or from the post-backup hook), so they can be fully async
//! without blocking axum's request thread on a slow remote.
//!
//! Hostkey policy: `accept_first_use` — store the first server key we
//! see on the destination row's metadata, then reject mismatches on
//! subsequent connects. For a v1 with a small known set of remotes
//! this is the sane default; we'll expose a "trust this fingerprint"
//! confirm button in a follow-up.
//!
//! Atomic uploads: write to `<remote_dir>/<filename>.partial`, then
//! `rename` to the final name once the byte count matches the source.
//! A network drop mid-transfer leaves only the .partial behind, which
//! the user can clean up safely.

use std::{
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};

use russh::{
    client,
    keys::{decode_secret_key, key::KeyPair, key::PublicKey},
};
use russh_sftp::{client::SftpSession, protocol::OpenFlags};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::{info, warn};

use crate::{
    error::{Error, Result},
    storage::{SftpCredential, SftpPublicConfig},
};

/// Maximum size of a single read from the local file before flushing
/// across SFTP. Tuned for a balance between fewer round-trips (bigger
/// is faster) and not stalling for a long time on a single packet
/// when the link is slow. 256 KB matches russh-sftp's defaults.
const SFTP_CHUNK: usize = 256 * 1024;

/// Wall-clock cap on the entire push. A backup tarball that takes
/// more than 30 minutes to upload is almost certainly a stuck
/// connection rather than slow-but-progressing.
const SFTP_TIMEOUT_SECS: u64 = 30 * 60;

/// Result of a push attempt — used by the caller to record state on
/// the destination row.
pub struct PushOutcome {
    pub remote_path: String,
    pub bytes_uploaded: u64,
}

/// Result of a connect probe. `captured` is what the server presented;
/// the caller persists it on first-use, or uses it for a "fingerprint
/// changed" diagnostic when `expected` was set and didn't match.
pub struct ConnectOutcome {
    /// The fingerprint the server presented this round.
    pub fingerprint: String,
}

/// Format `key.fingerprint()` (which is plain base64-no-pad of the
/// SHA-256) as `SHA256:<...>`, matching `ssh-keygen -lf` output.
pub fn format_ssh_fingerprint(key: &PublicKey) -> String {
    format!("SHA256:{}", key.fingerprint())
}

/// Pure helper: classify the relationship between a stored
/// fingerprint (None = TOFU mode) and a freshly-presented one.
/// Caller chooses what to do with each outcome — persist on
/// `FirstUse`, fail on `Mismatch`, proceed on `Match`.
pub(crate) enum FingerprintCheck<'a> {
    FirstUse(&'a str),
    Match,
    Mismatch { expected: &'a str, got: &'a str },
}

pub(crate) fn classify_fingerprint<'a>(
    expected: Option<&'a str>,
    presented: &'a str,
) -> FingerprintCheck<'a> {
    match expected {
        None => FingerprintCheck::FirstUse(presented),
        Some(e) if e == presented => FingerprintCheck::Match,
        Some(e) => FingerprintCheck::Mismatch {
            expected: e,
            got: presented,
        },
    }
}

/// SSH `Handler` implementing TOFU.
///
/// On every `check_server_key`:
///   1. Compute `SHA256:<fingerprint>` and stash it in `captured` so
///      the caller can read it after the connect (whether we accepted
///      or rejected).
///   2. If `expected` is `Some`, refuse the connection unless the
///      presented fingerprint matches — refusing inside the handler
///      means we never send the auth handshake to a possibly-MITM'd
///      peer, so password auth secrets stay local.
///   3. If `expected` is `None`, accept (TOFU first-use). The caller
///      persists `captured` after a successful connect.
struct TofuHandler {
    expected: Option<String>,
    captured: Arc<Mutex<Option<String>>>,
}

impl TofuHandler {
    fn new(expected: Option<String>) -> (Self, Arc<Mutex<Option<String>>>) {
        let captured = Arc::new(Mutex::new(None));
        (
            Self {
                expected,
                captured: captured.clone(),
            },
            captured,
        )
    }
}

#[async_trait::async_trait]
impl client::Handler for TofuHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &PublicKey,
    ) -> std::result::Result<bool, Self::Error> {
        let fp = format_ssh_fingerprint(server_public_key);
        if let Ok(mut g) = self.captured.lock() {
            *g = Some(fp.clone());
        }
        match classify_fingerprint(self.expected.as_deref(), &fp) {
            FingerprintCheck::FirstUse(_) => Ok(true),
            FingerprintCheck::Match => Ok(true),
            FingerprintCheck::Mismatch { expected, got } => {
                warn!(%expected, %got, "tofu: server hostkey changed — refusing connection");
                Ok(false)
            }
        }
    }
}

/// Connect, authenticate, and return an open SFTP session ready for
/// uploads, plus the verified server fingerprint. The caller is
/// responsible for keeping the session alive for the duration of the
/// upload — dropping it closes the channel.
///
/// `expected_fingerprint = None` means TOFU first-use (accept any
/// key, return what we saw so the caller can persist). `Some(fp)`
/// enforces match — connect aborts inside `check_server_key` if the
/// server presents a different key, before any auth secret crosses
/// the wire.
async fn open_sftp(
    config: &SftpPublicConfig,
    credential: &SftpCredential,
    expected_fingerprint: Option<&str>,
) -> Result<(client::Handle<TofuHandler>, SftpSession, String)> {
    let ssh_config = Arc::new(client::Config {
        inactivity_timeout: Some(Duration::from_secs(SFTP_TIMEOUT_SECS)),
        ..Default::default()
    });

    let (handler, captured) = TofuHandler::new(expected_fingerprint.map(str::to_owned));
    let addr = (config.host.as_str(), config.port);
    let connect_result = client::connect(ssh_config, addr, handler).await;
    // Read what was captured before propagating any error — if the
    // handler refused the key, we want the caller to see the
    // mismatching fingerprint in the error message.
    let presented = captured.lock().ok().and_then(|g| g.clone());
    let mut session = match connect_result {
        Ok(s) => s,
        Err(e) => {
            // Distinguish "server's key changed" from "TCP timed out".
            if let (Some(pres), Some(exp)) = (presented.as_deref(), expected_fingerprint) {
                if pres != exp {
                    return Err(Error::BadRequest(format!(
                        "server SSH host key changed since last connect — \
                         expected {exp}, got {pres}. \
                         If this is a legitimate key rotation, click \
                         \"Forget fingerprint\" on the destination row \
                         and retry. Otherwise treat this as a possible \
                         man-in-the-middle attack."
                    )));
                }
            }
            return Err(Error::Other(anyhow::anyhow!("ssh connect: {e}")));
        }
    };

    let fingerprint = presented
        .ok_or_else(|| Error::Other(anyhow::anyhow!("ssh handshake produced no hostkey")))?;

    let authed = match credential {
        SftpCredential::Password { password } => session
            .authenticate_password(&config.username, password)
            .await
            .map_err(|e| Error::Other(anyhow::anyhow!("ssh password auth: {e}")))?,
        SftpCredential::Key { key_pem, passphrase } => {
            let kp: KeyPair = decode_secret_key(key_pem, passphrase.as_deref())
                .map_err(|e| Error::BadRequest(format!("invalid private key: {e}")))?;
            session
                .authenticate_publickey(&config.username, Arc::new(kp))
                .await
                .map_err(|e| Error::Other(anyhow::anyhow!("ssh key auth: {e}")))?
        }
    };
    if !authed {
        return Err(Error::BadRequest(
            "SSH authentication rejected — check username + credentials".into(),
        ));
    }

    let channel = session
        .channel_open_session()
        .await
        .map_err(|e| Error::Other(anyhow::anyhow!("open channel: {e}")))?;
    channel
        .request_subsystem(true, "sftp")
        .await
        .map_err(|e| Error::Other(anyhow::anyhow!("request sftp: {e}")))?;
    let sftp = SftpSession::new(channel.into_stream())
        .await
        .map_err(|e| Error::Other(anyhow::anyhow!("open sftp session: {e}")))?;

    Ok((session, sftp, fingerprint))
}

/// Verify we can connect, auth, and stat the configured remote dir.
/// Returns the SHA-256 fingerprint of the server's hostkey so the
/// caller can persist it (TOFU first-use) or compare against a
/// stored value. No file is written.
///
/// `expected_fingerprint`: `None` accepts any (TOFU); `Some(fp)`
/// rejects with a friendly error pre-auth on mismatch.
pub async fn test(
    config: &SftpPublicConfig,
    credential: &SftpCredential,
    expected_fingerprint: Option<&str>,
) -> Result<ConnectOutcome> {
    let (mut session, sftp, fingerprint) =
        open_sftp(config, credential, expected_fingerprint).await?;
    sftp.metadata(&config.remote_dir).await.map_err(|e| {
        Error::BadRequest(format!(
            "remote_dir '{}' not accessible: {e} — does it exist and is it writable?",
            config.remote_dir
        ))
    })?;
    // Best-effort cleanup; ignore close failures.
    let _ = session.disconnect(russh::Disconnect::ByApplication, "ok", "").await;
    Ok(ConnectOutcome { fingerprint })
}

/// Stream `local_path` over SFTP into `<config.remote_dir>/<filename>`.
/// Atomic via .partial + rename. Returns the fingerprint the server
/// presented so the caller can persist it on first-use; on a TOFU
/// mismatch this returns BadRequest pre-auth.
pub async fn push(
    config: &SftpPublicConfig,
    credential: &SftpCredential,
    local_path: &Path,
    filename: &str,
    expected_fingerprint: Option<&str>,
) -> Result<(PushOutcome, String)> {
    if filename.contains('/')
        || filename.contains('\\')
        || filename.contains("..")
        || filename.is_empty()
    {
        return Err(Error::BadRequest("invalid filename for SFTP push".into()));
    }

    let (mut session, sftp, fingerprint) =
        open_sftp(config, credential, expected_fingerprint).await?;

    let final_path = join_remote(&config.remote_dir, filename);
    let partial_path = format!("{final_path}.partial");

    // Open local file as a tokio File so we can stream chunks.
    let mut local = tokio::fs::File::open(local_path)
        .await
        .map_err(|e| Error::Other(anyhow::anyhow!("open local tarball: {e}")))?;

    let mut remote = sftp
        .open_with_flags(
            &partial_path,
            OpenFlags::CREATE | OpenFlags::TRUNCATE | OpenFlags::WRITE,
        )
        .await
        .map_err(|e| Error::Other(anyhow::anyhow!("open remote {partial_path}: {e}")))?;

    let mut buf = vec![0u8; SFTP_CHUNK];
    let mut total = 0u64;
    loop {
        let n = local
            .read(&mut buf)
            .await
            .map_err(|e| Error::Other(anyhow::anyhow!("read local: {e}")))?;
        if n == 0 {
            break;
        }
        remote
            .write_all(&buf[..n])
            .await
            .map_err(|e| Error::Other(anyhow::anyhow!("write remote: {e}")))?;
        total += n as u64;
    }
    remote
        .shutdown()
        .await
        .map_err(|e| Error::Other(anyhow::anyhow!("close remote: {e}")))?;

    // Rename `.partial` → final name. Some servers don't overwrite on
    // rename; if the destination already exists we remove it first.
    let _ = sftp.remove_file(&final_path).await;
    sftp.rename(&partial_path, &final_path).await.map_err(|e| {
        Error::Other(anyhow::anyhow!(
            "rename {partial_path} → {final_path}: {e}"
        ))
    })?;

    info!(remote = %final_path, bytes = total, "sftp: pushed");

    let _ = session.disconnect(russh::Disconnect::ByApplication, "ok", "").await;

    Ok((
        PushOutcome {
            remote_path: final_path,
            bytes_uploaded: total,
        },
        fingerprint,
    ))
}

/// Join a remote dir and a filename. SFTP paths are POSIX even when
/// the server is Windows; always use `/`. Strips a trailing slash on
/// the dir so we never produce `//file`.
fn join_remote(dir: &str, filename: &str) -> String {
    let trimmed = dir.trim_end_matches('/');
    if trimmed.is_empty() {
        format!("/{filename}")
    } else {
        format!("{trimmed}/{filename}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_remote_handles_trailing_slash() {
        assert_eq!(join_remote("/home/user/backups", "x.tar.gz"), "/home/user/backups/x.tar.gz");
        assert_eq!(join_remote("/home/user/backups/", "x.tar.gz"), "/home/user/backups/x.tar.gz");
        assert_eq!(join_remote("/", "x.tar.gz"), "/x.tar.gz");
    }

    #[test]
    fn join_remote_handles_empty_dir() {
        assert_eq!(join_remote("", "x.tar.gz"), "/x.tar.gz");
    }

    #[test]
    fn classify_fingerprint_first_use_when_none_stored() {
        match classify_fingerprint(None, "SHA256:abc") {
            FingerprintCheck::FirstUse(fp) => assert_eq!(fp, "SHA256:abc"),
            _ => panic!("expected FirstUse"),
        }
    }

    #[test]
    fn classify_fingerprint_match_when_stored_equals_presented() {
        match classify_fingerprint(Some("SHA256:abc"), "SHA256:abc") {
            FingerprintCheck::Match => {}
            _ => panic!("expected Match"),
        }
    }

    #[test]
    fn classify_fingerprint_mismatch_when_stored_differs_from_presented() {
        match classify_fingerprint(Some("SHA256:abc"), "SHA256:xyz") {
            FingerprintCheck::Mismatch { expected, got } => {
                assert_eq!(expected, "SHA256:abc");
                assert_eq!(got, "SHA256:xyz");
            }
            _ => panic!("expected Mismatch"),
        }
    }

    #[test]
    fn classify_fingerprint_treats_format_strictly() {
        // Even a single-character difference must not match. The
        // whole point of TOFU is that a near-match is worse than no
        // match — we cannot reason about server identity from a
        // fingerprint that differs in any byte.
        match classify_fingerprint(Some("SHA256:abc"), "SHA256:abd") {
            FingerprintCheck::Mismatch { .. } => {}
            _ => panic!("expected Mismatch"),
        }
    }
}
