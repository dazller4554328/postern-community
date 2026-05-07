<script lang="ts">
  import { goto } from '$app/navigation';
  import { api, type Account, type MessageListItem } from '$lib/api';
  import { onMount } from 'svelte';
  import { formatRelative, formatSender } from '$lib/format';
  import { buildAccountColorMap } from '$lib/accountColor';
  import SenderAvatar from '$lib/components/SenderAvatar.svelte';
  import { swipe } from './swipe';

  interface Props {
    /** Currently-selected account id, or null for unified. */
    accountId: number | null;
    /** Active folder name (e.g. "INBOX", "[Gmail]/Starred"). */
    folder: string;
    /** Refresh trigger key — changes when the parent wants a reload. */
    reloadKey?: number;
    /** When set, switches from listMessages → search. Empty string
     *  (or undefined) shows the normal folder listing. */
    query?: string;
    /** Lets the shell collapse nonessential chrome while the list scrolls. */
    onScrollState?: (state: { top: number; direction: 'up' | 'down' | 'none' }) => void;
  }

  let { accountId, folder, reloadKey = 0, query = '', onScrollState }: Props = $props();

  let messages = $state<MessageListItem[]>([]);
  let loading = $state(true);
  let loadingMore = $state(false);
  let err = $state<string | null>(null);
  let hasMore = $state(true);

  // Account-colour map for the unread pill. Loaded once on mount —
  // cheap call, doesn't change unless the user adds/edits a mailbox
  // in Settings (which would re-mount this component on navigate-back).
  let accounts = $state<Account[]>([]);
  let accountColorMap = $derived(buildAccountColorMap(accounts));
  onMount(() => {
    void api.listAccounts().then((a) => (accounts = a)).catch(() => null);
  });

  const PAGE_SIZE = 40;

  async function load(reset: boolean) {
    if (reset) {
      loading = true;
      messages = [];
      hasMore = true;
    } else {
      if (!hasMore || loadingMore) return;
      loadingMore = true;
    }
    try {
      const offset = reset ? 0 : messages.length;
      const q = query.trim();
      const rows = q
        ? await api.search({
            q,
            account_id: accountId ?? undefined,
            limit: PAGE_SIZE,
            offset,
            sort: 'date_desc'
          })
        : await api.listMessages({
            account_id: accountId ?? undefined,
            label: folder,
            limit: PAGE_SIZE,
            offset,
            sort: 'date_desc'
          });
      messages = reset ? rows : [...messages, ...rows];
      hasMore = rows.length === PAGE_SIZE;
      err = null;
    } catch (e: unknown) {
      err = e instanceof Error ? e.message : String(e);
    } finally {
      loading = false;
      loadingMore = false;
    }
  }

  // Re-fetch whenever the selection, query, or reload key changes.
  $effect(() => {
    // Read the reactive deps so Svelte tracks them.
    void accountId;
    void folder;
    void reloadKey;
    void query;
    load(true);
  });

  async function archive(msg: MessageListItem) {
    // Optimistic remove.
    const prev = messages;
    messages = messages.filter((m) => m.id !== msg.id);
    try {
      await api.archiveMessage(msg.id);
    } catch (e: unknown) {
      messages = prev;
      err = e instanceof Error ? e.message : String(e);
    }
  }

  async function trash(msg: MessageListItem) {
    const prev = messages;
    messages = messages.filter((m) => m.id !== msg.id);
    try {
      await api.markTrash(msg.id);
    } catch (e: unknown) {
      messages = prev;
      err = e instanceof Error ? e.message : String(e);
    }
  }

  async function toggleRead(msg: MessageListItem) {
    const was = msg.is_read;
    messages = messages.map((m) => (m.id === msg.id ? { ...m, is_read: !was } : m));
    try {
      await api.setMessageRead(msg.id, !was);
    } catch {
      messages = messages.map((m) => (m.id === msg.id ? { ...m, is_read: was } : m));
    }
  }

  function openMessage(msg: MessageListItem) {
    // Flip the row to read locally and fire-and-forget on the server,
    // so the envelope updates immediately and the inbox re-fetch on
    // return reflects the new state. Matches desktop selectMessage.
    if (!msg.is_read) {
      messages = messages.map((m) => (m.id === msg.id ? { ...m, is_read: true } : m));
      api.setMessageRead(msg.id, true).catch(() => {
        /* swallow — next list refresh will heal any drift */
      });
    }
    goto(`/message/${msg.id}`);
  }

  let listEl = $state<HTMLElement | null>(null);
  let lastScrollTop = 0;
  function onScroll() {
    if (!listEl) return;
    const top = listEl.scrollTop;
    const delta = top - lastScrollTop;
    const direction = Math.abs(delta) < 4 ? 'none' : delta > 0 ? 'down' : 'up';
    lastScrollTop = Math.max(0, top);
    onScrollState?.({ top, direction });

    const near = top + listEl.clientHeight > listEl.scrollHeight - 600;
    if (near) void load(false);
  }
