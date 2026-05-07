<script lang="ts">
  import { onMount } from 'svelte';
  import { api, type TrustedSender, type Account } from '$lib/api';

  let senders = $state<TrustedSender[]>([]);
  let accounts = $state<Account[]>([]);
  let loading = $state(true);

  // Add form
  let showForm = $state(false);
  let formAccountId = $state<number | null>(null);
  let formEmail = $state('');
  let formErr = $state<string | null>(null);
  let busy = $state(false);

  onMount(async () => {
    try {
      [senders, accounts] = await Promise.all([
        api.listTrustedSenders(),
        api.listAccounts()
      ]);
      if (accounts.length > 0) formAccountId = accounts[0].id;
    } catch {
      // listTrustedSenders returns [] gracefully on the server when no
      // accounts have allowlisted senders yet, so a thrown error here
      // is real (auth, network) — surface as empty list and let the
      // user retry; the visible state stays consistent.
    } finally {
      loading = false;
    }
  });

  function acctLabel(id: number) {
    return accounts.find((a) => a.id === id)?.email ?? `#${id}`;
  }

  function fmtDate(ts: number) {
    return new Date(ts * 1000).toLocaleDateString();
  }

  async function add() {
    formErr = null;
    if (formAccountId === null) {
      formErr = 'Pick an account first.';
      return;
    }
    const email = formEmail.trim();
    if (!email.includes('@')) {
      formErr = 'Enter a full email address.';
      return;
    }
    busy = true;
    try {
      await api.addTrustedSender(formAccountId, email);
      senders = await api.listTrustedSenders();
      formEmail = '';
      showForm = false;
    } catch (e) {
      formErr = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
    }
  }

  async function remove(s: TrustedSender) {
    if (!confirm(`Remove ${s.email_lower} from the trusted senders list?`)) return;
    await api.deleteTrustedSender(s.id);
    senders = senders.filter((x) => x.id !== s.id);
  }
</script>

