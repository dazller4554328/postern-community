import { writable, get } from 'svelte/store';
import { browser } from '$app/environment';
import { api, type VaultState } from './api';

export const vaultState = writable<VaultState | null>(null);
/**
 * Mirrors the server's view of whether this browser already carries a
 * valid trusted-device cookie. The login gate uses it to suppress the
 * "Remember this device for 30 days" checkbox, since the box is in
 * effect already ticked.
 *
 * Gotcha: the server looks up the cookie in the *encrypted* DB, which
 * isn't readable while the vault is locked — so /api/vault/status
 * always reports `false` on the pre-unlock probe. That's why we also
 * keep `localTrustedDevice` (below), which caches the "I ticked
 * remember" bit client-side so the checkbox disappears as soon as the
 * user enrols, not only after unlock.
 */
export const trustedDevice = writable<boolean>(false);

/**
 * Client-side mirror of the trusted-device state. Set when the user
 * unlocks with `remember=true` and cleared on explicit lock / when the
 * cached 30-day TTL expires. Complements `trustedDevice` to cover the
 * pre-unlock window where the server can't read its encrypted device
 * table.
 */
export const localTrustedDevice = writable<boolean>(false);

const LOCAL_DEVICE_KEY = 'postern.deviceEnrolled';
// Match the server's DEVICE_TTL_SECS (30 days). Kept in sync manually
// — if the server ever changes, bump here too.
const LOCAL_DEVICE_TTL_MS = 30 * 24 * 3600 * 1000;

function readLocalDeviceFlag(): boolean {
  if (!browser) return false;
  try {
    const raw = localStorage.getItem(LOCAL_DEVICE_KEY);
    if (!raw) return false;
    const parsed = JSON.parse(raw) as { enrolledUntil?: number };
    const until = parsed?.enrolledUntil;
    if (typeof until !== 'number') return false;
    if (until <= Date.now()) {
      localStorage.removeItem(LOCAL_DEVICE_KEY);
      return false;
    }
    return true;
  } catch {
    return false;
  }
}

function writeLocalDeviceFlag(enrolled: boolean): void {
  if (!browser) return;
  try {
    if (enrolled) {
      localStorage.setItem(
        LOCAL_DEVICE_KEY,
        JSON.stringify({ enrolledUntil: Date.now() + LOCAL_DEVICE_TTL_MS })
      );
    } else {
      localStorage.removeItem(LOCAL_DEVICE_KEY);
    }
  } catch {
    /* ignore — private mode, quota, etc. */
  }
  localTrustedDevice.set(enrolled);
}

if (browser) {
  // Prime the store from storage so the gate's initial render knows.
  localTrustedDevice.set(readLocalDeviceFlag());
}

export async function refreshVaultState(): Promise<VaultState> {
  const { state, trusted_device } = await api.vaultStatus();
  vaultState.set(state);
  trustedDevice.set(trusted_device);
  // Re-read the local flag — handles the case where the TTL expired
  // between page loads so the checkbox correctly reappears.
  localTrustedDevice.set(readLocalDeviceFlag());
  return state;
}

export async function initVault(password: string) {
  const { state } = await api.vaultInit(password);
  vaultState.set(state);
}

export async function unlockVault(
  password: string,
  rememberDevice = false,
  opts: { totpCode?: string; recoveryCode?: string } = {}
) {
  const { state } = await api.vaultUnlock(password, rememberDevice, opts);
  vaultState.set(state);
  if (rememberDevice) writeLocalDeviceFlag(true);
}

export async function lockVault() {
  const { state } = await api.vaultLock();
  vaultState.set(state);
}

export function currentVaultState(): VaultState | null {
  return get(vaultState);
}

/**
 * Clear the client-side enrolment flag. Called from Settings when the
 * user revokes the current device so the checkbox reappears on the
 * next lock screen.
 */
export function clearLocalTrustedDevice(): void {
  writeLocalDeviceFlag(false);
}
