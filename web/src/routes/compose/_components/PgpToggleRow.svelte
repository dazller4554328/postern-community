<script lang="ts">
  interface Props {
    pgpEncrypt: boolean;
    pgpSign: boolean;
    attachKey: boolean;
    requestReceipt: boolean;
    pgpChecking: boolean;
    pgpAutoDetected: boolean;
    pgpJustDiscovered: string[];
    pgpMissing: string[];
    /** Fired when the user turns encryption ON — the parent uses this
     *  as the explicit-intent signal to run WKD/keyserver discovery
     *  (which is never done on keystroke, to avoid leaking the typed
     *  address). */
    onEnableEncrypt?: () => void;
  }
  let {
    pgpEncrypt = $bindable(),
    pgpSign = $bindable(),
    attachKey = $bindable(),
    requestReceipt = $bindable(),
    pgpChecking,
    pgpAutoDetected,
    pgpJustDiscovered,
    pgpMissing,
    onEnableEncrypt,
  }: Props = $props();

  function toggleEncrypt() {
    pgpEncrypt = !pgpEncrypt;
    if (pgpEncrypt) onEnableEncrypt?.();
  }
</script>

<div class="pgp-toggles">
  <button
    type="button"
    class="pgp-icon"
    class:active={pgpEncrypt}
    onclick={toggleEncrypt}
    title={pgpEncrypt
      ? 'PGP encrypt ON — click to disable'
      : pgpAutoDetected
        ? 'All recipients have keys — PGP encryption available'
        : pgpMissing.length > 0
          ? `Missing keys for: ${pgpMissing.join(', ')}`
          : 'PGP encrypt OFF — click to enable (needs recipient public keys)'}
  >
    <svg viewBox="0 0 20 20" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
      <rect x="4.5" y="9" width="11" height="8" rx="1.2"/>
      <path d="M7 9V6.5a3 3 0 0 1 6 0V9"/>
    </svg>
  </button>
  {#if pgpChecking}
    <span class="pgp-status checking" title="Looking up recipient keys…">…</span>
  {:else if pgpJustDiscovered.length > 0 && pgpAutoDetected}
    <span
      class="pgp-status discovered"
      title="Auto-discovered via WKD: {pgpJustDiscovered.join(', ')}"
    >+wkd</span>
  {:else if pgpAutoDetected && pgpEncrypt}
    <span class="pgp-status ok" title="All recipients have public keys">auto</span>
  {:else if pgpMissing.length > 0}
    <span class="pgp-status warn" title="No PGP key found for: {pgpMissing.join(', ')}">!</span>
  {/if}
  <button
    type="button"
    class="pgp-icon"
    class:active={pgpSign}
    onclick={() => (pgpSign = !pgpSign)}
    title={pgpSign ? 'PGP sign ON — click to disable' : 'PGP sign OFF — click to enable (proves sender identity)'}
  >
    <svg viewBox="0 0 20 20" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
      <path d="M10 18s-6-3.5-6-8.5V4l6-2 6 2v5.5c0 5-6 8.5-6 8.5z"/>
      <path d="M7.5 10l2 2 3.5-4"/>
    </svg>
  </button>
  <button
    type="button"
    class="pgp-icon"
    class:active={attachKey}
    onclick={() => (attachKey = !attachKey)}
    title={attachKey ? 'Attach public key ON — recipient can encrypt replies to you' : 'Attach public key OFF — click to attach your public key'}
  >
    <svg viewBox="0 0 20 20" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
      <circle cx="8" cy="8" r="4"/>
      <path d="M11 11 17 17"/>
      <path d="M15 13v4h-4"/>
    </svg>
  </button>
  <button
    type="button"
    class="pgp-icon"
    class:active={requestReceipt}
    onclick={() => (requestReceipt = !requestReceipt)}
    title={requestReceipt
      ? 'Request read receipt ON — recipient is asked to confirm the message was opened'
      : 'Request read receipt OFF — click to ask recipient to confirm read'}
  >
    <svg viewBox="0 0 20 20" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
      <path d="M3.5 6 10 11l6.5-5"/>
      <rect x="3.5" y="5" width="13" height="10" rx="1.2"/>
      <path d="M6.5 16.5 9 14"/>
      <path d="M11 14l2.5 2.5"/>
    </svg>
  </button>
</div>

<style>
  .pgp-toggles {
    display: inline-flex;
    gap: 0.35rem;
  }
  .pgp-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 36px;
    height: 36px;
    border-radius: 0.85rem;
    border: 1px solid color-mix(in oklab, currentColor 14%, transparent);
    background: transparent;
    color: inherit;
    opacity: 0.45;
    cursor: pointer;
    transition: opacity 120ms, background 120ms, border-color 120ms;
  }
  .pgp-icon:hover {
    opacity: 0.8;
    background: color-mix(in oklab, currentColor 6%, transparent);
  }
  .pgp-icon.active {
    opacity: 1;
    background: color-mix(in oklab, forestgreen 18%, transparent);
    border-color: color-mix(in oklab, forestgreen 50%, transparent);
    color: forestgreen;
  }
  .pgp-status {
    font-size: 0.65rem;
    font-weight: 700;
    padding: 0.1rem 0.35rem;
    border-radius: 0.2rem;
    line-height: 1;
    align-self: center;
  }
  .pgp-status.ok {
    color: forestgreen;
    background: color-mix(in oklab, forestgreen 12%, transparent);
  }
  .pgp-status.warn {
    color: color-mix(in oklab, orange 80%, currentColor);
    background: color-mix(in oklab, orange 12%, transparent);
  }
  .pgp-status.checking {
    color: var(--accent);
    background: color-mix(in oklab, var(--accent) 10%, transparent);
    letter-spacing: 0.08em;
  }
  .pgp-status.discovered {
    color: var(--accent);
    background: color-mix(in oklab, var(--accent) 14%, transparent);
    border: 1px solid color-mix(in oklab, var(--accent) 40%, transparent);
  }

  @media (max-width: 900px) {
    .pgp-toggles {
      justify-content: flex-start;
    }
    .pgp-icon {
      width: 34px;
      height: 34px;
      border-radius: 0.75rem;
    }
  }
</style>
