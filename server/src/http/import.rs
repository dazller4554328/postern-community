//! Mail import endpoints.
//!
//! Three routes, each solving a different real-world migration:
//!
//!   POST /import/maildir     — path-based, expects files already on the
//!                              server at /var/lib/postern/import.
//!                              VPS-friendly: rsync a huge Maildir there,
//!                              call this once. No upload size cap.
//!
//!   POST /import/mbox        — raw body is one mbox file (Thunderbird,
//!                              Gmail Takeout, Apple Mail export). Server
//!                              splits on ^From  lines and upserts each
//!                              message.
//!
//!   POST /import/archive-zip — raw body is a zip of a Maildir tree or
//!                              a folder of .eml files. Server extracts
//!                              to a per-request tempdir, runs the same
//!                              walker as /import/maildir, tempdir is
//!                              dropped on return (Drop guard — even on
//!                              panic).
//!
//! All three share the same dispatch into `import_from_dir` / the mbox
//! splitter — the format detection happens at the endpoint boundary and
//! everything below that layer is "here's a pile of RFC822 blobs, shove
//! them into the database."
//!
//! Size caps: upload routes carry an explicit DefaultBodyLimit layer
//! (see routes()) so axum's global 2MiB default doesn't silently reject
//! 1GB archives. 2GB ceiling is belt-and-braces against a malicious
//! client uploading /dev/zero.

use std::{io::{Read, Write}, path::PathBuf};

use axum::{
    body::Bytes,
    extract::{DefaultBodyLimit, Query, State},
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use super::AppState;
use crate::error::{Error, Result};

/// 2GiB upper bound on any single import upload. Large enough for a
/// decade of Gmail (a Takeout mbox for ~40GB account lands ~18GB but
/// comes in multiple chunks), small enough that a bad client can't
/// fill our disk without noticing.
const MAX_UPLOAD_BYTES: usize = 2 * 1024 * 1024 * 1024;

/// Cap on the uncompressed size of an archive. Zip-bomb mitigation:
/// a 1KB zip can expand to gigabytes, so we refuse anything larger
/// than twice the upload cap uncompressed.
const MAX_EXTRACTED_BYTES: u64 = 4 * 1024 * 1024 * 1024;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/import/maildir", post(import_maildir))
        .route(
            "/import/mbox",
            post(import_mbox).layer(DefaultBodyLimit::max(MAX_UPLOAD_BYTES)),
        )
        .route(
            "/import/archive-zip",
            post(import_archive_zip).layer(DefaultBodyLimit::max(MAX_UPLOAD_BYTES)),
        )
}

