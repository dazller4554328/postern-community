// Folder-display semantics — the rules every mail client needs to
// agree with itself on before count badges make sense.
//
// Background: most folders should show an unread badge when any of
// their messages are unread. But for some folders, the concept of
// "unread" doesn't apply semantically, and displaying a count there
// is user-hostile noise:
//
//   * Sent    — you wrote the message. "Unread" is a property of
//               inbound mail you haven't attended to yet. Every mail
//               client with taste (Evolution, Outlook, Gmail, Apple
//               Mail) suppresses the unread badge here. Thunderbird
//               is the lone holdout and its default is widely
//               considered a bug.
//   * Drafts  — they're your in-progress writes; none of them are
//               inbound, so there's nothing to "have read yet."
//               Show the total count (how many unfinished) instead.
//   * Outbox  — same logic as Drafts. In-flight, not received.
//   * Trash   — out of sight, out of mind. Evolution and Outlook
//               explicitly hide unread here because otherwise you
//               get badges on mail you've already decided to discard.
//   * All Mail (Gmail) — superset of Inbox; showing an unread badge
//               would double-count everything.
//
// The rule is applied uniformly across:
//   * sidebar per-account rows
//   * sidebar unified (cross-account) rows
//   * collapsed-account summary badge
//   * subtree aggregation for parent folder rows
//
// If we stop counting unread in these places, the weird "1 unread
// in Unified Sent that I can't find" experience disappears, because
// the aggregation never included Sent's phantom unread in the first
// place.

/**
 * Folder names where unread counts are NEVER displayed. Keyed by
 * lowercase name — match is case-insensitive. Covers both the Gmail
 * pseudo-folder spellings ("[Gmail]/Sent Mail") and the plain IMAP
 * conventions ("Sent", "Sent Mail", "Sent Items", etc.).
 */
const NO_UNREAD_FOLDERS = new Set<string>([
  // Sent
  'sent',
  'sent mail',
  'sent messages',
  'sent items',
  '[gmail]/sent mail',
  // Drafts
  'drafts',
  '[gmail]/drafts',
  // Outbox (rare over IMAP but some clients expose it)
  'outbox',
  // Trash / Bin / Deleted
  'trash',
  'bin',
  'deleted',
  'deleted items',
  'deleted messages',
  '[gmail]/trash',
  // All Mail — showing unread here double-counts every inbox message
  'all mail',
  '[gmail]/all mail'
]);

/**
 * Folders where the TOTAL count isn't useful either. Inbox in
 * particular: once you're caught up (0 unread) you don't also need
 * to see "382 messages" — the unread badge going away is the point.
 * User labels and everything else show total when unread is zero.
 */
const NO_TOTAL_FOLDERS = new Set<string>([
  'inbox',
  'all mail',
  '[gmail]/all mail',
  // Meta-views that just surface the same messages from elsewhere.
  '[gmail]/important',
  '[gmail]/starred'
]);

/**
 * Whether this folder should display an unread-count badge. Defaults
 * to true for any folder not on the suppression list — so user labels,
 * Gmail categories, Inbox, Spam, and any novel folder a server
 * invents all keep their badges without explicit entries here.
 */
export function showsUnread(folderName: string): boolean {
  return !NO_UNREAD_FOLDERS.has(folderName.trim().toLowerCase());
}

/**
 * Whether this folder should display a total-count badge (used when
 * there's no unread to show). True for Drafts / Outbox / Trash / Spam
 * / user labels; false for Inbox (unread is the signal) and meta-views.
 */
export function showsTotal(folderName: string): boolean {
  return !NO_TOTAL_FOLDERS.has(folderName.trim().toLowerCase());
}

/**
 * Convenience: returns the effective (unread, total) a row should
 * show after applying the semantic rules. Call at every render site
 * so the choice is consistent.
 */
export function displayCounts(
  folderName: string,
  raw: { unread: number; total: number }
): { unread: number; total: number } {
  return {
    unread: showsUnread(folderName) ? raw.unread : 0,
    total: showsTotal(folderName) ? raw.total : 0
  };
}

/**
 * True when this folder should NOT contribute to aggregate unread
 * counts — e.g. the collapsed-account summary badge, the "All
 * mailboxes" count, or a parent folder rolling up its subtree.
 * Same list as showsUnread but named for clarity at aggregation sites.
 */
export function countsTowardAggregateUnread(folderName: string): boolean {
  return showsUnread(folderName);
}
