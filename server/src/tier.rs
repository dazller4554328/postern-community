//! Tier-aware constants.
//!
//! Compile-time feature flag (`pro`) selects between the paid and
//! community builds. Every caps + feature-gate check routes through
//! this module so there's exactly one place to audit when the
//! free/paid split changes.
//!
//! Design: the build is a single binary, not a runtime-licensed one.
//! That means users who want to check "am I running the free build?"
//! don't need to inspect a license — they inspect the binary (and
//! the `/api/tier` endpoint surfaces the same answer to the UI).
//!
//! See also: `docs`… there aren't any yet. For the decision record,
//! see `memory/project_free_edition.md`.

use serde::Serialize;

/// Identifier for the current build. Surfaced over `/api/tier` so the
/// web UI can hide paid-only tabs.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    Pro,
    Community,
}

/// Which tier this binary was compiled as. `const` so the compiler
/// can fold `if tier::CURRENT == Tier::Pro` branches away in release
/// builds, keeping the free binary lean.
#[cfg(feature = "pro")]
pub const CURRENT: Tier = Tier::Pro;
#[cfg(not(feature = "pro"))]
pub const CURRENT: Tier = Tier::Community;

/// Hard cap on mailbox/account count. `None` = unlimited.
#[cfg(feature = "pro")]
pub const MAX_MAILBOXES: Option<usize> = None;
#[cfg(not(feature = "pro"))]
pub const MAX_MAILBOXES: Option<usize> = Some(3);

/// Maximum distance in the future `scheduled_at` may be from `now()`
/// at enqueue time. The undo-send window is the only legitimate use
/// of `scheduled_at` in the free build; paid gets unrestricted
/// send-later scheduling.
#[cfg(feature = "pro")]
pub const MAX_SEND_DELAY_SECS: Option<i64> = None;
#[cfg(not(feature = "pro"))]
pub const MAX_SEND_DELAY_SECS: Option<i64> = Some(60);

/// Whether the account's "purge Gmail categories" toggle is allowed.
/// Free users get the core IMAP sync + delete-after-sync but not the
/// aggressive category rescanner.
#[cfg(feature = "pro")]
pub const ALLOW_GMAIL_CATEGORIES_PURGE: bool = true;
#[cfg(not(feature = "pro"))]
pub const ALLOW_GMAIL_CATEGORIES_PURGE: bool = false;

/// Whether server-side retention sweeps are allowed.
#[cfg(feature = "pro")]
pub const ALLOW_SERVER_RETENTION: bool = true;
#[cfg(not(feature = "pro"))]
pub const ALLOW_SERVER_RETENTION: bool = false;

/// Whether auto-archive is allowed.
#[cfg(feature = "pro")]
pub const ALLOW_AUTO_ARCHIVE: bool = true;
#[cfg(not(feature = "pro"))]
pub const ALLOW_AUTO_ARCHIVE: bool = false;

/// Whether the mail-import endpoints are exposed. Both tiers get
/// this — import is a one-shot migration aid, and gating it behind
/// a paywall right when a user is deciding to commit to the product
/// is bad UX. Lives here (instead of as a hardcoded `true`) so the
/// decision is audit-traceable from tier.rs like the other flags.
pub const ALLOW_MAIL_IMPORT: bool = true;

/// Master switch for AI features (Ask Your Inbox, summarisation,
/// semantic search, etc.). Pro-only initially.
///
/// Flipping to `false` produces a build with zero AI code paths
/// active — the HTTP routes don't register, the embedding indexer
/// doesn't run, the inbox UI hides the AskBox button. Useful as a
/// kill-switch if a model misbehaves or to ship a clean
/// no-AI build for a privacy-paranoid deployment.
///
/// The community build defaults this off because the feature
/// requires a separately-installed Ollama (or BYO API key) and we
/// don't want to advertise an unconfigured surface.
#[cfg(feature = "pro")]
pub const ALLOW_AI: bool = true;
#[cfg(not(feature = "pro"))]
pub const ALLOW_AI: bool = false;

/// Payload shape for `/api/tier`. Everything the web UI needs to
/// decide which panels/tabs to show lives here.
#[derive(Debug, Clone, Serialize)]
pub struct TierInfo {
    pub tier: Tier,
    pub max_mailboxes: Option<usize>,
    pub max_send_delay_secs: Option<i64>,
    pub features: TierFeatures,
}

#[derive(Debug, Clone, Serialize)]
pub struct TierFeatures {
    pub vpn: bool,
    pub trusted_devices: bool,
    pub licensed_updates: bool,
    pub gmail_categories_purge: bool,
    pub server_retention: bool,
    pub auto_archive: bool,
    pub mail_import: bool,
    pub ai: bool,
}

pub fn current_info() -> TierInfo {
    TierInfo {
        tier: CURRENT,
        max_mailboxes: MAX_MAILBOXES,
        max_send_delay_secs: MAX_SEND_DELAY_SECS,
        features: TierFeatures {
            vpn: cfg!(feature = "pro"),
            trusted_devices: cfg!(feature = "pro"),
            licensed_updates: cfg!(feature = "pro"),
            gmail_categories_purge: ALLOW_GMAIL_CATEGORIES_PURGE,
            server_retention: ALLOW_SERVER_RETENTION,
            auto_archive: ALLOW_AUTO_ARCHIVE,
            mail_import: ALLOW_MAIL_IMPORT,
            ai: ALLOW_AI,
        },
    }
}
