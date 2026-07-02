<script lang="ts">
  interface Props {
    suggestions: string[];
    rect: { top: number; left: number; width: number } | null;
    onPick: (addr: string) => void;
  }
  let { suggestions, rect, onPick }: Props = $props();
</script>

{#if suggestions.length > 0 && rect}
  <ul
    class="ac-list"
    style:top="{rect.top}px"
    style:left="{rect.left}px"
    style:width="{rect.width}px"
  >
    {#each suggestions as s (s)}
      <li>
        <button type="button" onmousedown={(e) => { e.preventDefault(); onPick(s); }}>{s}</button>
      </li>
    {/each}
  </ul>
{/if}

<style>
  /* Auto-complete dropdown — rendered at the top of <body> with
     `position: fixed`, so it escapes every form/row stacking context
     instead of fighting them. JS computes top/left/width from the
     focused input's bounding rect. */
  .ac-list {
    position: fixed;
    z-index: 9999;
    list-style: none;
    margin: 0;
    padding: 0.25rem 0;
    background: var(--surface, #fff);
    border: 1px solid var(--border);
    border-radius: 0.9rem;
    box-shadow: 0 6px 18px rgba(0, 0, 0, 0.14);
    max-height: 200px;
    overflow-y: auto;
  }
  .ac-list li button {
    display: block;
    width: 100%;
    text-align: left;
    padding: 0.4rem 0.65rem;
    background: transparent;
    border: 0;
    color: inherit;
    font: inherit;
    font-size: 0.85rem;
    cursor: pointer;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .ac-list li button:hover {
    background: color-mix(in oklab, currentColor 8%, transparent);
  }
</style>
