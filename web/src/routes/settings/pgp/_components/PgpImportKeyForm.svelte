<script lang="ts">
  interface Props {
    onImport: (armored: string, passphrase?: string) => void | Promise<void>;
  }
  let { onImport }: Props = $props();

  let armored = $state('');
  let passphrase = $state('');
  let busy = $state(false);

  // Only private keys can be passphrase-protected — show the field for those.
  const isPrivate = $derived(armored.includes('BEGIN PGP PRIVATE KEY BLOCK'));

  async function submit(e: Event) {
    e.preventDefault();
    if (!armored.trim() || busy) return;
    busy = true;
    try {
      await onImport(armored, isPrivate && passphrase ? passphrase : undefined);
      armored = '';
      passphrase = '';
    } finally {
      busy = false;
    }
  }
</script>

<section class="panel">
  <div class="section-head">
    <span class="section-icon">⬒</span>
    <div>
      <h2>Import existing key</h2>
      <p>Bring in armored public or private key material.</p>
    </div>
  </div>
  <form onsubmit={submit}>
    <label>
      Armored key block
      <textarea
        bind:value={armored}
        rows="8"
        spellcheck="false"
        placeholder="-----BEGIN PGP PUBLIC KEY BLOCK-----&#10;... or ...&#10;-----BEGIN PGP PRIVATE KEY BLOCK-----"
      ></textarea>
      <small class="hint">
        Paste either a public key (just adds to keyring) or a private-key
        block (we derive the public half automatically).
      </small>
    </label>
    {#if isPrivate}
      <label>
        Passphrase <span class="opt">(only if the private key has one)</span>
        <input
          type="password"
          bind:value={passphrase}
          autocomplete="off"
          placeholder="Leave blank if the key isn't passphrase-protected"
        />
        <small class="hint">
          Postern unlocks the key once and stores it in the encrypted vault —
          the passphrase itself is not saved.
        </small>
      </label>
    {/if}
    <button type="submit" disabled={busy || !armored.trim()}>
      {busy ? 'Importing…' : 'Import'}
    </button>
  </form>
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
  textarea {
    font: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 0.78rem;
    padding: 0.7rem 0.8rem;
    border: 1px solid var(--border);
    border-radius: 0.85rem;
    background: color-mix(in oklab, var(--surface-2) 72%, transparent);
    color: inherit;
    resize: vertical;
  }
  input[type='password'] {
    font: inherit;
    font-size: 0.82rem;
    padding: 0.5rem 0.7rem;
    border: 1px solid var(--border);
    border-radius: 0.85rem;
    background: color-mix(in oklab, var(--surface-2) 72%, transparent);
    color: inherit;
  }
  .opt {
    font-weight: 400;
    opacity: 0.55;
  }
  .hint {
    opacity: 0.55;
    font-size: 0.72rem;
    font-weight: 400;
  }
  button {
    font: inherit;
    font-size: 0.82rem;
    padding: 0.45rem 0.9rem;
    border: 1px solid transparent;
    background: var(--accent);
    color: white;
    border-radius: 999px;
    cursor: pointer;
    font-weight: 600;
    align-self: flex-start;
  }
  button:hover:not(:disabled) {
    filter: brightness(0.97);
  }
  button:disabled { opacity: 0.5; cursor: progress; }
</style>
