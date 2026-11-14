<script lang="ts">
  export interface ContactForm {
    address: string;
    display_name: string;
    notes: string;
    is_favorite: boolean;
  }

  interface Props {
    mode: 'add' | 'edit';
    open: boolean;
    busy: boolean;
    err: string | null;
    form: ContactForm;
    /** Edit mode shows the address as read-only meta text. */
    editingAddress?: string;
    onClose: () => void;
    onSave: () => void | Promise<void>;
  }
  let {
    mode,
    open,
    busy,
    err,
    form = $bindable(),
    editingAddress,
    onClose,
    onSave,
  }: Props = $props();

  let canSave = $derived(mode === 'add' ? form.address.includes('@') : true);
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
      aria-labelledby="contact-modal-title"
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
    >
      <h3 id="contact-modal-title">{mode === 'add' ? 'Add contact' : 'Edit contact'}</h3>
      {#if mode === 'add'}
        <p class="modal-sub muted-addr">
          Enter someone you haven't emailed yet. They'll merge with auto-collected
          rows once a message arrives or is sent.
        </p>
      {:else if editingAddress}
        <p class="modal-sub muted-addr">{editingAddress}</p>
      {/if}

      {#if mode === 'add'}
        <label class="modal-field">
          <span>Email address <em>(required)</em></span>
          <input type="email" bind:value={form.address} placeholder="alice@example.com" required />
        </label>
      {/if}

      <label class="modal-field">
        <span>Display name</span>
        <input
          type="text"
          bind:value={form.display_name}
          placeholder={mode === 'add' ? 'Alice Allen' : 'Joe Bloggs'}
        />
      </label>

      <label class="modal-field">
        <span>Notes</span>
        <textarea
          rows="3"
          bind:value={form.notes}
          placeholder={mode === 'edit' ? "Anything you'd like to remember about them" : ''}
        ></textarea>
      </label>

      <label class="modal-toggle">
        <input type="checkbox" bind:checked={form.is_favorite} />
        <span>{mode === 'add' ? 'Favourite' : 'Favourite — pin to the top of the list'}</span>
      </label>

      {#if err}
        <p class="err-bubble">⚠ {err}</p>
      {/if}
      <div class="modal-actions">
        <button type="button" class="btn" onclick={onClose} disabled={busy}>Cancel</button>
        <button type="button" class="btn primary" onclick={onSave} disabled={busy || !canSave}>
          {busy ? (mode === 'add' ? 'Adding…' : 'Saving…') : (mode === 'add' ? 'Add' : 'Save')}
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
    width: 100%;
    max-width: 30rem;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 1rem;
    padding: 1.3rem 1.5rem;
    box-shadow: 0 24px 60px rgba(0, 0, 0, 0.3);
    color: var(--fg);
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }
  .modal h3 {
    margin: 0;
    font-size: 1.05rem;
  }
  .modal-sub {
    margin: 0;
    font-size: 0.83rem;
    line-height: 1.5;
  }
  .muted-addr {
    color: color-mix(in oklab, currentColor 55%, transparent);
  }
  .modal-field {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    font-size: 0.78rem;
    font-weight: 500;
  }
  .modal-field em {
    font-style: normal;
    font-weight: 500;
    color: color-mix(in oklab, currentColor 50%, transparent);
  }
  .modal-field input,
  .modal-field textarea {
    font: inherit;
    font-size: 0.9rem;
    padding: 0.5rem 0.65rem;
    border: 1px solid var(--border);
    border-radius: 0.5rem;
    background: var(--surface);
    color: inherit;
    width: 100%;
    box-sizing: border-box;
    resize: vertical;
  }
  .modal-field input:focus,
  .modal-field textarea:focus {
    outline: none;
    border-color: color-mix(in oklab, var(--accent) 45%, transparent);
    box-shadow: 0 0 0 3px color-mix(in oklab, var(--accent) 18%, transparent);
  }
  .modal-toggle {
    display: inline-flex;
    align-items: center;
    gap: 0.5rem;
    font-size: 0.86rem;
  }
  .modal-actions {
    display: flex;
    justify-content: flex-end;
    gap: 0.5rem;
    margin-top: 0.4rem;
  }
  .btn {
    font: inherit;
    font-size: 0.85rem;
    padding: 0.5rem 1.1rem;
    border-radius: 999px;
    cursor: pointer;
    border: 1px solid color-mix(in oklab, currentColor 18%, transparent);
    background: transparent;
    color: inherit;
  }
  .btn.primary {
    background: var(--accent);
    border-color: var(--accent);
    color: white;
    font-weight: 500;
  }
  .btn.primary:hover:not(:disabled) {
    filter: brightness(0.94);
  }
  .btn:hover:not(:disabled) {
    background: color-mix(in oklab, currentColor 6%, transparent);
  }
  .btn:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }
  .err-bubble {
    margin: 0;
    padding: 0.55rem 0.85rem;
    background: color-mix(in oklab, tomato 12%, transparent);
    border-left: 2px solid tomato;
    border-radius: 0 0.5rem 0.5rem 0;
    font-size: 0.84rem;
  }
</style>
