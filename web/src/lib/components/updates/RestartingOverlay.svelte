<script lang="ts">
  type RecoveryStatus = 'queued' | 'restarting' | 'recovered' | 'timeout';

  interface Props {
    open: boolean;
    status: RecoveryStatus;
  }
  let { open, status }: Props = $props();
</script>

{#if open}
  <div class="restart-overlay" role="alertdialog" aria-live="polite" aria-label="Update installing">
    <div class="restart-card">
      <div class="restart-spinner" aria-hidden="true">
        <svg viewBox="0 0 24 24" width="42" height="42" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <path d="M21 12a9 9 0 1 1-9-9" />
          <path d="M21 3v6h-6" />
        </svg>
      </div>
      <h3>Installing update</h3>
      <p class="restart-status">
        {#if status === 'queued'}
          Update queued — handing off to the host updater…
        {:else if status === 'restarting'}
          Container restarting. This usually takes 30–90 seconds.
        {:else if status === 'recovered'}
          New build is up. Reloading to the login page…
        {:else if status === 'timeout'}
          Update is taking longer than expected — reloading now. If the page errors, refresh manually.
        {/if}
      </p>
      <p class="restart-hint">
        You'll be returned to the login screen once the container is back. Keep this tab open.
      </p>
    </div>
  </div>
{/if}

<style>
  .restart-overlay {
    position: fixed;
    inset: 0;
    z-index: 9999;
    display: grid;
    place-items: center;
    background:
      radial-gradient(circle at 50% 35%, color-mix(in oklab, var(--accent) 22%, transparent), transparent 60%),
      color-mix(in oklab, var(--bg) 96%, black);
    backdrop-filter: blur(6px);
    animation: overlay-fade 240ms ease-out;
  }
  @keyframes overlay-fade {
    from { opacity: 0; }
    to   { opacity: 1; }
  }
  .restart-card {
    max-width: 28rem;
    padding: 2rem 2.2rem;
    border-radius: 1.2rem;
    background: var(--surface);
    border: 1px solid var(--border);
    box-shadow: 0 20px 60px rgba(0, 0, 0, 0.25);
    text-align: center;
  }
  .restart-card h3 {
    margin: 0.6rem 0 0.5rem;
    font-size: 1.2rem;
    font-weight: 650;
  }
  .restart-status {
    margin: 0 0 0.75rem;
    font-size: 0.95rem;
    color: var(--fg);
    line-height: 1.5;
    min-height: 1.5em;
  }
  .restart-hint {
    margin: 0;
    font-size: 0.8rem;
    color: var(--muted);
    line-height: 1.5;
  }
  .restart-spinner {
    color: var(--accent);
    animation: spin 1.4s linear infinite;
    transform-origin: center;
    display: inline-flex;
    margin-bottom: 0.4rem;
  }
  @keyframes spin {
    to { transform: rotate(360deg); }
  }
</style>
