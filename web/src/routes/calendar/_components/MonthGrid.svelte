<script lang="ts">
  import type { EventOccurrence, Reminder } from '$lib/api';

  const WEEKDAYS = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'];

  interface Props {
    gridDays: Date[];
    anchor: Date;
    today: Date;
    dayEvents: (d: Date) => EventOccurrence[];
    dayReminders: (d: Date) => Reminder[];
    sameDay: (a: Date, b: Date) => boolean;
    formatTime: (unix: number) => string;
    onSelectDay: (d: Date) => void;
  }
  let { gridDays, anchor, today, dayEvents, dayReminders, sameDay, formatTime, onSelectDay }: Props = $props();
</script>

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
        onclick={() => onSelectDay(day)}
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

<style>
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
  @media (max-width: 900px) {
    .cells { grid-auto-rows: minmax(4.5rem, 1fr); }
    .chips { font-size: 0.7rem; }
  }
</style>