<article class="trusted-shell">
  <div class="page-top">
    <a class="back" href="/settings">← Settings</a>
  </div>

  <header class="hero">
    <div class="hero-copy">
      <span class="eyebrow">Mail filtering</span>
      <h1>Trusted Senders</h1>
      <p>
        Addresses on this list never get filed as spam. When a message from a trusted
        sender lands in the Spam folder, Postern automatically moves it back to your inbox
        on the next sync. Senders are added here whenever you click <strong>Not spam</strong>
        on a message.
      </p>
    </div>
    <div class="hero-badges">
      <span class="hero-chip">Auto-rescued during sync</span>
      <span class="hero-chip">Per mailbox</span>
    </div>
  </header>

  {#if loading}
    <p class="muted">Loading…</p>
  {:else if senders.length === 0 && !showForm}
    <p class="muted">
      No trusted senders yet. Click <strong>Not spam</strong> on a message in the Spam
      folder, or add one manually below.
    </p>
  {:else if senders.length > 0}
    <ul class="senders">
      {#each senders as s (s.id)}
        <li>
          <div class="sender-info">
            <code class="sender-email">{s.email_lower}</code>
            <span class="sender-account">{acctLabel(s.account_id)}</span>
            <span class="sender-date">added {fmtDate(s.created_at)}</span>
          </div>
          <button class="danger" onclick={() => remove(s)}>Remove</button>
        </li>
      {/each}
    </ul>
  {/if}

  <div class="toolbar">
    {#if !showForm}
      <button class="add-btn" onclick={() => (showForm = true)}>+ Add address manually</button>
    {/if}
  </div>

  {#if showForm}
    <form class="add-form" onsubmit={(e) => { e.preventDefault(); add(); }}>
      <div class="field">
        <label for="ts-account">Mailbox</label>
        <select id="ts-account" bind:value={formAccountId}>
          {#each accounts as a (a.id)}
            <option value={a.id}>{a.email}</option>
          {/each}
        </select>
      </div>
      <div class="field">
        <label for="ts-email">Email address</label>
        <input
          id="ts-email"
          type="email"
          bind:value={formEmail}
          placeholder="alice@example.com"
          autocomplete="off"
        />
      </div>
      {#if formErr}
        <div class="err">⚠ {formErr}</div>
      {/if}
      <div class="form-actions">
        <button
          type="button"
          class="ghost"
          onclick={() => { showForm = false; formErr = null; formEmail = ''; }}
        >Cancel</button>
        <button type="submit" class="primary" disabled={busy}>
          {busy ? 'Adding…' : 'Add to allowlist'}
        </button>
      </div>
    </form>
  {/if}
</article>

<style>
  article.trusted-shell {
    width: 100%;
    max-width: clamp(60rem, 94vw, 110rem);
    margin: 0 auto;
    padding: 1.25rem 2rem 2.75rem;
    box-sizing: border-box;
  }
  .page-top { margin-bottom: 0.9rem; }
  .back {
    display: inline-block;
    color: inherit;
    opacity: 0.62;
    text-decoration: none;
    font-size: 0.85rem;
  }
  .back:hover { opacity: 1; }
  .hero {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 1rem;
    padding: 1.4rem 1.5rem;
    margin-bottom: 1.25rem;
    border: 1px solid var(--border);
    border-radius: 1.35rem;
    background:
      radial-gradient(circle at top right, color-mix(in oklab, var(--accent) 12%, transparent), transparent 32%),
      linear-gradient(180deg, color-mix(in oklab, var(--surface) 88%, white 12%), var(--surface));
    box-shadow: 0 18px 42px rgba(0, 0, 0, 0.08);
  }
  .eyebrow {
    display: inline-block;
    margin-bottom: 0.55rem;
    font-size: 0.7rem;
    text-transform: uppercase;
    letter-spacing: 0.12em;
    color: var(--muted);
    font-weight: 700;
  }
  .hero h1 {
    font-size: 2rem;
    font-weight: 650;
    margin: 0 0 0.4rem;
    letter-spacing: -0.03em;
  }
  .hero p {
    font-size: 0.9rem;
    color: var(--muted);
    margin: 0;
    line-height: 1.55;
    max-width: 44rem;
  }
  .hero-badges {
    display: flex;
    flex-wrap: wrap;
    gap: 0.45rem;
    align-content: start;
    justify-content: flex-end;
  }
  .hero-chip {
    display: inline-flex;
    align-items: center;
    padding: 0.42rem 0.72rem;
    border-radius: 999px;
    background: color-mix(in oklab, var(--surface-2) 82%, transparent);
    border: 1px solid color-mix(in oklab, currentColor 8%, transparent);
    font-size: 0.72rem;
    font-weight: 600;
  }
  .muted { opacity: 0.55; font-size: 0.88rem; }

  ul.senders {
    list-style: none;
    margin: 0 0 1rem;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.55rem;
  }
  ul.senders li {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    padding: 0.85rem 1rem;
    border: 1px solid var(--border);
    border-radius: 0.9rem;
    background: color-mix(in oklab, var(--surface) 96%, transparent);
  }
  .sender-info {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 0.7rem;
    min-width: 0;
  }
  .sender-email {
    font-size: 0.92rem;
    background: color-mix(in oklab, currentColor 6%, transparent);
    padding: 0.2em 0.55em;
    border-radius: 0.35em;
  }
  .sender-account {
    font-size: 0.75rem;
    opacity: 0.6;
    font-family: ui-monospace, monospace;
  }
  .sender-date { font-size: 0.72rem; opacity: 0.45; }

  ul.senders button.danger {
    font: inherit;
    font-size: 0.75rem;
    padding: 0.3rem 0.7rem;
    border: 1px solid color-mix(in oklab, crimson 30%, transparent);
    background: transparent;
    color: color-mix(in oklab, crimson 80%, currentColor);
    border-radius: 999px;
    cursor: pointer;
  }
  ul.senders button.danger:hover {
    background: color-mix(in oklab, crimson 10%, transparent);
  }

  .toolbar {
    display: flex;
    gap: 0.65rem;
    align-items: center;
    margin-bottom: 1rem;
  }
  .add-btn {
    font: inherit;
    font-size: 0.88rem;
    padding: 0.55rem 1rem;
    border: 1px dashed var(--border);
    background: transparent;
    color: inherit;
    border-radius: 999px;
    cursor: pointer;
    opacity: 0.75;
  }
  .add-btn:hover {
    opacity: 1;
    background: color-mix(in oklab, currentColor 4%, transparent);
  }

  .add-form {
    border: 1px solid var(--border);
    border-radius: 1rem;
    padding: 1.25rem;
    background: color-mix(in oklab, var(--surface) 96%, transparent);
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
    max-width: 38rem;
  }
  .field {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
    align-items: center;
  }
  .field label {
    font-size: 0.78rem;
    opacity: 0.6;
    min-width: 6rem;
  }
  .add-form input,
  .add-form select {
    font: inherit;
    font-size: 0.85rem;
    padding: 0.4rem 0.6rem;
    border: 1px solid color-mix(in oklab, currentColor 18%, transparent);
    background: color-mix(in oklab, var(--surface-2) 72%, transparent);
    color: inherit;
    border-radius: 0.8rem;
    flex: 1;
    min-width: 8rem;
  }
  .err {
    padding: 0.5rem 0.75rem;
    background: color-mix(in oklab, crimson 12%, transparent);
    border-left: 2px solid crimson;
    border-radius: 0 0.3rem 0.3rem 0;
    font-size: 0.82rem;
  }
  .form-actions {
    display: flex;
    gap: 0.5rem;
    justify-content: flex-end;
    margin-top: 0.5rem;
  }
  .form-actions button {
    font: inherit;
    font-size: 0.85rem;
    padding: 0.45rem 1rem;
    border: 1px solid var(--border);
    background: transparent;
    color: inherit;
    border-radius: 999px;
    cursor: pointer;
  }
  .form-actions button.primary {
    background: var(--accent);
    border-color: var(--accent);
    color: white;
    font-weight: 600;
  }
  .form-actions button.primary:hover { filter: brightness(0.97); }
  .form-actions button.primary:disabled { opacity: 0.6; cursor: progress; }
  .form-actions button.ghost:hover {
    background: color-mix(in oklab, currentColor 6%, transparent);
  }

  @media (max-width: 820px) {
    article.trusted-shell { padding: 1rem; }
    .hero { grid-template-columns: 1fr; }
    .hero-badges { justify-content: flex-start; }
    ul.senders li {
      flex-direction: column;
      align-items: flex-start;
      gap: 0.5rem;
    }
  }
</style>
