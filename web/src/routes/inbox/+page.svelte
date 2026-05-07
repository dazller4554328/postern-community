<script lang="ts">
  import './styles.css';
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { page } from '$app/stores';
  import {
    api,
    type Account,
    type FoldersResponse,
    type MessageListItem,
    type SearchHit,
    type ThreadSummary
  } from '$lib/api';
  import { buildAccountColorMap } from '$lib/accountColor';
  import { formatDate, formatRelative, formatSender } from '$lib/format';
  import FolderPicker from '$lib/components/FolderPicker.svelte';
  import Sidebar from '$lib/components/Sidebar.svelte';
  import MessageBody from '$lib/components/MessageBody.svelte';
  import ThreadView from '$lib/components/ThreadView.svelte';
  import VpnBadge from '$lib/components/VpnBadge.svelte';
  import AskBox from '$lib/components/inbox/AskBox.svelte';
  import SenderAvatar from '$lib/components/SenderAvatar.svelte';
  import { tier } from '$lib/tier';
  import { lockdown, refreshLockdown } from '$lib/lockdown';
  import { prefs, type ListMode, type RowStyle, type SortOption } from '$lib/prefs';
  import {
    isUnifiedSystem,
    UNIFIED_DISPLAY,
    UNIFIED_LABELS,
    type UnifiedSystem
  } from '$lib/unified';
  import { lockVault } from '$lib/vault';

  type QuickFilter = 'all' | 'unread' | 'starred' | 'attachments';
  type SplitOrient = 'vertical' | 'horizontal';
  type Density = 'compact' | 'normal' | 'comfortable';

  let folders = $state<FoldersResponse | null>(null);
  let accounts = $state<Account[]>([]);
  // Per-account colour, refreshed alongside the accounts list. Drives
  // the unread pill on each row so the user can see at a glance which
  // mailbox a message landed in. Falls back to a deterministic palette
  // pick when the user hasn't customised the colour.
  let accountColorMap = $derived(buildAccountColorMap(accounts));
  let messages = $state<MessageListItem[]>([]);
  let searchHits = $state<SearchHit[]>([]);
  let threads = $state<ThreadSummary[]>([]);
  let loading = $state(true);
  let loadingMore = $state(false);
  let err = $state<string | null>(null);
  let hasMore = $state(true);

  // List mode and row style are global preferences (persisted in settings).
  let listMode = $state<ListMode>('messages');
  let rowStyle = $state<RowStyle>('detailed');
  let sort = $state<SortOption>('date_desc');
  $effect(() => {
    const unsub = prefs.subscribe((p) => {
      listMode = p.listMode;
      rowStyle = p.rowStyle;
      sort = p.sort;
    });
    return unsub;
  });
  function toggleListMode() {
    prefs.update((p) => ({ ...p, listMode: p.listMode === 'threads' ? 'messages' : 'threads' }));
  }
  function toggleRowStyle() {
    prefs.update((p) => ({ ...p, rowStyle: p.rowStyle === 'detailed' ? 'compact' : 'detailed' }));
  }

  // Layout prefs — persisted in localStorage.
  let splitOrient = $state<SplitOrient>('vertical');
  let density = $state<Density>('normal');
  let quickFilter = $state<QuickFilter>('all');
  // Resizable column widths / list pane height (in px). Clamped on drag.
  let sidebarWidth = $state(288);
  let listWidth = $state(480);
  let listHeight = $state(360);
  let fromWidth = $state(160); // px — column width for the sender cell
  let sidebarHidden = $state(false);

  const MIN_SIDEBAR = 240;
  const MAX_SIDEBAR = 480;
  const MIN_LIST = 280;
  const MAX_LIST = 900;
  const MIN_LIST_HEIGHT = 160;
  const MAX_LIST_HEIGHT = 900;
  const MIN_FROM = 80;
  const MAX_FROM = 420;

  type ResizeTarget = 'sidebar' | 'list-x' | 'list-y' | 'from';

  function startResize(target: ResizeTarget, e: PointerEvent) {
    e.preventDefault();
    e.stopPropagation();
    const axis: 'x' | 'y' = target === 'list-y' ? 'y' : 'x';
    const start = axis === 'x' ? e.clientX : e.clientY;
    const startW =
      target === 'sidebar'
        ? sidebarWidth
        : target === 'list-x'
          ? listWidth
          : target === 'from'
            ? fromWidth
            : listHeight;
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);

    const onMove = (ev: PointerEvent) => {
      const delta = (axis === 'x' ? ev.clientX : ev.clientY) - start;
      if (target === 'sidebar') {
        sidebarWidth = Math.max(MIN_SIDEBAR, Math.min(MAX_SIDEBAR, startW + delta));
      } else if (target === 'list-x') {
        listWidth = Math.max(MIN_LIST, Math.min(MAX_LIST, startW + delta));
      } else if (target === 'from') {
        fromWidth = Math.max(MIN_FROM, Math.min(MAX_FROM, startW + delta));
      } else {
        listHeight = Math.max(MIN_LIST_HEIGHT, Math.min(MAX_LIST_HEIGHT, startW + delta));
      }
    };
    const onUp = (ev: PointerEvent) => {
      el.releasePointerCapture(ev.pointerId);
      el.removeEventListener('pointermove', onMove);
      el.removeEventListener('pointerup', onUp);
      el.removeEventListener('pointercancel', onUp);
    };
    el.addEventListener('pointermove', onMove);
    el.addEventListener('pointerup', onUp);
    el.addEventListener('pointercancel', onUp);
  }

  function toggleSidebar() {
    sidebarHidden = !sidebarHidden;
  }

  // True on phones and narrow tablets. Drives drawer-style sidebar +
  // single-pane navigation. SSR-safe — window is guarded.
  let isMobile = $state(false);
  onMount(() => {
    const mq = window.matchMedia('(max-width: 900px)');
    const sync = () => {
      const next = mq.matches;
      if (next !== isMobile) isMobile = next;
      // First-time on mobile we want the drawer closed regardless of
      // whatever desktop preference was persisted.
      if (next && !sidebarHidden) sidebarHidden = true;
      if (!next) mobileToolbarCollapsed = false;
    };
    sync();
    mq.addEventListener('change', sync);
    return () => mq.removeEventListener('change', sync);
  });

  // Auto-close the mobile drawer whenever navigation happens — folder
  // change, message open, unified-view switch. Desktop is unaffected.
  $effect(() => {
    // Depend on URL params so any navigation triggers this.
    activeAccount;
    activeFolder;
    activeUnified;
    selectedId;
    selectedThread;
    if (isMobile && !sidebarHidden) {
      sidebarHidden = true;
    }
  });

  // Top-bar "send & receive" — force an incremental sync for one
  // specific account or every account at once. "all" at start so the
  // common case (anything new anywhere?) is one click. Remembers the
  // last choice across reloads.
  let syncTarget = $state<'all' | number>('all');
  let syncing = $state(false);
  let mobileToolbarCollapsed = $state(false);
  let mobileToolbarDragStartY = $state<number | null>(null);

  function toggleMobileToolbar() {
    mobileToolbarCollapsed = !mobileToolbarCollapsed;
  }

  function beginMobileToolbarGesture(e: PointerEvent) {
    mobileToolbarDragStartY = e.clientY;
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
  }

  function endMobileToolbarGesture(e: PointerEvent) {
    const startY = mobileToolbarDragStartY;
    mobileToolbarDragStartY = null;
    const el = e.currentTarget as HTMLElement;
    if (el.hasPointerCapture(e.pointerId)) {
      el.releasePointerCapture(e.pointerId);
    }
    if (startY === null) return;
    const deltaY = e.clientY - startY;
    if (deltaY <= -24) mobileToolbarCollapsed = true;
    if (deltaY >= 24) mobileToolbarCollapsed = false;
  }

  async function forceSync() {
    if (syncing) return;
    const picked = syncTarget;
    syncing = true;
    try {
      if (picked === 'all') {
        const accounts = await api.listAccounts();
        await Promise.all(accounts.map((a) => api.triggerSync(a.id).catch(() => null)));
      } else {
        await api.triggerSync(picked);
      }
      // Give the scheduler a beat to pull, then refresh folder counts + list.
      await new Promise((r) => setTimeout(r, 1200));
      await refresh();
    } finally {
      syncing = false;
    }
  }

  const PAGE_SIZE = 50;

  // URL-driven state
  let activeAccount = $derived(
    $page.url.searchParams.get('account') ? Number($page.url.searchParams.get('account')) : null
  );
  let activeFolder = $derived($page.url.searchParams.get('folder'));
  let activeQuery = $derived($page.url.searchParams.get('q') ?? '');
  let isSearching = $derived(activeQuery.length > 0);
  // Unified cross-account system: ?u=inbox|sent|drafts|spam|trash
  let activeUnified = $derived.by<UnifiedSystem | null>(() => {
    const raw = $page.url.searchParams.get('u');
    return isUnifiedSystem(raw) ? raw : null;
  });
  let selectedId = $derived(
    $page.url.searchParams.get('m') ? Number($page.url.searchParams.get('m')) : null
  );
  // Threaded view uses ?t=<thread_id>. One or the other is set, never both.
  let selectedThread = $derived($page.url.searchParams.get('t'));
  let hasSelection = $derived(selectedId !== null || selectedThread !== null);

  // Multi-select: Set of message ids the user has checked. Kept as
  // a plain object because Svelte reactive Sets land awkwardly in
  // Svelte 5 — map-of-boolean reads the same in templates, commits
  // to state cleanly, and $derived() can recompute from it.
  let checked = $state<Record<number, true>>({});
  let lastCheckedId = $state<number | null>(null);
  let bulkBusy = $state(false);
  // The shift-click range pivot. Cleared when the filter/label
  // changes so we don't try to fill a range across a list refresh.
  let checkedIds = $derived(
    Object.keys(checked).map(Number).filter((n) => Number.isFinite(n))
  );
  let checkedCount = $derived(checkedIds.length);

  async function loadMessages(append = false) {
    if (!append) {
      messages = [];
      searchHits = [];
      threads = [];
      hasMore = true;
    }

    try {
      if (isSearching) {
        const hits = await api.search({
          q: activeQuery,
          account_id: activeAccount ?? undefined,
          limit: PAGE_SIZE,
          offset: append ? searchHits.length : 0,
          sort
        });
        searchHits = append ? [...searchHits, ...hits] : hits;
        hasMore = hits.length === PAGE_SIZE;
      } else if (listMode === 'threads') {
        const t = await api.listThreads({
          account_id: activeAccount ?? undefined,
          label: activeFolder ?? undefined,
          labels: activeUnified ? UNIFIED_LABELS[activeUnified] : undefined,
          limit: PAGE_SIZE,
          offset: append ? threads.length : 0
        });
        threads = append ? [...threads, ...t] : t;
        hasMore = t.length === PAGE_SIZE;
      } else {
        const msgs = await api.listMessages({
          account_id: activeAccount ?? undefined,
          label: activeFolder ?? undefined,
          labels: activeUnified ? UNIFIED_LABELS[activeUnified] : undefined,
          limit: PAGE_SIZE,
          offset: append ? messages.length : 0,
          sort
        });
        messages = append ? [...messages, ...msgs] : msgs;
        hasMore = msgs.length === PAGE_SIZE;
      }
      err = null;
    } catch (e) {
      err = e instanceof Error ? e.message : String(e);
    }
  }

  async function loadFolders() {
    try {
      folders = await api.folders();
    } catch (e) {
      console.error('folders load failed', e);
    }
  }

  async function refresh() {
    await Promise.all([loadMessages(), loadFolders()]);
  }

  // --- Silent auto-refresh ---
  // Every AUTO_POLL_MS while the tab is visible, fetch the first page
  // for the current filter context and merge any genuinely-new rows
  // (unseen IDs) into the top of the list. Existing rows keep their
  // position and keyed identity so the user's scroll and hover state
  // survive. Also refreshes folder counts so unread badges track.
  const AUTO_POLL_MS = 30_000;
  let autoPollHandle: ReturnType<typeof setInterval> | null = null;

  async function silentPoll() {
    if (typeof document !== 'undefined' && document.visibilityState !== 'visible') return;
    // Don't disturb searches — they're scoped, user-driven, and a
    // refresh that rescopes mid-read would be jarring.
    if (isSearching) return;
    try {
      if (listMode === 'threads') {
        const fresh = await api.listThreads({
          account_id: activeAccount ?? undefined,
          label: activeFolder ?? undefined,
          labels: activeUnified ? UNIFIED_LABELS[activeUnified] : undefined,
          limit: PAGE_SIZE,
          offset: 0
        });
        const existing = new Set(threads.map((t) => t.thread_id));
        const newOnes = fresh.filter((t) => !existing.has(t.thread_id));
        if (newOnes.length > 0) {
          threads = [...newOnes, ...threads];
        }
      } else {
        const fresh = await api.listMessages({
          account_id: activeAccount ?? undefined,
          label: activeFolder ?? undefined,
          labels: activeUnified ? UNIFIED_LABELS[activeUnified] : undefined,
          limit: PAGE_SIZE,
          offset: 0,
          sort
        });
        const existing = new Set(messages.map((m) => m.id));
        const newOnes = fresh.filter((m) => !existing.has(m.id));
        if (newOnes.length > 0) {
          messages = [...newOnes, ...messages];
        }
      }
      // Count/unread badges in the sidebar should reflect new arrivals
      // even when the user's looking at a different folder.
      await loadFolders();
    } catch {
      // Transient network errors, locked vault, etc. — next tick retries.
    }
  }

  async function loadMore() {
    loadingMore = true;
    try {
      await loadMessages(true);
    } finally {
      loadingMore = false;
    }
  }

  function handleSearch(q: string) {
    const url = new URL('/inbox', window.location.origin);
    if (q) url.searchParams.set('q', q);
    if (activeAccount !== null) url.searchParams.set('account', String(activeAccount));
    goto(url.pathname + url.search);
  }

  // Click-on-sender shortcut: jump the inbox into a search view that
  // lists every message from that address. Quoted so addresses that
  // contain dots or dashes survive the FTS5 tokenizer as a phrase.
  function filterBySender(addr: string | null | undefined) {
    if (!addr) return;
    handleSearch(`from:"${addr}"`);
  }

  function selectMessage(id: number) {
    const url = new URL($page.url);
    url.searchParams.set('m', String(id));
    url.searchParams.delete('t');
    goto(url.pathname + url.search, { replaceState: true, noScroll: true, keepFocus: true });
    markRead(id);
  }

  // Fire-and-forget: flip the row locally (optimistic) and tell the server.
  // Don't revert on failure — next list refresh will heal any drift, and a
  // transient 5xx shouldn't flicker the envelope back to "unread" under the user.
  function markRead(id: number) {
    const target = messages.find((m) => m.id === id);
    if (target && !target.is_read) {
      messages = messages.map((m) => (m.id === id ? { ...m, is_read: true } : m));
    }
    const hit = searchHits.find((m) => m.id === id);
    if (hit && !hit.is_read) {
      searchHits = searchHits.map((m) => (m.id === id ? { ...m, is_read: true } : m));
    }
    api.setMessageRead(id, true).catch(() => {
      /* swallow — optimistic UI wins until next refresh */
    });
  }

  function selectThread(id: string) {
    const url = new URL($page.url);
    url.searchParams.set('t', id);
    url.searchParams.delete('m');
    goto(url.pathname + url.search, { replaceState: true, noScroll: true, keepFocus: true });
  }

  // ── Multi-select helpers ────────────────────────────────────────────
  // Clear selection whenever the list context changes — moving between
  // folders / search / account filters implies the user's done with the
  // prior batch.
  $effect(() => {
    activeFolder;
    activeAccount;
    activeUnified;
    activeQuery;
    checked = {};
    lastCheckedId = null;
  });

  function isChecked(id: number): boolean {
    return Boolean(checked[id]);
  }

  function toggleChecked(id: number, event: MouseEvent | KeyboardEvent) {
    const isShift = event.shiftKey;
    // Shift-click extends the last checkbox action to cover the range
    // between lastCheckedId and id. Whichever state the anchor is in
    // propagates to the range — same as Gmail / Apple Mail.
    if (isShift && lastCheckedId !== null) {
      const items = filteredItems();
      const from = items.findIndex((m) => m.id === lastCheckedId);
      const to = items.findIndex((m) => m.id === id);
      if (from >= 0 && to >= 0) {
        const [lo, hi] = from < to ? [from, to] : [to, from];
        const targetState = !isChecked(id);
        const next = { ...checked };
        for (let i = lo; i <= hi; i++) {
          const mid = items[i].id;
          if (targetState) next[mid] = true;
          else delete next[mid];
        }
        checked = next;
        lastCheckedId = id;
        return;
      }
    }
    const next = { ...checked };
    if (next[id]) {
      delete next[id];
    } else {
      next[id] = true;
    }
    checked = next;
    lastCheckedId = id;
  }

  let allVisibleChecked = $derived.by(() => {
    const items = filteredItems();
    if (items.length === 0) return false;
    return items.every((m) => checked[m.id]);
  });
  let someVisibleChecked = $derived(checkedCount > 0 && !allVisibleChecked);

  function selectAllVisible() {
    const items = filteredItems();
    if (items.length === 0) return;
    if (allVisibleChecked) {
      checked = {};
      lastCheckedId = null;
      return;
    }
    const next: Record<number, true> = {};
    for (const m of items) next[m.id] = true;
    checked = next;
  }

  function clearSelection() {
    checked = {};
    lastCheckedId = null;
  }

  // Bulk actions. Optimistic: remove moved rows from the UI
  // immediately, call the bulk endpoint, re-check afterwards so any
  // failures get restored by the next refresh.
  /// Multi-select Move-to-folder. Opens the FolderPicker, then
  /// fires `bulkMoveTo` with whichever folder the user picked. We
  /// require all selected messages to belong to the same account
  /// so the picker has an unambiguous folder list to show — if the
  /// selection spans accounts the button stays disabled and an
  /// inline tooltip explains why.
  let moveBulkOpen = $state(false);
  function bulkMoveAccountId(): number | null {
    if (checkedIds.length === 0) return null;
    const all = [...messages, ...searchHits];
    const selected = all.filter((m) => checked[m.id]);
    if (selected.length === 0) return null;
    const first = selected[0].account_id;
    return selected.every((m) => m.account_id === first) ? first : null;
  }
  function bulkMoveExclude(): string[] {
    // Don't list the active folder in the picker — moving "to itself"
    // is a no-op and just clutters the menu.
    return activeFolder ? [activeFolder] : [];
  }
  async function bulkMovePicked(folder: string) {
    const ids = checkedIds.slice();
    moveBulkOpen = false;
    if (ids.length === 0) return;
    bulkBusy = true;
    const idSet = new Set(ids);
    // Optimistic: same as runBulk for trash/archive — selected rows
    // disappear from the current view.
    messages = messages.filter((m) => !idSet.has(m.id));
    searchHits = searchHits.filter((m) => !idSet.has(m.id));
    checked = {};
    lastCheckedId = null;
    try {
      await api.bulkMoveTo(ids, folder);
    } catch (e) {
      err = e instanceof Error ? e.message : String(e);
      await loadMessages();
    } finally {
      bulkBusy = false;
    }
  }

  async function runBulk(action: 'trash' | 'archive' | 'read' | 'unread' | 'spam' | 'notspam') {
    if (bulkBusy || checkedCount === 0) return;
    const ids = checkedIds;
    if (action === 'trash' && !confirm(`Move ${ids.length} message${ids.length === 1 ? '' : 's'} to Trash?`)) {
      return;
    }
    if (action === 'spam' && !confirm(`Move ${ids.length} message${ids.length === 1 ? '' : 's'} to Spam?`)) {
      return;
    }
    bulkBusy = true;
    const isMove = action === 'trash' || action === 'archive' || action === 'spam' || action === 'notspam';
    // Optimistic local update so the UI feels instant.
    const idSet = new Set(ids);
    if (isMove) {
      messages = messages.filter((m) => !idSet.has(m.id));
      searchHits = searchHits.filter((m) => !idSet.has(m.id));
    } else {
      const readFlag = action === 'read';
      messages = messages.map((m) => (idSet.has(m.id) ? { ...m, is_read: readFlag } : m));
      searchHits = searchHits.map((m) => (idSet.has(m.id) ? { ...m, is_read: readFlag } : m));
    }
    checked = {};
    lastCheckedId = null;
    try {
      await api.bulkAction(ids, action);
    } catch (e) {
      err = e instanceof Error ? e.message : String(e);
      // Any half-applied state will heal on the next poll / refresh.
      await loadMessages();
    } finally {
      bulkBusy = false;
    }
  }

  // Folder-level actions — act on every message currently tagged with
  // the active folder label on the active account, regardless of
  // pagination or filter state. Gmail / Outlook / Thunderbird all
  // expose these as a separate control from per-message bulk because
  // "everything in Trash" can easily be thousands of rows you don't
  // want to individually check.
  let folderActionBusy = $state(false);
  async function markFolderRead() {
    if (folderActionBusy) return;
    if (activeAccount == null || !activeFolder) return;
    folderActionBusy = true;
    try {
      await api.markFolderRead(activeAccount, activeFolder);
      // Optimistic: flip is_read on everything visible. Next poll
      // picks up the full set from the server.
      messages = messages.map((m) => ({ ...m, is_read: true }));
      searchHits = searchHits.map((m) => ({ ...m, is_read: true }));
    } catch (e) {
      err = e instanceof Error ? e.message : String(e);
      await loadMessages();
    } finally {
      folderActionBusy = false;
    }
  }

  async function emptyActiveFolder() {
    if (folderActionBusy) return;
    if (activeAccount == null || !activeFolder) return;
    const label = activeFolder;
    if (isDisposalFolder(label)) {
      // Trash / Spam — one confirm is enough, these are disposal folders.
      if (!confirm(
        `Permanently delete EVERY message in ${label}?\n\n` +
        `This cannot be undone. Both the local copy and the copy on ` +
        `the mail server are removed.`
      )) return;
    } else {
      // User label / archive folder — require the user to type the
      // folder name, so a misread dialog can't nuke real mail.
      const typed = prompt(
        `This will PERMANENTLY DELETE every message currently in "${label}" — ` +
        `both locally and on the mail server. Messages tagged with other labels on ` +
        `Gmail will lose the "${label}" label but stay under their other labels.\n\n` +
        `Type the folder name to confirm:`
      );
      if (typed !== label) {
        if (typed != null) err = `Empty folder cancelled — "${typed}" doesn't match "${label}".`;
        return;
      }
    }
    folderActionBusy = true;
    try {
      const r = await api.emptyFolder(activeAccount, label);
      messages = [];
      searchHits = [];
      checked = {};
      lastCheckedId = null;
      // No explicit success banner — the visibly-empty list + the
      // confirmation prompt the user just cleared is enough feedback.
      // Using the `err` slot for success messages conflates states and
      // trips screen readers that announce role="alert" semantics.
      // `r.deleted` is logged server-side via folder_emptied audit.
      void r;
      await loadMessages();
    } catch (e) {
      err = e instanceof Error ? e.message : String(e);
    } finally {
      folderActionBusy = false;
    }
  }

  // Mirror the backend's protected-folder list so the button isn't
  // offered where the server would reject the request.
  const PROTECTED_FOLDERS = new Set([
    'inbox',
    'sent',
    'drafts',
    'sent items',
    'sent messages',
    '[gmail]/sent mail',
    '[gmail]/drafts',
    '[gmail]/all mail',
    '[gmail]/important',
    '[gmail]/starred'
  ]);

  let canEmptyFolder = $derived.by(() => {
    if (!activeFolder) return false;
    return !PROTECTED_FOLDERS.has(activeFolder.toLowerCase());
  });

  // Whether the current folder is one where "empty" is a routine
  // operation (Gmail/Outlook expose big "Empty" buttons for these) —
  // anything else gets a stronger confirmation prompt because the
  // user is deleting work, not clearing a disposal bin.
  function isDisposalFolder(name: string | null): boolean {
    if (!name) return false;
    const f = name.toLowerCase();
    return (
      f === 'trash' ||
      f === 'spam' ||
      f === 'junk' ||
      f === '[gmail]/trash' ||
      f === '[gmail]/spam'
    );
  }

  // Whether the current view has anything to mark read.
  let hasUnreadInView = $derived(filteredItems().some((m) => !m.is_read));

  // Folder-actions dropdown open/close state. Closed by default;
  // click-outside and onmouseleave close it.
  let folderMenuOpen = $state(false);

  // Ask-Your-Inbox panel toggle. Slides up from the bottom, sits
  // above the message-detail pane. Only renders when the build's
  // tier supports AI — otherwise the button is hidden too.
  let askBoxOpen = $state(false);

  // Quick on/off mirror of `ai_settings.enabled`. Drives the toolbar
  // toggle next to the Ask button. When off: the Ask button is
  // disabled, any open AskBox is force-closed, and no API surface
  // dispatches outbound LLM calls (server enforces this — the toggle
  // is a UI affordance, not a security boundary).
  let aiEnabled = $state(false);
  let aiToggleBusy = $state(false);
  /// True once the initial aiStatus probe has resolved. Avoids a
  /// flicker where the toolbar shows the toggle in `off` state for
  /// a frame before settling.
  let aiStatusLoaded = $state(false);

  async function refreshAiEnabled() {
    try {
      const st = await api.aiStatus();
      aiEnabled = st.enabled;
    } catch {
      // Transient fetch error (network blip, brief 401 during a
      // vault rehandshake, route remount mid-flight). Leave the
      // toggle state untouched so navigation doesn't flicker the
      // label off when the backend is still serving AI fine.
      // The user only sees the toggle change when they explicitly
      // click it, OR when a successful status response says so.
    } finally {
      aiStatusLoaded = true;
    }
  }

  async function toggleAi() {
    if (aiToggleBusy) return;
    const next = !aiEnabled;
    aiToggleBusy = true;
    try {
      const saved = await api.aiSetEnabled(next);
      aiEnabled = saved.enabled;
      if (!aiEnabled && askBoxOpen) {
        // Hard close — turning AI off should kill any in-flight
        // session, not leave a dead terminal hanging open.
        askBoxOpen = false;
      }
    } catch (e) {
      console.error('aiSetEnabled failed', e);
    } finally {
      aiToggleBusy = false;
    }
  }

  // Hover preview — shows a floating card with the full snippet + date
  // when the pointer rests on a row for 400ms. Lives independent of the
  // split preview pane so you can peek without losing your selection.
  // Positioned horizontally centered on the cursor (clamped to viewport)
  // and vertically below the row — flips above when near the bottom.
  interface HoverState {
    item: DisplayItem;
    rect: DOMRect;
    cursorX: number;
  }
  let hovered = $state<HoverState | null>(null);
  let hoverTimer: ReturnType<typeof setTimeout> | null = null;

  function onRowEnter(item: DisplayItem, e: MouseEvent) {
    if (hoverTimer) clearTimeout(hoverTimer);
    const target = e.currentTarget as HTMLElement;
    const cursorX = e.clientX;
    hoverTimer = setTimeout(() => {
      hovered = { item, rect: target.getBoundingClientRect(), cursorX };
    }, 400);
  }
  function onRowMove(e: MouseEvent) {
    // Track the pointer while the card is up so it trails the cursor
    // horizontally — feels more like a native tooltip.
    if (hovered) hovered = { ...hovered, cursorX: e.clientX };
  }
  function onRowLeave() {
    if (hoverTimer) {
      clearTimeout(hoverTimer);
      hoverTimer = null;
    }
    hovered = null;
  }

  function closePreview() {
    const url = new URL($page.url);
    url.searchParams.delete('m');
    url.searchParams.delete('t');
    goto(url.pathname + url.search, { replaceState: true, noScroll: true, keepFocus: true });
  }

  onMount(() => {
    // Kick off the async initial load without awaiting — Svelte's
    // onMount contract wants a sync cleanup function, not a promise.
    (async () => {
      // Bare /inbox with no filters lands on the unified Inbox rather
      // than the cross-everything "all mail" view — matches what most
      // users expect on first open. `?all=1` keeps the old behaviour
      // for anyone who wants the firehose.
      const params = $page.url.searchParams;
      const hasAnyFilter =
        params.get('u') ||
        params.get('folder') ||
        params.get('account') ||
        params.get('m') ||
        params.get('t') ||
        params.get('q') ||
        params.get('all');
      if (!hasAnyFilter) {
        const url = new URL($page.url);
        url.searchParams.set('u', 'inbox');
        goto(url.pathname + url.search, { replaceState: true, keepFocus: true, noScroll: true });
      }

      // Restore layout prefs
      try {
        const saved = localStorage.getItem('postern.layout');
        if (saved) {
          const p = JSON.parse(saved);
          if (p.splitOrient === 'horizontal' || p.splitOrient === 'vertical') splitOrient = p.splitOrient;
          if (['compact', 'normal', 'comfortable'].includes(p.density)) density = p.density;
          if (typeof p.sidebarWidth === 'number') sidebarWidth = Math.max(MIN_SIDEBAR, Math.min(MAX_SIDEBAR, p.sidebarWidth));
          if (typeof p.listWidth === 'number') listWidth = Math.max(MIN_LIST, Math.min(MAX_LIST, p.listWidth));
          if (typeof p.listHeight === 'number') listHeight = Math.max(MIN_LIST_HEIGHT, Math.min(MAX_LIST_HEIGHT, p.listHeight));
          if (typeof p.fromWidth === 'number') fromWidth = Math.max(MIN_FROM, Math.min(MAX_FROM, p.fromWidth));
          if (typeof p.sidebarHidden === 'boolean') sidebarHidden = p.sidebarHidden;
        }
      } catch {}

      loading = true;
      try {
        const list = await api.listAccounts();
        accounts = list;
        if (list.length === 0) {
          goto('/setup');
          return;
        }
        await Promise.all([loadMessages(), loadFolders()]);
      } finally {
        loading = false;
      }
    })();

    // Seed the toolbar AI toggle from the backend so it shows the
    // right state on first paint. Best-effort; failure leaves the
    // toggle in its default-off position which is the safe option.
    if ($tier.features.ai) {
      void refreshAiEnabled();
    }
    // Seed the lockdown store so the toolbar lock indicator and
    // disabled-button states render correctly on first paint.
    void refreshLockdown();

    // Auto-refresh timer. 30s is a good compromise — the scheduler
    // pulls from IMAP every 60s by default, so polling at half that
    // catches new mail within the scheduler cycle ± a beat.
    autoPollHandle = setInterval(silentPoll, AUTO_POLL_MS);
    // Immediate refresh when the tab comes back into focus so that
    // switching apps on the phone or restoring a laptop from sleep
    // feels snappy rather than stale.
    const onVisibility = () => {
      if (document.visibilityState === 'visible') silentPoll();
    };
    document.addEventListener('visibilitychange', onVisibility);
    return () => {
      if (autoPollHandle) clearInterval(autoPollHandle);
      document.removeEventListener('visibilitychange', onVisibility);
    };
  });

  $effect(() => {
    // Persist layout prefs.
    try {
      localStorage.setItem(
        'postern.layout',
        JSON.stringify({ splitOrient, density, sidebarWidth, listWidth, listHeight, fromWidth, sidebarHidden })
      );
    } catch {}
  });

  $effect(() => {
    activeAccount;
    activeFolder;
    activeQuery;
    activeUnified;
    listMode;
    sort;
    if (!loading) loadMessages();
  });

  function activeFilterLabel() {
    if (isSearching) return `"${activeQuery}"`;
    if (activeUnified) return UNIFIED_DISPLAY[activeUnified];
    if (activeFolder) {
      const acct = folders?.accounts.find((a) => a.account_id === activeAccount);
      const folder =
        acct?.system.find((f) => f.name === activeFolder) ??
        acct?.categories.find((f) => f.name === activeFolder) ??
        acct?.user.find((f) => f.name === activeFolder);
      return folder?.display ?? activeFolder;
    }
    return 'All mail';
  }

  function activeScopeSummary() {
    if (activeUnified) return 'Unified system mailbox';
    if (activeAccount !== null) {
      const acct = folders?.accounts.find((a) => a.account_id === activeAccount);
      return acct?.email ?? 'Selected mailbox';
    }
    return folders
      ? `${folders.accounts.length} mailbox${folders.accounts.length === 1 ? '' : 'es'}`
      : 'All mailboxes';
  }

  type DisplayItem = MessageListItem & { match_snippet?: string };

  function rawItems(): DisplayItem[] {
    return isSearching ? searchHits : messages;
  }

  // Apply the quick filter on top of the server-side folder/search filter.
  function filteredItems(): DisplayItem[] {
    const raw = rawItems();
    switch (quickFilter) {
      case 'unread': return raw.filter((m) => !m.is_read);
      case 'starred': return raw.filter((m) => m.is_starred);
      case 'attachments': return raw.filter((m) => m.has_attachments);
      default: return raw;
    }
  }

  function toggleSplit() {
    splitOrient = splitOrient === 'vertical' ? 'horizontal' : 'vertical';
  }

  function nextDensity() {
    density = density === 'compact' ? 'normal' : density === 'normal' ? 'comfortable' : 'compact';
  }

  // Keyboard: j/k to move selection, Esc to close preview. Works for
  // both messages and threads — picks the right collection on the fly.
  function onKey(e: KeyboardEvent) {
    if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return;

    if (listMode === 'threads' && !isSearching) {
      if (threads.length === 0) return;
      const idx = selectedThread ? threads.findIndex((t) => t.thread_id === selectedThread) : -1;
      if (e.key === 'j') {
        e.preventDefault();
        const next = threads[Math.min(idx + 1, threads.length - 1)];
        if (next) selectThread(next.thread_id);
      } else if (e.key === 'k') {
        e.preventDefault();
        const prev = threads[Math.max(idx - 1, 0)];
        if (prev) selectThread(prev.thread_id);
      } else if (e.key === 'Escape' && selectedThread) {
        e.preventDefault();
        closePreview();
      }
      return;
    }

    const items = filteredItems();
    if (items.length === 0) return;
    const idx = selectedId ? items.findIndex((m) => m.id === selectedId) : -1;
    if (e.key === 'j') {
      e.preventDefault();
      const next = items[Math.min(idx + 1, items.length - 1)];
      if (next) selectMessage(next.id);
    } else if (e.key === 'k') {
      e.preventDefault();
      const prev = items[Math.max(idx - 1, 0)];
      if (prev) selectMessage(prev.id);
    } else if (e.key === 'Escape' && selectedId) {
      e.preventDefault();
      closePreview();
    }
  }
