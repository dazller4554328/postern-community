<script lang="ts">
  interface Props {
    open: boolean;
    busy: boolean;
    commitLabel: string;
    releaseNotes: string | null;
    onClose: () => void;
    onConfirm: () => void;
  }
  let { open, busy, commitLabel, releaseNotes, onClose, onConfirm }: Props = $props();
</script>

{#if open}
  <div
    class="modal-backdrop"
    role="presentation"
    onclick={onClose}
    onkeydown={(e) => { if (e.key === 'Escape') onClose(); }}
  >
    <div
      class="modal"
      role="dialog"
      aria-modal="true"
      aria-labelledby="update-confirm-title"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
    >
      <h3 id="update-confirm-title">Install update to {commitLabel}?</h3>
      <p>
        Postern will download the new release, verify it, back up your database,
        then rebuild the container. The mail server will be unreachable for
        roughly 30–60 seconds while the new container starts.
      </p>
      {#if releaseNotes}
        <p class="notes small"><em>{releaseNotes}</em></p>
      {/if}
      <div class="modal-actions">
        <button class="btn" onclick={onClose} disabled={busy}>Cancel</button>
        <button class="btn primary" onclick={onConfirm} disabled={busy}>
          {busy ? 'Queuing…' : 'Install now'}
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .modal-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.45);
    backdrop-filter: blur(3px);
    display: grid;
    place-items: center;
    z-index: 200;
    padding: 1rem;
  }
  .modal {
    max-width: 34rem;
    width: 100%;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 0.9rem;
    padding: 1.2rem 1.4rem 1.3rem;
    box-shadow: 0 24px 64px rgba(0, 0, 0, 0.3);
  }
  .modal h3 { margin: 0 0 0.7rem; font-size: 1.05rem; }
  .modal p { margin: 0 0 0.7rem; color: var(--fg); line-height: 1.5; }
  .modal-actions { display: flex; justify-content: flex-end; gap: 0.55rem; margin-top: 0.5rem; }
  .notes { color: var(--muted); }
  .small { font-size: 0.85rem; }
  .btn {
    font: inherit;
    font-size: 0.84rem;
    padding: 0.45rem 0.95rem;
    border: 1px solid var(--border);
    background: var(--surface-2);
    color: var(--fg);
    border-radius: 0.55rem;
    cursor: pointer;
  }
  .btn:hover:not(:disabled) { filter: brightness(0.97); }
  .btn:disabled { opacity: 0.55; cursor: progress; }
  .btn.primary {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--bg);
    font-weight: 600;
  }
</style>
