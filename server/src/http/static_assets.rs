use std::path::PathBuf;

use axum::{
    http::{header, HeaderValue, StatusCode, Uri},
    response::{IntoResponse, Response},
    Router,
};
use tower_http::services::ServeDir;

use super::AppState;

/// Serve the built SvelteKit SPA with HTML-5 fallback (`index.html` for
/// unknown routes, mirroring `adapter-static` + `fallback: 'index.html'`).
/// Anything starting with `/api`, `/health`, or `/version` is left alone.
pub fn routes(dir: PathBuf) -> Router<AppState> {
    let index = dir.join("index.html");
    Router::new().fallback_service(ServeDir::new(dir).fallback(tower::service_fn(
        move |_req: axum::http::Request<axum::body::Body>| {
            let index = index.clone();
            async move {
                match tokio::fs::read(&index).await {
                    Ok(bytes) => Ok::<_, std::convert::Infallible>(html_response(bytes)),
                    Err(_) => Ok(not_found()),
                }
            }
        },
    )))
}

fn html_response(bytes: Vec<u8>) -> Response {
    (
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/html; charset=utf-8"),
        )],
        bytes,
    )
        .into_response()
}

fn not_found() -> Response {
    (StatusCode::NOT_FOUND, "not found").into_response()
}

#[allow(dead_code)]
fn mime_from(uri: &Uri) -> &'static str {
    let path = uri.path();
    if path.ends_with(".js") {
        "application/javascript"
    } else if path.ends_with(".css") {
        "text/css"
    } else if path.ends_with(".json") {
        "application/json"
    } else if path.ends_with(".svg") {
        "image/svg+xml"
    } else {
        "text/html; charset=utf-8"
    }
}
