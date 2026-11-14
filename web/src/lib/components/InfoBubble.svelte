<script lang="ts">
  // Small inline (?) button that reveals a tooltip on hover and on focus.
  // Keyboard users get the same explanation — click/focus both open it.
  interface Props {
    text: string;
    /// Optional long-form help link (opens in a new tab).
    href?: string | null;
  }

  let { text, href = null }: Props = $props();
  let open = $state(false);
  let btn: HTMLButtonElement | null = $state(null);

  function toggle() {
    open = !open;
  }

  function onOutside(event: MouseEvent) {
    if (!open) return;
    if (btn && !btn.contains(event.target as Node)) open = false;
  }
</script>

<svelte:window onclick={onOutside} />

<span class="info-wrap">
  <button
    bind:this={btn}
    type="button"
    class="info-btn"
    class:active={open}
    aria-label="Help"
    onclick={(e) => { e.stopPropagation(); toggle(); }}
    onmouseenter={() => (open = true)}
    onmouseleave={() => (open = false)}
    onfocus={() => (open = true)}
    onblur={() => (open = false)}
  >
    <svg viewBox="0 0 12 12" width="11" height="11" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
      <circle cx="6" cy="6" r="4.5"/>
      <path d="M6 5.5v2.2"/>
      <circle cx="6" cy="3.6" r="0.5" fill="currentColor" stroke="none"/>
    </svg>
  </button>
  {#if open}
    <span class="bubble" role="tooltip">
      {text}
      {#if href}
        <a {href} target="_blank" rel="noopener">Learn more ↗</a>
      {/if}
    </span>
  {/if}
</span>

<style>
  .info-wrap {
    position: relative;
    display: inline-flex;
    align-items: center;
    line-height: 1;
    margin-left: 0.35rem;
  }
  .info-btn {
    width: 1.1rem;
    height: 1.1rem;
    display: inline-grid;
    place-items: center;
    padding: 0;
    border: 1px solid color-mix(in oklab, currentColor 20%, transparent);
    background: color-mix(in oklab, currentColor 5%, transparent);
    color: inherit;
    border-radius: 999px;
    cursor: help;
    opacity: 0.6;
    transition: opacity 120ms, background 120ms, border-color 120ms;
  }
  .info-btn:hover,
  .info-btn.active,
  .info-btn:focus-visible {
    opacity: 1;
    background: color-mix(in oklab, var(--accent) 14%, transparent);
    border-color: color-mix(in oklab, var(--accent) 28%, transparent);
    outline: none;
  }
  .bubble {
    position: absolute;
    top: calc(100% + 0.45rem);
    left: 50%;
    transform: translateX(-50%);
    z-index: 30;
    width: max-content;
    max-width: 22rem;
    padding: 0.6rem 0.8rem;
    border: 1px solid var(--border);
    border-radius: 0.7rem;
    background: var(--surface);
    color: inherit;
    box-shadow: 0 12px 26px rgba(0, 0, 0, 0.12);
    font-size: 0.76rem;
    font-weight: 400;
    line-height: 1.5;
    white-space: normal;
    opacity: 0.97;
    pointer-events: auto;
  }
  .bubble::before {
    content: '';
    position: absolute;
    top: -5px;
    left: 50%;
    transform: translateX(-50%) rotate(45deg);
    width: 8px;
    height: 8px;
    background: var(--surface);
    border-top: 1px solid var(--border);
    border-left: 1px solid var(--border);
  }
  .bubble a {
    display: inline-block;
    margin-top: 0.4rem;
    color: inherit;
    opacity: 0.85;
    text-decoration: underline;
    text-decoration-thickness: 1px;
    text-underline-offset: 2px;
  }
  .bubble a:hover {
    opacity: 1;
  }
</style>
