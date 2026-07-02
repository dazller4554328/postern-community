<script lang="ts">
  import './reminders.css';
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

<article class="reminders-shell">
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

