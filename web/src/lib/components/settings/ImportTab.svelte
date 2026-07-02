<script lang="ts">
  import { api, type Account } from '$lib/api';
  import InfoBubble from '$lib/components/InfoBubble.svelte';

  // Parent owns the accounts list (also displayed in other tabs);
  // this tab just reads it for the target-mailbox dropdown.
  let { accounts }: { accounts: Account[] } = $props();

  // Mail import — state local to this tab. One file at a time to
  // keep the UX obvious; multi-file drops extract archive-style.
  let importFile = $state<File | null>(null);
  let importAccountId = $state<number | null>(null);
  let importBusy = $state(false);
  let importError = $state<string | null>(null);
  let importResult = $state<{ scanned: number; imported: number; skipped: number; errors: number } | null>(null);
  let importDragActive = $state(false);

  // Path-based import. The server walks
  // `${POSTERN_IMPORT_DIR}/${path}` (sandboxed to the configured
  // import root) and upserts every RFC822 file it finds. Dedups by
  // Message-ID — safe to re-run.
  let importPath = $state('');
  let pathImportBusy = $state(false);

  function chooseImportFile(f: File | null) {
    importFile = f;
    importResult = null;
    importError = null;
  }

  function onImportDrop(e: DragEvent) {
    e.preventDefault();
    importDragActive = false;
    const f = e.dataTransfer?.files?.[0];
    if (f) chooseImportFile(f);
  }

  async function runImport() {
    if (!importFile) {
      importError = 'Pick a file first.';
      return;
    }
    importBusy = true;
    importError = null;
    importResult = null;
    try {
      const name = importFile.name.toLowerCase();
      // Route by extension. .zip → archive-zip, everything else → mbox
      // (covers .mbox, .mbx, Gmail Takeout's "All mail Including Spam and Trash.mbox",
      // and bare files without extensions).
      const report = name.endsWith('.zip')
        ? await api.importArchiveZip(importFile, importAccountId ?? undefined)
        : await api.importMbox(importFile, importAccountId ?? undefined);
      importResult = report;
    } catch (e) {
      importError = e instanceof Error ? e.message : String(e);
    } finally {
      importBusy = false;
    }
  }

  async function runPathImport() {
    if (pathImportBusy) return;
    pathImportBusy = true;
    importError = null;
    importResult = null;
    try {
      const report = await api.importMaildirPath(
        importPath.trim(),
        importAccountId ?? undefined
      );
      importResult = report;
    } catch (e) {
      importError = e instanceof Error ? e.message : String(e);
    } finally {
      pathImportBusy = false;
    }
  }
</script>

