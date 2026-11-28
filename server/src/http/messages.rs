use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;

use super::AppState;
use crate::{
    error::Result,
    storage::{Label, MessageDetail, MessageListItem},
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/messages", get(list))
        .route("/messages/:id", get(detail))
        .route("/messages/:id/read", post(set_read))
        .route("/messages/:id/spam", post(mark_spam))
        .route("/messages/:id/not-spam", post(mark_not_spam))
        .route("/messages/:id/trash", post(mark_trash))
        .route("/messages/:id/archive", post(archive))
        .route("/messages/:id/move", post(move_to))
        .route("/messages/:id/send-receipt", post(send_receipt))
        .route("/messages/bulk", post(bulk_action))
        .route("/messages/bulk/move-to", post(bulk_move_to))
        .route("/messages/folder-action", post(folder_action))
        .route("/labels", get(labels))
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
    #[serde(default = "default_sort")]
    sort: String,
}

fn default_sort() -> String {
    "date_desc".to_string()
}

const fn default_limit() -> i64 {
    50
}

fn collect_labels(labels: Option<&str>, label: Option<&str>) -> Vec<String> {
    if let Some(csv) = labels {
        return csv
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect();
    }
    label.map(|s| vec![s.to_string()]).unwrap_or_default()
}

async fn list(
    State(s): State<AppState>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<MessageListItem>>> {
    s.vault.require_unlocked()?;
    let limit = q.limit.clamp(1, 200);
    let offset = q.offset.max(0);
    let labels = collect_labels(q.labels.as_deref(), q.label.as_deref());
    Ok(Json(s.db.list_messages(
        q.account_id,
        &labels,
        limit,
        offset,
        &q.sort,
    )?))
}

async fn detail(State(s): State<AppState>, Path(id): Path<i64>) -> Result<Json<MessageDetail>> {
    s.vault.require_unlocked()?;
    let mut detail = s.db.get_message_detail(id)?;
    // in_reply_to + references live in the raw RFC822 headers, not in
    // any indexed column. Parse the blob so compose-reply can emit a
    // standards-compliant References chain for receiving clients.
    if let Ok(raw) = super::body::load_blob(&s, id) {
        let parsed = crate::sync::parser::parse(&raw);
        detail.in_reply_to = parsed.in_reply_to;
        detail.references = parsed.references;
    }
    Ok(Json(detail))
}

#[derive(serde::Serialize)]
struct SendReceiptResponse {
    id: i64,
    sent_to: String,
}

/// POST /api/messages/:id/send-receipt — manually dispatch a
/// disposition-notification (MDN) for a message that has a non-null
/// `receipt_to`. Postern never auto-sends receipts; the user has to
/// explicitly click "Send receipt" on the read banner. SMTP runs
/// inside `spawn_blocking` because the underlying lettre/smtp stack is
/// synchronous.
async fn send_receipt(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<SendReceiptResponse>> {
    s.vault.require_unlocked()?;
    let db = s.db.clone();
    let vpn = s.vpn.clone();
    let vault = s.vault.clone();
    let sent_to = tokio::task::spawn_blocking(move || {
        crate::send::send_read_receipt_blocking(&db, &vpn, &vault, id)
    })
    .await
    .map_err(|e| crate::error::Error::Other(anyhow::anyhow!("join: {e}")))??;
    Ok(Json(SendReceiptResponse { id, sent_to }))
}

#[derive(Debug, Deserialize)]
struct LabelQuery {
    account_id: i64,
}

async fn labels(
    State(s): State<AppState>,
    Query(q): Query<LabelQuery>,
) -> Result<Json<Vec<Label>>> {
    Ok(Json(s.db.list_labels(q.account_id)?))
}

/// Move a message to the Spam folder (IMAP server-side + local label update).
async fn mark_spam(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>> {
    move_message_to(s, id, "spam").await
}

/// Move a message from Spam back to INBOX, and allowlist the sender so
/// future mail from the same address bypasses the spam filter — sync
/// auto-rescues anything from a trusted sender that the IMAP server
/// later re-files into Spam.
async fn mark_not_spam(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>> {
    // Look up the message's sender BEFORE moving — the optimistic
    // relabel inside move_message_to doesn't change from_addr, but
    // doing this first keeps the allowlist insert next to the spam
    // verdict it's overriding.
    if let Ok(detail) = s.db.get_message_detail(id) {
        if let Some(from) = detail.message.from_addr.as_deref() {
            match s.db.add_trusted_sender(detail.message.account_id, from) {
                Ok(true) => tracing::info!(
                    msg_id = id,
                    sender = from,
                    "trusted-sender added via not-spam"
                ),
                Ok(false) => {} // already trusted
                Err(e) => tracing::warn!(error = %e, sender = from, "trusted-sender insert failed"),
            }
        }
    }
    move_message_to(s, id, "inbox").await
}

/// Move a message to Trash.
async fn mark_trash(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>> {
    move_message_to(s, id, "trash").await
}

/// Archive a message — moves it to the account's configured archive folder.
/// Falls back to [Gmail]/All Mail for Gmail accounts and Archive for IMAP.
async fn archive(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>> {
    move_message_to(s, id, "archive").await
}

#[derive(Debug, Deserialize)]
struct MoveBody {
    folder: String,
}

/// Move a message to an arbitrary folder. Creates the target folder on
/// IMAP if it doesn't exist, then issues the move and updates the local
/// label set optimistically.
async fn move_to(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<MoveBody>,
) -> Result<Json<serde_json::Value>> {
    s.vault.require_unlocked()?;
    let target = body.folder.trim().trim_matches('/').to_owned();
    if target.is_empty() {
        return Err(crate::error::Error::BadRequest("folder is required".into()));
    }
    let msg = s.db.get_message_detail(id)?;
    let account = s.db.get_account(msg.message.account_id)?;
    let password = s.db.account_password(account.id, &s.vault)?;

    // Best-known source folder — the current label set is our only clue
    // without round-tripping to the server. If we guess wrong the IMAP
    // Message-ID search just returns empty and the move no-ops.
    let from_folder = msg
        .labels
        .iter()
        .find(|l| l.as_str() != "INBOX")
        .cloned()
        .unwrap_or_else(|| "INBOX".to_string());

    let db = s.db.clone();
    let account_id = account.id;
    let msg_id = msg.message.id;

    crate::mail::spawn_move(
        s.vpn.clone(),
        account,
        password,
        msg_id,
        msg.message.message_id.clone(),
        from_folder,
        target.clone(),
        true,
        s.move_jobs.clone(),
    );

    // Register the label even before the IMAP sync catches up, and
    // overwrite the message's current labels with the new single label —
    // same optimistic pattern spam/trash/archive already use.
    db.upsert_label(account_id, &target, "user")?;
    db.relabel_message(msg_id, account_id, &[target.as_str()])?;

    Ok(Json(serde_json::json!({
        "id": msg_id,
        "target_folder": target,
        "labels": vec![target],
    })))
}

/// Bulk-apply an action to many message ids in one request. Keeps the
/// browser from firing N individual HTTP calls when the user
/// multi-selects + hits Delete / Archive / Mark-read.
///
/// Actions supported:
///   - `trash` / `archive` — move to the corresponding folder
///     (per-account, same resolution as the single-action endpoints)
///   - `read` / `unread` — flip `is_read` in the local DB only
///
/// The IMAP side of trash/archive is already fire-and-forget inside
/// `move_message_to`, so the endpoint returns quickly with counters
/// for successes / failures. A failure on one id doesn't short-circuit
/// the rest — the UI needs the per-id outcome to reconcile its
/// optimistic state.
#[derive(Debug, Deserialize)]
struct BulkActionBody {
    ids: Vec<i64>,
    action: String,
}

async fn bulk_action(
    State(s): State<AppState>,
    Json(body): Json<BulkActionBody>,
) -> Result<Json<serde_json::Value>> {
    let mut ok: Vec<i64> = Vec::with_capacity(body.ids.len());
    let mut failed: Vec<i64> = Vec::new();

    match body.action.as_str() {
        "trash" | "archive" | "spam" | "notspam" => {
            s.vault.require_unlocked()?;
            let target = body.action.as_str();
            for id in body.ids {
                match move_message_to(s.clone(), id, target).await {
                    Ok(_) => ok.push(id),
                    Err(e) => {
                        tracing::warn!(id, error = %e, "bulk {} failed", target);
                        failed.push(id);
                    }
                }
            }
        }
        "read" | "unread" => {
            let read = body.action == "read";
            for id in body.ids {
                match s.db.set_message_read(id, read) {
                    Ok(true) => ok.push(id),
                    Ok(false) => failed.push(id), // not found — probably deleted
                    Err(e) => {
                        tracing::warn!(id, error = %e, "bulk read-flag failed");
                        failed.push(id);
                    }
                }
            }
        }
        other => {
            return Err(crate::error::Error::BadRequest(format!(
                "unknown bulk action: {other}"
            )));
        }
    }

    Ok(Json(serde_json::json!({
        "action": body.action,
        "ok": ok,
        "failed": failed,
    })))
}

/// Multi-select "Move to <folder>". Loops the same logic the
/// per-message `/messages/:id/move` endpoint uses, applied to every
/// id in the request. Per-id failures don't short-circuit the rest;
/// the response carries `ok` + `failed` so the UI can reconcile its
/// optimistic update.
#[derive(Debug, Deserialize)]
struct BulkMoveBody {
    ids: Vec<i64>,
    folder: String,
}

async fn bulk_move_to(
    State(s): State<AppState>,
    Json(body): Json<BulkMoveBody>,
) -> Result<Json<serde_json::Value>> {
    s.vault.require_unlocked()?;
    let target = body.folder.trim().trim_matches('/').to_owned();
    if target.is_empty() {
        return Err(crate::error::Error::BadRequest("folder is required".into()));
    }

    let mut ok: Vec<i64> = Vec::with_capacity(body.ids.len());
    let mut failed: Vec<i64> = Vec::new();

    // Pre-register the label once per account so the per-message
    // updates inside the loop don't each recreate it.
    let mut accounts_seen: std::collections::HashSet<i64> = std::collections::HashSet::new();

    for id in body.ids {
        let result = move_one_to_folder(&s, id, &target, &mut accounts_seen).await;
        match result {
            Ok(()) => ok.push(id),
            Err(e) => {
                tracing::warn!(id, target = %target, error = %e, "bulk move failed");
                failed.push(id);
            }
        }
    }

    Ok(Json(serde_json::json!({
        "action": "move_to",
        "folder": target,
        "ok": ok,
        "failed": failed,
    })))
}

async fn move_one_to_folder(
    s: &AppState,
    id: i64,
    target: &str,
    accounts_seen: &mut std::collections::HashSet<i64>,
) -> Result<()> {
    let msg = s.db.get_message_detail(id)?;
    let account = s.db.get_account(msg.message.account_id)?;
    let password = s.db.account_password(account.id, &s.vault)?;

    let from_folder = msg
        .labels
        .iter()
        .find(|l| l.as_str() != "INBOX" && l.as_str() != target)
        .cloned()
        .unwrap_or_else(|| "INBOX".to_string());

    let db = s.db.clone();
    let account_id = account.id;
    let msg_id = msg.message.id;

    // Background IMAP move — same fire-and-forget pattern as
    // the single-message endpoint. Local labels update immediately;
    // the IMAP server catches up asynchronously.
    crate::mail::spawn_move(
        s.vpn.clone(),
        account,
        password,
        msg_id,
        msg.message.message_id.clone(),
        from_folder,
        target.to_owned(),
        true,
        s.move_jobs.clone(),
    );

    if accounts_seen.insert(account_id) {
        db.upsert_label(account_id, target, "user")?;
    }
    db.relabel_message(msg_id, account_id, &[target])?;

    Ok(())
}

/// Folder-wide actions that don't fit the bulk-by-id pattern.
///
/// `mark_read` flips `is_read` on every message in the folder on the
/// given account. Local-only — read state isn't synced to IMAP.
///
/// `empty` is permanent deletion, and for that reason is gated to
/// trash / spam folders only. Emptying INBOX or a user label would
/// be a catastrophic foot-gun if the dialog's "Are you sure?" got
/// misread; every other mail client (Gmail, Outlook, Thunderbird)
/// only exposes Empty on Trash and Spam for the same reason.
#[derive(Debug, Deserialize)]
struct FolderActionBody {
    account_id: i64,
    folder: String,
    /// "`mark_read`" or "empty".
    action: String,
}

async fn folder_action(
    State(s): State<AppState>,
    Json(body): Json<FolderActionBody>,
) -> Result<Json<serde_json::Value>> {
    s.vault.require_unlocked()?;
    let account = s.db.get_account(body.account_id)?;
    let folder = body.folder.trim();
    if folder.is_empty() {
        return Err(crate::error::Error::BadRequest("folder is required".into()));
    }

    match body.action.as_str() {
        "mark_read" => {
            let updated = s.db.mark_label_all_read(account.id, folder)?;
            let _ = s.db.log_event(
                "folder_mark_read",
                Some(&format!(
                    "{}: {} ({} updated)",
                    account.email, folder, updated
                )),
                None,
            );
            Ok(Json(serde_json::json!({
                "action": "mark_read",
                "folder": folder,
                "updated": updated,
            })))
        }
        "empty" => {
            // Safety gate: empty is permanent delete, so block the folders
            // where wiping everything is almost certainly a mistake —
            // INBOX (live mail), Sent (outgoing archive), Drafts (work
            // in progress). Trash, Spam, user labels, and Archive are
            // all allowed — the caller is expected to surface a clear
            // confirmation dialog for anything other than Trash/Spam.
            let is_gmail = account.kind == crate::storage::AccountKind::Gmail;
            let protected: &[&str] = if is_gmail {
                &[
                    "INBOX",
                    "[Gmail]/Sent Mail",
                    "[Gmail]/Drafts",
                    "[Gmail]/All Mail",
                    "[Gmail]/Important",
                    "[Gmail]/Starred",
                ]
            } else {
                &["INBOX", "Sent", "Drafts", "Sent Items", "Sent Messages"]
            };
            if protected.iter().any(|p| folder.eq_ignore_ascii_case(p)) {
                return Err(crate::error::Error::BadRequest(format!(
                    "{folder} is a protected system folder — empty is blocked here"
                )));
            }

            // Settle any in-flight bulk-trash IMAP MOVEs for this
            // account first. Without this, a user who bulk-trashes a
            // batch and immediately clicks Empty Trash can race the
            // MOVE: we hard-delete the local row, EXPUNGE finds the
            // server-side Trash empty (because the MOVE hasn't landed
            // yet), and the next sync re-imports the message from the
            // now-populated server-Trash. Cap at 8s so a stuck MOVE
            // doesn't block the whole empty action — the IMAP-side
            // expunge runs either way and stragglers get cleaned up
            // on the next user-triggered empty.
            let drained = s
                .move_jobs
                .await_idle(account.id, std::time::Duration::from_secs(8))
                .await;
            if !drained {
                tracing::warn!(
                    account = %account.email,
                    folder,
                    pending = s.move_jobs.pending(account.id),
                    "empty-folder: pending IMAP moves didn't settle in time; proceeding anyway"
                );
            }

            let (message_ids, deleted) = s.db.hard_delete_by_label(account.id, folder)?;

            let _ = s.db.log_event(
                "folder_emptied",
                Some(&format!(
                    "{}: {} ({} messages)",
                    account.email, folder, deleted
                )),
                None,
            );

            // Best-effort IMAP purge — the local rows are already gone
            // by the time we get here, so a failure on the server side
            // just means remote copies linger until the next sync
            // picks them up (for Trash/Spam they'll just get re-purged
            // on the next empty, so the divergence is bounded).
            if !message_ids.is_empty() {
                let password = s.db.account_password(account.id, &s.vault)?;
                crate::mail::spawn_expunge(
                    s.vpn.clone(),
                    account.clone(),
                    password,
                    folder.to_string(),
                );
            }

            Ok(Json(serde_json::json!({
                "action": "empty",
                "folder": folder,
                "deleted": deleted,
            })))
        }
        other => Err(crate::error::Error::BadRequest(format!(
            "unknown folder action: {other}"
        ))),
    }
}

async fn move_message_to(s: AppState, id: i64, target: &str) -> Result<Json<serde_json::Value>> {
    s.vault.require_unlocked()?;
    let msg = s.db.get_message_detail(id)?;
    let account = s.db.get_account(msg.message.account_id)?;
    let password = s.db.account_password(account.id, &s.vault)?;

    let (from_folder, to_folder, new_labels) =
        crate::mail::resolve_smart_move(target, &account, &msg.labels, msg.message.date_utc);

    let account_id = account.id;
    let msg_id = msg.message.id;

    crate::mail::spawn_move(
        s.vpn.clone(),
        account,
        password,
        msg_id,
        msg.message.message_id.clone(),
        from_folder,
        to_folder.clone(),
        crate::mail::smart_move_needs_ensure(target),
        s.move_jobs.clone(),
    );

    // Update local labels immediately (optimistic).
    let label_refs: Vec<&str> = new_labels.iter().map(std::string::String::as_str).collect();
    s.db.relabel_message(msg_id, account_id, &label_refs)?;

    Ok(Json(serde_json::json!({
        "id": msg_id,
        "moved_to": target,
        "target_folder": to_folder,
        "labels": new_labels,
    })))
}

#[derive(Debug, Deserialize)]
struct ReadPatch {
    #[serde(default = "default_true")]
    is_read: bool,
}

const fn default_true() -> bool {
    true
}

async fn set_read(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<ReadPatch>,
) -> Result<(StatusCode, Json<serde_json::Value>)> {
    let found = s.db.set_message_read(id, body.is_read)?;
    if found {
        Ok((
            StatusCode::OK,
            Json(serde_json::json!({ "id": id, "is_read": body.is_read })),
        ))
    } else {
        Ok((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "message not found" })),
        ))
    }
}
