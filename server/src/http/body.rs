use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, HeaderValue, StatusCode},
    response::Response,
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use tracing::warn;

use super::AppState;
use crate::{
    error::{Error, Result},
    privacy::{analyze_forensics, extract_attachment, render_body, Forensics, RenderedBody},
    sync,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/messages/:id/body", get(body_json))
        .route("/messages/:id/body.html", get(body_html))
        .route("/messages/:id/raw", get(raw))
        .route("/messages/:id/forensics", get(forensics))
        .route("/messages/:id/plain", get(plain))
        .route("/messages/:id/attachment/:index", get(attachment))
        .route("/messages/:id/attachment/:index/render-pdf", get(render_pdf))
        .route("/viewer-sandbox/status", get(sandbox_status))
}

pub fn img_proxy_routes() -> Router<AppState> {
    Router::new().route("/img-proxy/:token", get(img_proxy))
}

#[derive(Debug, Deserialize, Default)]
struct BodyQuery {
    /// When `1`, proxy remote images through our egress. Default is strict
    /// block — the iframe gets a 1x1 transparent placeholder instead.
    #[serde(default)]
    remote: Option<String>,
}

impl BodyQuery {
    fn allow_remote(&self) -> bool {
        matches!(self.remote.as_deref(), Some("1" | "true" | "yes"))
    }
}

/// Apply the lockdown override on top of the user's per-request
/// `?remote=1` choice. When lockdown is on, remote content is
/// blocked even if the user explicitly asked for it via the URL —
/// no remote image / font / CSS fetch happens until lockdown is
/// turned off. Returns `caller_choice` when lockdown can't be
/// determined (DB hiccup) — fail-open here is correct because the
/// only consequence is a remote fetch the user explicitly asked
/// for, not a destructive action.
fn effective_allow_remote(s: &AppState, caller_choice: bool) -> bool {
    if !caller_choice {
        return false;
    }
    match s.db.lockdown_enabled() {
        Ok(true) => false,
        _ => true,
    }
}

async fn body_json(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Query(q): Query<BodyQuery>,
) -> Result<Json<RenderedBody>> {
    let allow = effective_allow_remote(&s, q.allow_remote());
    let rendered = render_one(&s, id, allow)?;
    Ok(Json(rendered))
}

/// Serves just the inert HTML body, content-type text/html with strict
/// CSP. Intended to be loaded directly into an `<iframe sandbox>` — the
/// SPA does the wrapping.
async fn body_html(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Query(q): Query<BodyQuery>,
) -> Result<Response> {
    let allow = effective_allow_remote(&s, q.allow_remote());
    let rendered = render_one(&s, id, allow)?;

    let doc = format!(
        "<!doctype html>\n\
         <html><head>\n\
         <meta charset=\"utf-8\">\n\
         <meta name=\"referrer\" content=\"no-referrer\">\n\
         <base target=\"_blank\">\n\
         <style>\n\
           :root {{ color-scheme: light dark; }}\n\
           html, body {{ margin: 0; padding: 1rem; \
             font-family: ui-sans-serif, system-ui, sans-serif; \
             font-size: 0.95rem; line-height: 1.55; }}\n\
           img {{ max-width: 100%; height: auto; }}\n\
           pre {{ white-space: pre-wrap; word-wrap: break-word; \
             font-family: inherit; }}\n\
           a {{ color: #2a6df4; }}\n\
           blockquote {{ border-left: 3px solid rgba(128,128,128,0.3); \
             padding-left: 0.75rem; margin-left: 0; color: rgba(128,128,128,0.85); }}\n\
         </style>\n\
         </head><body>\n\
         {body}\n\
         </body></html>",
        body = rendered.html,
    );

    let mut resp = Response::new(Body::from(doc));
    resp.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    // Belt-and-braces. The iframe sandbox already blocks these; CSP is the
    // second layer that kicks in even if a misconfigured iframe leaks through.
    resp.headers_mut().insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'none'; img-src /img-proxy/ data:; style-src 'unsafe-inline'; \
             base-uri 'none'; form-action 'none'; frame-ancestors 'self'",
        ),
    );
    resp.headers_mut().insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    resp.headers_mut().insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    Ok(resp)
}

/// Serve the raw RFC822 source. Served as text/plain so the browser
/// shows it inline when the user picks "Source" view.
async fn raw(State(s): State<AppState>, Path(id): Path<i64>) -> Result<Response> {
    let bytes = load_blob(&s, id)?;
    let mut resp = Response::new(Body::from(bytes));
    resp.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    resp.headers_mut().insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    Ok(resp)
}

