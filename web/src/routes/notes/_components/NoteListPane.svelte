<script lang="ts">
  import type { Note } from '$lib/api';
  import { formatDate } from '$lib/format';
  import { summarize, displayTitle } from '../_lib/markdown';

  interface Props {
    notes: Note[];
    selected: Note | null;
    loading: boolean;
    err: string | null;
    query: string;
    onQueryChange: (q: string) => void;
    onNew: () => void;
    onSelect: (n: Note) => void;
  }
  let { notes, selected, loading, err, query, onQueryChange, onNew, onSelect }: Props = $props();
</script>

<aside class="list-pane">
  <header class="list-head">
    <span class="list-head-title">All notes</span>
    <button class="new-btn" onclick={onNew} aria-label="New note" title="New note">
      <svg width="18" height="18" viewBox="0 0 20 20" aria-hidden="true">
        <path d="M10 4v12M4 10h12" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" />
      </svg>
      <span>New</span>
    </button>
  </header>

  <div class="search">
    <input
      type="search"
      placeholder="Search notes…"
      value={query}
      oninput={(e) => onQueryChange((e.currentTarget as HTMLInputElement).value)}
      aria-label="Search notes"
    />
  </div>

  {#if loading}
    <div class="state">Loading…</div>
  {:else if err && notes.length === 0}
    <div class="state error">{err}</div>
  {:else if notes.length === 0}
    <div class="state empty">
      {query.trim() ? 'No matches.' : 'No notes yet — create one to get started.'}
    </div>
  {:else}
    <ul class="list">
      {#each notes as n (n.id)}
        <li>
          <button
            class="row"
            class:active={selected?.id === n.id}
            onclick={() => onSelect(n)}
          >
            <div class="row-head">
              {#if n.pinned}
                <span class="pin-dot" title="Pinned" aria-hidden="true">●</span>
              {/if}
              <span class="row-title">{displayTitle(n)}</span>
            </div>
            <div class="row-meta">
              <span class="row-summary">{summarize(n)}</span>
              <span class="row-date">{formatDate(n.updated_at * 1000)}</span>
            </div>
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</aside>

<style>
  .list-pane {
    display: flex;
    flex-direction: column;
    min-height: 0;
    border-right: 1px solid var(--border);
    background: var(--surface);
  }
  .list-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.7rem 0.9rem 0.5rem;
    gap: 0.5rem;
    border-bottom: 1px solid var(--border);
  }
  .list-head-title {
    font-size: 0.74rem;
    text-transform: uppercase;
    letter-spacing: 0.12em;
    color: var(--muted);
    font-weight: 700;
  }
  .new-btn {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    padding: 0.35rem 0.6rem;
    border-radius: 0.55rem;
    border: 1px solid var(--border);
    background: var(--bg);
    color: var(--fg);
    font-size: 0.85rem;
    cursor: pointer;
  }
  .new-btn:hover {
    background: var(--row-hover);
  }
  .search {
    padding: 0 0.7rem 0.5rem;
  }
  .search input {
    width: 100%;
    box-sizing: border-box;
    padding: 0.45rem 0.6rem;
    border-radius: 0.55rem;
    border: 1px solid var(--border);
    background: var(--bg);
    color: var(--fg);
    font-size: 0.9rem;
  }
  .search input:focus {
    outline: none;
    border-color: color-mix(in oklab, var(--accent) 40%, var(--border));
    box-shadow: 0 0 0 3px color-mix(in oklab, var(--accent) 18%, transparent);
  }
  .list {
    list-style: none;
    margin: 0;
    padding: 0 0.4rem 0.6rem;
    overflow-y: auto;
    flex: 1 1 auto;
  }
  .row {
    display: block;
    width: 100%;
    text-align: left;
    background: transparent;
    border: 0;
    color: inherit;
    padding: 0.55rem 0.65rem;
    border-radius: 0.6rem;
    cursor: pointer;
  }
  .row:hover {
    background: var(--row-hover);
  }
  .row.active {
    background: color-mix(in oklab, var(--accent) 14%, var(--surface));
    box-shadow: inset 0 0 0 1px color-mix(in oklab, var(--accent) 28%, transparent);
  }
  .row-head {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    min-width: 0;
  }
  .pin-dot {
    color: var(--accent);
    font-size: 0.7rem;
    line-height: 1;
  }
  .row-title {
    font-weight: 600;
    font-size: 0.94rem;
    color: var(--fg);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1 1 auto;
  }
  .row-meta {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    margin-top: 2px;
    color: var(--muted);
    font-size: 0.82rem;
  }
  .row-summary {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .row-date {
    flex: 0 0 auto;
    font-variant-numeric: tabular-nums;
    opacity: 0.85;
  }
  .state {
    padding: 1rem 1.1rem;
    color: var(--muted);
    font-size: 0.9rem;
  }
  .state.error {
    color: var(--danger, #d04a4a);
  }
  .state.empty {
    color: var(--muted);
  }
</style>
