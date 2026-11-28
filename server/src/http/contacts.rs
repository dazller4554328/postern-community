//! Contacts CRUD + autocomplete handlers.
//!
//! Lives behind the same `/api/messages/...` and `/api/contacts/...`
//! routes as before — split out from `http/messages.rs` so the
//! messages file stays focused on message lifecycle. The autocomplete
//! handler still scans the messages corpus directly (the contacts
//! table proved racy for that hot path on real mailboxes); it lives
//! here to keep all contact-related surfaces in one file.

use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    routing::{get, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use super::AppState;
use crate::error::Result;

/// Cap on uploaded photo size — keeps a row-stored blob from
/// blowing up the DB and stops a malicious client from filling
/// the disk with one PUT. 2 MB is generous for a contact avatar
/// (a 512×512 JPEG is typically <100 KB).
const MAX_PHOTO_BYTES: usize = 2 * 1024 * 1024;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/contacts/autocomplete", get(autocomplete))
        .route("/contacts", get(list_contacts).post(create_contact))
        .route(
            "/contacts/:id",
            get(get_contact)
                .patch(update_contact)
                .delete(delete_contact),
        )
        .route(
            "/contacts/:id/photo",
            put(set_contact_photo).delete(clear_contact_photo),
        )
}

#[derive(Debug, Deserialize)]
struct AutocompleteQuery {
    q: String,
    #[serde(default = "default_autocomplete_limit")]
    limit: i64,
}

const fn default_autocomplete_limit() -> i64 {
    10
}

