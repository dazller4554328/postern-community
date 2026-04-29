<script lang="ts">
  import { goto } from '$app/navigation';
  import { api, type MessageListItem } from '$lib/api';
  import { formatRelative, formatSender } from '$lib/format';
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
  }

  let { accountId, folder, reloadKey = 0, query = '' }: Props = $props();

  let messages = $state<MessageListItem[]>([]);
  let loading = $state(true);
  let loadingMore = $state(false);
  let err = $state<string | null>(null);
  let hasMore = $state(true);

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
    goto(`/message/${msg.id}`);
  }

  let listEl = $state<HTMLElement | null>(null);
  function onScroll() {
    if (!listEl) return;
    const near = listEl.scrollTop + listEl.clientHeight > listEl.scrollHeight - 600;
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
          <div class="avatar">
            <SenderAvatar email={msg.from_addr} size={44} fetchRemote={true} />
            {#if !msg.is_read}
              <span class="unread-dot" aria-label="unread"></span>
            {/if}
          </div>

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
    grid-template-columns: auto 1fr;
    gap: 0.75rem;
    padding: 0.75rem 0.9rem 0.75rem 0.9rem;
    align-items: start;
    background: inherit;
  }

  .avatar {
    position: relative;
    padding-top: 2px;
  }
  .unread-dot {
    position: absolute;
    top: 0;
    right: -2px;
    width: 9px;
    height: 9px;
    border-radius: 999px;
    background: var(--accent);
    border: 2px solid var(--surface);
  }

  .body {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .line1 {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 0.5rem;
  }
  .sender {
    font-size: 0.98rem;
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
    font-size: 0.92rem;
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
    font-size: 0.85rem;
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
