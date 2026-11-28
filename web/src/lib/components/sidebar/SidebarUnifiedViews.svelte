<script lang="ts">
  import { goto } from '$app/navigation';
  import type { FoldersResponse } from '$lib/api';
  import {
    showsUnread as folderShowsUnread,
    showsTotal as folderShowsTotal
  } from '$lib/folderSemantics';
  import {
    UNIFIED_DISPLAY,
    UNIFIED_ICON_NAME,
    unifiedCounts,
    type UnifiedSystem
  } from '$lib/unified';
  import FolderIcon from '../FolderIcon.svelte';

  interface Props {
    folders: FoldersResponse | null;
    activeUnified: UnifiedSystem | null | undefined;
    activeQuery: string;
  }

  let { folders, activeUnified, activeQuery }: Props = $props();

  /** Render order locked: Inbox/Drafts/Sent/Spam/Trash. Same shape
   *  every other client uses; reorder here would surprise users. */
  const UNIFIED_ORDER: UnifiedSystem[] = ['inbox', 'drafts', 'sent', 'spam', 'trash'];

  function navigateUnified(system: UnifiedSystem) {
    const url = new URL('/inbox', window.location.origin);
    url.searchParams.set('u', system);
    goto(url.pathname + url.search, {
      noScroll: true,
      keepFocus: true,
      invalidateAll: true
    });
  }

  function isUnifiedActive(system: UnifiedSystem) {
    return activeUnified === system && !activeQuery;
  }
</script>

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
          <!-- Small offset-stack glyph indicating this row aggregates
               across every account, not just the currently-focused one. -->
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

<style>
  /* Unified cross-account system folders sit at the top so you can
     treat Inbox / Sent / Drafts / Spam / Trash as one surface across
     every account. Visually distinct from the per-account tree below.
     Shared base styles (.row, .label, .unread-count, .section-label,
     .unified-divider, .total-count) live in Sidebar.svelte under
     :global() so this child inherits them. */
  .unified {
    padding: 0.1rem 0 0.2rem;
    display: flex;
    flex-direction: column;
    gap: 0.18rem;
  }
  .unified-row {
    font-weight: 500;
    border: 1px solid color-mix(in oklab, currentColor 8%, transparent);
    background: color-mix(in oklab, var(--surface) 92%, transparent);
  }
  .unified-row:hover {
    background: var(--row-hover);
    border-color: color-mix(in oklab, currentColor 14%, transparent);
  }
  .unified-row.active {
    background: var(--row-selected);
    border-color: color-mix(in oklab, var(--accent) 28%, transparent);
  }
  /* Two-layer icon: the folder icon + a small offset square behind it
     suggesting "stacked / aggregated across accounts." Makes a unified
     row visually distinct from the per-account folder rows below
     without needing an extra character column. */
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
</style>
