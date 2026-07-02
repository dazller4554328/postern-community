<script lang="ts">
  interface Props {
    checkedCount: number;
    allVisibleChecked: boolean;
    someVisibleChecked: boolean;
    bulkBusy: boolean;
    canMove: boolean;
    isSpamFolder: boolean;
    onSelectAll: () => void;
    onRun: (action: 'read' | 'unread' | 'archive' | 'spam' | 'notspam' | 'trash') => void;
    onMoveOpen: () => void;
    onClear: () => void;
  }
  let {
    checkedCount,
    allVisibleChecked,
    someVisibleChecked,
    bulkBusy,
    canMove,
    isSpamFolder,
    onSelectAll,
    onRun,
    onMoveOpen,
    onClear,
  }: Props = $props();
</script>

<div class="bulk-bar">
  <label class="bulk-all" title="Toggle all visible">
    <input
      type="checkbox"
      checked={allVisibleChecked}
      indeterminate={someVisibleChecked}
      onchange={onSelectAll}
    />
    <span class="bulk-count">{checkedCount} selected</span>
  </label>
  <div class="bulk-actions">
    <button type="button" onclick={() => onRun('read')} disabled={bulkBusy}>
      Mark read
    </button>
    <button type="button" onclick={() => onRun('unread')} disabled={bulkBusy}>
      Mark unread
    </button>
    <button type="button" onclick={() => onRun('archive')} disabled={bulkBusy}>
      Archive
    </button>
    <button
      type="button"
      onclick={onMoveOpen}
      disabled={bulkBusy || !canMove}
      title={!canMove
        ? 'Selection spans multiple accounts — narrow to one account first'
        : 'Move selected messages to a folder'}
    >
      Move to…
    </button>
    {#if isSpamFolder}
      <button type="button" onclick={() => onRun('notspam')} disabled={bulkBusy}>
        Not spam
      </button>
    {:else}
      <button type="button" onclick={() => onRun('spam')} disabled={bulkBusy}>
        Spam
      </button>
    {/if}
    <button type="button" class="danger" onclick={() => onRun('trash')} disabled={bulkBusy}>
      {bulkBusy ? 'Working…' : 'Trash'}
    </button>
    <button type="button" class="linklike" onclick={onClear} disabled={bulkBusy}>
      Clear
    </button>
  </div>
</div>
