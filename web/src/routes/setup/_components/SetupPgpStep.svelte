<script lang="ts">
  export type PgpChoice = 'skip' | 'generate' | 'import';

  interface Props {
    pgpChoice: PgpChoice;
    pgpUserId: string;
    pgpImportArmored: string;
    /** Pre-fill placeholder for the user-id field. */
    emailPlaceholder: string;
    disabled?: boolean;
  }
  let {
    pgpChoice = $bindable(),
    pgpUserId = $bindable(),
    pgpImportArmored = $bindable(),
    emailPlaceholder,
    disabled = false,
  }: Props = $props();
</script>

<fieldset class="pgp" {disabled}>
  <legend>3. PGP — optional</legend>
  <p class="sub-note">
    Set up end-to-end encryption for this address now, or skip and come back later
    via <em>Settings → PGP</em>.
  </p>

  <label class="pgp-option">
    <input type="radio" name="pgp" value="skip" bind:group={pgpChoice} />
    <div>
      <strong>Skip for now</strong>
      <span>Fine for most users. Postern still auto-discovers other people's keys (WKD) and auto-encrypts when possible.</span>
    </div>
  </label>

  <label class="pgp-option">
    <input type="radio" name="pgp" value="generate" bind:group={pgpChoice} />
    <div>
      <strong>Generate a new keypair</strong>
      <span>Postern creates an ed25519 keypair bound to this mailbox. Published to keys.openpgp.org only when you click Publish.</span>
    </div>
  </label>
  {#if pgpChoice === 'generate'}
    <label class="indent">
      Identity (User ID)
      <input
        type="text"
        bind:value={pgpUserId}
        placeholder="Your Name &lt;{emailPlaceholder || 'you@example.com'}&gt;"
        required
      />
      <small class="hint">
        Appears on the key as its user ID. Pre-filled from your email + display name.
      </small>
    </label>
  {/if}

  <label class="pgp-option">
    <input type="radio" name="pgp" value="import" bind:group={pgpChoice} />
    <div>
      <strong>Import existing key</strong>
      <span>Paste an armored public or private key you already have — Postern extracts the public half and stores the private half in the vault.</span>
    </div>
  </label>
  {#if pgpChoice === 'import'}
    <label class="indent">
      Armored key
      <textarea
        bind:value={pgpImportArmored}
        rows="8"
        spellcheck="false"
        autocomplete="off"
        placeholder="-----BEGIN PGP PRIVATE KEY BLOCK-----&#10;...&#10;-----END PGP PRIVATE KEY BLOCK-----"
        required
      ></textarea>
    </label>
  {/if}
</fieldset>

<style>
  fieldset {
    border: 0;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 0.85rem;
  }
  fieldset[disabled] {
    opacity: 0.45;
  }
  legend {
    font-size: 0.78rem;
    font-weight: 700;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    opacity: 0.7;
    padding: 0;
    margin-bottom: 0.1rem;
  }
  .sub-note {
    font-size: 0.78rem;
    opacity: 0.7;
    margin: 0 0 0.15rem;
    line-height: 1.5;
  }
  .pgp-option {
    display: flex;
    flex-direction: row;
    align-items: flex-start;
    gap: 0.6rem;
    padding: 0.65rem 0.8rem;
    border: 1px solid color-mix(in oklab, currentColor 12%, transparent);
    border-radius: 0.75rem;
    cursor: pointer;
    font-weight: 400;
  }
  .pgp-option:has(input:checked) {
    border-color: color-mix(in oklab, var(--accent) 40%, transparent);
    background: color-mix(in oklab, var(--accent) 6%, transparent);
  }
  .pgp-option input[type='radio'] {
    margin-top: 0.15rem;
    flex-shrink: 0;
    width: auto;
    padding: 0;
  }
  .pgp-option div {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
  }
  .pgp-option strong {
    font-size: 0.82rem;
  }
  .pgp-option span {
    font-size: 0.75rem;
    opacity: 0.6;
    line-height: 1.4;
  }
  .indent {
    margin-left: 1.9rem;
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 0.42rem;
    font-size: 0.8rem;
    opacity: 0.9;
    font-weight: 600;
  }
  input, textarea {
    font: inherit;
    padding: 0.72rem 0.82rem;
    border: 1px solid color-mix(in oklab, currentColor 18%, transparent);
    border-radius: 0.85rem;
    background: color-mix(in oklab, var(--surface-2) 74%, transparent);
    color: inherit;
    font-weight: 400;
  }
  input:focus, textarea:focus {
    outline: none;
    border-color: color-mix(in oklab, var(--accent) 32%, transparent);
    box-shadow: 0 0 0 3px color-mix(in oklab, var(--accent) 12%, transparent);
  }
  textarea {
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 0.78rem;
    resize: vertical;
  }
  .hint {
    opacity: 0.6;
    font-size: 0.72rem;
    font-weight: 400;
    line-height: 1.5;
  }
</style>
