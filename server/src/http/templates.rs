//! Email-templates REST surface. Backs Settings → Templates and the
//! "Template" picker on the compose form.
//!
//!   GET    /api/templates        — list all, sorted by name
//!   POST   /api/templates        — { name, subject?, body? }
//!   GET    /api/templates/:id
//!   PATCH  /api/templates/:id    — any subset of { name, subject, body }
//!   DELETE /api/templates/:id

use axum::{
    extract::{Path, State},
    routing::get,
    Json, Router,
};

use super::AppState;
use crate::{
    error::Result,
    storage::{EmailTemplate, NewTemplate, UpdateTemplate},
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/templates", get(list).post(create))
        .route(
            "/templates/:id",
            get(get_one).patch(update).delete(delete),
        )
}

async fn list(State(s): State<AppState>) -> Result<Json<Vec<EmailTemplate>>> {
    s.vault.require_unlocked()?;
    Ok(Json(s.db.list_templates()?))
}

async fn get_one(State(s): State<AppState>, Path(id): Path<i64>) -> Result<Json<EmailTemplate>> {
    s.vault.require_unlocked()?;
    Ok(Json(s.db.get_template(id)?))
}

async fn create(
    State(s): State<AppState>,
    Json(new): Json<NewTemplate>,
) -> Result<Json<EmailTemplate>> {
    s.vault.require_unlocked()?;
    Ok(Json(s.db.insert_template(&new)?))
}

async fn update(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(patch): Json<UpdateTemplate>,
) -> Result<Json<EmailTemplate>> {
    s.vault.require_unlocked()?;
    Ok(Json(s.db.update_template(id, &patch)?))
}

async fn delete(State(s): State<AppState>, Path(id): Path<i64>) -> Result<Json<serde_json::Value>> {
    s.vault.require_unlocked()?;
    s.db.delete_template(id)?;
    Ok(Json(serde_json::json!({ "deleted": id })))
}
