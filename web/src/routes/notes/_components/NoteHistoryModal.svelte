<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { api, type Note, type NoteRevision } from '$lib/api';

  interface Props {
    noteId: number;
    /// Fired after a successful restore with the post-restore note.
    /// Caller refreshes its draft state from this.
    onRestored: (note: Note) => void;
    onClose: () => void;
  }
  let { noteId, onRestored, onClose }: Props = $props();

  let revisions = $state<NoteRevision[]>([]);
  let loading = $state(true);
  let err = $state<string | null>(null);
  let selectedId = $state<number | null>(null);
  let restoreBusy = $state(false);

  onMount(async () => {
    try {
      revisions = await api.notesListRevisions(noteId);
      selectedId = revisions[0]?.id ?? null;
    } catch (e) {
      err = e instanceof Error ? e.message : String(e);
    } finally {
      loading = false;
      await tick();
      const dialog = document.querySelector<HTMLElement>('.note-history-dialog');
      dialog?.focus();
    }
  });

  let selected = $derived(revisions.find((r) => r.id === selectedId) ?? null);

  function formatAbsolute(ts: number): string {
    return new Date(ts * 1000).toLocaleString();
  }

  function formatRelative(ts: number): string {
    const diff = Math.max(0, Math.floor(Date.now() / 1000 - ts));
    if (diff < 60) return `${diff}s ago`;
    if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
    if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
    return `${Math.floor(diff / 86400)}d ago`;
  }

  function bodyPreview(body: string): string {
    const oneLine = body.replace(/\s+/g, ' ').trim();
    return oneLine.length > 80 ? `${oneLine.slice(0, 77)}…` : oneLine || '(empty)';
  }

  async function restore() {
    if (!selected || restoreBusy) return;
    const ok = window.confirm(
      'Replace the current note with this revision? The current text will become a new revision in the list — you can restore it back from here too.'
    );
    if (!ok) return;
    restoreBusy = true;
    try {
      const updated = await api.notesRestoreRevision(noteId, selected.id);
      onRestored(updated);
      onClose();
    } catch (e) {
      err = e instanceof Error ? e.message : String(e);
    } finally {
      restoreBusy = false;
    }
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      onClose();
    }
  }
</script>

<svelte:window onkeydown={onKey} />

<div
  class="scrim"
  role="presentation"
  onclick={onClose}
></div>

<div
  class="note-history-dialog"
  role="dialog"
  aria-modal="true"
  aria-labelledby="note-history-title"
  tabindex="-1"
