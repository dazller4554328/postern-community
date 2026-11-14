<script lang="ts">
  import { onMount } from 'svelte';
  import { api, type BackupSchedule } from '$lib/api';

  // Loaded from the server on mount; kept around to compute the dirty
  // flag (do the draft fields differ from what's persisted?).
  let schedule = $state<BackupSchedule | null>(null);

  // The form-bound copy. Edits go here; saveSchedule pushes it back to
  // the server and replaces `schedule` with the returned canonical row.
  let scheduleDraft = $state<{
    enabled: boolean;
    frequency: 'daily' | 'weekly';
    hour: number;
    minute: number;
    day_of_week: number;
    retention_count: number;
  } | null>(null);

  let scheduleSaving = $state(false);
  let scheduleSaved = $state(false);

  const DOW_NAMES = [
    'Sunday', 'Monday', 'Tuesday', 'Wednesday',
    'Thursday', 'Friday', 'Saturday'
  ];

  function scheduleDirty(): boolean {
    if (!schedule || !scheduleDraft) return false;
    return (
      scheduleDraft.enabled !== schedule.enabled ||
      scheduleDraft.frequency !== schedule.frequency ||
      scheduleDraft.hour !== schedule.hour ||
      scheduleDraft.minute !== schedule.minute ||
      scheduleDraft.day_of_week !== schedule.day_of_week ||
      scheduleDraft.retention_count !== schedule.retention_count
    );
  }

  async function saveSchedule() {
    if (!scheduleDraft) return;
    scheduleSaving = true;
    try {
      schedule = await api.setBackupSchedule(scheduleDraft);
      scheduleDraft = { ...scheduleDraft }; // refresh from saved
      scheduleSaved = true;
      setTimeout(() => (scheduleSaved = false), 2000);
    } catch (e) {
      alert(e instanceof Error ? e.message : String(e));
    } finally {
      scheduleSaving = false;
    }
  }

  function pad2(n: number): string {
    return n.toString().padStart(2, '0');
  }

  onMount(async () => {
    try {
      schedule = await api.getBackupSchedule();
      scheduleDraft = {
        enabled: schedule.enabled,
        frequency: schedule.frequency,
        hour: schedule.hour,
        minute: schedule.minute,
        day_of_week: schedule.day_of_week,
        retention_count: schedule.retention_count
      };
    } catch (e) {
      console.warn('schedule load failed', e);
    }
  });
</script>

