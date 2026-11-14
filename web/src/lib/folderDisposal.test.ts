import { describe, expect, test } from 'vitest';
import type { AccountFolders, AccountKind, FoldersResponse } from './api';
import {
  canonicalDisposalNames,
  isDisposalFolder,
  isProtectedFolder,
  unifiedEmptyTargets
} from './folderDisposal';

// Minimal AccountFolders fixture — only the fields the disposal logic
// reads. Avatar/colour/etc. are irrelevant here.
function acct(o: {
  id: number;
  kind: AccountKind;
  unified: boolean;
  system?: string[];
  user?: string[];
}): AccountFolders {
  const mk = (name: string) => ({
    name,
    display: name,
    kind: 'system' as const,
    total: 0,
    unread: 0,
    size_bytes: 0,
    weight: 0
  });
  return {
    account_id: o.id,
    kind: o.kind,
    include_in_unified: o.unified,
    system: (o.system ?? []).map(mk),
    user: (o.user ?? []).map(mk),
    categories: [],
    categories_missing: [],
    email: 't@example.com',
    avatar_seed: null,
    color: null
  } as unknown as AccountFolders;
}

describe('isProtectedFolder', () => {
  test('protects Inbox/Sent/Drafts and Gmail pseudo-folders (case-insensitive)', () => {
    expect(isProtectedFolder('INBOX')).toBe(true);
    expect(isProtectedFolder('Sent')).toBe(true);
    expect(isProtectedFolder('[Gmail]/All Mail')).toBe(true);
  });

  test('does not protect Trash, Spam, or user labels', () => {
    expect(isProtectedFolder('Trash')).toBe(false);
    expect(isProtectedFolder('Receipts')).toBe(false);
  });
});

describe('isDisposalFolder', () => {
  test('recognises trash/spam/junk flavours, null-safe', () => {
    expect(isDisposalFolder('Trash')).toBe(true);
    expect(isDisposalFolder('[Gmail]/Spam')).toBe(true);
    expect(isDisposalFolder('Junk')).toBe(true);
    expect(isDisposalFolder('Receipts')).toBe(false);
    expect(isDisposalFolder(null)).toBe(false);
  });
});

describe('canonicalDisposalNames', () => {
  test('Gmail uses its pseudo-paths', () => {
    expect(canonicalDisposalNames('trash', 'gmail')).toEqual(['[Gmail]/Trash']);
    expect(canonicalDisposalNames('spam', 'gmail')).toEqual(['[Gmail]/Spam']);
  });

  test('plain IMAP offers the common spellings, Trash first', () => {
    expect(canonicalDisposalNames('trash', 'imap')[0]).toBe('Trash');
    expect(canonicalDisposalNames('spam', 'imap')).toContain('Junk');
  });
});

describe('unifiedEmptyTargets', () => {
  test('returns [] when folders are not loaded', () => {
    expect(unifiedEmptyTargets(null, 'trash')).toEqual([]);
  });

  test('picks one canonical folder per included account, by kind', () => {
    const folders: FoldersResponse = {
      accounts: [
        acct({ id: 1, kind: 'gmail', unified: true, system: ['INBOX', '[Gmail]/Trash'] }),
        acct({ id: 2, kind: 'imap', unified: true, user: ['Bin'] })
      ]
    };
    expect(unifiedEmptyTargets(folders, 'trash')).toEqual([
      { accountId: 1, folder: '[Gmail]/Trash' },
      { accountId: 2, folder: 'Bin' }
    ]);
  });

  test('skips accounts excluded from the unified view', () => {
    const folders: FoldersResponse = {
      accounts: [acct({ id: 9, kind: 'imap', unified: false, system: ['Trash'] })]
    };
    expect(unifiedEmptyTargets(folders, 'trash')).toEqual([]);
  });

  test('skips accounts with no matching disposal folder', () => {
    const folders: FoldersResponse = {
      accounts: [acct({ id: 3, kind: 'imap', unified: true, system: ['INBOX'] })]
    };
    expect(unifiedEmptyTargets(folders, 'spam')).toEqual([]);
  });

  test('stops at the first matching candidate per account', () => {
    // Account has both 'Trash' and 'Deleted' — only the first canonical
    // hit ('Trash') should be targeted, never both.
    const folders: FoldersResponse = {
      accounts: [acct({ id: 4, kind: 'imap', unified: true, system: ['Trash', 'Deleted'] })]
    };
    expect(unifiedEmptyTargets(folders, 'trash')).toEqual([{ accountId: 4, folder: 'Trash' }]);
  });
});
