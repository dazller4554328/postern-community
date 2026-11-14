// Inbox list data + loading: owns the message / search-hit arrays and
// their load/paginate/poll lifecycle, plus the small optimistic mutators
// the page's action handlers drive (mark-read, bulk-remove, clear, …).
// Extracted from inbox/+page.svelte so the view component is left with
// navigation, selection and the action handlers rather than also being
// the data store.
//
// The query context (search vs folder, account, unified bucket, sort) is
// injected as a getter so the loaders read fresh URL-derived state each
// call without this module depending on $app or the page's $derived.

import { api, type MessageListItem, type SearchHit } from './api';
import { UNIFIED_LABELS, type UnifiedSystem } from './unified';
import type { SortOption } from './prefs';

const PAGE_SIZE = 50;

export interface QueryContext {
  isSearching: boolean;
  query: string;
  accountId: number | null;
  folder: string | null;
  unified: UnifiedSystem | null;
  sort: SortOption;
}

export function useInboxMessages(ctx: () => QueryContext) {
  let messages = $state<MessageListItem[]>([]);
  let searchHits = $state<SearchHit[]>([]);
  let loading = $state(true);
  let loadingMore = $state(false);
  let err = $state<string | null>(null);
  let hasMore = $state(true);
  // Monotonic load token. Each fresh (non-append) load claims a new
  // generation; a response whose generation is stale by the time it
  // resolves is dropped, so fast folder/account switching can't let an
  // older fetch overwrite the newer view's rows.
  let generation = 0;

  /** Fetch the current page (or append the next page) for the active
   *  query context. On search, fills `searchHits`; otherwise `messages`. */
  async function load(append = false) {
    const c = ctx();
    const gen = append ? generation : ++generation;
    if (!append) {
      messages = [];
      searchHits = [];
      hasMore = true;
    }
    try {
      if (c.isSearching) {
        const hits = await api.search({
          q: c.query,
          account_id: c.accountId ?? undefined,
          limit: PAGE_SIZE,
          offset: append ? searchHits.length : 0,
          sort: c.sort,
          // Only surface Trash/Spam rows when the user is actually
          // looking at those buckets — otherwise a global search keeps
          // re-returning the rows they just deleted.
          include_trash_spam: c.unified === 'trash' || c.unified === 'spam',
        });
        if (gen !== generation) return;
        searchHits = append ? [...searchHits, ...hits] : hits;
        hasMore = hits.length === PAGE_SIZE;
      } else {
        const msgs = await api.listMessages({
          account_id: c.accountId ?? undefined,
          label: c.folder ?? undefined,
          labels: c.unified ? UNIFIED_LABELS[c.unified] : undefined,
          limit: PAGE_SIZE,
          offset: append ? messages.length : 0,
          sort: c.sort,
        });
        if (gen !== generation) return;
        messages = append ? [...messages, ...msgs] : msgs;
        hasMore = msgs.length === PAGE_SIZE;
      }
      err = null;
    } catch (e) {
      if (gen !== generation) return;
      err = e instanceof Error ? e.message : String(e);
    }
  }

  async function loadMore() {
    loadingMore = true;
    try {
      await load(true);
    } finally {
      loadingMore = false;
    }
  }

  /** Fetch the first page and merge only genuinely-new rows (unseen ids)
   *  onto the top, so the user's scroll/hover/keyed state survives. The
   *  caller gates this on visibility + non-search before calling. */
  async function poll() {
    const c = ctx();
    const gen = generation;
    try {
      const fresh = await api.listMessages({
        account_id: c.accountId ?? undefined,
        label: c.folder ?? undefined,
        labels: c.unified ? UNIFIED_LABELS[c.unified] : undefined,
        limit: PAGE_SIZE,
        offset: 0,
        sort: c.sort,
      });
      // A load() (folder/account switch) since we started owns the list
      // now — merging our rows would mix two queries' results.
      if (gen !== generation) return;
      const existing = new Set(messages.map((m) => m.id));
      const newOnes = fresh.filter((m) => !existing.has(m.id));
      if (newOnes.length > 0) messages = [...newOnes, ...messages];
    } catch {
      // Transient network errors, locked vault, etc. — next tick retries.
    }
  }

  // ── Optimistic mutators (driven by the page's action handlers) ──────
  /** Flip one row to read in whichever list holds it (no-op if absent). */
  function markReadLocal(id: number) {
    if (messages.some((m) => m.id === id && !m.is_read))
      messages = messages.map((m) =>
        m.id === id ? { ...m, is_read: true } : m,
      );
    if (searchHits.some((m) => m.id === id && !m.is_read))
      searchHits = searchHits.map((m) =>
        m.id === id ? { ...m, is_read: true } : m,
      );
  }

  /** Set the read flag on a set of ids across both lists. */
  function setReadLocal(ids: number[], flag: boolean) {
    const s = new Set(ids);
    messages = messages.map((m) => (s.has(m.id) ? { ...m, is_read: flag } : m));
    searchHits = searchHits.map((m) =>
      s.has(m.id) ? { ...m, is_read: flag } : m,
    );
  }

  /** Flip every visible row to read (folder mark-all-read). */
  function markAllReadLocal() {
    messages = messages.map((m) => ({ ...m, is_read: true }));
    searchHits = searchHits.map((m) => ({ ...m, is_read: true }));
  }

  /** Drop a set of ids from both lists (move/trash/archive). */
  function removeLocal(ids: number[]) {
    const s = new Set(ids);
    messages = messages.filter((m) => !s.has(m.id));
    searchHits = searchHits.filter((m) => !s.has(m.id));
  }

  /** Empty both lists (folder-empty). */
  function clear() {
    messages = [];
    searchHits = [];
  }

  return {
    get messages() {
      return messages;
    },
    get searchHits() {
      return searchHits;
    },
    get loadingMore() {
      return loadingMore;
    },
    get hasMore() {
      return hasMore;
    },
    get loading() {
      return loading;
    },
    set loading(v: boolean) {
      loading = v;
    },
    get err() {
      return err;
    },
    set err(v: string | null) {
      err = v;
    },
    load,
    loadMore,
    poll,
    markReadLocal,
    setReadLocal,
    markAllReadLocal,
    removeLocal,
    clear,
  };
}
