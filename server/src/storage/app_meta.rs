//! Singleton app_meta row — install ID, license key + verification
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
        let conn = self.pool().get()?;
        conn.execute(
            "INSERT INTO app_meta (id, install_id) VALUES (1, ?1)
             ON CONFLICT(id) DO NOTHING",
            params![install_id],
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

    /// Read the lockdown flag. Cheap — single integer column on the
    /// singleton row. The check fires only on mutating endpoints,
    /// which are bounded by the user's hand on the keyboard so a
    /// per-call DB read is fine.
    pub fn lockdown_enabled(&self) -> Result<bool> {
        let conn = self.pool().get()?;
        let v: i64 = conn.query_row(
            "SELECT lockdown_enabled FROM app_meta WHERE id = 1",
            [],
            |r| r.get(0),
        )?;
        Ok(v != 0)
    }

    /// Toggle the lockdown flag. Returns the new state.
    pub fn set_lockdown_enabled(&self, on: bool) -> Result<bool> {
        let conn = self.pool().get()?;
        conn.execute(
            "UPDATE app_meta
                SET lockdown_enabled = ?1,
                    updated_at = strftime('%s','now')
              WHERE id = 1",
            params![i32::from(on)],
        )?;
        Ok(on)
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