<section class="panel">
  <div class="section-head">
    <h2>Import mail</h2>
    <p>
      Migrate from Gmail Takeout, Thunderbird, Apple Mail, or any
      tool that exports mbox or Maildir. Messages are parsed locally
      and written straight into Postern's encrypted store — nothing
      leaves this machine.
    </p>
  </div>

  <div class="field">
    <div class="field-label">
      <label for="import-account">Target mailbox</label>
      <InfoBubble text="Pick a mailbox to tag every imported message with. Leaving this on auto-detect scans Delivered-To / To / Cc headers against your configured accounts and skips any message that doesn't match — useful when importing an mbox that spans several identities." />
    </div>
    <select
      id="import-account"
      class="std-select"
      value={importAccountId ?? ''}
      onchange={(e) => {
        const v = (e.currentTarget as HTMLSelectElement).value;
        importAccountId = v === '' ? null : Number(v);
      }}
      disabled={importBusy}
    >
      <option value="">Auto-detect from headers</option>
      {#each accounts as a (a.id)}
        <option value={a.id}>{a.email}</option>
      {/each}
    </select>
  </div>

  <div class="field">
    <div class="field-label">
      <label>File</label>
      <InfoBubble text="Accepts a single .mbox file (Gmail Takeout, Thunderbird's Local Folders), or a .zip containing a Maildir tree or a folder of .eml files. 2 GB cap per upload — split bigger archives with `mbox split` or upload them in batches." />
    </div>
    <div
      class="import-drop"
      class:drag-over={importDragActive}
      role="button"
      tabindex="0"
      aria-label="Drop a file here or click to choose"
      ondragenter={(e) => { e.preventDefault(); importDragActive = true; }}
      ondragover={(e) => e.preventDefault()}
      ondragleave={() => (importDragActive = false)}
      ondrop={onImportDrop}
      onclick={() => document.getElementById('import-file-input')?.click()}
      onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); document.getElementById('import-file-input')?.click(); } }}
    >
      {#if importFile}
        <strong>{importFile.name}</strong>
        <span class="muted">{(importFile.size / (1024 * 1024)).toFixed(1)} MB</span>
      {:else}
        <span class="muted">Drop an .mbox or .zip here, or click to choose</span>
      {/if}
    </div>
    <input
      id="import-file-input"
      type="file"
      accept=".mbox,.mbx,.zip,application/mbox,application/zip"
      style="display: none"
      onchange={(e) => {
        const f = (e.currentTarget as HTMLInputElement).files?.[0] ?? null;
        chooseImportFile(f);
      }}
    />
  </div>

  <div class="actions">
    <button
      class="btn primary"
      type="button"
      disabled={importBusy || !importFile}
      onclick={runImport}
    >
      {importBusy ? 'Importing…' : 'Import'}
    </button>
    {#if importFile && !importBusy}
      <button class="btn ghost" type="button" onclick={() => chooseImportFile(null)}>
        Clear
      </button>
    {/if}
  </div>

  <div class="field" style="margin-top: 1.4rem;">
    <div class="field-label">
      <label for="import-path">Or import from server path</label>
      <InfoBubble text="Walks a directory on the server (sandboxed to the configured POSTERN_IMPORT_DIR bind-mount) and imports every RFC822 file it finds. Dedups by Message-ID — safe to re-run. Use this for recovery from on-server stores like Mailpile, Dovecot, etc. Leave blank for the bind-mount root, or specify a subdirectory like 'INBOX' or 'archive/2024'." />
    </div>
    <div class="seed-row">
      <input
        id="import-path"
        type="text"
        class="std-input"
        placeholder="leave blank to import everything"
        bind:value={importPath}
        disabled={pathImportBusy}
      />
      <button
        type="button"
        class="btn"
        onclick={runPathImport}
        disabled={pathImportBusy || importBusy}
      >
        {pathImportBusy ? 'Walking + importing…' : 'Import path'}
      </button>
    </div>
    <p class="field-help">
      Path is relative to the server's import bind-mount
      (<code>POSTERN_IMPORT_DIR</code> in <code>/opt/postern/.env</code>).
      Leave blank to walk the whole tree, or type a sub-path
      (e.g. <code>02da1</code>) to import a single sub-folder.
      The walker recurses — large stores can take a few minutes.
    </p>
  </div>

  {#if importError}
    <p class="field-help" style="color: var(--c-danger, #b91c1c);">
      <strong>Import failed:</strong> {importError}
    </p>
  {/if}

  {#if importResult}
    <div class="import-report">
      <div><strong>Scanned:</strong> {importResult.scanned}</div>
      <div><strong>Imported:</strong> {importResult.imported}</div>
      <div><strong>Skipped:</strong> {importResult.skipped}
        {#if importResult.skipped > 0}
          <span class="muted">(already in DB or no matching account)</span>
        {/if}
      </div>
      <div><strong>Errors:</strong> {importResult.errors}</div>
      {#if importResult.imported > 0}
        <p class="field-help">
          New messages are visible in Inbox — they're labelled INBOX
          on import. Move them into folders via the normal archive /
          label actions once you've had a look.
        </p>
      {/if}
    </div>
  {/if}

  <div class="field" style="margin-top: 1.5rem;">
    <div class="field-label">
      <label>Where to find exports</label>
    </div>
    <ul class="muted" style="padding-left: 1.2rem; line-height: 1.6;">
      <li><strong>Gmail:</strong> takeout.google.com → pick Mail → export as .mbox. Arrives as a zip with an mbox file inside — unzip first, then upload the .mbox.</li>
      <li><strong>Thunderbird:</strong> profile folder → <code>Mail/Local Folders/</code>. Each file (no extension) is an mbox; rename to <code>.mbox</code> and upload. Or zip the whole <code>Mail/</code> tree.</li>
      <li><strong>Apple Mail:</strong> Mailbox → Export → produces a <code>.mbox</code> package (actually a folder). Zip it first.</li>
      <li><strong>Mailpile / Claws / mutt:</strong> Maildir — zip the <code>cur/ new/ tmp/</code> tree.</li>
    </ul>
  </div>
</section>
