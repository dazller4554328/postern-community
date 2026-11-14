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
  import { colorForAccount } from '$lib/accountColor';

  // Each mailbox in the sidebar shows a coloured chip in its account
  // colour (set in Settings → Mailboxes; falls back to a deterministic
  // default keyed off id). Replaces the old robohash image — privacy-
  // wise we no longer leak "this email is being viewed" to robohash.org
  // and the chip carries the same identification value at a glance.
  function sidebarAccountColor(acct: AccountFolders): string {
    return colorForAccount({ id: acct.account_id, color: acct.color });
  }

  // Keep a stable reference so child handlers can call api.* without
  // re-importing — the import is already usable above.
  import { formatDate } from '$lib/format';
  import { prefs } from '$lib/prefs';
  import { type UnifiedSystem } from '$lib/unified';
  import './sidebar/sidebar.css';
  import FolderIcon from './FolderIcon.svelte';
  import FolderTreeNode from './sidebar/FolderTreeNode.svelte';
  import SidebarAccountSection from './sidebar/SidebarAccountSection.svelte';
  import type { TreeCtx, TreeNode } from './sidebar/folderTreeTypes';
  import SidebarFooter from './sidebar/SidebarFooter.svelte';
  import SidebarMasthead from './sidebar/SidebarMasthead.svelte';
  import SidebarSearch from './sidebar/SidebarSearch.svelte';
  import SidebarTools from './sidebar/SidebarTools.svelte';
  import SidebarUnifiedViews from './sidebar/SidebarUnifiedViews.svelte';
  import {
    showsUnread as folderShowsUnread,
    showsTotal as folderShowsTotal,
  } from '$lib/folderSemantics';
  import {
    folderTooltip,
    treeKey,
    accountUnread,
    subtreeUnread,
    buildTree,
    visibleSystem,
    visibleCategories,
  } from './sidebar/_lib/folderHelpers';

  // humanBytes / folderTooltip / treeKey / accountUnread / subtreeUnread
  // / buildTree / visibleSystem / visibleCategories live in
  // ./sidebar/_lib/folderHelpers.ts.

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

  function navigateFolder(accountId: number | null, folder: string | null) {
    const url = new URL('/inbox', window.location.origin);
    if (accountId !== null) url.searchParams.set('account', String(accountId));
    if (folder !== null) url.searchParams.set('folder', folder);
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


  // Bundle of reactive state + handlers passed down to every
  // FolderTreeNode instance. Getters re-read $state cells on each
  // access so reactivity propagates across the boundary.
  const treeCtx: TreeCtx = {
    get collapsedTreeNodes() { return collapsedTreeNodes; },
    get menuOpen() { return menuOpen; },
    get renameTarget() { return renameTarget; },
    get renameValue() { return renameValue; },
    set renameValue(v) { renameValue = v; },
    get creatingIn() { return creatingIn; },
    get createValue() { return createValue; },
    set createValue(v) { createValue = v; },
    cancelRename() { renameTarget = null; },
    cancelCreate() { creatingIn = null; },
    toggleTreeNode,
    openMenu,
    closeMenus,
    beginRename,
    beginCreate,
    submitRename,
    submitCreate,
    deleteFolder,
    isActive,
    navigateFolder,
    treeKey,
    folderShowsUnread,
    folderShowsTotal,
    folderTooltip,
    subtreeUnread,
  };
</script>

<svelte:window onclick={onDocClick} />

<aside class="sidebar">
  <SidebarMasthead />

  <SidebarSearch {activeQuery} {folders} {onSearch} />

  <nav>
    <SidebarUnifiedViews {folders} {activeUnified} {activeQuery} />

    <SidebarTools />
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
        {@const userTree = buildTree(acct.user, $prefs.hideEmptyFolders)}
        {@const visSystem = visibleSystem(acct.system, $prefs.hideEmptyFolders)}
        {@const visCategories = visibleCategories(acct.categories, $prefs.hideEmptyFolders)}
        {@const hasUserRows = userTree.length > 0 || (creatingIn?.accountId === acct.account_id && creatingIn?.parent === '')}

        <SidebarAccountSection
          {acct}
          {collapsed}
          {visSystem}
          {visCategories}
          {userTree}
          {hasUserRows}
          {acctUnread}
          accountColor={sidebarAccountColor(acct)}
          lastSyncLabel={lastSyncLabel(acct.account_id)}
          ctx={treeCtx}
          onToggleCollapse={() => toggleAccountCollapse(acct.account_id)}
        />
      {/each}
    {/if}
  </nav>

  <SidebarFooter />
</aside>


