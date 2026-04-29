<script lang="ts">
  // Contacts page — paginated address-book list with search, edit,
  // delete, favorite-toggle, and manual add. Backed by /api/contacts*
  // which all read/write the dedicated contacts table fed by the
  // message-insert hook + the boot backfill from existing mail.

  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { api, type Contact } from '$lib/api';
  import { formatDate } from '$lib/format';
  import SenderAvatar from '$lib/components/SenderAvatar.svelte';

  let contacts = $state<Contact[]>([]);
  let total = $state(0);
  let loading = $state(true);
  let err = $state<string | null>(null);

  // Search + paging state. Server caps limit at 500; 50 is a comfy
  // page size for desktop and not too long to scroll on mobile.
  let q = $state('');
  let qDebounced = $state('');
  let qTimer: ReturnType<typeof setTimeout> | null = null;
  let page = $state(0);
  const PAGE_SIZE = 50;
  let totalPages = $derived(Math.max(1, Math.ceil(total / PAGE_SIZE)));

  // Edit modal state. `editing` is the Contact being edited (or
  // null when closed). `addOpen` controls the manual-add modal.
  let editing = $state<Contact | null>(null);
  let editForm = $state({ display_name: '', notes: '', is_favorite: false });
  let editBusy = $state(false);
  let editErr = $state<string | null>(null);

  let addOpen = $state(false);
  let addForm = $state({
    address: '',
    display_name: '',
    notes: '',
    is_favorite: false
  });
  let addBusy = $state(false);
  let addErr = $state<string | null>(null);

  async function load() {
    loading = true;
    try {
      const r = await api.listContacts({
        q: qDebounced || undefined,
        limit: PAGE_SIZE,
        offset: page * PAGE_SIZE
      });
      contacts = r.contacts;
      total = r.total;
      err = null;
    } catch (e) {
      err = e instanceof Error ? e.message : String(e);
    } finally {
      loading = false;
    }
  }

  onMount(load);

  // Reset to page 0 whenever the search query changes; otherwise
  // the user types something specific on page 4 and gets "no
  // results" because the new query has fewer matches.
  $effect(() => {
    void qDebounced;
    page = 0;
    load();
  });

  function onSearchInput() {
    if (qTimer) clearTimeout(qTimer);
    qTimer = setTimeout(() => {
      qDebounced = q.trim();
    }, 220);
  }

  function prevPage() {
    if (page > 0) {
      page -= 1;
      load();
    }
  }
  function nextPage() {
    if (page < totalPages - 1) {
      page += 1;
      load();
    }
  }

  async function toggleFavorite(c: Contact) {
    try {
      const updated = await api.updateContact(c.id, { is_favorite: !c.is_favorite });
      contacts = contacts.map((x) => (x.id === c.id ? updated : x));
    } catch (e) {
      err = e instanceof Error ? e.message : String(e);
    }
  }

  function openEdit(c: Contact) {
    editing = c;
    editForm = {
      display_name: c.display_name ?? '',
      notes: c.notes ?? '',
      is_favorite: c.is_favorite
    };
    editErr = null;
  }
  function closeEdit() {
    if (editBusy) return;
    editing = null;
  }
  async function saveEdit() {
    if (!editing) return;
    editBusy = true;
    editErr = null;
    try {
      // Pass null (not undefined) when the user cleared a field, so
      // the server stores NULL instead of leaving the previous value.
      const updated = await api.updateContact(editing.id, {
        display_name: editForm.display_name.trim() ? editForm.display_name.trim() : null,
        notes: editForm.notes.trim() ? editForm.notes.trim() : null,
        is_favorite: editForm.is_favorite
      });
      contacts = contacts.map((x) => (x.id === updated.id ? updated : x));
      editing = null;
    } catch (e) {
      editErr = e instanceof Error ? e.message : String(e);
    } finally {
      editBusy = false;
    }
  }

  async function deleteContact(c: Contact) {
    if (
      !confirm(
        `Delete contact "${c.display_name || c.address}"?\n\n` +
          `They'll be re-added automatically the next time a message from or to them is synced.`
      )
    )
      return;
    try {
      await api.deleteContact(c.id);
      // Refetch the page rather than slicing locally — keeps total
      // count + page padding correct without bookkeeping.
      await load();
    } catch (e) {
      err = e instanceof Error ? e.message : String(e);
    }
  }

  function openAdd() {
    addForm = { address: '', display_name: '', notes: '', is_favorite: false };
    addErr = null;
    addOpen = true;
  }
  function closeAdd() {
    if (addBusy) return;
    addOpen = false;
  }
  async function saveAdd() {
    addBusy = true;
    addErr = null;
    try {
      await api.createContact({
        address: addForm.address.trim(),
        display_name: addForm.display_name.trim() || undefined,
        notes: addForm.notes.trim() || undefined,
        is_favorite: addForm.is_favorite
      });
      addOpen = false;
      // Jump back to page 0 so the new entry is visible.
      page = 0;
      await load();
    } catch (e) {
      addErr = e instanceof Error ? e.message : String(e);
    } finally {
      addBusy = false;
    }
  }

  function composeTo(c: Contact) {
    const target = c.display_name
      ? `${c.display_name} <${c.address}>`
      : c.address;
    goto(`/compose?to=${encodeURIComponent(target)}`);
  }
