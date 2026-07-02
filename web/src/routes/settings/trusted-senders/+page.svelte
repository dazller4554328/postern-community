<script lang="ts">
  import './trusted-senders.css';
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