async fn autocomplete(
    State(s): State<AppState>,
    Query(q): Query<AutocompleteQuery>,
) -> Result<Json<Vec<String>>> {
    if q.q.len() < 2 {
        return Ok(Json(vec![]));
    }
    // Restored to the original messages-corpus scan that's been
    // proven on real mailboxes since Sprint 1 (commit 707f033). The
    // contacts-table backed version was meant to be a faster
    // replacement once the table was populated, but the populate
    // path turned out to be racy with the upsert hook on real
    // deployments — the table ended up partial, autocomplete
    // returned nothing, and there was no good recovery short of a
    // manual rebuild. Direct messages-scan: always returns whatever
    // your indexed mail actually contains, no backfill dance, no
    // staleness window. The contacts table + UI still exists for
    // the address-book surface; it's just not on the autocomplete
    // hot path any more.
    let conn = s.db.pool().get()?;
    let pattern = format!("%{}%", q.q.replace('%', "\\%"));
    let limit = q.limit.clamp(1, 50);
    let mut stmt = conn.prepare(
        "SELECT DISTINCT from_addr FROM messages
         WHERE from_addr LIKE ?1 ESCAPE '\\'
         ORDER BY date_utc DESC
         LIMIT ?2",
    )?;
    let rows: Vec<String> = stmt
        .query_map(rusqlite::params![pattern, limit], |r| r.get(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    // Also search to_addrs (recipients we've sent to before).
    let mut stmt2 = conn.prepare(
        "SELECT DISTINCT to_addrs FROM messages
         WHERE to_addrs LIKE ?1 ESCAPE '\\'
         ORDER BY date_utc DESC
         LIMIT ?2",
    )?;
    let to_rows: Vec<String> = stmt2
        .query_map(rusqlite::params![pattern, limit], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut all: Vec<String> = rows;
    let q_lower = q.q.to_lowercase();
    for t in to_rows {
        for addr in t.split(',') {
            let a = addr.trim().to_string();
            if !a.is_empty() && a.to_lowercase().contains(&q_lower) && !all.contains(&a) {
                all.push(a);
            }
        }
    }
    all.truncate(limit as usize);
    Ok(Json(all))
}

#[derive(Debug, Deserialize)]
struct ContactsListQuery {
    #[serde(default)]
    q: Option<String>,
    #[serde(default = "default_contacts_limit")]
    limit: i64,
    #[serde(default)]
    offset: i64,
}

const fn default_contacts_limit() -> i64 {
    50
}

#[derive(Serialize)]
struct ContactsListReply {
    contacts: Vec<crate::storage::Contact>,
    total: i64,
}

/// GET /api/contacts?q=&limit=&offset= — paginated address-book
/// listing. Drives the Settings → Contacts page; usable via curl
/// too for inspection / debugging.
async fn list_contacts(
    State(s): State<AppState>,
    Query(p): Query<ContactsListQuery>,
) -> Result<Json<ContactsListReply>> {
    s.vault.require_unlocked()?;
    let q = p.q.as_deref();
    let contacts = s.db.list_contacts(q, p.limit, p.offset)?;
    let total = s.db.count_contacts(q)?;
    Ok(Json(ContactsListReply { contacts, total }))
}

#[derive(Debug, Deserialize)]
struct CreateContactBody {
    address: String,
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    notes: Option<String>,
    #[serde(default)]
    is_favorite: bool,
}

/// POST /api/contacts — manual add. Returns 409 (via `BadRequest`)
/// when the address already exists.
async fn create_contact(
    State(s): State<AppState>,
    Json(b): Json<CreateContactBody>,
) -> Result<(StatusCode, Json<crate::storage::Contact>)> {
    s.vault.require_unlocked()?;
    let id = s.db.create_contact(
        &b.address,
        b.display_name.as_deref(),
        b.notes.as_deref(),
        b.is_favorite,
    )?;
    let contact = s.db.get_contact(id)?.ok_or_else(|| {
        crate::error::Error::Other(anyhow::anyhow!("contact disappeared after insert"))
    })?;
    Ok((StatusCode::CREATED, Json(contact)))
}

async fn get_contact(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<crate::storage::Contact>> {
    s.vault.require_unlocked()?;
    let contact =
        s.db.get_contact(id)?
            .ok_or_else(|| crate::error::Error::NotFound)?;
    Ok(Json(contact))
}

#[derive(Debug, Deserialize)]
struct UpdateContactBody {
    /// `null` clears the display name; absent = leave alone; non-empty
    /// = replace. Two-tier `Option<Option<String>>` is what serde
    /// uses to distinguish missing-vs-null in JSON; we forward that
    /// directly to the storage layer.
    #[serde(default, deserialize_with = "deserialize_opt_opt_string")]
    display_name: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_opt_opt_string")]
    notes: Option<Option<String>>,
    #[serde(default)]
    is_favorite: Option<bool>,
}

/// Helper for the missing-vs-null distinction. Serde treats a missing
/// JSON field as None and an explicit null as Some(None) when we use
/// this custom deserializer; without it, both collapse to None.
fn deserialize_opt_opt_string<'de, D>(
    deserializer: D,
) -> std::result::Result<Option<Option<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let v = Option::<String>::deserialize(deserializer)?;
    Ok(Some(v))
}

async fn update_contact(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(b): Json<UpdateContactBody>,
) -> Result<Json<crate::storage::Contact>> {
    s.vault.require_unlocked()?;
    let updated =
        s.db.update_contact(id, b.display_name, b.notes, b.is_favorite)?;
    if !updated {
        return Err(crate::error::Error::NotFound);
    }
    let contact =
        s.db.get_contact(id)?
            .ok_or_else(|| crate::error::Error::NotFound)?;
    Ok(Json(contact))
}

async fn delete_contact(State(s): State<AppState>, Path(id): Path<i64>) -> Result<StatusCode> {
    s.vault.require_unlocked()?;
    let deleted = s.db.delete_contact(id)?;
    if !deleted {
        return Err(crate::error::Error::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

/// PUT /api/contacts/:id/photo — set or replace a contact's avatar.
/// Body is the raw image bytes; `Content-Type` MUST be image/*.
/// Returns 204 on success, 400 on bad MIME or oversize upload,
/// 404 if the contact id doesn't exist.
async fn set_contact_photo(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<StatusCode> {
    s.vault.require_unlocked()?;
    let mime = headers
        .get(header::CONTENT_TYPE)
        .and_then(|h| h.to_str().ok())
        .ok_or_else(|| crate::error::Error::BadRequest("Content-Type header is required".into()))?
        .to_owned();
    if !mime.starts_with("image/") {
        return Err(crate::error::Error::BadRequest(format!(
            "Content-Type must be image/*, got {mime}"
        )));
    }
    if body.is_empty() {
        return Err(crate::error::Error::BadRequest(
            "photo body is empty".into(),
        ));
    }
    if body.len() > MAX_PHOTO_BYTES {
        return Err(crate::error::Error::BadRequest(format!(
            "photo exceeds {MAX_PHOTO_BYTES} bytes"
        )));
    }
    let updated = s.db.set_contact_photo(id, &body, &mime)?;
    if !updated {
        return Err(crate::error::Error::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

/// DELETE /api/contacts/:id/photo — clear a contact's stored photo
/// so the avatar endpoint falls through to remote sources again.
async fn clear_contact_photo(State(s): State<AppState>, Path(id): Path<i64>) -> Result<StatusCode> {
    s.vault.require_unlocked()?;
    let cleared = s.db.clear_contact_photo(id)?;
    if !cleared {
        return Err(crate::error::Error::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}
