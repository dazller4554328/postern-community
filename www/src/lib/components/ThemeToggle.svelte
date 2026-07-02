<script lang="ts">
  import { theme } from '$lib/theme.svelte';
  import Icon from './Icon.svelte';
</script>

<button
  type="button"
  class="toggle"
  onclick={() => theme.toggle()}
  aria-label={theme.current === 'dark' ? 'Switch to light theme' : 'Switch to dark theme'}
  title="Toggle theme"
>
  <span class="ico" class:active={theme.current === 'light'}><Icon name="sun" size={17} /></span>
  <span class="ico" class:active={theme.current === 'dark'}><Icon name="moon" size={17} /></span>
</button>

<style>
  .toggle {
    position: relative;
    display: inline-grid;
    place-items: center;
    width: 38px;
    height: 38px;
    border-radius: var(--r-pill);
    border: 1px solid var(--border);
    background: var(--surface);
    color: var(--text-muted);
    cursor: pointer;
    transition:
      color var(--dur-fast) var(--ease),
      border-color var(--dur-fast) var(--ease),
      background-color var(--dur-fast) var(--ease);
  }
  .toggle:hover {
    color: var(--text);
    border-color: var(--border-strong);
  }
  .ico {
    grid-area: 1 / 1;
    display: grid;
    place-items: center;
    opacity: 0;
    transform: rotate(-30deg) scale(0.6);
    transition:
      opacity var(--dur) var(--ease),
      transform var(--dur) var(--ease);
  }
  .ico.active {
    opacity: 1;
    transform: none;
    color: var(--accent);
  }
</style>
