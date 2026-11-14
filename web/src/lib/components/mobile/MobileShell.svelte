<script lang="ts">
  import './MobileShell.css';
  import { page } from '$app/stores';
  import { goto } from '$app/navigation';
  import { onMount, tick, type Snippet } from 'svelte';
  import { api, type FoldersResponse } from '$lib/api';
  import { lockVault } from '$lib/vault';
  import { prefs, type Theme } from '$lib/prefs';
  import MobileInbox from './MobileInbox.svelte';
  import MobileMessage from './MobileMessage.svelte';
  import MobileDrawer from './MobileDrawer.svelte';
  import MobileBrandBar from './MobileBrandBar.svelte';
  import MobileViewStrip from './MobileViewStrip.svelte';

  // The mobile shell intercepts the heavy surfaces (inbox, single
  // message) and renders a BlueMail-style UI instead. For paths the
  // mobile UI doesn't cover (settings, setup, compose) we just render
  // the default route content directly so those pages keep their own
  // internal chrome.

  interface Props {
    children?: Snippet;
  }

  let { children }: Props = $props();

  // Current mobile view is derived from the URL pathname so the
  // browser back button, bookmarks, and deep links all behave.
  let path = $derived($page.url.pathname);
  type ShellMode = 'inbox' | 'message' | 'other';
  let mode: ShellMode = $derived.by(() => {
    if (path.startsWith('/message/')) return 'message';
    if (path === '/' || path === '/inbox' || path.startsWith('/inbox')) return 'inbox';
    return 'other';
  });
  let messageId: number | null = $derived.by(() => {
    if (mode !== 'message') return null;
    const m = path.match(/^\/message\/(\d+)/);
    return m ? Number(m[1]) : null;
  });

  // Drawer + selection state. One selection across the app, not per-
  // page — flipping accounts in the drawer refreshes the inbox.
  let drawerOpen = $state(false);
  let accountId = $state<number | null>(null);
  let folder = $state<string>('INBOX');
  let folders = $state<FoldersResponse | null>(null);
  let reloadKey = $state(0);

  // Inline search — overlays the top-bar with an input field while
  // active. MobileInbox switches from listMessages to api.search
  // whenever `searchQuery` is non-empty. Cleared on close + when
  // navigating away from inbox.
  let searchActive = $state(false);
  let searchQuery = $state('');
  let searchEl = $state<HTMLInputElement | null>(null);
  let chromeCompact = $state(false);
  let prefersDark = $state(false);
  let currentTheme = $state<Theme>('system');
  const LOGO_VERSION = '4';

  let effectiveTheme = $derived(
    currentTheme === 'system' ? (prefersDark ? 'dark' : 'light') : currentTheme
  );
  let logoSrc = $derived(
    currentTheme === 'cyberpunk'
      ? `/logo-cyberpunk.png?v=${LOGO_VERSION}`
      : effectiveTheme === 'dark'
        ? `/logo-dark.png?v=${LOGO_VERSION}`
        : `/logo-light.png?v=${LOGO_VERSION}`
  );

  async function openSearch() {
    searchActive = true;
    if ($page.url.pathname !== '/inbox') goto('/inbox');
    await tick();
    searchEl?.focus();
  }
  function closeSearch() {
    searchActive = false;
    searchQuery = '';
  }

  function handleInboxScroll(state: { top: number; direction: 'up' | 'down' | 'none' }) {
    if (searchActive) {
      chromeCompact = false;
      return;
    }
    if (state.top < 18) {
      chromeCompact = false;
    } else if (state.direction === 'down' && state.top > 42) {
      chromeCompact = true;
    } else if (state.direction === 'up') {
      chromeCompact = false;
    }
  }

  async function lockNow() {
    closeSearch();
    try {
      await lockVault();
    } catch {
      /* even if the API call fails, force a reload below so the
         vault gate re-evaluates and locks the UI */
    }
    if (typeof window !== 'undefined') window.location.assign('/');
  }

  onMount(() => {
    loadFolders();
  });

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

  async function loadFolders() {
    try {
      folders = await api.folders();
      if (folders.accounts.length === 1 && accountId === null) {
        accountId = folders.accounts[0].account_id;
      }
    } catch {
      /* non-fatal — the inbox will show its own error */
    }
  }

  function openDrawer() { drawerOpen = true; }
  function closeDrawer() { drawerOpen = false; }

  function selectFolder(id: number | null, name: string) {
    accountId = id;
    folder = name;
    reloadKey += 1;
    closeDrawer();
    closeSearch();
    chromeCompact = false;
    if (path !== '/inbox') goto('/inbox');
  }

  function selectView(
    target: 'unified' | 'reminders' | 'calendar' | 'outbox' | 'contacts' | 'notes'
  ) {
    closeSearch();
    chromeCompact = false;
    if (target === 'unified') {
      accountId = null;
      folder = 'INBOX';
      reloadKey += 1;
      if (path !== '/inbox') goto('/inbox');
    } else if (target === 'reminders') {
      goto('/reminders');
    } else if (target === 'calendar') {
      goto('/calendar');
    } else if (target === 'outbox') {
      goto('/outbox');
    } else if (target === 'contacts') {
      goto('/contacts');
    } else if (target === 'notes') {
      goto('/notes');
    }
  }

  function refreshNow() {
    chromeCompact = false;
    reloadKey += 1;
  }

  function openSettings() {
    goto('/settings');
    closeDrawer();
  }

  function title(): string {
    if (mode === 'message') return '';
    if (folder === 'INBOX' && accountId === null) return 'Unified inbox';
    if (folder === 'INBOX') {
      const a = folders?.accounts.find((x) => x.account_id === accountId);
      return a ? a.email.split('@')[0] : 'Inbox';
    }
    if (folders && accountId !== null) {
      const a = folders.accounts.find((x) => x.account_id === accountId);
      const f = a?.system.find((x) => x.name === folder) ?? a?.user.find((x) => x.name === folder);
      return f?.display ?? folder;
    }
    return folder;
  }

  // Bottom nav destinations. Compose is a FAB, not a tab, which is
  // consistent with BlueMail and most modern mail apps. Folders opens
  // the drawer rather than navigating.
  type Tab = 'mail' | 'settings';
  let activeTab: Tab = $derived.by(() => {
    if (path.startsWith('/settings')) return 'settings';
    return 'mail';
  });

  function openCompose() { goto('/compose'); }
