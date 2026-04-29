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
mod db;
// Trusted-device table + methods (touch/enroll/revoke) are pro-only.
// Schema migrations still run on free so a user upgrading free→paid
// doesn't face a migration gap; just the Rust layer is gated out.
#[cfg(feature = "pro")]
mod devices;
mod labels;
mod message_queries;
mod messages;
mod outbox;
mod reminders;
mod search_query;

pub use accounts::{Account, AccountKind, ArchiveStrategy, NewAccount};
pub use ai::{
    parse_exclusion_list, sender_patterns_to_like, AiActivityBucket, AiActivityDetail,
    AiActivityRow, AiSettings, ChatLogRow, MissingEmbedRow, NewAiActivity, NewChatLog,
    SimilarMessage, UpdateAiSettings,
};
pub use app_meta::AppMeta;
pub use audit::AuditEntry;
pub use auth_totp::AuthTotpStatus;
pub use backup_destinations::{
    BackupDestination, GDriveCredential, GDrivePublicConfig, NewBackupDestination,
    SftpCredential, SftpPublicConfig,
};
pub use backup_schedule::{
    should_fire_now, BackupFrequency, BackupSchedule, UpdateBackupSchedule,
};
pub use blobs::BlobStore;
pub use calendar::{CalAccount, CalCalendar, CalEvent, NewCalAccount, UpsertCalEvent};
pub use contacts::Contact;
pub use db::Db;
#[cfg(feature = "pro")]
pub use devices::TrustedDevice;
pub use labels::{FolderCount, Label};
pub use messages::{MessageDetail, MessageListItem, NewMessage, SearchHit, ThreadSummary};
pub use outbox::{OutboxEntry, OutboxStatus};
pub use reminders::{NewReminder, Reminder, ReminderRepeat, UpdateReminder};
