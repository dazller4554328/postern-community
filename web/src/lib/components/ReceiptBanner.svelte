<script lang="ts">
  import { api } from '$lib/api';

  interface Props {
    messageId: number;
    receiptTo: string;
  }

  let { messageId, receiptTo }: Props = $props();

  let phase = $state<'idle' | 'sending' | 'sent' | 'error'>('idle');
  let errorMsg = $state<string | null>(null);

  async function send() {
    if (phase === 'sending') return;
    phase = 'sending';
    errorMsg = null;
    try {
      await api.sendReadReceipt(messageId);
      phase = 'sent';
    } catch (e) {
      phase = 'error';
      errorMsg = e instanceof Error ? e.message : String(e);
    }
  }
</script>

<div class="receipt-banner" role="note" aria-live="polite">
  <div class="receipt-text">
    <strong>Read receipt requested.</strong>
    <span>The sender asked to be notified when you open this message ({receiptTo}). Postern never sends one automatically.</span>
  </div>
  <div class="receipt-actions">
    {#if phase === 'idle'}
      <button type="button" class="receipt-send" onclick={send}>Send receipt</button>
      <button type="button" class="receipt-ignore" onclick={() => (phase = 'sent')}>Ignore</button>
    {:else if phase === 'sending'}
      <span class="receipt-status">Sending…</span>
    {:else if phase === 'sent'}
      <span class="receipt-status done">Done — banner stays for this session.</span>
    {:else}
      <span class="receipt-status err">Failed: {errorMsg ?? 'unknown error'}</span>
      <button type="button" class="receipt-send" onclick={send}>Retry</button>
    {/if}
  </div>
</div>

<style>
  .receipt-banner {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 0.65rem 1rem;
    margin: 0.6rem 0 0;
    padding: 0.6rem 0.85rem;
    border: 1px solid color-mix(in oklab, dodgerblue 35%, transparent);
    border-radius: 0.6rem;
    background: color-mix(in oklab, dodgerblue 8%, var(--surface));
    font-size: 0.84rem;
  }
  .receipt-text {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
    min-width: 0;
    flex: 1 1 18rem;
  }
  .receipt-text span {
    color: var(--muted);
    font-size: 0.78rem;
  }
  .receipt-actions {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
  }
  .receipt-send,
  .receipt-ignore {
    font: inherit;
    font-size: 0.78rem;
    padding: 0.32rem 0.7rem;
    border-radius: 999px;
    cursor: pointer;
    border: 1px solid color-mix(in oklab, currentColor 22%, transparent);
    background: transparent;
    color: inherit;
  }
  .receipt-send {
    background: dodgerblue;
    border-color: dodgerblue;
    color: white;
    font-weight: 500;
  }
  .receipt-send:hover {
    background: color-mix(in oklab, dodgerblue 85%, black);
  }
  .receipt-ignore:hover {
    background: color-mix(in oklab, currentColor 7%, transparent);
  }
  .receipt-status {
    font-size: 0.78rem;
    color: var(--muted);
  }
  .receipt-status.done {
    color: forestgreen;
  }
  .receipt-status.err {
    color: #c83333;
  }
</style>
