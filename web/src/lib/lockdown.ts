// Lockdown-mode store. Single source of truth for the on/off
// state across components — inbox toolbar, MessageBody buttons,
// Compose, Settings panel — so flipping it from one tab updates
// every UI surface without per-component fetches.
//
// The actual enforcement is server-side; this store is purely a
// UX optimisation that mirrors the server flag client-side.

import { writable, get } from 'svelte/store';
import { api } from '$lib/api';

interface LockdownState {
  enabled: boolean;
  /// True after the first successful status fetch — components
  /// can use this to avoid flicker on first render.
  loaded: boolean;
}

const initial: LockdownState = { enabled: false, loaded: false };
export const lockdown = writable<LockdownState>(initial);

let refreshInFlight: Promise<void> | null = null;

export async function refreshLockdown(): Promise<void> {
  // De-dupe parallel fetches (the inbox + Settings can both
  // trigger this on mount). Returns the existing promise when
  // one is already in flight.
  if (refreshInFlight) return refreshInFlight;
  refreshInFlight = (async () => {
    try {
      const r = await api.lockdownStatus();
      lockdown.set({ enabled: r.enabled, loaded: true });
    } catch {
      // Transient — leave the previous state untouched but mark
      // loaded so consumers stop showing skeletons.
      lockdown.update((s) => ({ ...s, loaded: true }));
    } finally {
      refreshInFlight = null;
    }
  })();
  return refreshInFlight;
}

export async function setLockdown(enabled: boolean): Promise<boolean> {
  const r = await api.lockdownSet(enabled);
  lockdown.set({ enabled: r.enabled, loaded: true });
  return r.enabled;
}

/// Synchronous accessor — useful from event handlers that need
/// to gate an action without awaiting.
export function isLockdownOn(): boolean {
  return get(lockdown).enabled;
}