// ---------------------------------------------------------------------
// Shared types
// ---------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct ImportRequest {
    path: String,
    /// When set, assign all messages to this account. When absent,
    /// auto-detect from Delivered-To / To / X-Original-To headers
    /// matched against configured accounts.
    account_id: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct UploadQuery {
    /// When set, tag every imported message with this account. When
    /// absent, the header-based auto-detector runs against configured
    /// accounts and skips messages it can't attribute.
    account_id: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct ImportReport {
    pub scanned: usize,
    pub imported: usize,
    pub skipped: usize,
    pub errors: usize,
}

// ---------------------------------------------------------------------
// Route handlers
// ---------------------------------------------------------------------

async fn import_maildir(
    State(s): State<AppState>,
    Json(req): Json<ImportRequest>,
) -> Result<Json<ImportReport>> {
    s.vault.require_unlocked()?;
    let fixed_account = match req.account_id {
        Some(id) => Some(s.db.get_account(id)?),
        None => None,
    };
    let accounts = s.db.list_accounts()?;
    let db = s.db.clone();
    let blobs = s.blobs.clone();
    let vault = s.vault.clone();
    // Restrict to the configured import mount to prevent path traversal.
    let allowed_root = PathBuf::from("/var/lib/postern/import");
    let path = allowed_root.join(req.path.trim_start_matches('/'));
    if !path.starts_with(&allowed_root) {
        return Err(Error::BadRequest("path outside allowed import root".into()));
    }

    let report = tokio::task::spawn_blocking(move || {
        import_from_dir(
            &path,
            fixed_account.as_ref(),
            &accounts,
            &db,
            &blobs,
            &vault,
        )
    })
    .await
    .map_err(|e| Error::Other(anyhow::anyhow!("join: {e}")))??;

    Ok(Json(report))
}

async fn import_mbox(
    State(s): State<AppState>,
    Query(q): Query<UploadQuery>,
    body: Bytes,
) -> Result<Json<ImportReport>> {
    s.vault.require_unlocked()?;
    let fixed_account = match q.account_id {
        Some(id) => Some(s.db.get_account(id)?),
        None => None,
    };
    let accounts = s.db.list_accounts()?;
    let db = s.db.clone();
    let blobs = s.blobs.clone();
    let vault = s.vault.clone();

    let report = tokio::task::spawn_blocking(move || {
        import_mbox_bytes(
            body.as_ref(),
            fixed_account.as_ref(),
            &accounts,
            &db,
            &blobs,
            &vault,
        )
    })
    .await
    .map_err(|e| Error::Other(anyhow::anyhow!("join: {e}")))??;

    Ok(Json(report))
}

async fn import_archive_zip(
    State(s): State<AppState>,
    Query(q): Query<UploadQuery>,
    body: Bytes,
) -> Result<Json<ImportReport>> {
    s.vault.require_unlocked()?;
    let fixed_account = match q.account_id {
        Some(id) => Some(s.db.get_account(id)?),
        None => None,
    };
    let accounts = s.db.list_accounts()?;
    let db = s.db.clone();
    let blobs = s.blobs.clone();
    let vault = s.vault.clone();

    let report = tokio::task::spawn_blocking(move || -> Result<ImportReport> {
        // Per-request tempdir — dropped on any return path, including
        // panic, so we never leak extracted mail to /tmp.
        let td = tempfile::tempdir()
            .map_err(|e| Error::Other(anyhow::anyhow!("tempdir: {e}")))?;
        extract_zip(body.as_ref(), td.path())?;
        import_from_dir(
            td.path(),
            fixed_account.as_ref(),
            &accounts,
            &db,
            &blobs,
            &vault,
        )
    })
    .await
    .map_err(|e| Error::Other(anyhow::anyhow!("join: {e}")))??;

    Ok(Json(report))
}

// ---------------------------------------------------------------------
// mbox splitter
// ---------------------------------------------------------------------

/// Walk an mbox file and hand each message (as a byte slice) to
/// `import_one`. The separator is the classic mboxo/mboxrd rule:
/// a line that starts with literal `From ` (five chars, no colon)
/// at the beginning of the file or immediately after a blank line.
///
/// This accepts mbox, mboxo, mboxrd, mboxcl without distinguishing —
/// they all share the `From ` boundary convention, they differ only
/// on how the body's own `>From ` lines are escaped. We don't
/// un-escape because the content the downstream parser cares about
/// (headers, attachment boundaries) isn't affected by the escape.
///
/// Returns (scanned, imported, skipped, errors) aggregated across
/// every message in the blob.
fn import_mbox_bytes(
    raw: &[u8],
    fixed_account: Option<&crate::storage::Account>,
    all_accounts: &[crate::storage::Account],
    db: &std::sync::Arc<crate::storage::Db>,
    blobs: &std::sync::Arc<crate::storage::BlobStore>,
    vault: &crate::vault::Vault,
) -> Result<ImportReport> {
    let mut report = ImportReport {
        scanned: 0,
        imported: 0,
        skipped: 0,
        errors: 0,
    };

    let email_map: std::collections::HashMap<String, i64> = all_accounts
        .iter()
        .map(|a| (a.email.to_ascii_lowercase(), a.id))
        .collect();

    for msg in split_mbox(raw) {
        report.scanned += 1;
        if msg.is_empty() {
            report.skipped += 1;
            continue;
        }

        let account_id = if let Some(a) = fixed_account {
            a.id
        } else {
            match detect_account(msg, &email_map) {
                Some(id) => id,
                None => {
                    report.skipped += 1;
                    continue;
                }
            }
        };

        match upsert_raw(msg, account_id, db, blobs, vault) {
            Ok(true) => report.imported += 1,
            Ok(false) => report.skipped += 1,
            Err(e) => {
                warn!(error = %e, "mbox: upsert failed");
                report.errors += 1;
            }
        }
    }

    info!(?report, "mbox import complete");
    Ok(report)
}

/// Iterator-ish splitter (returns Vec to keep the control flow simple).
/// Scans byte-by-byte for `\nFrom ` sequences at start-of-line and
/// cuts on them; the very first message starts at offset 0 if the
/// file begins with `From `, otherwise offset 0 is skipped (malformed
/// leading junk) and we wait for the first real boundary.
fn split_mbox(raw: &[u8]) -> Vec<&[u8]> {
    let mut out = Vec::new();

    // Find all byte offsets where a From-line begins. Windows-origin
    // mboxes (Thunderbird on Windows, some migration exports) use
    // CRLF throughout, so we match both `\nFrom ` and `\r\nFrom `.
    // The cut position is always "one past the \n" so the message
    // starts at the first byte of the `From ` envelope line; the
    // body_start scan below drops that envelope line uniformly for
    // both line-ending flavours.
    let mut cuts: Vec<usize> = Vec::new();
    if raw.starts_with(b"From ") {
        cuts.push(0);
    }
    let mut i = 0;
    while i < raw.len() {
        // Detect CRLF-From first so we don't double-count the inner \n.
        if i + 7 <= raw.len() && &raw[i..i + 7] == b"\r\nFrom " {
            cuts.push(i + 2);
            i += 7;
            continue;
        }
        if i + 6 <= raw.len() && &raw[i..i + 6] == b"\nFrom " {
            cuts.push(i + 1);
            i += 6;
            continue;
        }
        i += 1;
    }

    for (idx, &start) in cuts.iter().enumerate() {
        let end = cuts.get(idx + 1).copied().unwrap_or(raw.len());
        // Drop the leading `From ...` envelope line — most RFC822
        // parsers tolerate it but it isn't a real header. Find the
        // first LF after start.
        let body_start = raw[start..end]
            .iter()
            .position(|&b| b == b'\n')
            .map(|p| start + p + 1)
            .unwrap_or(end);
        if body_start >= end {
            continue;
        }
        out.push(&raw[body_start..end]);
    }

    out
}

// ---------------------------------------------------------------------
// Zip extraction
// ---------------------------------------------------------------------

/// Extract every file in `archive` into `dest`, refusing symlinks,
/// absolute paths, and `..` traversal. Enforces both MAX_EXTRACTED_BYTES
/// and a per-entry cap so a crafted archive can't fill the disk before
/// the total check fires.
fn extract_zip(archive: &[u8], dest: &std::path::Path) -> Result<()> {
    let reader = std::io::Cursor::new(archive);
    let mut zip = zip::ZipArchive::new(reader)
        .map_err(|e| Error::BadRequest(format!("not a zip archive: {e}")))?;

    let mut total: u64 = 0;

    for i in 0..zip.len() {
        let mut entry = zip
            .by_index(i)
            .map_err(|e| Error::Other(anyhow::anyhow!("zip entry {i}: {e}")))?;
        let name = entry.name().to_owned();

        // Reject anything that would escape dest.
        let rel = std::path::Path::new(&name);
        if rel.is_absolute()
            || rel
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err(Error::BadRequest(format!(
                "zip entry {name} has an unsafe path"
            )));
        }

        // Reject symlink entries outright — a crafted zip can contain
        // a symlink pointing outside dest followed by a regular-file
        // entry that writes through it. The Unix mode for a symlink
        // is 0o120000. Safer to refuse symlinks entirely than to try
        // to validate their targets.
        if let Some(mode) = entry.unix_mode() {
            const S_IFLNK: u32 = 0o120000;
            if (mode & 0o170000) == S_IFLNK {
                return Err(Error::BadRequest(format!(
                    "zip entry {name} is a symlink — refused"
                )));
            }
        }
        let out_path = dest.join(rel);

        if entry.is_dir() {
            std::fs::create_dir_all(&out_path)
                .map_err(|e| Error::Other(anyhow::anyhow!("mkdir {:?}: {e}", out_path)))?;
            continue;
        }

        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| Error::Other(anyhow::anyhow!("mkdir {:?}: {e}", parent)))?;
        }

        // Cap writes by actual bytes produced, not entry.size() — an
        // attacker can lie about declared size in the central directory.
        // Wrap the writer in a take-style guard that trips the overall
        // extracted-bytes budget.
        let mut f = std::fs::File::create(&out_path)
            .map_err(|e| Error::Other(anyhow::anyhow!("create {:?}: {e}", out_path)))?;
        let remaining = MAX_EXTRACTED_BYTES.saturating_sub(total);
        let mut limited = (&mut entry).take(remaining + 1);
        let written = std::io::copy(&mut limited, &mut f)
            .map_err(|e| Error::Other(anyhow::anyhow!("write {:?}: {e}", out_path)))?;
        total = total.saturating_add(written);
        if total > MAX_EXTRACTED_BYTES {
            return Err(Error::BadRequest(format!(
                "zip would exceed {}GiB uncompressed — refusing",
                MAX_EXTRACTED_BYTES / (1024 * 1024 * 1024)
            )));
        }
        f.flush()
            .map_err(|e| Error::Other(anyhow::anyhow!("flush {:?}: {e}", out_path)))?;
    }

    Ok(())
}

