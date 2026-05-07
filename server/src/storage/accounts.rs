use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use super::Db;
use crate::{
    error::{Error, Result},
    vault::Vault,
};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AccountKind {
    Gmail,
    Imap,
}

impl AccountKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Gmail => "gmail",
            Self::Imap => "imap",
        }
    }

}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ArchiveStrategy {
    /// All archived messages land in the base archive folder.
    Single,
    /// Bucket by message year — e.g. `Archive/2026`.
    Yearly,
    /// Bucket by year/month — e.g. `Archive/2026/03`.
    Monthly,
}

impl ArchiveStrategy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Single => "single",
            Self::Yearly => "yearly",
            Self::Monthly => "monthly",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "single" => Ok(Self::Single),
            "yearly" => Ok(Self::Yearly),
            "monthly" => Ok(Self::Monthly),
            other => Err(crate::error::Error::BadRequest(format!(
                "unknown archive strategy: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Account {
    pub id: i64,
    pub kind: AccountKind,
    pub email: String,
    pub display_name: Option<String>,
    pub imap_host: String,
    pub imap_port: u16,
    pub smtp_host: Option<String>,
    pub smtp_port: Option<u16>,
    pub vpn_required: bool,
    pub delete_after_sync: bool,
    pub created_at: i64,
    /// User-configured archive folder base. NULL falls back to `Archive`
    /// (works for both Gmail and plain IMAP — Gmail treats it as a label).
    pub archive_folder: Option<String>,
    /// How the archive folder gets subdivided (single / yearly / monthly).
    pub archive_strategy: ArchiveStrategy,
    /// Whether Archive actions are exposed for this mailbox. When false
    /// the Archive button is hidden in the UI and auto-archive skips
    /// this account entirely.
    pub archive_enabled: bool,
    /// Master toggle for the scheduler-driven auto-archive sweep.
    pub auto_archive_enabled: bool,
    /// Messages older than this many days are eligible for auto-archive.
    pub auto_archive_age_days: i32,
    /// When true, auto-archive only considers messages marked read.
    pub auto_archive_read_only: bool,
    /// User-chosen seed for the robohash avatar. NULL falls back to the
    /// account's email address at the client.
    pub avatar_seed: Option<String>,
    /// Which robohash collection to render: set1 (classic robots),
    /// set2 (monsters), set3 (robot heads), set4 (kittens), set5 (geometric).
    pub avatar_set: String,
    /// Retention sweep: when true, messages older than `retention_days`
    /// are deleted from the IMAP server on each sync cycle. The local
    /// copy in Postern is preserved — this only frees provider-side
    /// quota. Scope is fixed: INBOX + user labels + Gmail categories;
    /// Sent/Drafts/Starred/Important/Archive/Spam/Trash are never
    /// touched.
    pub retention_enabled: bool,
    pub retention_days: i32,
    /// Gmail-only. When on *together with* `delete_after_sync`, the
    /// scheduler runs a post-sync pass over the five Gmail categories
    /// (Updates/Promotions/Social/Forums/Purchases) via X-GM-RAW and
    /// MOVEs every matched UID to [Gmail]/Trash. This picks up
    /// messages that normal folder sync can't see because Gmail hides
    /// categories from IMAP's folder list.
    pub purge_gmail_categories: bool,
    /// Paired with `purge_gmail_categories`. When on, the purge pass
    /// also permanently deletes everything in [Gmail]/Trash after
    /// moving category matches in — skipping Gmail's 30-day trash
    /// timer and freeing quota immediately. Wipes the entire Trash
    /// mailbox, not just purge targets.
    pub skip_gmail_trash: bool,
    /// Per-account signature. `signature_html` is used when the compose
    /// body is HTML; `signature_plain` mirrors it for plain-text sends.
    /// NULL means no signature. Auto-insertion into replies is a
    /// client-side preference, not stored here.
    pub signature_html: Option<String>,
    pub signature_plain: Option<String>,
    /// Master switch for inbound. When false, the scheduler skips this
    /// account entirely — no IMAP pulls, no auto-archive, no retention
    /// sweep. Row stays intact so the user can re-enable without
    /// re-entering credentials.
    pub sync_enabled: bool,
    /// Master switch for outbound. When false, SMTP send returns a
    /// clear error before touching the network.
    pub send_enabled: bool,
    /// Whether this mailbox participates in the Unified views
    /// (cross-account Inbox/Sent/Drafts/Spam/Trash + All mail).
    /// When false the mailbox still syncs and is visible per-account
    /// in the sidebar, but its messages are excluded from the
    /// aggregate surfaces.
    pub include_in_unified: bool,
    /// Per-account display colour as a hex string (`#3b82f6`). Drives
    /// the unread-indicator pill in the inbox row so the user can tell
    /// at a glance which mailbox a message landed in. NULL = client
    /// computes a deterministic default from `id`.
    pub color: Option<String>,
}

impl Account {
    /// The base archive folder without any date bucketing applied.
    /// Honours the user override, otherwise falls back to `Archive`.
    /// Gmail surfaces this as a label; plain IMAP as a folder.
    pub fn archive_folder_base(&self) -> &str {
        self.archive_folder
            .as_deref()
            .map(|s| s.trim_matches('/').trim())
            .filter(|s| !s.is_empty())
            .unwrap_or("Archive")
    }

    /// Compute the bucketed archive folder for a message with the given
    /// unix-epoch timestamp. Thunderbird-style: the bucket tracks the
    /// *message date*, not the archive-time, so old mail lands in its
    /// original year/month even when archived today.
    pub fn archive_folder_for(&self, message_ts_utc: i64) -> String {
        let base = self.archive_folder_base().to_owned();
        // Flat single match over every variant — adding a future
        // ArchiveStrategy now fails the compile instead of silently
        // falling through a nested unreachable!() arm.
        match self.archive_strategy {
            ArchiveStrategy::Single => base,
            ArchiveStrategy::Yearly => {
                let dt = chrono::DateTime::<chrono::Utc>::from_timestamp(message_ts_utc, 0)
                    .unwrap_or_else(chrono::Utc::now);
                format!("{base}/{}", dt.format("%Y"))
            }
            ArchiveStrategy::Monthly => {
                let dt = chrono::DateTime::<chrono::Utc>::from_timestamp(message_ts_utc, 0)
                    .unwrap_or_else(chrono::Utc::now);
                format!("{base}/{}/{}", dt.format("%Y"), dt.format("%m"))
            }
        }
    }
}

/// Derive the account kind from the IMAP hostname alone. This is the
/// single source of truth for `AccountKind` — we do NOT trust the
/// kind the client claims, and we do NOT trust what's stored in the
/// row, because either can drift out of sync with the real host
/// (that's the bug where a 7xhosting.com account shipped with
/// kind=gmail because the setup flow had that selected when the
/// host was typed in).
///
/// The rule is deliberately narrow: a request is "Gmail" iff the
/// IMAP host is Google's official IMAP endpoint. Google Workspace
/// domains still use `imap.gmail.com`, so this catches them too.
/// A proxy/relay that happens to talk to Gmail behind the scenes
/// wouldn't expose Gmail's IMAP extensions (X-GM-LABELS / X-GM-RAW),
/// so treating it as plain IMAP is the safe default.
pub fn detect_kind_from_host(imap_host: &str) -> AccountKind {
    let host = imap_host.trim().to_ascii_lowercase();
    if host == "imap.gmail.com" || host == "imap.googlemail.com" {
        AccountKind::Gmail
    } else {
        AccountKind::Imap
    }
}

#[derive(Debug, Deserialize)]
pub struct NewAccount {
    #[serde(default = "default_imap_kind")]
    pub kind: AccountKind,
    pub email: String,
    pub display_name: Option<String>,
    pub imap_host: String,
    pub imap_port: u16,
    pub smtp_host: Option<String>,
    pub smtp_port: Option<u16>,
    /// App password. Stored base64-wrapped in `secrets` for Sprint 1.
    /// Sprint 5 replaces this with KEK-encrypted storage derived from the
    /// user's master password.
    pub app_password: String,
    #[serde(default)]
    pub vpn_required: bool,
    #[serde(default)]
    pub delete_after_sync: bool,
}

fn default_imap_kind() -> AccountKind {
    AccountKind::Imap
}

impl Db {
    pub fn insert_account(&self, new: &NewAccount, vault: &Vault) -> Result<Account> {
        // Tier cap. Community builds are limited to 3 mailboxes; paid
        // has no cap. Enforced at the storage layer so every code path
        // that inserts an account (HTTP, setup wizard, test harness,
        // any future importer) runs through one check.
        if let Some(max) = crate::tier::MAX_MAILBOXES {
            let existing = self.count_accounts()?;
            if existing >= max {
                return Err(Error::BadRequest(format!(
                    "mailbox limit reached ({max}) — this is Postern Community. \
                     Upgrade to Postern to add more accounts."
                )));
            }
        }

        let mut conn = self.pool().get()?;
        let tx = conn.transaction()?;

        let cred_ref = format!("acct:{}", new.email);
        let wrapped = vault.encrypt(new.app_password.as_bytes())?;
        tx.execute(
            "INSERT INTO secrets(ref, ciphertext) VALUES (?1, ?2)
             ON CONFLICT(ref) DO UPDATE SET ciphertext = excluded.ciphertext",
            params![cred_ref, wrapped],
        )?;

        // Ignore whatever kind the client claimed — derive from the
        // IMAP host. This is the single write-time gate that keeps
        // `kind` honest for every future account, no matter how the
        // setup flow handed it to us.
        let kind = detect_kind_from_host(&new.imap_host);
        if kind != new.kind {
            tracing::info!(
                claimed = new.kind.as_str(),
                derived = kind.as_str(),
                host = %new.imap_host,
                "account kind corrected from IMAP host"
            );
        }
        let created_at = chrono::Utc::now().timestamp();
        tx.execute(
            "INSERT INTO accounts
                (kind, email, display_name, imap_host, imap_port,
                 smtp_host, smtp_port, credential_ref, vpn_required,
                 delete_after_sync, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                kind.as_str(),
                new.email,
                new.display_name,
                new.imap_host,
                new.imap_port,
                new.smtp_host,
                new.smtp_port,
                cred_ref,
                i32::from(new.vpn_required),
                i32::from(new.delete_after_sync),
                created_at,
            ],
        )?;
        let id = tx.last_insert_rowid();
        tx.commit()?;
        self.get_account(id)
    }

    pub fn get_account(&self, id: i64) -> Result<Account> {
        let conn = self.pool().get()?;
        conn.query_row(
            "SELECT id, kind, email, display_name, imap_host, imap_port,
                    smtp_host, smtp_port, vpn_required, delete_after_sync, created_at,
                    archive_folder, archive_strategy, archive_enabled,
                    auto_archive_enabled, auto_archive_age_days, auto_archive_read_only,
                    avatar_seed, avatar_set,
                    retention_enabled, retention_days,
                    purge_gmail_categories, skip_gmail_trash,
                    signature_html, signature_plain,
                    sync_enabled, send_enabled,
                    include_in_unified, color
             FROM accounts WHERE id = ?1",
            params![id],
            row_to_account,
        )
        .optional()?
        .ok_or(crate::error::Error::NotFound)
    }

    /// One-shot reconciliation: walk every account and rewrite the
    /// stored `kind` column to match what the IMAP host actually
    /// implies. No-op when rows are already correct. Called at
    /// startup so the audit log reflects the derived state even
    /// though runtime reads don't depend on it.
    pub fn reconcile_account_kinds(&self) -> Result<usize> {
        let conn = self.pool().get()?;
        let mut stmt = conn.prepare("SELECT id, imap_host, kind FROM accounts")?;
        let rows: Vec<(i64, String, String)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(stmt);

        let mut corrected = 0usize;
        for (id, host, stored) in rows {
            let want = detect_kind_from_host(&host);
            if want.as_str() != stored {
                conn.execute(
                    "UPDATE accounts SET kind = ?1 WHERE id = ?2",
                    rusqlite::params![want.as_str(), id],
                )?;
                tracing::info!(
                    account_id = id,
                    from = stored,
                    to = want.as_str(),
                    %host,
                    "reconciled account kind from host"
                );
                corrected += 1;
            }
        }
        Ok(corrected)
    }

    /// Cheap COUNT(*) — avoids materialising every account row just
    /// to gate-check the tier cap at insert time.
    pub fn count_accounts(&self) -> Result<usize> {
        let conn = self.pool().get()?;
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM accounts", [], |r| r.get(0))?;
        Ok(n as usize)
    }

    pub fn list_accounts(&self) -> Result<Vec<Account>> {
        let conn = self.pool().get()?;
        let mut stmt = conn.prepare(
            "SELECT id, kind, email, display_name, imap_host, imap_port,
                    smtp_host, smtp_port, vpn_required, delete_after_sync, created_at,
                    archive_folder, archive_strategy, archive_enabled,
                    auto_archive_enabled, auto_archive_age_days, auto_archive_read_only,
                    avatar_seed, avatar_set,
                    retention_enabled, retention_days,
                    purge_gmail_categories, skip_gmail_trash,
                    signature_html, signature_plain,
                    sync_enabled, send_enabled,
                    include_in_unified, color
             FROM accounts ORDER BY created_at ASC",
        )?;
        let rows = stmt.query_map([], row_to_account)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Set or clear the per-account display colour. Stores the hex
    /// string verbatim — validation lives in the HTTP handler so the
    /// storage layer doesn't have to care what counts as "valid hex".
    /// Pass `None` to revert to the client-side default.
    pub fn set_account_color(&self, id: i64, color: Option<&str>) -> Result<()> {
        let conn = self.pool().get()?;
        let normalized = color.map(|s| s.trim()).filter(|s| !s.is_empty());
        let n = conn.execute(
            "UPDATE accounts SET color = ?1 WHERE id = ?2",
            rusqlite::params![normalized, id],
        )?;
        if n == 0 {
            return Err(crate::error::Error::NotFound);
        }
        Ok(())
    }

    pub fn set_archive_folder(&self, id: i64, folder: Option<&str>) -> Result<()> {
        let conn = self.pool().get()?;
        let normalized = folder.map(|s| s.trim()).filter(|s| !s.is_empty());
        let n = conn.execute(
            "UPDATE accounts SET archive_folder = ?1 WHERE id = ?2",
            rusqlite::params![normalized, id],
        )?;
        if n == 0 {
            return Err(crate::error::Error::NotFound);
        }
        Ok(())
    }

    pub fn set_archive_strategy(&self, id: i64, strategy: ArchiveStrategy) -> Result<()> {
        let conn = self.pool().get()?;
        let n = conn.execute(
            "UPDATE accounts SET archive_strategy = ?1 WHERE id = ?2",
            rusqlite::params![strategy.as_str(), id],
        )?;
        if n == 0 {
            return Err(crate::error::Error::NotFound);
        }
        Ok(())
    }

    pub fn set_archive_enabled(&self, id: i64, enabled: bool) -> Result<()> {
        let conn = self.pool().get()?;
        let n = conn.execute(
            "UPDATE accounts SET archive_enabled = ?1 WHERE id = ?2",
            rusqlite::params![i32::from(enabled), id],
        )?;
        if n == 0 {
            return Err(crate::error::Error::NotFound);
        }
        Ok(())
    }

    pub fn set_purge_gmail_categories(&self, id: i64, enabled: bool) -> Result<()> {
        let conn = self.pool().get()?;
        let n = conn.execute(
            "UPDATE accounts SET purge_gmail_categories = ?1 WHERE id = ?2",
            rusqlite::params![i32::from(enabled), id],
        )?;
        if n == 0 {
            return Err(crate::error::Error::NotFound);
        }
        Ok(())
    }

    pub fn set_skip_gmail_trash(&self, id: i64, enabled: bool) -> Result<()> {
        let conn = self.pool().get()?;
        let n = conn.execute(
            "UPDATE accounts SET skip_gmail_trash = ?1 WHERE id = ?2",
            rusqlite::params![i32::from(enabled), id],
        )?;
        if n == 0 {
            return Err(crate::error::Error::NotFound);
        }
        Ok(())
    }

    /// Replace the signature for an account. Empty strings clear to NULL
    /// so the compose view treats "no signature" uniformly.
    pub fn set_signature(
        &self,
        id: i64,
        html: Option<&str>,
        plain: Option<&str>,
    ) -> Result<()> {
        let conn = self.pool().get()?;
        let html = html.map(str::trim).filter(|s| !s.is_empty());
        let plain = plain.map(str::trim).filter(|s| !s.is_empty());
        let n = conn.execute(
            "UPDATE accounts SET signature_html = ?1, signature_plain = ?2 WHERE id = ?3",
            rusqlite::params![html, plain, id],
        )?;
        if n == 0 {
            return Err(crate::error::Error::NotFound);
        }
        Ok(())
    }

    /// Toggle server-side retention + configure the age threshold.
    /// `days` is clamped to [1, 3650] so the UI can't ask for 0-day
    /// retention (which would delete everything) or dates too far out
    /// to be useful.
    pub fn set_retention(&self, id: i64, enabled: bool, days: i32) -> Result<()> {
        let days = days.clamp(1, 3650);
        let conn = self.pool().get()?;
        let n = conn.execute(
            "UPDATE accounts SET retention_enabled = ?1, retention_days = ?2 WHERE id = ?3",
            rusqlite::params![i32::from(enabled), days, id],
        )?;
        if n == 0 {
            return Err(crate::error::Error::NotFound);
        }
        Ok(())
    }

    pub fn set_avatar(&self, id: i64, seed: Option<&str>, set: &str) -> Result<()> {
        // Only accept the five robohash collections we expose.
        let set = match set {
            "set1" | "set2" | "set3" | "set4" | "set5" => set,
            other => {
                return Err(crate::error::Error::BadRequest(format!(
                    "unknown avatar set: {other}"
                )))
            }
        };
        let normalized = seed.map(|s| s.trim()).filter(|s| !s.is_empty());
        let conn = self.pool().get()?;
        let n = conn.execute(
            "UPDATE accounts SET avatar_seed = ?1, avatar_set = ?2 WHERE id = ?3",
            rusqlite::params![normalized, set, id],
        )?;
        if n == 0 {
            return Err(crate::error::Error::NotFound);
        }
        Ok(())
    }

    pub fn set_auto_archive(
        &self,
        id: i64,
        enabled: bool,
        age_days: i32,
        read_only: bool,
    ) -> Result<()> {
        let age = age_days.clamp(1, 3650);
        let conn = self.pool().get()?;
        let n = conn.execute(
            "UPDATE accounts
             SET auto_archive_enabled = ?1,
                 auto_archive_age_days = ?2,
                 auto_archive_read_only = ?3
             WHERE id = ?4",
            rusqlite::params![i32::from(enabled), age, i32::from(read_only), id],
        )?;
        if n == 0 {
            return Err(crate::error::Error::NotFound);
        }
        Ok(())
    }

    pub fn delete_account(&self, id: i64) -> Result<()> {
        let conn = self.pool().get()?;
        let cred_ref: Option<String> = conn
            .query_row(
                "SELECT credential_ref FROM accounts WHERE id = ?1",
                params![id],
                |r| r.get(0),
            )
            .optional()?;
        let n = conn.execute("DELETE FROM accounts WHERE id = ?1", params![id])?;
        if n == 0 {
            return Err(crate::error::Error::NotFound);
        }
        if let Some(r) = cred_ref {
            conn.execute("DELETE FROM secrets WHERE ref = ?1", params![r])?;
        }
        Ok(())
    }

    pub fn set_delete_after_sync(&self, id: i64, enabled: bool) -> Result<()> {
        let conn = self.pool().get()?;
        let n = conn.execute(
            "UPDATE accounts SET delete_after_sync = ?1 WHERE id = ?2",
            rusqlite::params![i32::from(enabled), id],
        )?;
        if n == 0 {
            return Err(crate::error::Error::NotFound);
        }
        Ok(())
    }

    pub fn set_sync_enabled(&self, id: i64, enabled: bool) -> Result<()> {
        let conn = self.pool().get()?;
        let n = conn.execute(
            "UPDATE accounts SET sync_enabled = ?1 WHERE id = ?2",
            rusqlite::params![i32::from(enabled), id],
        )?;
        if n == 0 {
            return Err(crate::error::Error::NotFound);
        }
        Ok(())
    }

    pub fn set_send_enabled(&self, id: i64, enabled: bool) -> Result<()> {
        let conn = self.pool().get()?;
        let n = conn.execute(
            "UPDATE accounts SET send_enabled = ?1 WHERE id = ?2",
            rusqlite::params![i32::from(enabled), id],
        )?;
        if n == 0 {
            return Err(crate::error::Error::NotFound);
        }
        Ok(())
    }

    pub fn set_include_in_unified(&self, id: i64, enabled: bool) -> Result<()> {
        let conn = self.pool().get()?;
        let n = conn.execute(
            "UPDATE accounts SET include_in_unified = ?1 WHERE id = ?2",
            rusqlite::params![i32::from(enabled), id],
        )?;
        if n == 0 {
            return Err(crate::error::Error::NotFound);
        }
        Ok(())
    }

    /// Re-wrap the credential ciphertext for an account. Called by the
    /// "change app password" flow after an IMAP probe confirms the new
    /// password actually works — this keeps us from overwriting good
    /// credentials with bad ones.
    pub fn update_account_password(
        &self,
        id: i64,
        new_password: &str,
        vault: &Vault,
    ) -> Result<()> {
        let conn = self.pool().get()?;
        let cred_ref: String = conn.query_row(
            "SELECT credential_ref FROM accounts WHERE id = ?1",
            params![id],
            |r| r.get(0),
        )?;
        let wrapped = vault.encrypt(new_password.as_bytes())?;
        conn.execute(
            "UPDATE secrets SET ciphertext = ?1 WHERE ref = ?2",
            params![wrapped, cred_ref],
        )?;
        Ok(())
    }

    pub fn account_password(&self, id: i64, vault: &Vault) -> Result<String> {
        let conn = self.pool().get()?;
        let (cred_ref,): (String,) = conn.query_row(
            "SELECT credential_ref FROM accounts WHERE id = ?1",
            params![id],
            |r| Ok((r.get(0)?,)),
        )?;
        let ciphertext: Vec<u8> = conn.query_row(
            "SELECT ciphertext FROM secrets WHERE ref = ?1",
            params![cred_ref],
            |r| r.get(0),
        )?;
        let plain = vault.decrypt(&ciphertext)?;
        Ok(String::from_utf8(plain).map_err(|e| {
            crate::error::Error::Other(anyhow::anyhow!("app password not utf8: {e}"))
        })?)
    }
}

fn row_to_account(r: &rusqlite::Row) -> rusqlite::Result<Account> {
    let strategy: String = r.get(12)?;
    let imap_host: String = r.get(4)?;
    // IMPORTANT: derive kind from the IMAP host every read. The stored
    // kind column is treated as a cache at best — if it disagrees with
    // what the host tells us, the host wins. This keeps every Gmail-
    // specific code path (append skip, X-GM extensions, [Gmail]/Trash
    // MOVE) aligned with what the server can actually do, no matter
    // how the row was originally inserted.
    let kind = detect_kind_from_host(&imap_host);
    Ok(Account {
        id: r.get(0)?,
        kind,
        email: r.get(2)?,
        display_name: r.get(3)?,
        imap_host,
        imap_port: r.get::<_, i64>(5)? as u16,
        smtp_host: r.get(6)?,
        smtp_port: r.get::<_, Option<i64>>(7)?.map(|p| p as u16),
        vpn_required: r.get::<_, i64>(8)? != 0,
        delete_after_sync: r.get::<_, i64>(9)? != 0,
        created_at: r.get(10)?,
        archive_folder: r.get(11)?,
        archive_strategy: ArchiveStrategy::parse(&strategy).unwrap_or(ArchiveStrategy::Single),
        archive_enabled: r.get::<_, i64>(13)? != 0,
        auto_archive_enabled: r.get::<_, i64>(14)? != 0,
        auto_archive_age_days: r.get::<_, i64>(15)? as i32,
        auto_archive_read_only: r.get::<_, i64>(16)? != 0,
        avatar_seed: r.get(17)?,
        avatar_set: r
            .get::<_, Option<String>>(18)?
            .unwrap_or_else(|| "set1".to_owned()),
        retention_enabled: r.get::<_, i64>(19)? != 0,
        retention_days: r.get::<_, i64>(20)? as i32,
        purge_gmail_categories: r.get::<_, i64>(21)? != 0,
        skip_gmail_trash: r.get::<_, i64>(22)? != 0,
        signature_html: r.get(23)?,
        signature_plain: r.get(24)?,
        sync_enabled: r.get::<_, i64>(25)? != 0,
        send_enabled: r.get::<_, i64>(26)? != 0,
        include_in_unified: r.get::<_, i64>(27)? != 0,
        color: r.get(28)?,
    })
}
