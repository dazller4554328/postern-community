<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import type { FoldersResponse, AccountFolders, FolderEntry } from '$lib/api';
  import { colorForAccount } from '$lib/accountColor';
  import FolderIcon from '$lib/components/FolderIcon.svelte';

  interface Props {
    open: boolean;
    folders: FoldersResponse | null;
    accountId: number | null;
    folder: string;
    onClose: () => void;
    onSelect: (accountId: number | null, folder: string) => void;
    onOpenSettings: () => void;
  }

  let { open, folders, accountId, folder, onClose, onSelect, onOpenSettings }: Props = $props();

  function openDatas() {
    onClose();
    goto('/datas');
  }
  let collapsedAccounts = $state<Record<number, boolean>>({});
  let allMailboxesCollapsed = $derived.by(() => {
    if (!folders || folders.accounts.length === 0) return false;
    return folders.accounts.every((a) => collapsedAccounts[a.account_id]);
  });

  // A mobile drawer should let you:
  //   - flip between unified and per-account inbox
  //   - see the common system folders for the current selection
  //   - punt to settings for everything else
  // Anything heavier (rules, VPN, PGP) lives in /settings on the
  // mobile side too, since the bulk of those pages already have
  // sensible mobile styles.

  onMount(() => {
    try {
      const saved = localStorage.getItem('postern.sidebar.collapsedAccounts');
      if (saved) collapsedAccounts = JSON.parse(saved);
    } catch {}
  });

  function persistCollapsed() {
    try {
      localStorage.setItem('postern.sidebar.collapsedAccounts', JSON.stringify(collapsedAccounts));
    } catch {}
  }

  function toggleAccount(id: number) {
    collapsedAccounts = { ...collapsedAccounts, [id]: !collapsedAccounts[id] };
    persistCollapsed();
  }

  function setAllCollapsed(collapsed: boolean) {
    if (!folders) return;
    collapsedAccounts = Object.fromEntries(folders.accounts.map((a) => [a.account_id, collapsed]));
    persistCollapsed();
  }

  function pickAccount(id: number | null) {
    // Opening a fresh account resets to the primary Inbox. We match
    // the desktop behaviour: Inbox = "INBOX" for IMAP, Gmail's
    // INBOX label for Gmail. The server normalises the former for
    // non-Gmail providers, so "INBOX" is the safe default.
    onSelect(id, 'INBOX');
  }

  function pickFolder(id: number | null, name: string) {
    onSelect(id, name);
  }

  function accountLabel(a: AccountFolders): string {
    return a.email;
  }

  function visibleFolders(a: AccountFolders): FolderEntry[] {
    const seen = new Set<string>();
    return [...a.system, ...a.user].filter((f) => {
      const key = f.name.toLowerCase();
      if (seen.has(key)) return false;
      seen.add(key);
      return true;
    });
  }
</script>

<div class="scrim" class:open role="presentation" onclick={onClose}></div>