async fn forensics(State(s): State<AppState>, Path(id): Path<i64>) -> Result<Json<Forensics>> {
    let bytes = load_blob(&s, id)?;
    let mut f = analyze_forensics(&bytes);
    // PGP/MIME messages have their real attachments inside the
    // encrypted blob. The outer envelope's MIME tree just shows the
    // pgp-encrypted stub + the .asc ciphertext, which is useless to
    // the UI. When we have a key that decrypts, re-analyse the
    // plaintext and replace mime_tree + attachments with what the
    // user actually wants to see / download. Headers / auth /
    // received chain stay from the outer message — those live there.
    if f.is_pgp_encrypted {
        if let Some(plain) = crate::pgp::try_decrypt(&bytes, &s.db, &s.vault) {
            let inner = analyze_forensics(&plain);
            f.attachments = inner.attachments;
            f.mime_tree = inner.mime_tree;
        }
    }
    Ok(Json(f))
}

#[derive(Debug, Deserialize, Default)]
struct AttachmentQuery {
    /// `download` (default) forces Content-Disposition: attachment.
    /// `inline` only takes effect for MIME types on the safe
    /// whitelist — anything else falls back to download behaviour.
    #[serde(default)]
    mode: Option<String>,
}

/// Serve the attachment bytes for message `id` at `index` (0-based,
/// matching the order they appear in the forensics payload).
///
/// Security model mirrors Mailpile:
///   - Default is attachment download — browser prompts, OS handles.
///   - Inline display is only allowed for a small set of MIME types
///     that every modern browser renders in its own sandbox
///     (images, audio, video, PDF). Everything else is forced to
///     download regardless of the `mode=inline` query param.
///   - `X-Content-Type-Options: nosniff` is set unconditionally so
///     the browser can't guess a "safer" MIME type and escalate a
///     disguised `.html` attachment into an inline render.
async fn attachment(
    State(s): State<AppState>,
    Path((id, index)): Path<(i64, usize)>,
    Query(q): Query<AttachmentQuery>,
) -> Result<Response> {
    let raw = load_blob(&s, id)?;
    // If the message is PGP-encrypted and we hold a secret that
    // decrypts it, extract from the plaintext instead — otherwise
    // the user gets armoured ciphertext instead of their invoice.
    let source = crate::pgp::try_decrypt(&raw, &s.db, &s.vault).unwrap_or(raw);

    let Some(att) = extract_attachment(&source, index) else {
        return Err(Error::NotFound);
    };

    let want_inline = matches!(q.mode.as_deref(), Some("inline"));
    let safe_for_inline = is_inline_whitelisted(&att.content_type);
    let disposition_kind = if want_inline && safe_for_inline {
        "inline"
    } else {
        "attachment"
    };

    let filename = sanitize_filename(att.filename.as_deref(), index);
    // RFC 5987 filename*= encodes non-ASCII cleanly; plain filename=
    // is kept as an ASCII fallback. Many older tools only read one or
    // the other, so we emit both.
    let disposition = format!(
        "{kind}; filename=\"{ascii}\"; filename*=UTF-8''{pct}",
        kind = disposition_kind,
        ascii = filename.ascii,
        pct = filename.percent,
    );

    let mut resp = Response::new(Body::from(att.bytes));
    resp.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&att.content_type)
            .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
    );
    resp.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&disposition)
            .unwrap_or_else(|_| HeaderValue::from_static("attachment")),
    );
    resp.headers_mut().insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    // Private: this is user mail; intermediate caches should never
    // hold onto it. Browser can cache within the session.
    resp.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, max-age=0"),
    );
    Ok(resp)
}

/// Path to the Unix socket the postern-viewer sandbox listens on.
/// Injected via the shared `viewer-socket` Docker volume. When the
/// viewer profile isn't enabled the directory is empty and the
/// socket simply doesn't exist; handlers fail cleanly with a 503.
const VIEWER_SOCKET: &str = "/run/viewer/viewer.sock";

