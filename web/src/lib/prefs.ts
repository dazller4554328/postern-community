import { writable } from 'svelte/store';
import { browser } from '$app/environment';

export type Theme =
  | 'system'
  | 'light'
  | 'dark'
  | 'cyberpunk'
  | 'solarized-light'
  | 'solarized-dark'
  | 'dracula'
  | 'nord'
  | 'gruvbox'
  | 'monokai'
  | 'sunset'
  | 'forest'
  | 'rose-pine'
  | 'sepia'
  | 'acid-rain'
  | 'synth-candy'
  | 'volcanic'
  | 'abyssal'
  | 'arcade';

/** Every theme id except 'system'. Used for pre-paint whitelist + pill list. */
export const EXPLICIT_THEMES: Exclude<Theme, 'system'>[] = [
  'light',
  'dark',
  'cyberpunk',
  'solarized-light',
  'solarized-dark',
  'dracula',
  'nord',
  'gruvbox',
  'monokai',
  'sunset',
  'forest',
  'rose-pine',
  'sepia',
  'acid-rain',
  'synth-candy',
  'volcanic',
  'abyssal',
  'arcade'
];

/** Themes that render on a light canvas — used to pick which logo to show. */
export const LIGHT_THEMES: Theme[] = ['light', 'solarized-light', 'sunset', 'sepia', 'synth-candy'];
export type FontId = 'system' | 'serif' | 'rounded' | 'mono';
/**
 * Default view when opening a message.
 *   - plain : Mailpile-style — show extracted text only, no HTML rendering
 *   - html  : sanitized HTML in the sandboxed iframe (our Sprint 2 default)
 *   - source: raw RFC822
 */
export type DefaultView = 'plain' | 'html' | 'source';
export type ListMode = 'messages' | 'threads';
/**
 * How each row in the message list is laid out.
 *   - detailed : sender | subject — snippet | date  (current default)
 *   - compact  : sender | subject | relative-time (hover reveals a snippet preview)
 */
export type RowStyle = 'detailed' | 'compact';
export type SortOption = 'date_desc' | 'date_asc' | 'sender_asc' | 'sender_desc' | 'subject_asc' | 'subject_desc';

export interface Prefs {
  theme: Theme;
  font: FontId;
  defaultView: DefaultView;
  listMode: ListMode;
  rowStyle: RowStyle;
  sort: SortOption;
  /**
   * When true, a thin ticker along the bottom of the app scrolls each
   * new audit + activity event once before dropping it. Useful for
   * keeping an eye on sync cycles + security events without opening the
   * dedicated logs page.
   */
  eventTicker: boolean;
  /** Master switch for new-mail notifications (title flash + in-app toast). */
  notifyNewMail: boolean;
  /** Play a short chime on arrival. Requires one prior user interaction. */
  notifySound: boolean;
  /** Also fire an OS-level notification. Requires browser permission. */
  notifyOsToast: boolean;
  /**
   * Seconds to hold a "Send" in the outbox before dispatch. 0 disables
   * undo; the server still queues the send but the worker picks it up
   * on its next tick (≤ 2s). Max 60 — beyond that, use Send Later
   * explicitly.
   */
  sendUndoSecs: number;
  /** Auto-insert the account signature when composing a reply or forward. */
  signatureOnReplies: boolean;
  /**
   * Hide folders with zero messages (0 total, 0 unread) from the sidebar.
   * Keeps the tree focused on places where mail actually lives. System
   * folders (INBOX, Sent, Drafts, etc.) are always shown regardless.
   */
  hideEmptyFolders: boolean;
  /**
   * When true, the compose pane runs the local Harper grammar / spell
   * checker against the body. Off by default-no, on by default — most
   * users want the safety net. Set false to suppress the suggestion
   * panel and skip loading the wasm bundle.
   */
  composeGrammarCheck: boolean;
}

const DEFAULT: Prefs = {
  theme: 'system',
  font: 'system',
  defaultView: 'html',
  listMode: 'messages',
  rowStyle: 'detailed',
  sort: 'date_desc',
  eventTicker: false,
  notifyNewMail: true,
  notifySound: true,
  notifyOsToast: false,
  sendUndoSecs: 10,
  signatureOnReplies: false,
  hideEmptyFolders: false,
  composeGrammarCheck: true
};
const STORAGE_KEY = 'postern.prefs';

