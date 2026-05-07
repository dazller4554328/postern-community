mod accounts;
mod ai;
mod app_meta;
mod audit;
mod auth_totp;
mod backup_destinations;
mod backup_schedule;
mod blobs;
mod calendar;
mod contacts;
mod curated;
mod db;
// Trusted-device table + methods (touch/enroll/revoke) are pro-only.
// Schema migrations still run on free so a user upgrading free→paid
// doesn't face a migration gap; just the Rust layer is gated out.
#[cfg(feature = "pro")]
mod devices;
mod labels;
mod message_queries;
mod messages;
mod notes;
mod outbox;
mod reminders;
mod search_query;
mod trusted_senders;

pub use accounts::{Account, AccountKind, ArchiveStrategy, NewAccount};
pub use ai::{
    parse_exclusion_list, sender_patterns_to_like, AiActivityBucket, AiActivityDetail,
    AiActivityRow, AiSettings, ChatLogRow, NewAiActivity, NewChatLog, SimilarMessage,
    UpdateAiSettings,
};
pub use app_meta::AppMeta;
pub use audit::AuditEntry;
pub use backup_destinations::{
    BackupDestination, GDriveCredential, GDrivePublicConfig, NewBackupDestination,
    SftpCredential, SftpPublicConfig,
};
pub use backup_schedule::{
    should_fire_now, BackupFrequency, BackupSchedule, UpdateBackupSchedule,
};
pub use blobs::BlobStore;
pub use calendar::{
    CalAccount, CalCalendar, CalEvent, NewCalAccount, NewLocalEvent, PatchLocalEvent,
    UpsertCalEvent,
};
pub use contacts::Contact;
pub use curated::CuratedListItem;
pub use db::Db;
#[cfg(feature = "pro")]
pub use devices::TrustedDevice;

/// SHA-256 hex of a session/device cookie token. Identical bytes to
/// `storage::devices::hash_token` (the pro module's helper) — kept
/// here so non-pro builds (which gate out the whole `devices` module)
/// can still derive the same hash for `Vault::session_*` lookups.
pub fn hash_session_token(token: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(token.as_bytes());
    hex::encode(h.finalize())
}
pub use labels::{FolderCount, Label};
pub use messages::{MessageDetail, MessageListItem, NewMessage, SearchHit, ThreadSummary};
pub use notes::{NewNote, Note, UpdateNote};
pub use outbox::OutboxEntry;
pub use reminders::{NewReminder, Reminder, UpdateReminder};
pub use trusted_senders::TrustedSender;
