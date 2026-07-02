<script lang="ts">
  import { prefs, type RowStyle, type SortOption } from '$lib/prefs';
  import { UNIFIED_DISPLAY, type UnifiedSystem } from '$lib/unified';

  type QuickFilter = 'all' | 'unread' | 'starred' | 'attachments';
  type Density = 'compact' | 'normal' | 'comfortable';
  type SplitOrient = 'vertical' | 'horizontal';

  let {
    allVisibleChecked,
    someVisibleChecked,
    quickFilter = $bindable(),
    activeAccount,
    activeFolder,
    activeUnified,
    hasUnreadInView,
    canEmptyFolder,
    folderActionBusy,
    showEmptyDisposal = false,
    emptyDisposalLabel = '',
    density,
    rowStyle,
    splitOrient,
    sort = $bindable(),
    onSelectAll,
    onMarkFolderRead,
    onEmptyFolder,
    onNextDensity,
    onToggleRowStyle,
    onToggleSplit
  }: {
    allVisibleChecked: boolean;
    someVisibleChecked: boolean;
    quickFilter: QuickFilter;
    activeAccount: number | null;
    activeFolder: string | null;
    activeUnified: UnifiedSystem | null;
    hasUnreadInView: boolean;
    canEmptyFolder: boolean;
    folderActionBusy: boolean;
    showEmptyDisposal?: boolean;
    emptyDisposalLabel?: string;
    density: Density;
    rowStyle: RowStyle;
    splitOrient: SplitOrient;
    sort: SortOption;
    onSelectAll: () => void;
    onMarkFolderRead: () => void;
    onEmptyFolder: () => void;
    onNextDensity: () => void;
    onToggleRowStyle: () => void;
    onToggleSplit: () => void;
  } = $props();

  let folderMenuOpen = $state(false);

  // What the folder-action buttons should call the active scope —
  // "Trash" / "Spam" for unified system rows, otherwise the real
  // per-account folder name.
  let folderActionLabel = $derived(
    activeUnified ? UNIFIED_DISPLAY[activeUnified] : activeFolder
  );

  // Only show the folder-actions trigger when at least one action
  // inside the dropdown will actually render. "Mark as read" is
  // per-account only (so requires activeAccount + activeFolder), and
  // "Empty" requires canEmptyFolder. Otherwise the user would see a
  // dropdown that opens to nothing.
  let hasMarkReadAction = $derived(
    hasUnreadInView && activeAccount != null && !!activeFolder
  );
  let hasFolderScope = $derived(hasMarkReadAction || canEmptyFolder);
</script>

<svelte:document onclick={() => { if (folderMenuOpen) folderMenuOpen = false; }} />

