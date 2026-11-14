<script lang="ts">
  import type { FoldersResponse } from '$lib/api';
  import AdvancedSearchDrawer from './AdvancedSearchDrawer.svelte';

  interface Props {
    /** URL-side active query — mirrors into the input on navigation. */
    activeQuery: string;
    folders: FoldersResponse | null;
    /** Triggered by Enter, the AdvancedSearchDrawer's submit, or any
     *  caller that needs the parent's full query handler to run. */
    onSearch: (q: string) => void;
  }

  let { activeQuery, folders, onSearch }: Props = $props();

  let searchInput = $state('');
  let advancedOpen = $state(false);

  // Mirror the URL's active-query into the search input. Kept as a
  // thin $effect so the input updates when the user navigates back/
  // forward through history.
  $effect(() => {
    searchInput = activeQuery;
  });

  async function submitSearch(e: Event) {
    e.preventDefault();
    onSearch(searchInput.trim());
  }
</script>

<form class="search" onsubmit={submitSearch}>
  <label class="search-box">
    <svg viewBox="0 0 20 20" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round">
      <circle cx="8.5" cy="8.5" r="5.5" />
      <path d="m13 13 4 4" />
    </svg>
    <input
      type="search"
      placeholder="Search mail"
      title="Use from: to: subject: body: has:attachment is:unread is:starred label: before: after: older_than:30d account:  — or click the filter icon for the form."
      bind:value={searchInput}
      autocomplete="off"
      spellcheck="false"
    />
    <button
      type="button"
      class="search-advanced-toggle"
      class:open={advancedOpen}
      title={advancedOpen ? 'Close advanced search' : 'Advanced search'}
      aria-label={advancedOpen ? 'Close advanced search' : 'Advanced search'}
      aria-expanded={advancedOpen}
      onclick={() => (advancedOpen = !advancedOpen)}
    >
      <svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">
        <path d="M2 4h12M4 8h8M6 12h4"/>
      </svg>
    </button>
  </label>
</form>
{#if advancedOpen}
  <AdvancedSearchDrawer
    searchText={searchInput}
    {folders}
    onSearch={(q) => { searchInput = q; onSearch(q); }}
    onClose={() => (advancedOpen = false)}
  />
{/if}

<style>
  form.search {
    padding: 0 1rem 0.6rem;
  }
  .search-box {
    display: grid;
    grid-template-columns: 14px 1fr auto;
    align-items: center;
    gap: 0.55rem;
    padding: 0.7rem 0.8rem;
    border: 1px solid color-mix(in oklab, currentColor 12%, transparent);
    background: color-mix(in oklab, var(--surface-2) 72%, transparent);
    border-radius: 0.8rem;
    color: var(--muted);
  }
  .search-advanced-toggle {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    padding: 0;
    background: transparent;
    color: inherit;
    border: 0;
    border-radius: 0.35rem;
    opacity: 0.55;
    cursor: pointer;
    transition: opacity 120ms, background 120ms, color 120ms;
  }
  .search-advanced-toggle:hover,
  .search-advanced-toggle.open {
    opacity: 1;
    background: color-mix(in oklab, currentColor 8%, transparent);
    color: var(--accent);
  }
  form.search input {
    width: 100%;
    font: inherit;
    font-size: 0.84rem;
    padding: 0;
    border: 0;
    background: transparent;
    color: inherit;
    box-sizing: border-box;
  }
  form.search input:focus {
    outline: none;
  }
  .search-box:focus-within {
    border-color: color-mix(in oklab, var(--accent) 32%, transparent);
    box-shadow: 0 0 0 3px color-mix(in oklab, var(--accent) 12%, transparent);
    color: inherit;
  }
</style>
