<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { page } from '$app/stores';
  import {
    api,
    type AccountFolders,
    type FolderEntry,
    type FoldersResponse,
    type SyncReport
  } from '$lib/api';
  import { robohashUrl } from '$lib/avatar';

  // AccountFolders carries the same avatar_seed/avatar_set as Account,
  // so we build the RoboHash URL inline rather than pulling the full
  // Account list just to display a picture.
  function sidebarAvatarUrl(acct: AccountFolders): string {
    const seed = acct.avatar_seed?.trim() || acct.email;
    return robohashUrl(seed, acct.avatar_set ?? 'set1', 160);
  }

  // Keep a stable reference so child handlers can call api.* without
  // re-importing — the import is already usable above.
  import { formatDate } from '$lib/format';
  import { prefs, type Theme } from '$lib/prefs';
  import {
    UNIFIED_DISPLAY,
    UNIFIED_ICON_NAME,
    unifiedCounts,
    type UnifiedSystem
  } from '$lib/unified';
  import FolderIcon from './FolderIcon.svelte';
  import {
    showsUnread as folderShowsUnread,
    showsTotal as folderShowsTotal,
    countsTowardAggregateUnread
  } from '$lib/folderSemantics';

  const UNIFIED_ORDER: UnifiedSystem[] = ['inbox', 'drafts', 'sent', 'spam', 'trash'];

  /// Format a byte count for the row tooltip — "12.4 MB", "3 KB",
  /// "521 B". Same scale step the rest of the app uses.
  function humanBytes(n: number): string {
    if (n < 1024) return `${n} B`;
    if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
    if (n < 1024 * 1024 * 1024) return `${(n / (1024 * 1024)).toFixed(1)} MB`;
    return `${(n / (1024 * 1024 * 1024)).toFixed(2)} GB`;
  }

  /// Build the title= tooltip for a folder row: shows what the badge
  /// can't fit. Evolution puts size in the folder properties dialog;
  /// putting it on hover is the lighter equivalent.
  function folderTooltip(display: string, total: number, unread: number, sizeBytes: number): string {
    const parts = [display];
    if (total > 0) {
      parts.push(unread > 0 ? `${unread} unread of ${total}` : `${total} message${total === 1 ? '' : 's'}`);
    }
    if (sizeBytes > 0) parts.push(humanBytes(sizeBytes));
    return parts.join(' · ');
  }

  interface Props {
    folders: FoldersResponse | null;
    activeAccount: number | null;
    activeFolder: string | null;
    activeQuery: string;
    activeUnified?: UnifiedSystem | null;
    onSearch: (q: string) => void;
    onRefresh: () => void;
  }

  let {
    folders,
    activeAccount,
    activeFolder,
    activeQuery,
    activeUnified = null,
    onSearch,
    onRefresh
  }: Props = $props();

  let searchInput = $state('');

  // Advanced-search drawer state. The drawer's "Search" button
  // compiles these fields into an operator string ('from:alice
  // subject:invoice has:attachment before:2025-01-01'), which the
  // parser on the server reads back into a structured query. Keeping
  // the operator syntax as the source of truth means power users can
  // type queries directly and always get the same behaviour.
  let advancedOpen = $state(false);
  let advFrom = $state('');
  let advTo = $state('');
  let advSubject = $state('');
  let advBody = $state('');
  let advAfter = $state('');  // YYYY-MM-DD
  let advBefore = $state('');
  let advLabel = $state('');
  let advAccountEmail = $state('');
  let advHasAttachment = $state(false);
  let advIsUnread = $state(false);
  let advIsStarred = $state(false);
  let advIsEncrypted = $state(false);

  function quoteIfSpaces(v: string): string {
    return /\s/.test(v) ? `"${v.replace(/"/g, '')}"` : v;
  }

  function compileAdvancedQuery(): string {
    const parts: string[] = [];
    // Leave any free-text the user already typed into the search bar
    // as the leading keywords so 'invoice' + {from: alice} becomes
    // 'invoice from:alice', matching the server-side parser's order
    // assumptions.
    const existing = searchInput.trim();
    if (existing) parts.push(existing);
    if (advFrom.trim()) parts.push(`from:${quoteIfSpaces(advFrom.trim())}`);
    if (advTo.trim()) parts.push(`to:${quoteIfSpaces(advTo.trim())}`);
    if (advSubject.trim()) parts.push(`subject:${quoteIfSpaces(advSubject.trim())}`);
    if (advBody.trim()) parts.push(`body:${quoteIfSpaces(advBody.trim())}`);
    if (advAfter) parts.push(`after:${advAfter}`);
    if (advBefore) parts.push(`before:${advBefore}`);
    if (advLabel.trim()) parts.push(`label:${quoteIfSpaces(advLabel.trim())}`);
    if (advAccountEmail) parts.push(`account:${advAccountEmail}`);
    if (advHasAttachment) parts.push('has:attachment');
    if (advIsUnread) parts.push('is:unread');
    if (advIsStarred) parts.push('is:starred');
    if (advIsEncrypted) parts.push('is:encrypted');
    return parts.join(' ');
  }

  function runAdvancedSearch() {
    const q = compileAdvancedQuery();
    searchInput = q;
    onSearch(q);
    advancedOpen = false;
  }

  function resetAdvanced() {
    advFrom = '';
    advTo = '';
    advSubject = '';
    advBody = '';
    advAfter = '';
    advBefore = '';
    advLabel = '';
    advAccountEmail = '';
    advHasAttachment = false;
    advIsUnread = false;
    advIsStarred = false;
    advIsEncrypted = false;
  }
  let syncing = $state(false);
  let syncReports = $state<Record<number, SyncReport | null>>({});
  let collapsedAccounts = $state<Record<number, boolean>>({});
  let collapsedTreeNodes = $state<Record<string, boolean>>({});
  let allMailboxesCollapsed = $derived.by(() => {
    if (!folders || folders.accounts.length === 0) return false;
    return folders.accounts.every((acct) => collapsedAccounts[acct.account_id]);
  });

  // Pick the right logo variant for the active theme. We don't rely on
  // CSS media queries + Svelte scoping — that had the <picture> and
  // override <img> both render together. One <img> with a reactive src
  // is simpler and deterministic.
  let prefersDark = $state(false);
  let currentTheme = $state<Theme>('system');

  onMount(() => {
    const mql = window.matchMedia('(prefers-color-scheme: dark)');
    prefersDark = mql.matches;
    const handler = (e: MediaQueryListEvent) => (prefersDark = e.matches);
    mql.addEventListener('change', handler);
    const unsub = prefs.subscribe((p) => (currentTheme = p.theme));
    return () => {
      mql.removeEventListener('change', handler);
      unsub();
    };
  });

  let effectiveTheme = $derived(
    currentTheme === 'system' ? (prefersDark ? 'dark' : 'light') : currentTheme
  );

  // --- Outbox pending-count badge -----------------------------------------
  //
  // We poll /api/outbox every 30s and count the items still in flight
  // (pending or sending). The number goes into the sidebar Outbox row as
  // a badge so users can spot scheduled/undo-able sends at a glance.
  // Failures are surfaced via their own lookup in the Outbox page — we
  // stay quiet here rather than double-alarm the count.
  let outboxPendingCount = $state(0);
  async function refreshOutboxCount() {
    try {
      const rows = await api.outboxList();
      outboxPendingCount = rows.filter(
        (r) => r.status === 'pending' || r.status === 'sending'
      ).length;
    } catch {
      // Vault locked, 401 over the tunnel, etc. Leave the old count in
      // place — next tick will retry.
    }
  }
  onMount(() => {
    void refreshOutboxCount();
    const t = setInterval(refreshOutboxCount, 30_000);
    return () => clearInterval(t);
  });

  // --- Tool-row active-state detection ------------------------------------
  function isToolActive(prefix: string): boolean {
    return $page.url.pathname === prefix || $page.url.pathname.startsWith(`${prefix}/`);
  }
  function isActivityActive(): boolean {
    return $page.url.pathname === '/settings/audit' && $page.url.searchParams.get('tab') === 'activity';
  }
  function isAuditActive(): boolean {
    return $page.url.pathname === '/settings/audit' && $page.url.searchParams.get('tab') !== 'activity';
  }
  function isSettingsActive(): boolean {
    return $page.url.pathname.startsWith('/settings') && !$page.url.pathname.startsWith('/settings/audit');
  }
  const LOGO_VERSION = '4';
  let logoSrc = $derived(
    currentTheme === 'cyberpunk'
      ? `/logo-cyberpunk.png?v=${LOGO_VERSION}`
      : effectiveTheme === 'dark'
        ? `/logo-dark.png?v=${LOGO_VERSION}`
        : `/logo-light.png?v=${LOGO_VERSION}`
  );

  // Mirror the URL's active-query into the search input. Kept as a
  // thin $effect so the input updates when the user navigates back/
  // forward through history.
  $effect(() => {
    searchInput = activeQuery;
  });

  // Restore collapsed-state from localStorage ONCE at mount. Previously
  // this lived inside the $effect above, which meant every keystroke in
  // the search box re-read storage and overwrote any in-memory toggles
  // the user had just made — a classic "why did my collapsed folders
  // just spring open?" bug. Mount-only restore fixes it; persist() still
  // writes on every change.
  onMount(() => {
    try {
      const a = localStorage.getItem('postern.sidebar.collapsedAccounts');
      if (a) collapsedAccounts = JSON.parse(a);
      const t = localStorage.getItem('postern.sidebar.collapsedTreeNodes');
      if (t) collapsedTreeNodes = JSON.parse(t);
    } catch {}
  });

  function persist() {
    try {
      localStorage.setItem('postern.sidebar.collapsedAccounts', JSON.stringify(collapsedAccounts));
      localStorage.setItem('postern.sidebar.collapsedTreeNodes', JSON.stringify(collapsedTreeNodes));
    } catch {}
  }

  function toggleAccountCollapse(id: number) {
    collapsedAccounts = { ...collapsedAccounts, [id]: !collapsedAccounts[id] };
    persist();
  }

  function setAllMailboxesCollapsed(collapsed: boolean) {
    if (!folders) return;
    collapsedAccounts = Object.fromEntries(
      folders.accounts.map((acct) => [acct.account_id, collapsed])
    );
    persist();
  }


  function toggleTreeNode(accountId: number, fullPath: string) {
    const key = `${accountId}:${fullPath}`;
    collapsedTreeNodes = { ...collapsedTreeNodes, [key]: !collapsedTreeNodes[key] };
    persist();
  }

  // ---- Folder context menu ----
  // Single open menu at a time; tracked by account + path so each row
  // opens its own instance.
  let menuOpen = $state<string | null>(null);
  let renameTarget = $state<{ accountId: number; path: string } | null>(null);
  let renameValue = $state('');
  let creatingIn = $state<{ accountId: number; parent: string } | null>(null);
  let createValue = $state('');

  function openMenu(accountId: number, path: string, e: Event) {
    e.stopPropagation();
    const key = `${accountId}:${path}`;
    menuOpen = menuOpen === key ? null : key;
  }
  function closeMenus() {
    menuOpen = null;
  }

  // Close menu / dismiss any inline form on outside click.
  function onDocClick() {
    if (menuOpen) menuOpen = null;
  }
  function beginRename(accountId: number, path: string, current: string) {
    renameTarget = { accountId, path };
    renameValue = current;
    menuOpen = null;
  }
  function beginCreate(accountId: number, parentPath: string) {
    creatingIn = { accountId, parent: parentPath };
    createValue = '';
    menuOpen = null;
    // Always keep the account expanded while we're creating something
    // inside it — otherwise the input renders inside a collapsed panel
    // and the user thinks nothing happened.
    if (collapsedAccounts[accountId]) {
      collapsedAccounts = { ...collapsedAccounts, [accountId]: false };
      persist();
    }
  }
  async function submitRename() {
    if (!renameTarget) return;
    const from = renameTarget.path;
    const toRaw = renameValue.trim();
    if (!toRaw || toRaw === from) {
      renameTarget = null;
      return;
    }
    try {
      // Preserve parent path when the user types only the leaf name.
      const parent = from.includes('/') ? from.slice(0, from.lastIndexOf('/')) : '';
      const to = toRaw.includes('/') || !parent ? toRaw : `${parent}/${toRaw}`;
      await api.renameFolder(renameTarget.accountId, from, to);
      renameTarget = null;
      onRefresh();
    } catch (e) {
      alert(e instanceof Error ? e.message : String(e));
    }
  }
  async function submitCreate() {
    if (!creatingIn) return;
    const leaf = createValue.trim().replace(/^\/+|\/+$/g, '');
    if (!leaf) {
      creatingIn = null;
      return;
    }
    const fullName = creatingIn.parent ? `${creatingIn.parent}/${leaf}` : leaf;
    const aid = creatingIn.accountId;
    try {
      await api.createFolder(aid, fullName);
      // Make sure the account (and any parent tree nodes) are expanded
      // so the new entry is visible immediately after refresh.
      if (collapsedAccounts[aid]) {
        collapsedAccounts = { ...collapsedAccounts, [aid]: false };
      }
      if (creatingIn.parent) {
        // Walk the parent path: expand every ancestor so the child
        // we just created sits on a visible branch.
        const parts = creatingIn.parent.split('/');
        const next: Record<string, boolean> = { ...collapsedTreeNodes };
        let acc = '';
        for (const p of parts) {
          acc = acc ? `${acc}/${p}` : p;
          next[`${aid}:${acc}`] = false;
        }
        collapsedTreeNodes = next;
      }
      persist();
      creatingIn = null;
      onRefresh();
    } catch (e) {
      alert(e instanceof Error ? e.message : String(e));
    }
  }
  async function deleteFolder(accountId: number, path: string) {
    try {
      await api.deleteFolder(accountId, path, false);
      onRefresh();
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      if (msg.includes('force=true') || msg.includes('messages')) {
        if (confirm(`${msg}\n\nDelete anyway? Messages in this folder will lose this label locally.`)) {
          try {
            await api.deleteFolder(accountId, path, true);
            onRefresh();
          } catch (e2) {
            alert(e2 instanceof Error ? e2.message : String(e2));
          }
        }
      } else {
        alert(msg);
      }
    }
  }

  function treeKey(accountId: number, fullPath: string) {
    return `${accountId}:${fullPath}`;
  }

  async function submitSearch(e: Event) {
    e.preventDefault();
    onSearch(searchInput.trim());
  }

  function navigateFolder(accountId: number | null, folder: string | null) {
    const url = new URL('/inbox', window.location.origin);
    if (accountId !== null) url.searchParams.set('account', String(accountId));
    if (folder !== null) url.searchParams.set('folder', folder);
    goto(url.pathname + url.search, { noScroll: true, keepFocus: true, invalidateAll: true });
  }

  function navigateUnified(system: UnifiedSystem) {
    const url = new URL('/inbox', window.location.origin);
    url.searchParams.set('u', system);
    goto(url.pathname + url.search, { noScroll: true, keepFocus: true, invalidateAll: true });
  }

  async function syncAll() {
    if (!folders || syncing) return;
    syncing = true;
    try {
      await Promise.all(folders.accounts.map((a) => api.triggerSync(a.account_id)));
      await new Promise((r) => setTimeout(r, 1500));
      const reports = await Promise.all(
        folders.accounts.map(
          async (a) =>
            [a.account_id, await api.syncStatus(a.account_id).catch(() => null)] as const
        )
      );
      syncReports = Object.fromEntries(reports);
      onRefresh();
    } finally {
      syncing = false;
    }
  }

  function lastSyncLabel(id: number) {
    const r = syncReports[id];
    if (!r) return null;
    if (r.error) return `⚠ ${r.error.slice(0, 40)}`;
    const total = r.folders.reduce((a, f) => a + f.new, 0);
    return `${formatDate(r.finished_at)} · +${total}`;
  }

  function isActive(accountId: number | null, folder: string | null) {
    if (activeUnified) return false;
    const matchAccount = (activeAccount ?? null) === accountId;
    const matchFolder = (activeFolder ?? null) === folder;
    return matchAccount && matchFolder && !activeQuery;
  }

  function isUnifiedActive(system: UnifiedSystem) {
    return activeUnified === system && !activeQuery;
  }

  function accountUnread(acct: AccountFolders): number {
    const all = [...acct.system, ...acct.categories, ...acct.user];
    // Only count folders where unread is semantically meaningful —
    // Sent / Drafts / Trash / Outbox / All-Mail never contribute.
    // Otherwise a collapsed-account badge would overstate: "3 unread"
    // when 2 are real Inbox mail and 1 is a phantom \Unseen on a
    // sent message you can't action.
    return all
      .filter((f) => countsTowardAggregateUnread(f.name))
      .reduce((s, f) => s + f.unread, 0);
  }

  // ---- Tree builder for user-label hierarchy ----
  // IMAP folder names commonly use "/" as the hierarchy separator (e.g.
  // "Work/Projects/Alpha"). A folder with no entry is a grouping-only
  // parent (children have mail, the parent itself doesn't exist as an
  // IMAP folder). We sort siblings alphabetically and let the tree walk
  // handle indentation via its depth.
  interface TreeNode {
    segment: string;
    fullPath: string;
    entry: FolderEntry | null;
    children: TreeNode[];
  }

  function buildTree(folders: FolderEntry[]): TreeNode[] {
    const filtered = $prefs.hideEmptyFolders
      ? folders.filter((f) => (f.total ?? 0) > 0 || (f.unread ?? 0) > 0)
      : folders;
    const root: TreeNode = { segment: '', fullPath: '', entry: null, children: [] };
    const sorted = [...filtered].sort((a, b) => a.name.localeCompare(b.name));
    for (const f of sorted) {
      const parts = f.name.split('/');
      let cur = root;
      let acc = '';
      for (let i = 0; i < parts.length; i++) {
        const seg = parts[i];
        acc = acc ? `${acc}/${seg}` : seg;
        let child = cur.children.find((c) => c.segment === seg);
        if (!child) {
          child = { segment: seg, fullPath: acc, entry: null, children: [] };
          cur.children.push(child);
        }
        if (i === parts.length - 1) child.entry = f;
        cur = child;
      }
    }
    return root.children;
  }

  /**
   * System folders + Gmail categories with zero messages. System-row
   * filtering is a subset: never hide INBOX / Sent / Drafts / Trash
   * even when empty, since those are reference points the user
   * expects to see. Only categories (Updates, Promotions, Social…)
   * and generic empties get hidden.
   */
  const NEVER_HIDE_SYSTEM = new Set([
    'INBOX', 'Sent', 'Drafts', 'Trash', 'Spam', 'Junk',
    '[Gmail]/Sent Mail', '[Gmail]/Drafts', '[Gmail]/Trash', '[Gmail]/Spam'
  ]);
  function visibleSystem(list: FolderEntry[]): FolderEntry[] {
    if (!$prefs.hideEmptyFolders) return list;
    return list.filter((f) =>
      NEVER_HIDE_SYSTEM.has(f.name) || (f.total ?? 0) > 0 || (f.unread ?? 0) > 0
    );
  }
  function visibleCategories(list: FolderEntry[]): FolderEntry[] {
    if (!$prefs.hideEmptyFolders) return list;
    return list.filter((f) => (f.total ?? 0) > 0 || (f.unread ?? 0) > 0);
  }

  // Aggregate unread across a subtree — shown on a collapsed parent so
  // the user knows there's something inside worth expanding. Apply
  // the same semantic filter as accountUnread so a subtree containing
  // a user-named "Sent" folder doesn't contribute phantom counts.
  function subtreeUnread(n: TreeNode): number {
    let u = 0;
    if (n.entry && countsTowardAggregateUnread(n.entry.name)) {
      u += n.entry.unread;
    }
    for (const c of n.children) u += subtreeUnread(c);
    return u;
  }