/// Content types we can convert to PDF via the viewer sandbox.
/// Mirrors the sandbox's own ALLOWED_EXTS — keep the two lists in
/// sync when adding formats (extension set in deploy/viewer/server.py).
fn is_convertible(ct: &str) -> bool {
    let ct = ct.trim().to_ascii_lowercase();
    matches!(
        ct.as_str(),
        "application/msword"
            | "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
            | "application/vnd.ms-excel"
            | "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
            | "application/vnd.ms-powerpoint"
            | "application/vnd.openxmlformats-officedocument.presentationml.presentation"
            | "application/vnd.oasis.opendocument.text"
            | "application/vnd.oasis.opendocument.spreadsheet"
            | "application/vnd.oasis.opendocument.presentation"
            | "application/rtf"
            | "text/rtf"
            | "text/csv"
    )
}

/// Status probe for the UI — tells the frontend whether the Preview
/// button should appear for Office-doc attachments. Checks socket
/// existence only; avoids actually connecting until the user asks
/// for a conversion so a broken viewer doesn't slow every page load.
async fn sandbox_status(State(_s): State<AppState>) -> Result<Json<serde_json::Value>> {
    // Can't use FileType::is_socket() in the std portable API, but
    // Metadata::file_type is portable, and on Unix the MetadataExt
    // trait exposes the raw mode. We're only ever running on Linux
    // in a container so Unix-specific API is fine here.
    use std::os::unix::fs::FileTypeExt;
    let available = tokio::fs::metadata(VIEWER_SOCKET)
        .await
        .map(|m| m.file_type().is_socket())
        .unwrap_or(false);
    // Deliberately don't echo the internal socket path in the response —
    // it's an implementation detail the UI doesn't need, and surfacing
    // it invites clients to rely on the exact path.
    Ok(Json(serde_json::json!({
        "viewer_available": available,
    })))
}

/// Convert an Office / OpenDocument attachment to PDF via the viewer
/// sandbox and stream the PDF to the browser. Gated by:
///   1. Attachment must exist
///   2. Content-type must be on the convertible whitelist
///   3. Viewer socket must be reachable
/// Any other failure returns 5xx — the viewer page renders a friendly
/// "Preview not available, use Download" message in that case.
async fn render_pdf(
    State(s): State<AppState>,
    Path((id, index)): Path<(i64, usize)>,
) -> Result<Response> {
    let raw = load_blob(&s, id)?;
    let source = crate::pgp::try_decrypt(&raw, &s.db, &s.vault).unwrap_or(raw);

    let Some(att) = extract_attachment(&source, index) else {
        return Err(Error::NotFound);
    };
    if !is_convertible(&att.content_type) {
        return Err(Error::BadRequest(format!(
            "{} can't be converted to PDF — download the file instead",
            att.content_type
        )));
    }

    let filename = att.filename.clone().unwrap_or_else(|| format!("attachment-{index}"));
    let pdf = convert_via_viewer(&att.bytes, &filename)
        .await
        .map_err(|e| {
            warn!(error = %e, "viewer sandbox conversion failed");
            Error::Other(anyhow::anyhow!(
                "document conversion failed: {e}. If this is the first \
                 convertible attachment you've opened, the viewer sandbox \
                 may not be running — start it with \
                 `docker compose --profile viewer up -d`."
            ))
        })?;

    let mut resp = Response::new(Body::from(pdf));
    resp.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/pdf"),
    );
    // Deliberately Content-Disposition: inline — this endpoint is
    // called by the in-browser PDF viewer iframe, not by a download
    // link. The browser renders it as a PDF directly.
    resp.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static("inline"),
    );
    resp.headers_mut().insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    resp.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, max-age=0"),
    );
    Ok(resp)
}

