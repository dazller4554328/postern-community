//! Trusted-senders allowlist. Per-account list of email addresses that
//! should never be filed as spam.
//!
//! Populated automatically when the user clicks "Not spam" on a message
//! in the inbox UI; editable from Settings → Trusted senders. The IMAP
//! sync hook checks this table before honouring a server-side spam
//! verdict and auto-moves matching mail back to INBOX.

use rusqlite::{params, OptionalExtension};
use serde::Serialize;

use super::Db;
use crate::error::Result;

#[derive(Debug, Clone, Serialize)]
pub struct TrustedSender {
    pub id: i64,
    pub account_id: i64,
    pub email_lower: String,
    pub created_at: i64,
}

/// Pull the bare address out of a `From:`-style header value. Accepts:
///   - "alice@example.com"
///   - "Alice <alice@example.com>"
///   - "  Alice <ALICE@example.com>  " (trims + lowercases)
/// Returns None when no `@` is present after parsing, so callers can
/// treat the input as "no sender to remember" and skip the insert.
pub fn normalize_address(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let inner = match (trimmed.rfind('<'), trimmed.rfind('>')) {
        (Some(open), Some(close)) if close > open + 1 => trimmed[open + 1..close].trim(),
        _ => trimmed,
    };
    if !inner.contains('@') {
        return None;
    }
    Some(inner.to_ascii_lowercase())
}

impl Db {
    /// Insert (or no-op-on-conflict) a trusted-sender row. Returns true
    /// on a fresh insert, false when the address was already trusted —
    /// lets the HTTP handler tell the user "added" vs "already on the
    /// list" without a separate lookup.
    pub fn add_trusted_sender(&self, account_id: i64, email: &str) -> Result<bool> {
        let Some(addr) = normalize_address(email) else {
            return Err(crate::error::Error::BadRequest(
                "address must contain @".into(),
            ));
        };
        let conn = self.pool().get()?;
        let n = conn.execute(
            "INSERT OR IGNORE INTO trusted_senders(account_id, email_lower)
             VALUES (?1, ?2)",
            params![account_id, addr],
        )?;
        Ok(n > 0)
    }

    /// All trusted addresses across all accounts, paired with their
    /// account id. Used by the settings page to show one combined
    /// list with an account label per row.
    pub fn list_all_trusted_senders(&self) -> Result<Vec<TrustedSender>> {
        let conn = self.pool().get()?;
        let mut stmt = conn.prepare(
            "SELECT id, account_id, email_lower, created_at
             FROM trusted_senders
             ORDER BY account_id ASC, email_lower ASC",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(TrustedSender {
                    id: r.get(0)?,
                    account_id: r.get(1)?,
                    email_lower: r.get(2)?,
                    created_at: r.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn delete_trusted_sender(&self, id: i64) -> Result<bool> {
        let conn = self.pool().get()?;
        let n = conn.execute("DELETE FROM trusted_senders WHERE id = ?1", params![id])?;
        Ok(n > 0)
    }

    /// Membership check — used during IMAP sync to decide whether a
    /// spam-folder arrival should be auto-rescued. Empty input or a
    /// missing `@` short-circuits to false so the caller doesn't need
    /// to special-case those inputs.
    pub fn is_trusted_sender(&self, account_id: i64, email: &str) -> Result<bool> {
        let Some(addr) = normalize_address(email) else {
            return Ok(false);
        };
        let conn = self.pool().get()?;
        let hit: Option<i64> = conn
            .query_row(
                "SELECT 1 FROM trusted_senders
                 WHERE account_id = ?1 AND email_lower = ?2",
                params![account_id, addr],
                |r| r.get(0),
            )
            .optional()?;
        Ok(hit.is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_handles_bare_address() {
        assert_eq!(
            normalize_address("alice@example.com").as_deref(),
            Some("alice@example.com")
        );
    }

    #[test]
    fn normalize_extracts_from_display_name_form() {
        assert_eq!(
            normalize_address("Alice <Alice@Example.com>").as_deref(),
            Some("alice@example.com")
        );
    }

    #[test]
    fn normalize_lowercases_and_trims() {
        assert_eq!(
            normalize_address("  BOB@EXAMPLE.COM  ").as_deref(),
            Some("bob@example.com")
        );
    }

    #[test]
    fn normalize_rejects_garbage() {
        assert_eq!(normalize_address(""), None);
        assert_eq!(normalize_address("no-at-sign"), None);
        assert_eq!(normalize_address("Bare Name <>"), None);
    }
}
