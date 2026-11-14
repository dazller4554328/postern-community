//! Secure-notes HTTP endpoints.
//!
//! Vault-gated via the router's lock guard; handlers still call
//! `require_unlocked` for defense in depth, same pattern as the
//! reminders / calendar endpoints.

use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};

use super::AppState;
use crate::{
    error::Result,
    storage::{NewNote, Note, NoteRevision, UpdateNote},
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/notes", get(list_notes).post(create_note))
        .route(
            "/notes/:id",
            get(get_note).patch(update_note).delete(delete_note),
        )
        .route("/notes/:id/revisions", get(list_revisions))
        .route("/notes/:id/revisions/:rev_id", get(get_revision))
        .route(
            "/notes/:id/revisions/:rev_id/restore",
            post(restore_revision),
        )
}

async fn list_notes(State(s): State<AppState>) -> Result<Json<Vec<Note>>> {
    s.vault.require_unlocked()?;
    Ok(Json(s.db.list_notes()?))
}

async fn get_note(State(s): State<AppState>, Path(id): Path<i64>) -> Result<Json<Note>> {
    s.vault.require_unlocked()?;
    Ok(Json(s.db.get_note(id)?))
}

async fn create_note(State(s): State<AppState>, Json(new): Json<NewNote>) -> Result<Json<Note>> {
    s.vault.require_unlocked()?;
    Ok(Json(s.db.insert_note(&new)?))
}

async fn update_note(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(patch): Json<UpdateNote>,
) -> Result<Json<Note>> {
    s.vault.require_unlocked()?;
    Ok(Json(s.db.update_note(id, &patch)?))
}

async fn delete_note(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>> {
    s.vault.require_unlocked()?;
    s.db.delete_note(id)?;
    Ok(Json(serde_json::json!({ "deleted": id })))
}

async fn list_revisions(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Vec<NoteRevision>>> {
    s.vault.require_unlocked()?;
    Ok(Json(s.db.list_note_revisions(id)?))
}

async fn get_revision(
    State(s): State<AppState>,
    Path((id, rev_id)): Path<(i64, i64)>,
) -> Result<Json<NoteRevision>> {
    s.vault.require_unlocked()?;
    Ok(Json(s.db.get_note_revision(id, rev_id)?))
}

async fn restore_revision(
    State(s): State<AppState>,
    Path((id, rev_id)): Path<(i64, i64)>,
) -> Result<Json<Note>> {
    s.vault.require_unlocked()?;
    Ok(Json(s.db.restore_note_revision(id, rev_id)?))
}
