<script lang="ts">
  import { onMount } from 'svelte';
  import {
    api,
    type Reminder,
    type NewReminder,
    type ReminderRepeat
  } from '$lib/api';

  let reminders = $state<Reminder[]>([]);
  let includeDone = $state(false);
  let loading = $state(true);
  let err = $state<string | null>(null);

  // Inline editor state — either creating a new row or editing an
  // existing one.
  let editing = $state<Reminder | null>(null);
  let showCreate = $state(false);
  let draftTitle = $state('');
  let draftNotes = $state('');
  let draftDate = $state(''); // yyyy-mm-dd
  let draftTime = $state(''); // HH:mm
  let draftRepeat = $state<ReminderRepeat>('none');
  let savingBusy = $state(false);

  onMount(() => {
    void load();
  });

  async function load() {
    loading = true;
    try {
      reminders = await api.remindersList(includeDone);
      err = null;
    } catch (e) {
      err = e instanceof Error ? e.message : String(e);
      reminders = [];
    } finally {
      loading = false;
    }
  }

  function resetDraft() {
    draftTitle = '';
    draftNotes = '';
    const now = new Date();
    // Default to "the next round 15-minute slot" so the time input
    // lands on something reasonable without forcing users to clear
    // seconds.
    now.setMinutes(Math.ceil(now.getMinutes() / 15) * 15, 0, 0);
    draftDate = isoDateLocal(now);
    draftTime = isoTimeLocal(now);
    draftRepeat = 'none';
  }

  function startCreate() {
    editing = null;
    resetDraft();
    showCreate = true;
  }

  function startEdit(r: Reminder) {
    editing = r;
    const d = new Date(r.due_at_utc * 1000);
    draftTitle = r.title;
    draftNotes = r.notes ?? '';
    draftDate = isoDateLocal(d);
    draftTime = isoTimeLocal(d);
    draftRepeat = r.repeat;
    showCreate = false;
  }

  function cancelEdit() {
    editing = null;
    showCreate = false;
  }

  async function saveDraft() {
    const title = draftTitle.trim();
    if (!title) {
      err = 'Title is required';
      return;
    }
    const combined = new Date(`${draftDate}T${draftTime}`);
    if (Number.isNaN(combined.getTime())) {
      err = 'Invalid date/time';
      return;
    }
    const due_at_utc = Math.floor(combined.getTime() / 1000);
    savingBusy = true;
    try {
      if (editing) {
        await api.remindersUpdate(editing.id, {
          title,
          notes: draftNotes.trim() || null,
          due_at_utc,
          repeat: draftRepeat
        });
      } else {
        const body: NewReminder = {
          title,
          notes: draftNotes.trim() || null,
          due_at_utc,
          repeat: draftRepeat
        };
        await api.remindersCreate(body);
      }
      editing = null;
      showCreate = false;
      await load();
    } catch (e) {
      err = e instanceof Error ? e.message : String(e);
    } finally {
      savingBusy = false;
    }
  }

  async function markDone(r: Reminder) {
    try {
      await api.remindersMarkDone(r.id);
      await load();
    } catch (e) {
      err = e instanceof Error ? e.message : String(e);
    }
  }

  async function remove(r: Reminder) {
    if (!confirm(`Delete "${r.title}"?`)) return;
    try {
      await api.remindersDelete(r.id);
      await load();
    } catch (e) {
      err = e instanceof Error ? e.message : String(e);
    }
  }

  function isoDateLocal(d: Date): string {
    const y = d.getFullYear();
    const m = String(d.getMonth() + 1).padStart(2, '0');
    const day = String(d.getDate()).padStart(2, '0');
    return `${y}-${m}-${day}`;
  }

  function isoTimeLocal(d: Date): string {
    const h = String(d.getHours()).padStart(2, '0');
    const m = String(d.getMinutes()).padStart(2, '0');
    return `${h}:${m}`;
  }

  function formatDue(unix: number): string {
    return new Date(unix * 1000).toLocaleString([], {
      weekday: 'short',
      month: 'short',
      day: 'numeric',
      hour: 'numeric',
      minute: '2-digit'
    });
  }

  function repeatLabel(r: ReminderRepeat): string {
    switch (r) {
      case 'daily': return 'Daily';
      case 'weekly': return 'Weekly';
      case 'monthly': return 'Monthly';
      default: return '';
    }
  }

  function isOverdue(r: Reminder): boolean {
    if (r.done) return false;
    return r.due_at_utc * 1000 < Date.now();
  }
</script>

