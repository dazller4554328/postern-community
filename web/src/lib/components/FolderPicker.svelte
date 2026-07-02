<script lang="ts">
  import { onMount } from 'svelte';
  import { api, type FoldersResponse } from '$lib/api';

  interface Props {
    accountId: number;
    /// Folder names to exclude from the picker (usually the message's
    /// current label set — no point moving to the folder it's in).
    exclude?: string[];
    onPick: (folder: string) => void;
    onClose: () => void;
  }

  let { accountId, exclude = [], onPick, onClose }: Props = $props();

  let folders = $state<FoldersResponse | null>(null);
  let loading = $state(true);
  let query = $state('');
  let newFolder = $state('');
  let creating = $state(false);
  let err = $state<string | null>(null);

  onMount(async () => {
    try {
      folders = await api.folders();
    } catch (e) {
      err = e instanceof Error ? e.message : String(e);
    } finally {
      loading = false;
    }
  });

  interface PickerItem {
    name: string;
    display: string;
    kind: string;
    total: number;
    unread: number;
    weight: number;
    group: 'System' | 'Gmail categories' | 'Folders';
  }

  let accountFolders = $derived(
    folders?.accounts.find((a) => a.account_id === accountId)
  );
  // Flatten all categories into one list for filtering; the picker
  // shows them grouped but the search query spans everything.
  let all = $derived.by<PickerItem[]>(() => {
    if (!accountFolders) return [];
    const sys: PickerItem[] = accountFolders.system.map((f) => ({ ...f, group: 'System' as const }));
    const cat: PickerItem[] = accountFolders.categories.map((f) => ({ ...f, group: 'Gmail categories' as const }));
    const usr: PickerItem[] = accountFolders.user.map((f) => ({ ...f, group: 'Folders' as const }));
    return [...sys, ...cat, ...usr].filter((f) => !exclude.includes(f.name));
  });

  let filtered = $derived.by(() => {
    const q = query.trim().toLowerCase();
    if (!q) return all;
    return all.filter(
      (f) => f.display.toLowerCase().includes(q) || f.name.toLowerCase().includes(q)
    );
  });

  async function pickCreated() {
    const name = newFolder.trim().replace(/^\/+|\/+$/g, '');
    if (!name) return;
    creating = true;
    err = null;
    try {
      await api.createFolder(accountId, name);
      onPick(name);
    } catch (e) {
      err = e instanceof Error ? e.message : String(e);
    } finally {
      creating = false;
    }
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') onClose();
  }
</script>

<svelte:window onkeydown={onKeydown} />

<div
  class="scrim"
  onclick={onClose}
  onkeydown={(e) => e.key === 'Enter' && onClose()}
  role="button"
  tabindex="-1"
  aria-label="Close folder picker"
></div>

<div class="dialog" role="dialog" aria-modal="true" aria-label="Pick a folder">
  <header>
    <strong>Move to folder</strong>
    <button type="button" class="close" aria-label="Close" onclick={onClose}>✕</button>
  </header>

  <input
    type="search"
    bind:value={query}
    placeholder="Search folders…"
    autocomplete="off"
    spellcheck="false"
  />

  {#if loading}
    <p class="muted">Loading folders…</p>
  {:else if !accountFolders}
    <p class="muted">Account not found.</p>
  {:else}
    <div class="scroll">
      {#if filtered.length === 0}
        <p class="muted empty">No folders match "{query}".</p>
      {:else}
        {#each ['System', 'Gmail categories', 'Folders'] as groupName (groupName)}
          {@const items = filtered.filter((f) => f.group === groupName)}
          {#if items.length > 0}
            <div class="group-label">{groupName}</div>
            <ul>
              {#each items as f (f.name)}
                <li>
                  <button type="button" onclick={() => onPick(f.name)}>
                    <span class="label">{f.display}</span>
                    <span class="path">{f.name}</span>
                    {#if f.total > 0}
                      <span class="count">{f.total}</span>
                    {/if}
                  </button>
                </li>
              {/each}
            </ul>
          {/if}
        {/each}
      {/if}
    </div>

    <form
      class="create"
      onsubmit={(e) => {
        e.preventDefault();
        pickCreated();
      }}
    >
      <input
        type="text"
        bind:value={newFolder}
        placeholder="+ New folder (e.g. Work/Clients)"
        autocomplete="off"
        spellcheck="false"
      />
      <button type="submit" disabled={!newFolder.trim() || creating}>
        {creating ? 'Creating…' : 'Create &amp; move'}
      </button>
    </form>
    {#if err}
      <div class="err">{err}</div>
    {/if}
  {/if}
</div>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.35);
    z-index: 100;
  }
  .dialog {
    position: fixed;
    z-index: 101;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    width: min(32rem, calc(100vw - 2rem));
    max-height: 80vh;
    display: flex;
    flex-direction: column;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 1rem;
    box-shadow: 0 30px 60px rgba(0, 0, 0, 0.22);
    overflow: hidden;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 1rem 1.15rem 0.5rem;
  }
  header strong {
    font-size: 1rem;
    font-weight: 650;
  }
  .close {
    background: transparent;
    border: 0;
    color: inherit;
    font-size: 1rem;
    cursor: pointer;
    opacity: 0.55;
    padding: 0.25rem 0.5rem;
  }
  .close:hover { opacity: 1; }
  input[type='search'],
  .create input {
    margin: 0 1.15rem 0.4rem;
    font: inherit;
    font-size: 0.88rem;
    padding: 0.55rem 0.75rem;
    border: 1px solid color-mix(in oklab, currentColor 18%, transparent);
    background: color-mix(in oklab, currentColor 3%, transparent);
    color: inherit;
    border-radius: 0.6rem;
  }
  .scroll {
    flex: 1;
    overflow-y: auto;
    padding: 0.3rem 0 0.5rem;
  }
  .group-label {
    padding: 0.4rem 1.15rem 0.2rem;
    font-size: 0.68rem;
    font-weight: 700;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    opacity: 0.55;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  li button {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    width: 100%;
    padding: 0.55rem 1.15rem;
    border: 0;
    background: transparent;
    color: inherit;
    font: inherit;
    cursor: pointer;
    text-align: left;
  }
  li button:hover {
    background: color-mix(in oklab, currentColor 6%, transparent);
  }
  .label {
    font-weight: 600;
    font-size: 0.88rem;
  }
  .path {
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 0.72rem;
    opacity: 0.55;
    flex: 1;
  }
  .count {
    font-size: 0.72rem;
    opacity: 0.55;
    padding: 0.1rem 0.45rem;
    border-radius: 999px;
    background: color-mix(in oklab, currentColor 6%, transparent);
  }
  .create {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 0.4rem;
    padding: 0.25rem 1.15rem 0.9rem;
    border-top: 1px solid var(--border);
    padding-top: 0.7rem;
    align-items: center;
  }
  .create input { margin: 0; }
  .create button {
    font: inherit;
    font-size: 0.82rem;
    font-weight: 650;
    padding: 0.55rem 0.95rem;
    border: 1px solid var(--accent);
    background: var(--accent);
    color: white;
    border-radius: 0.6rem;
    cursor: pointer;
  }
  .create button:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }
  .muted {
    padding: 0.6rem 1.15rem;
    opacity: 0.55;
    font-size: 0.85rem;
  }
  .empty { text-align: center; }
  .err {
    margin: 0 1.15rem 0.9rem;
    padding: 0.55rem 0.75rem;
    font-size: 0.8rem;
    background: color-mix(in oklab, crimson 10%, transparent);
    border-left: 2px solid crimson;
    border-radius: 0 0.6rem 0.6rem 0;
  }
</style>
