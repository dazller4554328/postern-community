<script lang="ts">
  import type { CalEvent } from '$lib/api';

  interface Props {
    event: CalEvent;
    canEdit: boolean;
    formatDate: (unix: number) => string;
    onEdit: () => void;
    onClose: () => void;
  }
  let { event, canEdit, formatDate, onEdit, onClose }: Props = $props();
</script>

<div
  class="detail-backdrop"
  role="presentation"
  onclick={onClose}
  onkeydown={(e) => { if (e.key === 'Escape') onClose(); }}
>
  <div
    class="detail"
    role="dialog"
    aria-modal="true"
    onclick={(e) => e.stopPropagation()}
  >
    <header>
      <h3>{event.summary ?? '(no title)'}</h3>
      <div class="detail-actions">
        {#if canEdit}
          <button class="btn small" onclick={onEdit}>Edit</button>
        {/if}
        <button class="btn ghost" onclick={onClose}>Close</button>
      </div>
    </header>
    <dl>
      <dt>Starts</dt>
      <dd>{formatDate(event.dtstart_utc)}</dd>
      {#if event.dtend_utc}
        <dt>Ends</dt>
        <dd>{formatDate(event.dtend_utc)}</dd>
      {/if}
      {#if event.location}
        <dt>Location</dt>
        <dd>{event.location}</dd>
      {/if}
      {#if event.rrule}
        <dt>Recurs</dt>
        <dd><code>{event.rrule}</code></dd>
      {/if}
    </dl>
    {#if event.description}
      <pre class="description">{event.description}</pre>
    {/if}
  </div>
</div>

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
  .detail-actions {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    flex-shrink: 0;
  }
  dl {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 0.3rem 0.85rem;
    font-size: 0.88rem;
    margin: 0 0 0.85rem;
  }
  dt { color: var(--muted); font-weight: 500; }
  dd { margin: 0; }
  code {
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 0.82rem;
    background: color-mix(in oklab, currentColor 6%, transparent);
    padding: 0.05rem 0.35rem;
    border-radius: 0.25em;
  }
  .description {
    margin: 0;
    white-space: pre-wrap;
    font: inherit;
    font-size: 0.88rem;
    color: var(--fg);
    background: var(--surface-2);
    padding: 0.75rem;
    border-radius: 0.55rem;
    line-height: 1.55;
  }
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
  .btn:hover { filter: brightness(0.97); }
  .btn.ghost { background: transparent; }
  .btn.small { padding: 0.22rem 0.55rem; font-size: 0.78rem; }
</style>