/// Minimal HTTP client over Unix socket. The viewer sandbox speaks
/// HTTP/1.1 with a single endpoint (POST /convert), so a full HTTP
/// stack is overkill — we write the request by hand and slurp the
/// entire response into memory. PDF output is bounded by the 200MiB
/// input cap the sandbox enforces on its side.
async fn convert_via_viewer(bytes: &[u8], filename: &str) -> std::result::Result<Vec<u8>, String> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::UnixStream;
    use tokio::time::{timeout, Duration};

    // Connect with a bounded timeout so a jammed viewer container
    // doesn't hang the Postern request indefinitely.
    let mut stream = timeout(Duration::from_secs(5), UnixStream::connect(VIEWER_SOCKET))
        .await
        .map_err(|_| "viewer socket connect timed out".to_string())?
        .map_err(|e| format!("viewer socket connect: {e}"))?;

    // Sanitize filename for the header — printable ASCII only, no
    // control chars, and explicitly no path separators / NUL. The
    // viewer does its own sanitization too but belt-and-braces.
    let safe_name: String = filename
        .chars()
        .filter(|c| {
            (c.is_ascii_graphic() || *c == ' ')
                && !matches!(c, '/' | '\\' | '\0')
        })
        .take(200)
        .collect();
    let safe_name = if safe_name.is_empty() {
        "attachment".to_string()
    } else {
        safe_name
    };

    let req = format!(
        "POST /convert HTTP/1.1\r\n\
         Host: viewer\r\n\
         Content-Type: application/octet-stream\r\n\
         Content-Length: {}\r\n\
         X-Filename: {}\r\n\
         Connection: close\r\n\r\n",
        bytes.len(),
        safe_name,
    );
    timeout(Duration::from_secs(5), stream.write_all(req.as_bytes()))
        .await
        .map_err(|_| "write headers timed out".to_string())?
        .map_err(|e| format!("write headers: {e}"))?;
    timeout(Duration::from_secs(30), stream.write_all(bytes))
        .await
        .map_err(|_| "write body timed out".to_string())?
        .map_err(|e| format!("write body: {e}"))?;

    // Read until close. 120s is plenty for LibreOffice to finish
    // (the sandbox has its own 90s conversion cap).
    let mut resp = Vec::new();
    timeout(Duration::from_secs(120), stream.read_to_end(&mut resp))
        .await
        .map_err(|_| "read response timed out".to_string())?
        .map_err(|e| format!("read response: {e}"))?;

    // Parse minimal HTTP response: <status line>\r\n<headers>\r\n\r\n<body>
    let Some(sep) = find_double_crlf(&resp) else {
        return Err("viewer returned no headers".into());
    };
    let (head, body) = resp.split_at(sep + 4);
    let head_str = std::str::from_utf8(head).map_err(|_| "non-utf8 headers".to_string())?;
    let status_line = head_str.lines().next().unwrap_or("");
    // "HTTP/1.1 200 OK" — grab the numeric code
    let code = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(0);
    if code != 200 {
        let msg = std::str::from_utf8(body)
            .unwrap_or("(non-utf8 error)")
            .trim()
            .to_string();
        return Err(format!("viewer returned {code}: {msg}"));
    }
    Ok(body.to_vec())
}

fn find_double_crlf(haystack: &[u8]) -> Option<usize> {
    haystack.windows(4).position(|w| w == b"\r\n\r\n")
}

/// Same whitelist Mailpile uses — formats every modern browser can
/// render safely in its own sandbox. Anything not on this list
/// falls back to `attachment` disposition so the OS handles it.
fn is_inline_whitelisted(ct: &str) -> bool {
    let ct = ct.trim().to_ascii_lowercase();
    matches!(
        ct.as_str(),
        "image/png"
            | "image/jpeg"
            | "image/gif"
            | "image/webp"
            | "image/tiff"
            // SVG is deliberately NOT on this list: an SVG document opened
            // directly in the browser (Content-Disposition: inline) runs
            // any inline <script> elements it contains. Safe inside an
            // <img> tag because images don't execute scripts, but the
            // "Open in new tab" behaviour of this endpoint is not inside
            // an <img>. Download-only for SVG.
            | "audio/mp3"
            | "audio/mpeg"
            | "audio/ogg"
            | "audio/x-wav"
            | "audio/wav"
            | "video/mpeg"
            | "video/ogg"
            | "video/mp4"
            | "video/webm"
            | "application/pdf"
            | "text/plain"
    )
}

struct SafeFilename {
    /// ASCII-only fallback for legacy clients that ignore filename*.
    ascii: String,
    /// RFC 5987 percent-encoded UTF-8 for the filename* param.
    percent: String,
}

fn sanitize_filename(raw: Option<&str>, fallback_index: usize) -> SafeFilename {
    let name = raw
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(&"attachment");
    // Strip anything that could let a nested quote or CRLF escape the
    // Content-Disposition header value. Mailpile's extract path does
    // the same — header injection via attachment filenames is a real
    // class of bug.
    let cleaned: String = name
        .chars()
        .filter(|c| !matches!(c, '\r' | '\n' | '"' | '\0'))
        .collect();
    let ascii: String = cleaned
        .chars()
        .map(|c| {
            if c.is_ascii() && !c.is_ascii_control() {
                c
            } else {
                '_'
            }
        })
        .collect();
    let ascii = if ascii.trim().is_empty() {
        format!("attachment-{fallback_index}")
    } else {
        ascii
    };
    let percent = percent_encode(&cleaned);
    SafeFilename { ascii, percent }
}

