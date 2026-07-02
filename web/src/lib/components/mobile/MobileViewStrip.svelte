<script lang="ts">
  type ViewKey = 'unified' | 'reminders' | 'calendar' | 'outbox' | 'contacts' | 'notes';

  interface Props {
    activeView: ViewKey | null;
    onSelect: (v: ViewKey) => void;
  }
  let { activeView, onSelect }: Props = $props();
</script>

<nav class="view-strip" aria-label="Switch view">
  <button
    class="view-chip"
    class:active={activeView === 'unified'}
    onclick={() => onSelect('unified')}
    aria-label="Unified inbox"
    title="Unified inbox"
  >
    <svg viewBox="0 0 24 24" width="22" height="22" aria-hidden="true">
      <path d="M3 6h18v12H3z" fill="currentColor" opacity="0.15" />
      <path d="M3 6h18v12H3z" fill="none" stroke="currentColor" stroke-width="2" stroke-linejoin="round" />
      <path d="m3.5 6.5 8.5 6.5L20.5 6.5" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" />
    </svg>
  </button>
  <button
    class="view-chip"
    class:active={activeView === 'reminders'}
    onclick={() => onSelect('reminders')}
    aria-label="Reminders"
    title="Reminders"
  >
    <svg viewBox="0 0 24 24" width="22" height="22" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
      <circle cx="12" cy="13" r="7.5" />
      <path d="M12 9v4l2.5 1.5" />
      <path d="M9 3.5h6M5 6l2-2M19 6l-2-2" />
    </svg>
  </button>
  <button
    class="view-chip"
    class:active={activeView === 'calendar'}
    onclick={() => onSelect('calendar')}
    aria-label="Calendar"
    title="Calendar"
  >
    <svg viewBox="0 0 24 24" width="22" height="22" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
      <rect x="3.5" y="5.5" width="17" height="14" rx="1.5" />
      <path d="M3.5 10h17M8 3.5v4M16 3.5v4" />
    </svg>
  </button>
  <button
    class="view-chip"
    class:active={activeView === 'outbox'}
    onclick={() => onSelect('outbox')}
    aria-label="Outbox"
    title="Outbox"
  >
    <svg viewBox="0 0 24 24" width="22" height="22" fill="none" stroke="currentColor" stroke-width="2" stroke-linejoin="round" stroke-linecap="round" aria-hidden="true">
      <path d="M4 4h12l4 4v12H4z" />
      <path d="M8 4v6h8V4M8 14h8" />
    </svg>
  </button>
  <button
    class="view-chip"
    class:active={activeView === 'contacts'}
    onclick={() => onSelect('contacts')}
    aria-label="Contacts"
    title="Contacts"
  >
    <svg viewBox="0 0 24 24" width="22" height="22" fill="none" stroke="currentColor" stroke-width="2" stroke-linejoin="round" stroke-linecap="round" aria-hidden="true">
      <circle cx="12" cy="9" r="3.5" />
      <path d="M5.5 19.5c0-3.6 2.9-6 6.5-6s6.5 2.4 6.5 6" />
    </svg>
  </button>
  <button
    class="view-chip"
    class:active={activeView === 'notes'}
    onclick={() => onSelect('notes')}
    aria-label="Notes"
    title="Notes"
  >
    <svg viewBox="0 0 24 24" width="22" height="22" fill="none" stroke="currentColor" stroke-width="2" stroke-linejoin="round" stroke-linecap="round" aria-hidden="true">
      <path d="M6 3.5h8L18.5 8v12.5h-12.5z" />
      <path d="M14 3.5V8h4.5" />
      <path d="M9 13h6M9 16h6M9 10h3" />
    </svg>
  </button>
</nav>

<style>
  .view-strip {
    flex: 0 0 auto;
    display: flex;
    align-items: stretch;
    justify-content: space-around;
    gap: 0.3rem;
    padding: 0.3rem 0.6rem 0.5rem;
    background: var(--surface);
    border-bottom: 1px solid var(--border);
    position: relative;
    z-index: 40;
    box-shadow: 0 1px 0 color-mix(in oklab, var(--border) 50%, transparent);
  }
  .view-chip {
    flex: 1 1 0;
    min-width: 0;
    height: 44px;
    display: inline-grid;
    place-items: center;
    background: transparent;
    border: 0;
    color: color-mix(in oklab, currentColor 55%, transparent);
    border-radius: 14px;
    cursor: pointer;
    touch-action: manipulation;
    transition: background-color 140ms ease, color 140ms ease;
  }
  .view-chip :global(svg) {
    width: 24px;
    height: 24px;
  }
  .view-chip:active {
    background: var(--row-hover);
    transform: scale(0.97);
  }
  .view-chip.active {
    color: var(--accent);
    background: color-mix(in oklab, var(--accent) 12%, transparent);
  }
</style>
