//! Trusted-senders REST surface. Backs Settings → Trusted senders and
//! is the persistence layer for the inbox "Not spam" action.
//!
//!   GET    /api/trusted-senders               — list all (across accounts)
//!   POST   /api/trusted-senders               — { account_id, email }
//!   DELETE /api/trusted-senders/:id           — remove one entry

use axum::{
    extract::{Path, State},
    routing::get,
    Json, Router,
};
use serde::Deserialize;

use super::AppState;
use crate::{error::Result, storage::TrustedSender};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/trusted-senders", get(list).post(create))
        .route("/trusted-senders/:id", axum::routing::delete(delete))
}

async fn list(State(s): State<AppState>) -> Result<Json<Vec<TrustedSender>>> {
    s.vault.require_unlocked()?;
    Ok(Json(s.db.list_all_trusted_senders()?))
}

#[derive(Debug, Deserialize)]
struct CreateBody {
    account_id: i64,
    email: String,
}

async fn create(
    State(s): State<AppState>,
    Json(body): Json<CreateBody>,
) -> Result<Json<serde_json::Value>> {
    s.vault.require_unlocked()?;
    // Validate the account exists — without this we'd silently insert
    // a row referencing a deleted account that ON DELETE CASCADE would
    // never see (FK gets enforced lazily on SQLite without
    // PRAGMA foreign_keys=ON, which our pool doesn't currently set).
    let _ = s.db.get_account(body.account_id)?;
    let inserted = s.db.add_trusted_sender(body.account_id, &body.email)?;
    Ok(Json(serde_json::json!({
        "account_id": body.account_id,
        "email": body.email,
        "inserted": inserted,
    })))
}

async fn delete(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>> {
    s.vault.require_unlocked()?;
    let removed = s.db.delete_trusted_sender(id)?;
    Ok(Json(serde_json::json!({
        "id": id,
        "removed": removed,
    })))
}
