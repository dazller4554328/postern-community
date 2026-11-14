// New-mail notification primitives.
//
// Phase 1: foreground only — works while a Postern tab is open. The
// module exposes a tiny toast queue, a Web Audio chime, a title-flash
// helper, and an OS-notification dispatcher. Everything is gated on
// prefs the user controls in Settings → Display → Notifications.
//
// Phase 2 (service worker + Web Push) will sit on top of this without
// changing the call sites: the sync layer still says "new mail
// arrived", the dispatcher decides how to surface it.

import { writable, get } from 'svelte/store';
import { browser } from '$app/environment';
import { prefs } from './prefs';

export interface Toast {
  id: number;
  count: number;
  subject?: string;
  from?: string;
}

export const toasts = writable<Toast[]>([]);
let nextToastId = 1;

const TOAST_TTL_MS = 6_000;

function pushToast(t: Toast): void {
  toasts.update((list) => [...list, t]);
  setTimeout(() => {
    toasts.update((list) => list.filter((x) => x.id !== t.id));
  }, TOAST_TTL_MS);
}

export function dismissToast(id: number): void {
  toasts.update((list) => list.filter((x) => x.id !== id));
}

// --- Chime -----------------------------------------------------------------
//
// Rising two-note sine chime. Short, unobtrusive. AudioContext is lazy
// because browsers block audio graph construction before a user
// gesture. We silently no-op if construction fails; the next
// interaction will unlock it.

let ctx: AudioContext | null = null;

function audioCtx(): AudioContext | null {
  if (!browser) return null;
  if (ctx) return ctx;
  try {
    const Ctor = window.AudioContext || (window as unknown as { webkitAudioContext: typeof AudioContext }).webkitAudioContext;
    ctx = new Ctor();
    return ctx;
  } catch {
    return null;
  }
}

function playChime(): void {
  const ac = audioCtx();
  if (!ac) return;
  // If the context got suspended (tab backgrounded, autoplay policy),
  // try to resume. Failure here is fine — we just miss one beep.
  if (ac.state === 'suspended') {
    void ac.resume().catch(() => {
      /* ignore */
    });
  }

  const now = ac.currentTime;
  const master = ac.createGain();
  master.gain.value = 0.18;
  master.connect(ac.destination);

  const tone = (freq: number, start: number, dur: number) => {
    const osc = ac.createOscillator();
    const gain = ac.createGain();
    osc.type = 'sine';
    osc.frequency.value = freq;
    osc.connect(gain).connect(master);
    // Exponential ramps can't touch zero — start and end near-silent
    // instead. Gives a soft attack + decay without clicks.
    gain.gain.setValueAtTime(0.0001, start);
    gain.gain.exponentialRampToValueAtTime(0.9, start + 0.015);
    gain.gain.exponentialRampToValueAtTime(0.0001, start + dur);
    osc.start(start);
    osc.stop(start + dur + 0.02);
  };

  // E5 → A5 — a gentle rising "new mail" motif.
  tone(659.25, now, 0.16);
  tone(880.0, now + 0.11, 0.32);
}

// --- Title flash -----------------------------------------------------------
//
// Swap document.title to an unread-count prefix until the tab becomes
// visible again. Preserves the original title so we can restore it.

let baseTitle: string | null = null;
let unreadBadge = 0;
let visibilityBound = false;

function bindVisibility(): void {
  if (visibilityBound || !browser) return;
  document.addEventListener('visibilitychange', () => {
    if (document.visibilityState === 'visible') {
      unreadBadge = 0;
      if (baseTitle !== null) {
        document.title = baseTitle;
      }
    }
  });
  visibilityBound = true;
}

function flashTitle(addCount: number): void {
  if (!browser) return;
  bindVisibility();
  if (baseTitle === null) baseTitle = document.title || 'Postern';
  // If the tab is already visible the user doesn't need a prefix.
  if (document.visibilityState === 'visible') return;
  unreadBadge += addCount;
  document.title = `(${unreadBadge}) ${baseTitle}`;
}

// --- OS toast --------------------------------------------------------------

export type NotificationPermissionState = 'default' | 'granted' | 'denied' | 'unsupported';

export function osNotificationPermission(): NotificationPermissionState {
  if (!browser) return 'unsupported';
  if (typeof Notification === 'undefined') return 'unsupported';
  return Notification.permission as NotificationPermissionState;
}

export async function requestOsPermission(): Promise<NotificationPermissionState> {
  if (!browser || typeof Notification === 'undefined') return 'unsupported';
  try {
    const result = await Notification.requestPermission();
    return result as NotificationPermissionState;
  } catch {
    return 'denied';
  }
}

function sendOsToast(title: string, body: string): void {
  if (!browser) return;
  if (typeof Notification === 'undefined') return;
  if (Notification.permission !== 'granted') return;
  try {
    // A tag lets repeated arrivals collapse into one OS banner instead
    // of stacking up in the system tray.
    new Notification(title, { body, tag: 'postern-new-mail', icon: '/favicon.svg' });
  } catch {
    /* ignore — some platforms throw when the tab has no focus rights */
  }
}

// --- Entry point ----------------------------------------------------------

export interface NewMailEvent {
  count: number;
  subject?: string | null;
  from?: string | null;
}

export function notifyNewMail(ev: NewMailEvent): void {
  if (!browser) return;
  if (ev.count <= 0) return;

  const p = get(prefs);
  if (!p.notifyNewMail) return;

  // In-app toast — always runs when master is on. It's the one surface
  // that works on every device regardless of permission or audio
  // policy, so it's the safest fallback signal.
  pushToast({
    id: nextToastId++,
    count: ev.count,
    subject: ev.subject ?? undefined,
    from: ev.from ?? undefined
  });

  flashTitle(ev.count);

  if (p.notifySound) playChime();

  if (p.notifyOsToast) {
    const title = ev.count === 1 ? 'New mail' : `${ev.count} new messages`;
    const body = ev.subject
      ? ev.from
        ? `${ev.from}\n${ev.subject}`
        : ev.subject
      : ev.from ?? 'Open Postern to read.';
    sendOsToast(title, body);
  }
}

// --- Reminders ---------------------------------------------------------------
//
// Reused infrastructure: same prefs gate, same chime, same title flash.
// We reuse the `notifyNewMail` preference as the master notification
// switch — a user who turned off new-mail pings almost certainly doesn't
// want reminder pings either. Fine-grained control can land later.

export interface FiredReminder {
  id: number;
  title: string;
  notes?: string | null;
}

export function notifyReminder(r: FiredReminder): void {
  if (!browser) return;
  const p = get(prefs);
  if (!p.notifyNewMail) return;

  flashTitle(1);
  if (p.notifySound) playChime();
  if (p.notifyOsToast) {
    sendOsToast('Reminder', r.notes ? `${r.title}\n${r.notes}` : r.title);
  }
}