</script>

<div class="mobile-inbox-list" bind:this={listEl} onscroll={onScroll}>
  {#if loading}
    <div class="state">Loading…</div>
  {:else if err}
    <div class="state error">Couldn't load messages. {err}</div>
  {:else if messages.length === 0}
    <div class="state empty">No messages in this folder.</div>
  {:else}
    {#each messages as msg (msg.id)}
      <div
        class="row"
        class:unread={!msg.is_read}
        role="button"
        tabindex="0"
        onclick={() => openMessage(msg)}
        onkeydown={(e) => {
          if (e.key === 'Enter' || e.key === ' ') {
            e.preventDefault();
            openMessage(msg);
          }
        }}
        use:swipe={{
          onLeft: () => archive(msg),
          onRight: () => toggleRead(msg)
        }}
      >
        <div class="swipe-bg left" aria-hidden="true">
          <span>Archive</span>
        </div>
        <div class="swipe-bg right" aria-hidden="true">
          <span>{msg.is_read ? 'Mark unread' : 'Mark read'}</span>
        </div>

        <div class="row-inner">
          <span
            class="m-envelope"
            class:unread={!msg.is_read}
            class:encrypted={msg.is_encrypted}
            style:--pill-color={accountColorMap[msg.account_id] ?? 'var(--accent)'}
            aria-label={`${msg.is_read ? 'Read' : 'Unread'}${msg.is_encrypted ? ' PGP encrypted' : ''}`}
          >
            {#if msg.is_read}
              <svg viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round" stroke-linecap="round" aria-hidden="true">
                <path d="M3 9.5 12 15l9-5.5"/>
                <path d="M3 9.5v10h18v-10"/>
                <path d="M3 9.5 12 4l9 5.5"/>
              </svg>
            {:else}
              <svg viewBox="0 0 24 24" width="20" height="20" fill="currentColor" aria-hidden="true">
                <path d="M2.5 6.5A1.5 1.5 0 0 1 4 5h16a1.5 1.5 0 0 1 1.5 1.5v11A1.5 1.5 0 0 1 20 19H4a1.5 1.5 0 0 1-1.5-1.5v-11Zm1.6.2 7.9 5.5 7.9-5.5-.5-.2H4.5l-.4.2Z"/>
              </svg>
            {/if}
            {#if msg.is_encrypted}<span class="pgp-flag" aria-hidden="true">PGP</span>{/if}
          </span>
          <SenderAvatar email={msg.from_addr} size={40} fetchRemote={true} />

          <div class="body">
            <div class="line1">
              <span class="sender" class:bold={!msg.is_read}>{formatSender(msg.from_addr)}</span>
              <span class="time">{formatRelative(msg.date_utc)}</span>
            </div>
            <div class="line2">
              <span class="subject" class:bold={!msg.is_read}>
                {msg.subject || '(no subject)'}
              </span>
            </div>
            <div class="line3">
              <span class="snippet">{msg.snippet || ''}</span>
              <span class="meta">
                {#if msg.has_attachments}
                  <span class="meta-icon" title="has attachments">
                    <svg viewBox="0 0 24 24" aria-hidden="true">
                      <path d="m8 12.5 5.7-5.7a3.2 3.2 0 0 1 4.5 4.5l-7.4 7.4a5 5 0 0 1-7.1-7.1l7.6-7.6" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" />
                      <path d="m9.5 14 5.8-5.8" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" />
                    </svg>
                  </span>
                {/if}
                {#if msg.is_encrypted}
                  <span class="meta-icon" title="encrypted">
                    <svg viewBox="0 0 24 24" aria-hidden="true">
                      <rect x="5" y="10" width="14" height="10" rx="2" fill="none" stroke="currentColor" stroke-width="1.8" />
                      <path d="M8.5 10V7.5a3.5 3.5 0 0 1 7 0V10" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" />
                    </svg>
                  </span>
                {/if}
                {#if msg.is_starred}
                  <span class="star on" aria-label="starred">
                    <svg viewBox="0 0 24 24" aria-hidden="true">
                      <path d="m12 3 2.7 5.6 6.2.9-4.5 4.4 1.1 6.1-5.5-2.9L6.5 20l1.1-6.1-4.5-4.4 6.2-.9Z" fill="currentColor" />
                    </svg>
                  </span>
                {/if}
              </span>
            </div>
          </div>
        </div>
      </div>
    {/each}
    {#if loadingMore}
      <div class="state">Loading more…</div>
    {:else if !hasMore && messages.length > 0}
      <div class="state faint">That's everything.</div>
    {/if}
  {/if}
</div>

<style>
  .mobile-inbox-list {
    flex: 1 1 auto;
    overflow-y: auto;
    overscroll-behavior: contain;
    -webkit-overflow-scrolling: touch;
    background: var(--bg);
    /* The floating nav pill covers ~72px of the viewport. Clear enough
       room so the last row + footer state ("That's everything.") sit
       above the pill instead of being hidden behind it. */
    padding-bottom: calc(6.4rem + env(safe-area-inset-bottom));
  }

  .state {
    padding: 2rem 1.25rem;
    color: var(--muted);
    font-size: 0.95rem;
    text-align: center;
  }
  .state.error { color: #d6483c; }
  .state.faint { opacity: 0.6; }

  .row {
    position: relative;
    background: var(--surface);
    border-bottom: 1px solid var(--border);
    touch-action: pan-y;
    user-select: none;
    will-change: transform;
  }
  .row.unread {
    background: color-mix(in oklab, var(--accent) 4%, var(--surface));
  }

  .swipe-bg {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    padding: 0 1.25rem;
    font-size: 0.85rem;
    font-weight: 600;
    color: #fff;
    pointer-events: none;
  }
  .swipe-bg.left {
    background: #d6483c;
    justify-content: flex-end;
  }
  .swipe-bg.right {
    background: var(--accent);
    justify-content: flex-start;
  }

  .row-inner {
    position: relative;
    display: grid;
    grid-template-columns: 1.4rem auto 1fr;
    gap: 0.6rem;
    padding: 0.7rem 0.9rem;
    align-items: start;
    background: inherit;
  }

  /* Mailbox-coloured envelope. Unread = filled in account colour;
     read = outlined in muted grey. PGP flag overlays bottom-right. */
  .m-envelope {
    position: relative;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    margin-top: 0.3rem;
    line-height: 0;
    color: var(--muted);
    flex-shrink: 0;
  }
  .m-envelope.unread {
    color: var(--pill-color, var(--accent));
  }
  .pgp-flag {
    position: absolute;
    right: -0.5rem;
    bottom: -0.35rem;
    background: #fbbf24;
    color: #1a1102;
    font-size: 0.56rem;
    font-weight: 800;
    letter-spacing: 0.04em;
    line-height: 1;
    padding: 0.12rem 0.22rem;
    border-radius: 0.25rem;
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.25);
  }
  .m-envelope:not(.unread) .pgp-flag {
    background: color-mix(in oklab, var(--muted) 72%, transparent);
    color: var(--bg);
    box-shadow: none;
  }

  .body {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .line1 {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 0.5rem;
  }
  .sender {
    font-size: 0.96rem;
    color: var(--fg);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1 1 auto;
  }
  .time {
    font-size: 0.78rem;
    color: var(--muted);
    flex: 0 0 auto;
  }

  .line2 .subject {
    font-size: 0.9rem;
    color: var(--fg);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    display: block;
  }

  .line3 {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }
  .snippet {
    flex: 1 1 auto;
    min-width: 0;
    color: var(--muted);
    font-size: 0.84rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .meta {
    flex: 0 0 auto;
    display: flex;
    align-items: center;
    gap: 0.3rem;
  }
  .meta-icon,
  .star {
    width: 17px;
    height: 17px;
    display: inline-grid;
    place-items: center;
    color: var(--muted);
    opacity: 0.72;
  }
  .meta-icon svg,
  .star svg {
    width: 16px;
    height: 16px;
    display: block;
  }

  .star {
    background: transparent;
    border: 0;
  }
  .star.on {
    color: #e2b429;
    opacity: 1;
  }

  .bold {
    font-weight: 700;
  }
</style>