fn percent_encode(s: &str) -> String {
    // RFC 5987: unreserved characters pass through; everything else
    // gets %XX-encoded using UTF-8 bytes.
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Extracted plain-text body. Separate endpoint (vs /body which wraps in
/// HTML for the iframe) because the Plain view wants just the text to
/// render in a <pre> in the UI itself.
async fn plain(State(s): State<AppState>, Path(id): Path<i64>) -> Result<Json<PlainBody>> {
    let bytes = load_blob(&s, id)?;
    let text = sync::body_text_of(&bytes).unwrap_or_default();
    Ok(Json(PlainBody { text }))
}

#[derive(serde::Serialize)]
struct PlainBody {
    text: String,
}

fn load_blob(s: &AppState, id: i64) -> Result<Vec<u8>> {
    let blob_sha256: String = {
        let conn = s.db.pool().get()?;
        conn.query_row(
            "SELECT blob_sha256 FROM messages WHERE id = ?1",
            rusqlite::params![id],
            |r| r.get(0),
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Error::NotFound,
            other => Error::Db(other),
        })?
    };
    Ok(s.blobs.get(&blob_sha256)?)
}

fn render_one(s: &AppState, id: i64, allow_remote: bool) -> Result<RenderedBody> {
    let raw = load_blob(s, id)?;

    // If the message is PGP-encrypted and we have a matching secret key,
    // swap the encrypted blob for its plaintext before the HTML sanitizer
    // sees it. The decrypted payload is usually a MIME sub-document; when
    // it's just bare text we wrap it in a minimal text/plain header so
    // render_body has something to chew on.
    let decrypted = crate::pgp::try_decrypt(&raw, &s.db, &s.vault);
    let raw: Vec<u8> = match decrypted {
        Some(plain)
            if plain.starts_with(b"Content-Type")
                || plain.starts_with(b"MIME-Version")
                || plain.starts_with(b"From ") =>
        {
            plain
        }
        Some(plain) => {
            let mut wrapped = b"Content-Type: text/plain; charset=utf-8\r\n\r\n".to_vec();
            wrapped.extend_from_slice(&plain);
            wrapped
        }
        None => raw,
    };

    Ok(render_body(&raw, &s.proxy, allow_remote))
}

async fn img_proxy(State(s): State<AppState>, Path(token): Path<String>) -> Result<Response> {
    let Some(url) = s.proxy.resolve(&token) else {
        return Err(Error::NotFound);
    };
    match s.proxy.fetch(&url).await {
        Ok(img) => {
            let mut resp = Response::new(Body::from(img.bytes));
            resp.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_str(&img.content_type)
                    .unwrap_or(HeaderValue::from_static("application/octet-stream")),
            );
            resp.headers_mut().insert(
                header::CACHE_CONTROL,
                HeaderValue::from_static("private, max-age=86400"),
            );
            // These headers protect us from being reflected into an SSRF or XSS chain.
            resp.headers_mut().insert(
                "x-content-type-options",
                HeaderValue::from_static("nosniff"),
            );
            resp.headers_mut().insert(
                header::CONTENT_SECURITY_POLICY,
                HeaderValue::from_static("default-src 'none'; img-src 'self' data:"),
            );
            Ok(resp)
        }
        Err(e) => {
            warn!(%url, error = %e, "img-proxy fetch failed");
            // Serve a 1x1 transparent gif on failure so the iframe doesn't
            // turn into a broken-image icon every remote fetch error.
            const PLACEHOLDER: &[u8] = &[
                0x47, 0x49, 0x46, 0x38, 0x39, 0x61, 0x01, 0x00, 0x01, 0x00, 0x80, 0x00, 0x00, 0xff,
                0xff, 0xff, 0x00, 0x00, 0x00, 0x21, 0xf9, 0x04, 0x01, 0x00, 0x00, 0x00, 0x00, 0x2c,
                0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x02, 0x01, 0x44, 0x00, 0x3b,
            ];
            let mut resp = Response::new(Body::from(PLACEHOLDER));
            resp.headers_mut()
                .insert(header::CONTENT_TYPE, HeaderValue::from_static("image/gif"));
            *resp.status_mut() = StatusCode::OK;
            Ok(resp)
        }
    }
}
