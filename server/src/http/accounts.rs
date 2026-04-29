use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use super::AppState;
use crate::{
    error::{Error, Result},
    storage::{Account, NewAccount},
    sync::SyncReport,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/accounts", get(list).post(create))
        .route("/accounts/:id", axum::routing::delete(delete))
        .route("/accounts/:id/credentials", post(update_credentials))
        .route("/accounts/:id/sync-enabled", post(set_sync_enabled))
        .route("/accounts/:id/send-enabled", post(set_send_enabled))
        .route("/accounts/:id/include-in-unified", post(set_include_in_unified))
        .route("/accounts/:id/sync", post(trigger_sync))
        .route("/accounts/:id/sync/full", post(trigger_full_sync))
        .route("/accounts/:id/sync/status", get(sync_status))
        .route("/accounts/:id/delete-policy", post(set_delete_policy))
        .route("/accounts/:id/archive-folder", post(set_archive_folder))
        .route("/accounts/:id/archive-strategy", post(set_archive_strategy))
        .route("/accounts/:id/archive-enabled", post(set_archive_enabled))
        .route("/accounts/:id/auto-archive", post(set_auto_archive))
        .route(
            "/accounts/:id/auto-archive/preview",
            get(auto_archive_preview),
        )
        .route(
            "/accounts/:id/purge-gmail-categories",
            post(set_purge_gmail_categories),
        )
        .route("/accounts/:id/skip-gmail-trash", post(set_skip_gmail_trash))
        .route("/accounts/:id/retention", post(set_retention))
        .route("/accounts/:id/retention/preview", get(retention_preview))
        .route(
            "/accounts/:id/rescan-gmail-labels",
            post(rescan_gmail_labels),
        )
        .route(
            "/accounts/:id/purge-server-copies",
            post(start_purge_server_copies),
        )
        .route("/accounts/:id/purge-status", get(purge_status))
        .route("/accounts/:id/avatar", post(set_avatar))
        .route("/accounts/:id/signature", post(set_signature))
        .route(
            "/settings/sync-interval",
            get(get_sync_interval).post(set_sync_interval),
        )
}

#[derive(Serialize)]
struct AccountView {
    #[serde(flatten)]
    account: Account,
}

async fn list(State(s): State<AppState>) -> Result<Json<Vec<AccountView>>> {
    let accounts = s.db.list_accounts()?;
    Ok(Json(
        accounts
            .into_iter()
            .map(|a| AccountView { account: a })
            .collect(),
    ))
}

/// Fast credential check against the live IMAP server, routed through
/// the VPN interface when the kill-switch is engaged. Returns BadRequest
/// with a friendly message on any failure — the caller (account create
/// or credentials update) surfaces it to the UI so the user sees "bad
/// App Password" instead of a vague sync-error minutes later.
async fn probe_imap_credentials(
    s: &AppState,
    host: String,
    port: u16,
    email: String,
    password: String,
) -> Result<()> {
    let bind_iface = s.vpn.bind_iface();
    tokio::task::spawn_blocking(move || {
        crate::sync::probe(&host, port, &email, &password, bind_iface.as_deref())
    })
    .await
    .map_err(|e| Error::Other(anyhow::anyhow!("probe join: {e}")))?
    .map_err(Error::BadRequest)
}

async fn create(
    State(s): State<AppState>,
    Json(new): Json<NewAccount>,
) -> Result<Json<AccountView>> {
    if new.app_password.is_empty() {
        return Err(Error::BadRequest("app_password is required".into()));
    }
    if new.imap_host.is_empty() {
        return Err(Error::BadRequest("imap_host is required".into()));
    }

    s.vault.require_unlocked()?;

    // Probe the IMAP creds BEFORE inserting. The old behaviour ("insert
    // then kick background sync") meant bad passwords only showed up as
    // a vague sync-error toast minutes later — the user had no idea
    // whether the row was good until the scheduler tick fired.
    probe_imap_credentials(
        &s,
        new.imap_host.clone(),
        new.imap_port,
        new.email.clone(),
        new.app_password.clone(),
    )
    .await?;

    let account = s.db.insert_account(&new, &s.vault)?;
    let _ = s.db.log_event(
        "account_added",
        Some(&format!("{} ({}:{})", account.email, account.imap_host, account.imap_port)),
        None,
    );
    // Kick off an initial sync in the background now that we know the
    // creds actually work.
    s.scheduler.trigger(account.id).await;
    Ok(Json(AccountView { account }))
}

