use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};

use super::AppState;
use crate::{
    error::Result,
    rules::{NewRule, Rule},
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/rules", get(list).post(create))
        .route("/rules/:id", axum::routing::delete(delete))
        .route("/rules/:id/toggle", post(toggle))
        .route("/rules/apply", post(apply_retroactive))
}

async fn list(State(s): State<AppState>) -> Result<Json<Vec<Rule>>> {
    Ok(Json(s.db.list_rules()?))
}

async fn create(State(s): State<AppState>, Json(r): Json<NewRule>) -> Result<Json<Rule>> {
    Ok(Json(s.db.create_rule(&r)?))
}

async fn delete(State(s): State<AppState>, Path(id): Path<i64>) -> Result<Json<serde_json::Value>> {
    s.db.delete_rule(id)?;
    Ok(Json(serde_json::json!({ "deleted": id })))
}

#[derive(serde::Deserialize)]
struct ToggleBody {
    enabled: bool,
}

async fn apply_retroactive(State(s): State<AppState>) -> Result<Json<serde_json::Value>> {
    let db = s.db.clone();
    let (checked, acted) =
        tokio::task::spawn_blocking(move || crate::rules::apply_rules_retroactive(&db))
            .await
            .map_err(|e| crate::error::Error::Other(anyhow::anyhow!("join: {e}")))?;
    Ok(Json(
        serde_json::json!({ "checked": checked, "acted": acted }),
    ))
}

async fn toggle(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(b): Json<ToggleBody>,
) -> Result<Json<Rule>> {
    Ok(Json(s.db.toggle_rule(id, b.enabled)?))
}
