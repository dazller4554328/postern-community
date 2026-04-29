use axum::{http::StatusCode, response::IntoResponse, Json};
use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("not found")]
    NotFound,

    #[error("bad request: {0}")]
    BadRequest(String),

    /// State conflict — e.g. trying to cancel an outbox row that has
    /// already dispatched. Maps to HTTP 409.
    #[error("conflict: {0}")]
    Conflict(String),

    /// Vault is uninitialised or locked — maps to HTTP 423.
    #[error("locked: {0}")]
    Locked(String),

    #[error("imap: {0}")]
    Imap(String),

    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("db: {0}")]
    Db(#[from] rusqlite::Error),

    #[error("pool: {0}")]
    Pool(#[from] r2d2::Error),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Serialize)]
struct ErrBody {
    error: String,
}

impl IntoResponse for Error {
    fn into_response(self) -> axum::response::Response {
        let status = match &self {
            Error::NotFound => StatusCode::NOT_FOUND,
            Error::BadRequest(_) => StatusCode::BAD_REQUEST,
            Error::Conflict(_) => StatusCode::CONFLICT,
            Error::Locked(_) => StatusCode::LOCKED,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        if status.is_server_error() {
            tracing::error!(error = ?self, "request failed");
        }
        (
            status,
            Json(ErrBody {
                error: self.to_string(),
            }),
        )
            .into_response()
    }
}
