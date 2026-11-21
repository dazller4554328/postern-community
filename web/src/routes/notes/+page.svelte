<script lang="ts">
  // Secure notes — markdown notepad backed by /api/notes*. Same vault
  // gate as the rest of the app: anything we read/write lives in the
  // SQLCipher DB and is unreachable while locked.
  //
  // Layout: master/detail. Sidebar list of notes (pinned + recent),
  // editor on the right with debounced auto-save. On narrow screens
  // the list collapses behind a "back" button when a note is open.

  import './notes.css';
  import { onDestroy, onMount, tick } from 'svelte';
  import { page } from '$app/stores';
  import { api, type Note } from '$lib/api';
  import { formatDate } from '$lib/format';
  import NoteListPane from './_components/NoteListPane.svelte';
  import NoteEditor from './_components/NoteEditor.svelte';
  import NoteHistoryModal from './_components/NoteHistoryModal.svelte';

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

  // Mobile detail-mode toggle. When a note is open on a narrow
  // viewport the list slides away so the editor gets full width.
  let mobileShowList = $state(true);

  let historyOpen = $state(false);

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

  // previewHtml / summarize / displayTitle live in _lib/markdown.ts.

  function backToList() {
    if (saveTimer) {
      clearTimeout(saveTimer);
      saveTimer = null;
      void persist();
    }
    mobileShowList = true;
  }

  async function openHistory() {
    if (!selected) return;
    // Flush any pending debounced save so the history list reflects
    // what the user actually has on screen — otherwise they might see
    // a "previous" version that's identical to what they're staring at.
    if (saveTimer) {
      clearTimeout(saveTimer);
      saveTimer = null;
      await persist();
    }
    historyOpen = true;
  }

  function onRevisionRestored(updated: Note) {
    // Re-bind selection and the editor's draft buffers to the restored
    // version. The list row also needs splicing in case the row count
    // or pinned flag changed (the restore only touches title/body, so
    // pinned stays put, but updated_at moved it to the top).
    const idx = notes.findIndex((n) => n.id === updated.id);
    if (idx >= 0) notes[idx] = updated;
    bindSelected(updated);
    savedAt = updated.updated_at;
  }

    let savedLabel = $derived.by(() => {
    if (savingBusy) return 'Saving…';
    if (!savedAt) return '';
    const ago = Math.max(0, Math.floor(Date.now() / 1000 - savedAt));
    if (ago < 5) return 'Saved just now';
    if (ago < 60) return `Saved ${ago}s ago`;
    if (ago < 3600) return `Saved ${Math.floor(ago / 60)}m ago`;
    return `Saved ${formatDate(savedAt * 1000)}`;
  });
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
  <NoteListPane
    notes={filtered}
    {selected}
    {loading}
    {err}
    query={q}
    onQueryChange={(v) => (q = v)}
    onNew={newNote}
    onSelect={selectNote}
  />

  <NoteEditor
    {selected}
    bind:draftTitle
    bind:draftBody
    {draftPinned}
    bind:preview
    {err}
    {savedLabel}
    onScheduleSave={scheduleSave}
    onTogglePin={togglePin}
    onDelete={deleteSelected}
    onBackToList={backToList}
    onOpenHistory={openHistory}
  />
</div>
</div>

{#if historyOpen && selected}
  <NoteHistoryModal
    noteId={selected.id}
    onRestored={onRevisionRestored}
    onClose={() => (historyOpen = false)}
  />
{/if}

