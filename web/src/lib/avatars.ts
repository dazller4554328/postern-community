// Global "remote sender-avatar lookups" opt-in. Off by default — a
// lookup tells Libravatar/Gravatar/DuckDuckGo/the sender's domain that
// this mailbox is viewing this sender. The server enforces the same
// flag (see avatar.rs); this store is the UI mirror so SenderAvatar
// doesn't even fire the request when the user hasn't opted in.

import { writable } from 'svelte/store';
import { browser } from '$app/environment';
import { api } from '$lib/api';

export const remoteAvatars = writable(false);

let hydrated = false;

/** Fetch the server's value once per session. Safe to call from every
 *  SenderAvatar mount — only the first call hits the network. */
export async function ensureRemoteAvatarSetting(): Promise<void> {
  if (!browser || hydrated) return;
  hydrated = true;
  try {
    const r = await api.getRemoteAvatars();
    remoteAvatars.set(!!r.enabled);
  } catch {
    // Leave it off on error — privacy is the safe default.
  }
}

/** Persist + apply a new value (called from Settings → Privacy). */
export async function setRemoteAvatars(enabled: boolean): Promise<void> {
  await api.setRemoteAvatars(enabled);
  remoteAvatars.set(enabled);
  hydrated = true;
}
