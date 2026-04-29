use std::{net::SocketAddr, sync::Arc};

use anyhow::Context;
use clap::Parser;
use tokio::net::TcpListener;
use tracing::info;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

mod backup;
mod backup_destinations;
mod backup_orchestrator;
mod backup_scheduler;
mod caldav;
mod gdrive;
mod config;
mod error;
mod http;
mod llm;
mod net;
mod outbox;
mod pgp;
mod privacy;
mod restore;
mod rules;
mod send;
mod smtp;
mod storage;
mod sync;
mod tier;
mod vault;
mod vpn;

use config::Config;
use privacy::ImageProxy;
use storage::{BlobStore, Db};
use sync::Scheduler;
use vpn::VpnManager;

#[derive(Parser, Debug)]
#[command(name = "postern", version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(clap::Subcommand, Debug)]
enum Command {
    /// Run the server (default).
    Serve,
    /// Run pending database migrations and exit.
    Migrate,
    /// Print the resolved configuration and exit.
    PrintConfig,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();
    let cli = Cli::parse();
    let config = Config::from_env().context("load config")?;

    match cli.command.unwrap_or(Command::Serve) {
        Command::PrintConfig => {
            println!("{config:#?}");
            Ok(())
        }
        Command::Migrate => {
            let db = Db::open(&config.db_path)?;
            db.migrate()?;
            info!("migrations applied");
            Ok(())
        }
        Command::Serve => serve(config).await,
    }
}

async fn serve(config: Config) -> anyhow::Result<()> {
    // BEFORE we open the DB pool: if a previous run wrote a
    // `.restore-on-boot` marker, swap the data dir contents with the
    // staged backup. Done here because the DB pool will hold open
    // file handles for the lifetime of the process — once it's open,
    // we can't safely overwrite the file.
    if let Err(e) = restore::consume_pending_restore(&config.data_dir) {
        tracing::error!(error = %e, "restore: boot-time consume failed");
        return Err(anyhow::anyhow!("restore failed at boot: {e}"));
    }

    let db = Arc::new(Db::open(&config.db_path).context("open db")?);
    // Run migrations only on plain DBs at startup. An encrypted DB
    // can't be queried until the vault hands us a key — migrations
    // re-run inside `Vault::apply_db_encryption` after rekey.
    if db.is_plain_sqlite()? {
        db.migrate().context("migrate")?;
    }

    let blobs = Arc::new(BlobStore::new(&config.blob_dir).context("blob store")?);

    // Background backfills for rows that pre-date later migrations.
    // Both run in a blocking task so HTTP comes up immediately on large
    // mailboxes.
    {
        let db = db.clone();
        let blobs = blobs.clone();
        tokio::task::spawn_blocking(move || {
            run_body_backfill(&db, &blobs);
            run_thread_backfill(&db, &blobs);
            run_subject_key_backfill(&db, &blobs);
        });
    }

    let proxy = ImageProxy::new();
    let vpn = VpnManager::new(db.clone(), proxy.clone());
    let mut vault = vault::Vault::new(db.clone(), config.data_dir.clone());
    vault.set_vpn(vpn.clone());
    vault.set_blob_store(blobs.clone());
    let (scheduler, _scheduler_handle) =
        Scheduler::start(db.clone(), blobs.clone(), vpn.clone(), vault.clone());

    // Drain the outbox: undo-send windows, scheduled sends, and any
    // 'sending' rows stranded by a previous process restart.
    let _outbox_handle = outbox::spawn(db.clone(), vpn.clone(), vault.clone());

    // AI provider holder — both chat + embed providers live behind
    // an async lock so Settings → AI can swap them at runtime. We
    // seed from the persisted settings row + the encrypted API key
    // (if any). Every step is best-effort: a misconfigured provider
    // boots an empty holder, AI surfaces grey out in the UI, and
    // the user fixes it from Settings without a restart.
    //
    // Pass vpn so cloud providers (Anthropic/OpenAI/Grok) bind their
    // outbound TCP to wg0 when the kill-switch is up. Without that,
    // the firewall REJECTs the call before it leaves.
    let llm = init_llm_holder(&db, &vault, &vpn).await;

    let state = http::AppState::new(
        db.clone(),
        blobs.clone(),
        scheduler,
        proxy,
        vpn.clone(),
        vault.clone(),
        llm.clone(),
    );

    // AI indexer — walks new messages once a minute and embeds
    // them. Always spawn; the indexer reads the embed provider
    // from the holder each tick, so a runtime provider swap is
    // observed automatically.
    let _indexer_handle = Some(llm::indexer::spawn_with_holder(
        db.clone(),
        vault.clone(),
        llm.clone(),
    ));
    // Backup tick task — wakes every 60s, fires when schedule says so.
    // Uses the same BackupJobs registry the manual button uses, so the
    // UI sees scheduled runs identically to manual ones.
    let _backup_scheduler_handle = backup_scheduler::spawn(
        db.clone(),
        vault.clone(),
        state.backup_jobs.clone(),
        config.data_dir.clone(),
        std::env::var("POSTERN_BACKUP_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| std::path::PathBuf::from("/var/lib/postern/backups")),
        vpn.clone(),
    );
    let app = http::router(state, config.static_dir.clone());
    let addr: SocketAddr = config.bind.parse().context("parse bind addr")?;
    let listener = TcpListener::bind(addr).await.context("bind tcp")?;
    info!(?addr, static_dir = ?config.static_dir, "postern listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("axum serve")?;

    Ok(())
}

fn init_tracing() {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,postern=debug"));
    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer())
        .init();
}

/// Boot-time AI seeding. Builds the chat + embed providers from the
/// persisted `ai_settings` row (with API key decrypted via the vault
/// when needed) and runs a best-effort `health()` on each. Returns a
/// `LlmHolder` whose providers are live or `None` per the probe
/// outcome — Settings → AI can swap them at runtime, no restart
/// required.
///
/// Tier-gated: `tier::ALLOW_AI = false` short-circuits to an empty
/// holder so the community build never opens an outbound API socket.
async fn init_llm_holder(
    db: &std::sync::Arc<Db>,
    vault: &crate::vault::Vault,
    vpn: &crate::vpn::VpnManager,
) -> llm::LlmHolder {
    if !tier::ALLOW_AI {
        return llm::LlmHolder::default();
    }
    // Settings row + key. If the vault is locked at boot the key
    // can't be decrypted — leave the holder empty for now; the user
    // unlocks the vault, then re-saves Settings → AI to re-seed the
    // providers. Same dance you already do for IMAP credentials.
    let settings = match db.get_ai_settings() {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(error = %e, "ai: get_ai_settings failed at boot");
            return llm::LlmHolder::default();
        }
    };
    let api_key = match db.ai_api_key(vault) {
        Ok(k) => k,
        Err(e) => {
            tracing::info!(error = %e, "ai: api key unavailable at boot (vault may be locked)");
            None
        }
    };
    let bind_iface = vpn.bind_iface();
    let chat = match llm::build_chat_provider(&settings, api_key.as_deref(), bind_iface.as_deref()) {
        Ok(c) => probe_or_drop(c, "chat").await,
        Err(e) => {
            tracing::warn!(error = %e, "ai: chat provider build failed");
            None
        }
    };
    let embed_api_key = match db.ai_embed_api_key(vault) {
        Ok(k) => k,
        Err(e) => {
            tracing::info!(error = %e, "ai: embed api key unavailable at boot");
            None
        }
    };
    let embed = match llm::build_embed_provider(&settings, api_key.as_deref(), embed_api_key.as_deref(), bind_iface.as_deref()) {
        Ok(c) => probe_or_drop(c, "embed").await,
        Err(e) => {
            tracing::warn!(error = %e, "ai: embed provider build failed");
            None
        }
    };
    // Wrap with the activity-logging decorator so every chat /
    // embed call writes a row to ai_activity_log. Health probes
    // pass through unlogged (too spammy).
    let chat = chat.map(|p| llm::ActivityLoggedProvider::wrap(p, db.clone()));
    let embed = embed.map(|p| llm::ActivityLoggedProvider::wrap(p, db.clone()));
    llm::LlmHolder::new(chat, embed)
}

