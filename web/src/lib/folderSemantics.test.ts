import { describe, expect, test } from 'vitest';
import {
  countsTowardAggregateUnread,
  displayCounts,
  showsTotal,
  showsUnread
} from './folderSemantics';

describe('showsUnread', () => {
  test('Inbox and user labels keep their unread badge', () => {
    expect(showsUnread('INBOX')).toBe(true);
    expect(showsUnread('Receipts')).toBe(true);
  });

  test('Sent / Drafts / Trash suppress unread (case-insensitive, trimmed)', () => {
    expect(showsUnread('Sent')).toBe(false);
    expect(showsUnread('  drafts ')).toBe(false);
    expect(showsUnread('[Gmail]/Sent Mail')).toBe(false);
  });
});

describe('showsTotal', () => {
  test('Inbox does not show a total (unread is the signal)', () => {
    expect(showsTotal('INBOX')).toBe(false);
  });

  test('Drafts shows a total instead of unread', () => {
    expect(showsTotal('Drafts')).toBe(true);
    expect(showsUnread('Drafts')).toBe(false);
  });
});

describe('displayCounts', () => {
  test('zeroes unread for suppressed folders but keeps total where allowed', () => {
    expect(displayCounts('Sent', { unread: 3, total: 9 })).toEqual({
      unread: 0,
      total: 9
    });
  });

  test('keeps unread for normal folders and zeroes Inbox total', () => {
    expect(displayCounts('INBOX', { unread: 2, total: 50 })).toEqual({
      unread: 2,
      total: 0
    });
  });
});

describe('countsTowardAggregateUnread', () => {
  test('mirrors showsUnread so phantom Sent unread never rolls up', () => {
    expect(countsTowardAggregateUnread('Sent')).toBe(false);
    expect(countsTowardAggregateUnread('INBOX')).toBe(true);
  });
});
