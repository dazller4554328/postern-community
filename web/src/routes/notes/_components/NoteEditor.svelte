<script lang="ts">
  import type { Note } from '$lib/api';
  import { previewHtml } from '../_lib/markdown';

  interface Props {
    selected: Note | null;
    draftTitle: string;
    draftBody: string;
    draftPinned: boolean;
    preview: boolean;
    err: string | null;
    savedLabel: string;
    onScheduleSave: () => void;
    onTogglePin: () => void;
    onDelete: () => void;
    onBackToList: () => void;
    onOpenHistory: () => void;
  }
  let {
    selected,
    draftTitle = $bindable(),
    draftBody = $bindable(),
    draftPinned,
    preview = $bindable(),
    err,
    savedLabel,
    onScheduleSave,
    onTogglePin,
    onDelete,
    onBackToList,
    onOpenHistory,
  }: Props = $props();

  let bodyEl: HTMLTextAreaElement | null = $state(null);
</script>

<section class="editor-pane">
  {#if !selected}
    <div class="placeholder">
      <p>Select a note from the list, or create a new one.</p>
    </div>
  {:else}
    <header class="editor-head">
      <button class="back-btn" onclick={onBackToList} aria-label="Back to list">
        <svg width="18" height="18" viewBox="0 0 20 20" aria-hidden="true">
          <path d="M12.5 4 6 10l6.5 6" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" />
        </svg>
      </button>
      <input
        class="title"
        type="text"
        placeholder="Untitled"
        bind:value={draftTitle}
        oninput={onScheduleSave}
      />
      <div class="head-actions">
        <button
          class="icon"
          class:on={draftPinned}
          onclick={onTogglePin}
          aria-label={draftPinned ? 'Unpin note' : 'Pin note'}
          title={draftPinned ? 'Unpin' : 'Pin'}
        >
          <svg width="18" height="18" viewBox="0 0 20 20" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">
            <path d="M11.5 3.5 16.5 8.5l-3 1-3 3-1-3-3-1z" />
            <path d="m7 13-3 3" />
          </svg>
        </button>
        <button
          class="icon"
          onclick={onOpenHistory}
          aria-label="Revision history"
          title="Revision history — restore an earlier version"
        >
          <svg width="18" height="18" viewBox="0 0 20 20" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">
            <path d="M10 4.5a5.5 5.5 0 1 1-3.9 1.6" />
            <path d="M6 3.5v3h3" />
            <path d="M10 7v3.2l2 1.3" />
          </svg>
        </button>
        <button
          class="icon"
          class:on={preview}
          onclick={() => (preview = !preview)}
          aria-label={preview ? 'Edit' : 'Preview'}
          title={preview ? 'Edit' : 'Preview'}
        >
          {#if preview}
            <svg width="18" height="18" viewBox="0 0 20 20" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">
              <path d="M3.5 14.5 14 4l2.5 2.5L6 17H3.5z" />
              <path d="m12.5 5.5 2 2" />
            </svg>
          {:else}
            <svg width="18" height="18" viewBox="0 0 20 20" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">
              <path d="M2 10s3-5 8-5 8 5 8 5-3 5-8 5-8-5-8-5z" />
              <circle cx="10" cy="10" r="2.5" />
            </svg>
          {/if}
        </button>
        <button
          class="icon danger"
          onclick={onDelete}
          aria-label="Delete note"
          title="Delete"
        >
          <svg width="18" height="18" viewBox="0 0 20 20" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">
            <path d="M4 6h12" />
            <path d="m6 6 .8 10a1 1 0 0 0 1 .9h4.4a1 1 0 0 0 1-.9L14 6" />
            <path d="M8.5 6V4.5h3V6" />
          </svg>
        </button>
      </div>
    </header>

    <div class="status-line">
      <span class="meta">{savedLabel}</span>
      {#if err}<span class="err">{err}</span>{/if}
    </div>

    {#if preview}
      <div class="preview">
        {#if draftBody.trim()}
          {@html previewHtml(draftBody)}
        {:else}
          <p class="placeholder">Nothing to preview yet.</p>
        {/if}
      </div>
    {:else}
      <textarea
        bind:this={bodyEl}
        class="body"
        placeholder="Markdown supported. **bold**, *italic*, `code`, # headings, - lists, [links](https://…)"
        bind:value={draftBody}
        oninput={onScheduleSave}
      ></textarea>
    {/if}
  {/if}
</section>

<style>
  .editor-pane {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    background: var(--bg);
  }
  .placeholder {
    flex: 1 1 auto;
    display: grid;
    place-items: center;
    color: var(--muted);
    font-size: 0.95rem;
    padding: 2rem;
  }
  .editor-head {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.7rem 1rem 0.55rem;
    border-bottom: 1px solid var(--border);
  }
  .back-btn {
    display: none;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    border-radius: 0.5rem;
    border: 0;
    background: transparent;
    color: var(--fg);
    cursor: pointer;
  }
  .back-btn:hover {
    background: var(--row-hover);
  }
  .title {
    flex: 1 1 auto;
    border: 0;
    background: transparent;
    color: var(--fg);
    font-size: 1.15rem;
    font-weight: 700;
    padding: 0.25rem 0.1rem;
    min-width: 0;
  }
  .title:focus {
    outline: none;
  }
  .title::placeholder {
    color: var(--muted);
    font-weight: 600;
  }
  .head-actions {
    display: inline-flex;
    align-items: center;
    gap: 0.15rem;
  }
  .icon {
    width: 32px;
    height: 32px;
    display: inline-grid;
    place-items: center;
    border-radius: 0.5rem;
    border: 0;
    background: transparent;
    color: var(--fg);
    cursor: pointer;
  }
  .icon:hover {
    background: var(--row-hover);
  }
  .icon.on {
    background: color-mix(in oklab, var(--accent) 18%, transparent);
    color: var(--accent);
  }
  .icon.danger:hover {
    background: color-mix(in oklab, var(--danger, #d04a4a) 16%, transparent);
    color: var(--danger, #d04a4a);
  }
  .status-line {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    padding: 0.3rem 1rem;
    color: var(--muted);
    font-size: 0.78rem;
    min-height: 1.2rem;
  }
  .status-line .err {
    color: var(--danger, #d04a4a);
  }
  .body {
    flex: 1 1 auto;
    width: 100%;
    box-sizing: border-box;
    border: 0;
    background: transparent;
    color: var(--fg);
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 0.95rem;
    line-height: 1.55;
    padding: 0.8rem 1.05rem 1.5rem;
    resize: none;
  }
  .body:focus {
    outline: none;
  }
  .preview {
    flex: 1 1 auto;
    overflow-y: auto;
    padding: 0.8rem 1.2rem 1.5rem;
    font-size: 0.96rem;
    line-height: 1.55;
  }
  .preview :global(h1),
  .preview :global(h2),
  .preview :global(h3) {
    margin: 1.1em 0 0.5em;
    line-height: 1.3;
  }
  .preview :global(p) {
    margin: 0.5em 0;
  }
  .preview :global(ul),
  .preview :global(ol) {
    padding-left: 1.4em;
  }
  .preview :global(code) {
    background: color-mix(in oklab, var(--fg) 8%, transparent);
    padding: 0.1em 0.35em;
    border-radius: 0.3em;
    font-size: 0.92em;
  }
  .preview :global(pre) {
    background: color-mix(in oklab, var(--fg) 6%, transparent);
    padding: 0.7em 0.9em;
    border-radius: 0.5em;
    overflow-x: auto;
  }
  .preview :global(pre code) {
    background: transparent;
    padding: 0;
  }
  .preview :global(blockquote) {
    border-left: 3px solid color-mix(in oklab, var(--accent) 40%, transparent);
    padding: 0.2em 0.9em;
    color: var(--muted);
    margin: 0.7em 0;
  }
  .preview :global(a) {
    color: var(--accent);
  }
  @media (max-width: 760px) {
    .back-btn {
      display: inline-flex;
    }
  }
</style>