// ---------------------------------------------------------------------
// Shared: per-message insert + directory walker
// ---------------------------------------------------------------------

/// Parse a raw RFC822 message and upsert it. Returns true on insert,
/// false if the message was already in the DB (deduped by Message-ID).
fn upsert_raw(
    body: &[u8],
    account_id: i64,
    db: &std::sync::Arc<crate::storage::Db>,
    blobs: &std::sync::Arc<crate::storage::BlobStore>,
    vault: &crate::vault::Vault,
) -> Result<bool> {
    use crate::sync::parser;

    let parsed = parser::parse(body);
    let hash = blobs
        .put(body)
        .map_err(|e| Error::Other(anyhow::anyhow!("blob: {e}")))?;
    let is_encrypted = parser::is_pgp_encrypted(body);

    let nm = parser::into_new_message(
        account_id,
        parsed,
        hash,
        body.len(),
        vec!["INBOX".to_string()],
        None,
        true,
        is_encrypted,
    );

    let inserted = db
        .upsert_message(&nm)
        .map_err(|e| Error::Other(anyhow::anyhow!("db: {e}")))?;

    crate::pgp::harvest_autocrypt(body, db, vault);
    Ok(inserted)
}

fn import_from_dir(
    dir: &std::path::Path,
    fixed_account: Option<&crate::storage::Account>,
    all_accounts: &[crate::storage::Account],
    db: &std::sync::Arc<crate::storage::Db>,
    blobs: &std::sync::Arc<crate::storage::BlobStore>,
    vault: &crate::vault::Vault,
) -> Result<ImportReport> {
    use std::fs;

    let mut report = ImportReport {
        scanned: 0,
        imported: 0,
        skipped: 0,
        errors: 0,
    };

    let email_map: std::collections::HashMap<String, i64> = all_accounts
        .iter()
        .map(|a| (a.email.to_ascii_lowercase(), a.id))
        .collect();

    let files = collect_mail_files(dir)?;
    info!(count = files.len(), path = ?dir, accounts = email_map.len(), "import: scanning files");

    for path in &files {
        report.scanned += 1;
        let body = match fs::read(path) {
            Ok(b) => b,
            Err(e) => {
                warn!(file = ?path, error = %e, "import: read failed");
                report.errors += 1;
                continue;
            }
        };

        if body.is_empty() {
            report.skipped += 1;
            continue;
        }

        // If this looks like an mbox file (multiple `^From ` boundaries
        // inside), fan out — otherwise treat it as a single RFC822 blob.
        // Detection: at least one LF-From-space boundary beyond offset 0
        // AND the file starts with `From ` (otherwise it's probably an
        // .eml that just happens to have a quoted "From " in the body).
        if body.starts_with(b"From ")
            && body
                .windows(6)
                .any(|w| w == b"\nFrom " || w == b"\nFrom\t")
        {
            let sub = import_mbox_bytes(&body, fixed_account, all_accounts, db, blobs, vault)?;
            report.scanned = report.scanned - 1 + sub.scanned;
            report.imported += sub.imported;
            report.skipped += sub.skipped;
            report.errors += sub.errors;
            continue;
        }

        let account_id = if let Some(a) = fixed_account {
            a.id
        } else {
            match detect_account(&body, &email_map) {
                Some(id) => id,
                None => {
                    report.skipped += 1;
                    continue;
                }
            }
        };

        match upsert_raw(&body, account_id, db, blobs, vault) {
            Ok(true) => report.imported += 1,
            Ok(false) => report.skipped += 1,
            Err(e) => {
                warn!(error = %e, "import: upsert failed");
                report.errors += 1;
            }
        }

        if report.scanned % 1000 == 0 {
            info!(?report, "import: progress");
        }
    }

    info!(?report, "import complete");
    Ok(report)
}

