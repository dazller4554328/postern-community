<script lang="ts">
  import './templates.css';
  import { onMount } from 'svelte';
  import { api, type EmailTemplate } from '$lib/api';

  let templates = $state<EmailTemplate[]>([]);
  let loading = $state(true);

  // Create / edit form. `editingId === null` means "new template".
  let showForm = $state(false);
  let editingId = $state<number | null>(null);
  let formName = $state('');
  let formSubject = $state('');
  let formBody = $state('');
  let formErr = $state<string | null>(null);
  let busy = $state(false);

  onMount(async () => {
    try {
      templates = await api.templatesList();
    } catch {
      // Surface as an empty list; the user can retry by reloading.
    } finally {
      loading = false;
    }
  });

  function openNew() {
    editingId = null;
    formName = '';
    formSubject = '';
    formBody = '';
    formErr = null;
    showForm = true;
  }

  function openEdit(t: EmailTemplate) {
    editingId = t.id;
    formName = t.name;
    formSubject = t.subject;
    formBody = t.body;
    formErr = null;
    showForm = true;
  }

  function closeForm() {
    showForm = false;
    editingId = null;
    formErr = null;
  }

  function preview(body: string) {
    return body.replace(/\s+/g, ' ').trim();
  }

  async function save() {
    formErr = null;
    if (!formName.trim()) {
      formErr = 'Give the template a name.';
      return;
    }
    busy = true;
    try {
      const payload = { name: formName, subject: formSubject, body: formBody };
      if (editingId === null) {
        await api.templatesCreate(payload);
      } else {
        await api.templatesUpdate(editingId, payload);
      }
      templates = await api.templatesList();
      closeForm();
    } catch (e) {
      formErr = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
    }
  }

  async function remove(t: EmailTemplate) {
    if (!confirm(`Delete the "${t.name}" template?`)) return;
    await api.templatesDelete(t.id);
    templates = templates.filter((x) => x.id !== t.id);
    if (editingId === t.id) closeForm();
  }
</script>

<article class="templates-shell">
  <div class="page-top">
    <a class="back" href="/settings">← Settings</a>
  </div>

  <header class="hero">
    <div class="hero-copy">
      <span class="eyebrow">Compose</span>
      <h1>Email Templates</h1>
      <p>
        Save a subject and body you send often, then pick it from the
        <strong>Template</strong> row when writing a message. Your mailbox signature is
        added automatically, so leave it out of the template.
      </p>
    </div>
    <div class="hero-badges">
      <span class="hero-chip">Works with every mailbox</span>
      <span class="hero-chip">Signature kept</span>
    </div>
  </header>

  {#if loading}
    <p class="muted">Loading…</p>
  {:else if templates.length === 0 && !showForm}
    <p class="muted">No templates yet. Create one below and it will appear in the compose form.</p>
  {:else if templates.length > 0}
    <ul class="templates">
      {#each templates as t (t.id)}
        <li>
          <div class="tpl-info">
            <span class="tpl-name">{t.name}</span>
            {#if t.subject}<span class="tpl-subject">Subject: {t.subject}</span>{/if}
            {#if t.body.trim()}<span class="tpl-preview">{preview(t.body)}</span>{/if}
          </div>
          <div class="tpl-actions">
            <button class="ghost-sm" onclick={() => openEdit(t)}>Edit</button>
            <button class="danger" onclick={() => remove(t)}>Delete</button>
          </div>
        </li>
      {/each}
    </ul>
  {/if}

  <div class="toolbar">
    {#if !showForm}
      <button class="add-btn" onclick={openNew}>+ New template</button>
    {/if}
  </div>

  {#if showForm}
    <form class="add-form" onsubmit={(e) => { e.preventDefault(); save(); }}>
      <div class="field">
        <label for="tpl-name">Name</label>
        <input
          id="tpl-name"
          type="text"
          bind:value={formName}
          placeholder="Invoice reminder"
          autocomplete="off"
          maxlength="120"
        />
      </div>
      <div class="field">
        <label for="tpl-subject">Subject</label>
        <input
          id="tpl-subject"
          type="text"
          bind:value={formSubject}
          placeholder="Optional — leave blank to keep whatever is already there"
          autocomplete="off"
        />
      </div>
      <div class="field stacked">
        <label for="tpl-body">Message</label>
        <textarea id="tpl-body" bind:value={formBody} placeholder="Hi,&#10;&#10;…"></textarea>
      </div>
      {#if formErr}
        <div class="err">⚠ {formErr}</div>
      {/if}
      <div class="form-actions">
        <button type="button" class="ghost" onclick={closeForm}>Cancel</button>
        <button type="submit" class="primary" disabled={busy}>
          {busy ? 'Saving…' : editingId === null ? 'Create template' : 'Save changes'}
        </button>
      </div>
    </form>
  {/if}
</article>