export const FONT_STACKS: Record<FontId, string> = {
  system:
    'ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif',
  serif:
    '"Iowan Old Style", "Palatino Linotype", Palatino, "Book Antiqua", Georgia, serif',
  rounded:
    '"SF Pro Rounded", ui-rounded, "Nunito", "Quicksand", system-ui, sans-serif',
  mono: 'ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, monospace'
};

export const FONT_LABELS: Record<FontId, string> = {
  system: 'System (default)',
  serif: 'Serif — Iowan / Palatino',
  rounded: 'Rounded — SF / Nunito',
  mono: 'Monospace'
};

function load(): Prefs {
  if (!browser) return DEFAULT;
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw) {
      const parsed = JSON.parse(raw);
      return {
        theme: (['system', ...EXPLICIT_THEMES] as Theme[]).includes(parsed.theme) ? parsed.theme : DEFAULT.theme,
        font: (Object.keys(FONT_STACKS) as FontId[]).includes(parsed.font) ? parsed.font : DEFAULT.font,
        defaultView: (['plain', 'html', 'source'] as DefaultView[]).includes(parsed.defaultView)
          ? parsed.defaultView
          : DEFAULT.defaultView,
        listMode: (['messages', 'threads'] as ListMode[]).includes(parsed.listMode)
          ? parsed.listMode
          : DEFAULT.listMode,
        rowStyle: (['detailed', 'compact'] as RowStyle[]).includes(parsed.rowStyle)
          ? parsed.rowStyle
          : DEFAULT.rowStyle,
        sort: (['date_desc', 'date_asc', 'sender_asc', 'sender_desc', 'subject_asc', 'subject_desc'] as SortOption[]).includes(parsed.sort)
          ? parsed.sort
          : DEFAULT.sort,
        eventTicker: typeof parsed.eventTicker === 'boolean' ? parsed.eventTicker : DEFAULT.eventTicker,
        notifyNewMail: typeof parsed.notifyNewMail === 'boolean' ? parsed.notifyNewMail : DEFAULT.notifyNewMail,
        notifySound: typeof parsed.notifySound === 'boolean' ? parsed.notifySound : DEFAULT.notifySound,
        notifyOsToast: typeof parsed.notifyOsToast === 'boolean' ? parsed.notifyOsToast : DEFAULT.notifyOsToast,
        sendUndoSecs:
          typeof parsed.sendUndoSecs === 'number' &&
          Number.isFinite(parsed.sendUndoSecs) &&
          parsed.sendUndoSecs >= 0 &&
          parsed.sendUndoSecs <= 60
            ? Math.round(parsed.sendUndoSecs)
            : DEFAULT.sendUndoSecs,
        signatureOnReplies:
          typeof parsed.signatureOnReplies === 'boolean'
            ? parsed.signatureOnReplies
            : DEFAULT.signatureOnReplies,
        hideEmptyFolders:
          typeof parsed.hideEmptyFolders === 'boolean'
            ? parsed.hideEmptyFolders
            : DEFAULT.hideEmptyFolders,
        composeGrammarCheck:
          typeof parsed.composeGrammarCheck === 'boolean'
            ? parsed.composeGrammarCheck
            : DEFAULT.composeGrammarCheck
      };
    }
  } catch {
    /* ignore */
  }
  return DEFAULT;
}

function apply(p: Prefs) {
  if (!browser) return;
  const root = document.documentElement;
  if (p.theme === 'system') {
    delete root.dataset.theme;
  } else {
    root.dataset.theme = p.theme;
  }
  root.style.setProperty('--font-ui', FONT_STACKS[p.font]);
}

export const prefs = writable<Prefs>(load());

if (browser) {
  // The pre-paint script in app.html applies the initial values before
  // any CSS runs — this subscriber picks up later changes from the
  // settings UI and persists them.
  prefs.subscribe((p) => {
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(p));
    } catch {
      /* ignore */
    }
    apply(p);
  });
}