<div class="toolbar">
  <label
    class="inline-select-all"
    title={allVisibleChecked ? 'Deselect all visible' : 'Select all visible'}
  >
    <input
      type="checkbox"
      checked={allVisibleChecked}
      indeterminate={someVisibleChecked}
      onchange={onSelectAll}
      aria-label="Select all visible"
    />
    <span class="inline-label">Select</span>
  </label>
  <div class="filter-chips" role="tablist" aria-label="Quick filter">
    <button role="tab" class:active={quickFilter === 'all'} onclick={() => (quickFilter = 'all')}>All</button>
    <button role="tab" class:active={quickFilter === 'unread'} onclick={() => (quickFilter = 'unread')}>Unread</button>
    <button role="tab" class:active={quickFilter === 'starred'} onclick={() => (quickFilter = 'starred')}>Starred</button>
    <button role="tab" class:active={quickFilter === 'attachments'} onclick={() => (quickFilter = 'attachments')}>Files</button>
  </div>
  {#if hasFolderScope}
    <div class="folder-menu" role="presentation">
      <button
        type="button"
        class="folder-menu-trigger"
        aria-haspopup="menu"
        aria-expanded={folderMenuOpen}
        title={`Actions for ${folderActionLabel}`}
        onclick={(e) => { e.stopPropagation(); folderMenuOpen = !folderMenuOpen; }}
      >
        <svg viewBox="0 0 20 20" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.55" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <path d="M2.5 5.5A1.5 1.5 0 0 1 4 4h3.2a1.5 1.5 0 0 1 1.2.6l1 1.4h6.1a1.5 1.5 0 0 1 1.5 1.5V15a1.5 1.5 0 0 1-1.5 1.5H4A1.5 1.5 0 0 1 2.5 15V5.5Z"/>
          <circle cx="13" cy="11.5" r="1.4"/>
          <path d="M13 8.6v1M13 13.4v1M10.6 11.5h1M14.4 11.5h1"/>
        </svg>
        <span class="chev" class:open={folderMenuOpen} aria-hidden="true">
          <svg viewBox="0 0 12 12" width="9" height="9" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
            <path d="m3 4.5 3 3 3-3"/>
          </svg>
        </span>
      </button>
      {#if folderMenuOpen}
        <div
          class="folder-menu-pop"
          role="menu"
          onclick={(e) => e.stopPropagation()}
        >
          {#if hasUnreadInView && activeAccount != null && activeFolder}
            <button
              type="button"
              role="menuitem"
              disabled={folderActionBusy}
              onclick={() => { folderMenuOpen = false; onMarkFolderRead(); }}
            >Mark {folderActionLabel} as read</button>
          {/if}
          {#if canEmptyFolder}
            <button
              type="button"
              role="menuitem"
              class="danger"
              disabled={folderActionBusy}
              onclick={() => { folderMenuOpen = false; onEmptyFolder(); }}
            >Empty {folderActionLabel} (permanent)</button>
          {/if}
        </div>
      {/if}
    </div>
  {/if}
  {#if showEmptyDisposal}
    <button
      type="button"
      class="empty-pill"
      disabled={folderActionBusy}
      onclick={onEmptyFolder}
      title={`Permanently delete every message in ${emptyDisposalLabel}`}
    >
      <svg viewBox="0 0 16 16" width="12" height="12" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
        <path d="M3 4h10M6.5 4V2.8a.8.8 0 0 1 .8-.8h1.4a.8.8 0 0 1 .8.8V4M4.5 4l.6 8.4a1.2 1.2 0 0 0 1.2 1.1h3.4a1.2 1.2 0 0 0 1.2-1.1L11.5 4M6.7 7v4M9.3 7v4"/>
      </svg>
      <span>{folderActionBusy ? 'Emptying…' : `Empty ${emptyDisposalLabel}`}</span>
    </button>
  {/if}
  <div class="display-tools" aria-label="Display controls">
    <button class="icon-btn" title="Toggle density" onclick={onNextDensity}>
      {#if density === 'compact'}▤{:else if density === 'normal'}▦{:else}▩{/if}
    </button>
    <button class="icon-btn" title={rowStyle === 'detailed' ? 'Detailed rows — click for compact + hover preview' : 'Compact rows — click for detailed'} onclick={onToggleRowStyle}>
      {#if rowStyle === 'detailed'}≡{:else}⋯{/if}
    </button>
    <button class="icon-btn" title="Toggle split orientation" onclick={onToggleSplit}>
      {#if splitOrient === 'vertical'}▥{:else}▤{/if}
    </button>
    <select
      class="sort-select"
      bind:value={sort}
      onchange={() => prefs.update((p) => ({ ...p, sort }))}
      title="Sort messages"
    >
      <option value="date_desc">Newest first</option>
      <option value="date_asc">Oldest first</option>
      <option value="sender_asc">Sender A→Z</option>
      <option value="sender_desc">Sender Z→A</option>
      <option value="subject_asc">Subject A→Z</option>
      <option value="subject_desc">Subject Z→A</option>
    </select>
  </div>
</div>