<script lang="ts">
  import './pgp.css';
  import { onMount } from 'svelte';
  import { api, type PgpKey, type PgpDiscovery, type PgpPublishResult } from '$lib/api';
  import { formatDate } from '$lib/format';
  import PgpGenerateKeyForm from './_components/PgpGenerateKeyForm.svelte';
  import PgpImportKeyForm from './_components/PgpImportKeyForm.svelte';
  import PgpDiscoverKeyForm from './_components/PgpDiscoverKeyForm.svelte';

  let keys = $state<PgpKey[]>([]);
  let loading = $state(true);
  let err = $state<string | null>(null);

  async function refresh() {
    try {
      keys = await api.pgpKeys();
      err = null;
    } catch (e) {
      err = e instanceof Error ? e.message : String(e);
    }
  }

  async function generate(userId: string) {
    err = null;
    try {
      await api.pgpGenerate(userId);
      await refresh();
    } catch (e) {
      err = e instanceof Error ? e.message : String(e);
    }
  }

  async function importKey(armored: string, passphrase?: string) {
    err = null;
    try {
      await api.pgpImport(armored, passphrase);
      await refresh();
    } catch (e) {
      err = e instanceof Error ? e.message : String(e);
    }
  }

  async function exportKey(k: PgpKey) {
    const r = await api.pgpExport(k.id);
    const blob = new Blob([r.armored], { type: 'application/pgp-keys' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `${k.fingerprint.slice(0, 16)}.asc`;
    a.click();
    URL.revokeObjectURL(url);
  }

  async function deleteKey(k: PgpKey) {
    const warning = k.is_secret
      ? `Delete SECRET key ${k.user_id}? You will no longer be able to decrypt messages encrypted to this key. Export it first if you want a backup.`
      : `Delete public key for ${k.user_id}?`;
    if (!confirm(warning)) return;
    await api.pgpDelete(k.id);
    await refresh();
  }

  async function runDiscovery(email: string): Promise<PgpDiscovery | null> {
    err = null;
    try {
      return await api.pgpDiscover(email);
    } catch (e) {
      err = e instanceof Error ? e.message : String(e);
      return null;
    }
  }

  async function addDiscovered(armored: string) {
    try {
      await api.pgpImport(armored);
      await refresh();
    } catch (e) {
      err = e instanceof Error ? e.message : String(e);
    }
  }

  // Per-key publish state — tracks the in-flight upload and the
  // result banner so the row can show "check your inbox to verify"
  // without navigating away.
  let publishingKeyId = $state<number | null>(null);
  let publishResults = $state<Record<number, PgpPublishResult>>({});
  async function publishKey(k: PgpKey) {
    if (publishingKeyId !== null) return;
    publishingKeyId = k.id;
    err = null;
    try {
      const result = await api.pgpPublish(k.id);
      publishResults = { ...publishResults, [k.id]: result };
    } catch (e) {
      err = e instanceof Error ? e.message : String(e);
    } finally {
      publishingKeyId = null;
    }
  }

  // Contact-key filtering — the auto-harvested list gets long fast
  // (every Autocrypt sender, every WKD lookup, every discovered
  // recipient). Give the user a search box so finding a specific
  // address doesn't require scrolling.
  let contactFilter = $state('');
  let showContacts = $state(true);

  // Pagination — same pattern as Security → trusted devices. The
  // contact-key list can grow into the hundreds once Autocrypt + WKD
  // discovery have run for a while; rendering them all at once
  // tanks the panel.
  const CONTACTS_PAGE_SIZE = 25;
  let contactsPage = $state(0);

  // Scan-keyserver state. Runs against every configured mail
  // account and shows which ones have a published key on
  // keys.openpgp.org. Handy for "am I discoverable for every
  // address I send from?".
  let scanning = $state(false);
  let scanResults = $state<import('$lib/api').PgpKeyserverStatus[] | null>(null);
  async function runKeyserverScan() {
    scanning = true;
    err = null;
    try {
      scanResults = await api.pgpKeyserverScan();
    } catch (e) {
      err = e instanceof Error ? e.message : String(e);
    } finally {
      scanning = false;
    }
  }

  // Backup — concatenates every key into one .asc. Public-only
  // bundles are safe to download anywhere; full backups contain
  // PRIVATE KEY BLOCK sections and warrant a confirm.
  let downloadingBackup = $state(false);
  async function downloadBackup(includeSecret: boolean) {
    if (includeSecret) {
      const warning =
        'This download includes your PRIVATE keys. Anyone with the file can decrypt your mail and impersonate you.\n\nStore it somewhere safe (encrypted disk, password manager, offline USB). Continue?';
      if (!confirm(warning)) return;
    }
    downloadingBackup = true;
    try {
      const blob = await api.pgpExportAll(includeSecret);
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      const stamp = new Date().toISOString().slice(0, 10);
      a.download = includeSecret
        ? `postern-pgp-backup-${stamp}.asc`
        : `postern-pgp-public-${stamp}.asc`;
      a.click();
      URL.revokeObjectURL(url);
    } catch (e) {
      err = e instanceof Error ? e.message : String(e);
    } finally {
      downloadingBackup = false;
    }
  }

  // Split the key list into ownership buckets. "Mine" = everything
  // we hold a secret for; "Contacts" = harvested public-only keys.
  let myKeys = $derived(keys.filter((k) => k.is_secret));
  let contactKeys = $derived(keys.filter((k) => !k.is_secret));
  let filteredContacts = $derived.by(() => {
    const q = contactFilter.trim().toLowerCase();
    if (!q) return contactKeys;
    return contactKeys.filter((k) => {
      const uid = k.user_id.toLowerCase();
      const email = (k.primary_email ?? '').toLowerCase();
      const fpr = k.fingerprint.toLowerCase();
      return uid.includes(q) || email.includes(q) || fpr.includes(q);
    });
  });
  let totalContactPages = $derived(
    Math.max(1, Math.ceil(filteredContacts.length / CONTACTS_PAGE_SIZE))
  );
  let pagedContacts = $derived(
    filteredContacts.slice(
      contactsPage * CONTACTS_PAGE_SIZE,
      (contactsPage + 1) * CONTACTS_PAGE_SIZE
    )
  );
  // Reset to page 0 whenever the filter changes (a search starting on
  // page 5 would otherwise show "No contacts match …") or when the
  // list shrinks past the current window (e.g. after deleting the
  // last item on the current page).
  $effect(() => {
    contactFilter;
    contactsPage = 0;
  });
  $effect(() => {
    if (contactsPage >= totalContactPages) {
      contactsPage = Math.max(0, totalContactPages - 1);
    }
  });

  function contactsPrev() {
    if (contactsPage > 0) contactsPage -= 1;
  }
  function contactsNext() {
    if (contactsPage < totalContactPages - 1) contactsPage += 1;
  }

  onMount(async () => {
    await refresh();
    loading = false;
  });

  function formatFpr(f: string) {
    return f.match(/.{1,4}/g)?.join(' ') ?? f;
  }

  function sourceBadge(source: string) {
    switch (source) {
      case 'generated': return { text: 'generated here', class: 'gen' };
      case 'imported': return { text: 'imported', class: 'imp' };
      case 'autocrypt': return { text: 'via Autocrypt', class: 'auto' };
      case 'wkd': return { text: 'via WKD', class: 'wkd' };
      case 'keyserver': return { text: 'keyserver', class: 'ks' };
      default: return { text: source, class: '' };
    }
  }
</script>

<article class="pgp-shell">
  <div class="page-top">
    <a class="back" href="/settings">← Settings</a>
  </div>

  <header class="hero">
    <div class="hero-copy">
      <span class="eyebrow">Key Material</span>
      <h1>PGP Keys</h1>
      <p class="sub">
        Generate, import, inspect, and discover keys inside the same hardened control
        surface. Secret material stays local to this Postern node.
      </p>
    </div>
    <div class="hero-badges">
      <span class="hero-chip">Local keyring</span>
      <span class="hero-chip">WKD discovery</span>
      <span class="hero-chip">Autocrypt-ready</span>
    </div>
  </header>

  {#if err}<p class="err">{err}</p>{/if}

  {#snippet keyRow(k: PgpKey)}
    <li>
      <div class="key-main">
        <div class="key-top">
          <strong>{k.user_id}</strong>
          {#if k.is_secret}
            <span class="pill sec">SECRET</span>
          {:else}
            <span class="pill pub">public</span>
          {/if}
          <span class="pill src {sourceBadge(k.source).class}">{sourceBadge(k.source).text}</span>
        </div>
        <div class="fpr">{formatFpr(k.fingerprint)}</div>
        <div class="meta">
          created {formatDate(k.created_at)}
          {#if k.expires_at}· expires {formatDate(k.expires_at)}{/if}
          {#if k.primary_email}· <code>{k.primary_email}</code>{/if}
        </div>
      </div>
      <div class="key-actions">
        <button onclick={() => exportKey(k)}>Export</button>
        {#if k.is_secret}
          <button
            onclick={() => publishKey(k)}
            disabled={publishingKeyId === k.id}
            title="Upload this public key to keys.openpgp.org so clients like Proton / K-9 / GPG can find it."
          >
            {publishingKeyId === k.id ? 'Publishing…' : 'Publish'}
          </button>
        {/if}
        <button class="danger" onclick={() => deleteKey(k)}>Delete</button>
      </div>
      {#if publishResults[k.id]}
        {@const res = publishResults[k.id]}
        <div class="publish-result">
          {#if res.verification_sent.length > 0}
            <p>
              <strong>Verification email sent.</strong>
              Check <code>{res.verification_sent.join(', ')}</code> and click the link
              from <code>keys@keys.openpgp.org</code> — your key becomes publicly
              retrievable once the link is clicked.
            </p>
          {/if}
          {#if res.already_published.length > 0}
            <p>
              <strong>Already public:</strong>
              <code>{res.already_published.join(', ')}</code>
            </p>
          {/if}
          <p class="fine">
            Fingerprint Hagrid reports: <code>{res.key_fpr.slice(0, 16)}…</code>.
            <a href={res.key_url} target="_blank" rel="noopener">view on keys.openpgp.org</a>
          </p>
        </div>
      {/if}
    </li>
  {/snippet}

  <section class="panel">
    <div class="section-head">
      <span class="section-icon">◇</span>
      <div>
        <h2>Your keys</h2>
        <p>
          Secret keys are your identities. Public keys are contacts we've
          collected from WKD, Autocrypt headers, or manual import.
        </p>
      </div>
    </div>

    <!-- Toolbar: scan keyserver + backup — sits once above both lists so
         it applies to the whole keyring, not just one section. -->
    <div class="keyring-toolbar">
      <button onclick={runKeyserverScan} disabled={scanning}>
        {scanning ? 'Scanning…' : 'Scan keyserver'}
      </button>
      <button onclick={() => downloadBackup(false)} disabled={downloadingBackup}>
        Download public keys
      </button>
      <button
        class="caution"
        onclick={() => downloadBackup(true)}
        disabled={downloadingBackup}
        title="Full backup: public + private keys. Store this file somewhere safe."
      >
        {downloadingBackup ? 'Working…' : 'Download backup (incl. private)'}
      </button>
    </div>

    {#if scanResults}
      <div class="scan-results">
        <strong>keys.openpgp.org status — your accounts:</strong>
        <ul>
          {#each scanResults as r (r.email)}
            <li class="scan-row presence-{r.presence}">
              <span class="mark">
                {#if r.presence === 'published'}✓
                {:else if r.presence === 'notfound'}✗
                {:else}?{/if}
              </span>
              <code>{r.email}</code>
              <span class="verdict">
                {#if r.presence === 'published'}
                  published — clients can find your key
                {:else if r.presence === 'notfound'}
                  no public key found — click Publish on your key above
                {:else}
                  couldn't reach the keyserver
                {/if}
              </span>
            </li>
          {/each}
        </ul>
      </div>
    {/if}

    {#if loading}
      <p class="muted">Loading…</p>
    {:else if keys.length === 0}
      <p class="muted">No keys yet. Generate one below.</p>
    {:else}
      <h3 class="subhead">My keys <span class="count">({myKeys.length})</span></h3>
      {#if myKeys.length === 0}
        <p class="muted">You haven't generated or imported a key yet.</p>
      {:else}
        <ul class="keylist">
          {#each myKeys as k (k.id)}
            {@render keyRow(k)}
          {/each}
        </ul>
      {/if}

      <div class="subhead-row">
        <h3 class="subhead">
          Contacts
          <span class="count">({filteredContacts.length}{contactFilter ? ` / ${contactKeys.length}` : ''})</span>
          <button
            type="button"
            class="linklike"
            onclick={() => (showContacts = !showContacts)}
            aria-expanded={showContacts}
          >{showContacts ? 'hide' : 'show'}</button>
        </h3>
        {#if showContacts && contactKeys.length > 5}
          <input
            type="search"
            class="contact-search"
            placeholder="Filter by name, email, or fingerprint"
            bind:value={contactFilter}
          />
        {/if}
      </div>
      {#if showContacts}
        {#if contactKeys.length === 0}
          <p class="muted">No contact keys yet. They'll appear here when Postern discovers or harvests one.</p>
        {:else if filteredContacts.length === 0}
          <p class="muted">No contacts match "{contactFilter}".</p>
        {:else}
          <ul class="keylist">
            {#each pagedContacts as k (k.id)}
              {@render keyRow(k)}
            {/each}
          </ul>
          {#if totalContactPages > 1}
            <nav class="pager" aria-label="Contact key pages">
              <button
                type="button"
                disabled={contactsPage === 0}
                onclick={contactsPrev}
              >Previous</button>
              <span class="pager-status">
                Page {contactsPage + 1} of {totalContactPages}
                <span class="muted">· {filteredContacts.length} {filteredContacts.length === 1 ? 'key' : 'keys'}</span>
              </span>
              <button
                type="button"
                disabled={contactsPage >= totalContactPages - 1}
                onclick={contactsNext}
              >Next</button>
            </nav>
          {/if}
        {/if}
      {/if}
    {/if}
  </section>

  <PgpGenerateKeyForm onGenerate={generate} />
  <PgpImportKeyForm onImport={importKey} />
  <PgpDiscoverKeyForm onDiscover={runDiscovery} onAddDiscovered={addDiscovered} />
</article>