>
  <header>
    <h2 id="note-history-title">Revision history</h2>
    <button class="close" onclick={onClose} aria-label="Close">×</button>
  </header>

  {#if loading}
    <p class="placeholder">Loading…</p>
  {:else if err}
    <p class="placeholder err">{err}</p>
  {:else if revisions.length === 0}
    <p class="placeholder">
      No history yet. Revisions are captured automatically whenever you
      make a meaningful change (at most one per 30 seconds, up to 50 per note).
    </p>
  {:else}
    <div class="layout">
      <ul class="rev-list" role="listbox" aria-label="Revisions">
        {#each revisions as r (r.id)}
          <li>
            <button
              class="rev"
              class:active={r.id === selectedId}
              role="option"
              aria-selected={r.id === selectedId}
              onclick={() => (selectedId = r.id)}
            >
              <span class="rev-time">{formatRelative(r.created_at)}</span>
              <span class="rev-abs">{formatAbsolute(r.created_at)}</span>
              <span class="rev-preview">{bodyPreview(r.body)}</span>
            </button>
          </li>
        {/each}
      </ul>
      <div class="rev-detail">
        {#if selected}
          <div class="rev-detail-head">
            <div class="rev-detail-meta">
              <strong>{selected.title || '(untitled)'}</strong>
              <span class="muted">{formatAbsolute(selected.created_at)}</span>
            </div>
            <button
              class="btn primary"
              onclick={restore}
              disabled={restoreBusy}
            >
              {restoreBusy ? 'Restoring…' : 'Restore this version'}
            </button>
          </div>
          <pre class="rev-body">{selected.body || '(empty body)'}</pre>
        {:else}
          <p class="placeholder">Pick a revision on the left to see it here.</p>
        {/if}
      </div>
    </div>
  {/if}
</div>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    background: color-mix(in oklab, black 38%, transparent);
    z-index: 90;
  }
  .note-history-dialog {
    position: fixed;
    inset: 5% 5% 5% 5%;
    max-width: 1100px;
    max-height: 720px;
    margin: auto;
    background: var(--bg);
    color: var(--fg);
    border: 1px solid var(--border);
    border-radius: 0.95rem;
    box-shadow: 0 22px 60px rgba(0, 0, 0, 0.35);
    z-index: 91;
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  .note-history-dialog:focus-visible {
    outline: none;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.85rem 1.1rem 0.7rem;
    border-bottom: 1px solid var(--border);
  }
  header h2 {
    margin: 0;
    font-size: 1.02rem;
    font-weight: 700;
    letter-spacing: -0.01em;
  }
  .close {
    border: 0;
    background: transparent;
    color: var(--fg);
    font-size: 1.4rem;
    line-height: 1;
    padding: 0.2rem 0.55rem;
    border-radius: 0.4rem;
    cursor: pointer;
  }
  .close:hover {
    background: color-mix(in oklab, currentColor 9%, transparent);
  }
  .placeholder {
    padding: 1.6rem 1.4rem;
    color: var(--muted);
    text-align: center;
  }
  .placeholder.err {
    color: var(--danger, #d04a4a);
  }
  .layout {
    display: grid;
    grid-template-columns: minmax(280px, 1fr) 2fr;
    flex: 1 1 auto;
    min-height: 0;
  }
  .rev-list {
    list-style: none;
    margin: 0;
    padding: 0.35rem 0.4rem;
    border-right: 1px solid var(--border);
    overflow-y: auto;
  }
  .rev-list li {
    padding: 0;
  }
  .rev {
    display: grid;
    grid-template-columns: 1fr;
    gap: 0.1rem;
    width: 100%;
    text-align: left;
    border: 0;
    background: transparent;
    color: inherit;
    padding: 0.55rem 0.7rem;
    border-radius: 0.5rem;
    cursor: pointer;
  }
  .rev:hover {
    background: color-mix(in oklab, currentColor 5%, transparent);
  }
  .rev.active {
    background: color-mix(in oklab, var(--accent) 16%, transparent);
  }
  .rev-time {
    font-weight: 600;
    font-size: 0.86rem;
  }
  .rev-abs {
    font-size: 0.72rem;
    color: var(--muted);
  }
  .rev-preview {
    font-size: 0.78rem;
    color: var(--muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .rev-detail {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
  }
  .rev-detail-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.8rem;
    padding: 0.7rem 1rem 0.55rem;
    border-bottom: 1px solid var(--border);
  }
  .rev-detail-meta {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
    min-width: 0;
    overflow: hidden;
  }
  .rev-detail-meta strong {
    font-size: 0.95rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .rev-detail-meta .muted {
    font-size: 0.74rem;
    color: var(--muted);
  }
  .btn.primary {
    background: var(--accent);
    color: var(--accent-fg, white);
    border: 0;
    padding: 0.5rem 0.95rem;
    border-radius: 0.5rem;
    font-weight: 600;
    cursor: pointer;
    white-space: nowrap;
  }
  .btn.primary:disabled {
    opacity: 0.55;
    cursor: progress;
  }
  .rev-body {
    flex: 1 1 auto;
    margin: 0;
    padding: 0.85rem 1.1rem 1rem;
    white-space: pre-wrap;
    word-break: break-word;
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 0.88rem;
    line-height: 1.55;
    overflow: auto;
  }
  @media (max-width: 760px) {
    .note-history-dialog {
      inset: 0;
      max-height: none;
      border-radius: 0;
      border: 0;
    }
    .layout {
      grid-template-columns: 1fr;
      grid-template-rows: minmax(160px, 35%) 1fr;
    }
    .rev-list {
      border-right: 0;
      border-bottom: 1px solid var(--border);
    }
  }
</style>
