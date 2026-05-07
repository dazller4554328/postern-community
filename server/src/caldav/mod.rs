//! CalDAV client — discovery, sync, and minimal multistatus parsing.
//!
//! Scope is deliberately narrow: read-only sync of VEVENTs from a
//! handful of well-known CalDAV servers (Nextcloud, iCloud, Fastmail,
//! Radicale, Baïkal). Write-back (event create / edit / delete) is a
//! follow-up — see the caller sites in `http::caldav` for the ready
//! extension points.
//!
//! We don't pull in a caldav-specific crate. The protocol is just
//! PROPFIND + REPORT + Basic auth; a thin reqwest + quick-xml layer is
//! both easier to reason about and avoids coupling to a crate we'd
//! have to patch the moment we hit a server quirk.

mod client;
mod discover;
mod parse;
mod recurrence;
mod sync;

pub use recurrence::expand_rrule_in_range;
pub use sync::{sync_account, SyncReport};
