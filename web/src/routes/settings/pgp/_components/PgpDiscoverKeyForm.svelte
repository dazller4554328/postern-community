<script lang="ts">
  import type { PgpDiscovery } from '$lib/api';

  interface Props {
    onDiscover: (email: string) => Promise<PgpDiscovery | null>;
    onAddDiscovered: (armored: string) => void | Promise<void>;
  }
  let { onDiscover, onAddDiscovered }: Props = $props();

  let email = $state('');
  let busy = $state(false);
  let result = $state<PgpDiscovery | null>(null);

  async function submit(e: Event) {
    e.preventDefault();
    if (!email.trim() || busy) return;
    busy = true;
    result = null;
    try {
      result = await onDiscover(email.trim());
    } finally {
      busy = false;
    }
  }

  async function addDiscovered() {
    if (!result?.armored_public_key) return;
    await onAddDiscovered(result.armored_public_key);
    result = null;
    email = '';
  }
</script>

<section class="panel">
  <div class="section-head">
    <span class="section-icon">⌁</span>
    <div>
      <h2>Find someone's public key</h2>
      <p>Search WKD first, then keys.openpgp.org, and stage results for import.</p>
    </div>
  </div>
  <form onsubmit={submit}>
    <label>
      Email address
      <input type="email" bind:value={email} placeholder="alice@example.com" />
      <small class="hint">Checks WKD first (recipient's own domain), then keys.openpgp.org.</small>
    </label>
    <button type="submit" disabled={busy || !email.trim()}>
      {busy ? 'Searching…' : 'Search'}
    </button>
  </form>

  {#if result}
    <div class="discovery">
      {#if result.source === 'not_found'}
        <p class="not-found">
          No key published for <strong>{email}</strong>.
          They'll need to either publish to WKD / keys.openpgp.org, or send
          you a signed message first so we can harvest their Autocrypt header.
        </p>
      {:else}
        <p class="found">
          ✅ Found via <strong>{result.source === 'wkd' ? 'WKD' : 'keyserver'}</strong>
        </p>
        <details>
          <summary>Armored key</summary>
          <pre>{result.armored_public_key}</pre>
        </details>
        <button onclick={addDiscovered}>Add to my keyring</button>
      {/if}
      {#if result.url_tried.length}
        <details class="tried">
          <summary>URLs tried ({result.url_tried.length})</summary>
          <ul>
            {#each result.url_tried as u (u)}
              <li><code>{u}</code></li>
            {/each}
          </ul>
        </details>
      {/if}
    </div>
  {/if}
</section>

<style>
  .panel {
    margin-bottom: 1rem;
    padding: 1.2rem 1.2rem 1.05rem;
    border: 1px solid var(--border);
    border-radius: 1.2rem;
    background: color-mix(in oklab, var(--surface) 92%, transparent);
    box-shadow: 0 14px 32px rgba(0, 0, 0, 0.05);
  }
  .section-head {
    display: grid;
    grid-template-columns: 2.1rem minmax(0, 1fr);
    gap: 0.85rem;
    margin-bottom: 1rem;
    align-items: start;
  }
  .section-icon {
    display: inline-grid;
    place-items: center;
    width: 2.1rem;
    height: 2.1rem;
    border-radius: 0.72rem;
    background: color-mix(in oklab, var(--surface-2) 86%, transparent);
    border: 1px solid color-mix(in oklab, currentColor 8%, transparent);
    font-size: 0.95rem;
    font-weight: 700;
  }
  .section-head h2 {
    font-size: 0.8rem;
    text-transform: uppercase;
    letter-spacing: 0.1em;
    font-weight: 700;
    opacity: 0.72;
    margin: 0 0 0.2rem;
  }
  .section-head p {
    margin: 0;
    color: var(--muted);
    font-size: 0.82rem;
    line-height: 1.45;
  }
  form {
    display: flex;
    flex-direction: column;
    gap: 1rem;
    padding: 1.1rem 1.2rem;
    border: 1px solid var(--border);
    border-radius: 1rem;
    background: color-mix(in oklab, var(--surface) 96%, transparent);
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
    font-size: 0.82rem;
    font-weight: 500;
    opacity: 0.9;
  }
  input {
    font: inherit;
    font-size: 0.85rem;
    padding: 0.7rem 0.8rem;
    border: 1px solid var(--border);
    border-radius: 0.85rem;
    background: color-mix(in oklab, var(--surface-2) 72%, transparent);
    color: inherit;
    font-weight: 400;
  }
  .hint {
    opacity: 0.55;
    font-size: 0.72rem;
    font-weight: 400;
  }
  button[type='submit'] {
    align-self: flex-start;
    background: var(--accent);
    color: white;
    border: 1px solid transparent;
  }
  button {
    font: inherit;
    font-size: 0.82rem;
    padding: 0.45rem 0.9rem;
    border: 1px solid var(--border);
    background: transparent;
    color: inherit;
    border-radius: 999px;
    cursor: pointer;
    font-weight: 600;
  }
  button:hover:not(:disabled) {
    background: color-mix(in oklab, currentColor 8%, transparent);
  }
  button[type='submit']:hover:not(:disabled) {
    background: var(--accent);
    filter: brightness(0.97);
  }
  button:disabled { opacity: 0.5; cursor: progress; }
  .discovery {
    margin-top: 1rem;
    padding: 0.95rem 1rem;
    border: 1px solid var(--border);
    border-radius: 1rem;
    background: color-mix(in oklab, var(--surface-2) 72%, transparent);
    font-size: 0.85rem;
  }
  .found { color: color-mix(in oklab, forestgreen 85%, currentColor 15%); margin: 0 0 0.5rem; }
  .not-found { margin: 0; opacity: 0.75; }
  .discovery details { margin: 0.5rem 0; }
  .discovery summary { cursor: pointer; font-size: 0.78rem; opacity: 0.65; }
  .discovery pre {
    margin: 0.5rem 0;
    padding: 0.8rem;
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 0.7rem;
    background: color-mix(in oklab, currentColor 4%, transparent);
    border-radius: 0.85rem;
    overflow-x: auto;
    white-space: pre-wrap;
    word-break: break-all;
  }
  .discovery ul { margin: 0.25rem 0 0 1rem; padding: 0; font-size: 0.72rem; opacity: 0.6; }
  .discovery code { font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; }
</style>
