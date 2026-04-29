<script lang="ts">
  import { goto } from '$app/navigation';
  import { api } from '$lib/api';
  import MessageBody from '$lib/components/MessageBody.svelte';

  interface Props {
    messageId: number;
  }

  let { messageId }: Props = $props();

  let busy = $state(false);
  let err = $state<string | null>(null);

  function back() {
    // If we got here from /inbox we could `history.back()`; but a
    // deep-link opened from Gmail-style notifications wouldn't have a
    // history entry. `goto` to /inbox is always safe.
    goto('/inbox');
  }

  async function archive() {
    if (busy) return;
    busy = true;
    try {
      await api.archiveMessage(messageId);
      back();
    } catch (e: unknown) {
      err = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
    }
  }

  async function trash() {
    if (busy) return;
    busy = true;
    try {
      await api.markTrash(messageId);
      back();
    } catch (e: unknown) {
      err = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
    }
  }

  async function markUnread() {
    if (busy) return;
    busy = true;
    try {
      await api.setMessageRead(messageId, false);
      back();
    } catch (e: unknown) {
      err = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
    }
  }

  function reply() {
    goto(`/compose?reply=${messageId}`);
  }
  function forward() {
    goto(`/compose?forward=${messageId}`);
  }
</script>

<div class="mobile-message">
  <header class="top-bar">
    <button class="icon-btn" onclick={back} aria-label="back">
      <svg viewBox="0 0 24 24" aria-hidden="true">
        <path d="M15 5 8 12l7 7" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round" />
      </svg>
    </button>
    <div class="spacer"></div>
    <button class="icon-btn" onclick={markUnread} aria-label="mark unread" disabled={busy}>
      <svg viewBox="0 0 24 24" aria-hidden="true">
        <path d="M4 6.5h16v11H4z" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linejoin="round" />
        <path d="m4.5 7 7.5 5.5L19.5 7" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" />
        <circle cx="19" cy="6" r="3" fill="var(--accent)" />
      </svg>
    </button>
  </header>

  {#if err}
    <div class="err">Something went wrong: {err}</div>
  {/if}

  <div class="body">
    <MessageBody messageId={messageId} variant="full" />
  </div>

  <footer class="action-bar">
    <button class="act" onclick={reply} disabled={busy}>
      <span class="glyph">
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <path d="M10 7 5 12l5 5" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" />
          <path d="M5 12h8a6 6 0 0 1 6 6v1" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" />
        </svg>
      </span>
      <span class="lbl">Reply</span>
    </button>
    <button class="act" onclick={forward} disabled={busy}>
      <span class="glyph">
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <path d="m14 7 5 5-5 5" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" />
          <path d="M5 19v-1a6 6 0 0 1 6-6h8" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" />
        </svg>
      </span>
      <span class="lbl">Forward</span>
    </button>
    <button class="act" onclick={archive} disabled={busy}>
      <span class="glyph">
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <path d="M4 5h16v4H4z" fill="currentColor" opacity="0.18" />
          <path d="M4 5h16v4H4z" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linejoin="round" />
          <path d="M6 9v10h12V9" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linejoin="round" />
          <path d="M9 13h6" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" />
        </svg>
      </span>
      <span class="lbl">Archive</span>
    </button>
    <button class="act danger" onclick={trash} disabled={busy}>
      <span class="glyph">
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <path d="M5 7h14" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" />
          <path d="M9 7V5h6v2" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linejoin="round" />
          <path d="M7 7l1 13h8l1-13" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linejoin="round" />
          <path d="M10 11v5M14 11v5" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" />
        </svg>
      </span>
      <span class="lbl">Delete</span>
    </button>
  </footer>
</div>

<style>
  .mobile-message {
    display: flex;
    flex-direction: column;
    height: 100dvh;
    background: var(--bg);
    overflow: hidden;
  }

  .top-bar {
    flex: 0 0 auto;
    display: flex;
    align-items: center;
    gap: 0.35rem;
    padding: 0.35rem 0.5rem;
    background: var(--surface);
    border-bottom: 1px solid var(--border);
    padding-top: max(0.35rem, env(safe-area-inset-top));
  }
  .spacer { flex: 1 1 auto; }

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
    display: inline-grid;
    place-items: center;
  }
  .icon-btn svg {
    width: 22px;
    height: 22px;
  }
  .icon-btn:hover { background: var(--row-hover); }
  .icon-btn:disabled { opacity: 0.45; cursor: default; }

  .err {
    padding: 0.6rem 1rem;
    background: color-mix(in oklab, #d6483c 16%, var(--bg));
    color: #d6483c;
    font-size: 0.85rem;
  }

  .body {
    flex: 1 1 auto;
    overflow-y: auto;
    -webkit-overflow-scrolling: touch;
    padding-bottom: calc(5.4rem + env(safe-area-inset-bottom));
  }

  .action-bar {
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
    z-index: 35;
  }
  .act {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 3px;
    min-height: 52px;
    padding: 0.34rem 0.25rem;
    background: transparent;
    border: 0;
    color: var(--muted);
    cursor: pointer;
    touch-action: manipulation;
    font-size: 0.72rem;
    border-radius: 16px;
    transition: background 160ms ease, color 160ms ease, transform 140ms ease;
  }
  .act:hover,
  .act:active {
    color: var(--fg);
    background: var(--row-hover);
  }
  .act:active { transform: scale(0.97); }
  .act:disabled { opacity: 0.45; cursor: default; }
  .act.danger { color: #d6483c; }
  .glyph {
    width: 24px;
    height: 24px;
    display: inline-grid;
    place-items: center;
  }
  .glyph svg {
    width: 23px;
    height: 23px;
    display: block;
  }
  .lbl {
    font-size: 0.7rem;
    opacity: 0.85;
  }
</style>
