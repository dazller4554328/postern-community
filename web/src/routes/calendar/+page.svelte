<script lang="ts">
  import { onMount } from 'svelte';
  import {
    api,
    type CalAccount,
    type CalCalendar,
    type EventOccurrence,
    type CalEvent,
    type NewLocalEvent,
    type PatchLocalEvent,
    type Reminder
  } from '$lib/api';

  // Anchor date drives the month grid. Defaults to today; Prev/Next
  // reset to the 1st of the neighbouring month so the grid is stable
  // regardless of where within the month you navigate from.
  let anchor = $state<Date>(startOfMonth(new Date()));
  let view = $state<'month' | 'day'>('month');
  let selectedDay = $state<Date | null>(null);

  let accounts = $state<CalAccount[]>([]);
  let calendars = $state<CalCalendar[]>([]);
  let events = $state<EventOccurrence[]>([]);
  let reminders = $state<Reminder[]>([]);
  let loading = $state(true);
  let err = $state<string | null>(null);
  let syncBusy = $state<Record<number, boolean>>({});

  // calendar_id → account.kind, so the UI can decide whether
  // edit/delete should even appear (CalDAV events are read-only at the
  // API surface until two-way sync ships).
  let calendarKind = $derived.by(() => {
    const accountById = new Map(accounts.map((a) => [a.id, a]));
    const out: Record<number, 'local' | 'caldav'> = {};
    for (const c of calendars) {
      out[c.id] = (accountById.get(c.account_id)?.kind ?? 'caldav') as
        | 'local'
        | 'caldav';
    }
    return out;
  });
  let defaultLocalCalendarId = $derived.by(() => {
    const localAcc = accounts.find((a) => a.kind === 'local');
    if (!localAcc) return null;
    const cal = calendars.find((c) => c.account_id === localAcc.id);
    return cal?.id ?? null;
  });

  // Detail popover (read-only view).
  let detailEvent = $state<CalEvent | null>(null);
  let detailBusy = $state(false);

  // Editor (create + edit).
  type EditorMode = 'create' | 'edit';
  let editorOpen = $state(false);
  let editorMode = $state<EditorMode>('create');
  let editorBusy = $state(false);
  let editorEventId = $state<number | null>(null);
  let editorCalendarId = $state<number | null>(null);
  let editorForm = $state({
    summary: '',
    description: '',
    location: '',
    dtstart: '', // datetime-local string ('YYYY-MM-DDTHH:mm')
    dtend: '',
    all_day: false,
    repeat: 'none' as 'none' | 'daily' | 'weekly' | 'monthly' | 'yearly'
  });

  onMount(() => {
    void loadAccounts();
    void loadCalendars();
    void loadEvents();
  });

  $effect(() => {
    anchor;
    void loadEvents();
  });

  async function loadAccounts() {
    try {
      accounts = await api.calListAccounts();
    } catch (e) {
      accounts = [];
      err = e instanceof Error ? e.message : String(e);
    }
  }

  async function loadCalendars() {
    try {
      calendars = await api.calListCalendars();
    } catch (e) {
      calendars = [];
      err = e instanceof Error ? e.message : String(e);
    }
  }

  async function loadEvents() {
    loading = true;
    try {
      // Pad the range so the grid's leading/trailing days still get
      // populated. Month view renders a 6-week grid which may include
      // days from the prior/next month.
      const from = Math.floor(startOfGrid(anchor).getTime() / 1000);
      const to = Math.floor(endOfGrid(anchor).getTime() / 1000);
      // Parallel: events + reminders share the same range query.
      const [ev, rem] = await Promise.all([
        api.calListEventsInRange(from, to),
        api.remindersInRange(from, to)
      ]);
      events = ev;
      reminders = rem;
      err = null;
    } catch (e) {
      events = [];
      reminders = [];
      err = e instanceof Error ? e.message : String(e);
    } finally {
      loading = false;
    }
  }

  async function syncAccount(id: number) {
    syncBusy = { ...syncBusy, [id]: true };
    try {
      await api.calSyncAccount(id);
      await Promise.all([loadAccounts(), loadEvents()]);
    } catch (e) {
      err = e instanceof Error ? e.message : String(e);
    } finally {
      syncBusy = { ...syncBusy, [id]: false };
    }
  }

  async function openEventDetail(occ: EventOccurrence) {
    detailBusy = true;
    try {
      detailEvent = await api.calGetEvent(occ.id);
    } catch (e) {
      err = e instanceof Error ? e.message : String(e);
    } finally {
      detailBusy = false;
    }
  }

  // ---- Editor ------------------------------------------------------

  /** `Date` → `YYYY-MM-DDTHH:mm` in local time. The browser's
   *  `datetime-local` input wants exactly this — it intentionally
   *  doesn't accept timezones because its purpose is "the time the
   *  user typed in their local clock." We flip back to UTC in
   *  `localDatetimeToUnix` on the way out. */
  function unixToLocalDatetime(unix: number): string {
    const d = new Date(unix * 1000);
    const pad = (n: number) => n.toString().padStart(2, '0');
    return (
      `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}` +
      `T${pad(d.getHours())}:${pad(d.getMinutes())}`
    );
  }
  function localDatetimeToUnix(s: string): number {
    if (!s) return Math.floor(Date.now() / 1000);
    return Math.floor(new Date(s).getTime() / 1000);
  }

  function rruleFromRepeat(r: typeof editorForm.repeat): string | null {
    switch (r) {
      case 'daily':
        return 'FREQ=DAILY';
      case 'weekly':
        return 'FREQ=WEEKLY';
      case 'monthly':
        return 'FREQ=MONTHLY';
      case 'yearly':
        return 'FREQ=YEARLY';
      default:
        return null;
    }
  }
  function repeatFromRrule(rr: string | null | undefined): typeof editorForm.repeat {
    if (!rr) return 'none';
    if (/FREQ=DAILY/i.test(rr)) return 'daily';
    if (/FREQ=WEEKLY/i.test(rr)) return 'weekly';
    if (/FREQ=MONTHLY/i.test(rr)) return 'monthly';
    if (/FREQ=YEARLY/i.test(rr)) return 'yearly';
    return 'none';
  }

  function openCreateEditor(forDay?: Date) {
    if (defaultLocalCalendarId == null) {
      err = 'No local calendar available yet — try reloading the page.';
      return;
    }
    const start = new Date(forDay ?? today);
    // If the caller passed a day with hours zeroed (e.g. from a month-
    // grid cell), default the start to a sensible 9am on that day so
    // the user isn't typing into 00:00.
    if (start.getHours() === 0 && start.getMinutes() === 0) {
      start.setHours(9, 0, 0, 0);
    }
    const end = new Date(start);
    end.setHours(end.getHours() + 1);
    editorMode = 'create';
    editorEventId = null;
    editorCalendarId = defaultLocalCalendarId;
    editorForm = {
      summary: '',
      description: '',
      location: '',
      dtstart: unixToLocalDatetime(Math.floor(start.getTime() / 1000)),
      dtend: unixToLocalDatetime(Math.floor(end.getTime() / 1000)),
      all_day: false,
      repeat: 'none'
    };
    editorOpen = true;
  }

  function openEditEditor(ev: CalEvent) {
    const kind = calendarKind[ev.calendar_id];
    if (kind !== 'local') {
      err = 'This event lives on a CalDAV calendar — edit it on the source server.';
      return;
    }
    editorMode = 'edit';
    editorEventId = ev.id;
    editorCalendarId = ev.calendar_id;
    editorForm = {
      summary: ev.summary ?? '',
      description: ev.description ?? '',
      location: ev.location ?? '',
      dtstart: unixToLocalDatetime(ev.dtstart_utc),
      dtend: ev.dtend_utc ? unixToLocalDatetime(ev.dtend_utc) : '',
      all_day: ev.all_day,
      repeat: repeatFromRrule(ev.rrule)
    };
    editorOpen = true;
    // Close the read-only detail view if it was open.
    detailEvent = null;
  }

  function closeEditor() {
    editorOpen = false;
    editorEventId = null;
    editorCalendarId = null;
  }

  async function saveEditor() {
    if (!editorForm.summary.trim()) {
      err = 'Give the event a title.';
      return;
    }
    const dtstart_utc = localDatetimeToUnix(editorForm.dtstart);
    const dtend_utc = editorForm.dtend
      ? localDatetimeToUnix(editorForm.dtend)
      : null;
    if (dtend_utc != null && dtend_utc < dtstart_utc) {
      err = 'End time can\'t be before start time.';
      return;
    }
    const rrule = rruleFromRepeat(editorForm.repeat);
    editorBusy = true;
    err = null;
    try {
      if (editorMode === 'create') {
        const body: NewLocalEvent = {
          calendar_id: editorCalendarId ?? undefined,
          summary: editorForm.summary.trim(),
          description: editorForm.description.trim() || null,
          location: editorForm.location.trim() || null,
          dtstart_utc,
          dtend_utc,
          all_day: editorForm.all_day,
          rrule
        };
        await api.calCreateEvent(body);
      } else if (editorEventId != null) {
        const patch: PatchLocalEvent = {
          summary: editorForm.summary.trim(),
          description: editorForm.description.trim() || null,
          location: editorForm.location.trim() || null,
          dtstart_utc,
          dtend_utc,
          all_day: editorForm.all_day,
          rrule
        };
        await api.calUpdateEvent(editorEventId, patch);
      }
      closeEditor();
      await loadEvents();
    } catch (e) {
      err = e instanceof Error ? e.message : String(e);
    } finally {
      editorBusy = false;
    }
  }

  async function deleteFromEditor() {
    if (editorEventId == null) return;
    const ok = confirm('Delete this event? This cannot be undone.');
    if (!ok) return;
    editorBusy = true;
    err = null;
    try {
      await api.calDeleteEvent(editorEventId);
      closeEditor();
      await loadEvents();
    } catch (e) {
      err = e instanceof Error ? e.message : String(e);
    } finally {
      editorBusy = false;
    }
  }

  function gotoPrevMonth() {
    const d = new Date(anchor);
    d.setDate(1);
    d.setMonth(d.getMonth() - 1);
    anchor = d;
  }
  function gotoNextMonth() {
    const d = new Date(anchor);
    d.setDate(1);
    d.setMonth(d.getMonth() + 1);
    anchor = d;
  }
  function gotoToday() {
    anchor = startOfMonth(new Date());
    selectedDay = null;
    view = 'month';
  }

  // --- Date helpers (local zone — calendar views are user-facing) ---

  function startOfMonth(d: Date): Date {
    const x = new Date(d);
    x.setDate(1);
    x.setHours(0, 0, 0, 0);
    return x;
  }

  /** Sunday-anchored week containing the month's first day. */
  function startOfGrid(d: Date): Date {
    const first = startOfMonth(d);
    const weekday = first.getDay();
    const x = new Date(first);
    x.setDate(first.getDate() - weekday);
    return x;
  }

  /** 6-week grid end (exclusive-ish — last day at 23:59). */
  function endOfGrid(d: Date): Date {
    const start = startOfGrid(d);
    const x = new Date(start);
    x.setDate(start.getDate() + 42);
    x.setHours(23, 59, 59, 999);
    return x;
  }

  function sameDay(a: Date, b: Date): boolean {
    return (
      a.getFullYear() === b.getFullYear() &&
      a.getMonth() === b.getMonth() &&
      a.getDate() === b.getDate()
    );
  }

  function dayEvents(day: Date): EventOccurrence[] {
    const ms0 = new Date(day).setHours(0, 0, 0, 0);
    const ms1 = ms0 + 24 * 3600 * 1000 - 1;
    const s0 = Math.floor(ms0 / 1000);
    const s1 = Math.floor(ms1 / 1000);
    return events
      .filter((e) => {
        const end = e.dtend_utc ?? e.dtstart_utc + 3600;
        return e.dtstart_utc <= s1 && end >= s0;
      })
      .sort((a, b) => a.dtstart_utc - b.dtstart_utc);
  }

  function dayReminders(day: Date): Reminder[] {
    const ms0 = new Date(day).setHours(0, 0, 0, 0);
    const ms1 = ms0 + 24 * 3600 * 1000 - 1;
    const s0 = Math.floor(ms0 / 1000);
    const s1 = Math.floor(ms1 / 1000);
    return reminders
      .filter((r) => r.due_at_utc >= s0 && r.due_at_utc <= s1)
      .sort((a, b) => a.due_at_utc - b.due_at_utc);
  }

  function formatTime(unix: number): string {
    return new Date(unix * 1000).toLocaleTimeString([], {
      hour: 'numeric',
      minute: '2-digit'
    });
  }

  function formatDate(unix: number): string {
    return new Date(unix * 1000).toLocaleString([], {
      weekday: 'short',
      month: 'short',
      day: 'numeric',
      hour: 'numeric',
      minute: '2-digit'
    });
  }

  // Pre-compute the month grid cells so the template stays clean.
  let gridDays = $derived.by(() => {
    const days: Date[] = [];
    const start = startOfGrid(anchor);
    for (let i = 0; i < 42; i++) {
      const d = new Date(start);
      d.setDate(start.getDate() + i);
      days.push(d);
    }
    return days;
  });

  let today = $state(new Date());
  onMount(() => {
    const t = setInterval(() => (today = new Date()), 60_000);
    return () => clearInterval(t);
  });

  let monthLabel = $derived(
    anchor.toLocaleString([], { month: 'long', year: 'numeric' })
  );
  const WEEKDAYS = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'];