</script>

<div class="mobile-app">
  {#if mode === 'message' && messageId !== null}
    <MobileMessage {messageId} />
  {:else if mode === 'inbox'}
    {#if searchActive}
      <!-- Search overlay replaces the brand-bar while active.
           Filter strip stays so the user can flip view mid-search. -->
      <header class="top-bar search-bar">
        <button class="icon-btn" onclick={closeSearch} aria-label="close search">
          <svg width="22" height="22" viewBox="0 0 24 24" aria-hidden="true">
            <path d="M15 5 8 12l7 7" stroke="currentColor" stroke-width="2.6" fill="none" stroke-linecap="round" stroke-linejoin="round" />
          </svg>
        </button>
        <input
          bind:this={searchEl}
          bind:value={searchQuery}
          type="search"
          inputmode="search"
          enterkeyhint="search"
          autocomplete="off"
          spellcheck="false"
          placeholder="Search mail…"
          class="search-input"
          aria-label="Search mail"
        />
        {#if searchQuery}
          <button
            class="icon-btn"
            onclick={() => (searchQuery = '')}
            aria-label="clear search"
          >
            <svg width="20" height="20" viewBox="0 0 24 24" aria-hidden="true">
              <path d="M6 6l12 12M18 6 6 18" stroke="currentColor" stroke-width="2.4" fill="none" stroke-linecap="round" />
            </svg>
          </button>
        {/if}
      </header>
    {:else}
      <MobileBrandBar
        {logoSrc}
        compact={chromeCompact}
        onSearch={openSearch}
        onRefresh={refreshNow}
        onLock={lockNow}
      />
    {/if}

    <MobileViewStrip
      activeView={
        path.startsWith('/inbox') && accountId === null && folder === 'INBOX' ? 'unified'
        : path.startsWith('/reminders') ? 'reminders'
        : path.startsWith('/calendar') ? 'calendar'
        : path.startsWith('/outbox') ? 'outbox'
        : path.startsWith('/contacts') ? 'contacts'
        : path.startsWith('/notes') ? 'notes'
        : null
      }
      onSelect={selectView}
    />

    <MobileInbox
      {accountId}
      {folder}
      {reloadKey}
      query={searchActive ? searchQuery : ''}
      onScrollState={handleInboxScroll}
    />
  {:else}
    <!-- Settings / setup / compose / anything else — render the route
         content directly. These surfaces already have their own
         mobile-responsive styles. -->
    <div class="pass-through">
      {#if children}{@render children()}{/if}
    </div>
  {/if}

  {#if mode !== 'message'}
    <nav class="bottom-nav" aria-label="primary">
      <button class="tab" class:active={activeTab === 'mail'} onclick={() => goto('/inbox')}>
        <span class="glyph">
          {#if activeTab === 'mail'}
            <svg viewBox="0 0 24 24" aria-hidden="true">
              <path d="M4 6.5h16v11H4z" fill="currentColor" />
              <path d="m4.5 7 7.5 5.5L19.5 7" fill="none" stroke="var(--surface)" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" />
            </svg>
          {:else}
            <svg viewBox="0 0 24 24" aria-hidden="true">
              <path d="M4 6.5h16v11H4z" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linejoin="round" />
              <path d="m4.5 7 7.5 5.5L19.5 7" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" />
            </svg>
          {/if}
        </span>
        <span class="lbl">Mail</span>
      </button>
      <button class="tab" onclick={openDrawer}>
        <span class="glyph">
          <svg viewBox="0 0 24 24" aria-hidden="true">
            <path d="M3.5 8.5h6l1.8 2H20.5v8a1.5 1.5 0 0 1-1.5 1.5H5A1.5 1.5 0 0 1 3.5 18.5z" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linejoin="round" />
            <path d="M3.5 10V6.5A1.5 1.5 0 0 1 5 5h4.5l1.8 2H19a1.5 1.5 0 0 1 1.5 1.5V10" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linejoin="round" />
          </svg>
        </span>
        <span class="lbl">Folders</span>
      </button>
      <button class="tab" class:active={activeTab === 'settings'} onclick={openSettings}>
        <span class="glyph">
          {#if activeTab === 'settings'}
            <svg viewBox="0 0 24 24" aria-hidden="true">
              <path d="M19.3 13.4a7.8 7.8 0 0 0 0-2.8l2-1.5-2-3.4-2.4 1a8.1 8.1 0 0 0-2.4-1.4L14.1 2.8H9.9l-.4 2.5A8.1 8.1 0 0 0 7.1 6.7l-2.4-1-2 3.4 2 1.5a7.8 7.8 0 0 0 0 2.8l-2 1.5 2 3.4 2.4-1a8.1 8.1 0 0 0 2.4 1.4l.4 2.5h4.2l.4-2.5a8.1 8.1 0 0 0 2.4-1.4l2.4 1 2-3.4-2-1.5Z" fill="currentColor" />
              <circle cx="12" cy="12" r="3" fill="var(--surface)" />
            </svg>
          {:else}
            <svg viewBox="0 0 24 24" aria-hidden="true">
              <circle cx="12" cy="12" r="3.2" fill="none" stroke="currentColor" stroke-width="2.2" />
              <path d="M19.3 13.4a7.8 7.8 0 0 0 0-2.8l2-1.5-2-3.4-2.4 1a8.1 8.1 0 0 0-2.4-1.4L14.1 2.8H9.9l-.4 2.5A8.1 8.1 0 0 0 7.1 6.7l-2.4-1-2 3.4 2 1.5a7.8 7.8 0 0 0 0 2.8l-2 1.5 2 3.4 2.4-1a8.1 8.1 0 0 0 2.4 1.4l.4 2.5h4.2l.4-2.5a8.1 8.1 0 0 0 2.4-1.4l2.4 1 2-3.4-2-1.5Z" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round" />
            </svg>
          {/if}
        </span>
        <span class="lbl">Settings</span>
      </button>
      <button class="tab tab-compose" onclick={openCompose} aria-label="compose">
        <span class="glyph">
          <svg viewBox="0 0 24 24" aria-hidden="true">
            <path d="M14.5 4.5 19.5 9.5 8 21H3v-5L14.5 4.5Z" fill="currentColor" />
            <path d="m14.5 4.5 5 5" stroke="var(--accent)" stroke-width="1.6" fill="none" stroke-linecap="round" />
          </svg>
        </span>
        <span class="lbl">Compose</span>
      </button>
    </nav>
  {/if}

  <MobileDrawer
    open={drawerOpen}
    {folders}
    {accountId}
    {folder}
    onClose={closeDrawer}
    onSelect={selectFolder}
    onOpenSettings={openSettings}
  />
</div>

