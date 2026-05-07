//! Inbound message rule engine.
//!
//! Rules are evaluated against every newly-inserted message during IMAP
//! sync. Each rule has one condition and one action. Multiple rules can
//! fire on the same message (priority order, all matching rules apply).
//!
//! Condition fields: from, to, cc, subject, any_header
//! Condition operators: contains, not_contains, equals, starts_with, ends_with
//! Action types: move_to, label, mark_read, mark_starred, trash, spam

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::{error::Result, storage::Db};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    pub id: i64,
    pub account_id: Option<i64>,
    pub name: String,
    pub enabled: bool,
    pub priority: i32,
    pub condition_field: String,
    pub condition_op: String,
    pub condition_value: String,
    pub action_type: String,
    pub action_value: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NewRule {
    pub account_id: Option<i64>,
    pub name: String,
    pub condition_field: String,
    pub condition_op: String,
    pub condition_value: String,
    pub action_type: String,
    pub action_value: String,
    #[serde(default)]
    pub priority: i32,
}

impl Db {
    pub fn list_rules(&self) -> Result<Vec<Rule>> {
        let conn = self.pool().get()?;
        let mut stmt = conn.prepare(
            "SELECT id, account_id, name, enabled, priority,
                    condition_field, condition_op, condition_value,
                    action_type, action_value, created_at, updated_at
             FROM rules ORDER BY priority DESC, id ASC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(Rule {
                id: r.get(0)?,
                account_id: r.get(1)?,
                name: r.get(2)?,
                enabled: r.get::<_, i64>(3)? != 0,
                priority: r.get::<_, i64>(4)? as i32,
                condition_field: r.get(5)?,
                condition_op: r.get(6)?,
                condition_value: r.get(7)?,
                action_type: r.get(8)?,
                action_value: r.get(9)?,
                created_at: r.get(10)?,
                updated_at: r.get(11)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn create_rule(&self, r: &NewRule) -> Result<Rule> {
        let conn = self.pool().get()?;
        let now = chrono::Utc::now().timestamp();
        conn.execute(
            "INSERT INTO rules (account_id, name, enabled, priority,
                condition_field, condition_op, condition_value,
                action_type, action_value, created_at, updated_at)
             VALUES (?1, ?2, 1, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                r.account_id,
                r.name,
                r.priority as i64,
                r.condition_field,
                r.condition_op,
                r.condition_value,
                r.action_type,
                r.action_value,
                now,
                now,
            ],
        )?;
        let id = conn.last_insert_rowid();
        self.get_rule(id)
    }

    pub fn get_rule(&self, id: i64) -> Result<Rule> {
        let conn = self.pool().get()?;
        conn.query_row(
            "SELECT id, account_id, name, enabled, priority,
                    condition_field, condition_op, condition_value,
                    action_type, action_value, created_at, updated_at
             FROM rules WHERE id = ?1",
            params![id],
            |r| {
                Ok(Rule {
                    id: r.get(0)?,
                    account_id: r.get(1)?,
                    name: r.get(2)?,
                    enabled: r.get::<_, i64>(3)? != 0,
                    priority: r.get::<_, i64>(4)? as i32,
                    condition_field: r.get(5)?,
                    condition_op: r.get(6)?,
                    condition_value: r.get(7)?,
                    action_type: r.get(8)?,
                    action_value: r.get(9)?,
                    created_at: r.get(10)?,
                    updated_at: r.get(11)?,
                })
            },
        )
        .optional()?
        .ok_or(crate::error::Error::NotFound)
    }

    pub fn delete_rule(&self, id: i64) -> Result<()> {
        let conn = self.pool().get()?;
        let n = conn.execute("DELETE FROM rules WHERE id = ?1", params![id])?;
        if n == 0 {
            return Err(crate::error::Error::NotFound);
        }
        Ok(())
    }

    pub fn toggle_rule(&self, id: i64, enabled: bool) -> Result<Rule> {
        let conn = self.pool().get()?;
        conn.execute(
            "UPDATE rules SET enabled = ?1, updated_at = ?2 WHERE id = ?3",
            params![i32::from(enabled), chrono::Utc::now().timestamp(), id],
        )?;
        self.get_rule(id)
    }

    pub fn rules_for_account(&self, account_id: i64) -> Result<Vec<Rule>> {
        let conn = self.pool().get()?;
        let mut stmt = conn.prepare(
            "SELECT id, account_id, name, enabled, priority,
                    condition_field, condition_op, condition_value,
                    action_type, action_value, created_at, updated_at
             FROM rules
             WHERE enabled = 1 AND (account_id IS NULL OR account_id = ?1)
             ORDER BY priority DESC, id ASC",
        )?;
        let rows = stmt.query_map(params![account_id], |r| {
            Ok(Rule {
                id: r.get(0)?,
                account_id: r.get(1)?,
                name: r.get(2)?,
                enabled: r.get::<_, i64>(3)? != 0,
                priority: r.get::<_, i64>(4)? as i32,
                condition_field: r.get(5)?,
                condition_op: r.get(6)?,
                condition_value: r.get(7)?,
                action_type: r.get(8)?,
                action_value: r.get(9)?,
                created_at: r.get(10)?,
                updated_at: r.get(11)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }
}

/// Test a single rule condition against message fields.
pub fn matches_rule(rule: &Rule, from: &str, to: &str, cc: &str, subject: &str) -> bool {
    let field_value = match rule.condition_field.as_str() {
        "from" => from,
        "to" => to,
        "cc" => cc,
        "subject" => subject,
        "any" => {
            // Match against all fields.
            return check_op(&rule.condition_op, from, &rule.condition_value)
                || check_op(&rule.condition_op, to, &rule.condition_value)
                || check_op(&rule.condition_op, cc, &rule.condition_value)
                || check_op(&rule.condition_op, subject, &rule.condition_value);
        }
        _ => return false,
    };
    check_op(&rule.condition_op, field_value, &rule.condition_value)
}

fn check_op(op: &str, haystack: &str, needle: &str) -> bool {
    let h = haystack.to_ascii_lowercase();
    let n = needle.to_ascii_lowercase();
    match op {
        "contains" => h.contains(&n),
        "not_contains" => !h.contains(&n),
        "equals" => h == n,
        "starts_with" => h.starts_with(&n),
        "ends_with" => h.ends_with(&n),
        _ => false,
    }
}

/// Apply matching rules to a newly-synced message. Returns the list
/// of actions taken (for logging).
pub fn apply_rules(
    db: &Db,
    account_id: i64,
    message_id: i64,
    from: &str,
    to: &str,
    cc: &str,
    subject: &str,
) -> Vec<String> {
    let rules = match db.rules_for_account(account_id) {
        Ok(r) => r,
        Err(_) => return vec![],
    };

    let is_gmail = db
        .get_account(account_id)
        .map(|a| a.kind == crate::storage::AccountKind::Gmail)
        .unwrap_or(true);

    let mut actions = Vec::new();
    for rule in &rules {
        if !matches_rule(rule, from, to, cc, subject) {
            continue;
        }

        let action_desc = format!("{}:{}", rule.action_type, rule.action_value);
        match rule.action_type.as_str() {
            "move_to" | "label" => {
                let labels = vec![rule.action_value.as_str()];
                if db.relabel_message(message_id, account_id, &labels).is_ok() {
                    actions.push(action_desc);
                }
            }
            "mark_read" => {
                let _ = db.set_message_read(message_id, true);
                actions.push(action_desc);
            }
            "trash" => {
                let trash = if is_gmail { "[Gmail]/Trash" } else { "Trash" };
                let _ = db.relabel_message(message_id, account_id, &[trash]);
                actions.push(action_desc);
            }
            "spam" => {
                let spam = if is_gmail { "[Gmail]/Spam" } else { "Spam" };
                let _ = db.relabel_message(message_id, account_id, &[spam]);
                actions.push(action_desc);
            }
            _ => {}
        }
    }

    if !actions.is_empty() {
        info!(message_id, ?actions, "rules applied");
    }
    actions
}

/// Retroactively apply all enabled rules to existing messages.
/// Returns (checked, acted) counts.
pub fn apply_rules_retroactive(db: &Db) -> (usize, usize) {
    let accounts = match db.list_accounts() {
        Ok(a) => a,
        Err(_) => return (0, 0),
    };

    let mut checked = 0usize;
    let mut acted = 0usize;

    for account in &accounts {
        let rules = match db.rules_for_account(account.id) {
            Ok(r) if !r.is_empty() => r,
            _ => continue,
        };

        let conn = match db.pool().get() {
            Ok(c) => c,
            Err(_) => continue,
        };

        let mut stmt = match conn.prepare(
            "SELECT id, from_addr, to_addrs, cc_addrs, subject
             FROM messages WHERE account_id = ?1",
        ) {
            Ok(s) => s,
            Err(_) => continue,
        };

        let rows: Vec<(i64, String, String, String, String)> = stmt
            .query_map(rusqlite::params![account.id], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                    r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                    r.get::<_, Option<String>>(3)?.unwrap_or_default(),
                    r.get::<_, Option<String>>(4)?.unwrap_or_default(),
                ))
            })
            .ok()
            .map(|r| r.filter_map(|x| x.ok()).collect())
            .unwrap_or_default();
        drop(stmt);
        drop(conn);

        for (msg_id, from, to, cc, subject) in &rows {
            checked += 1;
            for rule in &rules {
                if !matches_rule(rule, from, to, cc, subject) {
                    continue;
                }
                let is_gmail = account.kind == crate::storage::AccountKind::Gmail;
                match rule.action_type.as_str() {
                    "move_to" | "label" => {
                        let _ =
                            db.relabel_message(*msg_id, account.id, &[rule.action_value.as_str()]);
                        acted += 1;
                    }
                    "mark_read" => {
                        let _ = db.set_message_read(*msg_id, true);
                        acted += 1;
                    }
                    "trash" => {
                        let t = if is_gmail { "[Gmail]/Trash" } else { "Trash" };
                        let _ = db.relabel_message(*msg_id, account.id, &[t]);
                        acted += 1;
                    }
                    "spam" => {
                        let s = if is_gmail { "[Gmail]/Spam" } else { "Spam" };
                        let _ = db.relabel_message(*msg_id, account.id, &[s]);
                        acted += 1;
                    }
                    _ => {}
                }
                break; // first matching rule wins for retroactive
            }
        }
    }

    info!(checked, acted, "retroactive rules applied");
    (checked, acted)
}
