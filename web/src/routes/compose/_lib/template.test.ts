import { describe, expect, test } from 'vitest';
import { applyTemplate, hasDraftText } from './template';

const REPLY_NO_SIG = '\n\nOn 10/11/2026, 09:00:00, Bob <bob@example.com> wrote:\n> original\n> more';
const REPLY_WITH_SIG = '\n\n-- \nDarren\n\nOn 10/11/2026, 09:00:00, Bob wrote:\n> original';
const FORWARD = '\n\n--- Forwarded message ---\nFrom: bob@example.com\nDate: x\nSubject: y\nTo: z\n\n> original';

describe('applyTemplate', () => {
  test('replaces an empty body outright', () => {
    expect(applyTemplate('', 'Hello there\n')).toBe('Hello there');
  });

  test('keeps the signature block below the template', () => {
    expect(applyTemplate('\n\n-- \nDarren\n', 'Hi,\n\nInvoice attached.')).toBe(
      'Hi,\n\nInvoice attached.\n\n-- \nDarren\n'
    );
  });

  test('keeps signature and quoted reply on a reply draft', () => {
    expect(applyTemplate(REPLY_WITH_SIG, 'Thanks Bob.')).toBe(
      'Thanks Bob.\n\n-- \nDarren\n\nOn 10/11/2026, 09:00:00, Bob wrote:\n> original'
    );
  });

  test('keeps the quoted reply when there is no signature', () => {
    expect(applyTemplate(REPLY_NO_SIG, 'Thanks Bob.')).toBe(
      'Thanks Bob.\n\nOn 10/11/2026, 09:00:00, Bob <bob@example.com> wrote:\n> original\n> more'
    );
  });

  test('keeps the forwarded block when there is no signature', () => {
    expect(applyTemplate(FORWARD, 'FYI')).toBe('FYI' + FORWARD);
  });

  test('handles a body that starts with the signature marker', () => {
    expect(applyTemplate('-- \nSig\n', 'Body')).toBe('Body\n\n-- \nSig\n');
  });

  test('overwrites existing text above the signature', () => {
    expect(applyTemplate('old text\n\n-- \nSig\n', 'new')).toBe('new\n\n-- \nSig\n');
  });

  test('does not treat "On … wrote:" mid-sentence as a boundary', () => {
    const body = 'I was wondering what On Monday, Bob wrote:\nabout.';
    expect(applyTemplate(body, 'X')).toBe('X');
  });
});

describe('hasDraftText', () => {
  test('false for blank, signature-only, or quote-only bodies', () => {
    expect(hasDraftText('')).toBe(false);
    expect(hasDraftText('\n\n-- \nSig\n')).toBe(false);
    expect(hasDraftText(REPLY_WITH_SIG)).toBe(false);
    expect(hasDraftText(REPLY_NO_SIG)).toBe(false);
    expect(hasDraftText(FORWARD)).toBe(false);
  });

  test('true once the user has typed above the signature or quote', () => {
    expect(hasDraftText('hello\n\n-- \nSig\n')).toBe(true);
    expect(hasDraftText('hello' + REPLY_NO_SIG)).toBe(true);
    expect(hasDraftText('hello')).toBe(true);
  });
});
