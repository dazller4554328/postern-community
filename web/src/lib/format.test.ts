import { afterEach, describe, expect, test, vi } from 'vitest';
import { flagEmoji, formatRelative, formatSender, humanBytes } from './format';

describe('formatSender', () => {
  test('extracts the display name from a "Name <addr>" header', () => {
    expect(formatSender('Jane Doe <jane@example.com>')).toBe('Jane Doe');
  });

  test('strips surrounding quotes from the display name', () => {
    expect(formatSender('"Doe, Jane" <jane@example.com>')).toBe('Doe, Jane');
  });

  test('returns the raw address when there is no display name', () => {
    expect(formatSender('jane@example.com')).toBe('jane@example.com');
  });

  test('handles a null sender', () => {
    expect(formatSender(null)).toBe('(unknown sender)');
  });
});

describe('humanBytes', () => {
  test('formats bytes, KB and MB at the right thresholds', () => {
    expect(humanBytes(512)).toBe('512 B');
    expect(humanBytes(2048)).toBe('2.0 KB');
    expect(humanBytes(5 * 1024 * 1024)).toBe('5.00 MB');
  });
});

describe('flagEmoji', () => {
  test('maps a 2-letter country code to its flag', () => {
    expect(flagEmoji('GB')).toBe('🇬🇧');
    expect(flagEmoji('us')).toBe('🇺🇸'); // case-insensitive
  });

  test('returns empty string for invalid input', () => {
    expect(flagEmoji(null)).toBe('');
    expect(flagEmoji('USA')).toBe('');
    expect(flagEmoji('1!')).toBe('');
  });
});

describe('formatRelative', () => {
  afterEach(() => vi.useRealTimers());

  test('renders coarse relative buckets against a fixed clock', () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-01-10T12:00:00Z'));
    const now = Math.floor(Date.now() / 1000);

    expect(formatRelative(now - 10)).toBe('just now');
    expect(formatRelative(now - 5 * 60)).toBe('5m ago');
    expect(formatRelative(now - 3 * 3600)).toBe('3h ago');
    expect(formatRelative(now - 86_400)).toBe('Yesterday');
    expect(formatRelative(now - 3 * 86_400)).toBe('3d ago');
  });

  test('renders an em dash for a missing timestamp', () => {
    expect(formatRelative(0)).toBe('—');
  });
});
