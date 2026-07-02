use axum::{
    extract::{Query, State},
    routing::get,
    Json, Router,
};
use serde::Deserialize;

use super::AppState;
use crate::{error::Result, storage::SearchHit};

pub fn routes() -> Router<AppState> {
    Router::new().route("/search", get(search))
}

#[derive(Debug, Deserialize)]
struct SearchQuery {
    q: String,
    account_id: Option<i64>,
    #[serde(default = "default_limit")]
    limit: i64,
    #[serde(default)]
    offset: i64,
    #[serde(default = "default_sort")]
    sort: String,
    /// Default false — match Gmail and exclude Trash/Spam from global
    /// search hits. Caller flips it on when the user is explicitly
    /// inside unified Trash / Spam and expects to see those rows.
    #[serde(default)]
    include_trash_spam: bool,
}

const fn default_limit() -> i64 {
    50
}

fn default_sort() -> String {
    "date_desc".to_string()
}

async fn search(
    State(s): State<AppState>,
    Query(q): Query<SearchQuery>,
) -> Result<Json<Vec<SearchHit>>> {
    let limit = q.limit.clamp(1, 200);
    let offset = q.offset.max(0);
    Ok(Json(s.db.search(
        &q.q,
        q.account_id,
        limit,
        offset,
        &q.sort,
        q.include_trash_spam,
    )?))
}
