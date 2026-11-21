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
        // A bare "no rows" from rusqlite means the record is missing, not
        // that the server faulted — surface it as 404 with a generic body
        // instead of leaking the internal db message as a 500.
        let missing_record = matches!(&self, Error::Db(rusqlite::Error::QueryReturnedNoRows));
        let status = if missing_record {
            StatusCode::NOT_FOUND
        } else {
            match &self {
                Error::NotFound => StatusCode::NOT_FOUND,
                Error::BadRequest(_) => StatusCode::BAD_REQUEST,
                Error::Conflict(_) => StatusCode::CONFLICT,
                Error::Locked(_) => StatusCode::LOCKED,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            }
        };
        if status.is_server_error() {
            tracing::error!(error = ?self, "request failed");
        }
        // Db/Io/Pool/Other messages carry internals (paths, schema, pool
        // state) — log them above, but never echo them to the client.
        // Imap stays verbatim: it describes the remote server's response
        // and is what the user needs to debug account setup.
        let error = if missing_record {
            "not found".to_string()
        } else {
            match &self {
                Error::Db(_) | Error::Io(_) | Error::Pool(_) | Error::Other(_) => {
                    "internal error".to_string()
                }
                _ => self.to_string(),
            }
        };
        (status, Json(ErrBody { error })).into_response()
    }
}