</script>

<svelte:window onkeydown={onKey} onclick={() => { if (folderMenuOpen) folderMenuOpen = false; }} />

<div class="shell" class:detail-open={isMobile && hasSelection}>
  <div
    class="top-bar-wrap"
    class:hidden-mobile={isMobile && hasSelection}
    class:collapsed-mobile={isMobile && mobileToolbarCollapsed}
  >
  <div class="top-bar" role="toolbar" aria-label="Main actions">
    {#if $lockdown.enabled}
      <button
        type="button"
        class="tb-btn"
        disabled
        title="Lockdown mode is on — turn it off in Settings → AI to compose mail"
        aria-label="Compose (disabled)"
      >
        <svg viewBox="0 0 20 20" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
          <path d="M14.5 2.5 17 5l-9 9-3 1 1-3Z"/>
          <path d="M12.5 4.5 15.5 7.5"/>
        </svg>
        <span class="tb-label">Compose</span>
      </button>
    {:else}
      <a class="tb-btn" href="/compose" title="Compose" aria-label="Compose">
        <svg viewBox="0 0 20 20" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
          <path d="M14.5 2.5 17 5l-9 9-3 1 1-3Z"/>
          <path d="M12.5 4.5 15.5 7.5"/>
        </svg>
        <span class="tb-label">Compose</span>
      </a>
    {/if}
    {#if $lockdown.enabled}
      <span
        class="lockdown-pill"
        title="Lockdown mode is active. Sending, replying, archiving, trashing, moving, and remote content are all blocked. Turn off in Settings → AI."
      >
        <svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">
          <rect x="3" y="7.5" width="10" height="6.5" rx="1.2"/>
          <path d="M5 7.5V5a3 3 0 0 1 6 0v2.5"/>
        </svg>
        <span>Lockdown</span>
      </span>
    {/if}

    <div class="tb-sep" aria-hidden="true"></div>

    <div class="tb-group">
      <button
        type="button"
        class="tb-btn"
        disabled={syncing}
        onclick={forceSync}
        title="Force send & receive for the selected account"
        aria-label="Send and receive"
      >
        <svg viewBox="0 0 20 20" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" class:spinning={syncing}>
          <path d="M3 10a7 7 0 0 1 12-5l2-2v5h-5"/>
          <path d="M17 10a7 7 0 0 1-12 5l-2 2v-5h5"/>
        </svg>
        <span class="tb-label">{syncing ? 'Syncing…' : 'Send & receive'}</span>
      </button>
      <select
        class="tb-select"
        bind:value={syncTarget}
        disabled={syncing}
        title="Which account to sync"
        aria-label="Account to sync"
      >
        <option value="all">All accounts</option>
        {#if folders}
          {#each folders.accounts as a (a.account_id)}
            <option value={a.account_id}>{a.email}</option>
          {/each}
        {/if}
      </select>
    </div>

    <div class="tb-spacer"></div>

    <div class="tb-vpn">
      <VpnBadge />
    </div>
    <button
      type="button"
      class="tb-btn tb-lock"
      onclick={() => lockVault()}
      title="Lock the mailbox — the vault will require your master password to unlock again"
      aria-label="Lock"
    >
      <svg viewBox="0 0 20 20" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
        <rect x="4.5" y="9" width="11" height="8" rx="1.2"/>
        <path d="M7 9V6.5a3 3 0 0 1 6 0V9"/>
      </svg>
      <span class="tb-label">Lock</span>
    </button>
  </div>
  {#if isMobile && !hasSelection}
    <button
      type="button"
      class="top-bar-handle"
      aria-expanded={!mobileToolbarCollapsed}
      aria-label={mobileToolbarCollapsed ? 'Show inbox actions' : 'Hide inbox actions'}
      title={mobileToolbarCollapsed ? 'Show inbox actions' : 'Hide inbox actions'}
      onclick={toggleMobileToolbar}
      onpointerdown={beginMobileToolbarGesture}
      onpointerup={endMobileToolbarGesture}
      onpointercancel={() => (mobileToolbarDragStartY = null)}
    >
      <span class="handle-grip" aria-hidden="true"></span>
      <span class="handle-label">{mobileToolbarCollapsed ? 'Show actions' : 'Hide actions'}</span>
    </button>
  {/if}
  </div>

  <div
    class="main-grid"
    class:sidebar-hidden={sidebarHidden}
    class:mobile={isMobile}
    class:drawer-open={isMobile && !sidebarHidden}
    style="--sidebar-width:{sidebarHidden ? 0 : sidebarWidth}px; --list-width:{listWidth}px; --list-height:{listHeight}px; --from-width:{fromWidth}px;"
  >
  {#if isMobile && !sidebarHidden}
    <!-- Scrim behind the drawer — tap to dismiss. Mobile only. -->
    <div
      class="drawer-scrim"
      role="button"
      tabindex="-1"
      aria-label="Close folders"
      onclick={() => (sidebarHidden = true)}
      onkeydown={(e) => e.key === 'Escape' && (sidebarHidden = true)}
    ></div>
  {/if}
  {#if isMobile}
    <!-- Mobile: always-rendered drawer so we can animate transform. -->
    <div class="sidebar-slot drawer">
      <Sidebar
        {folders}
        {activeAccount}
        {activeFolder}
        {activeQuery}
        {activeUnified}
        onSearch={handleSearch}
        onRefresh={refresh}
      />
    </div>
  {:else if !sidebarHidden}
    <!-- Desktop: direct grid children, unchanged. -->
    <Sidebar
      {folders}
      {activeAccount}
      {activeFolder}
      {activeQuery}
      {activeUnified}
      onSearch={handleSearch}
      onRefresh={refresh}
    />
    <div
      class="resizer resizer-v"
      role="separator"
      aria-orientation="vertical"
      aria-label="Resize sidebar"
      onpointerdown={(e) => startResize('sidebar', e)}
    ></div>
  {/if}

  <div class="content" class:split-horizontal={splitOrient === 'horizontal' && (selectedId !== null || selectedThread !== null)} class:split-vertical={splitOrient === 'vertical' && (selectedId !== null || selectedThread !== null)} class:no-preview={selectedId === null && selectedThread === null}>
    <section class="list-pane">
      <header class="pane-header">
        <div class="pane-heading">
          <button
            class="icon-btn sidebar-toggle"
            title={hasSelection && isMobile ? 'Back to list' : sidebarHidden ? 'Show folder list' : 'Hide folder list'}
            aria-label={hasSelection && isMobile ? 'Back to list' : sidebarHidden ? 'Show folder list' : 'Hide folder list'}
            onclick={toggleSidebar}
          >
            {#if hasSelection && isMobile}
              ←
            {:else if sidebarHidden}
              ☰
            {:else}
              ⇤
            {/if}
          </button>
          {#if activeAccount != null}
            {@const acct = folders?.accounts.find((a) => a.account_id === activeAccount) ?? null}
            {#if acct}
              <span
                class="heading-color"
                style:background-color={accountColorMap[activeAccount] ?? 'var(--accent)'}
                title={acct.email}
                aria-hidden="true"
              ></span>
            {/if}
          {/if}
          <div class="heading-copy">
            <h2>{activeFilterLabel()}</h2>
            <p>{activeScopeSummary()}</p>
          </div>
        </div>
        <div class="toolbar">
          <label
            class="inline-select-all"
            title={allVisibleChecked ? 'Deselect all visible' : 'Select all visible'}
          >
            <input
              type="checkbox"
              checked={allVisibleChecked}
              indeterminate={someVisibleChecked}
              onchange={selectAllVisible}
              aria-label="Select all visible"
            />
            <span class="inline-label">Select</span>
          </label>
          <div class="filter-chips" role="tablist" aria-label="Quick filter">
            <button role="tab" class:active={quickFilter === 'all'} onclick={() => (quickFilter = 'all')}>All</button>
            <button role="tab" class:active={quickFilter === 'unread'} onclick={() => (quickFilter = 'unread')}>Unread</button>
            <button role="tab" class:active={quickFilter === 'starred'} onclick={() => (quickFilter = 'starred')}>Starred</button>
            <button role="tab" class:active={quickFilter === 'attachments'} onclick={() => (quickFilter = 'attachments')}>Files</button>
          </div>
          {#if $tier.features.ai}
            <div class="ai-cluster" class:disabled={!aiEnabled} class:loading={!aiStatusLoaded}>
              <!-- Master switch. Off = no providers loaded server-side,
                   no outbound API calls, indexer dormant. The button
                   next to it grays out so it's obvious AI is parked. -->
              <button
                type="button"
                class="ai-toggle"
                class:on={aiEnabled}
                role="switch"
                aria-checked={aiEnabled}
                aria-label={aiEnabled ? 'Turn AI off' : 'Turn AI on'}
                title={aiEnabled
                  ? 'AI is on — click to turn off'
                  : 'AI is off — click to turn on'}
                disabled={aiToggleBusy || !aiStatusLoaded}
                onclick={toggleAi}
              >
                <span class="ai-toggle-track" aria-hidden="true">
                  <span class="ai-toggle-thumb"></span>
                </span>
              </button>
              <button
                type="button"
                class="ask-trigger"
                class:active={askBoxOpen}
                title={aiEnabled
                  ? 'Datas — ask anything about your mail'
                  : 'AI is off — flip the switch on the left to enable'}
                aria-pressed={askBoxOpen}
                disabled={!aiEnabled}
                onclick={() => (askBoxOpen = !askBoxOpen)}
              >
                <span class="ask-mark" aria-hidden="true">✶</span> Datas
              </button>
            </div>
          {/if}
          {#if activeAccount != null && activeFolder && (hasUnreadInView || canEmptyFolder)}
            <div class="folder-menu" role="presentation">
              <button
                type="button"
                class="folder-menu-trigger"
                aria-haspopup="menu"
                aria-expanded={folderMenuOpen}
                title={`Actions for ${activeFolder}`}
                onclick={(e) => { e.stopPropagation(); folderMenuOpen = !folderMenuOpen; }}
              >
                <!-- Folder-with-gear glyph: clearly signals 'folder
                     actions' without needing a text label. -->
                <svg viewBox="0 0 20 20" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.55" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                  <path d="M2.5 5.5A1.5 1.5 0 0 1 4 4h3.2a1.5 1.5 0 0 1 1.2.6l1 1.4h6.1a1.5 1.5 0 0 1 1.5 1.5V15a1.5 1.5 0 0 1-1.5 1.5H4A1.5 1.5 0 0 1 2.5 15V5.5Z"/>
                  <circle cx="13" cy="11.5" r="1.4"/>
                  <path d="M13 8.6v1M13 13.4v1M10.6 11.5h1M14.4 11.5h1"/>
                </svg>
                <span class="chev" class:open={folderMenuOpen} aria-hidden="true">
                  <svg viewBox="0 0 12 12" width="9" height="9" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
                    <path d="m3 4.5 3 3 3-3"/>
                  </svg>
                </span>
              </button>
              {#if folderMenuOpen}
                <div
                  class="folder-menu-pop"
                  role="menu"
                  onclick={(e) => e.stopPropagation()}
                >
                  {#if hasUnreadInView}
                    <button
                      type="button"
                      role="menuitem"
                      disabled={folderActionBusy}
                      onclick={() => { folderMenuOpen = false; markFolderRead(); }}
                    >Mark {activeFolder} as read</button>
                  {/if}
                  {#if canEmptyFolder}
                    <button
                      type="button"
                      role="menuitem"
                      class="danger"
                      disabled={folderActionBusy}
                      onclick={() => { folderMenuOpen = false; emptyActiveFolder(); }}
                    >Empty {activeFolder} (permanent)</button>
                  {/if}
                </div>
              {/if}
            </div>
          {/if}
          <div class="display-tools" aria-label="Display controls">
            <button class="icon-btn" title={listMode === 'threads' ? 'Threads — click for flat list' : 'Flat messages — click for threads'} onclick={toggleListMode}>
              {#if listMode === 'threads'}🧵{:else}☰{/if}
            </button>
            <button class="icon-btn" title="Toggle density" onclick={nextDensity}>
              {#if density === 'compact'}▤{:else if density === 'normal'}▦{:else}▩{/if}
            </button>
            <button class="icon-btn" title={rowStyle === 'detailed' ? 'Detailed rows — click for compact + hover preview' : 'Compact rows — click for detailed'} onclick={toggleRowStyle}>
              {#if rowStyle === 'detailed'}≡{:else}⋯{/if}
            </button>
            <button class="icon-btn" title="Toggle split orientation" onclick={toggleSplit}>
              {#if splitOrient === 'vertical'}▥{:else}▤{/if}
            </button>
            <select
              class="sort-select"
              bind:value={sort}
              onchange={() => prefs.update((p) => ({ ...p, sort }))}
              title="Sort messages"
            >
              <option value="date_desc">Newest first</option>
              <option value="date_asc">Oldest first</option>
              <option value="sender_asc">Sender A→Z</option>
              <option value="sender_desc">Sender Z→A</option>
              <option value="subject_asc">Subject A→Z</option>
              <option value="subject_desc">Subject Z→A</option>
            </select>
          </div>
        </div>
      </header>

      {#if loading}
        <p class="placeholder">Loading…</p>
      {:else if err}
        <p class="placeholder err">Error: {err}</p>
      {:else if listMode === 'threads' && !isSearching}
        {#if threads.length === 0}
          <div class="placeholder">
            <p>No threads in this folder yet.</p>
          </div>
        {:else}
          <div class="list-scroll">
            <ul class="thread-list density-{density}" class:zebra={$prefs.zebraRows}>
              {#each threads as t (t.thread_id)}
                <li>
                  <button
                    class="trow"
                    class:unread={t.unread_count > 0}
                    class:selected={t.thread_id === selectedThread}
                    onclick={() => selectThread(t.thread_id)}
                  >
                    <span class="participants">
                      {t.participants.slice(0, 3).map((p) => formatSender(p)).join(', ')}
                      {#if t.message_count > 1}
                        <span class="count" title="{t.message_count} messages">({t.message_count})</span>
                      {/if}
                    </span>
                    <span class="subject">
                      <span class="subject-text">{t.subject || '(no subject)'}</span>
                      {#if t.latest_snippet}
                        <span class="snippet">— {t.latest_snippet}</span>
                      {/if}
                    </span>
                    <span class="meta-col">
                      {#if t.has_attachments}<span class="attach" title="has attachments">📎</span>{/if}
                      {#if t.unread_count > 0}
                        <span class="unread-badge">{t.unread_count}</span>
                      {/if}
                      <time>{formatDate(t.latest_date_utc)}</time>
                    </span>
                  </button>
                </li>
              {/each}
            </ul>
            {#if hasMore}
              <div class="load-more">
                <button onclick={loadMore} disabled={loadingMore}>
                  {loadingMore ? 'Loading…' : 'Load more'}
                </button>
              </div>
            {/if}
          </div>
        {/if}
      {:else if filteredItems().length === 0}
        <div class="placeholder">
          {#if isSearching}
            <p>No messages match <strong>"{activeQuery}"</strong>.</p>
          {:else if quickFilter !== 'all'}
            <p>No {quickFilter} messages here.</p>
          {:else}
            <p>No messages in this folder yet.</p>
          {/if}
        </div>
      {:else}
        {#if checkedCount > 0}
          <div class="bulk-bar">
            <label class="bulk-all" title="Toggle all visible">
              <input
                type="checkbox"
                checked={allVisibleChecked}
                indeterminate={someVisibleChecked}
                onchange={selectAllVisible}
              />
              <span class="bulk-count">{checkedCount} selected</span>
            </label>
            <div class="bulk-actions">
              <button type="button" onclick={() => runBulk('read')} disabled={bulkBusy}>
                Mark read
              </button>
              <button type="button" onclick={() => runBulk('unread')} disabled={bulkBusy}>
                Mark unread
              </button>
              <button type="button" onclick={() => runBulk('archive')} disabled={bulkBusy}>
                Archive
              </button>
              <button
                type="button"
                onclick={() => (moveBulkOpen = true)}
                disabled={bulkBusy || bulkMoveAccountId() === null}
                title={bulkMoveAccountId() === null
                  ? 'Selection spans multiple accounts — narrow to one account first'
                  : 'Move selected messages to a folder'}
              >
                Move to…
              </button>
              {#if activeFolder && (activeFolder.toLowerCase() === 'spam' || activeFolder.toLowerCase() === '[gmail]/spam' || activeFolder.toLowerCase() === 'junk')}
                <button type="button" onclick={() => runBulk('notspam')} disabled={bulkBusy}>
                  Not spam
                </button>
              {:else}
                <button type="button" onclick={() => runBulk('spam')} disabled={bulkBusy}>
                  Spam
                </button>
              {/if}
              <button type="button" class="danger" onclick={() => runBulk('trash')} disabled={bulkBusy}>
                {bulkBusy ? 'Working…' : 'Trash'}
              </button>
              <button type="button" class="linklike" onclick={clearSelection} disabled={bulkBusy}>
                Clear
              </button>
            </div>
          </div>
        {/if}
        <div class="list-scroll">
          <ul class="msg-list density-{density} style-{rowStyle}" class:bulk-active={checkedCount > 0} class:zebra={$prefs.zebraRows}>
            {#each filteredItems() as m (m.id)}
              <li class:checked={isChecked(m.id)}>
                <button
                  class="row"
                  class:unread={!m.is_read}
                  class:selected={m.id === selectedId}
                  onclick={() => selectMessage(m.id)}
                  onmouseenter={(e) => onRowEnter(m, e)}
                  onmousemove={onRowMove}
                  onmouseleave={onRowLeave}
                >
                  <span
                    class="envelope"
                    class:unread={!m.is_read}
                    class:encrypted={m.is_encrypted}
                    style:--pill-color={accountColorMap[m.account_id] ?? 'var(--accent)'}
                    title={`${m.is_read ? 'Read' : 'Unread'}${m.is_encrypted ? ' · PGP encrypted' : ''}`}
                    aria-label={`${m.is_read ? 'Read' : 'Unread'}${m.is_encrypted ? ' PGP encrypted' : ''}`}
                  >
                    {#if m.is_read}
                      <!-- Read: outline envelope. CSS colours it via
                           currentColor — muted grey when read, account
                           colour when unread. -->
                      <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round" stroke-linecap="round" aria-hidden="true">
                        <path d="M3 9.5 12 15l9-5.5"/>
                        <path d="M3 9.5v10h18v-10"/>
                        <path d="M3 9.5 12 4l9 5.5"/>
                      </svg>
                    {:else}
                      <!-- Unread: filled envelope, colour via currentColor. -->
                      <svg viewBox="0 0 24 24" width="18" height="18" fill="currentColor" aria-hidden="true">
                        <path d="M2.5 6.5A1.5 1.5 0 0 1 4 5h16a1.5 1.5 0 0 1 1.5 1.5v11A1.5 1.5 0 0 1 20 19H4a1.5 1.5 0 0 1-1.5-1.5v-11Zm1.6.2 7.9 5.5 7.9-5.5-.5-.2H4.5l-.4.2Z"/>
                      </svg>
                    {/if}
                    {#if m.is_encrypted}
                      <!-- PGP overlay — bigger and more visible than the
                           in-envelope padlock we used to ship. Sits
                           bottom-right and reads "PGP" so even tiny
                           thumbnails of the row remain self-explanatory. -->
                      <span class="pgp-flag" aria-hidden="true">PGP</span>
                    {/if}
                  </span>
                  <SenderAvatar email={m.from_addr} size={26} />
                  <span class="star" class:starred={m.is_starred}>{m.is_starred ? '★' : '☆'}</span>
                  <span class="from" title={m.from_addr ?? ''}>
                    <span
                      role="button"
                      tabindex="-1"
                      class="from-click"
                      title={m.from_addr ? `Show all from ${m.from_addr}` : ''}
                      onclick={(e) => {
                        e.stopPropagation();
                        filterBySender(m.from_addr);
                      }}
                      onkeydown={(e) => {
                        if (e.key === 'Enter' || e.key === ' ') {
                          e.preventDefault();
                          e.stopPropagation();
                          filterBySender(m.from_addr);
                        }
                      }}
                    >{formatSender(m.from_addr)}</span>
                  </span>
                  <span
                    class="col-resizer"
                    role="separator"
                    aria-orientation="vertical"
                    aria-label="Resize sender column"
                    onpointerdown={(e) => {
                      e.stopPropagation();
                      startResize('from', e);
                    }}
                  ></span>
                  <span class="subject">
                    <span class="subject-text">{m.subject || '(no subject)'}</span>
                    {#if rowStyle === 'detailed'}
                      {#if m.match_snippet}
                        <span class="snippet">— {m.match_snippet?.replace(/<\/?mark>/g, '') ?? ''}</span>
                      {:else if m.snippet}
                        <span class="snippet">— {m.snippet}</span>
                      {/if}
                    {/if}
                  </span>
                  <span class="meta-col">
                    {#if m.has_attachments}<span class="attach" title="has attachments">📎</span>{/if}
                    {#if rowStyle === 'detailed'}
                      <span class="acct">{m.account_email.split('@')[0]}</span>
                      <time>{formatDate(m.date_utc)}</time>
                    {:else}
                      <time class="relative" title={new Date(m.date_utc * 1000).toLocaleString()}>
                        {formatRelative(m.date_utc)}
                      </time>
                    {/if}
                  </span>
                </button>
                <span class="row-check">
                  <input
                    type="checkbox"
                    aria-label="Select message"
                    checked={isChecked(m.id)}
                    onclick={(e) => {
                      e.stopPropagation();
                      toggleChecked(m.id, e);
                    }}
                  />
                </span>
              </li>
            {/each}
          </ul>
          {#if hasMore}
            <div class="load-more">
              <button onclick={loadMore} disabled={loadingMore}>
                {loadingMore ? 'Loading…' : 'Load more'}
              </button>
            </div>
          {/if}
        </div>
      {/if}
    </section>

    {#if selectedId !== null || selectedThread !== null}
      {#if splitOrient === 'vertical'}
        <div
          class="resizer resizer-v resizer-inner"
          role="separator"
          aria-orientation="vertical"
          aria-label="Resize preview"
          onpointerdown={(e) => startResize('list-x', e)}
        ></div>
      {:else}
        <div
          class="resizer resizer-h resizer-inner-h"
          role="separator"
          aria-orientation="horizontal"
          aria-label="Resize preview"
          onpointerdown={(e) => startResize('list-y', e)}
        ></div>
      {/if}
    {/if}

    {#if selectedId !== null || selectedThread !== null}
      <section class="preview-pane">
        <header class="preview-header">
          <button class="close" onclick={closePreview} title="Close preview (Esc)">{isMobile ? '←' : '×'}</button>
          {#if isMobile}
            <div class="preview-context">
              <span class="preview-kicker">{selectedThread !== null ? 'Conversation' : 'Message'}</span>
              <span class="preview-title">{activeFilterLabel()}</span>
            </div>
          {/if}
        </header>
        {#if selectedThread !== null}
          {#key selectedThread}
            <ThreadView threadId={selectedThread} />
          {/key}
        {:else if selectedId !== null}
          {#key selectedId}
            <MessageBody messageId={selectedId} variant="preview" />
          {/key}
        {/if}
      </section>
    {/if}
  </div>
  </div>

  {#if askBoxOpen && $tier.features.ai}
    <AskBox accountId={activeAccount} onClose={() => (askBoxOpen = false)} />
  {/if}
</div>

{#if moveBulkOpen}
  {@const acct = bulkMoveAccountId()}
  {#if acct !== null}
    <FolderPicker
      accountId={acct}
      exclude={bulkMoveExclude()}
      onPick={bulkMovePicked}
      onClose={() => (moveBulkOpen = false)}
    />
  {/if}
{/if}

{#if hovered}
  {@const r = hovered.rect}
  {@const m = hovered.item}
  {@const CARD_W = 360}
  {@const CARD_H = 200}
  {@const margin = 12}
  {@const rawLeft = hovered.cursorX - CARD_W / 2}
  {@const clampedLeft = Math.max(margin, Math.min(window.innerWidth - CARD_W - margin, rawLeft))}
  {@const belowRoom = window.innerHeight - r.bottom}
  {@const aboveRoom = r.top}
  {@const placeAbove = belowRoom < CARD_H + margin && aboveRoom > belowRoom}
  {@const cardTop = placeAbove ? Math.max(margin, r.top - CARD_H - 8) : r.bottom + 8}
  <div
    class="hover-card"
    style="left: {clampedLeft}px; top: {cardTop}px;"
    role="tooltip"
  >
    <div class="hc-head">
      <span class="hc-from">{formatSender(m.from_addr)}</span>
      <span class="hc-date" title={new Date(m.date_utc * 1000).toLocaleString()}>
        {formatRelative(m.date_utc)}
      </span>
    </div>
    <div class="hc-subject">{m.subject || '(no subject)'}</div>
    {#if m.snippet}
      <div class="hc-snippet">{m.snippet}</div>
    {:else}
      <div class="hc-snippet empty">No preview text — open the message to view.</div>
    {/if}
    <div class="hc-footer">
      {#if m.has_attachments}<span class="hc-attach">📎 attachment</span>{/if}
      <span class="hc-acct">{m.account_email}</span>
    </div>
  </div>
{/if}

