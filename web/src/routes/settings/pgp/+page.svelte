<script lang="ts">
  import { onMount } from 'svelte';
  import { api, type PgpKey, type PgpDiscovery, type PgpPublishResult } from '$lib/api';
  import { formatDate } from '$lib/format';

  let keys = $state<PgpKey[]>([]);
  let loading = $state(true);
  let err = $state<string | null>(null);

  let genUserId = $state('');
  let generating = $state(false);

  let importArmored = $state('');
  let importing = $state(false);

  let discoverEmail = $state('');
  let discovering = $state(false);
  let discoveryResult = $state<PgpDiscovery | null>(null);

  async function refresh() {
    try {
      keys = await api.pgpKeys();
      err = null;
    } catch (e) {
      err = e instanceof Error ? e.message : String(e);
    }
  }

  async function generate(e: Event) {
    e.preventDefault();
    if (!genUserId.trim()) return;
    generating = true;
    err = null;
    try {
      await api.pgpGenerate(genUserId.trim());
      genUserId = '';
      await refresh();
    } catch (e) {
      err = e instanceof Error ? e.message : String(e);
    } finally {
      generating = false;
    }
  }

  async function importKey(e: Event) {
    e.preventDefault();
    if (!importArmored.trim()) return;
    importing = true;
    err = null;
    try {
      await api.pgpImport(importArmored);
      importArmored = '';
      await refresh();
    } catch (e) {
      err = e instanceof Error ? e.message : String(e);
    } finally {
      importing = false;
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

  async function runDiscovery(e: Event) {
    e.preventDefault();
    if (!discoverEmail.trim()) return;
    discovering = true;
    err = null;
    discoveryResult = null;
    try {
      discoveryResult = await api.pgpDiscover(discoverEmail.trim());
    } catch (e) {
      err = e instanceof Error ? e.message : String(e);
    } finally {
      discovering = false;
    }
  }

  async function addDiscovered() {
    if (!discoveryResult?.armored_public_key) return;
    try {
      await api.pgpImport(discoveryResult.armored_public_key);
      discoveryResult = null;
      discoverEmail = '';
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
            {#each filteredContacts as k (k.id)}
              {@render keyRow(k)}
            {/each}
          </ul>
        {/if}
      {/if}
    {/if}
  </section>

  <section class="panel">
    <div class="section-head">
      <span class="section-icon">✦</span>
      <div>
        <h2>Generate your keypair</h2>
        <p>Create a fresh identity for encrypted mail handled by this server.</p>
      </div>
    </div>
    <form onsubmit={generate}>
      <label>
        User ID
        <input
          type="text"
          bind:value={genUserId}
          placeholder="Your Name <you@example.com>"
          spellcheck="false"
        />
        <small class="hint">
          Standard format: name followed by email in angle brackets. Ed25519 + ECDH
          keypair, no passphrase.
        </small>
      </label>
      <button type="submit" disabled={generating || !genUserId.trim()}>
        {generating ? 'Generating (this takes a second)…' : 'Generate'}
      </button>
    </form>
  </section>

  <section class="panel">
    <div class="section-head">
      <span class="section-icon">⬒</span>
      <div>
        <h2>Import existing key</h2>
        <p>Bring in armored public or private key material.</p>
      </div>
    </div>
    <form onsubmit={importKey}>
      <label>
        Armored key block
        <textarea
          bind:value={importArmored}
          rows="8"
          spellcheck="false"
          placeholder="-----BEGIN PGP PUBLIC KEY BLOCK-----&#10;... or ...&#10;-----BEGIN PGP PRIVATE KEY BLOCK-----"
        ></textarea>
        <small class="hint">
          Paste either a public key (just adds to keyring) or a private-key
          block (we derive the public half automatically).
        </small>
      </label>
      <button type="submit" disabled={importing || !importArmored.trim()}>
        {importing ? 'Importing…' : 'Import'}
      </button>
    </form>
  </section>

  <section class="panel">
    <div class="section-head">
      <span class="section-icon">⌁</span>
      <div>
        <h2>Find someone's public key</h2>
        <p>Search WKD first, then keys.openpgp.org, and stage results for import.</p>
      </div>
    </div>
    <form onsubmit={runDiscovery}>
      <label>
        Email address
        <input type="email" bind:value={discoverEmail} placeholder="alice@example.com" />
        <small class="hint">Checks WKD first (recipient's own domain), then keys.openpgp.org.</small>
      </label>
      <button type="submit" disabled={discovering || !discoverEmail.trim()}>
        {discovering ? 'Searching…' : 'Search'}
      </button>
    </form>

    {#if discoveryResult}
      <div class="discovery">
        {#if discoveryResult.source === 'not_found'}
          <p class="not-found">
            No key published for <strong>{discoverEmail}</strong>.
            They'll need to either publish to WKD / keys.openpgp.org, or send
            you a signed message first so we can harvest their Autocrypt header.
          </p>
        {:else}
          <p class="found">
            ✅ Found via <strong>{discoveryResult.source === 'wkd' ? 'WKD' : 'keyserver'}</strong>
          </p>
          <details>
            <summary>Armored key</summary>
            <pre>{discoveryResult.armored_public_key}</pre>
          </details>
          <button onclick={addDiscovered}>Add to my keyring</button>
        {/if}
        {#if discoveryResult.url_tried.length}
          <details class="tried">
            <summary>URLs tried ({discoveryResult.url_tried.length})</summary>
            <ul>
              {#each discoveryResult.url_tried as u (u)}
                <li><code>{u}</code></li>
              {/each}
            </ul>
          </details>
        {/if}
      </div>
    {/if}
  </section>
</article>

<style>
  article.pgp-shell {
    width: 100%;
    max-width: 68rem;
    padding: 2rem 2rem 2.75rem;
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
    margin-bottom: 1rem;
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
    font-size: 2.1rem;
    font-weight: 650;
    letter-spacing: -0.03em;
    margin: 0 0 0.55rem;
  }
  .hero .sub {
    color: var(--muted);
    font-size: 0.95rem;
    line-height: 1.55;
    margin: 0;
    max-width: 46rem;
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
  .muted { opacity: 0.55; font-size: 0.85rem; }
  .err { color: #c83333; font-size: 0.85rem; margin: 0 0 1rem; }

  ul.keylist {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }
  ul.keylist li {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 1rem;
    padding: 1rem 1rem;
    border: 1px solid var(--border);
    border-radius: 1rem;
    background: color-mix(in oklab, var(--surface) 96%, transparent);
  }
  .key-main { flex: 1; min-width: 0; }
  .key-top {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    flex-wrap: wrap;
    margin-bottom: 0.3rem;
  }
  .key-top strong {
    font-weight: 650;
    font-size: 0.95rem;
  }
  .pill {
    font-size: 0.65rem;
    padding: 0.1rem 0.45rem;
    border-radius: 999px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    font-weight: 600;
  }
  .pill.sec { background: color-mix(in oklab, crimson 25%, transparent); }
  .pill.pub { background: color-mix(in oklab, dodgerblue 20%, transparent); }
  .keyring-toolbar {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
    margin-bottom: 1rem;
    padding-bottom: 0.9rem;
    border-bottom: 1px solid var(--border);
  }
  .keyring-toolbar button.caution {
    border-color: color-mix(in oklab, crimson 35%, var(--border));
    color: color-mix(in oklab, crimson 85%, currentColor);
  }
  .scan-results {
    margin-bottom: 1rem;
    padding: 0.7rem 0.9rem;
    border-radius: 0.6rem;
    background: color-mix(in oklab, var(--accent) 6%, transparent);
    border: 1px solid color-mix(in oklab, var(--accent) 25%, transparent);
    font-size: 0.82rem;
  }
  .scan-results ul {
    list-style: none;
    margin: 0.45rem 0 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }
  .scan-row {
    display: grid;
    grid-template-columns: 1.2rem auto 1fr;
    gap: 0.5rem;
    align-items: baseline;
    font-size: 0.78rem;
  }
  .scan-row .mark {
    text-align: center;
    font-weight: 700;
  }
  .scan-row .verdict {
    opacity: 0.7;
  }
  .scan-row.presence-published .mark { color: #10b981; }
  .scan-row.presence-notfound .mark { color: #ef4444; }
  .scan-row.presence-unknown .mark { color: #9ca3af; }

  .subhead {
    display: flex;
    align-items: baseline;
    gap: 0.5rem;
    margin: 0.6rem 0 0.5rem;
    font-size: 0.85rem;
    letter-spacing: 0.02em;
    opacity: 0.85;
    font-weight: 650;
  }
  .subhead .count {
    font-size: 0.72rem;
    opacity: 0.55;
    font-weight: 400;
  }
  .subhead .linklike {
    background: transparent;
    border: 0;
    color: var(--accent);
    cursor: pointer;
    padding: 0;
    font: inherit;
    font-size: 0.72rem;
    text-decoration: underline;
  }
  .subhead-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    margin-top: 1.1rem;
  }
  .contact-search {
    font: inherit;
    font-size: 0.82rem;
    padding: 0.35rem 0.65rem;
    border-radius: 0.45rem;
    border: 1px solid var(--border);
    background: color-mix(in oklab, var(--surface-2) 50%, transparent);
    color: inherit;
    min-width: 16rem;
  }

  .publish-result {
    grid-column: 1 / -1;
    margin-top: 0.6rem;
    padding: 0.6rem 0.75rem;
    border-radius: 0.55rem;
    background: color-mix(in oklab, var(--accent) 10%, transparent);
    border: 1px solid color-mix(in oklab, var(--accent) 35%, transparent);
    font-size: 0.78rem;
  }
  .publish-result p {
    margin: 0.25rem 0;
  }
  .publish-result .fine {
    opacity: 0.7;
    font-size: 0.72rem;
  }
  .pill.src { background: color-mix(in oklab, currentColor 10%, transparent); text-transform: none; letter-spacing: 0; font-weight: 500; }
  .fpr {
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 0.72rem;
    opacity: 0.7;
    margin-bottom: 0.25rem;
  }
  .meta {
    font-size: 0.72rem;
    opacity: 0.55;
  }
  .meta code {
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 0.9em;
  }
  .key-actions {
    display: flex;
    gap: 0.4rem;
    flex-shrink: 0;
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
  input, textarea {
    font: inherit;
    font-size: 0.85rem;
    padding: 0.7rem 0.8rem;
    border: 1px solid var(--border);
    border-radius: 0.85rem;
    background: color-mix(in oklab, var(--surface-2) 72%, transparent);
    color: inherit;
    font-weight: 400;
  }
  textarea {
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 0.78rem;
    resize: vertical;
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
  button[type='submit'] {
    align-self: flex-start;
    background: var(--accent);
    color: white;
    border-color: var(--accent);
  }
  button:disabled { opacity: 0.5; cursor: progress; }
  button.danger {
    border-color: color-mix(in oklab, crimson 35%, transparent);
    color: color-mix(in oklab, crimson 80%, currentColor 20%);
  }

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

  @media (max-width: 820px) {
    article.pgp-shell {
      padding: 1rem;
    }
    .hero {
      grid-template-columns: 1fr;
    }
    .hero-badges {
      justify-content: flex-start;
    }
    ul.keylist li {
      flex-direction: column;
    }
    .key-actions {
      width: 100%;
      justify-content: flex-start;
    }
  }
</style>
