use axum::{
    extract::{Query, State},
    routing::get,
    Json, Router,
};
use serde::Deserialize;

use super::AppState;
use crate::{error::Result, storage::AuditEntry};

pub fn routes() -> Router<AppState> {
    Router::new().route("/audit", get(list))
}

#[derive(Debug, Deserialize)]
struct AuditQuery {
    #[serde(default = "default_limit")]
    limit: i64,
    #[serde(default)]
    offset: i64,
    /// Optional filter: "security" or "activity". Omitted = all categories.
    #[serde(default)]
    category: Option<String>,
}

const fn default_limit() -> i64 {
    100
}

async fn list(
    State(s): State<AppState>,
    Query(q): Query<AuditQuery>,
) -> Result<Json<Vec<AuditEntry>>> {
    let limit = q.limit.clamp(1, 500);
    let cat = q
        .category
        .as_deref()
        .filter(|c| matches!(*c, "security" | "activity"));
    Ok(Json(s.db.list_audit(cat, limit, q.offset)?))
}
