//! `GET /api/curated/messages` — sender-engagement-ranked list.
//!
//! Phase 1 of the Curated view: a single endpoint that returns
//! the same `MessageListItem` shape the inbox uses, plus a
//! `curated_score` field for debugging. The frontend renders these
//! with the standard message-row component, so the new view is
//! basically "inbox sorted differently".

use axum::{
    extract::{Query, State},
    routing::get,
    Json, Router,
};
use serde::Deserialize;

use super::AppState;
use crate::{error::Result, storage::CuratedListItem};

pub fn routes() -> Router<AppState> {
    Router::new().route("/curated/messages", get(list_curated))
}

/// Pagination + scope for the curated list. Keep it stable with
/// `/api/messages` so the frontend can swap one URL for the other.
#[derive(Debug, Deserialize)]
struct ListQuery {
    /// Optional account filter — when omitted, returns the unified
    /// view (every account that has `include_in_unified = 1`).
    account_id: Option<i64>,
    #[serde(default = "default_limit")]
    limit: i64,
    #[serde(default)]
    offset: i64,
}

const fn default_limit() -> i64 {
    50
}

async fn list_curated(
    State(s): State<AppState>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<CuratedListItem>>> {
    s.vault.require_unlocked()?;
    // Clamp paging — keep DoS surface small. Same caps the regular
    // message list uses; centralised constants would be nicer but
    // not worth a refactor for this one endpoint.
    let limit = q.limit.clamp(1, 200);
    let offset = q.offset.max(0);
    let rows = s.db.list_curated(q.account_id, limit, offset)?;
    Ok(Json(rows))
}
