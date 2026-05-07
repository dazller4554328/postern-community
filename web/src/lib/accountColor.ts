// Per-account display-colour helper.
//
// The user can set a custom colour per mailbox in Settings → Mailboxes;
// when that's null we fall back to a deterministic palette pick based
// on the account id, so the same mailbox keeps the same colour across
// reloads even before they've customised anything.
//
// Palette is the same one used by SenderAvatar — picks that look fine
// on both light and dark canvases and don't muddy together when two
// pills sit next to each other.

import type { Account } from './api';

export const ACCOUNT_COLOR_PALETTE: readonly string[] = [
  '#3b82f6', // blue
  '#10b981', // emerald
  '#f59e0b', // amber
  '#ef4444', // red
  '#8b5cf6', // violet
  '#ec4899', // pink
  '#14b8a6', // teal
  '#6366f1', // indigo
  '#84cc16', // lime
  '#f97316'  // orange
];

/**
 * Resolve the colour to render for a given account. Honours the
 * persisted `color` field; falls back to a deterministic palette pick
 * keyed off `id` so a freshly-added mailbox lights up immediately.
 */
export function colorForAccount(account: Pick<Account, 'id' | 'color'>): string {
  if (account.color && /^#[0-9a-fA-F]{6}$/.test(account.color)) {
    return account.color.toLowerCase();
  }
  return ACCOUNT_COLOR_PALETTE[
    Math.abs(account.id) % ACCOUNT_COLOR_PALETTE.length
  ];
}

/**
 * Build a quick lookup table from a list of accounts. Inbox rows look
 * up by `account_id` per row, so we want O(1) access not a linear
 * scan per row.
 */
export function buildAccountColorMap(
  accounts: Pick<Account, 'id' | 'color'>[]
): Record<number, string> {
  const out: Record<number, string> = {};
  for (const a of accounts) {
    out[a.id] = colorForAccount(a);
  }
  return out;
}
