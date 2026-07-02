<script lang="ts">
  import './styles.css';
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { page } from '$app/stores';
  import {
    api,
    type Account,
    type FoldersResponse,
    type MessageListItem
  } from '$lib/api';
  import { buildAccountColorMap } from '$lib/accountColor';
  import { formatDate, formatRelative, formatSender } from '$lib/format';
  import FolderPicker from '$lib/components/FolderPicker.svelte';
  import HoverCard from '$lib/components/inbox/HoverCard.svelte';
  import InboxToolbar from '$lib/components/inbox/InboxToolbar.svelte';
  import InboxTopBar from './_components/InboxTopBar.svelte';
  import BulkBar from './_components/BulkBar.svelte';
  import InboxPreviewPane from './_components/InboxPreviewPane.svelte';
  import MessageRow from './_components/MessageRow.svelte';
  import Sidebar from '$lib/components/Sidebar.svelte';
  import SenderAvatar from '$lib/components/SenderAvatar.svelte';
  import { tier } from '$lib/tier';
  import { prefs, type RowStyle, type SortOption } from '$lib/prefs';
  import {
    isUnifiedSystem,
    UNIFIED_DISPLAY,
    type UnifiedSystem
  } from '$lib/unified';
  import {
    isDisposalFolder,
    isProtectedFolder,
    unifiedEmptyTargets
  } from '$lib/folderDisposal';
  import { usePaneResize } from '$lib/usePaneResize.svelte';
  import { allSelected, selectAll, selectedIds, toggleOne, toggleRange } from '$lib/bulkSelection';
  import { useInboxMessages } from '$lib/useInboxMessages.svelte';

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

  // Row style and sort are global preferences (persisted in settings).
  let rowStyle = $state<RowStyle>('detailed');
  let sort = $state<SortOption>('date_desc');
  $effect(() => {
    const unsub = prefs.subscribe((p) => {
      rowStyle = p.rowStyle;
      sort = p.sort;
    });
    return unsub;
  });
  function toggleRowStyle() {
    prefs.update((p) => ({ ...p, rowStyle: p.rowStyle === 'detailed' ? 'compact' : 'detailed' }));
  }

  // Layout prefs — persisted in localStorage.
  let splitOrient = $state<SplitOrient>('vertical');
  let density = $state<Density>('normal');
  let quickFilter = $state<QuickFilter>('all');
  // Resizable sidebar / list / preview pane geometry lives in a
  // composable (sizes + drag handler + clamps). The page keeps only the
  // sidebar *visibility* toggle, which is coupled to the mobile drawer.
  const panes = usePaneResize();
  let sidebarHidden = $state(false);

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
  let hasSelection = $derived(selectedId !== null);

  // Inbox list data + loaders + optimistic mutators. The view keeps
  // navigation/selection/actions; the composable owns the message and
  // search-hit arrays and their load/poll/paginate lifecycle. Query
  // context is read fresh on each load from the URL-derived state above.
  const inbox = useInboxMessages(() => ({
    isSearching,
    query: activeQuery,
    accountId: activeAccount,
    folder: activeFolder,
    unified: activeUnified,
    sort
  }));

  // Multi-select: Set of message ids the user has checked. Kept as
  // a plain object because Svelte reactive Sets land awkwardly in
  // Svelte 5 — map-of-boolean reads the same in templates, commits
  // to state cleanly, and $derived() can recompute from it.
  let checked = $state<Record<number, true>>({});
  let lastCheckedId = $state<number | null>(null);
  let bulkBusy = $state(false);
  // The shift-click range pivot. Cleared when the filter/label
  // changes so we don't try to fill a range across a list refresh.
  let checkedIds = $derived(selectedIds(checked));
  let checkedCount = $derived(checkedIds.length);

  async function loadFolders() {
    try {
      folders = await api.folders();
    } catch (e) {
      console.error('folders load failed', e);
    }
  }

  async function refresh() {
    await Promise.all([inbox.load(), loadFolders()]);
  }

  // --- Silent auto-refresh ---
  // Every AUTO_POLL_MS while the tab is visible, merge any genuinely-new
  // rows into the top of the list (the composable handles the fetch +
  // dedup), then refresh folder counts so unread badges track new
  // arrivals even in other folders. Searches are left alone — scoped,
  // user-driven, and a mid-read rescope would be jarring.
  const AUTO_POLL_MS = 30_000;
  let autoPollHandle: ReturnType<typeof setInterval> | null = null;

  async function silentPoll() {
    if (typeof document !== 'undefined' && document.visibilityState !== 'visible') return;
    if (isSearching) return;
    await inbox.poll();
    await loadFolders();
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
    goto(url.pathname + url.search, { replaceState: true, noScroll: true, keepFocus: true });
    markRead(id);
  }

  // Fire-and-forget: flip the row locally (optimistic) and tell the server.
  // Don't revert on failure — next list refresh will heal any drift, and a
  // transient 5xx shouldn't flicker the envelope back to "unread" under the user.
  function markRead(id: number) {
    inbox.markReadLocal(id);
    api.setMessageRead(id, true).catch(() => {
      /* swallow — optimistic UI wins until next refresh */
    });
  }

  // ── Multi-select helpers ───────────────────────────────────────────────────────────────────
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
    // Shift-click extends the last checkbox action to cover the range
    // between lastCheckedId and id, propagating the anchor's state —
    // same as Gmail / Apple Mail. toggleRange returns the map unchanged
    // (same ref) when an anchor isn't visible, so we fall through to a
    // single toggle.
    if (event.shiftKey && lastCheckedId !== null) {
      const next = toggleRange(checked, filteredItems(), lastCheckedId, id, !isChecked(id));
      if (next !== checked) {
        checked = next;
        lastCheckedId = id;
        return;
      }
    }
    checked = toggleOne(checked, id);
    lastCheckedId = id;
  }

  let allVisibleChecked = $derived.by(() => allSelected(checked, filteredItems()));
  let someVisibleChecked = $derived(checkedCount > 0 && !allVisibleChecked);

  function selectAllVisible() {
    const items = filteredItems();
    if (items.length === 0) return;
    if (allVisibleChecked) {
      checked = {};
      lastCheckedId = null;
      return;
    }
    checked = selectAll(items);
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
    const all = [...inbox.messages, ...inbox.searchHits];
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
    inbox.removeLocal(ids);
    checked = {};
    lastCheckedId = null;
    try {
      await api.bulkMoveTo(ids, folder);
    } catch (e) {
      inbox.err = e instanceof Error ? e.message : String(e);
      await inbox.load();
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
    if (isMove) {
      inbox.removeLocal(ids);
    } else {
      inbox.setReadLocal(ids, action === 'read');
    }
    checked = {};
    lastCheckedId = null;
    try {
      await api.bulkAction(ids, action);
    } catch (e) {
      inbox.err = e instanceof Error ? e.message : String(e);
      // Any half-applied state will heal on the next poll / refresh.
      await inbox.load();
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
      inbox.markAllReadLocal();
    } catch (e) {
      inbox.err = e instanceof Error ? e.message : String(e);
      await inbox.load();
    } finally {
      folderActionBusy = false;
    }
  }

  async function emptyActiveFolder() {
    if (folderActionBusy) return;

    // Unified Trash / Spam: loop every account that has a matching
    // disposal folder and empty each one. The server already gates
    // `empty` to non-protected folders, so the worst a misconfigured
    // account can do is no-op.
    if (activeUnified === 'trash' || activeUnified === 'spam') {
      const display = UNIFIED_DISPLAY[activeUnified];
      const targets = unifiedEmptyTargets(folders, activeUnified);
      if (targets.length === 0) {
        inbox.err = `No ${display} folders found across your accounts.`;
        return;
      }
      if (!confirm(
        `Permanently delete EVERY message in ${display} across ` +
        `${targets.length} mailbox${targets.length === 1 ? '' : 'es'}?\n\n` +
        `This cannot be undone. Both the local copy and the copy on ` +
        `each mail server are removed.`
      )) return;
      folderActionBusy = true;
      try {
        const results = await Promise.allSettled(
          targets.map((t) => api.emptyFolder(t.accountId, t.folder))
        );
        inbox.clear();
        checked = {};
        lastCheckedId = null;
        const fails = results.filter((r) => r.status === 'rejected');
        if (fails.length > 0) {
          inbox.err = `Empty ${display} partially failed: ${fails.length} of ${targets.length} mailbox${targets.length === 1 ? '' : 'es'} errored.`;
        }
        await inbox.load();
      } finally {
        folderActionBusy = false;
      }
      return;
    }

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
        if (typed != null) inbox.err = `Empty folder cancelled — "${typed}" doesn't match "${label}".`;
        return;
      }
    }
    folderActionBusy = true;
    try {
      const r = await api.emptyFolder(activeAccount, label);
      inbox.clear();
      checked = {};
      lastCheckedId = null;
      // No explicit success banner — the visibly-empty list + the
      // confirmation prompt the user just cleared is enough feedback.
      // Using the `err` slot for success messages conflates states and
      // trips screen readers that announce role="alert" semantics.
      // `r.deleted` is logged server-side via folder_emptied audit.
      void r;
      await inbox.load();
    } catch (e) {
      inbox.err = e instanceof Error ? e.message : String(e);
    } finally {
      folderActionBusy = false;
    }
  }

  let canEmptyFolder = $derived.by(() => {
    // Unified Trash / Spam: surface the empty action when at least
    // one account actually has a matching folder. Unified Inbox /
    // Sent / Drafts are protected — same rationale as per-account
    // protected folders.
    if (activeUnified === 'trash' || activeUnified === 'spam') {
      return unifiedEmptyTargets(folders, activeUnified).length > 0;
    }
    if (!activeFolder) return false;
    return !isProtectedFolder(activeFolder);
  });

  // True when the user is looking at any Trash- or Spam-flavoured
  // scope (per-account or unified). Drives the prominent inline
  // Empty button next to the page heading — purging these is a
  // common, expected operation, so it earns a real button rather
  // than being buried in a dropdown.
  let isOnDisposalView = $derived(
    activeUnified === 'trash' ||
    activeUnified === 'spam' ||
    isDisposalFolder(activeFolder)
  );

  let disposalViewLabel = $derived.by(() => {
    if (activeUnified === 'trash') return 'Trash';
    if (activeUnified === 'spam') return 'Spam';
    if (activeFolder?.toLowerCase().includes('spam') ||
        activeFolder?.toLowerCase().includes('junk')) return 'Spam';
    return 'Trash';
  });


  // Whether the current view has anything to mark read.
  let hasUnreadInView = $derived(filteredItems().some((m) => !m.is_read));

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
          panes.applySaved(p);
          if (typeof p.sidebarHidden === 'boolean') sidebarHidden = p.sidebarHidden;
        }
      } catch {}

      inbox.loading = true;
      try {
        const list = await api.listAccounts();
        accounts = list;
        if (list.length === 0) {
          goto('/setup');
          return;
        }
        await Promise.all([inbox.load(), loadFolders()]);
      } finally {
        inbox.loading = false;
      }
    })();

    // Seed the toolbar AI toggle from the backend so it shows the
    // right state on first paint. Best-effort; failure leaves the
    // toggle in its default-off position which is the safe option.

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
        JSON.stringify({
          splitOrient,
          density,
          sidebarWidth: panes.sidebarWidth,
          listWidth: panes.listWidth,
          listHeight: panes.listHeight,
          fromWidth: panes.fromWidth,
          sidebarHidden
        })
      );
    } catch {}
  });

  $effect(() => {
    activeAccount;
    activeFolder;
    activeQuery;
    activeUnified;
    sort;
    if (!inbox.loading) inbox.load();
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
    return isSearching ? inbox.searchHits : inbox.messages;
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

  // Keyboard: j/k to move selection, Esc to close preview.
  function onKey(e: KeyboardEvent) {
    if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return;

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

<svelte:window onkeydown={onKey} />

<div class="shell" class:detail-open={isMobile && hasSelection}>
  <InboxTopBar
    {folders}
    bind:syncTarget
    {syncing}
    {isMobile}
    {hasSelection}
    {mobileToolbarCollapsed}
    onForceSync={forceSync}
    onToggleMobileToolbar={toggleMobileToolbar}
    onBeginMobileToolbarGesture={beginMobileToolbarGesture}
    onEndMobileToolbarGesture={endMobileToolbarGesture}
    onCancelMobileToolbarGesture={() => (mobileToolbarDragStartY = null)}
  />

  <div
    class="main-grid"
    class:sidebar-hidden={sidebarHidden}
    class:mobile={isMobile}
    class:drawer-open={isMobile && !sidebarHidden}
    style="--sidebar-width:{sidebarHidden ? 0 : panes.sidebarWidth}px; --list-width:{panes.listWidth}px; --list-height:{panes.listHeight}px; --from-width:{panes.fromWidth}px;"
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
      onpointerdown={(e) => panes.startResize('sidebar', e)}
    ></div>
  {/if}

  <div class="content" class:split-horizontal={splitOrient === 'horizontal' && selectedId !== null} class:split-vertical={splitOrient === 'vertical' && selectedId !== null} class:no-preview={selectedId === null}>
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
        <InboxToolbar
          {allVisibleChecked}
          {someVisibleChecked}
          bind:quickFilter
          {activeAccount}
          {activeFolder}
          {activeUnified}
          {hasUnreadInView}
          canEmptyFolder={canEmptyFolder && !isOnDisposalView}
          {folderActionBusy}
          showEmptyDisposal={isOnDisposalView && canEmptyFolder}
          emptyDisposalLabel={disposalViewLabel}
          {density}
          {rowStyle}
          {splitOrient}
          bind:sort
          onSelectAll={selectAllVisible}
          onMarkFolderRead={markFolderRead}
          onEmptyFolder={emptyActiveFolder}
          onNextDensity={nextDensity}
          onToggleRowStyle={toggleRowStyle}
          onToggleSplit={toggleSplit}
        />
      </header>

      {#if inbox.loading}
        <p class="placeholder">Loading…</p>
      {:else if inbox.err}
        <p class="placeholder err">Error: {inbox.err}</p>
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
          <BulkBar
            {checkedCount}
            {allVisibleChecked}
            {someVisibleChecked}
            {bulkBusy}
            canMove={bulkMoveAccountId() !== null}
            isSpamFolder={!!activeFolder && (activeFolder.toLowerCase() === 'spam' || activeFolder.toLowerCase() === '[gmail]/spam' || activeFolder.toLowerCase() === 'junk')}
            onSelectAll={selectAllVisible}
            onRun={runBulk}
            onMoveOpen={() => (moveBulkOpen = true)}
            onClear={clearSelection}
          />
        {/if}
        <div class="list-scroll">
          <ul class="msg-list density-{density} style-{rowStyle}" class:bulk-active={checkedCount > 0} class:zebra={$prefs.zebraRows}>
            {#each filteredItems() as m (m.id)}
              <MessageRow
                message={m}
                selected={m.id === selectedId}
                {rowStyle}
                accountColor={accountColorMap[m.account_id] ?? 'var(--accent)'}
                checked={isChecked(m.id)}
                onSelect={() => selectMessage(m.id)}
                onToggleCheck={(e) => toggleChecked(m.id, e)}
                onFilterBySender={() => filterBySender(m.from_addr)}
                onColResize={(e) => panes.startResize('from', e)}
                onMouseEnter={(e) => onRowEnter(m, e)}
                onMouseMove={onRowMove}
                onMouseLeave={onRowLeave}
              />
            {/each}
          </ul>
          {#if inbox.hasMore}
            <div class="load-more">
              <button onclick={inbox.loadMore} disabled={inbox.loadingMore}>
                {inbox.loadingMore ? 'Loading…' : 'Load more'}
              </button>
            </div>
          {/if}
        </div>
      {/if}
    </section>

    {#if selectedId !== null}
      <InboxPreviewPane
        {selectedId}
        {splitOrient}
        {isMobile}
        activeFilterLabel={activeFilterLabel()}
        onResizeStart={(target, e) => panes.startResize(target, e)}
        onClose={closePreview}
      />
    {/if}
  </div>
  </div>

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
  <HoverCard {hovered} />
{/if}
