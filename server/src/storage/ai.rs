//! Storage layer for AI settings.
//!
//! Trimmed down 2026-05-08: Datas (RAG / Q&A), embeddings, chat
//! history, and per-call activity logging were all removed. AI is
//! now exclusively used by the compose-pane Polish button and the
//! Dictate transcription flow, so this module just persists the
//! provider config + the encrypted API key.

use rusqlite::{params, OptionalExtension};
use serde::Serialize;

use super::Db;
use crate::error::{Error, Result};

/// Persisted form of Settings → AI. Returned from `get_ai_settings`
/// and surfaced verbatim to the UI; the API key never travels in this
/// struct (only its presence does, via `api_key_set` in the HTTP DTO).
#[derive(Debug, Clone, Serialize)]
pub struct AiSettings {
    pub enabled: bool,
    pub provider_kind: String,
    pub chat_model: String,
    pub base_url: Option<String>,
    pub api_key_ref: Option<String>,
    pub cloud_consent: bool,
    /// "Always on" — when true, the AI provider holder is rebuilt
    /// automatically the first time the vault unlocks after a
    /// restart. Defaults to true so post-update experience is
    /// continuous.
    pub auto_start: bool,
    /// Optional chat-model override used by the compose-pane Polish
    /// rewrite. NULL or empty = inherit `chat_model`. Provider stays
    /// the configured chat provider — same API key, just a different
    /// model id (e.g. gpt-4o for one, gpt-4o-mini for the other).
    pub polish_chat_model: Option<String>,
    pub updated_at: i64,
}

/// Mutable shape sent to `set_ai_settings`. Mirrors `AiSettings` minus
/// the immutable / derived fields.
#[derive(Debug, Clone)]
pub struct UpdateAiSettings<'a> {
    pub enabled: bool,
    pub provider_kind: &'a str,
    pub chat_model: &'a str,
    pub base_url: Option<&'a str>,
    pub cloud_consent: bool,
    pub auto_start: bool,
    pub polish_chat_model: Option<&'a str>,
}

impl Db {
    /// Read the singleton AI settings row (id = 1). Always returns
    /// `Ok` once migrations have run — the migration seeds the row.
    pub fn get_ai_settings(&self) -> Result<AiSettings> {
        let conn = self.pool().get()?;
        let row = conn.query_row(
            "SELECT enabled, provider_kind, chat_model,
                    base_url, api_key_ref, cloud_consent, updated_at,
                    auto_start, polish_chat_model
             FROM ai_settings WHERE id = 1",
            [],
            |r| {
                Ok(AiSettings {
                    enabled: r.get::<_, i64>(0)? != 0,
                    provider_kind: r.get(1)?,
                    chat_model: r.get(2)?,
                    base_url: r.get(3)?,
                    api_key_ref: r.get(4)?,
                    cloud_consent: r.get::<_, i64>(5)? != 0,
                    updated_at: r.get(6)?,
                    auto_start: r.get::<_, i64>(7)? != 0,
                    polish_chat_model: r.get(8)?,
                })
            },
        )?;
        Ok(row)
    }

    /// Save the singleton AI settings row + (optionally) rotate the
    /// API key in the secrets table.
    ///
    /// `api_key` semantics:
    ///   * `None` — leave the existing key alone (UI sends None when
    ///     the user hasn't touched the field).
    ///   * `Some("")` — clear the key.
    ///   * `Some(value)` — encrypt with the vault and upsert.
    pub fn set_ai_settings(
        &self,
        update: &UpdateAiSettings,
        api_key: Option<&str>,
        vault: &crate::vault::Vault,
    ) -> Result<AiSettings> {
        let mut conn = self.pool().get()?;
        let tx = conn.transaction()?;

        let key_ref = upsert_ai_key(&tx, api_key, "ai:api_key", "api_key_ref", vault)?;

        let now = chrono::Utc::now().timestamp();
        tx.execute(
            "UPDATE ai_settings
                SET enabled = ?1,
                    provider_kind = ?2,
                    chat_model = ?3,
                    base_url = ?4,
                    api_key_ref = ?5,
                    cloud_consent = ?6,
                    updated_at = ?7,
                    auto_start = ?8,
                    polish_chat_model = ?9
              WHERE id = 1",
            params![
                i32::from(update.enabled),
                update.provider_kind,
                update.chat_model,
                update.base_url,
                key_ref,
                i32::from(update.cloud_consent),
                now,
                i32::from(update.auto_start),
                update.polish_chat_model,
            ],
        )?;
        tx.commit()?;
        self.get_ai_settings()
    }

    /// Decrypt the chat-side API key. None when no key has been
    /// configured yet (e.g. provider=Ollama).
    pub fn ai_api_key(&self, vault: &crate::vault::Vault) -> Result<Option<String>> {
        let settings = self.get_ai_settings()?;
        let Some(key_ref) = settings.api_key_ref else {
            return Ok(None);
        };
        let conn = self.pool().get()?;
        let cipher: Option<Vec<u8>> = conn
            .query_row(
                "SELECT ciphertext FROM secrets WHERE ref = ?1",
                params![key_ref],
                |r| r.get(0),
            )
            .optional()?;
        let Some(c) = cipher else {
            return Ok(None);
        };
        let plain = vault.decrypt(&c)?;
        Ok(Some(String::from_utf8(plain).map_err(|e| {
            Error::Other(anyhow::anyhow!("ai key not utf-8: {e}"))
        })?))
    }
}

/// Internal: rotate / clear / keep the AI key. Returns the new
/// `api_key_ref` value to write into `ai_settings`.
fn upsert_ai_key(
    tx: &rusqlite::Transaction<'_>,
    api_key: Option<&str>,
    secrets_ref: &str,
    column: &str,
    vault: &crate::vault::Vault,
) -> Result<Option<String>> {
    match api_key {
        None => {
            let sql = format!("SELECT {column} FROM ai_settings WHERE id = 1");
            let existing: Option<String> =
                tx.query_row(&sql, [], |r| r.get(0)).optional()?.flatten();
            Ok(existing)
        }
        Some("") => {
            let sql = format!("SELECT {column} FROM ai_settings WHERE id = 1");
            let prior: Option<String> = tx.query_row(&sql, [], |r| r.get(0)).optional()?.flatten();
            if let Some(r) = prior {
                tx.execute("DELETE FROM secrets WHERE ref = ?1", params![r])?;
            }
            Ok(None)
        }
        Some(plaintext) => {
            let r = secrets_ref.to_owned();
            let wrapped = vault.encrypt(plaintext.as_bytes())?;
            tx.execute(
                "INSERT INTO secrets(ref, ciphertext) VALUES (?1, ?2)
                 ON CONFLICT(ref) DO UPDATE SET ciphertext = excluded.ciphertext",
                params![r, wrapped],
            )?;
            Ok(Some(r))
        }
    }
}
