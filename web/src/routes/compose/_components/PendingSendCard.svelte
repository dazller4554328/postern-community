<script lang="ts">
  import { goto } from '$app/navigation';

  interface PendingState {
    /// Seconds remaining until the worker dispatches. 0 = window passed.
    countdown: number;
    /// Unix-seconds dispatch time. Used to format "Dispatches at HH:MM"
    /// for scheduled (non-immediate) sends.
    scheduledAt: number;
    /// Frozen at submit so the recap shows who the message is going to,
    /// even after the input fields have been reset.
    draft: {
      to: string;
      cc: string;
      bcc: string;
      subject: string;
    };
  }

  interface Props {
    pending: PendingState;
    /// Drives the "Sent. Undo in Ns" vs "Scheduled. Dispatches at …"
    /// label split — they share the same card.
    sendChoice: 'now' | 'in5m' | 'in30m' | 'in1h' | 'tomorrow9' | 'custom';
    undoBusy: boolean;
    onUndo: () => void;
  }

  let { pending, sendChoice, undoBusy, onUndo }: Props = $props();
</script>

<div class="pending">
  {#if pending.countdown > 0 && sendChoice === 'now'}
    <div class="pending-head">
      <strong>Sent.</strong>
      <span class="pending-countdown">Undo in <strong>{pending.countdown}s</strong></span>
    </div>
    <p class="pending-hint">
      The message is held in the outbox. Click Undo to bring it
      back and edit, or ignore this banner to let it dispatch.
    </p>
  {:else if pending.countdown > 0}
    <div class="pending-head">
      <strong>Scheduled.</strong>
      <span class="pending-countdown">
        Dispatches {new Date(pending.scheduledAt * 1000).toLocaleString()}
      </span>
    </div>
    <p class="pending-hint">
      Sitting in the outbox until dispatch time. Manage or cancel
      from the <a href="/outbox">outbox page</a>.
    </p>
  {:else}
    <div class="pending-head">
      <strong>Dispatching…</strong>
      <span class="pending-countdown">Waiting on the worker</span>
    </div>
    <p class="pending-hint">
      The undo window has passed. Forensics appear below once SMTP
      confirms.
    </p>
  {/if}

  <!-- Recap so mistakes (wrong recipient, wrong subject) are visible
       while the undo window is still open. Full unsummarized recipient
       list — truncation would defeat the whole "catch the mistake"
       point. -->
  <dl class="recap">
    <dt>To</dt>
    <dd>{pending.draft.to || '(none)'}</dd>
    {#if pending.draft.cc.trim()}
      <dt>Cc</dt>
      <dd>{pending.draft.cc}</dd>
    {/if}
    {#if pending.draft.bcc.trim()}
      <dt>Bcc</dt>
      <dd>{pending.draft.bcc}</dd>
    {/if}
    <dt>Subject</dt>
    <dd class="recap-subject">{pending.draft.subject || '(no subject)'}</dd>
  </dl>

  <div class="pending-actions">
    <button
      class="ghost"
      onclick={onUndo}
      disabled={undoBusy || pending.countdown === 0}
    >{undoBusy ? 'Undoing…' : 'Undo'}</button>
    <button class="ghost" onclick={() => goto('/outbox')}>View outbox</button>
  </div>
</div>

<style>
  .pending {
    padding: 1rem 1.25rem;
    border: 1px solid color-mix(in oklab, var(--accent) 40%, transparent);
    background: color-mix(in oklab, var(--accent) 7%, var(--surface));
    border-radius: 1rem;
    display: flex;
    flex-direction: column;
    gap: 0.55rem;
  }
  .pending-head {
    display: flex;
    align-items: baseline;
    gap: 0.9rem;
    flex-wrap: wrap;
  }
  .pending-head strong {
    font-size: 1.05rem;
  }
  .pending-countdown {
    font-variant-numeric: tabular-nums;
    color: var(--muted);
  }
  .pending-countdown strong {
    color: var(--fg);
  }
  .pending-hint {
    margin: 0;
    font-size: 0.88rem;
    color: var(--muted);
  }
  .pending-hint a {
    color: inherit;
    text-decoration: underline;
  }
  .pending-actions {
    display: flex;
    gap: 0.5rem;
    flex-wrap: wrap;
  }
  .pending-actions button.ghost {
    font: inherit;
    font-size: 0.86rem;
    padding: 0.45rem 0.95rem;
    border: 1px solid var(--border);
    background: transparent;
    color: inherit;
    border-radius: 999px;
    cursor: pointer;
  }
  .pending-actions button.ghost:hover:not(:disabled) {
    background: color-mix(in oklab, currentColor 6%, transparent);
  }
  .pending-actions button.ghost:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }

  /* Same recap layout the SendSuccessPanel uses. Both blocks show the
     same recipient verification info, so keep them visually identical. */
  .recap {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr);
    gap: 0.25rem 0.85rem;
    margin: 0.1rem 0 0;
    padding: 0.7rem 0.85rem;
    background: color-mix(in oklab, currentColor 4%, transparent);
    border: 1px solid color-mix(in oklab, currentColor 10%, transparent);
    border-radius: 0.55rem;
    font-size: 0.88rem;
  }
  .recap dt {
    color: var(--muted);
    font-weight: 500;
    font-size: 0.78rem;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    padding-top: 0.12rem;
  }
  .recap dd {
    margin: 0;
    min-width: 0;
    overflow-wrap: anywhere;
    word-break: break-word;
    color: var(--fg);
  }
  .recap-subject {
    font-weight: 600;
  }
</style>
