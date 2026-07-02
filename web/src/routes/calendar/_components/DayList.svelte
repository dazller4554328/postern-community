<script lang="ts">
  import type { EventOccurrence, Reminder } from '$lib/api';

  interface Props {
    selectedDay: Date;
    events: EventOccurrence[];
    reminders: Reminder[];
    formatTime: (unix: number) => string;
    onCreateEvent: () => void;
    onOpenEvent: (ev: EventOccurrence) => void;
  }
  let { selectedDay, events, reminders, formatTime, onCreateEvent, onOpenEvent }: Props = $props();
</script>

<div class="day-view">
  <div class="day-head">
    <h2>{selectedDay.toLocaleDateString([], { weekday: 'long', month: 'long', day: 'numeric' })}</h2>
    <button class="btn small" onclick={onCreateEvent}>+ Event</button>
  </div>
  <ul class="day-list">
    {#each events as ev (ev.id + '-' + ev.occurrence_index)}
      <li>
        <button class="event-row" onclick={() => onOpenEvent(ev)}>
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
    {#each reminders as r (r.id)}
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
    {#if events.length === 0 && reminders.length === 0}
      <li class="empty-day">Nothing scheduled.</li>
    {/if}
  </ul>
</div>

<style>
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
  .btn.small { padding: 0.22rem 0.55rem; font-size: 0.78rem; }
  @media (max-width: 900px) {
    .event-row { grid-template-columns: 6rem 1fr; }
    .event-row .loc { grid-column: 1 / -1; text-align: left; }
  }
</style>