</script>

<article class="cal-shell">
  <div class="page-top">
    <a class="back" href="/inbox">← Inbox</a>
  </div>

  <header class="hero">
    <div class="hero-copy">
      <span class="eyebrow">Calendars</span>
      <h1>{monthLabel}</h1>
      <p>
        Local-first calendar — your events live in the encrypted vault on
        this device. Optionally sync read-only with a CalDAV server (Nextcloud,
        iCloud, Fastmail, Radicale) from Settings → Calendars.
      </p>
    </div>
    <div class="controls">
      <button class="btn primary" onclick={() => openCreateEditor(selectedDay ?? today)}>+ New event</button>
      <button class="btn" onclick={gotoPrevMonth}>‹</button>
      <button class="btn" onclick={gotoToday}>Today</button>
      <button class="btn" onclick={gotoNextMonth}>›</button>
      <div class="seg">
        <button class="btn" class:active={view === 'month'} onclick={() => (view = 'month')}>Month</button>
        <button class="btn" class:active={view === 'day'} onclick={() => { view = 'day'; selectedDay ??= new Date(); }}>Day</button>
      </div>
    </div>
  </header>

  {#if err}
    <p class="err">⚠ {err}</p>
  {/if}

  {#if accounts.length === 0}
    <div class="empty">
      <h2>Setting up your local calendar…</h2>
      <p>One moment — provisioning the on-device calendar.</p>
    </div>
  {:else}
    {#if accounts.some((a) => a.kind === 'caldav')}
      <div class="sync-strip">
        {#each accounts.filter((a) => a.kind === 'caldav') as a (a.id)}
          <span class="account-pill">
            <strong>{a.label}</strong>
            {#if a.last_sync_error}
              <span class="err-pill" title={a.last_sync_error}>sync err</span>
            {:else if a.last_sync_at}
              <span class="muted">synced {new Date(a.last_sync_at * 1000).toLocaleTimeString()}</span>
            {:else}
              <span class="muted">awaiting first sync</span>
            {/if}
            <button
              class="btn small"
              disabled={syncBusy[a.id]}
              onclick={() => syncAccount(a.id)}
            >{syncBusy[a.id] ? '…' : 'Sync'}</button>
          </span>
        {/each}
      </div>
    {/if}

    {#if view === 'month'}
      <div class="grid">
        <div class="weekdays">
          {#each WEEKDAYS as w (w)}<span>{w}</span>{/each}
        </div>
        <div class="cells">
          {#each gridDays as day, idx (idx)}
            {@const inMonth = day.getMonth() === anchor.getMonth()}
            {@const isToday = sameDay(day, today)}
            {@const evs = dayEvents(day)}
            {@const rems = dayReminders(day)}
            {@const totalItems = evs.length + rems.length}
            {@const evShown = evs.slice(0, Math.min(evs.length, 3))}
            {@const remShown = rems.slice(0, Math.max(0, 3 - evShown.length))}
            <button
              class="cell"
              class:other-month={!inMonth}
              class:today={isToday}
              onclick={() => { selectedDay = day; view = 'day'; }}
            >
              <span class="day-num">{day.getDate()}</span>
              <div class="chips">
                {#each evShown as ev (ev.id + '-' + ev.occurrence_index)}
                  <span class="chip" title={ev.summary ?? '(no title)'}>
                    {#if !ev.all_day}<span class="chip-time">{formatTime(ev.dtstart_utc)}</span>{/if}
                    <span class="chip-text">{ev.summary ?? '(no title)'}</span>
                  </span>
                {/each}
                {#each remShown as r (r.id)}
                  <span
                    class="chip reminder"
                    class:done={r.done}
                    title={r.title}
                  >
                    <span class="chip-time">⏰ {formatTime(r.due_at_utc)}</span>
                    <span class="chip-text">{r.title}</span>
                  </span>
                {/each}
                {#if totalItems > 3}
                  <span class="chip more">+{totalItems - 3} more</span>
                {/if}
              </div>
            </button>
          {/each}
        </div>
      </div>
    {:else if view === 'day' && selectedDay}
      {@const todaysEvents = dayEvents(selectedDay)}
      {@const todaysReminders = dayReminders(selectedDay)}
      <div class="day-view">
        <div class="day-head">
          <h2>{selectedDay.toLocaleDateString([], { weekday: 'long', month: 'long', day: 'numeric' })}</h2>
          <button class="btn small" onclick={() => openCreateEditor(selectedDay!)}>+ Event</button>
        </div>
        <ul class="day-list">
          {#each todaysEvents as ev (ev.id + '-' + ev.occurrence_index)}
            <li>
              <button class="event-row" onclick={() => openEventDetail(ev)}>
                <span class="time">
                  {#if ev.all_day}
                    All day
                  {:else}
                    {formatTime(ev.dtstart_utc)}
                    {#if ev.dtend_utc}–{formatTime(ev.dtend_utc)}{/if}
                  {/if}
                </span>
                <span class="title">
                  {ev.summary ?? '(no title)'}
                  {#if ev.is_recurring}<span class="badge">↻</span>{/if}
                </span>
                {#if ev.location}<span class="loc">{ev.location}</span>{/if}
              </button>
            </li>
          {/each}
          {#each todaysReminders as r (r.id)}
            <li>
              <a class="event-row reminder-row" class:done={r.done} href="/reminders">
                <span class="time">⏰ {formatTime(r.due_at_utc)}</span>
                <span class="title">
                  {r.title}
                  {#if r.repeat !== 'none'}<span class="badge">↻</span>{/if}
                  {#if r.done}<span class="badge done-tag">Done</span>{/if}
                </span>
                {#if r.notes}<span class="loc">{r.notes}</span>{/if}
              </a>
            </li>
          {/each}
          {#if todaysEvents.length === 0 && todaysReminders.length === 0}
            <li class="empty-day">Nothing scheduled.</li>
          {/if}
        </ul>
      </div>
    {/if}
  {/if}

  {#if detailEvent}
    <div
      class="detail-backdrop"
      role="presentation"
      onclick={() => (detailEvent = null)}
      onkeydown={(e) => { if (e.key === 'Escape') detailEvent = null; }}
    >
      <div
        class="detail"
        role="dialog"
        aria-modal="true"
        onclick={(e) => e.stopPropagation()}
      >
        <header>
          <h3>{detailEvent.summary ?? '(no title)'}</h3>
          <div class="detail-actions">
            {#if calendarKind[detailEvent.calendar_id] === 'local'}
              <button class="btn small" onclick={() => openEditEditor(detailEvent!)}>Edit</button>
            {/if}
            <button class="btn ghost" onclick={() => (detailEvent = null)}>Close</button>
          </div>
        </header>
        <dl>
          <dt>Starts</dt>
          <dd>{formatDate(detailEvent.dtstart_utc)}</dd>
          {#if detailEvent.dtend_utc}
            <dt>Ends</dt>
            <dd>{formatDate(detailEvent.dtend_utc)}</dd>
          {/if}
          {#if detailEvent.location}
            <dt>Location</dt>
            <dd>{detailEvent.location}</dd>
          {/if}
          {#if detailEvent.rrule}
            <dt>Recurs</dt>
            <dd><code>{detailEvent.rrule}</code></dd>
          {/if}
        </dl>
        {#if detailEvent.description}
          <pre class="description">{detailEvent.description}</pre>
        {/if}
      </div>
    </div>
  {/if}

  {#if editorOpen}
    <div
      class="detail-backdrop"
      role="presentation"
      onclick={closeEditor}
      onkeydown={(e) => { if (e.key === 'Escape') closeEditor(); }}
    >
      <div
        class="detail editor"
        role="dialog"
        aria-modal="true"
        aria-label={editorMode === 'create' ? 'New event' : 'Edit event'}
        onclick={(e) => e.stopPropagation()}
      >
        <header>
          <h3>{editorMode === 'create' ? 'New event' : 'Edit event'}</h3>
          <button class="btn ghost" onclick={closeEditor}>Close</button>
        </header>
        <form
          onsubmit={(e) => {
            e.preventDefault();
            void saveEditor();
          }}
        >
          <label class="field">
            <span>Title</span>
            <input
              type="text"
              bind:value={editorForm.summary}
              placeholder="What's happening?"
              autocomplete="off"
              autofocus
            />
          </label>

          <label class="field inline">
            <input type="checkbox" bind:checked={editorForm.all_day} />
            <span>All day</span>
          </label>

          <div class="field-row">
            <label class="field">
              <span>Starts</span>
              <input
                type="datetime-local"
                bind:value={editorForm.dtstart}
                required
              />
            </label>
            <label class="field">
              <span>Ends</span>
              <input
                type="datetime-local"
                bind:value={editorForm.dtend}
              />
            </label>
          </div>

          <label class="field">
            <span>Repeats</span>
            <select bind:value={editorForm.repeat}>
              <option value="none">Doesn't repeat</option>
              <option value="daily">Daily</option>
              <option value="weekly">Weekly</option>
              <option value="monthly">Monthly</option>
              <option value="yearly">Yearly</option>
            </select>
          </label>

          <label class="field">
            <span>Location</span>
            <input
              type="text"
              bind:value={editorForm.location}
              placeholder="Optional"
              autocomplete="off"
            />
          </label>

          <label class="field">
            <span>Notes</span>
            <textarea
              bind:value={editorForm.description}
              rows="3"
              placeholder="Optional"
            ></textarea>
          </label>

          <footer class="editor-footer">
            {#if editorMode === 'edit'}
              <button
                type="button"
                class="btn danger"
                disabled={editorBusy}
                onclick={deleteFromEditor}
              >Delete</button>
            {/if}
            <span class="spacer"></span>
            <button type="button" class="btn ghost" onclick={closeEditor} disabled={editorBusy}>
              Cancel
            </button>
            <button type="submit" class="btn primary" disabled={editorBusy}>
              {editorBusy ? 'Saving…' : editorMode === 'create' ? 'Create' : 'Save'}
            </button>
          </footer>
        </form>
      </div>
    </div>
  {/if}

  {#if loading}<div class="loading">Loading events…</div>{/if}
</article>

<style>
  article.cal-shell {
    max-width: clamp(60rem, 94vw, 110rem);
    width: 100%;
    margin: 0 auto;
    padding: 0 0 2rem;
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

  .controls {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    align-self: start;
  }
  .seg { display: inline-flex; border: 1px solid var(--border); border-radius: 0.55rem; overflow: hidden; }
  .seg .btn { border: 0; border-radius: 0; padding: 0.4rem 0.75rem; background: transparent; }
  .seg .btn.active { background: color-mix(in oklab, var(--accent) 25%, var(--surface)); }

  .btn {
    padding: 0.4rem 0.75rem;
    border: 1px solid var(--border);
    background: var(--surface-2);
    color: var(--fg);
    border-radius: 0.45rem;
    cursor: pointer;
    font: inherit;
    font-size: 0.85rem;
  }
  .btn:hover:not(:disabled) { filter: brightness(0.97); }
  .btn:disabled { opacity: 0.55; cursor: progress; }
  .btn.ghost { background: transparent; }
  .btn.small { padding: 0.22rem 0.55rem; font-size: 0.78rem; }
  .btn.primary {
    background: var(--accent);
    color: var(--bg);
    border-color: transparent;
    font-weight: 600;
  }
  .btn.danger {
    background: transparent;
    border-color: color-mix(in oklab, crimson 45%, transparent);
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

  .sync-strip {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
    margin-bottom: 0.85rem;
  }
  .account-pill {
    display: inline-flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.35rem 0.7rem;
    border-radius: 999px;
    background: color-mix(in oklab, var(--accent) 8%, var(--surface));
    border: 1px solid var(--border);
    font-size: 0.82rem;
  }
  .err-pill {
    font-size: 0.7rem;
    color: color-mix(in oklab, crimson 70%, var(--fg) 30%);
    border: 1px solid color-mix(in oklab, crimson 45%, transparent);
    padding: 0.05rem 0.35rem;
    border-radius: 0.3rem;
  }
  .muted { color: var(--muted); font-size: 0.78rem; }

  .grid {
    border: 1px solid var(--border);
    border-radius: 1rem;
    overflow: hidden;
    background: var(--surface);
  }
  .weekdays {
    display: grid;
    grid-template-columns: repeat(7, 1fr);
    padding: 0.4rem 0;
    background: color-mix(in oklab, var(--surface-2) 85%, transparent);
    border-bottom: 1px solid var(--border);
    font-size: 0.74rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--muted);
  }
  .weekdays span {
    text-align: center;
    font-weight: 600;
  }
  .cells {
    display: grid;
    grid-template-columns: repeat(7, 1fr);
    grid-auto-rows: minmax(5.5rem, 7.5rem);
  }
  .cell {
    border-right: 1px solid var(--border);
    border-bottom: 1px solid var(--border);
    padding: 0.4rem 0.5rem 0.5rem;
    text-align: left;
    background: var(--surface);
    cursor: pointer;
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    min-width: 0;
    color: inherit;
    font: inherit;
  }
  .cell:hover {
    background: color-mix(in oklab, var(--accent) 6%, var(--surface));
  }
  .cell.other-month {
    background: color-mix(in oklab, var(--surface-2) 50%, var(--surface));
    color: var(--muted);
  }
  .cell.today .day-num {
    background: var(--accent);
    color: white;
    border-radius: 999px;
    padding: 0.05rem 0.45rem;
  }
  .day-num { font-size: 0.85rem; font-weight: 600; align-self: flex-start; }

  .chips {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
    min-width: 0;
    overflow: hidden;
  }
  .chip {
    display: flex;
    align-items: baseline;
    gap: 0.3rem;
    padding: 0.08rem 0.35rem;
    border-radius: 0.35rem;
    background: color-mix(in oklab, var(--accent) 18%, var(--surface));
    font-size: 0.74rem;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .chip.more {
    background: transparent;
    color: var(--muted);
    padding: 0.08rem 0.2rem;
  }
  .chip.reminder {
    background: color-mix(in oklab, var(--accent) 8%, var(--surface-2));
    border-left: 2px solid var(--accent);
    padding-left: 0.3rem;
  }
  .chip.reminder.done {
    opacity: 0.55;
    text-decoration: line-through;
  }
  .chip-time {
    color: color-mix(in oklab, var(--accent) 70%, var(--fg) 30%);
    font-variant-numeric: tabular-nums;
    font-size: 0.7rem;
    flex-shrink: 0;
  }
  .chip-text {
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
  }

  .day-view {
    margin-top: 0.75rem;
    padding: 1rem 1.25rem;
    border: 1px solid var(--border);
    border-radius: 1rem;
    background: var(--surface);
  }
  .day-view h2 { margin: 0; font-size: 1.15rem; }
  .day-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    margin-bottom: 0.75rem;
  }
  .day-list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 0.45rem; }
  .event-row {
    display: grid;
    grid-template-columns: 10rem 1fr auto;
    gap: 0.8rem;
    align-items: baseline;
    padding: 0.55rem 0.75rem;
    width: 100%;
    border: 1px solid var(--border);
    border-radius: 0.55rem;
    background: var(--surface);
    color: inherit;
    font: inherit;
    cursor: pointer;
    text-align: left;
  }
  .event-row:hover { background: color-mix(in oklab, var(--accent) 6%, var(--surface)); }
  .event-row.reminder-row {
    border-left: 3px solid var(--accent);
    text-decoration: none;
  }
  .event-row.reminder-row.done { opacity: 0.6; }
  .event-row.reminder-row.done .title { text-decoration: line-through; }
  .event-row .badge.done-tag {
    background: color-mix(in oklab, var(--accent) 20%, var(--surface-2));
    color: color-mix(in oklab, var(--accent) 80%, var(--fg) 20%);
    padding: 0.05rem 0.35rem;
    border-radius: 0.3rem;
    font-size: 0.7rem;
    margin-left: 0.4rem;
  }
  .event-row .time { color: var(--muted); font-variant-numeric: tabular-nums; font-size: 0.82rem; }
  .event-row .title { font-weight: 600; min-width: 0; overflow: hidden; text-overflow: ellipsis; }
  .event-row .badge {
    font-size: 0.75rem;
    color: color-mix(in oklab, var(--accent) 60%, var(--fg) 40%);
    margin-left: 0.35rem;
  }
  .event-row .loc { color: var(--muted); font-size: 0.8rem; text-align: right; }
  .empty-day { color: var(--muted); font-style: italic; padding: 0.75rem; }

  .detail-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.4);
    backdrop-filter: blur(3px);
    display: grid;
    place-items: center;
    z-index: 100;
    padding: 1rem;
  }
  .detail {
    max-width: 34rem;
    width: 100%;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 1rem;
    padding: 1.1rem 1.3rem 1.4rem;
    box-shadow: 0 24px 64px rgba(0, 0, 0, 0.28);
  }
  .detail header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 1rem;
    margin-bottom: 0.85rem;
  }
  .detail h3 { margin: 0; font-size: 1.1rem; }
  .detail-actions {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    flex-shrink: 0;
  }
  .editor form {
    display: flex;
    flex-direction: column;
    gap: 0.7rem;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    font-size: 0.82rem;
    color: var(--muted);
  }
  .field.inline {
    flex-direction: row;
    align-items: center;
    gap: 0.55rem;
    font-size: 0.85rem;
    color: var(--fg);
  }
  .field input[type='text'],
  .field input[type='datetime-local'],
  .field select,
  .field textarea {
    font: inherit;
    font-size: 0.92rem;
    padding: 0.55rem 0.65rem;
    border: 1px solid var(--border);
    background: color-mix(in oklab, var(--surface-2) 70%, transparent);
    color: var(--fg);
    border-radius: 0.55rem;
    box-sizing: border-box;
    width: 100%;
  }
  .field textarea { resize: vertical; }
  .field input:focus,
  .field select:focus,
  .field textarea:focus {
    outline: none;
    border-color: color-mix(in oklab, var(--accent) 45%, var(--border));
    box-shadow: 0 0 0 3px color-mix(in oklab, var(--accent) 15%, transparent);
  }
  .field-row {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0.7rem;
  }
  .editor-footer {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    margin-top: 0.4rem;
  }
  .editor-footer .spacer { flex: 1 1 auto; }
  @media (max-width: 600px) {
    .field-row { grid-template-columns: 1fr; }
  }
  .detail dl {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 0.3rem 0.85rem;
    font-size: 0.88rem;
    margin: 0 0 0.85rem;
  }
  .detail dt { color: var(--muted); font-weight: 500; }
  .detail dd { margin: 0; }
  .detail code {
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 0.82rem;
    background: color-mix(in oklab, currentColor 6%, transparent);
    padding: 0.05rem 0.35rem;
    border-radius: 0.25em;
  }
  .detail .description {
    margin: 0;
    white-space: pre-wrap;
    font: inherit;
    font-size: 0.88rem;
    color: var(--fg);
    background: var(--surface-2);
    padding: 0.75rem;
    border-radius: 0.55rem;
    line-height: 1.55;
  }

  .err {
    padding: 0.6rem 0.9rem;
    border-radius: 0.6rem;
    background: color-mix(in oklab, crimson 10%, var(--surface));
    color: color-mix(in oklab, crimson 70%, var(--fg) 30%);
    margin-bottom: 0.75rem;
  }
  .loading {
    margin-top: 0.5rem;
    color: var(--muted);
    font-size: 0.85rem;
  }

  @media (max-width: 900px) {
    article.cal-shell { padding: 0.75rem; }
    .hero { grid-template-columns: 1fr; }
    .controls { flex-wrap: wrap; }
    .cells { grid-auto-rows: minmax(4.5rem, 1fr); }
    .chips { font-size: 0.7rem; }
    .event-row { grid-template-columns: 6rem 1fr; }
    .event-row .loc { grid-column: 1 / -1; text-align: left; }
  }
</style>