<article class="shell">
  <div class="page-top">
    <a class="back" href="/inbox">← Inbox</a>
  </div>

  <header class="hero">
    <div class="hero-copy">
      <span class="eyebrow">Reminders</span>
      <h1>Stay on top of it</h1>
      <p>
        Local-only reminders. Nothing leaves this device. Fires a
        notification at the due time — snooze or mark done from the
        popup.
      </p>
    </div>
    <div class="controls">
      <label class="toggle">
        <input type="checkbox" bind:checked={includeDone} onchange={() => void load()} />
        <span>Show completed</span>
      </label>
      <button class="btn primary" onclick={startCreate}>+ New</button>
    </div>
  </header>

  {#if err}
    <p class="err">⚠ {err}</p>
  {/if}

  {#if showCreate || editing}
    <section class="editor">
      <h2>{editing ? 'Edit reminder' : 'New reminder'}</h2>
      <div class="field">
        <label for="rem-title">Title</label>
        <input
          id="rem-title"
          type="text"
          bind:value={draftTitle}
          placeholder="e.g. Call the plumber"
          autocomplete="off"
        />
      </div>
      <div class="field">
        <label for="rem-notes">Notes (optional)</label>
        <textarea
          id="rem-notes"
          rows="2"
          bind:value={draftNotes}
          placeholder="Anything extra to remember"
        ></textarea>
      </div>
      <div class="row-2">
        <div class="field">
          <label for="rem-date">Date</label>
          <input id="rem-date" type="date" bind:value={draftDate} />
        </div>
        <div class="field">
          <label for="rem-time">Time</label>
          <input id="rem-time" type="time" bind:value={draftTime} />
        </div>
        <div class="field">
          <label for="rem-repeat">Repeat</label>
          <select id="rem-repeat" bind:value={draftRepeat}>
            <option value="none">Never</option>
            <option value="daily">Daily</option>
            <option value="weekly">Weekly</option>
            <option value="monthly">Monthly</option>
          </select>
        </div>
      </div>
      <div class="editor-actions">
        <button class="btn" onclick={cancelEdit}>Cancel</button>
        <button class="btn primary" disabled={savingBusy} onclick={saveDraft}>
          {savingBusy ? 'Saving…' : editing ? 'Save changes' : 'Create'}
        </button>
      </div>
    </section>
  {/if}

  {#if loading}
    <p class="loading">Loading reminders…</p>
  {:else if reminders.length === 0}
    <div class="empty">
      <h2>Nothing on the list</h2>
      <p>Hit <strong>+ New</strong> to schedule your first reminder.</p>
    </div>
  {:else}
    <ul class="list">
      {#each reminders as r (r.id)}
        <li class:done={r.done} class:overdue={isOverdue(r)}>
          <div class="line-1">
            <button
              class="check"
              aria-label={r.done ? 'Already done' : 'Mark done'}
              disabled={r.done}
              onclick={() => markDone(r)}
            >{r.done ? '✓' : ''}</button>
            <div class="core">
              <div class="title-row">
                <strong class="title">{r.title}</strong>
                {#if r.repeat !== 'none'}<span class="pill">↻ {repeatLabel(r.repeat)}</span>{/if}
                {#if isOverdue(r) && !r.done}<span class="pill warn">Overdue</span>{/if}
                {#if r.snoozed_until_utc && !r.done}<span class="pill">Snoozed</span>{/if}
              </div>
              <div class="meta">
                <span class="due">{formatDue(r.due_at_utc)}</span>
                {#if r.notes}<span class="notes">· {r.notes}</span>{/if}
              </div>
            </div>
            <div class="ops">
              <button class="icon" onclick={() => startEdit(r)} title="Edit">Edit</button>
              <button class="icon danger" onclick={() => remove(r)} title="Delete">Delete</button>
            </div>
          </div>
        </li>
      {/each}
    </ul>
  {/if}
</article>

<style>
  article.shell {
    max-width: 60rem;
    width: 100%;
    margin: 0 auto;
    padding: 1.25rem 1.75rem 2rem;
    box-sizing: border-box;
  }
  .page-top { margin-bottom: 0.55rem; }
  .back { color: var(--muted); text-decoration: none; font-size: 0.85rem; }
  .back:hover { color: var(--fg); }

  .hero {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 1rem;
    padding: 1.3rem 1.4rem;
    margin-bottom: 1rem;
    border: 1px solid var(--border);
    border-radius: 1.3rem;
    background:
      radial-gradient(circle at top right, color-mix(in oklab, var(--accent) 12%, transparent), transparent 35%),
      linear-gradient(180deg, color-mix(in oklab, var(--surface) 88%, white 12%), var(--surface));
    box-shadow: 0 18px 42px rgba(0, 0, 0, 0.08);
  }
  .eyebrow {
    display: inline-block;
    margin-bottom: 0.55rem;
    font-size: 0.7rem;
    text-transform: uppercase;
    letter-spacing: 0.12em;
    color: var(--muted);
    font-weight: 700;
  }
  .hero h1 { margin: 0 0 0.3rem; font-size: 1.9rem; letter-spacing: -0.02em; }
  .hero p { margin: 0; color: var(--muted); max-width: 40rem; }

  .controls { display: flex; gap: 0.6rem; align-self: start; align-items: center; }

  .toggle {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    font-size: 0.82rem;
    color: var(--muted);
    cursor: pointer;
  }

  .btn {
    padding: 0.45rem 0.9rem;
    border: 1px solid var(--border);
    background: var(--surface-2);
    color: var(--fg);
    border-radius: 0.5rem;
    cursor: pointer;
    font: inherit;
    font-size: 0.88rem;
  }
  .btn.primary {
    background: var(--accent);
    color: white;
    border-color: transparent;
    font-weight: 600;
  }
  .btn:disabled { opacity: 0.55; cursor: progress; }

  .editor {
    border: 1px solid var(--border);
    background: var(--surface);
    border-radius: 1rem;
    padding: 1rem 1.2rem 1.1rem;
    margin-bottom: 1rem;
    display: flex;
    flex-direction: column;
    gap: 0.8rem;
  }
  .editor h2 { margin: 0; font-size: 1.05rem; }
  .field { display: flex; flex-direction: column; gap: 0.3rem; min-width: 0; }
  .field label { font-size: 0.78rem; color: var(--muted); font-weight: 500; }
  .field input[type='text'],
  .field input[type='date'],
  .field input[type='time'],
  .field select,
  .field textarea {
    padding: 0.5rem 0.65rem;
    border: 1px solid var(--border);
    border-radius: 0.5rem;
    background: var(--surface-2);
    color: var(--fg);
    font: inherit;
    font-size: 0.92rem;
  }
  .field textarea { resize: vertical; min-height: 2.5rem; }
  .row-2 { display: grid; grid-template-columns: 1fr 1fr 1fr; gap: 0.8rem; }
  .editor-actions { display: flex; gap: 0.5rem; justify-content: flex-end; }

  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.45rem;
  }
  .list li {
    border: 1px solid var(--border);
    border-radius: 0.7rem;
    background: var(--surface);
    padding: 0.65rem 0.85rem;
  }
  .list li.done { opacity: 0.58; }
  .list li.overdue { border-left: 3px solid color-mix(in oklab, crimson 60%, var(--accent)); }

  .line-1 {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto;
    gap: 0.8rem;
    align-items: start;
  }

  .check {
    width: 1.4rem;
    height: 1.4rem;
    border-radius: 999px;
    border: 1.5px solid var(--border);
    background: transparent;
    color: var(--accent);
    cursor: pointer;
    font-size: 0.9rem;
    line-height: 1;
    display: grid;
    place-items: center;
    padding: 0;
    margin-top: 0.1rem;
  }
  .check:hover:not(:disabled) { border-color: var(--accent); }
  .check:disabled { cursor: default; background: color-mix(in oklab, var(--accent) 20%, transparent); border-color: var(--accent); }

  .core { min-width: 0; }
  .title-row { display: flex; flex-wrap: wrap; gap: 0.45rem; align-items: baseline; }
  .title { font-size: 0.98rem; font-weight: 600; }
  .list li.done .title { text-decoration: line-through; }

  .pill {
    font-size: 0.68rem;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    padding: 0.05rem 0.45rem;
    border-radius: 999px;
    background: color-mix(in oklab, var(--accent) 18%, var(--surface-2));
    color: color-mix(in oklab, var(--accent) 80%, var(--fg) 20%);
  }
  .pill.warn {
    background: color-mix(in oklab, crimson 22%, var(--surface-2));
    color: color-mix(in oklab, crimson 70%, var(--fg) 30%);
  }

  .meta { margin-top: 0.15rem; font-size: 0.82rem; color: var(--muted); }
  .notes { margin-left: 0.25rem; }

  .ops { display: flex; gap: 0.35rem; }
  .icon {
    background: transparent;
    border: 1px solid var(--border);
    color: var(--fg);
    font-size: 0.78rem;
    padding: 0.25rem 0.55rem;
    border-radius: 0.4rem;
    cursor: pointer;
  }
  .icon:hover { background: var(--surface-2); }
  .icon.danger:hover {
    border-color: color-mix(in oklab, crimson 55%, var(--border));
    color: color-mix(in oklab, crimson 70%, var(--fg) 30%);
  }

  .empty {
    padding: 2.5rem 1.5rem;
    text-align: center;
    border: 1px dashed var(--border);
    border-radius: 1rem;
    color: var(--muted);
  }
  .empty h2 { margin: 0 0 0.5rem; color: var(--fg); }

  .err {
    padding: 0.6rem 0.9rem;
    border-radius: 0.6rem;
    background: color-mix(in oklab, crimson 10%, var(--surface));
    color: color-mix(in oklab, crimson 70%, var(--fg) 30%);
    margin-bottom: 0.75rem;
  }
  .loading { color: var(--muted); font-size: 0.88rem; }

  @media (max-width: 900px) {
    article.shell { padding: 0.75rem; }
    .hero { grid-template-columns: 1fr; }
    .row-2 { grid-template-columns: 1fr 1fr; }
    .line-1 { grid-template-columns: auto 1fr; }
    .ops { grid-column: 2; justify-content: flex-end; margin-top: 0.35rem; }
  }
</style>
