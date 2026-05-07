<script lang="ts">
  // Secure notes — markdown notepad backed by /api/notes*. Same vault
  // gate as the rest of the app: anything we read/write lives in the
  // SQLCipher DB and is unreachable while locked.
  //
  // Layout: master/detail. Sidebar list of notes (pinned + recent),
  // editor on the right with debounced auto-save. On narrow screens
  // the list collapses behind a "back" button when a note is open.

  import { onDestroy, onMount, tick } from 'svelte';
  import { page } from '$app/stores';
  import { marked } from 'marked';
  import { api, type Note } from '$lib/api';
  import { formatDate } from '$lib/format';

  // Popup mode (?popup=1) — opened from the Compose page's "Notes"
  // button so the user can see notes side-by-side with their draft.
  // Strips the Inbox backlink + the hero block so the small popup
  // window is all list + editor.
  let isPopup = $derived($page.url.searchParams.get('popup') === '1');

  let notes = $state<Note[]>([]);
  let selected = $state<Note | null>(null);
  let loading = $state(true);
  let err = $state<string | null>(null);

  let q = $state('');
  let preview = $state(false);

  // Debounced save — we copy the selected note's fields into local
  // editor state so typing stays buttery, then push the diff back
  // to the server after the user pauses.
  let draftTitle = $state('');
  let draftBody = $state('');
  let draftPinned = $state(false);
  let savingBusy = $state(false);
  let savedAt = $state<number | null>(null);
  let saveTimer: ReturnType<typeof setTimeout> | null = null;
  const SAVE_DEBOUNCE_MS = 600;

  let bodyEl = $state<HTMLTextAreaElement | null>(null);

  // Mobile detail-mode toggle. When a note is open on a narrow
  // viewport the list slides away so the editor gets full width.
  let mobileShowList = $state(true);

  const filtered = $derived.by(() => {
    const needle = q.trim().toLowerCase();
    if (!needle) return notes;
    return notes.filter((n) => {
      const t = (n.title || '').toLowerCase();
      const b = (n.body || '').toLowerCase();
      return t.includes(needle) || b.includes(needle);
    });
  });

  onMount(() => {
    void load();
  });

  onDestroy(() => {
    if (saveTimer) clearTimeout(saveTimer);
  });

  async function load(keepSelectedId?: number) {
    loading = true;
    try {
      notes = await api.notesList();
      err = null;
      // Re-bind selection by id if the caller asked us to (so a save
      // that re-orders the list doesn't deselect the active note).
      if (keepSelectedId != null) {
        const match = notes.find((n) => n.id === keepSelectedId);
        if (match) bindSelected(match);
      } else if (selected) {
        const match = notes.find((n) => n.id === selected!.id);
        if (match) selected = match;
      }
    } catch (e) {
      err = e instanceof Error ? e.message : String(e);
      notes = [];
    } finally {
      loading = false;
    }
  }

  function bindSelected(n: Note) {
    selected = n;
    draftTitle = n.title;
    draftBody = n.body;
    draftPinned = n.pinned;
    savedAt = n.updated_at;
    preview = false;
    mobileShowList = false;
  }

  function selectNote(n: Note) {
    if (saveTimer) {
      clearTimeout(saveTimer);
      saveTimer = null;
      void persist();
    }
    bindSelected(n);
  }

  async function newNote() {
    if (saveTimer) {
      clearTimeout(saveTimer);
      saveTimer = null;
      await persist();
    }
    try {
      const created = await api.notesCreate({});
      notes = [created, ...notes];
      bindSelected(created);
      await tick();
      // Drop focus into the title field so the user can start typing.
      const titleInput = document.querySelector<HTMLInputElement>(
        '.editor input.title'
      );
      titleInput?.focus();
    } catch (e) {
      err = e instanceof Error ? e.message : String(e);
    }
  }

  function scheduleSave() {
    if (!selected) return;
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = setTimeout(() => {
      saveTimer = null;
      void persist();
    }, SAVE_DEBOUNCE_MS);
  }

  async function persist() {
    if (!selected) return;
    const id = selected.id;
    const patch = {
      title: draftTitle,
      body: draftBody,
      pinned: draftPinned
    };
    savingBusy = true;
    try {
      const updated = await api.notesUpdate(id, patch);
      // Splice the updated row in place to keep the visible list in
      // sync without a full refetch — refetch only when ordering may
      // have changed (pin toggle).
      const idx = notes.findIndex((n) => n.id === id);
      if (idx >= 0) notes[idx] = updated;
      savedAt = updated.updated_at;
      if (selected && selected.id === id) {
        selected = updated;
      }
      err = null;
    } catch (e) {
      err = e instanceof Error ? e.message : String(e);
    } finally {
      savingBusy = false;
    }
  }

  async function togglePin() {
    if (!selected) return;
    draftPinned = !draftPinned;
    if (saveTimer) {
      clearTimeout(saveTimer);
      saveTimer = null;
    }
    await persist();
    // Re-sort the list after a pin change.
    await load(selected.id);
  }

  async function deleteSelected() {
    if (!selected) return;
    const ok = confirm('Delete this note? This cannot be undone.');
    if (!ok) return;
    const id = selected.id;
    if (saveTimer) {
      clearTimeout(saveTimer);
      saveTimer = null;
    }
    try {
      await api.notesDelete(id);
      notes = notes.filter((n) => n.id !== id);
      selected = null;
      mobileShowList = true;
    } catch (e) {
      err = e instanceof Error ? e.message : String(e);
    }
  }

  function previewHtml(src: string): string {
    // marked is synchronous when called without async options. We pin
    // gfm + breaks for the most "expected" rendering on short notes.
    try {
      return marked.parse(src || '', {
        async: false,
        gfm: true,
        breaks: true
      }) as string;
    } catch {
      return '';
    }
  }

  function summarize(n: Note): string {
    const body = (n.body || '').trim();
    if (!body) return '';
    const firstLine = body.split('\n').find((l) => l.trim().length > 0) ?? '';
    // Strip the most obvious markdown noise so the list preview reads
    // like prose, not like raw markdown.
    return firstLine
      .replace(/^#{1,6}\s+/, '')
      .replace(/[*_`>~-]+/g, '')
      .slice(0, 140);
  }

  function displayTitle(n: Note): string {
    const t = (n.title || '').trim();
    if (t) return t;
    const summary = summarize(n);
    return summary || 'Untitled';
  }

  function backToList() {
    if (saveTimer) {
      clearTimeout(saveTimer);
      saveTimer = null;
      void persist();
    }
    mobileShowList = true;
  }

  function savedLabel(): string {
    if (savingBusy) return 'Saving…';
    if (!savedAt) return '';
    const ago = Math.max(0, Math.floor(Date.now() / 1000 - savedAt));
    if (ago < 5) return 'Saved just now';
    if (ago < 60) return `Saved ${ago}s ago`;
    if (ago < 3600) return `Saved ${Math.floor(ago / 60)}m ago`;
    return `Saved ${formatDate(savedAt * 1000)}`;
  }
</script>

<svelte:head>
  <title>Notes · Postern</title>
</svelte:head>

<div class="notes-shell" class:popup={isPopup}>
  {#if !isPopup}
    <div class="page-top">
      <a class="back" href="/inbox">← Inbox</a>
    </div>

    <header class="hero">
      <div class="hero-copy">
        <span class="eyebrow">Secure notes</span>
        <h1>Notes</h1>
        <p>
          Markdown notepad backed by the same vault as your mail. Pin
          what matters, search across everything, and rest knowing
          nothing leaves the encrypted store on disk.
        </p>
      </div>
      <button type="button" class="btn primary hero-action" onclick={newNote}>+ New note</button>
    </header>
  {/if}

<div class="page" class:show-list={mobileShowList}>
  <aside class="list-pane">
    <header class="list-head">
      <span class="list-head-title">All notes</span>
      <button class="new-btn" onclick={newNote} aria-label="New note" title="New note">
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
        bind:value={q}
        aria-label="Search notes"
      />
    </div>

    {#if loading}
      <div class="state">Loading…</div>
    {:else if err && notes.length === 0}
      <div class="state error">{err}</div>
    {:else if filtered.length === 0}
      <div class="state empty">
        {q.trim() ? 'No matches.' : 'No notes yet — create one to get started.'}
      </div>
    {:else}
      <ul class="list">
        {#each filtered as n (n.id)}
          <li>
            <button
              class="row"
              class:active={selected?.id === n.id}
              onclick={() => selectNote(n)}
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

  <section class="editor-pane">
    {#if !selected}
      <div class="placeholder">
        <p>Select a note from the list, or create a new one.</p>
      </div>
    {:else}
      <header class="editor-head">
        <button class="back-btn" onclick={backToList} aria-label="Back to list">
          <svg width="18" height="18" viewBox="0 0 20 20" aria-hidden="true">
            <path d="M12.5 4 6 10l6.5 6" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" />
          </svg>
        </button>
        <input
          class="title"
          type="text"
          placeholder="Untitled"
          bind:value={draftTitle}
          oninput={scheduleSave}
        />
        <div class="head-actions">
          <button
            class="icon"
            class:on={draftPinned}
            onclick={togglePin}
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
            onclick={deleteSelected}
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
        <span class="meta">{savedLabel()}</span>
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
          oninput={scheduleSave}
        ></textarea>
      {/if}
    {/if}
  </section>
</div>
</div>

<style>
  .notes-shell {
    width: 100%;
    max-width: clamp(60rem, 94vw, 110rem);
    margin: 0 auto;
    padding: 1.25rem 1.5rem 2rem;
    box-sizing: border-box;
    display: flex;
    flex-direction: column;
    min-height: 100dvh;
    gap: 1rem;
  }

  /* Popup mode: edge-to-edge in a small window, no hero, single
     master/detail pane filling the viewport. */
  .notes-shell.popup {
    max-width: none;
    padding: 0.5rem;
    gap: 0.5rem;
  }
  .notes-shell.popup .page {
    min-height: 0;
    border-radius: 0.6rem;
  }

  .page {
    flex: 1 1 auto;
    display: grid;
    grid-template-columns: minmax(280px, 22rem) 1fr;
    gap: 0;
    width: 100%;
    min-height: 32rem;
    border: 1px solid var(--border);
    border-radius: 1rem;
    overflow: hidden;
    background: var(--bg);
    color: var(--fg);
  }

  .page-top {
    margin-bottom: -0.25rem;
  }
  .back {
    color: var(--muted);
    text-decoration: none;
    font-size: 0.85rem;
  }
  .back:hover {
    color: var(--fg);
  }

  .hero {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 1.5rem;
    padding: 1.3rem 1.4rem;
    border: 1px solid var(--border);
    border-radius: 1.3rem;
    background:
      radial-gradient(circle at top right, color-mix(in oklab, var(--accent) 12%, transparent), transparent 35%),
      linear-gradient(180deg, color-mix(in oklab, var(--surface) 88%, white 12%), var(--surface));
    box-shadow: 0 18px 42px rgba(0, 0, 0, 0.08);
  }
  .hero-copy {
    flex: 1 1 auto;
    min-width: 0;
  }
  .hero .eyebrow {
    display: inline-block;
    margin-bottom: 0.55rem;
    font-size: 0.7rem;
    text-transform: uppercase;
    letter-spacing: 0.12em;
    color: var(--muted);
    font-weight: 700;
  }
  .hero h1 {
    margin: 0 0 0.3rem;
    font-size: 1.75rem;
  }
  .hero p {
    margin: 0;
    color: var(--muted);
    max-width: 44rem;
    line-height: 1.5;
  }
  .btn.primary {
    flex-shrink: 0;
    padding: 0.55rem 1rem;
    border-radius: 999px;
    border: 0;
    background: var(--accent);
    color: var(--bg);
    cursor: pointer;
    font: inherit;
    font-size: 0.88rem;
    font-weight: 600;
  }
  .btn.primary:hover {
    filter: brightness(0.96);
  }
  .hero-action { margin-top: 0.25rem; }

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

  /* Mobile master/detail switching: under 760px the editor takes
     over the whole viewport once a note is selected. */
  @media (max-width: 760px) {
    .page {
      grid-template-columns: 1fr;
    }
    .list-pane {
      display: none;
      border-right: 0;
    }
    .editor-pane {
      display: flex;
    }
    .page.show-list .list-pane {
      display: flex;
    }
    .page.show-list .editor-pane {
      display: none;
    }
    .back-btn {
      display: inline-flex;
    }
  }
</style>
