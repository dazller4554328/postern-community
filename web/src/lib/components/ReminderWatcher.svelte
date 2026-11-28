<script lang="ts">
  // Polls /api/reminders/due every 30s and surfaces each fired reminder
  // as an actionable toast with Snooze (5m/1h/tomorrow) and Done
  // buttons. Complements NotificationHost which only handles new-mail.
  //
  // The backend marks each due row as `notified=1` when the poll
  // returns it, so re-opening a second tab won't re-fire the same
  // reminder. Snooze clears the notified flag so the next tick
  // re-surfaces it at the snoozed time.

  import { onMount } from 'svelte';
  import { api, type Reminder, type SnoozeUntil } from '$lib/api';
  import { notifyReminder } from '$lib/notifications';

  const POLL_MS = 30_000;

  let active = $state<Reminder[]>([]);
  let pollHandle: ReturnType<typeof setInterval> | null = null;

  async function poll() {
    let due: Reminder[];
    try {
      due = await api.remindersDue();
    } catch {
      // Vault locked, transient network, etc. Stay quiet — the next
      // tick will try again and the server's `notified=1` logic keeps
      // us idempotent even if a previous call half-succeeded.
      return;
    }
    for (const r of due) {
      notifyReminder({ id: r.id, title: r.title, notes: r.notes });
      active = [...active, r];
    }
  }

  async function snooze(id: number, until: SnoozeUntil) {
    try {
      await api.remindersSnooze(id, until);
    } catch {
      // Non-fatal; the toast still dismisses. Next poll will re-show
      // if the snooze didn't stick.
    }
    active = active.filter((r) => r.id !== id);
  }

  async function markDone(id: number) {
    try {
      await api.remindersMarkDone(id);
    } catch {
      /* ignore */
    }
    active = active.filter((r) => r.id !== id);
  }

  function dismiss(id: number) {
    // Dismissal without action keeps `notified=1` on the server, so
    // the reminder won't re-fire. The user can still see it on the
    // /reminders page.
    active = active.filter((r) => r.id !== id);
  }

  onMount(() => {
    void poll();
    pollHandle = setInterval(poll, POLL_MS);
    const onVisible = () => {
      if (document.visibilityState === 'visible') void poll();
    };
    document.addEventListener('visibilitychange', onVisible);
    return () => {
      if (pollHandle) clearInterval(pollHandle);
      document.removeEventListener('visibilitychange', onVisible);
    };
  });
</script>

{#if active.length > 0}
  <div class="stack" aria-live="polite" aria-label="Reminders">
    {#each active as r (r.id)}
      <div class="card">
        <div class="row-main">
          <span class="bell" aria-hidden="true">⏰</span>
          <div class="body">
            <strong class="title">{r.title}</strong>
            {#if r.notes}<span class="notes">{r.notes}</span>{/if}
          </div>
          <button class="close" aria-label="Dismiss" onclick={() => dismiss(r.id)}>×</button>
        </div>
        <div class="actions">
          <button onclick={() => snooze(r.id, '5m')}>Snooze 5m</button>
          <button onclick={() => snooze(r.id, '1h')}>Snooze 1h</button>
          <button onclick={() => snooze(r.id, 'tomorrow')}>Tomorrow</button>
          <button class="primary" onclick={() => markDone(r.id)}>Done</button>
        </div>
      </div>
    {/each}
  </div>
{/if}

<style>
  .stack {
    position: fixed;
    /* Sit below the new-mail stack in the same corner so they don't
       overlap when both fire at once. */
    top: calc(max(1rem, env(safe-area-inset-top)) + 6rem);
    right: 1rem;
    z-index: 1000;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    max-width: min(24rem, calc(100vw - 2rem));
  }

  .card {
    background: color-mix(in oklab, var(--surface) 94%, transparent);
    color: var(--fg);
    border: 1px solid var(--border);
    border-left: 3px solid var(--accent);
    border-radius: 0.65rem;
    padding: 0.75rem 0.85rem 0.65rem;
    box-shadow:
      0 10px 24px rgba(0, 0, 0, 0.22),
      0 2px 4px rgba(0, 0, 0, 0.06);
    backdrop-filter: blur(14px);
    animation: card-in 220ms cubic-bezier(0.16, 1, 0.3, 1);
  }

  .row-main {
    display: grid;
    grid-template-columns: auto 1fr auto;
    gap: 0.65rem;
    align-items: start;
  }
  .bell { font-size: 1.1rem; line-height: 1.2; }
  .body { display: flex; flex-direction: column; gap: 0.15rem; min-width: 0; }
  .title { font-size: 0.95rem; font-weight: 650; line-height: 1.2; }
  .notes {
    font-size: 0.82rem;
    color: var(--muted);
    overflow: hidden;
    text-overflow: ellipsis;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
  }
  .close {
    border: 0;
    background: transparent;
    color: var(--muted);
    font-size: 1rem;
    line-height: 1;
    cursor: pointer;
    padding: 0.1rem 0.35rem;
    border-radius: 0.3rem;
  }
  .close:hover { background: color-mix(in oklab, currentColor 10%, transparent); color: var(--fg); }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.35rem;
    margin-top: 0.55rem;
  }
  .actions button {
    padding: 0.3rem 0.6rem;
    border-radius: 0.4rem;
    border: 1px solid var(--border);
    background: var(--surface-2);
    color: var(--fg);
    font-size: 0.78rem;
    cursor: pointer;
    font: inherit;
    font-size: 0.78rem;
  }
  .actions button:hover { filter: brightness(0.96); }
  .actions button.primary {
    background: var(--accent);
    color: white;
    border-color: transparent;
    font-weight: 600;
  }

  @keyframes card-in {
    from { opacity: 0; transform: translateX(12px); }
    to { opacity: 1; transform: translateX(0); }
  }

  @media (max-width: 900px) {
    .stack {
      top: calc(max(0.5rem, env(safe-area-inset-top)) + 5rem);
      right: 0.5rem;
      left: 0.5rem;
      max-width: unset;
    }
  }
</style>