#[derive(serde::Deserialize)]
struct CredentialsBody {
    app_password: String,
}

async fn update_credentials(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(b): Json<CredentialsBody>,
) -> Result<Json<serde_json::Value>> {
    if b.app_password.is_empty() {
        return Err(Error::BadRequest("app_password is required".into()));
    }
    s.vault.require_unlocked()?;
    let account = s.db.get_account(id)?;
    probe_imap_credentials(
        &s,
        account.imap_host.clone(),
        account.imap_port,
        account.email.clone(),
        b.app_password.clone(),
    )
    .await?;

    s.db.update_account_password(id, &b.app_password, &s.vault)?;
    let _ = s.db.log_event(
        "account_credentials_updated",
        Some(&account.email),
        None,
    );
    // Retry sync with the fresh credentials so the user gets instant
    // feedback that things are working.
    s.scheduler.trigger(id).await;
    Ok(Json(serde_json::json!({ "id": id, "ok": true })))
}

#[derive(serde::Deserialize)]
struct EnabledBody {
    enabled: bool,
}

async fn set_sync_enabled(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(b): Json<EnabledBody>,
) -> Result<Json<serde_json::Value>> {
    s.db.set_sync_enabled(id, b.enabled)?;
    let acct = s.db.get_account(id)?;
    let _ = s.db.log_event(
        "sync_enabled_changed",
        Some(&format!(
            "{}: {}",
            acct.email,
            if b.enabled { "on" } else { "off" }
        )),
        None,
    );
    if b.enabled {
        // Re-enabling should pull whatever came in while we were paused.
        s.scheduler.trigger(id).await;
    }
    Ok(Json(serde_json::json!({ "id": id, "sync_enabled": b.enabled })))
}

async fn set_send_enabled(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(b): Json<EnabledBody>,
) -> Result<Json<serde_json::Value>> {
    s.db.set_send_enabled(id, b.enabled)?;
    let acct = s.db.get_account(id)?;
    let _ = s.db.log_event(
        "send_enabled_changed",
        Some(&format!(
            "{}: {}",
            acct.email,
            if b.enabled { "on" } else { "off" }
        )),
        None,
    );
    Ok(Json(serde_json::json!({ "id": id, "send_enabled": b.enabled })))
}

async fn set_include_in_unified(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(b): Json<EnabledBody>,
) -> Result<Json<serde_json::Value>> {
    s.db.set_include_in_unified(id, b.enabled)?;
    let acct = s.db.get_account(id)?;
    let _ = s.db.log_event(
        "include_in_unified_changed",
        Some(&format!(
            "{}: {}",
            acct.email,
            if b.enabled { "on" } else { "off" }
        )),
        None,
    );
    Ok(Json(
        serde_json::json!({ "id": id, "include_in_unified": b.enabled }),
    ))
}

async fn delete(State(s): State<AppState>, Path(id): Path<i64>) -> Result<Json<serde_json::Value>> {
    s.db.delete_account(id)?;
    Ok(Json(serde_json::json!({ "deleted": id })))
}

