//! Singleton `app_meta` row — install ID, license key + verification
//! state, and the most recent update-check response.
//!
//! The install ID is a v4 UUID generated on first boot and never
//! changes. We send it to the update server with every `/check` so
//! licenses can eventually bind to a specific install without
//! forcing the user to re-enter a key if the VPS address rotates.

use rusqlite::{params, OptionalExtension};
use serde::Serialize;

use super::Db;
use crate::error::{Error, Result};

#[derive(Debug, Clone, Serialize)]
pub struct AppMeta {
    pub install_id: String,
    pub license_key: Option<String>,
    pub license_status: String,
    pub license_tier: Option<String>,
    pub license_verified_at_utc: Option<i64>,
    pub last_update_check_at_utc: Option<i64>,
    pub last_update_check_json: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl Db {
    /// Returns the singleton row, creating it with a fresh UUID the
    /// first time it's ever called.
    pub fn app_meta_get_or_init(&self) -> Result<AppMeta> {
        let conn = self.pool().get()?;
        let existing: Option<AppMeta> = conn
            .query_row(
                "SELECT install_id, license_key, license_status, license_tier,
                        license_verified_at_utc, last_update_check_at_utc,
                        last_update_check_json, created_at, updated_at
                   FROM app_meta WHERE id = 1",
                [],
                row_to_app_meta,
            )
            .optional()?;
        if let Some(m) = existing {
            return Ok(m);
        }
        drop(conn);

        let install_id = uuid::Uuid::new_v4().to_string();
        // First-boot license seeding: the bootstrap installer drops the
        // user's license key into POSTERN_LICENSE_KEY in /opt/postern/.env
        // (which docker-compose loads via env_file). Picking it up here
        // saves the user from re-pasting it in Settings → Updates.
        // Only fires on initial INSERT — we never overwrite a row the
        // user has since cleared or rotated through the UI.
        let seeded_license: Option<String> = None;
        let conn = self.pool().get()?;
        conn.execute(
            "INSERT INTO app_meta (id, install_id, license_key) VALUES (1, ?1, ?2)
             ON CONFLICT(id) DO NOTHING",
            params![install_id, seeded_license],
        )?;
        conn.query_row(
            "SELECT install_id, license_key, license_status, license_tier,
                    license_verified_at_utc, last_update_check_at_utc,
                    last_update_check_json, created_at, updated_at
               FROM app_meta WHERE id = 1",
            [],
            row_to_app_meta,
        )
        .map_err(Error::from)
    }

    pub fn app_meta_set_license(&self, key: Option<&str>) -> Result<AppMeta> {
        // Any change of key invalidates the cached verification state —
        // the caller will normally follow up with a /verify call.
        let conn = self.pool().get()?;
        conn.execute(
            "UPDATE app_meta
                SET license_key = ?1,
                    license_status = 'unknown',
                    license_tier = NULL,
                    license_verified_at_utc = NULL,
                    updated_at = strftime('%s','now')
              WHERE id = 1",
            params![key.map(str::trim).filter(|s| !s.is_empty())],
        )?;
        drop(conn);
        self.app_meta_get_or_init()
    }

    pub fn app_meta_record_license_check(
        &self,
        status: &str,
        tier: Option<&str>,
    ) -> Result<AppMeta> {
        let conn = self.pool().get()?;
        conn.execute(
            "UPDATE app_meta
                SET license_status = ?1,
                    license_tier = ?2,
                    license_verified_at_utc = strftime('%s','now'),
                    updated_at = strftime('%s','now')
              WHERE id = 1",
            params![status, tier],
        )?;
        drop(conn);
        self.app_meta_get_or_init()
    }

    /// Whether the user has opted into remote sender-avatar lookups
    /// (Libravatar/Gravatar, sender-domain icons). Off by default — see
    /// migration 0063. Kept as a standalone scalar read rather than a
    /// field on `AppMeta` so the hot avatar path doesn't pull the whole
    /// row (and so the four `AppMeta` SELECTs stay untouched).
    pub fn remote_avatars_enabled(&self) -> Result<bool> {
        // Guarantee the singleton row exists before reading the column.
        self.app_meta_get_or_init()?;
        let conn = self.pool().get()?;
        let v: i64 = conn.query_row(
            "SELECT remote_avatars_enabled FROM app_meta WHERE id = 1",
            [],
            |r| r.get(0),
        )?;
        Ok(v != 0)
    }

    pub fn set_remote_avatars_enabled(&self, enabled: bool) -> Result<()> {
        self.app_meta_get_or_init()?;
        let conn = self.pool().get()?;
        conn.execute(
            "UPDATE app_meta
                SET remote_avatars_enabled = ?1,
                    updated_at = strftime('%s','now')
              WHERE id = 1",
            params![i64::from(enabled)],
        )?;
        Ok(())
    }

    pub fn app_meta_record_update_check(&self, json: &str) -> Result<AppMeta> {
        let conn = self.pool().get()?;
        conn.execute(
            "UPDATE app_meta
                SET last_update_check_at_utc = strftime('%s','now'),
                    last_update_check_json = ?1,
                    updated_at = strftime('%s','now')
              WHERE id = 1",
            params![json],
        )?;
        drop(conn);
        self.app_meta_get_or_init()
    }
}


fn row_to_app_meta(r: &rusqlite::Row) -> rusqlite::Result<AppMeta> {
    Ok(AppMeta {
        install_id: r.get(0)?,
        license_key: r.get(1)?,
        license_status: r.get(2)?,
        license_tier: r.get(3)?,
        license_verified_at_utc: r.get(4)?,
        last_update_check_at_utc: r.get(5)?,
        last_update_check_json: r.get(6)?,
        created_at: r.get(7)?,
        updated_at: r.get(8)?,
    })
}

