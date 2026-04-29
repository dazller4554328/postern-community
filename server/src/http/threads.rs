use axum::{
    extract::{Path, Query, State},
    routing::get,
    Json, Router,
};
use serde::Deserialize;

use super::AppState;
use crate::{
    error::Result,
    storage::{MessageListItem, ThreadSummary},
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/threads", get(list))
        .route("/threads/:thread_id/messages", get(thread_messages))
}

#[derive(Debug, Deserialize)]
struct ListQuery {
    account_id: Option<i64>,
    label: Option<String>,
    labels: Option<String>,
    #[serde(default = "default_limit")]
    limit: i64,
    #[serde(default)]
    offset: i64,
}

const fn default_limit() -> i64 {
    50
}

async fn list(
    State(s): State<AppState>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<ThreadSummary>>> {
    let limit = q.limit.clamp(1, 200);
    let offset = q.offset.max(0);
    let labels: Vec<String> = if let Some(csv) = q.labels {
        csv.split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect()
    } else if let Some(single) = q.label {
        vec![single]
    } else {
        vec![]
    };
    Ok(Json(s.db.list_threads(
        q.account_id,
        &labels,
        limit,
        offset,
    )?))
}

async fn thread_messages(
    State(s): State<AppState>,
    Path(thread_id): Path<String>,
) -> Result<Json<Vec<MessageListItem>>> {
    // URL-decoded by axum already. Message-IDs commonly contain '@' and
    // angle brackets — those round-trip fine as path params.
    Ok(Json(s.db.thread_messages(&thread_id)?))
}