</script>

<svelte:head>
  <title>Contacts — Postern</title>
</svelte:head>

<article class="contacts-page">
  <header class="page-head">
    <div class="head-text">
      <h1>Contacts</h1>
      <p>
        Auto-collected from sync and send. Every address that's been on a message
        gets a row here — edit display names, jot notes, favourite the people you
        email a lot, or add someone manually before you've ever sent them mail.
      </p>
    </div>
    <button type="button" class="btn primary" onclick={openAdd}>+ Add contact</button>
  </header>

  <div class="toolbar">
    <input
      class="search"
      type="search"
      bind:value={q}
      oninput={onSearchInput}
      placeholder="Search by name or address…"
      autocomplete="off"
    />
    <span class="result-count">
      {#if loading}
        Loading…
      {:else if total === 0}
        No contacts
      {:else}
        {total.toLocaleString()} {total === 1 ? 'contact' : 'contacts'}
      {/if}
    </span>
  </div>

  {#if err}
    <p class="err-bubble">⚠ {err}</p>
  {/if}

  {#if !loading && contacts.length === 0 && !err}
    <div class="empty">
      {#if qDebounced}
        <p>No contacts match <em>"{qDebounced}"</em>.</p>
      {:else}
        <p>
          No contacts yet. They'll start appearing as soon as you sync mail —
          or click <strong>Add contact</strong> to enter one manually.
        </p>
      {/if}
    </div>
  {/if}

  {#if contacts.length > 0}
    <ul class="contact-list">
      {#each contacts as c (c.id)}
        <li class="contact-row">
          <button
            type="button"
            class="fav-btn"
            class:on={c.is_favorite}
            onclick={() => toggleFavorite(c)}
            aria-label={c.is_favorite ? 'Remove favourite' : 'Mark favourite'}
            title={c.is_favorite ? 'Remove favourite' : 'Mark favourite'}
          >
            {#if c.is_favorite}
              <svg viewBox="0 0 20 20" width="18" height="18" fill="currentColor">
                <path d="M10 2.5l2.4 4.86 5.36.78-3.88 3.78.92 5.34L10 14.74 5.2 17.26l.92-5.34L2.24 8.14l5.36-.78z" />
              </svg>
            {:else}
              <svg viewBox="0 0 20 20" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round">
                <path d="M10 2.5l2.4 4.86 5.36.78-3.88 3.78.92 5.34L10 14.74 5.2 17.26l.92-5.34L2.24 8.14l5.36-.78z" />
              </svg>
            {/if}
          </button>

          <SenderAvatar email={c.address} size={36} fetchRemote={false} />

          <div class="contact-body">
            <div class="contact-line">
              <strong>{c.display_name || c.address}</strong>
              {#if c.display_name}
                <span class="muted-addr">{c.address}</span>
              {/if}
            </div>
            <div class="contact-meta">
              {#if c.message_count > 0}
                {c.message_count.toLocaleString()}
                {c.message_count === 1 ? 'message' : 'messages'}
                · last on {formatDate(c.last_seen_utc)}
              {:else}
                Manual entry — no messages yet
              {/if}
              {#if c.notes}<span class="has-notes" title={c.notes}>· note</span>{/if}
            </div>
          </div>

          <div class="row-actions">
            <button class="btn ghost small" type="button" onclick={() => composeTo(c)}>Email</button>
            <button class="btn ghost small" type="button" onclick={() => openEdit(c)}>Edit</button>
            <button class="btn ghost small danger" type="button" onclick={() => deleteContact(c)}>Delete</button>
          </div>
        </li>
      {/each}
    </ul>

    {#if totalPages > 1}
      <div class="pager" aria-label="Contact pages">
        <button
          type="button"
          class="btn ghost"
          disabled={page === 0 || loading}
          onclick={prevPage}
        >Previous</button>
        <span class="pager-status">
          Page {page + 1} of {totalPages}
        </span>
        <button
          type="button"
          class="btn ghost"
          disabled={page >= totalPages - 1 || loading}
          onclick={nextPage}
        >Next</button>
      </div>
    {/if}
  {/if}
</article>

<!-- Edit modal -->
{#if editing}
  <div class="modal-backdrop" role="presentation" onclick={closeEdit} onkeydown={(e) => { if (e.key === 'Escape') closeEdit(); }}>
    <div class="modal" role="dialog" aria-modal="true" aria-labelledby="edit-title" tabindex="-1" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.stopPropagation()}>
      <h3 id="edit-title">Edit contact</h3>
      <p class="modal-sub muted-addr">{editing.address}</p>
      <label class="modal-field">
        <span>Display name</span>
        <input type="text" bind:value={editForm.display_name} placeholder="Joe Bloggs" />
      </label>
      <label class="modal-field">
        <span>Notes</span>
        <textarea rows="3" bind:value={editForm.notes} placeholder="Anything you'd like to remember about them"></textarea>
      </label>
      <label class="modal-toggle">
        <input type="checkbox" bind:checked={editForm.is_favorite} />
        <span>Favourite — pin to the top of the list</span>
      </label>
      {#if editErr}
        <p class="err-bubble">⚠ {editErr}</p>
      {/if}
      <div class="modal-actions">
        <button type="button" class="btn" onclick={closeEdit} disabled={editBusy}>Cancel</button>
        <button type="button" class="btn primary" onclick={saveEdit} disabled={editBusy}>
          {editBusy ? 'Saving…' : 'Save'}
        </button>
      </div>
    </div>
  </div>
{/if}

<!-- Add modal -->
{#if addOpen}
  <div class="modal-backdrop" role="presentation" onclick={closeAdd} onkeydown={(e) => { if (e.key === 'Escape') closeAdd(); }}>
    <div class="modal" role="dialog" aria-modal="true" aria-labelledby="add-title" tabindex="-1" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.stopPropagation()}>
      <h3 id="add-title">Add contact</h3>
      <p class="modal-sub muted-addr">
        Enter someone you haven't emailed yet. They'll merge with auto-collected
        rows once a message arrives or is sent.
      </p>
      <label class="modal-field">
        <span>Email address <em>(required)</em></span>
        <input type="email" bind:value={addForm.address} placeholder="alice@example.com" required />
      </label>
      <label class="modal-field">
        <span>Display name</span>
        <input type="text" bind:value={addForm.display_name} placeholder="Alice Allen" />
      </label>
      <label class="modal-field">
        <span>Notes</span>
        <textarea rows="3" bind:value={addForm.notes}></textarea>
      </label>
      <label class="modal-toggle">
        <input type="checkbox" bind:checked={addForm.is_favorite} />
        <span>Favourite</span>
      </label>
      {#if addErr}
        <p class="err-bubble">⚠ {addErr}</p>
      {/if}
      <div class="modal-actions">
        <button type="button" class="btn" onclick={closeAdd} disabled={addBusy}>Cancel</button>
        <button type="button" class="btn primary" onclick={saveAdd} disabled={addBusy || !addForm.address.includes('@')}>
          {addBusy ? 'Adding…' : 'Add'}
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .contacts-page {
    max-width: 60rem;
    margin: 0 auto;
    padding: 1.5rem 1.2rem 6rem;
  }

  .page-head {
    display: flex;
    align-items: flex-start;
    gap: 1.5rem;
    margin-bottom: 1.2rem;
  }
  .page-head .head-text {
    flex: 1 1 auto;
    min-width: 0;
  }
  .page-head h1 {
    margin: 0 0 0.4rem;
    font-size: 1.6rem;
    font-weight: 650;
    letter-spacing: -0.02em;
  }
  .page-head p {
    margin: 0;
    color: color-mix(in oklab, currentColor 65%, transparent);
    font-size: 0.88rem;
    line-height: 1.5;
  }

  .toolbar {
    display: flex;
    align-items: center;
    gap: 0.85rem;
    margin-bottom: 0.85rem;
  }
  .toolbar .search {
    flex: 1 1 auto;
    min-width: 0;
    padding: 0.65rem 0.95rem;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: var(--surface-2, color-mix(in oklab, currentColor 4%, var(--surface)));
    color: inherit;
    font: inherit;
    font-size: 0.92rem;
  }
  .toolbar .search:focus {
    outline: none;
    border-color: color-mix(in oklab, var(--accent) 45%, transparent);
    box-shadow: 0 0 0 3px color-mix(in oklab, var(--accent) 18%, transparent);
  }
  .result-count {
    color: color-mix(in oklab, currentColor 60%, transparent);
    font-size: 0.85rem;
    flex-shrink: 0;
  }

  .empty {
    padding: 2rem 1rem;
    text-align: center;
    color: color-mix(in oklab, currentColor 60%, transparent);
    font-size: 0.92rem;
  }

  .contact-list {
    list-style: none;
    margin: 0;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: 0.85rem;
    overflow: hidden;
    background: var(--surface);
  }
  .contact-row {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    padding: 0.7rem 0.9rem;
    border-bottom: 1px solid var(--border);
  }
  .contact-row:last-child {
    border-bottom: 0;
  }
  .contact-row:hover {
    background: color-mix(in oklab, currentColor 4%, transparent);
  }

  .fav-btn {
    background: transparent;
    border: 0;
    cursor: pointer;
    color: color-mix(in oklab, currentColor 35%, transparent);
    padding: 0.25rem;
    border-radius: 999px;
    display: inline-grid;
    place-items: center;
  }
  .fav-btn.on {
    color: gold;
  }
  .fav-btn:hover {
    background: color-mix(in oklab, currentColor 6%, transparent);
  }

  .contact-body {
    flex: 1 1 auto;
    min-width: 0;
  }
  .contact-line {
    display: flex;
    align-items: baseline;
    gap: 0.6rem;
    flex-wrap: wrap;
    line-height: 1.3;
  }
  .contact-line strong {
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .muted-addr {
    color: color-mix(in oklab, currentColor 55%, transparent);
    font-size: 0.84rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .contact-meta {
    margin-top: 0.18rem;
    color: color-mix(in oklab, currentColor 55%, transparent);
    font-size: 0.78rem;
  }
  .has-notes {
    color: color-mix(in oklab, var(--accent) 75%, currentColor);
    margin-left: 0.25rem;
  }

  .row-actions {
    display: inline-flex;
    gap: 0.35rem;
    flex-shrink: 0;
  }
  .row-actions .btn.small {
    padding: 0.32rem 0.65rem;
    font-size: 0.78rem;
    border-radius: 0.4rem;
  }
  .row-actions .btn.danger {
    color: color-mix(in oklab, crimson 75%, currentColor);
    border-color: color-mix(in oklab, crimson 35%, var(--border));
  }

  .pager {
    display: flex;
    align-items: center;
    gap: 0.7rem;
    margin: 0.8rem 0 0;
    padding: 0.55rem 0.75rem;
    background: color-mix(in oklab, currentColor 4%, transparent);
    border-radius: 0.5rem;
    font-size: 0.84rem;
  }
  .pager-status {
    flex: 1 1 auto;
    text-align: center;
    color: color-mix(in oklab, currentColor 70%, transparent);
  }

  .btn {
    font: inherit;
    font-size: 0.85rem;
    padding: 0.5rem 1.1rem;
    border-radius: 999px;
    cursor: pointer;
    border: 1px solid color-mix(in oklab, currentColor 18%, transparent);
    background: transparent;
    color: inherit;
  }
  .btn.primary {
    background: var(--accent);
    border-color: var(--accent);
    color: white;
    font-weight: 500;
  }
  .btn.primary:hover:not(:disabled) {
    filter: brightness(0.94);
  }
  .btn.ghost:hover:not(:disabled) {
    background: color-mix(in oklab, currentColor 6%, transparent);
  }
  .btn:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }

  /* Modal */
  .modal-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.45);
    backdrop-filter: blur(3px);
    display: grid;
    place-items: center;
    z-index: 200;
    padding: 1rem;
  }
  .modal {
    width: 100%;
    max-width: 30rem;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 1rem;
    padding: 1.3rem 1.5rem;
    box-shadow: 0 24px 60px rgba(0, 0, 0, 0.3);
    color: var(--fg);
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }
  .modal h3 {
    margin: 0;
    font-size: 1.05rem;
  }
  .modal-sub {
    margin: 0;
    font-size: 0.83rem;
    line-height: 1.5;
  }
  .modal-field {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    font-size: 0.78rem;
    font-weight: 500;
  }
  .modal-field em {
    font-style: normal;
    font-weight: 500;
    color: color-mix(in oklab, currentColor 50%, transparent);
  }
  .modal-field input,
  .modal-field textarea {
    font: inherit;
    font-size: 0.9rem;
    padding: 0.5rem 0.65rem;
    border: 1px solid var(--border);
    border-radius: 0.5rem;
    background: var(--surface);
    color: inherit;
    width: 100%;
    box-sizing: border-box;
    resize: vertical;
  }
  .modal-field input:focus,
  .modal-field textarea:focus {
    outline: none;
    border-color: color-mix(in oklab, var(--accent) 45%, transparent);
    box-shadow: 0 0 0 3px color-mix(in oklab, var(--accent) 18%, transparent);
  }
  .modal-toggle {
    display: inline-flex;
    align-items: center;
    gap: 0.5rem;
    font-size: 0.86rem;
  }
  .modal-actions {
    display: flex;
    justify-content: flex-end;
    gap: 0.5rem;
    margin-top: 0.4rem;
  }

  .err-bubble {
    margin: 0;
    padding: 0.55rem 0.85rem;
    background: color-mix(in oklab, tomato 12%, transparent);
    border-left: 2px solid tomato;
    border-radius: 0 0.5rem 0.5rem 0;
    font-size: 0.84rem;
  }

  @media (max-width: 600px) {
    .page-head {
      flex-direction: column;
      gap: 0.7rem;
    }
    .row-actions {
      flex-wrap: wrap;
    }
  }
</style>
