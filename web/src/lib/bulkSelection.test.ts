import { describe, expect, test } from 'vitest';
import {
  allSelected,
  selectAll,
  selectedIds,
  toggleOne,
  toggleRange,
  type Checked
} from './bulkSelection';

const items = (...ids: number[]) => ids.map((id) => ({ id }));

describe('selectedIds', () => {
  test('returns the numeric ids of the selected map', () => {
    expect(selectedIds({ 3: true, 7: true }).sort()).toEqual([3, 7]);
    expect(selectedIds({})).toEqual([]);
  });
});

describe('toggleOne', () => {
  test('adds an unselected id and removes a selected one, immutably', () => {
    const a: Checked = {};
    const b = toggleOne(a, 5);
    expect(b).toEqual({ 5: true });
    expect(a).toEqual({}); // original untouched
    expect(toggleOne(b, 5)).toEqual({});
  });
});

describe('selectAll', () => {
  test('selects every visible item', () => {
    expect(selectAll(items(1, 2, 3))).toEqual({ 1: true, 2: true, 3: true });
    expect(selectAll([])).toEqual({});
  });
});

describe('allSelected', () => {
  test('true only when every visible item is selected', () => {
    expect(allSelected({ 1: true, 2: true }, items(1, 2))).toBe(true);
    expect(allSelected({ 1: true }, items(1, 2))).toBe(false);
  });

  test('false for an empty list (nothing to select)', () => {
    expect(allSelected({}, [])).toBe(false);
  });
});

describe('toggleRange', () => {
  const list = items(10, 20, 30, 40, 50);

  test('selects an inclusive forward range', () => {
    expect(toggleRange({}, list, 20, 40, true)).toEqual({ 20: true, 30: true, 40: true });
  });

  test('works regardless of click order (reverse range)', () => {
    expect(toggleRange({}, list, 40, 20, true)).toEqual({ 20: true, 30: true, 40: true });
  });

  test('deselects across the range when targetState is false', () => {
    const start: Checked = { 10: true, 20: true, 30: true, 40: true };
    expect(toggleRange(start, list, 20, 40, false)).toEqual({ 10: true });
  });

  test('preserves selections outside the range', () => {
    expect(toggleRange({ 50: true }, list, 10, 20, true)).toEqual({
      10: true,
      20: true,
      50: true
    });
  });

  test('returns the same reference when an anchor id is missing', () => {
    const start: Checked = { 10: true };
    // 99 isn't in the list → caller falls back to a single toggle.
    expect(toggleRange(start, list, 99, 30, true)).toBe(start);
  });
});
