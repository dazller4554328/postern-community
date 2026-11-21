<script lang="ts">
  // Contacts page — paginated address-book list with search, edit,
  // delete, favorite-toggle, and manual add. Backed by /api/contacts*
  // which all read/write the dedicated contacts table fed by the
  // message-insert hook + the boot backfill from existing mail.

  import './contacts.css';
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { api, type Contact } from '$lib/api';
  import ContactRow from './_components/ContactRow.svelte';
  import ContactFormModal from './_components/ContactFormModal.svelte';

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
  // address is unused in edit mode (rendered as meta text by the modal)
  // but lives on the form so the shared ContactFormModal type matches.
  let editForm = $state({ address: '', display_name: '', notes: '', is_favorite: false });
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

  // Photo upload — bumps the avatar URL with a cache-buster query
  // string so SenderAvatar refetches and the new image appears
  // immediately without a page reload.
  let photoBumps = $state<Record<number, number>>({});
  async function uploadPhoto(c: Contact, file: File) {
    try {
      await api.setContactPhoto(c.id, file);
      photoBumps = { ...photoBumps, [c.id]: Date.now() };
    } catch (e) {
      err = e instanceof Error ? e.message : String(e);
    }
  }
  async function clearPhoto(c: Contact) {
    if (!confirm(`Clear stored photo for ${c.display_name || c.address}?`)) return;
    try {
      await api.clearContactPhoto(c.id);
      photoBumps = { ...photoBumps, [c.id]: Date.now() };
    } catch (e) {
      err = e instanceof Error ? e.message : String(e);
    }
  }

  function openEdit(c: Contact) {
    editing = c;
    editForm = {
      address: c.address,
      display_name: c.display_name ?? '',
      notes: c.notes ?? '',
      is_favorite: c.is_favorite,
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
  <div class="page-top">
    <a class="back" href="/inbox">← Inbox</a>
  </div>

  <header class="hero">
    <div class="hero-copy">
      <span class="eyebrow">Address book</span>
      <h1>Contacts</h1>
      <p>
        Auto-collected from sync and send. Every address that's been on a message
        gets a row here — edit display names, jot notes, favourite the people you
        email a lot, or add someone manually before you've ever sent them mail.
      </p>
    </div>
    <button type="button" class="btn primary hero-action" onclick={openAdd}>+ Add contact</button>
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
        <ContactRow
          contact={c}
          photoVersion={photoBumps[c.id]}
          onToggleFavorite={() => toggleFavorite(c)}
          onUploadPhoto={(f) => uploadPhoto(c, f)}
          onClearPhoto={() => clearPhoto(c)}
          onCompose={() => composeTo(c)}
          onEdit={() => openEdit(c)}
          onDelete={() => deleteContact(c)}
        />
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

<!-- Edit modal: address field is omitted (edit-only), shown as meta. -->
<ContactFormModal
  mode="edit"
  open={editing !== null}
  busy={editBusy}
  err={editErr}
  bind:form={editForm}
  editingAddress={editing?.address ?? undefined}
  onClose={closeEdit}
  onSave={saveEdit}
/>

<!-- Add modal: address field is required + writable. -->
<ContactFormModal
  mode="add"
  open={addOpen}
  busy={addBusy}
  err={addErr}
  bind:form={addForm}
  onClose={closeAdd}
  onSave={saveAdd}
/>