</script>

<svelte:window onclick={onDocClick} />

<aside>
  <header class="masthead">
    <a class="brand" href="/inbox" aria-label="Postern home">
      <span class="brand-mark" aria-hidden="true"></span>
      <img src={logoSrc} alt="Postern" class="logo" />
    </a>
    <p class="brand-copy">Your mail. Your server. Your keys.</p>
    <div class="masthead-meta">
      <span class="vault-note">Vault-backed session</span>
    </div>
  </header>

  <form class="search" onsubmit={submitSearch}>
    <label class="search-box">
      <svg viewBox="0 0 20 20" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round">
        <circle cx="8.5" cy="8.5" r="5.5" />
        <path d="m13 13 4 4" />
      </svg>
      <input
        type="search"
        placeholder="Search mail"
        title="Use from: to: subject: body: has:attachment is:unread is:starred label: before: after: older_than:30d account:  — or click the filter icon for the form."
        bind:value={searchInput}
        autocomplete="off"
        spellcheck="false"
      />
      <button
        type="button"
        class="search-advanced-toggle"
        class:open={advancedOpen}
        title={advancedOpen ? 'Close advanced search' : 'Advanced search'}
        aria-label={advancedOpen ? 'Close advanced search' : 'Advanced search'}
        aria-expanded={advancedOpen}
        onclick={() => (advancedOpen = !advancedOpen)}
      >
        <svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">
          <path d="M2 4h12M4 8h8M6 12h4"/>
        </svg>
      </button>
    </label>
  </form>
  {#if advancedOpen}
    <div class="search-advanced" role="region" aria-label="Advanced search">
      <div class="sa-row">
        <label class="sa-field">
          <span>From</span>
          <input type="text" placeholder="alice@corp.com" bind:value={advFrom} />
        </label>
        <label class="sa-field">
          <span>To / Cc</span>
          <input type="text" placeholder="bob" bind:value={advTo} />
        </label>
      </div>
      <div class="sa-row">
        <label class="sa-field">
          <span>Subject</span>
          <input type="text" placeholder="invoice" bind:value={advSubject} />
        </label>
        <label class="sa-field">
          <span>Body contains</span>
          <input type="text" placeholder="quarterly" bind:value={advBody} />
        </label>
      </div>
      <div class="sa-row">
        <label class="sa-field">
          <span>After</span>
          <input type="date" bind:value={advAfter} />
        </label>
        <label class="sa-field">
          <span>Before</span>
          <input type="date" bind:value={advBefore} />
        </label>
      </div>
      <div class="sa-row">
        <label class="sa-field">
          <span>Label / folder</span>
          <input type="text" placeholder="Work/Projects" bind:value={advLabel} />
        </label>
        <label class="sa-field">
          <span>Account email</span>
          <select bind:value={advAccountEmail}>
            <option value="">Any mailbox</option>
            {#if folders}
              {#each folders.accounts as a (a.account_id)}
                <option value={a.email}>{a.email}</option>
              {/each}
            {/if}
          </select>
        </label>
      </div>
      <div class="sa-checks">
        <label><input type="checkbox" bind:checked={advHasAttachment} /> Has attachment</label>
        <label><input type="checkbox" bind:checked={advIsUnread} /> Unread</label>
        <label><input type="checkbox" bind:checked={advIsStarred} /> Starred</label>
        <label><input type="checkbox" bind:checked={advIsEncrypted} /> Encrypted (PGP)</label>
      </div>
      <div class="sa-actions">
        <button type="button" class="sa-btn primary" onclick={runAdvancedSearch}>Search</button>
        <button type="button" class="sa-btn" onclick={resetAdvanced}>Clear fields</button>
        <details class="sa-help">
          <summary>Operator reference</summary>
          <ul>
            <li><code>from:</code>, <code>to:</code>, <code>cc:</code>, <code>subject:</code>, <code>body:</code> — scope to a field</li>
            <li><code>has:attachment</code> — only messages with files attached</li>
            <li><code>is:unread</code> · <code>is:read</code> · <code>is:starred</code> · <code>is:encrypted</code></li>
            <li><code>label:Work/Projects</code> — only in this label</li>
            <li><code>before:2025-01-01</code> · <code>after:2025-06-15</code> — date range</li>
            <li><code>older_than:30d</code> · <code>newer_than:7d</code> — relative (s/m/h/d/w/y)</li>
            <li><code>account:you@gmail.com</code> — scope to one mailbox</li>
            <li><code>-word</code> — exclude (same as <code>NOT word</code>)</li>
            <li><code>"exact phrase"</code> — phrase match</li>
          </ul>
        </details>
      </div>
    </div>
  {/if}

  <nav>
    {#if folders && folders.accounts.length > 0}
      <div class="section-label">Unified views</div>
      <div class="unified">
        {#each UNIFIED_ORDER as sys (sys)}
          {@const c = unifiedCounts(folders.accounts, sys)}
          {@const canUnread = folderShowsUnread(UNIFIED_ICON_NAME[sys])}
          {@const canTotal = folderShowsTotal(UNIFIED_ICON_NAME[sys])}
          <button
            type="button"
            class="row unified-row"
            class:active={isUnifiedActive(sys)}
            class:unread={canUnread && c.unread > 0}
            onclick={() => navigateUnified(sys)}
            title={`${UNIFIED_DISPLAY[sys]} — all accounts`}
          >
            <span class="unified-icon-wrap" aria-hidden="true">
              <FolderIcon name={UNIFIED_ICON_NAME[sys]} kind="system" />
              <!-- Small offset-stack glyph indicating this row
                   aggregates across every account, not just the
                   currently-focused one. -->
              <span class="unified-stack-badge"></span>
            </span>
            <span class="label">{UNIFIED_DISPLAY[sys]}</span>
            {#if canUnread && c.unread > 0}
              <span class="unread-count">
                {c.unread}{#if canTotal && c.total > c.unread}<span class="total-tail">/{c.total}</span>{/if}
              </span>
            {:else if canTotal && c.total > 0}
              <span class="total-count">{c.total}</span>
            {/if}
          </button>
        {/each}
      </div>
      <div class="unified-divider" aria-hidden="true"></div>
    {/if}

    <!-- Tools: cross-mailbox and operational surfaces
         that were cluttering the footer. Outbox sits first because
         it's the most time-sensitive (users come here to cancel a
         scheduled or undo-able send). -->
    <div class="section-label">Tools</div>
    <div class="tools">
      <a
        class="row tool-row"
        class:active={isToolActive('/outbox')}
        href="/outbox"
        title="Scheduled and pending sends"
      >
        <span class="tool-icon" aria-hidden="true">
          <svg viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.65" stroke-linecap="round" stroke-linejoin="round">
            <path d="M3.5 5.5h13v9h-13z"/>
            <path d="m4 6 6 4.6L16 6"/>
            <path d="M13 3.5h4v4"/>
            <path d="M17 3.5 11.5 9"/>
          </svg>
        </span>
        <span class="label">Outbox</span>
        {#if outboxPendingCount > 0}
          <span class="unread-count" title="Pending sends">{outboxPendingCount}</span>
        {/if}
      </a>
      <a
        class="row tool-row"
        class:active={isToolActive('/calendar')}
        href="/calendar"
        title="Calendar"
      >
        <span class="tool-icon" aria-hidden="true">
          <svg viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.65" stroke-linecap="round" stroke-linejoin="round">
            <rect x="3" y="4.5" width="14" height="12" rx="1.6"/>
            <path d="M6.5 2.8v3.4M13.5 2.8v3.4M3 8h14"/>
            <path d="M6.5 11h.1M10 11h.1M13.5 11h.1M6.5 14h.1M10 14h.1"/>
          </svg>
        </span>
        <span class="label">Calendar</span>
      </a>
      <a
        class="row tool-row"
        class:active={isToolActive('/reminders')}
        href="/reminders"
        title="Local reminders"
      >
        <span class="tool-icon" aria-hidden="true">
          <svg viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.65" stroke-linecap="round" stroke-linejoin="round">
            <circle cx="10" cy="10.5" r="5.8"/>
            <path d="M7 3.2 4.8 2M13 3.2 15.2 2M10 7.2v3.5l2.4 1.5M6.3 16l-1.2 1.4M13.7 16l1.2 1.4"/>
          </svg>
        </span>
        <span class="label">Reminders</span>
      </a>
      <a
        class="row tool-row"
        class:active={isToolActive('/contacts')}
        href="/contacts"
        title="Address book — auto-collected from sync + send"
      >
        <span class="tool-icon" aria-hidden="true">
          <svg viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.65" stroke-linecap="round" stroke-linejoin="round">
            <circle cx="10" cy="7.5" r="3"/>
            <path d="M4.5 16.5c0-3 2.5-5 5.5-5s5.5 2 5.5 5"/>
          </svg>
        </span>
        <span class="label">Contacts</span>
      </a>
      <a
        class="row tool-row"
        class:active={isActivityActive()}
        href="/settings/audit?tab=activity"
        title="Server activity — sync cycles, sends, errors"
      >
        <span class="tool-icon activity" aria-hidden="true">
          <svg viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.65" stroke-linecap="round" stroke-linejoin="round">
            <path d="M3 11h3l2-5 4 8 2-4h3"/>
            <path d="M3 16h14"/>
          </svg>
        </span>
        <span class="label">Activity</span>
      </a>
      <a
        class="row tool-row"
        class:active={isAuditActive()}
        href="/settings/audit"
        title="Security audit log"
      >
        <span class="tool-icon audit" aria-hidden="true">
          <svg viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.65" stroke-linecap="round" stroke-linejoin="round">
            <path d="M10 17.5s-5.8-3.4-5.8-8.2V4.1L10 2.2l5.8 1.9v5.2c0 4.8-5.8 8.2-5.8 8.2z"/>
            <path d="m7.4 10 1.8 1.8 3.5-4"/>
          </svg>
        </span>
        <span class="label">Audit</span>
      </a>
    </div>
    <div class="unified-divider" aria-hidden="true"></div>

    <div class="section-label">All accounts</div>
    <button
      type="button"
      class="all-mail row emphasis-row"
      class:active={isActive(null, null)}
      onclick={() => navigateFolder(null, null)}
    >
      <FolderIcon name="INBOX" kind="system" />
      <span class="label">All mail</span>
      {#if folders}
        {@const totalUnread = folders.accounts
          .filter((a) => a.include_in_unified)
          .reduce((s, a) => s + accountUnread(a), 0)}
        {#if totalUnread > 0}
          <span class="unread-count">{totalUnread}</span>
        {/if}
      {/if}
    </button>

    {#if folders}
      <div class="section-label section-label-action">
        <span>Mailboxes</span>
        <span class="section-actions">
          <a
            class="hdr-chip hdr-chip-icon"
            href="/setup"
            title="Add mailbox"
            aria-label="Add mailbox"
          >
            <svg viewBox="0 0 14 14" width="11" height="11" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round">
              <path d="M7 2.5v9M2.5 7h9"/>
            </svg>
          </a>
          <button
            type="button"
            class="hdr-chip"
            class:toggled={$prefs.hideEmptyFolders}
            title={$prefs.hideEmptyFolders ? 'Showing non-empty folders only — click to show all' : 'Showing every folder — click to hide empty ones'}
            onclick={() => prefs.update((p) => ({ ...p, hideEmptyFolders: !p.hideEmptyFolders }))}
          >
            {$prefs.hideEmptyFolders ? 'Non-empty' : 'All'}
          </button>
          <button
            type="button"
            class="hdr-chip"
            title={allMailboxesCollapsed ? 'Expand all' : 'Collapse all'}
            onclick={() => setAllMailboxesCollapsed(!allMailboxesCollapsed)}
          >
            {allMailboxesCollapsed ? 'Expand' : 'Collapse'}
          </button>
        </span>
      </div>
      {#each folders.accounts as acct (acct.account_id)}
        {@const collapsed = collapsedAccounts[acct.account_id]}
        {@const acctUnread = accountUnread(acct)}
        {@const userTree = buildTree(acct.user)}
        {@const visSystem = visibleSystem(acct.system)}
        {@const visCategories = visibleCategories(acct.categories)}
        {@const hasUserRows = userTree.length > 0 || (creatingIn?.accountId === acct.account_id && creatingIn?.parent === '')}

        <section class="account" class:collapsed>
          <button
            type="button"
            class="acct-header"
            onclick={() => toggleAccountCollapse(acct.account_id)}
            title={collapsed ? 'Expand folders' : 'Collapse folders'}
            aria-expanded={!collapsed}
          >
            <span class="acct-avatar-wrap">
              <img
                class="acct-avatar"
                class:collapsed
                src={sidebarAvatarUrl(acct)}
                alt=""
                loading="lazy"
              />
            </span>
            <span class="acct-meta">
              <span class="email" title={acct.email}>{acct.email}</span>
              {#if !collapsed && lastSyncLabel(acct.account_id)}
                <span class="last-sync">{lastSyncLabel(acct.account_id)}</span>
              {/if}
            </span>
            {#if collapsed && acctUnread > 0}
              <span class="unread-count acct-badge">{acctUnread}</span>
            {/if}
            <span class="acct-chevron" class:open={!collapsed} aria-hidden="true">
              <svg viewBox="0 0 12 12" width="10" height="10" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
                <path d="m4 2 4 4-4 4"/>
              </svg>
            </span>
          </button>

          {#if !collapsed}
            <div class="account-body">
            {#each visSystem as f, i (f.name)}
              {@const isLastSystem = i === visSystem.length - 1 && visCategories.length === 0 && !hasUserRows}
              {@const canUnread = folderShowsUnread(f.name)}
              {@const canTotal = folderShowsTotal(f.name)}
              <div class="tree-node">
                <div class="tree-row-wrap">
                  <span class="conn elbow" class:end={isLastSystem} aria-hidden="true"></span>
                  <span class="tree-chev-spacer" aria-hidden="true"></span>
                  <button
                    type="button"
                    class="row tree-row branch-row"
                    class:active={isActive(acct.account_id, f.name)}
                    class:unread={canUnread && f.unread > 0}
                    onclick={() => navigateFolder(acct.account_id, f.name)}
                    title={folderTooltip(f.display, f.total, f.unread, f.size_bytes)}
                  >
                    <FolderIcon name={f.name} kind="system" />
                    <span class="label">{f.display}</span>
                    {#if canUnread && f.unread > 0}
                      <span class="unread-count">
                        {f.unread}{#if canTotal && f.total > f.unread}<span class="total-tail">/{f.total}</span>{/if}
                      </span>
                    {:else if canTotal && f.total > 0}
                      <span class="total-count">{f.total}</span>
                    {/if}
                  </button>
                </div>
              </div>
            {/each}

            {#if visCategories.length > 0}
              {#each visCategories as f, i (f.name)}
                {@const isLastCategory = i === visCategories.length - 1 && !hasUserRows}
                {@const canUnread = folderShowsUnread(f.name)}
                {@const canTotal = folderShowsTotal(f.name)}
                <div class="tree-node">
                  <div class="tree-row-wrap">
                    <span class="conn elbow" class:end={isLastCategory} aria-hidden="true"></span>
                    <span class="tree-chev-spacer" aria-hidden="true"></span>
                    <button
                      type="button"
                      class="row tree-row branch-row category-row"
                      class:active={isActive(acct.account_id, f.name)}
                      class:unread={canUnread && f.unread > 0}
                      onclick={() => navigateFolder(acct.account_id, f.name)}
                      title={folderTooltip(f.display, f.total, f.unread, f.size_bytes)}
                    >
                      <FolderIcon name={f.name} kind="gmail_category" />
                      <span class="label">{f.display}</span>
                      {#if canUnread && f.unread > 0}
                        <span class="unread-count">
                          {f.unread}{#if canTotal && f.total > f.unread}<span class="total-tail">/{f.total}</span>{/if}
                        </span>
                      {:else if canTotal && f.total > 0}
                        <span class="total-count">{f.total}</span>
                      {/if}
                    </button>
                  </div>
                </div>
              {/each}
            {/if}

            <div class="tree-node">
              <div class="tree-row-wrap">
                <span class="conn elbow" class:end={!hasUserRows} aria-hidden="true"></span>
                <span class="tree-chev-spacer" aria-hidden="true"></span>
                <button
                  type="button"
                  class="row tree-row branch-row tree-action-row"
                  onclick={() => beginCreate(acct.account_id, '')}
                  title="Create a new top-level folder"
                  aria-label="New folder"
                >
                  <span class="new-folder-icon" aria-hidden="true">+</span>
                  <span class="label">New folder</span>
                </button>
              </div>
            </div>
            {#if creatingIn?.accountId === acct.account_id && creatingIn?.parent === ''}
              <div class="tree-node">
                <div class="tree-row-wrap">
                  <span class="conn elbow end" aria-hidden="true"></span>
                  <span class="tree-chev-spacer" aria-hidden="true"></span>
                  <form
                    class="inline-form"
                    onsubmit={(e) => { e.preventDefault(); submitCreate(); }}
                  >
                    <input
                      type="text"
                      bind:value={createValue}
                      placeholder="new folder name…"
                      onblur={() => (creatingIn = null)}
                      onkeydown={(e) => { if (e.key === 'Escape') creatingIn = null; }}
                      autofocus
                    />
                  </form>
                </div>
              </div>
            {/if}
            {#each userTree as node, i (node.fullPath)}
              {@render treeNode(acct.account_id, node, 0, [], i === userTree.length - 1)}
            {/each}
            </div>
          {/if}
        </section>
      {/each}
    {/if}
  </nav>

  <footer>
    <a
      class="row tool-row footer-settings"
      class:active={isSettingsActive()}
      href="/settings"
      data-sveltekit-preload-code="off"
      data-sveltekit-preload-data="off"
      title="Appearance, mailboxes, privacy, updates"
    >
      <span class="tool-icon" aria-hidden="true">
        <svg viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.55" stroke-linecap="round" stroke-linejoin="round">
          <circle cx="10" cy="10" r="2.4" />
          <path d="M16.2 11.8a6.6 6.6 0 0 0 0-3.6l1.7-1.3-1.6-2.7-2 .8a6.6 6.6 0 0 0-3.1-1.8l-.3-2.1h-3.1l-.3 2.1a6.6 6.6 0 0 0-3.1 1.8l-2-.8L1 6.9l1.7 1.3a6.6 6.6 0 0 0 0 3.6L1 13.1l1.6 2.7 2-.8a6.6 6.6 0 0 0 3.1 1.8l.3 2.1h3.1l.3-2.1a6.6 6.6 0 0 0 3.1-1.8l2 .8L18 13.1Z"/>
        </svg>
      </span>
      <span class="label">Settings</span>
    </a>
  </footer>
</aside>

{#snippet treeNode(accountId: number, node: TreeNode, depth: number, trail: boolean[], isLast: boolean)}
  {@const hasChildren = node.children.length > 0}
  {@const key = treeKey(accountId, node.fullPath)}
  {@const nodeCollapsed = collapsedTreeNodes[key]}
  {@const u = subtreeUnread(node)}
  {@const menuKey = `${accountId}:${node.fullPath}`}
  {@const isRenaming = renameTarget?.accountId === accountId && renameTarget.path === node.fullPath}
  <div class="tree-node">
    <div class="tree-row-wrap">
      <!-- Ancestor rail columns: one per ancestor depth. `cont=true` means
           that ancestor still has siblings below, so a vertical rail
           continues through this row. `cont=false` = blank column. -->
      {#each trail as cont}
        <span class="conn" class:rail={cont} aria-hidden="true"></span>
      {/each}
      <!-- Elbow into this node: └── if this node is the last child of
           its parent, otherwise ├──. -->
      <span class="conn elbow" class:end={isLast} aria-hidden="true"></span>
      {#if hasChildren}
        <button
          type="button"
          class="tree-chev"
          class:open={!nodeCollapsed}
          onclick={() => toggleTreeNode(accountId, node.fullPath)}
          title={nodeCollapsed ? 'Expand' : 'Collapse'}
          aria-label={nodeCollapsed ? 'Expand' : 'Collapse'}
        >
          <svg viewBox="0 0 12 12" width="10" height="10" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
            <path d="m4 2 4 4-4 4"/>
          </svg>
        </button>
      {:else}
        <span class="tree-chev-spacer" aria-hidden="true"></span>
      {/if}
      {#if isRenaming}
        <form
          class="inline-form"
          onsubmit={(e) => { e.preventDefault(); submitRename(); }}
        >
          <input
            type="text"
            bind:value={renameValue}
            onblur={() => (renameTarget = null)}
            onkeydown={(e) => { if (e.key === 'Escape') renameTarget = null; }}
            autofocus
          />
        </form>
      {:else if node.entry}
        {@const canUnread = folderShowsUnread(node.entry.name)}
        {@const canTotal = folderShowsTotal(node.entry.name)}
        <button
          type="button"
          class="row tree-row"
          class:active={isActive(accountId, node.entry.name)}
          class:unread={canUnread && node.entry.unread > 0}
          onclick={() => navigateFolder(accountId, node.entry!.name)}
          oncontextmenu={(e) => { e.preventDefault(); openMenu(accountId, node.fullPath, e); }}
          title={folderTooltip(node.entry.display, node.entry.total, node.entry.unread, node.entry.size_bytes)}
        >
          <FolderIcon name={node.segment} kind="user" />
          <span class="label">{node.segment}</span>
          {#if canUnread && node.entry.unread > 0}
            <span class="unread-count">
              {node.entry.unread}{#if canTotal && node.entry.total > node.entry.unread}<span class="total-tail">/{node.entry.total}</span>{/if}
            </span>
          {:else if canTotal && node.entry.total > 0}
            <span class="total-count">{node.entry.total}</span>
          {/if}
        </button>
        <button
          type="button"
          class="row-menu"
          onclick={(e) => openMenu(accountId, node.fullPath, e)}
          aria-label="Folder actions"
          title="Folder actions"
        >
          ⋯
        </button>
      {:else}
        <!-- Grouping node with no folder of its own -->
        <button
          type="button"
          class="row tree-row grouping"
          onclick={() => toggleTreeNode(accountId, node.fullPath)}
          oncontextmenu={(e) => { e.preventDefault(); openMenu(accountId, node.fullPath, e); }}
          title="Folder group"
        >
          <FolderIcon name="" kind="user" />
          <span class="label">{node.segment}</span>
          {#if nodeCollapsed && u > 0}
            <span class="unread-count">{u}</span>
          {/if}
        </button>
        <button
          type="button"
          class="row-menu"
          onclick={(e) => openMenu(accountId, node.fullPath, e)}
          aria-label="Folder actions"
          title="Folder actions"
        >
          ⋯
        </button>
      {/if}
      {#if menuOpen === menuKey}
        <div
          class="ctx-menu"
          role="menu"
          onclick={(e) => e.stopPropagation()}
          onkeydown={(e) => { if (e.key === 'Escape') closeMenus(); }}
          tabindex="-1"
        >
          <button type="button" onclick={() => beginCreate(accountId, node.fullPath)}>New subfolder…</button>
          {#if node.entry}
            <button type="button" onclick={() => beginRename(accountId, node.fullPath, node.segment)}>Rename…</button>
            <button type="button" class="danger" onclick={() => deleteFolder(accountId, node.fullPath)}>Delete</button>
          {/if}
        </div>
      {/if}
    </div>
    {#if creatingIn?.accountId === accountId && creatingIn?.parent === node.fullPath}
      <div class="tree-node">
        <div class="tree-row-wrap">
          {#each [...trail, !isLast] as cont}
            <span class="conn" class:rail={cont} aria-hidden="true"></span>
          {/each}
          <span class="conn elbow end" aria-hidden="true"></span>
          <span class="tree-chev-spacer" aria-hidden="true"></span>
          <form
            class="inline-form"
            onsubmit={(e) => { e.preventDefault(); submitCreate(); }}
          >
            <input
              type="text"
              bind:value={createValue}
              placeholder="new folder name…"
              onblur={() => (creatingIn = null)}
              onkeydown={(e) => { if (e.key === 'Escape') creatingIn = null; }}
              autofocus
            />
          </form>
        </div>
      </div>
    {/if}
    {#if hasChildren && !nodeCollapsed}
      {#each node.children as child, i (child.fullPath)}
        {@render treeNode(
          accountId,
          child,
          depth + 1,
          /* Children's trail = our trail + a flag for OUR column:
             "does this (current) node still have siblings below?"
             That's !isLast. */
          [...trail, !isLast],
          i === node.children.length - 1
        )}
      {/each}
    {/if}
  </div>
{/snippet}

<style>
  aside {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    overflow: hidden;
    min-width: 0;
    width: 100%;
    max-width: 100%;
    box-sizing: border-box;
    background:
      linear-gradient(180deg, color-mix(in oklab, var(--surface) 90%, white 10%), var(--surface)),
      var(--surface);
    border-right: 1px solid color-mix(in oklab, currentColor 10%, transparent);
  }

  .masthead {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 0.65rem;
    padding: 1.15rem 1rem 0.85rem;
    border-bottom: 1px solid var(--border);
    background:
      radial-gradient(circle at top left, color-mix(in oklab, var(--accent) 12%, transparent), transparent 48%),
      linear-gradient(180deg, color-mix(in oklab, var(--surface-2) 75%, transparent), transparent);
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    color: inherit;
    text-decoration: none;
    line-height: 0;
    min-width: 0;
  }
  .brand-mark {
    width: 1rem;
    height: 1rem;
    border-radius: 0.28rem;
    background:
      linear-gradient(135deg, var(--accent), color-mix(in oklab, var(--accent) 40%, white 60%));
    box-shadow:
      0 0 0 1px color-mix(in oklab, var(--accent) 18%, transparent),
      0 10px 22px color-mix(in oklab, var(--accent) 18%, transparent);
    flex-shrink: 0;
  }
  .brand .logo {
    display: block;
    height: 31px;
    width: auto;
    max-width: 100%;
    object-fit: contain;
    object-position: left center;
  }
  .brand-copy {
    margin: 0;
    font-size: 0.76rem;
    line-height: 1.45;
    color: var(--muted);
    max-width: 22ch;
  }
  .masthead-meta {
    display: flex;
    align-items: center;
    gap: 0.55rem;
    flex-wrap: wrap;
  }
  .vault-note {
    font-size: 0.68rem;
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--muted);
  }
  form.search {
    padding: 0 1rem 0.6rem;
  }
  .search-box {
    display: grid;
    grid-template-columns: 14px 1fr auto;
    align-items: center;
    gap: 0.55rem;
    padding: 0.7rem 0.8rem;
    border: 1px solid color-mix(in oklab, currentColor 12%, transparent);
    background: color-mix(in oklab, var(--surface-2) 72%, transparent);
    border-radius: 0.8rem;
    color: var(--muted);
  }
  .search-advanced-toggle {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    padding: 0;
    background: transparent;
    color: inherit;
    border: 0;
    border-radius: 0.35rem;
    opacity: 0.55;
    cursor: pointer;
    transition: opacity 120ms, background 120ms, color 120ms;
  }
  .search-advanced-toggle:hover,
  .search-advanced-toggle.open {
    opacity: 1;
    background: color-mix(in oklab, currentColor 8%, transparent);
    color: var(--accent);
  }
  form.search input {
    width: 100%;
    font: inherit;
    font-size: 0.84rem;
    padding: 0;
    border: 0;
    background: transparent;
    color: inherit;
    box-sizing: border-box;
  }
  form.search input:focus {
    outline: none;
  }
  .search-box:focus-within {
    border-color: color-mix(in oklab, var(--accent) 32%, transparent);
    box-shadow: 0 0 0 3px color-mix(in oklab, var(--accent) 12%, transparent);
    color: inherit;
  }

  /* Advanced-search drawer: sits below the search-box, matches its
     rounded surface-2 visual so the whole search area reads as one
     unit rather than a modal over the sidebar. */
  .search-advanced {
    margin: 0 1rem 0.7rem;
    padding: 0.7rem 0.8rem 0.8rem;
    border: 1px solid color-mix(in oklab, currentColor 10%, transparent);
    background: color-mix(in oklab, var(--surface-2) 58%, transparent);
    border-radius: 0.8rem;
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
    font-size: 0.78rem;
  }
  .sa-row {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0.5rem;
  }
  .sa-field {
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
    min-width: 0;
  }
  .sa-field > span {
    font-size: 0.66rem;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--muted);
  }
  .sa-field input,
  .sa-field select {
    font: inherit;
    font-size: 0.78rem;
    padding: 0.38rem 0.55rem;
    border: 1px solid color-mix(in oklab, currentColor 12%, transparent);
    background: var(--surface, #fff);
    color: inherit;
    border-radius: 0.45rem;
    min-width: 0;
  }
  .sa-field input:focus,
  .sa-field select:focus {
    outline: none;
    border-color: color-mix(in oklab, var(--accent) 36%, transparent);
    box-shadow: 0 0 0 3px color-mix(in oklab, var(--accent) 12%, transparent);
  }
  .sa-checks {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0.3rem 0.55rem;
    padding-top: 0.1rem;
  }
  .sa-checks label {
    display: inline-flex;
    align-items: center;
    gap: 0.45rem;
    font-size: 0.77rem;
    cursor: pointer;
  }
  .sa-actions {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    flex-wrap: wrap;
    margin-top: 0.1rem;
  }
  .sa-btn {
    font: inherit;
    font-size: 0.77rem;
    font-weight: 600;
    padding: 0.42rem 0.85rem;
    border: 1px solid color-mix(in oklab, currentColor 12%, transparent);
    border-radius: 999px;
    background: var(--surface);
    color: inherit;
    cursor: pointer;
    transition: background 120ms, border-color 120ms;
  }
  .sa-btn:hover {
    background: color-mix(in oklab, currentColor 5%, var(--surface));
  }
  .sa-btn.primary {
    background: color-mix(in oklab, var(--accent) 22%, var(--surface));
    border-color: color-mix(in oklab, var(--accent) 46%, transparent);
  }
  .sa-btn.primary:hover {
    background: color-mix(in oklab, var(--accent) 32%, var(--surface));
  }
  .sa-help {
    margin-left: auto;
    font-size: 0.72rem;
    color: var(--muted);
  }
  .sa-help summary {
    cursor: pointer;
    user-select: none;
  }
  .sa-help summary:hover {
    color: inherit;
  }
  .sa-help ul {
    margin: 0.4rem 0 0;
    padding-left: 1rem;
    line-height: 1.55;
  }
  .sa-help code {
    font-size: 0.7rem;
    padding: 0.04rem 0.25rem;
    background: color-mix(in oklab, currentColor 8%, transparent);
    border-radius: 0.2rem;
  }

  nav {
    flex: 1;
    overflow-y: auto;
    overflow-x: hidden;
    min-width: 0;
    padding: 0 0.55rem 0.85rem;
    font-size: 0.84rem;
  }
  .section-label {
    padding: 0.75rem 0.45rem 0.35rem;
    font-size: 0.67rem;
    font-weight: 700;
    letter-spacing: 0.09em;
    text-transform: uppercase;
    color: var(--muted);
  }
  .section-label-action {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
  }
  /* Mailboxes header controls — single pill family so +, 'All'/'Non-empty',
     and 'Collapse'/'Expand' share the same height and visual weight.
     Icon-only variant (the + button) is a square of the same height. */
  .hdr-chip {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    height: 1.55rem;
    padding: 0 0.55rem;
    border: 1px solid color-mix(in oklab, currentColor 10%, transparent);
    background: color-mix(in oklab, var(--surface-2) 70%, transparent);
    color: inherit;
    text-decoration: none;
    border-radius: 999px;
    font: inherit;
    font-size: 0.68rem;
    font-weight: 600;
    letter-spacing: 0.02em;
    line-height: 1;
    cursor: pointer;
    opacity: 0.78;
    white-space: nowrap;
    transition: background 120ms, border-color 120ms, opacity 120ms;
  }
  .hdr-chip:hover {
    opacity: 1;
    background: color-mix(in oklab, var(--accent) 10%, var(--surface));
    border-color: color-mix(in oklab, var(--accent) 26%, transparent);
  }
  .hdr-chip-icon {
    width: 1.55rem;
    padding: 0;
    opacity: 0.65;
  }
  .hdr-chip-icon:hover {
    opacity: 1;
    color: var(--accent);
  }
  .hdr-chip.toggled {
    opacity: 1;
    background: color-mix(in oklab, var(--accent) 18%, var(--surface));
    border-color: color-mix(in oklab, var(--accent) 40%, transparent);
    color: color-mix(in oklab, currentColor 78%, var(--accent) 22%);
  }

  /* Every clickable row shares the same grid: [icon] [label] [badge] */
  .row {
    display: grid;
    grid-template-columns: 16px 1fr auto;
    align-items: center;
    gap: 0.5rem;
    width: 100%;
    padding: 0.5rem 0.7rem;
    background: transparent;
    border: 0;
    color: inherit;
    font: inherit;
    font-size: 0.84rem;
    text-align: left;
    cursor: pointer;
    border-radius: 0.7rem;
    margin: 2px 0;
  }
  .row:hover {
    background: color-mix(in oklab, currentColor 6%, transparent);
  }
  .row.active {
    background: color-mix(in oklab, var(--accent) 14%, transparent);
    box-shadow: inset 0 0 0 1px color-mix(in oklab, var(--accent) 14%, transparent);
    font-weight: 500;
  }
  .row.unread .label {
    font-weight: 600;
  }
  .label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .all-mail {
    margin-bottom: 0.55rem;
    font-weight: 500;
  }
  .emphasis-row {
    background: color-mix(in oklab, var(--surface-2) 82%, transparent);
  }
  .all-mail,
  .unified-row {
    border: 1px solid color-mix(in oklab, currentColor 8%, transparent);
    background: color-mix(in oklab, var(--surface) 92%, transparent);
  }
  .all-mail:hover,
  .unified-row:hover {
    background: var(--row-hover);
    border-color: color-mix(in oklab, currentColor 14%, transparent);
  }
  .all-mail.active,
  .unified-row.active {
    background: var(--row-selected);
    border-color: color-mix(in oklab, var(--accent) 28%, transparent);
  }

  /* Unified cross-account system folders sit at the top so you can
     treat Inbox / Sent / Drafts / Spam / Trash as one surface across
     every account. Visually distinct from the per-account tree below. */
  .unified {
    padding: 0.1rem 0 0.2rem;
    display: flex;
    flex-direction: column;
    gap: 0.18rem;
  }
  .unified-row {
    font-weight: 500;
  }
  /* Two-layer icon: the folder icon + a small offset square behind
     it suggesting "stacked / aggregated across accounts." Makes a
     unified row visually distinct from the per-account folder rows
     below without needing an extra character column. */
  .unified-icon-wrap {
    position: relative;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 18px;
    height: 18px;
    flex-shrink: 0;
  }
  .unified-icon-wrap :global(svg) {
    position: relative;
    z-index: 1;
  }
  .unified-stack-badge {
    position: absolute;
    right: -3px;
    bottom: -3px;
    width: 8px;
    height: 8px;
    border-radius: 2px;
    background: color-mix(in oklab, var(--accent) 62%, var(--surface));
    border: 1px solid var(--surface);
    box-shadow: 0 0 0 1px color-mix(in oklab, currentColor 18%, transparent);
    z-index: 2;
    pointer-events: none;
  }
  .unified-divider {
    height: 1px;
    margin: 0.55rem 0.45rem 0.2rem;
    background: color-mix(in oklab, currentColor 10%, transparent);
  }

  .account {
    margin-bottom: 0.55rem;
    padding: 0.08rem 0;
    border-radius: 0.9rem;
    border: 0;
    background: transparent;
    box-shadow: none;
  }
  .account.collapsed {
    margin-bottom: 0.35rem;
    background: transparent;
  }

  button.acct-header {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    width: 100%;
    padding: 0.58rem 0.72rem;
    background: color-mix(in oklab, var(--surface-2) 52%, transparent);
    border: 1px solid color-mix(in oklab, currentColor 8%, transparent);
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
    border-radius: 0.68rem;
    margin-top: 0.05rem;
    box-shadow: inset 0 1px 0 color-mix(in oklab, white 4%, transparent);
    transition: background 120ms, border-color 120ms;
  }
  button.acct-header:hover {
    background: color-mix(in oklab, var(--surface-2) 70%, transparent);
    border-color: color-mix(in oklab, currentColor 14%, transparent);
  }
  .account:not(.collapsed) > .acct-header {
    margin-bottom: 0.28rem;
  }
  .account-body > .tree-node {
    margin-left: 0.22rem;
    margin-right: 0.22rem;
  }
  .account:not(.collapsed) .tree-row-wrap {
    background: transparent;
    border: 1px solid transparent;
  }
  .acct-avatar-wrap {
    display: inline-flex;
    flex-shrink: 0;
    align-items: center;
    justify-content: center;
  }
  img.acct-avatar {
    width: 1.55rem;
    height: 1.55rem;
    display: block;
    border-radius: 0.38rem;
    object-fit: cover;
    background: color-mix(in oklab, var(--surface-2) 72%, transparent);
    border: 1px solid color-mix(in oklab, currentColor 10%, transparent);
    padding: 0.08rem;
    box-sizing: border-box;
    image-rendering: -webkit-optimize-contrast;
    transition: transform 140ms ease, opacity 140ms ease;
  }
  .acct-header:hover img.acct-avatar {
    transform: scale(1.05);
  }
  img.acct-avatar.collapsed {
    transform: scale(0.94);
    opacity: 0.9;
  }

  .acct-meta {
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 0.05rem;
    flex: 1;
    min-width: 0;
  }
  .acct-header .email {
    font-size: 0.81rem;
    opacity: 0.95;
    font-weight: 620;
    letter-spacing: 0;
    line-height: 1.2;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .acct-header .last-sync {
    font-size: 0.63rem;
    opacity: 0.54;
    font-weight: 400;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .acct-chevron {
    flex-shrink: 0;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 1.25rem;
    height: 1.25rem;
    opacity: 0.46;
    border-radius: 999px;
    transition: transform 120ms ease, opacity 120ms ease, background 120ms ease;
  }
  .acct-chevron.open {
    transform: rotate(90deg);
    opacity: 0.75;
  }
  .acct-header:hover .acct-chevron {
    opacity: 0.9;
    background: color-mix(in oklab, currentColor 8%, transparent);
  }
  .acct-header .acct-badge {
    flex-shrink: 0;
  }
  .account-body {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 0.02rem;
    padding: 0.02rem 0.08rem 0.12rem;
  }
  .account-body::before {
    content: '';
    position: absolute;
    left: 1.18rem;
    top: 0.12rem;
    bottom: 0.3rem;
    width: 1px;
    background: color-mix(in oklab, currentColor 18%, transparent);
    opacity: 0.72;
    pointer-events: none;
  }

  .total-count {
    font-size: 0.72rem;
    opacity: 0.4;
    font-variant-numeric: tabular-nums;
  }
  .unread-count {
    background: color-mix(in oklab, var(--accent) 78%, transparent);
    color: white;
    padding: 0.1rem 0.48rem;
    border-radius: 999px;
    font-weight: 700;
    font-size: 0.68rem;
    min-width: 1ch;
    text-align: center;
    font-variant-numeric: tabular-nums;
  }
  /* The "/523" tail inside the unread badge, e.g. "16/523".
     Slightly muted so the unread number stays the focal point. */
  .unread-count .total-tail {
    margin-left: 1px;
    font-weight: 500;
    opacity: 0.7;
  }

  .conn {
    width: 14px;
    height: 100%;
    min-height: 24px;
    position: relative;
    flex-shrink: 0;
    pointer-events: none;
  }
  .conn.rail::before {
    content: '';
    position: absolute;
    left: 50%;
    top: 0;
    bottom: 0;
    border-left: 1px solid color-mix(in oklab, currentColor 30%, transparent);
  }
  .conn.elbow::before {
    content: '';
    position: absolute;
    left: 50%;
    top: 0;
    bottom: 0;
    border-left: 1px solid color-mix(in oklab, currentColor 30%, transparent);
  }
  .conn.elbow.end::before {
    bottom: auto;
    height: 50%;
  }
  .conn.elbow::after {
    content: '';
    position: absolute;
    left: 50%;
    right: -1px;
    top: 50%;
    border-top: 1px solid color-mix(in oklab, currentColor 30%, transparent);
  }
  .tree-row-wrap {
    display: flex;
    align-items: stretch;
    position: relative;
    margin: 0;
    min-height: 24px;
    border-radius: 0.45rem;
  }
  .row-menu {
    background: transparent;
    border: 0;
    color: inherit;
    font: inherit;
    font-size: 1rem;
    line-height: 1;
    padding: 0.15rem 0.5rem;
    opacity: 0;
    cursor: pointer;
    border-radius: 999px;
    transition: opacity 120ms, background 120ms;
  }
  .tree-row-wrap:hover .row-menu,
  .row-menu:focus-visible {
    opacity: 0.7;
  }
  .row-menu:hover {
    opacity: 1;
    background: color-mix(in oklab, currentColor 8%, transparent);
  }
  .ctx-menu {
    position: absolute;
    right: 0.25rem;
    top: 100%;
    z-index: 40;
    min-width: 10rem;
    padding: 0.25rem;
    border: 1px solid var(--border);
    border-radius: 0.6rem;
    background: var(--surface);
    box-shadow: 0 14px 30px rgba(0, 0, 0, 0.18);
    display: flex;
    flex-direction: column;
  }
  .ctx-menu button {
    text-align: left;
    padding: 0.5rem 0.65rem;
    font: inherit;
    font-size: 0.8rem;
    background: transparent;
    border: 0;
    color: inherit;
    border-radius: 0.4rem;
    cursor: pointer;
  }
  .ctx-menu button:hover {
    background: color-mix(in oklab, currentColor 8%, transparent);
  }
  .ctx-menu button.danger {
    color: color-mix(in oklab, crimson 80%, currentColor 20%);
  }
  .ctx-menu button.danger:hover {
    background: color-mix(in oklab, crimson 12%, transparent);
  }
  .inline-form {
    padding-right: 0.35rem;
  }
  .inline-form input {
    font: inherit;
    font-size: 0.82rem;
    width: 100%;
    padding: 0.28rem 0.5rem;
    border: 1px solid var(--accent);
    background: var(--surface);
    color: inherit;
    border-radius: 0.35rem;
  }
  .tree-chev,
  .tree-chev-spacer {
    width: 14px;
    height: 18px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    background: transparent;
    border: 0;
    color: inherit;
    cursor: pointer;
    padding: 0;
    opacity: 0.55;
    flex-shrink: 0;
    align-self: center;
  }
  .tree-chev {
    transition: transform 120ms ease;
    border-radius: 0.25rem;
  }
  .tree-chev:hover {
    opacity: 1;
    background: color-mix(in oklab, currentColor 8%, transparent);
  }
  .tree-chev.open {
    transform: rotate(90deg);
  }
  .tree-row {
    padding-left: 0.38rem;
    padding-top: 0.24rem;
    padding-bottom: 0.24rem;
    border-radius: 0.36rem;
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 0.38rem;
  }
  .tree-row :global(svg) {
    width: 12px;
    height: 12px;
    flex-shrink: 0;
    opacity: 0.75;
  }
  .tree-row .label {
    font-size: 0.8rem;
    line-height: 1.1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .branch-row {
    padding-right: 0.48rem;
  }
  .branch-row .label {
    font-weight: 520;
  }
  .category-row .label {
    opacity: 0.82;
  }
  .tree-action-row {
    color: var(--muted);
  }
  .tree-action-row:hover,
  .tree-action-row:focus-visible {
    color: inherit;
  }
  .new-folder-icon {
    width: 12px;
    height: 12px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border-radius: 0.2rem;
    border: 1px solid color-mix(in oklab, currentColor 18%, transparent);
    font-size: 0.78rem;
    line-height: 1;
    opacity: 0.78;
  }
  .tree-row-wrap:hover .tree-row {
    background: color-mix(in oklab, currentColor 4%, var(--surface));
  }
  .tree-row.active {
    background: color-mix(in oklab, var(--accent) 14%, transparent);
  }
  .tree-row.grouping .label {
    opacity: 0.62;
  }

  @media (max-width: 900px) {
    .acct-chevron {
      display: none;
    }
    .account-body::before {
      display: none;
    }
  }

  footer {
    padding: 0.55rem 0.55rem 0.75rem;
    border-top: 1px solid color-mix(in oklab, currentColor 8%, transparent);
    display: block;
    min-width: 0;
    overflow: hidden;
    box-sizing: border-box;
  }
  /* Settings link shares the tool-row visual language so the footer
     feels like part of the sidebar rather than a bare link pinned to
     the bottom. */
  .footer-settings {
    width: 100%;
    margin: 0;
    box-sizing: border-box;
  }

  /* Tools section uses the same visual language as unified rows: small
     themed glyph, bordered row, and active color from the theme tokens. */
  .tools {
    display: flex;
    flex-direction: column;
    gap: 0.18rem;
    padding: 0.1rem 0 0.2rem;
  }
  .tool-row {
    font-weight: 500;
    text-decoration: none;
    color: inherit;
    border: 1px solid color-mix(in oklab, currentColor 8%, transparent);
    background: color-mix(in oklab, var(--surface) 92%, transparent);
  }
  .tool-row:hover {
    background: var(--row-hover);
    border-color: color-mix(in oklab, currentColor 14%, transparent);
  }
  .tool-row.active {
    background: var(--row-selected);
    border-color: color-mix(in oklab, var(--accent) 28%, transparent);
  }
  .tool-icon {
    width: 16px;
    height: 16px;
    display: inline-grid;
    place-items: center;
    color: color-mix(in oklab, currentColor 78%, var(--accent));
  }
  .tool-icon svg {
    width: 16px;
    height: 16px;
    display: block;
  }
  .tool-icon.activity {
    color: color-mix(in oklab, mediumseagreen 70%, var(--fg));
  }
  .tool-icon.audit {
    color: color-mix(in oklab, var(--accent) 78%, var(--fg));
  }
  .tool-row.active .tool-icon,
  .tool-row:hover .tool-icon {
    color: var(--accent);
  }

  /* + button next to the Mailboxes section header. Takes the place
     of the old "+ Add mailbox" footer link. */
  .section-actions {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
  }
</style>
