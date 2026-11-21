<script lang="ts">
  import { prefs } from '$lib/prefs';

  type SendChoice = 'now' | 'in5m' | 'in30m' | 'in1h' | 'tomorrow9' | 'custom';

  interface Props {
    /// Bidirectional — the parent reads it in its submit handler
    /// (resolveScheduledAt branches on this) and the dropdown writes
    /// it when the user picks an option.
    sendChoice?: SendChoice;
    /// Datetime-local input value, bidirectional for the same reason.
    customScheduledAt?: string;
    /// True while the parent's submit handler is enqueueing — disables
    /// the button to prevent double-submit.
    sending: boolean;
  }

  let {
    sendChoice = $bindable<SendChoice>('now'),
    customScheduledAt = $bindable<string>(''),
    sending
  }: Props = $props();

  let showMenu = $state(false);

  // The Send button label changes based on the picked option so the
  // user knows what'll happen on click (Mail.app's pattern).
  let label = $derived.by(() => {
    if (sending) return 'Queueing…';
    if (sendChoice === 'now') {
      return $prefs.sendUndoSecs > 0 ? `Send (undo ${$prefs.sendUndoSecs}s)` : 'Send';
    }
    if (sendChoice === 'in5m') return 'Send in 5 min';
    if (sendChoice === 'in30m') return 'Send in 30 min';
    if (sendChoice === 'in1h') return 'Send in 1 h';
    if (sendChoice === 'tomorrow9') return 'Send tomorrow 9 AM';
    return 'Send at custom time';
  });

  function pick(choice: SendChoice) {
    sendChoice = choice;
    showMenu = false;
  }
</script>

<div class="schedule-group">
  <button type="submit" class="primary" disabled={sending}>
    <svg viewBox="0 0 20 20" width="16" height="16" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
      <path d="m2 10 16-7.5L13 14l-3-3.5L2 10Z"/>
      <path d="M10 10.5 18 2.5"/>
    </svg>
    {label}
  </button>
  <button
    type="button"
    class="schedule-toggle"
    title="Schedule send"
    aria-label="Schedule options"
    onclick={() => (showMenu = !showMenu)}
  >▾</button>
  {#if showMenu}
    <div class="schedule-menu" role="menu">
      <button type="button" role="menuitemradio" aria-checked={sendChoice === 'now'} onclick={() => pick('now')}>
        Send now
        {#if $prefs.sendUndoSecs > 0}<span class="hint">{$prefs.sendUndoSecs}s undo window</span>{/if}
      </button>
      <button type="button" role="menuitemradio" aria-checked={sendChoice === 'in5m'} onclick={() => pick('in5m')}>In 5 minutes</button>
      <button type="button" role="menuitemradio" aria-checked={sendChoice === 'in30m'} onclick={() => pick('in30m')}>In 30 minutes</button>
      <button type="button" role="menuitemradio" aria-checked={sendChoice === 'in1h'} onclick={() => pick('in1h')}>In 1 hour</button>
      <button type="button" role="menuitemradio" aria-checked={sendChoice === 'tomorrow9'} onclick={() => pick('tomorrow9')}>Tomorrow at 9 AM</button>
      <label class="schedule-custom">
        <span>Custom:</span>
        <input
          type="datetime-local"
          bind:value={customScheduledAt}
          onchange={() => (sendChoice = 'custom')}
        />
      </label>
    </div>
  {/if}
</div>

<style>
  /* The primary submit button styling matches the parent's
     .bar-actions button.primary — it's duplicated here rather than
     :global'd because the look is fundamental to the send action
     and worth carrying with the component. */
  .schedule-group {
    position: relative;
    display: inline-flex;
    align-items: stretch;
  }
  .schedule-group .primary {
    font: inherit;
    font-size: 0.85rem;
    padding: 0.5rem 1.15rem;
    border-radius: 999px;
    cursor: pointer;
    border: 1px solid dodgerblue;
    display: inline-flex;
    align-items: center;
    gap: 0.5rem;
    background: dodgerblue;
    color: white;
    font-weight: 500;
    /* Right side flat so it merges with the ▾ toggle. */
    border-top-right-radius: 0;
    border-bottom-right-radius: 0;
  }
  .schedule-group .primary:hover:not(:disabled) {
    background: color-mix(in oklab, dodgerblue 85%, black);
  }
  .schedule-group .primary:disabled { opacity: 0.55; cursor: progress; }

  .schedule-toggle {
    padding: 0 0.55rem;
    border: 1px solid var(--border);
    border-left: 0;
    border-top-right-radius: 0.55rem;
    border-bottom-right-radius: 0.55rem;
    background: color-mix(in oklab, var(--accent) 18%, var(--surface));
    color: var(--fg);
    cursor: pointer;
    font-size: 0.8rem;
  }
  .schedule-toggle:hover {
    background: color-mix(in oklab, var(--accent) 28%, var(--surface));
  }
  .schedule-menu {
    position: absolute;
    right: 0;
    bottom: calc(100% + 0.4rem);
    min-width: 15rem;
    border: 1px solid var(--border);
    background: var(--surface);
    border-radius: 0.7rem;
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.2);
    display: flex;
    flex-direction: column;
    padding: 0.3rem;
    gap: 0.1rem;
    z-index: 50;
  }
  .schedule-menu button {
    text-align: left;
    background: transparent;
    border: 0;
    padding: 0.5rem 0.7rem;
    border-radius: 0.45rem;
    cursor: pointer;
    color: var(--fg);
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 0.6rem;
    font-size: 0.88rem;
  }
  .schedule-menu button:hover {
    background: color-mix(in oklab, var(--accent) 12%, transparent);
  }
  .schedule-menu button[aria-checked='true'] {
    background: color-mix(in oklab, var(--accent) 18%, transparent);
    font-weight: 600;
  }
  .schedule-menu .hint {
    font-size: 0.72rem;
    color: var(--muted);
    font-weight: 400;
  }
  .schedule-custom {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.45rem 0.7rem 0.2rem;
    border-top: 1px solid var(--border);
    margin-top: 0.15rem;
    font-size: 0.82rem;
  }
  .schedule-custom span {
    font-size: 0.8rem;
    color: var(--muted);
  }
  .schedule-custom input {
    flex: 1;
    font-size: 0.82rem;
  }
</style>
