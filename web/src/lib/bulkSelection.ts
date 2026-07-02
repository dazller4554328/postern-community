// Pure set-algebra for the inbox multi-select. The selection is a
// Record<id, true> (chosen over Set for clean Svelte 5 reactivity in
// templates); these helpers compute the *next* map for each operation
// without touching component state, so the fiddly shift-click range
// math is unit-testable in isolation. The inbox view keeps the reactive
// `checked` state and calls these to derive the next value.

export type Checked = Record<number, true>;

interface HasId {
  id: number;
}

/** The selected ids, as finite numbers. */
export function selectedIds(checked: Checked): number[] {
  return Object.keys(checked)
    .map(Number)
    .filter((n) => Number.isFinite(n));
}

/** Toggle a single id on/off, returning a new map. */
export function toggleOne(checked: Checked, id: number): Checked {
  const next = { ...checked };
  if (next[id]) delete next[id];
  else next[id] = true;
  return next;
}

/** Select every visible item. */
export function selectAll(items: HasId[]): Checked {
  const next: Checked = {};
  for (const m of items) next[m.id] = true;
  return next;
}

/** True when every visible item is selected (false for an empty list). */
export function allSelected(checked: Checked, items: HasId[]): boolean {
  return items.length > 0 && items.every((m) => checked[m.id]);
}

/**
 * Shift-click range fill: apply `targetState` to every item between
 * `fromId` and `toId` (inclusive) in `items`, whichever order they
 * appear. `targetState` comes from the clicked row's prior state so the
 * anchor's state propagates across the range — same as Gmail / Apple
 * Mail. Returns the original map unchanged (same reference) if either
 * id isn't in `items`, so callers can fall back to a single toggle.
 */
export function toggleRange(
  checked: Checked,
  items: HasId[],
  fromId: number,
  toId: number,
  targetState: boolean
): Checked {
  const from = items.findIndex((m) => m.id === fromId);
  const to = items.findIndex((m) => m.id === toId);
  if (from < 0 || to < 0) return checked;
  const [lo, hi] = from < to ? [from, to] : [to, from];
  const next = { ...checked };
  for (let i = lo; i <= hi; i++) {
    const mid = items[i].id;
    if (targetState) next[mid] = true;
    else delete next[mid];
  }
  return next;
}
