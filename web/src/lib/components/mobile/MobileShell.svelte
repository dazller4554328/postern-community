<script lang="ts">
  import { page } from '$app/stores';
  import { goto } from '$app/navigation';
  import { onMount, tick, type Snippet } from 'svelte';
  import { api, type FoldersResponse } from '$lib/api';
  import { lockVault } from '$lib/vault';
  import { prefs, type Theme } from '$lib/prefs';
  import MobileInbox from './MobileInbox.svelte';
  import MobileMessage from './MobileMessage.svelte';
  import MobileDrawer from './MobileDrawer.svelte';

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
  // the drawer rather than navigating. Datas opens the standalone
  // /datas page where the AskBox runs in fullscreen mode.
  type Tab = 'mail' | 'datas' | 'settings';
  let activeTab: Tab = $derived.by(() => {
    if (path.startsWith('/settings')) return 'settings';
    if (path.startsWith('/datas')) return 'datas';
    return 'mail';
  });

  function openCompose() { goto('/compose'); }
  function openDatas() { goto('/datas'); }
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
      <!-- Brand bar — compact Postern identity on the left, action icons
           on the right (search / refresh / lock). Telegram-style. -->
      <header class="brand-bar" class:compact={chromeCompact}>
        <a href="/inbox" class="brand-link" aria-label="Postern home">
          <span class="brand-dot" aria-hidden="true"></span>
          <img src={logoSrc} alt="Postern" class="brand-logo" />
        </a>
        <div class="brand-actions">
          <button class="icon-btn" onclick={openSearch} aria-label="search">
            <svg width="22" height="22" viewBox="0 0 24 24" aria-hidden="true">
              <circle cx="10.5" cy="10.5" r="6.5" fill="none" stroke="currentColor" stroke-width="2.2" />
              <path d="m15.5 15.5 5 5" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" />
            </svg>
          </button>
          <button class="icon-btn" onclick={refreshNow} aria-label="refresh">
            <svg width="22" height="22" viewBox="0 0 24 24" aria-hidden="true">
              <path d="M20 12a8 8 0 1 1-2.34-5.66M20 4v5h-5" stroke="currentColor" stroke-width="2.4" fill="none" stroke-linecap="round" stroke-linejoin="round" />
            </svg>
          </button>
          <button class="icon-btn" onclick={lockNow} aria-label="lock vault" title="Lock vault">
            <svg width="22" height="22" viewBox="0 0 24 24" aria-hidden="true">
              <rect x="5" y="11" width="14" height="9" rx="1.6" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linejoin="round" />
              <path d="M8 11V7.5a4 4 0 0 1 8 0V11" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" />
            </svg>
          </button>
        </div>
      </header>
    {/if}

    <!-- View filter strip — five primary destinations as icons,
         active view highlighted. Fits in one row at any width
         because each chip is icon-only. Replaces the old
         dropdown picker (which hid these behind a tap). -->
    <nav class="view-strip" aria-label="Switch view">
      <button
        class="view-chip"
        class:active={path.startsWith('/inbox') && accountId === null && folder === 'INBOX'}
        onclick={() => selectView('unified')}
        aria-label="Unified inbox"
        title="Unified inbox"
      >
        <svg viewBox="0 0 24 24" width="22" height="22" aria-hidden="true">
          <path d="M3 6h18v12H3z" fill="currentColor" opacity="0.15" />
          <path d="M3 6h18v12H3z" fill="none" stroke="currentColor" stroke-width="2" stroke-linejoin="round" />
          <path d="m3.5 6.5 8.5 6.5L20.5 6.5" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" />
        </svg>
      </button>
      <button
        class="view-chip"
        class:active={path.startsWith('/reminders')}
        onclick={() => selectView('reminders')}
        aria-label="Reminders"
        title="Reminders"
      >
        <svg viewBox="0 0 24 24" width="22" height="22" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <circle cx="12" cy="13" r="7.5" />
          <path d="M12 9v4l2.5 1.5" />
          <path d="M9 3.5h6M5 6l2-2M19 6l-2-2" />
        </svg>
      </button>
      <button
        class="view-chip"
        class:active={path.startsWith('/calendar')}
        onclick={() => selectView('calendar')}
        aria-label="Calendar"
        title="Calendar"
      >
        <svg viewBox="0 0 24 24" width="22" height="22" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <rect x="3.5" y="5.5" width="17" height="14" rx="1.5" />
          <path d="M3.5 10h17M8 3.5v4M16 3.5v4" />
        </svg>
      </button>
      <button
        class="view-chip"
        class:active={path.startsWith('/outbox')}
        onclick={() => selectView('outbox')}
        aria-label="Outbox"
        title="Outbox"
      >
        <svg viewBox="0 0 24 24" width="22" height="22" fill="none" stroke="currentColor" stroke-width="2" stroke-linejoin="round" stroke-linecap="round" aria-hidden="true">
          <path d="M4 4h12l4 4v12H4z" />
          <path d="M8 4v6h8V4M8 14h8" />
        </svg>
      </button>
      <button
        class="view-chip"
        class:active={path.startsWith('/contacts')}
        onclick={() => selectView('contacts')}
        aria-label="Contacts"
        title="Contacts"
      >
        <svg viewBox="0 0 24 24" width="22" height="22" fill="none" stroke="currentColor" stroke-width="2" stroke-linejoin="round" stroke-linecap="round" aria-hidden="true">
          <circle cx="12" cy="9" r="3.5" />
          <path d="M5.5 19.5c0-3.6 2.9-6 6.5-6s6.5 2.4 6.5 6" />
        </svg>
      </button>
      <button
        class="view-chip"
        class:active={path.startsWith('/notes')}
        onclick={() => selectView('notes')}
        aria-label="Notes"
        title="Notes"
      >
        <svg viewBox="0 0 24 24" width="22" height="22" fill="none" stroke="currentColor" stroke-width="2" stroke-linejoin="round" stroke-linecap="round" aria-hidden="true">
          <path d="M6 3.5h8L18.5 8v12.5h-12.5z" />
          <path d="M14 3.5V8h4.5" />
          <path d="M9 13h6M9 16h6M9 10h3" />
        </svg>
      </button>
    </nav>

    <MobileInbox
      {accountId}
      {folder}
      {reloadKey}
      query={searchActive ? searchQuery : ''}
      onScrollState={handleInboxScroll}
    />

    <button class="fab" onclick={openCompose} aria-label="compose">
      <!-- Bolder filled pencil-edit glyph. -->
      <svg width="24" height="24" viewBox="0 0 24 24" aria-hidden="true">
        <path d="M14.5 4.5 19.5 9.5 8 21H3v-5L14.5 4.5Z" fill="currentColor" />
        <path d="m14.5 4.5 5 5" stroke="var(--surface)" stroke-width="1.6" fill="none" stroke-linecap="round" />
      </svg>
    </button>

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
      <button class="tab" class:active={activeTab === 'datas'} onclick={openDatas}>
        <span class="glyph">
          {#if activeTab === 'datas'}
            <!-- Filled spark / sparkles icon — same vibe as a star
                 but more "AI assistant" coded so users intuit this is
                 the inbox-AI thing rather than a favourites tab. -->
            <svg viewBox="0 0 24 24" aria-hidden="true">
              <path d="M12 3.5l1.85 4.65L18.5 10l-4.65 1.85L12 16.5l-1.85-4.65L5.5 10l4.65-1.85L12 3.5Z" fill="currentColor" />
              <path d="M18.5 14.5l.85 2.15L21.5 17.5l-2.15.85L18.5 20.5l-.85-2.15L15.5 17.5l2.15-.85L18.5 14.5Z" fill="currentColor" />
            </svg>
          {:else}
            <svg viewBox="0 0 24 24" aria-hidden="true">
              <path d="M12 3.5l1.85 4.65L18.5 10l-4.65 1.85L12 16.5l-1.85-4.65L5.5 10l4.65-1.85L12 3.5Z" fill="none" stroke="currentColor" stroke-width="2" stroke-linejoin="round" />
              <path d="M18.5 14.5l.85 2.15L21.5 17.5l-2.15.85L18.5 20.5l-.85-2.15L15.5 17.5l2.15-.85L18.5 14.5Z" fill="none" stroke="currentColor" stroke-width="2" stroke-linejoin="round" />
            </svg>
          {/if}
        </span>
        <span class="lbl">Datas</span>
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

<style>
  .mobile-app {
    display: flex;
    flex-direction: column;
    height: 100dvh;
    width: 100%;
    background: var(--bg);
    overflow: hidden;
  }

  .top-bar {
    flex: 0 0 auto;
    display: flex;
    align-items: center;
    gap: 0.25rem;
    padding: 0.4rem 0.55rem;
    padding-top: max(0.35rem, env(safe-area-inset-top));
    background: var(--surface);
    border-bottom: 1px solid var(--border);
    position: relative;
    z-index: 40;
  }
  .icon-btn {
    background: transparent;
    border: 0;
    color: var(--fg);
    width: 42px;
    height: 42px;
    padding: 0;
    border-radius: 999px;
    cursor: pointer;
    touch-action: manipulation;
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }
  .icon-btn:hover,
  .icon-btn:active { background: var(--row-hover); }

  /* Search overlay — replaces the whole top-bar while active. Same
     vertical rhythm as the normal bar so the layout doesn't jump. */
  .top-bar.search-bar {
    gap: 0.5rem;
    padding: 0.4rem 0.55rem;
    padding-top: max(0.4rem, env(safe-area-inset-top));
  }
  .search-input {
    flex: 1 1 auto;
    min-width: 0;
    padding: 0.55rem 0.7rem;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: var(--surface-2, color-mix(in oklab, currentColor 6%, var(--surface)));
    color: var(--fg);
    font: inherit;
    font-size: 0.95rem;
  }
  .search-input:focus {
    outline: none;
    border-color: color-mix(in oklab, var(--accent) 50%, transparent);
    box-shadow: 0 0 0 3px color-mix(in oklab, var(--accent) 18%, transparent);
  }

  /* Brand bar — Telegram-style top row. Compact Postern identity on the
     left, action icons on the right (search / refresh / lock). */
  .brand-bar {
    flex: 0 0 auto;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    min-height: calc(3.5rem + env(safe-area-inset-top));
    max-height: calc(3.5rem + env(safe-area-inset-top));
    padding: 0.48rem 0.7rem 0.5rem;
    padding-top: max(0.48rem, env(safe-area-inset-top));
    background: var(--surface);
    position: relative;
    z-index: 41;
    overflow: hidden;
    transition:
      max-height 180ms ease,
      min-height 180ms ease,
      padding 180ms ease,
      opacity 140ms ease,
      transform 180ms ease;
    will-change: max-height, transform, opacity;
  }
  .brand-bar.compact {
    min-height: 0;
    max-height: 0;
    padding-top: 0;
    padding-bottom: 0;
    opacity: 0;
    transform: translateY(-10px);
    pointer-events: none;
  }
  .brand-link {
    display: inline-flex;
    align-items: center;
    gap: 0.55rem;
    min-width: 0;
    text-decoration: none;
    color: inherit;
    padding: 0.12rem 0.28rem 0.12rem 0.1rem;
    border-radius: 0.8rem;
  }
  .brand-link:active { background: var(--row-hover); }
  .brand-dot {
    width: 0.92rem;
    height: 0.92rem;
    border-radius: 0.26rem;
    background:
      linear-gradient(135deg, var(--accent), color-mix(in oklab, var(--accent) 40%, white 60%));
    box-shadow:
      0 0 0 1px color-mix(in oklab, var(--accent) 18%, transparent),
      0 8px 18px color-mix(in oklab, var(--accent) 18%, transparent);
    flex: 0 0 auto;
  }
  .brand-logo {
    display: block;
    height: 29px;
    width: auto;
    max-width: min(45vw, 168px);
    object-fit: contain;
    object-position: left center;
    min-width: 0;
  }
  .brand-actions {
    display: inline-flex;
    align-items: center;
    gap: 0.06rem;
    flex-shrink: 0;
  }

  /* View filter strip — five icon-only chips below the brand bar.
     Active chip gets the accent tint so the user knows where they
     are without label text. */
  .view-strip {
    flex: 0 0 auto;
    display: flex;
    align-items: stretch;
    justify-content: space-around;
    gap: 0.3rem;
    padding: 0.28rem 0.7rem 0.5rem;
    background: var(--surface);
    border-bottom: 1px solid var(--border);
    position: relative;
    z-index: 40;
    box-shadow: 0 1px 0 color-mix(in oklab, var(--border) 50%, transparent);
  }
  .view-chip {
    flex: 1 1 0;
    min-width: 0;
    height: 36px;
    display: inline-grid;
    place-items: center;
    background: transparent;
    border: 0;
    color: color-mix(in oklab, currentColor 55%, transparent);
    border-radius: 999px;
    cursor: pointer;
    touch-action: manipulation;
    transition: background-color 140ms ease, color 140ms ease;
  }
  .view-chip:active {
    background: var(--row-hover);
    transform: scale(0.97);
  }
  .view-chip.active {
    color: var(--accent);
    background: color-mix(in oklab, var(--accent) 12%, transparent);
  }

  @media (max-width: 380px) {
    .brand-bar {
      gap: 0.25rem;
      padding-left: 0.55rem;
      padding-right: 0.5rem;
    }
    .brand-logo {
      height: 27px;
      max-width: 136px;
    }
    .icon-btn {
      width: 39px;
      height: 39px;
    }
  }

  /* Floating compose FAB. Sits above the floating nav. */
  .fab {
    position: fixed;
    bottom: calc(5.2rem + max(0.25rem, env(safe-area-inset-bottom)));
    right: 1rem;
    width: 56px;
    height: 56px;
    border-radius: 18px;
    background: var(--accent);
    color: #fff;
    border: 0;
    box-shadow:
      0 10px 24px rgba(0, 0, 0, 0.28),
      0 2px 4px rgba(0, 0, 0, 0.12);
    cursor: pointer;
    display: grid;
    place-items: center;
    z-index: 30;
    transition: transform 140ms ease, box-shadow 140ms ease;
    touch-action: manipulation;
  }
  .fab:active {
    transform: scale(0.94);
    box-shadow: 0 4px 10px rgba(0, 0, 0, 0.24);
  }

  /* Telegram-style floating nav pill. Raised off the viewport edge,
     rounded, with a soft shadow so it reads as an overlay layer rather
     than chrome. */
  .bottom-nav {
    position: fixed;
    left: 0.75rem;
    right: 0.75rem;
    bottom: calc(0.55rem + env(safe-area-inset-bottom));
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 2px;
    background: color-mix(in oklab, var(--surface) 94%, transparent);
    backdrop-filter: blur(14px) saturate(140%);
    -webkit-backdrop-filter: blur(14px) saturate(140%);
    border: 1px solid var(--border);
    border-radius: 22px;
    padding: 0.35rem;
    box-shadow:
      0 14px 34px rgba(0, 0, 0, 0.22),
      0 4px 10px rgba(0, 0, 0, 0.10);
    z-index: 25;
  }
  .tab {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 2px;
    min-height: 52px;
    padding: 0.34rem 0.25rem;
    background: transparent;
    border: 0;
    border-radius: 16px;
    color: var(--muted);
    cursor: pointer;
    touch-action: manipulation;
    transition: background 160ms ease, color 160ms ease;
  }
  .tab:active {
    background: var(--row-hover);
    transform: scale(0.97);
  }
  .tab:disabled { opacity: 0.4; cursor: default; }
  .tab.active {
    color: var(--accent);
    background: color-mix(in oklab, var(--accent) 12%, transparent);
  }
  .tab .glyph {
    width: 24px;
    height: 24px;
    display: inline-grid;
    place-items: center;
  }
  .tab .glyph svg {
    width: 24px;
    height: 24px;
    display: block;
  }
  .tab .lbl {
    font-size: 0.7rem;
    line-height: 1;
    font-weight: 600;
    letter-spacing: 0.01em;
  }

  .pass-through {
    flex: 1 1 auto;
    overflow-y: auto;
    min-height: 0;
    padding-bottom: calc(5.4rem + env(safe-area-inset-bottom));
  }
</style>
