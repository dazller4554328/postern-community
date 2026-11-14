<script lang="ts">
  import type { RepeatChoice } from '../_lib/recurrence';

  export interface EditorForm {
    summary: string;
    description: string;
    location: string;
    dtstart: string;
    dtend: string;
    all_day: boolean;
    repeat: RepeatChoice;
  }

  type EditorMode = 'create' | 'edit';

  interface Props {
    open: boolean;
    mode: EditorMode;
    busy: boolean;
    form: EditorForm;
    onClose: () => void;
    onSave: () => void | Promise<void>;
    onDelete: () => void | Promise<void>;
  }
  let {
    open,
    mode,
    busy,
    form = $bindable(),
    onClose,
    onSave,
    onDelete,
  }: Props = $props();
</script>

{#if open}
  <div
    class="detail-backdrop"
    role="presentation"
    onclick={onClose}
    onkeydown={(e) => { if (e.key === 'Escape') onClose(); }}
  >
    <div
      class="detail editor"
      role="dialog"
      aria-modal="true"
      aria-label={mode === 'create' ? 'New event' : 'Edit event'}
      onclick={(e) => e.stopPropagation()}
    >
      <header>
        <h3>{mode === 'create' ? 'New event' : 'Edit event'}</h3>
        <button class="btn ghost" onclick={onClose}>Close</button>
      </header>
      <form
        onsubmit={(e) => {
          e.preventDefault();
          void onSave();
        }}
      >
        <label class="field">
          <span>Title</span>
          <input
            type="text"
            bind:value={form.summary}
            placeholder="What's happening?"
            autocomplete="off"
            autofocus
          />
        </label>

        <label class="field inline">
          <input type="checkbox" bind:checked={form.all_day} />
          <span>All day</span>
        </label>

        <div class="field-row">
          <label class="field">
            <span>Starts</span>
            <input
              type="datetime-local"
              bind:value={form.dtstart}
              required
            />
          </label>
          <label class="field">
            <span>Ends</span>
            <input
              type="datetime-local"
              bind:value={form.dtend}
            />
          </label>
        </div>

        <label class="field">
          <span>Repeats</span>
          <select bind:value={form.repeat}>
            <option value="none">Doesn't repeat</option>
            <option value="daily">Daily</option>
            <option value="weekly">Weekly</option>
            <option value="monthly">Monthly</option>
            <option value="yearly">Yearly</option>
          </select>
        </label>

        <label class="field">
          <span>Location</span>
          <input
            type="text"
            bind:value={form.location}
            placeholder="Optional"
            autocomplete="off"
          />
        </label>

        <label class="field">
          <span>Notes</span>
          <textarea
            bind:value={form.description}
            rows="3"
            placeholder="Optional"
          ></textarea>
        </label>

        <footer class="editor-footer">
          {#if mode === 'edit'}
            <button
              type="button"
              class="btn danger"
              disabled={busy}
              onclick={onDelete}
            >Delete</button>
          {/if}
          <span class="spacer"></span>
          <button type="button" class="btn ghost" onclick={onClose} disabled={busy}>
            Cancel
          </button>
          <button type="submit" class="btn primary" disabled={busy}>
            {busy ? 'Saving…' : mode === 'create' ? 'Create' : 'Save'}
          </button>
        </footer>
      </form>
    </div>
  </div>
{/if}

<style>
  .detail-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.4);
    backdrop-filter: blur(3px);
    display: grid;
    place-items: center;
    z-index: 100;
    padding: 1rem;
  }
  .detail {
    max-width: 34rem;
    width: 100%;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 1rem;
    padding: 1.1rem 1.3rem 1.4rem;
    box-shadow: 0 24px 64px rgba(0, 0, 0, 0.28);
  }
  .detail header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 1rem;
    margin-bottom: 0.85rem;
  }
  .detail h3 { margin: 0; font-size: 1.1rem; }
  .editor form {
    display: flex;
    flex-direction: column;
    gap: 0.7rem;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    font-size: 0.82rem;
    color: var(--muted);
  }
  .field.inline {
    flex-direction: row;
    align-items: center;
    gap: 0.55rem;
    font-size: 0.85rem;
    color: var(--fg);
  }
  .field input[type='text'],
  .field input[type='datetime-local'],
  .field select,
  .field textarea {
    font: inherit;
    font-size: 0.92rem;
    padding: 0.55rem 0.65rem;
    border: 1px solid var(--border);
    background: color-mix(in oklab, var(--surface-2) 70%, transparent);
    color: var(--fg);
    border-radius: 0.55rem;
    box-sizing: border-box;
    width: 100%;
  }
  .field textarea { resize: vertical; }
  .field input:focus,
  .field select:focus,
  .field textarea:focus {
    outline: none;
    border-color: color-mix(in oklab, var(--accent) 45%, var(--border));
    box-shadow: 0 0 0 3px color-mix(in oklab, var(--accent) 15%, transparent);
  }
  .field-row {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0.7rem;
  }
  .editor-footer {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    margin-top: 0.4rem;
  }
  .editor-footer .spacer { flex: 1 1 auto; }
  .btn {
    padding: 0.4rem 0.75rem;
    border: 1px solid var(--border);
    background: var(--surface-2);
    color: var(--fg);
    border-radius: 0.45rem;
    cursor: pointer;
    font: inherit;
    font-size: 0.85rem;
  }
  .btn:hover:not(:disabled) { filter: brightness(0.97); }
  .btn:disabled { opacity: 0.55; cursor: progress; }
  .btn.ghost { background: transparent; }
  .btn.primary {
    background: var(--accent);
    color: var(--bg);
    border-color: transparent;
    font-weight: 600;
  }
  .btn.danger {
    background: transparent;
    border-color: color-mix(in oklab, crimson 45%, transparent);
    color: color-mix(in oklab, crimson 70%, var(--fg) 30%);
  }
  @media (max-width: 600px) {
    .field-row { grid-template-columns: 1fr; }
  }
</style>