/// Run a non-fatal `health()` probe against a candidate provider —
/// drop it if the probe errors so AI surfaces grey out cleanly
/// instead of 500-ing on every click.
async fn probe_or_drop(
    candidate: Option<std::sync::Arc<dyn llm::LlmProvider>>,
    role: &str,
) -> Option<std::sync::Arc<dyn llm::LlmProvider>> {
    let p = candidate?;
    match p.health().await {
        Ok(()) => {
            tracing::info!(role, provider = p.id(), "ai: provider healthy");
            Some(p)
        }
        Err(e) => {
            tracing::info!(role, provider = p.id(), error = %e, "ai: provider failed health probe");
            None
        }
    }
}

/// Backfill body_text for rows that pre-date migration 0002. Bounded work —
/// processes in small batches so we never hold the pool for long.
fn run_body_backfill(db: &Db, blobs: &BlobStore) {
    let mut total = 0usize;
    loop {
        let res = db.backfill_bodies(100, |hash| {
            let raw = blobs.get(hash).ok()?;
            sync::body_text_of(&raw)
        });
        match res {
            Ok(0) => break,
            Ok(n) => {
                total += n;
                tracing::info!(batch = n, total, "body_text backfill batch");
            }
            Err(e) => {
                tracing::warn!(error = %e, "body_text backfill failed; will retry next startup");
                return;
            }
        }
    }
    if total > 0 {
        tracing::info!(total, "body_text backfill complete");
    }
}

/// Fill in `subject_key` for rows that pre-date migration 0018.
/// This is what enables the "Re: without References" merge: once
/// every message carries a normalized subject, newly-arriving orphan
/// replies can look up their parent thread by exact key match.
fn run_subject_key_backfill(db: &Db, blobs: &BlobStore) {
    let mut total = 0usize;
    loop {
        let res = db.backfill_subject_keys(200, |hash| {
            let raw = blobs.get(hash).ok()?;
            sync::subject_key_of(&raw)
        });
        match res {
            Ok(0) => break,
            Ok(n) => {
                total += n;
                tracing::info!(batch = n, total, "subject_key backfill batch");
            }
            Err(e) => {
                tracing::warn!(error = %e, "subject_key backfill failed; will retry next startup");
                return;
            }
        }
    }
    if total > 0 {
        tracing::info!(total, "subject_key backfill complete");
    }
}

/// Rewrite per-UID `thread_id`s from the Sprint 1 era into JWZ roots.
/// Bounded-batch, stops when there's nothing left to convert.
fn run_thread_backfill(db: &Db, blobs: &BlobStore) {
    let mut total = 0usize;
    loop {
        let res = db.backfill_threads(200, |hash| {
            let raw = blobs.get(hash).ok()?;
            Some(sync::thread_id_of(&raw))
        });
        match res {
            Ok(0) => break,
            Ok(n) => {
                total += n;
                tracing::info!(batch = n, total, "thread_id backfill batch");
            }
            Err(e) => {
                tracing::warn!(error = %e, "thread_id backfill failed; will retry next startup");
                return;
            }
        }
    }
    if total > 0 {
        tracing::info!(total, "thread_id backfill complete");
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
    info!("shutdown signal received");
}