/// Scan RFC822 headers for Delivered-To, X-Original-To, To, and Cc
/// and match against configured accounts. First match wins.
fn detect_account(raw: &[u8], email_map: &std::collections::HashMap<String, i64>) -> Option<i64> {
    let head_end = raw.len().min(8192);
    let head = &raw[..head_end];
    let text = String::from_utf8_lossy(head);

    for prefix in &["delivered-to:", "x-original-to:", "to:", "cc:"] {
        for line in text.lines() {
            let lower = line.to_ascii_lowercase();
            if lower.starts_with(prefix) {
                let value = &line[prefix.len()..];
                for addr in extract_emails(value) {
                    if let Some(&id) = email_map.get(&addr) {
                        return Some(id);
                    }
                }
            }
        }
    }
    None
}

fn extract_emails(value: &str) -> Vec<String> {
    let mut out = Vec::new();
    for chunk in value.split(',') {
        let trimmed = chunk.trim().to_ascii_lowercase();
        if let (Some(lt), Some(gt)) = (trimmed.find('<'), trimmed.rfind('>')) {
            if lt < gt {
                out.push(trimmed[lt + 1..gt].trim().to_string());
                continue;
            }
        }
        let bare = trimmed.trim_matches(|c: char| c.is_whitespace() || c == '"');
        if bare.contains('@') && !bare.contains(' ') {
            out.push(bare.to_string());
        }
    }
    out
}

