// Pure folder-disposal logic extracted from the inbox view.
//
// "Disposal" = emptying Trash / Spam (and, with a stronger confirm,
// user labels). These functions decide *which* folders an empty action
// targets and *whether* the action is allowed — no API calls, no DOM,
// no component state. Keeping them pure makes the destructive paths in
// the inbox view unit-testable in isolation.

import type { AccountKind, FoldersResponse } from './api';

export type DisposalSystem = 'trash' | 'spam';

export interface EmptyTarget {
  accountId: number;
  folder: string;
}

/**
 * Folders the backend refuses to empty. Mirrored here (keyed lowercase)
 * so the UI never offers the action where the server would reject it.
 */
export const PROTECTED_FOLDERS: ReadonlySet<string> = new Set([
  'inbox',
  'sent',
  'drafts',
  'sent items',
  'sent messages',
  '[gmail]/sent mail',
  '[gmail]/drafts',
  '[gmail]/all mail',
  '[gmail]/important',
  '[gmail]/starred'
]);

/** True when the backend would reject an "empty" of this folder. */
export function isProtectedFolder(name: string): boolean {
  return PROTECTED_FOLDERS.has(name.toLowerCase());
}

/**
 * Whether the folder is a Trash- or Spam-flavoured disposal bin, where
 * "empty" is a routine operation (one confirm) rather than deleting
 * real work (type-the-name confirm).
 */
export function isDisposalFolder(name: string | null): boolean {
  if (!name) return false;
  const f = name.toLowerCase();
  return (
    f === 'trash' ||
    f === 'spam' ||
    f === 'junk' ||
    f === '[gmail]/trash' ||
    f === '[gmail]/spam'
  );
}

/**
 * Canonical folder-name candidates for a disposal system, ordered by
 * preference, per account kind. Gmail uses its `[Gmail]/…` pseudo-paths;
 * plain IMAP servers spell these folders many ways.
 */
export function canonicalDisposalNames(system: DisposalSystem, kind: AccountKind): string[] {
  if (system === 'trash') {
    return kind === 'gmail'
      ? ['[Gmail]/Trash']
      : ['Trash', 'Deleted', 'Deleted Items', 'Deleted Messages', 'Bin'];
  }
  return kind === 'gmail'
    ? ['[Gmail]/Spam']
    : ['Spam', 'Junk', 'Junk E-mail', 'Junk Mail'];
}

/**
 * Resolve the per-account folders that make up a unified Trash / Spam
 * view, so the caller can fire one empty per real folder. Only accounts
 * included in the unified view are considered, and at most one canonical
 * folder is picked per account — passing "[Gmail]/Trash" to a plain IMAP
 * account with a stale label row would just produce a noisy expunge error.
 */
export function unifiedEmptyTargets(
  folders: FoldersResponse | null,
  system: DisposalSystem
): EmptyTarget[] {
  if (!folders) return [];
  const out: EmptyTarget[] = [];
  for (const acct of folders.accounts) {
    if (!acct.include_in_unified) continue;
    const candidates = canonicalDisposalNames(system, acct.kind);
    const present = new Set([...acct.system, ...acct.user].map((f) => f.name));
    for (const name of candidates) {
      if (present.has(name)) {
        out.push({ accountId: acct.account_id, folder: name });
        break;
      }
    }
  }
  return out;
}
