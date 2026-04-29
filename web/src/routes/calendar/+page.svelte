<script lang="ts">
  import { onMount } from 'svelte';
  import {
    api,
    type CalAccount,
    type EventOccurrence,
    type CalEvent,
    type Reminder
  } from '$lib/api';

  // Anchor date drives the month grid. Defaults to today; Prev/Next
  // reset to the 1st of the neighbouring month so the grid is stable
  // regardless of where within the month you navigate from.
  let anchor = $state<Date>(startOfMonth(new Date()));
  let view = $state<'month' | 'day'>('month');
  let selectedDay = $state<Date | null>(null);

  let accounts = $state<CalAccount[]>([]);
  let events = $state<EventOccurrence[]>([]);
  let reminders = $state<Reminder[]>([]);
  let loading = $state(true);
  let err = $state<string | null>(null);
  let syncBusy = $state<Record<number, boolean>>({});

  // Detail popover.
  let detailEvent = $state<CalEvent | null>(null);
  let detailBusy = $state(false);

  onMount(() => {
    void loadAccounts();
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
        Events pulled from your CalDAV accounts. Read-only for now — create
        and edit arrive in the next iteration.
      </p>
    </div>
    <div class="controls">
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
      <h2>No calendar accounts yet</h2>
      <p>
        Add one in <a href="/settings?tab=calendars">Settings → Calendars</a>
        to start syncing. Nextcloud, iCloud, Fastmail, Radicale and Baïkal
        all work with their app-password (no OAuth required).
      </p>
    </div>
  {:else}
    <div class="sync-strip">
      {#each accounts as a (a.id)}
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
        <h2>{selectedDay.toLocaleDateString([], { weekday: 'long', month: 'long', day: 'numeric' })}</h2>
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
          <button class="btn ghost" onclick={() => (detailEvent = null)}>Close</button>
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

  {#if loading}<div class="loading">Loading events…</div>{/if}
</article>

<style>
  article.cal-shell {
    max-width: 82rem;
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
  .day-view h2 { margin: 0 0 0.75rem; font-size: 1.15rem; }
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
