pub mod auto_archive;
mod gmail_labels;
pub mod gmail_rescan;
pub mod idle;
mod imap;
pub mod label_policy;
pub mod parser;
pub mod purge;
mod scheduler;

pub use idle::IdleSupervisor;
pub use imap::{probe, FolderRole, ImapClient};
pub use parser::{body_text_of, subject_key_of, thread_id_of, uid_set};
pub use scheduler::{Scheduler, SyncReport};
