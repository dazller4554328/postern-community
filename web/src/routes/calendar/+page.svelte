<script lang="ts">
  import './calendar.css';
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
  import EventDetailDialog from './_components/EventDetailDialog.svelte';
  import EventEditorDialog from './_components/EventEditorDialog.svelte';
  import MonthGrid from './_components/MonthGrid.svelte';
  import DayList from './_components/DayList.svelte';
  import {
    unixToLocalDatetime,
    localDatetimeToUnix,
    rruleFromRepeat,
    repeatFromRrule,
    type RepeatChoice,
  } from './_lib/recurrence';

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
    repeat: 'none' as RepeatChoice,
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
      <MonthGrid
        {gridDays}
        {anchor}
        {today}
        {dayEvents}
        {dayReminders}
        {sameDay}
        {formatTime}
        onSelectDay={(d) => { selectedDay = d; view = 'day'; }}
      />
    {:else if view === 'day' && selectedDay}
      <DayList
        {selectedDay}
        events={dayEvents(selectedDay)}
        reminders={dayReminders(selectedDay)}
        {formatTime}
        onCreateEvent={() => openCreateEditor(selectedDay!)}
        onOpenEvent={openEventDetail}
      />
    {/if}
  {/if}

  {#if detailEvent}
    <EventDetailDialog
      event={detailEvent}
      canEdit={calendarKind[detailEvent.calendar_id] === 'local'}
      {formatDate}
      onEdit={() => openEditEditor(detailEvent!)}
      onClose={() => (detailEvent = null)}
    />
  {/if}

  <EventEditorDialog
    open={editorOpen}
    mode={editorMode}
    busy={editorBusy}
    bind:form={editorForm}
    onClose={closeEditor}
    onSave={saveEditor}
    onDelete={deleteFromEditor}
  />

  {#if loading}<div class="loading">Loading events…</div>{/if}
</article>

