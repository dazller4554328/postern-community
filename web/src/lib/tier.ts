// Shared reactive store for the build tier. Fetched once per page
// load and cached — the answer is a compile-time constant on the
// server, so refetching is waste.

import { writable, type Writable } from 'svelte/store';
import { api, type TierInfo } from './api';

const INITIAL: TierInfo = {
  // Defaults lean "pro-like" so the UI doesn't flash "community"
  // gates for a frame before the real answer arrives. On free the
  // first fetch flips it to the real values within one tick.
  tier: 'pro',
  max_mailboxes: null,
  max_send_delay_secs: null,
  features: {
    vpn: true,
    trusted_devices: true,
    licensed_updates: true,
    gmail_categories_purge: true,
    server_retention: true,
    auto_archive: true,
    mail_import: true,
    ai: true
  }
};

export const tier: Writable<TierInfo> = writable(INITIAL);

let fetched = false;
/**
 * Kick the one-shot fetch. Safe to call from multiple places —
 * subsequent calls are no-ops.
 */
export function ensureTierLoaded(): void {
  if (fetched) return;
  fetched = true;
  void api
    .tier()
    .then((t) => tier.set(t))
    .catch(() => {
      // Endpoint is always-available (on both tiers, through the
      // lock guard), so a failure here usually means the server
      // isn't running yet. Reset the flag so the next caller can
      // retry.
      fetched = false;
    });
}