async fn trigger_sync(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>> {
    s.db.get_account(id)?; // 404 if missing
    s.scheduler.trigger(id).await;
    Ok(Json(serde_json::json!({ "triggered": id })))
}

/// Reset sync cursors for all folders on this account and kick off a
/// fresh pull from UID 1. Useful when the label map gets out of sync
/// with the server (e.g. after changing how labels are accumulated).
async fn trigger_full_sync(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>> {
    s.db.get_account(id)?;
    let cleared = s.db.reset_sync_state_for_account(id)?;
    s.scheduler.trigger(id).await;
    Ok(Json(
        serde_json::json!({ "triggered": id, "reset_folders": cleared }),
    ))
}

async fn sync_status(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Option<SyncReport>>> {
    s.db.get_account(id)?;
    Ok(Json(s.scheduler.last_report(id).await))
}

#[derive(serde::Deserialize)]
struct DeletePolicyBody {
    delete_after_sync: bool,
}

async fn get_sync_interval(State(s): State<AppState>) -> Result<Json<serde_json::Value>> {
    let secs = s.scheduler.get_interval();
    Ok(Json(serde_json::json!({ "interval_secs": secs })))
}

#[derive(serde::Deserialize)]
struct IntervalBody {
    interval_secs: u64,
}

async fn set_sync_interval(
    State(s): State<AppState>,
    Json(b): Json<IntervalBody>,
) -> Result<Json<serde_json::Value>> {
    s.scheduler.set_interval(b.interval_secs);
    let actual = s.scheduler.get_interval();
    let _ =
        s.db.log_event("sync_interval_changed", Some(&format!("{}s", actual)), None);
    Ok(Json(serde_json::json!({ "interval_secs": actual })))
}

async fn set_delete_policy(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(b): Json<DeletePolicyBody>,
) -> Result<Json<serde_json::Value>> {
    let was_enabled = s.db.get_account(id)?.delete_after_sync;
    s.db.set_delete_after_sync(id, b.delete_after_sync)?;
    let label = if b.delete_after_sync {
        "delete_after_sync"
    } else {
        "keep_on_server"
    };
    let acct = s.db.get_account(id)?;
    let _ = s.db.log_event(
        "sync_policy_changed",
        Some(&format!("{}: {}", acct.email, label)),
        None,
    );

    // false → true transition: kick off the backfill purge in the
    // background. The streaming sync only deletes UIDs in the *current*
    // batch, so flipping this flag wouldn't otherwise touch the years
    // of mail already on the server. We deliberately don't await the
    // job — years of mail can take many minutes; the UI polls
    // /purge-status for progress.
    if !was_enabled && b.delete_after_sync {
        spawn_purge(&s, &acct, crate::sync::purge::PurgeTrigger::PolicyChange).await?;
    }

    Ok(Json(
        serde_json::json!({ "id": id, "delete_after_sync": b.delete_after_sync }),
    ))
}

#[derive(serde::Deserialize)]
struct ArchiveFolderBody {
    /// New archive folder. null/empty clears the override back to default.
    archive_folder: Option<String>,
}

async fn set_archive_folder(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(b): Json<ArchiveFolderBody>,
) -> Result<Json<serde_json::Value>> {
    s.db.set_archive_folder(id, b.archive_folder.as_deref())?;
    let acct = s.db.get_account(id)?;
    let resolved = acct.archive_folder_base().to_string();
    let _ = s.db.log_event(
        "archive_folder_changed",
        Some(&format!("{}: {}", acct.email, resolved)),
        None,
    );
    Ok(Json(serde_json::json!({
        "id": id,
        "archive_folder": acct.archive_folder,
        "effective": resolved,
    })))
}

#[derive(serde::Deserialize)]
struct ArchiveEnabledBody {
    enabled: bool,
}

async fn set_archive_enabled(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(b): Json<ArchiveEnabledBody>,
) -> Result<Json<serde_json::Value>> {
    s.db.set_archive_enabled(id, b.enabled)?;
    let acct = s.db.get_account(id)?;
    let _ = s.db.log_event(
        "archive_enabled_changed",
        Some(&format!(
            "{}: {}",
            acct.email,
            if b.enabled { "on" } else { "off" }
        )),
        None,
    );
    Ok(Json(
        serde_json::json!({ "id": id, "archive_enabled": b.enabled }),
    ))
}

#[derive(serde::Deserialize)]
struct ArchiveStrategyBody {
    strategy: String,
}

#[derive(serde::Deserialize)]
struct AutoArchiveBody {
    enabled: bool,
    age_days: i32,
    read_only: bool,
}

async fn set_auto_archive(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(b): Json<AutoArchiveBody>,
) -> Result<Json<serde_json::Value>> {
    s.db.set_auto_archive(id, b.enabled, b.age_days, b.read_only)?;
    let acct = s.db.get_account(id)?;
    let detail = if acct.auto_archive_enabled {
        format!(
            "{}: on (>{}d, {})",
            acct.email,
            acct.auto_archive_age_days,
            if acct.auto_archive_read_only {
                "read only"
            } else {
                "all"
            }
        )
    } else {
        format!("{}: off", acct.email)
    };
    let _ = s.db.log_event("auto_archive_changed", Some(&detail), None);
    Ok(Json(serde_json::json!({
        "id": id,
        "auto_archive_enabled": acct.auto_archive_enabled,
        "auto_archive_age_days": acct.auto_archive_age_days,
        "auto_archive_read_only": acct.auto_archive_read_only,
    })))
}

async fn auto_archive_preview(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>> {
    let acct = s.db.get_account(id)?;
    let age = acct.auto_archive_age_days.max(1) as i64;
    let cutoff = chrono::Utc::now().timestamp() - age * 86_400;
    let count = s.db.count_auto_archive_candidates(
        acct.id,
        cutoff,
        acct.auto_archive_read_only,
        acct.archive_folder_base(),
    )?;
    Ok(Json(serde_json::json!({
        "eligible_count": count,
        "age_days": acct.auto_archive_age_days,
        "read_only": acct.auto_archive_read_only,
        "archive_base": acct.archive_folder_base(),
    })))
}

#[derive(Debug, Deserialize)]
struct PurgeGmailCategoriesBody {
    enabled: bool,
}

async fn set_purge_gmail_categories(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(b): Json<PurgeGmailCategoriesBody>,
) -> Result<Json<serde_json::Value>> {
    let acct = s.db.get_account(id)?;
    if acct.kind != crate::storage::AccountKind::Gmail {
        return Err(Error::BadRequest("category purge is Gmail-only".into()));
    }
    s.db.set_purge_gmail_categories(id, b.enabled)?;
    let detail = format!(
        "{}: category purge {}",
        acct.email,
        if b.enabled { "on" } else { "off" }
    );
    let _ =
        s.db.log_event("purge_gmail_categories_changed", Some(&detail), None);
    Ok(Json(serde_json::json!({
        "id": id,
        "purge_gmail_categories": b.enabled,
    })))
}

async fn set_skip_gmail_trash(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(b): Json<PurgeGmailCategoriesBody>,
) -> Result<Json<serde_json::Value>> {
    let acct = s.db.get_account(id)?;
    if acct.kind != crate::storage::AccountKind::Gmail {
        return Err(Error::BadRequest("skip-trash is Gmail-only".into()));
    }
    s.db.set_skip_gmail_trash(id, b.enabled)?;
    let detail = format!(
        "{}: skip-gmail-trash {}",
        acct.email,
        if b.enabled { "on" } else { "off" }
    );
    let _ =
        s.db.log_event("skip_gmail_trash_changed", Some(&detail), None);
    Ok(Json(serde_json::json!({
        "id": id,
        "skip_gmail_trash": b.enabled,
    })))
}

#[derive(Debug, Deserialize)]
struct RetentionBody {
    enabled: bool,
    days: i32,
}

async fn set_retention(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(b): Json<RetentionBody>,
) -> Result<Json<serde_json::Value>> {
    s.db.set_retention(id, b.enabled, b.days)?;
    let acct = s.db.get_account(id)?;
    let detail = if acct.retention_enabled {
        format!(
            "{}: on (>{}d, server-side delete)",
            acct.email, acct.retention_days
        )
    } else {
        format!("{}: off", acct.email)
    };
    let _ = s.db.log_event("retention_changed", Some(&detail), None);
    Ok(Json(serde_json::json!({
        "id": id,
        "retention_enabled": acct.retention_enabled,
        "retention_days": acct.retention_days,
    })))
}

async fn retention_preview(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>> {
    let acct = s.db.get_account(id)?;
    let days = acct.retention_days.max(1) as i64;
    let cutoff = chrono::Utc::now().timestamp() - days * 86_400;
    let count = s.db.count_retention_candidates(acct.id, cutoff)?;
    Ok(Json(serde_json::json!({
        "eligible_count": count,
        "days": acct.retention_days,
    })))
}

/// Walk `[Gmail]/All Mail` and paint hidden labels (categories, user
/// labels) onto every message Postern already has locally. No bodies
/// are re-downloaded — this is the cheap backfill path for accounts
/// that synced before the X-GM-LABELS pass existed, or whenever the
/// user wants to reconcile against Gmail's current label state.
///
/// Gmail-only. Non-Gmail accounts get a 400.
async fn rescan_gmail_labels(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>> {
    s.vault.require_unlocked()?;
    let account = s.db.get_account(id)?;
    if account.kind != crate::storage::AccountKind::Gmail {
        return Err(Error::BadRequest(
            "label rescan is Gmail-only — plain IMAP accounts don't have hidden labels".into(),
        ));
    }
    let password = s.db.account_password(account.id, &s.vault)?;
    let iface = s.vpn.bind_iface();
    let db = s.db.clone();
    let email = account.email.clone();

    let (scanned, updated) = tokio::task::spawn_blocking(move || -> Result<(u32, u32)> {
        crate::sync::gmail_rescan::rescan(
            &account.imap_host,
            account.imap_port,
            &account.email,
            &password,
            &account,
            &db,
            iface.as_deref(),
        )
    })
    .await
    .map_err(|e| Error::Other(anyhow::anyhow!("join: {e}")))??;

    let detail = format!("{email}: scanned {scanned}, updated {updated} messages");
    let _ = s.db.log_activity("gmail_labels_rescanned", Some(&detail));
    Ok(Json(serde_json::json!({
        "scanned": scanned,
        "updated": updated,
    })))
}

#[derive(serde::Deserialize)]
struct AvatarBody {
    /// null/empty clears the override so the account falls back to its email.
    seed: Option<String>,
    set: String,
}

async fn set_avatar(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(b): Json<AvatarBody>,
) -> Result<Json<serde_json::Value>> {
    s.db.set_avatar(id, b.seed.as_deref(), &b.set)?;
    let acct = s.db.get_account(id)?;
    let _ = s.db.log_event(
        "avatar_changed",
        Some(&format!("{}: {}", acct.email, acct.avatar_set)),
        None,
    );
    Ok(Json(serde_json::json!({
        "id": id,
        "avatar_seed": acct.avatar_seed,
        "avatar_set": acct.avatar_set,
    })))
}

#[derive(Deserialize)]
struct SignatureBody {
    html: Option<String>,
    plain: Option<String>,
}

async fn set_signature(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(b): Json<SignatureBody>,
) -> Result<Json<serde_json::Value>> {
    s.db.set_signature(id, b.html.as_deref(), b.plain.as_deref())?;
    let acct = s.db.get_account(id)?;
    Ok(Json(serde_json::json!({
        "id": id,
        "signature_html": acct.signature_html,
        "signature_plain": acct.signature_plain,
    })))
}

async fn set_archive_strategy(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(b): Json<ArchiveStrategyBody>,
) -> Result<Json<serde_json::Value>> {
    let strategy = crate::storage::ArchiveStrategy::parse(&b.strategy)?;
    s.db.set_archive_strategy(id, strategy)?;
    let acct = s.db.get_account(id)?;
    let _ = s.db.log_event(
        "archive_strategy_changed",
        Some(&format!("{}: {}", acct.email, strategy.as_str())),
        None,
    );
    // Preview what a message archived *right now* would land in.
    let preview = acct.archive_folder_for(chrono::Utc::now().timestamp());
    Ok(Json(serde_json::json!({
        "id": id,
        "strategy": strategy.as_str(),
        "preview": preview,
    })))
}

// ---------------------------------------------------------------------
// Server-purge backfill — see crate::sync::purge.
//
// Two entry points:
//   1. Toggle of `delete_after_sync` from false→true → auto-spawned
//      from `set_delete_policy`. Mode = Execute, trigger = PolicyChange.
//   2. Manual `POST /accounts/:id/purge-server-copies` — body chooses
//      precheck vs execute. Trigger = Manual.
// Both write through the same shared `PurgeJobs` registry so the UI
// polls `GET /accounts/:id/purge-status` for progress regardless of
// origin.
// ---------------------------------------------------------------------

#[derive(serde::Deserialize, Default)]
struct PurgeBody {
    /// When true, walk the server and report counts but never delete
    /// anything. Defaults to false (Execute).
    #[serde(default)]
    precheck_only: bool,
}

async fn start_purge_server_copies(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    body: Option<Json<PurgeBody>>,
) -> Result<Json<serde_json::Value>> {
    s.vault.require_unlocked()?;
    let account = s.db.get_account(id)?;
    let precheck = body.map(|Json(b)| b.precheck_only).unwrap_or(false);
    let mode = if precheck {
        crate::sync::purge::PurgeMode::Precheck
    } else {
        crate::sync::purge::PurgeMode::Execute
    };

    if s.purge_jobs.is_running(account.id) {
        return Err(Error::BadRequest(
            "a purge job is already running for this account".into(),
        ));
    }

    spawn_purge_with_mode(&s, &account, mode, crate::sync::purge::PurgeTrigger::Manual).await?;
    Ok(Json(
        serde_json::json!({ "id": id, "started": true, "mode": mode }),
    ))
}

async fn purge_status(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>> {
    s.db.get_account(id)?;
    let report = s.purge_jobs.get(id);
    Ok(Json(serde_json::json!({ "id": id, "report": report })))
}

/// Auto-trigger from policy change. Always Execute mode.
async fn spawn_purge(
    s: &AppState,
    account: &Account,
    trigger: crate::sync::purge::PurgeTrigger,
) -> Result<()> {
    if s.purge_jobs.is_running(account.id) {
        // Caller flipped the toggle while a previous purge was still
        // in flight — don't double up. The running job will finish on
        // the same DB state.
        return Ok(());
    }
    spawn_purge_with_mode(s, account, crate::sync::purge::PurgeMode::Execute, trigger).await
}

async fn spawn_purge_with_mode(
    s: &AppState,
    account: &Account,
    mode: crate::sync::purge::PurgeMode,
    trigger: crate::sync::purge::PurgeTrigger,
) -> Result<()> {
    let password = s.db.account_password(account.id, &s.vault)?;
    let bind_iface = s.vpn.bind_iface();
    // Seed a Running placeholder so /purge-status returns "started"
    // even before the spawn_blocking thread has done its first FETCH.
    let initial = {
        let mut r = crate::sync::purge::PurgeReport::new_initial(account.id, mode, trigger);
        r.started_at = chrono::Utc::now().timestamp();
        r
    };
    s.purge_jobs.set(initial);

    let db = s.db.clone();
    let jobs = s.purge_jobs.clone();
    let acct_clone = account.clone();
    let email = account.email.clone();
    tokio::task::spawn_blocking(move || {
        let report = match crate::sync::purge::purge_synced_server_copies(
            &acct_clone,
            &password,
            &db,
            bind_iface.as_deref(),
            mode,
            trigger,
        ) {
            Ok(r) => r,
            Err(e) => {
                tracing::error!(error = %e, email = %email, "purge: catastrophic failure");
                let mut r =
                    crate::sync::purge::PurgeReport::new_initial(acct_clone.id, mode, trigger);
                r.errors.push(e.to_string());
                r.finish_failed();
                r
            }
        };
        let detail = format!(
            "{}: scanned={} verified={} skipped={} moved={} expunged={} ({} errors)",
            email,
            report.scanned,
            report.verified_safe,
            report.skipped_no_local_copy,
            report.moved_or_deleted,
            report.expunged_from_trash,
            report.errors.len()
        );
        tracing::info!(target: "purge", "{detail}");
        let _ = db.log_activity("server_purge", Some(&detail));
        // Hop back into the async runtime to update the registry.
        jobs.set(report);
    });
    Ok(())
}