{#if scheduleDraft}
  <div class="schedule-section panel">
    <h3>Automatic backups</h3>
    <p class="muted">
      Postern fires a backup at the chosen time. Off-site destinations
      get pushed automatically afterwards. Retention prunes old local
      tarballs once the new one is written.
    </p>
    <div class="sched-grid">
      <label class="sched-field">
        <span class="sched-label">Frequency</span>
        <select bind:value={scheduleDraft.frequency}>
          <option value="daily">Daily</option>
          <option value="weekly">Weekly</option>
        </select>
      </label>
      {#if scheduleDraft.frequency === 'weekly'}
        <label class="sched-field">
          <span class="sched-label">Day</span>
          <select bind:value={scheduleDraft.day_of_week}>
            {#each DOW_NAMES as name, i}
              <option value={i}>{name}</option>
            {/each}
          </select>
        </label>
      {/if}
      <label class="sched-field">
        <span class="sched-label">Time <em>(server local)</em></span>
        <input
          type="time"
          value={`${pad2(scheduleDraft.hour)}:${pad2(scheduleDraft.minute)}`}
          oninput={(e) => {
            const [h, m] = (e.currentTarget as HTMLInputElement).value
              .split(':')
              .map(Number);
            if (!Number.isNaN(h) && !Number.isNaN(m) && scheduleDraft) {
              scheduleDraft = { ...scheduleDraft, hour: h, minute: m };
            }
          }}
        />
      </label>
      <label class="sched-field">
        <span class="sched-label">Keep latest <em>(backups)</em></span>
        <input
          type="number"
          min="0"
          max="365"
          bind:value={scheduleDraft.retention_count}
        />
      </label>
    </div>
    <div class="sched-status" class:on={scheduleDraft.enabled}>
      <label class="sched-toggle">
        <input
          type="checkbox"
          class="switch"
          bind:checked={scheduleDraft.enabled}
          aria-label="Enable scheduled backups"
        />
        <span class="track" aria-hidden="true">
          <span class="thumb"></span>
        </span>
      </label>
      <div class="sched-status-body">
        <strong>{scheduleDraft.enabled ? 'Scheduled backups on' : 'Scheduled backups off'}</strong>
        <span class="sched-status-detail">
          {#if scheduleDraft.enabled}
            Runs {scheduleDraft.frequency === 'weekly'
              ? `every ${DOW_NAMES[scheduleDraft.day_of_week]}`
              : 'every day'} at {pad2(scheduleDraft.hour)}:{pad2(scheduleDraft.minute)} server time, keeps the latest {scheduleDraft.retention_count} {scheduleDraft.retention_count === 1 ? 'backup' : 'backups'}.
          {:else}
            Only manual backups will run. Toggle on to enable an automatic schedule.
          {/if}
        </span>
      </div>
    </div>
    <div class="sched-actions">
      <button
        type="button"
        class="btn primary"
        disabled={!scheduleDirty() || scheduleSaving}
        onclick={saveSchedule}
      >
        {scheduleSaving ? 'Saving…' : 'Save schedule'}
      </button>
      {#if scheduleSaved}
        <span class="saved-flash">Saved ✓</span>
      {/if}
    </div>
  </div>
{/if}

<style>
  /* Shared design-system classes (.panel, .btn, .btn.primary, .muted)
     stay scoped to the parent under :global() so this child inherits
     them without duplicating. */
  .schedule-section { margin-bottom: 1rem; padding: 1.1rem 1.25rem 1rem; }
  .schedule-section h3 { margin: 0 0 0.4rem; font-size: 0.95rem; font-weight: 650; }
  .schedule-section p {
    font-size: 0.82rem;
    line-height: 1.5;
    margin: 0 0 0.85rem;
    color: var(--muted);
  }
  /* Schedule fields lay out as a responsive grid: each column is at
     least 9rem wide and grows to fill, so on a desktop the four
     fields sit on one row and on mobile they wrap two-per-row
     without overflow. */
  .sched-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(9rem, 1fr));
    gap: 0.75rem 1rem;
    align-items: start;
  }
  .sched-field {
    display: grid;
    grid-template-rows: auto auto;
    gap: 0.3rem;
    font-size: 0.78rem;
    font-weight: 500;
  }
  .sched-label {
    display: block;
    line-height: 1.2;
    color: color-mix(in oklab, currentColor 70%, transparent);
    font-weight: 600;
    letter-spacing: 0.005em;
  }
  .sched-label em {
    font-style: normal;
    font-weight: 500;
    color: color-mix(in oklab, currentColor 50%, transparent);
    margin-left: 0.25rem;
  }
  .sched-field input,
  .sched-field select {
    font: inherit;
    font-size: 0.9rem;
    padding: 0.5rem 0.6rem;
    border: 1px solid var(--border);
    border-radius: 0.5rem;
    background: var(--surface);
    color: inherit;
    width: 100%;
    box-sizing: border-box;
    height: 2.4rem;
  }
  .sched-field input:focus,
  .sched-field select:focus {
    outline: none;
    border-color: color-mix(in oklab, var(--accent) 40%, transparent);
    box-shadow: 0 0 0 3px color-mix(in oklab, var(--accent) 14%, transparent);
  }

  .sched-status {
    margin-top: 1rem;
    display: flex;
    align-items: center;
    gap: 0.85rem;
    padding: 0.8rem 1rem;
    background: color-mix(in oklab, currentColor 4%, transparent);
    border: 1px solid var(--border);
    border-radius: 0.7rem;
    transition: background-color 160ms ease, border-color 160ms ease;
  }
  .sched-status.on {
    background: color-mix(in oklab, var(--accent) 8%, transparent);
    border-color: color-mix(in oklab, var(--accent) 30%, var(--border));
  }
  .sched-status-body {
    display: flex;
    flex-direction: column;
    gap: 0.18rem;
    min-width: 0;
    flex: 1 1 auto;
  }
  .sched-status-body strong {
    font-size: 0.92rem;
    font-weight: 600;
  }
  .sched-status-detail {
    font-size: 0.8rem;
    line-height: 1.4;
    color: color-mix(in oklab, currentColor 65%, transparent);
  }

  /* Custom toggle switch. Native checkbox stays for a11y / form
     semantics but is positioned-absolutely off-screen behind the
     visible track. */
  .sched-toggle {
    position: relative;
    display: inline-flex;
    align-items: center;
    cursor: pointer;
    flex: 0 0 auto;
  }
  .sched-toggle .switch {
    position: absolute;
    opacity: 0;
    width: 100%;
    height: 100%;
    margin: 0;
    cursor: pointer;
  }
  .sched-toggle .track {
    width: 40px;
    height: 22px;
    border-radius: 999px;
    background: color-mix(in oklab, currentColor 20%, transparent);
    position: relative;
    transition: background-color 160ms ease;
    flex-shrink: 0;
  }
  .sched-toggle .thumb {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 18px;
    height: 18px;
    border-radius: 999px;
    background: var(--surface);
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.18);
    transition: transform 180ms cubic-bezier(0.2, 0.8, 0.2, 1);
  }
  .sched-toggle .switch:checked + .track {
    background: var(--accent);
  }
  .sched-toggle .switch:checked + .track .thumb {
    transform: translateX(18px);
  }
  .sched-toggle .switch:focus-visible + .track {
    box-shadow: 0 0 0 3px color-mix(in oklab, var(--accent) 30%, transparent);
  }

  .sched-actions { display: flex; gap: 0.6rem; align-items: center; margin-top: 0.85rem; }
  .saved-flash { font-size: 0.78rem; color: var(--c-success, #2f9e6b); }
</style>
