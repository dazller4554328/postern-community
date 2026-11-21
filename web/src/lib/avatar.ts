import type { Account, RobohashSet } from './api';

export const ROBOHASH_SETS: { id: RobohashSet; label: string; hint: string }[] = [
  { id: 'set1', label: 'Robots', hint: 'Classic RoboHash robots' },
  { id: 'set2', label: 'Monsters', hint: 'Cute monsters' },
  { id: 'set3', label: 'Heads', hint: 'Robot heads' },
  { id: 'set4', label: 'Kittens', hint: 'Kittens' },
  { id: 'set5', label: 'Blocks', hint: 'Geometric blocks' }
];

/// Compose a RoboHash URL from a seed string, chosen set, and render size
/// in pixels (square). Robohash omits the bgset parameter by default, which
/// yields a transparent PNG — what we want so the sidebar chrome shows
/// through the edges of the avatar.
export function robohashUrl(seed: string, set: RobohashSet, size = 64): string {
  const encoded = encodeURIComponent(seed.trim() || 'postern');
  return `https://robohash.org/${encoded}?set=${set}&size=${size}x${size}`;
}

/// Resolve the effective seed + set for an account, respecting user
/// overrides and falling back to the email address.
export function avatarUrl(account: Account, size = 64): string {
  const seed = account.avatar_seed?.trim() || account.email;
  const set = account.avatar_set || 'set1';
  return robohashUrl(seed, set, size);
}

/// Generate a random seed for the "Randomize" button. Uses crypto.getRandomValues
/// where available so collisions are extremely unlikely.
export function randomAvatarSeed(): string {
  const chars = 'abcdefghijklmnopqrstuvwxyz0123456789';
  let out = '';
  if (typeof crypto !== 'undefined' && 'getRandomValues' in crypto) {
    const buf = new Uint32Array(8);
    crypto.getRandomValues(buf);
    for (let i = 0; i < buf.length; i++) out += chars[buf[i] % chars.length];
  } else {
    for (let i = 0; i < 10; i++) out += chars[Math.floor(Math.random() * chars.length)];
  }
  return out;
}
