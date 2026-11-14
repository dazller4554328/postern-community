//! `/api/tier` — surfaces the build tier and feature flags so the
//! web UI can decide which tabs to render. Works whether the vault
//! is locked or unlocked; the answer is a compile-time constant, no
//! sensitive data.

use axum::{routing::get, Json, Router};

use super::AppState;
use crate::tier::{current_info, TierInfo};

pub fn routes() -> Router<AppState> {
    Router::new().route("/tier", get(get_tier))
}

async fn get_tier() -> Json<TierInfo> {
    Json(current_info())
}