fn collect_mail_files(dir: &std::path::Path) -> Result<Vec<PathBuf>> {
    use std::fs;
    use tracing::warn;

    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let entries = match fs::read_dir(&d) {
            Ok(e) => e,
            Err(e) => {
                // Log + skip — permission denied on a subdir shouldn't
                // fail the whole import, but silently dropping it is
                // worse than flagging it.
                warn!(path = ?d, error = %e, "import: cannot read directory, skipping");
                continue;
            }
        };
        for entry in entries {
            let Ok(entry) = entry else { continue };
            let ft = match entry.file_type() {
                Ok(t) => t,
                Err(_) => continue,
            };
            if ft.is_dir() {
                stack.push(entry.path());
            } else if ft.is_file() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                // Skip Maildir control files and dotfiles.
                if name_str != "wervd.ver" && !name_str.starts_with('.') {
                    out.push(entry.path());
                }
            }
        }
    }
    out.sort();
    Ok(out)
}

// ---------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_mbox_handles_single_message() {
        let raw = b"From foo@bar Thu Jan  1 00:00:00 2026\n\
                    From: sender@example.com\n\
                    To: recipient@example.com\n\
                    Subject: hello\n\
                    \n\
                    body text\n";
        let msgs = split_mbox(raw);
        assert_eq!(msgs.len(), 1);
        assert!(msgs[0].starts_with(b"From: sender@example.com"));
    }

    #[test]
    fn split_mbox_splits_multiple_messages() {
        let raw = b"From foo Thu Jan 1 00:00:00 2026\n\
                    Subject: one\n\
                    \n\
                    first\n\
                    From bar Thu Jan 2 00:00:00 2026\n\
                    Subject: two\n\
                    \n\
                    second\n";
        let msgs = split_mbox(raw);
        assert_eq!(msgs.len(), 2);
        assert!(msgs[0].starts_with(b"Subject: one"));
        assert!(msgs[1].starts_with(b"Subject: two"));
    }

    #[test]
    fn split_mbox_empty_input() {
        assert!(split_mbox(b"").is_empty());
    }

    #[test]
    fn split_mbox_handles_crlf_line_endings() {
        // Windows/Thunderbird exports use CRLF throughout. The
        // splitter must match `\r\nFrom ` just like `\nFrom `.
        let raw = b"From a Thu Jan 1 2026\r\n\
                    Subject: one\r\n\r\n\
                    first\r\n\
                    From b Thu Jan 2 2026\r\n\
                    Subject: two\r\n\r\n\
                    second\r\n";
        let msgs = split_mbox(raw);
        assert_eq!(msgs.len(), 2);
        assert!(msgs[0].starts_with(b"Subject: one"));
        assert!(msgs[1].starts_with(b"Subject: two"));
    }

    #[test]
    fn split_mbox_no_leading_from_is_tolerated() {
        // A file that starts with junk but contains valid boundaries.
        let raw = b"garbage\n\
                    From ok Thu Jan 1 2026\n\
                    Subject: real\n\
                    \n\
                    body\n";
        let msgs = split_mbox(raw);
        assert_eq!(msgs.len(), 1);
        assert!(msgs[0].starts_with(b"Subject: real"));
    }
}
