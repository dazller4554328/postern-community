// Unified cross-account system folders.
//
// Each system maps to a set of IMAP folder names covering the common
// spellings across Gmail and conventional IMAP servers. Clicking a
// unified row shows messages whose labels match ANY of these names.

import type { AccountFolders } from './api';
import { countsTowardAggregateUnread } from './folderSemantics';

export type UnifiedSystem = 'inbox' | 'sent' | 'drafts' | 'spam' | 'trash';

export const UNIFIED_LABELS: Record<UnifiedSystem, string[]> = {
  inbox: ['INBOX'],
  sent: ['Sent', 'Sent Mail', 'Sent Messages', 'Sent Items', '[Gmail]/Sent Mail'],
  drafts: ['Drafts', '[Gmail]/Drafts'],
  spam: ['Spam', 'Junk', '[Gmail]/Spam'],
  trash: ['Trash', 'Bin', 'Deleted', '[Gmail]/Trash']
};

export const UNIFIED_DISPLAY: Record<UnifiedSystem, string> = {
  inbox: 'Inbox',
  sent: 'Sent',
  drafts: 'Drafts',
  spam: 'Spam',
  trash: 'Trash'
};

// For the sidebar icons — matches FolderIcon's lookup keys.
export const UNIFIED_ICON_NAME: Record<UnifiedSystem, string> = {
  inbox: 'INBOX',
  sent: 'Sent',
  drafts: 'Drafts',
  spam: 'Spam',
  trash: 'Trash'
};

export function isUnifiedSystem(s: string | null): s is UnifiedSystem {
  return s === 'inbox' || s === 'sent' || s === 'drafts' || s === 'spam' || s === 'trash';
}

// Sum unread + total across every account's folder that matches the
// unified system's label set.
export function unifiedCounts(
  accounts: AccountFolders[] | undefined,
  system: UnifiedSystem
): { unread: number; total: number } {
  if (!accounts) return { unread: 0, total: 0 };
  const names = new Set(UNIFIED_LABELS[system]);
  let unread = 0;
  let total = 0;
  for (const a of accounts) {
    // Hide mailboxes the user has excluded from the unified view.
    // They still render in the sidebar per-account but don't
    // contribute to the cross-account badges at the top.
    if (!a.include_in_unified) continue;
    for (const f of [...a.system, ...a.user]) {
      if (!names.has(f.name)) continue;
      total += f.total;
      // Folder semantics: Sent / Drafts / Trash never contribute
      // unread — "unread" doesn't apply there. Without this filter,
      // a "phantom 1 unread in Unified Sent" appears when any
      // per-account Sent has a message the provider left flagged
      // \Unseen (Gmail does this intermittently on outgoing mail).
      if (countsTowardAggregateUnread(f.name)) {
        unread += f.unread;
      }
    }
  }
  return { unread, total };
}
