//! Single source of truth for "what local labels does a sync of
//! folder X for account Y do to message Z?" Pure functions, no DB,
//! no IMAP — testable without any harness.
//!
//! See `docs/STORAGE_INVARIANTS.md` for the rationale; the table
//! there and the table-driven tests below are the same spec.
//!
//! Both call sites consume `decide_mirror`:
//!
//!   - `sync::imap::sync_folder` → during the streaming fetch, asks
//!     "given this message and this folder, what local labels should
//!     I attach?"
//!
//!   - `sync::gmail_rescan` → on explicit X-GM-LABELS rescan, asks
//!     the same question after pulling the server's authoritative
//!     label set.
//!
//! Codifying this as a single function means a future bug-fix lands
//! in one place and the unit tests catch the regression. The
//! migration 0035 incident traced directly to *not* having this
//! abstraction — label-mirror policy was scattered conditionals in
//! sync_folder, and the answer drifted.

use crate::storage::AccountKind;

/// What the caller should do with the local labels for the message
/// it's about to upsert.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirrorDecision {
    /// Labels to attach to the message. Caller passes these into
    /// `NewMessage::label_names`. Empty = don't add anything (the
    /// message keeps whatever labels it had before).
    pub add: Vec<String>,
}

/// Whether Postern has already seen this Message-ID for this account.
///
/// Threaded through `decide_mirror` because policy changes for
/// already-known messages — the rule is "first sighting decides the
/// initial label; subsequent sightings don't re-tag."
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessagePresence {
    New,
    AlreadyKnown,
}

/// Pure decision: given the account, the folder being synced, and
/// whether Postern already knows the message, return the local-label
/// changes for this sighting.
///
/// **The single mirror rule**, lifted from `STORAGE_INVARIANTS.md`:
///
///   - First sighting → tag with the source folder name (so the
///     message lands in that folder's view).
///
///   - Already-known sighting in any folder → add nothing. The
///     message keeps the labels it has from earlier sync paths or
///     from explicit user moves. Server-side label changes (e.g.
///     Gmail's MOVE-to-Trash) are NOT mirrored locally; Postern's
///     UI is canonical for organisation.
///
///   - Special case: Gmail Trash sync of an already-known message
///     was the bug at the centre of migration 0035 / commits
///     07776c6→55e286d. The general "AlreadyKnown ⇒ no add" rule
///     covers it; the test below pins the case explicitly.
pub fn decide_mirror(
    _account_kind: AccountKind,
    folder: &str,
    presence: MessagePresence,
) -> MirrorDecision {
    match presence {
        MessagePresence::New => MirrorDecision {
            add: vec![folder.to_owned()],
        },
        MessagePresence::AlreadyKnown => MirrorDecision { add: Vec::new() },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn add(folder: &str, presence: MessagePresence, kind: AccountKind) -> Vec<String> {
        decide_mirror(kind, folder, presence).add
    }

    /// First sighting in INBOX always tags INBOX, both Gmail and IMAP.
    #[test]
    fn new_inbox_tags_inbox() {
        assert_eq!(add("INBOX", MessagePresence::New, AccountKind::Gmail), vec!["INBOX"]);
        assert_eq!(add("INBOX", MessagePresence::New, AccountKind::Imap), vec!["INBOX"]);
    }

    /// First sighting in any other folder tags that folder name as-is.
    #[test]
    fn new_other_folder_tags_folder() {
        assert_eq!(
            add("[Gmail]/Sent Mail", MessagePresence::New, AccountKind::Gmail),
            vec!["[Gmail]/Sent Mail"]
        );
        assert_eq!(
            add("Custom/Project", MessagePresence::New, AccountKind::Imap),
            vec!["Custom/Project"]
        );
    }

    /// Already-known message sighted again in INBOX adds nothing.
    /// (Live sync rarely re-sees INBOX UIDs once UID-NEXT moves
    /// past, but explicit user-driven sync or rescan can.)
    #[test]
    fn known_inbox_adds_nothing() {
        assert!(add("INBOX", MessagePresence::AlreadyKnown, AccountKind::Gmail).is_empty());
        assert!(add("INBOX", MessagePresence::AlreadyKnown, AccountKind::Imap).is_empty());
    }

    /// **The migration 0035 test.** Already-known message sighted in
    /// `[Gmail]/Trash` MUST NOT add the Trash label. Otherwise it
    /// shows up in two views (its original home + Trash) and any
    /// later "strip the original" reflex collapses to data loss.
    #[test]
    fn known_gmail_trash_adds_nothing() {
        assert!(add("[Gmail]/Trash", MessagePresence::AlreadyKnown, AccountKind::Gmail).is_empty());
    }

    /// First sighting in Gmail Trash *does* tag Trash — that's a
    /// genuine "user trashed in Gmail web before Postern saw the
    /// message". The user trashed it, the user expects to see it
    /// in Trash.
    #[test]
    fn new_gmail_trash_tags_trash() {
        assert_eq!(
            add("[Gmail]/Trash", MessagePresence::New, AccountKind::Gmail),
            vec!["[Gmail]/Trash"]
        );
    }

    /// Same rule for plain IMAP — sync over Trash sees an unknown
    /// message, tag it with the folder name.
    #[test]
    fn new_imap_trash_tags_trash() {
        assert_eq!(
            add("Trash", MessagePresence::New, AccountKind::Imap),
            vec!["Trash"]
        );
    }

    /// Already-known plain-IMAP message in any folder adds nothing.
    /// (Plain IMAP rarely re-syncs the same UID, but the rule
    /// shouldn't depend on that — it should depend on presence.)
    #[test]
    fn known_imap_any_folder_adds_nothing() {
        for folder in ["INBOX", "Sent", "Drafts", "Spam", "Trash", "Custom"] {
            assert!(
                add(folder, MessagePresence::AlreadyKnown, AccountKind::Imap).is_empty(),
                "folder {folder} should add nothing for known message"
            );
        }
    }

    /// First-sighting rule applies regardless of folder name —
    /// custom labels, hierarchical names, Gmail-special bracket
    /// names all tag the literal folder string.
    #[test]
    fn new_custom_folder_names() {
        for folder in [
            "[Gmail]/All Mail",
            "[Gmail]/Drafts",
            "Archive/2026/04",
            "Receipts",
        ] {
            let result = add(folder, MessagePresence::New, AccountKind::Gmail);
            assert_eq!(result, vec![folder.to_string()], "folder {folder}");
        }
    }
}
