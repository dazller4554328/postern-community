import { describe, expect, test } from 'vitest';
import {
  ACCOUNT_COLOR_PALETTE,
  buildAccountColorMap,
  colorForAccount
} from './accountColor';

describe('colorForAccount', () => {
  test('honours a valid custom colour, lower-cased', () => {
    expect(colorForAccount({ id: 1, color: '#AABBCC' })).toBe('#aabbcc');
  });

  test('falls back to a deterministic palette pick when colour is null', () => {
    // id 0 → palette[0]; same id always lands on the same swatch.
    expect(colorForAccount({ id: 0, color: null })).toBe(ACCOUNT_COLOR_PALETTE[0]);
    expect(colorForAccount({ id: 13, color: null })).toBe(
      ACCOUNT_COLOR_PALETTE[13 % ACCOUNT_COLOR_PALETTE.length]
    );
  });

  test('ignores a malformed custom colour and falls back to the palette', () => {
    expect(colorForAccount({ id: 0, color: 'not-a-hex' })).toBe(ACCOUNT_COLOR_PALETTE[0]);
    expect(colorForAccount({ id: 0, color: '#fff' })).toBe(ACCOUNT_COLOR_PALETTE[0]);
  });

  test('handles negative ids without going out of palette bounds', () => {
    const c = colorForAccount({ id: -7, color: null });
    expect(ACCOUNT_COLOR_PALETTE).toContain(c);
  });
});

describe('buildAccountColorMap', () => {
  test('builds an O(1) id → colour lookup for a list of accounts', () => {
    const map = buildAccountColorMap([
      { id: 1, color: '#123456' },
      { id: 2, color: null }
    ]);
    expect(map[1]).toBe('#123456');
    expect(map[2]).toBe(ACCOUNT_COLOR_PALETTE[2 % ACCOUNT_COLOR_PALETTE.length]);
  });

  test('returns an empty map for no accounts', () => {
    expect(buildAccountColorMap([])).toEqual({});
  });
});