<aside class="drawer" class:open aria-label="Folders and accounts" aria-hidden={!open}>
  <div class="drawer-head">
    <div class="brand">Postern</div>
    <button class="close" onclick={onClose} aria-label="close drawer">
      <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true">
        <path d="m6 6 12 12M18 6 6 18" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" />
      </svg>
    </button>
  </div>

  <nav class="drawer-body">
    <!-- Unified inbox — select all accounts -->
    {#if folders && folders.accounts.length > 1}
      <button
        class="row unified"
        class:active={accountId === null}
        onclick={() => pickAccount(null)}
      >
        <span class="icon"><FolderIcon name="INBOX" kind="system" /></span>
        <span class="label">All inboxes</span>
        {#if folders.accounts.reduce((n, a) => n + (a.system.find((f) => f.name.toLowerCase() === 'inbox')?.unread ?? 0), 0) > 0}
          <span class="badge">
            {folders.accounts.reduce((n, a) => n + (a.system.find((f) => f.name.toLowerCase() === 'inbox')?.unread ?? 0), 0)}
          </span>
        {/if}
      </button>
    {/if}

    {#if folders}
      <div class="mailbox-tools">
        <span>Mailboxes</span>
        <button type="button" onclick={() => setAllCollapsed(!allMailboxesCollapsed)}>
          {allMailboxesCollapsed ? 'Expand all' : 'Collapse all'}
        </button>
      </div>
      {#each folders.accounts as a (a.account_id)}
        {@const collapsed = collapsedAccounts[a.account_id]}
        <div class="account-block">
          <div class="account-head">
            <button
              class="row account"
              class:active={accountId === a.account_id && folder.toLowerCase() === 'inbox'}
              onclick={() => pickAccount(a.account_id)}
            >
              <span
                class="account-chip"
                style:background-color={colorForAccount({ id: a.account_id, color: a.color })}
                aria-hidden="true"
              ></span>
              <span class="label">{accountLabel(a)}</span>
            </button>
            <button
              type="button"
              class="collapse-toggle"
              class:open={!collapsed}
              onclick={() => toggleAccount(a.account_id)}
              aria-label={collapsed ? 'Expand mailbox' : 'Collapse mailbox'}
              aria-expanded={!collapsed}
            >
              <svg viewBox="0 0 16 16" width="16" height="16" aria-hidden="true">
                <path d="m6 3 5 5-5 5" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" />
              </svg>
            </button>
          </div>

          {#if !collapsed}
            <div class="folders">
              {#each visibleFolders(a) as f (f.name)}
                <div class="tree-row-wrap">
                  <span class="tree-conn" aria-hidden="true"></span>
                  <button
                    class="row sub"
                    class:active={accountId === a.account_id && folder === f.name}
                    onclick={() => pickFolder(a.account_id, f.name)}
                  >
                    <span class="icon"><FolderIcon name={f.name} kind={f.kind} /></span>
                    <span class="label">{f.display}</span>
                    {#if f.unread > 0}
                      <span class="badge">{f.unread}</span>
                    {/if}
                  </button>
                </div>
              {/each}
            </div>
          {/if}
        </div>
      {/each}
    {/if}

    <div class="divider"></div>

    <button class="row" onclick={openDatas}>
      <span class="icon">
        <svg viewBox="0 0 24 24" width="19" height="19" aria-hidden="true">
          <path d="M12 3.5l1.85 4.65L18.5 10l-4.65 1.85L12 16.5l-1.85-4.65L5.5 10l4.65-1.85L12 3.5Z" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round" />
          <path d="M18.5 14.5l.85 2.15L21.5 17.5l-2.15.85L18.5 20.5l-.85-2.15L15.5 17.5l2.15-.85L18.5 14.5Z" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round" />
        </svg>
      </span>
      <span class="label">Datas</span>
    </button>

    <button class="row" onclick={() => { onOpenSettings(); onClose(); }}>
      <span class="icon">
        <svg viewBox="0 0 24 24" width="19" height="19" aria-hidden="true">
          <path d="M12 8.2a3.8 3.8 0 1 0 0 7.6 3.8 3.8 0 0 0 0-7.6Z" fill="none" stroke="currentColor" stroke-width="1.8" />
          <path d="M19.4 13.5a7.9 7.9 0 0 0 0-3l2-1.5-2-3.5-2.4 1a8.6 8.6 0 0 0-2.6-1.5L14 2.5h-4l-.4 2.5A8.6 8.6 0 0 0 7 6.5l-2.4-1-2 3.5 2 1.5a7.9 7.9 0 0 0 0 3l-2 1.5 2 3.5 2.4-1a8.6 8.6 0 0 0 2.6 1.5L10 21.5h4l.4-2.5a8.6 8.6 0 0 0 2.6-1.5l2.4 1 2-3.5-2-1.5Z" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round" />
        </svg>
      </span>
      <span class="label">Settings</span>
    </button>
  </nav>
</aside>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.5);
    opacity: 0;
    pointer-events: none;
    transition: opacity 220ms ease;
    z-index: 60;
  }
  .scrim.open {
    opacity: 1;
    pointer-events: auto;
  }

  .drawer {
    position: fixed;
    top: 0;
    left: 0;
    /* 100dvh keeps the drawer honest on mobile browsers whose toolbars
       shrink-grow the viewport — otherwise the bottom of a long
       account list falls below the visible area and reads as "cut
       off". */
    bottom: 0;
    height: 100dvh;
    width: min(92vw, 23.5rem);
    background: var(--surface);
    border-right: 1px solid var(--border);
    transform: translateX(-100%);
    transition: transform 240ms cubic-bezier(0.2, 0.8, 0.2, 1);
    z-index: 70;
    display: flex;
    flex-direction: column;
    box-shadow: 0 20px 60px rgba(0, 0, 0, 0.28);
  }
  .drawer.open {
    transform: translateX(0);
  }

  .drawer-head {
    padding: 0.9rem 0.95rem 0.75rem;
    padding-top: max(0.9rem, env(safe-area-inset-top));
    display: flex;
    align-items: center;
    justify-content: space-between;
    border-bottom: 1px solid var(--border);
    background: color-mix(in oklab, var(--surface) 92%, var(--surface-2));
  }
  .brand {
    font-weight: 700;
    font-size: 1.1rem;
    letter-spacing: 0;
  }
  .close {
    background: transparent;
    border: 0;
    color: var(--muted);
    width: 40px;
    height: 40px;
    padding: 0;
    border-radius: 999px;
    cursor: pointer;
    display: inline-grid;
    place-items: center;
  }
  .close:hover { background: var(--row-hover); color: var(--fg); }

  .drawer-body {
    flex: 1 1 auto;
    min-height: 0;
    overflow-y: auto;
    -webkit-overflow-scrolling: touch;
    overscroll-behavior: contain;
    padding: 0.5rem 0;
    /* Leave room past the home-indicator + one comfortable tap so the
       final row isn't glued to the safe-area edge. */
    padding-bottom: calc(4.5rem + env(safe-area-inset-bottom));
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .mailbox-tools {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    padding: 0.75rem 1rem 0.35rem;
    color: var(--muted);
    font-size: 0.72rem;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }
  .mailbox-tools button {
    border: 1px solid color-mix(in oklab, currentColor 12%, transparent);
    background: color-mix(in oklab, var(--surface-2) 72%, transparent);
    color: var(--fg);
    border-radius: 999px;
    padding: 0.32rem 0.58rem;
    font: inherit;
    font-size: 0.7rem;
    letter-spacing: 0;
    text-transform: none;
    cursor: pointer;
  }
  .mailbox-tools button:hover {
    background: color-mix(in oklab, var(--accent) 12%, var(--surface));
    border-color: color-mix(in oklab, var(--accent) 28%, transparent);
  }

  .row {
    display: flex;
    align-items: center;
    gap: 0.7rem;
    padding: 0.62rem 1rem;
    background: transparent;
    border: 0;
    color: var(--fg);
    font-size: 0.94rem;
    text-align: left;
    cursor: pointer;
    width: 100%;
  }
  .row:hover { background: var(--row-hover); }
  .row.active {
    background: var(--row-selected);
    color: var(--accent);
    font-weight: 600;
  }
  .row.sub {
    padding-left: 0.45rem;
    padding-right: 0.75rem;
    font-size: 0.9rem;
    border-radius: 0.45rem;
  }
  .row.unified {
    font-weight: 600;
    padding: 0.7rem 1rem;
  }
  .row.account {
    padding-top: 0.68rem;
    padding-bottom: 0.68rem;
    font-weight: 600;
  }

  .icon {
    width: 22px;
    height: 22px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex: 0 0 auto;
    color: var(--muted);
  }
  .row.active .icon { color: var(--accent); }

  .account-chip {
    flex: 0 0 auto;
    width: 0.95rem;
    height: 0.95rem;
    border-radius: 999px;
    box-shadow:
      0 0 0 2px color-mix(in oklab, currentColor 8%, transparent),
      inset 0 1px 0 rgba(255, 255, 255, 0.18);
  }

  .label {
    flex: 1 1 auto;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .badge {
    flex: 0 0 auto;
    background: var(--accent);
    color: #fff;
    font-size: 0.72rem;
    font-weight: 700;
    padding: 2px 8px;
    border-radius: 999px;
    min-width: 22px;
    text-align: center;
  }

  .account-block {
    margin: 0.2rem 0.55rem 0;
    border-radius: 0.8rem;
    overflow: visible;
  }

  .account-head {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 42px;
    align-items: center;
    border-radius: 0.8rem;
  }
  .account-head .row.account {
    padding-left: 0.45rem;
    border-radius: 0.8rem 0 0 0.8rem;
  }
  .collapse-toggle {
    width: 42px;
    height: 42px;
    display: inline-grid;
    place-items: center;
    border: 0;
    border-radius: 0 0.8rem 0.8rem 0;
    background: transparent;
    color: var(--muted);
    cursor: pointer;
  }
  .collapse-toggle.open svg {
    transform: rotate(90deg);
  }
  .collapse-toggle:hover {
    color: var(--fg);
    background: var(--row-hover);
  }
  .collapse-toggle svg {
    transition: transform 140ms ease;
  }

  .folders {
    display: flex;
    flex-direction: column;
    gap: 0.08rem;
    padding: 0.08rem 0 0.18rem;
    margin-left: 0.95rem;
  }

  .tree-row-wrap {
    display: flex;
    align-items: stretch;
    min-height: 30px;
  }

  .tree-conn {
    width: 1.1rem;
    position: relative;
    flex: 0 0 auto;
    pointer-events: none;
  }
  .tree-conn::before {
    content: '';
    position: absolute;
    left: 0.52rem;
    top: 0;
    bottom: 0;
    width: 1px;
    background: color-mix(in oklab, currentColor 20%, transparent);
    opacity: 0.7;
  }
  .tree-conn::after {
    content: '';
    position: absolute;
    left: 0.52rem;
    top: 50%;
    width: 0.62rem;
    height: 1px;
    background: color-mix(in oklab, currentColor 20%, transparent);
    opacity: 0.7;
  }

  .divider {
    height: 1px;
    background: var(--border);
    margin: 0.6rem 0.9rem;
  }
</style>
