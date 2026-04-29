use std::collections::HashSet;

use axum::{
    extract::{Path, Query, State},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use super::AppState;
use crate::{
    error::{Error, Result},
    storage::{Account, FolderCount},
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/folders", get(list))
        .route("/accounts/:id/folders", post(create).delete(delete))
        .route("/accounts/:id/folders/rename", post(rename))
}

#[derive(Debug, Serialize)]
struct FoldersResponse {
    accounts: Vec<AccountFolders>,
}

#[derive(Debug, Serialize)]
struct AccountFolders {
    account_id: i64,
    email: String,
    kind: String,
    /// User-chosen RoboHash seed override. Null falls back to `email`.
    avatar_seed: Option<String>,
    /// RoboHash collection — set1..set5.
    avatar_set: String,
    /// Curated system folders (Inbox, Drafts, etc.) in a fixed display order.
    system: Vec<FolderEntry>,
    /// Gmail `[Gmail]/Categories/*` folders the user has exposed via IMAP.
    categories: Vec<FolderEntry>,
    /// User-created labels.
    user: Vec<FolderEntry>,
    /// Categories Gmail is hiding from IMAP — the UI nudges the user to
    /// enable them in Gmail settings. Empty for non-Gmail accounts.
    categories_missing: Vec<&'static str>,
    /// Mirrors the Account field — lets the frontend apply the same
    /// "hide me from the unified view" filter at display time (counts
    /// + aggregate rows).
    include_in_unified: bool,
}

#[derive(Debug, Serialize)]
struct FolderEntry {
    /// Raw IMAP folder name used for filtering — e.g. "INBOX", "[Gmail]/All Mail".
    name: String,
    /// User-friendly display name — e.g. "Inbox", "All Mail".
    display: String,
    /// `system` | `user` | `gmail_category`.
    kind: String,
    total: i64,
    unread: i64,
    /// Sum of message blob sizes in the folder, bytes. Surfaced in the
    /// sidebar tooltip so the operator can see folder occupancy at a
    /// glance — same place Evolution puts it (folder properties).
    size_bytes: i64,
    /// Sort weight for client-side rendering. Lower = higher in the list.
    weight: i32,
}

const SYSTEM_ORDER: &[(&str, &str, i32)] = &[
    ("INBOX", "Inbox", 0),
    ("[Gmail]/Important", "Important", 10),
    ("[Gmail]/Starred", "Starred", 20),
    ("[Gmail]/Sent Mail", "Sent", 30),
    ("[Gmail]/Drafts", "Drafts", 40),
    ("[Gmail]/All Mail", "All Mail", 50),
    ("[Gmail]/Spam", "Spam", 60),
    ("[Gmail]/Trash", "Trash", 70),
    ("Sent", "Sent", 30),
    ("Drafts", "Drafts", 40),
    ("Archive", "Archive", 50),
    ("Spam", "Spam", 60),
    ("Junk", "Spam", 60),
    ("Trash", "Trash", 70),
    ("Deleted Items", "Trash", 70),
];

/// Folder names the UI must never let a user rename or delete — these
/// are server-managed or semantically critical to the mail protocol.
const SYSTEM_PROTECTED: &[&str] = &[
    "INBOX",
    "Sent",
    "Drafts",
    "Archive",
    "Spam",
    "Junk",
    "Trash",
    "Deleted Items",
];

/// Gmail's five standard categories — we check whether each one has a
/// matching IMAP folder and surface the missing ones to the UI.
const GMAIL_CATEGORIES: &[&str] = &[
    "CATEGORY_PERSONAL",
    "CATEGORY_SOCIAL",
    "CATEGORY_UPDATES",
    "CATEGORY_FORUMS",
    "CATEGORY_PROMOTIONS",
];

async fn list(State(s): State<AppState>) -> Result<Json<FoldersResponse>> {
    let accounts = s.db.list_accounts()?;
    let mut out = Vec::with_capacity(accounts.len());
    for a in accounts {
        out.push(folders_for(&s, &a)?);
    }
    Ok(Json(FoldersResponse { accounts: out }))
}

fn folders_for(s: &AppState, account: &Account) -> Result<AccountFolders> {
    let counts = s.db.folder_counts(account.id)?;

    let mut system = Vec::new();
    let mut categories = Vec::new();
    let mut user = Vec::new();

    let present_names: HashSet<&str> = counts.iter().map(|c| c.name.as_str()).collect();

    for c in &counts {
        if let Some((_, display, weight)) = SYSTEM_ORDER.iter().find(|(n, _, _)| *n == c.name) {
            system.push(entry(c, display, "system", *weight));
        } else if looks_like_category(&c.name) {
            let display = display_for_category(&c.name).to_owned();
            categories.push(entry_owned(c, display, "gmail_category", 100));
        } else if c.kind != "system" {
            // Everything else is a user label.
            user.push(entry_owned(c, c.name.clone(), "user", 200));
        }
    }

    system.sort_by_key(|e| (e.weight, e.display.clone()));
    categories.sort_by_key(|e| e.display.clone());
    user.sort_by_key(|e| e.display.clone());

    let categories_missing = if matches!(account.kind, crate::storage::AccountKind::Gmail) {
        GMAIL_CATEGORIES
            .iter()
            .copied()
            .filter(|cat| {
                !present_names
                    .iter()
                    .any(|n| n.contains(cat) || n.ends_with(display_for_category(cat)))
            })
            .collect()
    } else {
        vec![]
    };

    Ok(AccountFolders {
        account_id: account.id,
        email: account.email.clone(),
        kind: match account.kind {
            crate::storage::AccountKind::Gmail => "gmail".into(),
            crate::storage::AccountKind::Imap => "imap".into(),
        },
        avatar_seed: account.avatar_seed.clone(),
        avatar_set: account.avatar_set.clone(),
        system,
        categories,
        user,
        categories_missing,
        include_in_unified: account.include_in_unified,
    })
}

fn entry(c: &FolderCount, display: &str, kind: &str, weight: i32) -> FolderEntry {
    FolderEntry {
        name: c.name.clone(),
        display: display.to_owned(),
        kind: kind.to_owned(),
        total: c.total,
        unread: c.unread,
        size_bytes: c.size_bytes,
        weight,
    }
}

fn entry_owned(c: &FolderCount, display: String, kind: &str, weight: i32) -> FolderEntry {
    FolderEntry {
        name: c.name.clone(),
        display,
        kind: kind.to_owned(),
        total: c.total,
        unread: c.unread,
        size_bytes: c.size_bytes,
        weight,
    }
}

fn looks_like_category(name: &str) -> bool {
    name.starts_with("CATEGORY_")
        || name.starts_with("[Gmail]/Categories/")
        || name.starts_with("Categories/")
}

fn display_for_category(raw: &str) -> &str {
    match raw {
        "CATEGORY_PERSONAL" => "Personal",
        "CATEGORY_SOCIAL" => "Social",
        "CATEGORY_UPDATES" => "Updates",
        "CATEGORY_FORUMS" => "Forums",
        "CATEGORY_PROMOTIONS" => "Promotions",
        s if s.starts_with("[Gmail]/Categories/") => s.trim_start_matches("[Gmail]/Categories/"),
        s if s.starts_with("Categories/") => s.trim_start_matches("Categories/"),
        s => s,
    }
}

// ============================================================
// Mutation endpoints
// ============================================================

fn normalize(name: &str) -> Result<String> {
    let trimmed = name.trim().trim_matches('/');
    if trimmed.is_empty() {
        return Err(Error::BadRequest("folder name is required".into()));
    }
    if trimmed.split('/').any(|s| s.trim().is_empty()) {
        return Err(Error::BadRequest(
            "folder name cannot have empty path segments".into(),
        ));
    }
    Ok(trimmed.to_owned())
}

fn guard_system_folder(name: &str) -> Result<()> {
    if name.starts_with("[Gmail]/") {
        return Err(Error::BadRequest(
            "cannot modify Gmail system folders (they're server-managed)".into(),
        ));
    }
    if SYSTEM_PROTECTED.contains(&name) {
        return Err(Error::BadRequest(format!(
            "'{name}' is a system folder and cannot be modified"
        )));
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
struct CreateBody {
    name: String,
}

async fn create(
    State(s): State<AppState>,
    Path(account_id): Path<i64>,
    Json(body): Json<CreateBody>,
) -> Result<Json<serde_json::Value>> {
    s.vault.require_unlocked()?;
    let name = normalize(&body.name)?;
    if name.starts_with("[Gmail]/") {
        return Err(Error::BadRequest(
            "cannot create folders under [Gmail]/ (that namespace is server-managed)".into(),
        ));
    }
    let account = s.db.get_account(account_id)?;
    let password = s.db.account_password(account.id, &s.vault)?;
    let bind_iface = bind_iface(&s);

    let name_cloned = name.clone();
    let acct_email = account.email.clone();
    tokio::task::spawn_blocking(move || -> Result<()> {
        let mut client = crate::sync::ImapClient::connect(
            &account.imap_host,
            account.imap_port,
            &account.email,
            &password,
            bind_iface.as_deref(),
        )?;
        client.ensure_folder(&name_cloned)?;
        client.logout();
        Ok(())
    })
    .await
    .map_err(|e| Error::Other(anyhow::anyhow!("join: {e}")))??;

    // Register the label locally so the sidebar shows the new folder
    // immediately — otherwise it'd only appear on the next sync pass.
    s.db.upsert_label(account_id, &name, "user")?;
    let _ = s.db.log_event(
        "folder_created",
        Some(&format!("{acct_email}: {name}")),
        None,
    );

    Ok(Json(
        serde_json::json!({ "account_id": account_id, "name": name }),
    ))
}

#[derive(Debug, Deserialize)]
struct DeleteQuery {
    name: String,
    /// When true, skip the emptiness check and delete anyway. UI wraps
    /// this with a scary confirm dialog.
    #[serde(default)]
    force: bool,
}

async fn delete(
    State(s): State<AppState>,
    Path(account_id): Path<i64>,
    Query(q): Query<DeleteQuery>,
) -> Result<Json<serde_json::Value>> {
    s.vault.require_unlocked()?;
    let name = normalize(&q.name)?;
    guard_system_folder(&name)?;

    let account = s.db.get_account(account_id)?;

    // Block deletion of the configured archive base — renaming it away
    // first forces the user to pick a new bucket before destroying the
    // old one.
    if name == account.archive_folder_base() {
        return Err(Error::BadRequest(
            "this is your configured archive folder — change the archive base in Settings → Archive first".into(),
        ));
    }

    if !q.force {
        let count = s.db.count_messages_with_label(account_id, &name)?;
        if count > 0 {
            return Err(Error::BadRequest(format!(
                "folder has {count} messages — empty it first, or pass force=true to delete anyway"
            )));
        }
    }

    let password = s.db.account_password(account.id, &s.vault)?;
    let bind_iface = bind_iface(&s);
    let name_cloned = name.clone();
    let acct_email = account.email.clone();
    tokio::task::spawn_blocking(move || -> Result<()> {
        let mut client = crate::sync::ImapClient::connect(
            &account.imap_host,
            account.imap_port,
            &account.email,
            &password,
            bind_iface.as_deref(),
        )?;
        client.delete_folder(&name_cloned)?;
        client.logout();
        Ok(())
    })
    .await
    .map_err(|e| Error::Other(anyhow::anyhow!("join: {e}")))??;

    let dropped = s.db.delete_label_tree(account_id, &name)?;
    let _ = s.db.log_event(
        "folder_deleted",
        Some(&format!("{acct_email}: {name} ({dropped} labels)")),
        None,
    );

    Ok(Json(serde_json::json!({
        "account_id": account_id,
        "name": name,
        "labels_removed": dropped,
    })))
}

#[derive(Debug, Deserialize)]
struct RenameBody {
    from: String,
    to: String,
}

async fn rename(
    State(s): State<AppState>,
    Path(account_id): Path<i64>,
    Json(body): Json<RenameBody>,
) -> Result<Json<serde_json::Value>> {
    s.vault.require_unlocked()?;
    let from = normalize(&body.from)?;
    let to = normalize(&body.to)?;
    if from == to {
        return Ok(Json(serde_json::json!({ "noop": true, "name": from })));
    }
    guard_system_folder(&from)?;
    if to.starts_with("[Gmail]/") {
        return Err(Error::BadRequest(
            "cannot move a folder into the [Gmail]/ namespace".into(),
        ));
    }

    let account = s.db.get_account(account_id)?;
    if from == account.archive_folder_base() {
        return Err(Error::BadRequest(
            "renaming the configured archive folder would orphan your archive — update the base in Settings → Archive first".into(),
        ));
    }

    if s.db.label_exists(account_id, &to)? {
        return Err(Error::BadRequest(format!(
            "a folder named '{to}' already exists"
        )));
    }

    let password = s.db.account_password(account.id, &s.vault)?;
    let bind_iface = bind_iface(&s);
    let from_c = from.clone();
    let to_c = to.clone();
    let acct_email = account.email.clone();

    tokio::task::spawn_blocking(move || -> Result<()> {
        let mut client = crate::sync::ImapClient::connect(
            &account.imap_host,
            account.imap_port,
            &account.email,
            &password,
            bind_iface.as_deref(),
        )?;
        // If the new path has parents that don't exist yet, the RENAME
        // command is supposed to create them — but not every server
        // does. Belt-and-braces: ensure the parent path first.
        if let Some(parent) = to_c.rsplit_once('/').map(|(p, _)| p) {
            if !parent.is_empty() {
                client.ensure_folder(parent)?;
            }
        }
        client.rename_folder(&from_c, &to_c)?;
        client.logout();
        Ok(())
    })
    .await
    .map_err(|e| Error::Other(anyhow::anyhow!("join: {e}")))??;

    let renamed = s.db.rename_label_tree(account_id, &from, &to)?;
    let _ = s.db.log_event(
        "folder_renamed",
        Some(&format!("{acct_email}: {from} -> {to} ({renamed} labels)")),
        None,
    );

    Ok(Json(serde_json::json!({
        "account_id": account_id,
        "from": from,
        "to": to,
        "labels_renamed": renamed,
    })))
}

fn bind_iface(s: &AppState) -> Option<String> {
    if s.vpn.status().interface_up {
        Some("wg0".to_owned())
    } else {
        None
    }
}
