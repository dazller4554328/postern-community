<script lang="ts">
  import { onMount } from 'svelte';
  import { api, type RestoreValidation, type BackupDestination, type CloudBackup } from '$lib/api';
  import { formatDate } from '$lib/format';

  /// Three phases mirror the API: stage (upload OR pick existing) →
  /// validate (password prompt) → apply (confirm + restart).
  type RestoreStep =
    | 'idle'
    | 'staging' // upload in flight OR copying server-side file
    | 'password'
    | 'validating'
    | 'review'
    | 'applying';

  interface Props {
    /// `step` is bindable so the parent can disable other UI (e.g.
    /// the per-row Restore buttons on the backups table) when a
    /// restore is mid-flight.
    step?: RestoreStep;
  }

  let { step = $bindable<RestoreStep>('idle') }: Props = $props();

  let restoreLabel = $state<string | null>(null);
  let restoreStagingId = $state<string | null>(null);
  let restorePassword = $state('');
  let restoreSummary = $state<RestoreValidation | null>(null);
  let restoreError = $state<string | null>(null);
  /// True while the `staging` step is a server-side Drive download
  /// (can take minutes) rather than a quick upload/hard-link, so the
  /// message can set expectations.
  let stagingFromCloud = $state(false);

  // --- Google Drive restore picker -----------------------------------
  let gdriveDests = $state<BackupDestination[]>([]);
  let cloudDestId = $state<number | null>(null);
  let cloudList = $state<CloudBackup[] | null>(null);
  let cloudLoading = $state(false);
  let cloudError = $state<string | null>(null);

  onMount(async () => {
    try {
      const all = await api.listBackupDestinations();
      gdriveDests = all.filter((d) => d.kind === 'gdrive');
    } catch {
      gdriveDests = [];
    }
  });

  async function browseCloud(destId: number, label: string) {
    cloudDestId = destId;
    cloudError = null;
    cloudLoading = true;
    cloudList = null;
    try {
      cloudList = await api.listCloudBackups(destId);
    } catch (e) {
      cloudError = e instanceof Error ? e.message : String(e);
    } finally {
      cloudLoading = false;
    }
    void label;
  }

  /// Restore a chosen Drive backup: the server downloads it into a
  /// staging dir, then we drop into the same password → apply flow.
  async function chooseCloudBackup(destId: number, fileId: string, name: string) {
    if (step !== 'idle') return;
    restoreLabel = name;
    restoreError = null;
    stagingFromCloud = true;
    step = 'staging';
    setTimeout(() => {
      document.getElementById('restore-section')?.scrollIntoView({
        behavior: 'smooth',
        block: 'start'
      });
    }, 50);
    try {
      const r = await api.restoreFromGdrive(destId, fileId);
      restoreStagingId = r.staging_id;
      step = 'password';
    } catch (e) {
      restoreError = e instanceof Error ? e.message : String(e);
      step = 'idle';
    } finally {
      stagingFromCloud = false;
    }
  }

  function cloudDate(rfc3339: string): string {
    const d = new Date(rfc3339);
    return Number.isNaN(d.getTime()) ? rfc3339 : d.toLocaleString();
  }

  /// Same human-byte formatter as the destination card; restore
  /// summary often tops 100MB so we want GB tier here too.
  function humanSize(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
    return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
  }

  function resetRestore() {
    step = 'idle';
    restoreLabel = null;
    restoreStagingId = null;
    restorePassword = '';
    restoreSummary = null;
    restoreError = null;
    stagingFromCloud = false;
    cloudList = null;
    cloudDestId = null;
    cloudError = null;
  }

  async function pickRestoreFile(e: Event) {
    const input = e.currentTarget as HTMLInputElement;
    const f = input.files?.[0] ?? null;
    if (!f) return;
    restoreLabel = f.name;
    restoreError = null;
    step = 'staging';
    try {
      const r = await api.uploadRestoreBackup(f);
      restoreStagingId = r.staging_id;
      step = 'password';
    } catch (e) {
      restoreError = e instanceof Error ? e.message : String(e);
      step = 'idle';
    }
  }

  /// Called by the parent when the user clicks Restore on an existing
  /// backup row. Skips the upload phase — the file is already on
  /// disk; the server hard-links it into a staging dir and we go
  /// straight to the password prompt.
  export async function startFromExisting(filename: string) {
    if (step !== 'idle') {
      // Don't start a second restore while one is in flight.
      return;
    }
    restoreLabel = filename;
    restoreError = null;
    step = 'staging';
    // Auto-scroll the restore section into view so the password
    // prompt isn't hidden below the fold.
    setTimeout(() => {
      document.getElementById('restore-section')?.scrollIntoView({
        behavior: 'smooth',
        block: 'start'
      });
    }, 50);
    try {
      const r = await api.restoreFromExistingBackup(filename);
      restoreStagingId = r.staging_id;
      step = 'password';
    } catch (e) {
      restoreError = e instanceof Error ? e.message : String(e);
      step = 'idle';
    }
  }

  async function submitRestorePassword() {
    if (!restoreStagingId || !restorePassword) return;
    restoreError = null;
    step = 'validating';
    try {
      restoreSummary = await api.validateRestoreBackup(restoreStagingId, restorePassword);
      step = 'review';
    } catch (e) {
      restoreError = e instanceof Error ? e.message : String(e);
      step = 'password';
    }
  }

  async function applyRestore() {
    if (!restoreStagingId) return;
    if (
      !confirm(
        'Apply this backup? Postern will restart immediately. Your current local data ' +
          'is moved aside (recoverable from .pre-restore-* on disk for one boot) and ' +
          'replaced by the contents of the backup. After restart you log in with the ' +
          'master password from when the backup was made.'
      )
    ) return;
    step = 'applying';
    restoreError = null;
    try {
      await api.applyRestoreBackup(restoreStagingId);
      // The server will exit ~2s after responding. There's no point
      // polling — connection drops and the orchestrator restarts us.
      // Show a static "restarting" panel; the user can refresh
      // manually or re-load the page once Postern comes back.
    } catch (e) {
      restoreError = e instanceof Error ? e.message : String(e);
      step = 'review';
    }
  }

  async function cancelRestore() {
    if (restoreStagingId) {
      try {
        await api.cancelRestoreBackup(restoreStagingId);
      } catch {
        // Ignore — staging dirs are also cleaned up on next backup.
      }
    }
    resetRestore();
  }
</script>

<div class="restore-section panel" id="restore-section">
  <h3>Restore from an off-site / uploaded backup</h3>
  <p class="muted">
    Use this if your backup file lives off-machine — on your laptop, an
    external drive, or anywhere else. To restore one of the backups in
    the table above, just click <strong>Restore</strong> on that row;
    no upload needed.
  </p>

  {#if step === 'idle'}
    <label class="file-pick">
      <input type="file" accept=".tar.gz,.gz,application/gzip" onchange={pickRestoreFile} />
      <span class="btn">Choose backup file…</span>
    </label>
    {#if restoreError}
      <div class="err" style="margin-top: 0.6rem;">⚠ {restoreError}</div>
    {/if}

    {#if gdriveDests.length > 0}
      <div class="cloud-restore">
        <h4>…or restore from Google Drive</h4>
        <p class="muted">
          Pull a backup straight from your Drive folder — the server
          downloads it directly, so you don't have to fetch a multi-GB
          file to this browser first.
        </p>
        <div class="cloud-dest-row">
          {#each gdriveDests as d (d.id)}
            <button class="btn" onclick={() => browseCloud(d.id, d.label)}>
              Browse “{d.label}”
            </button>
          {/each}
        </div>
        {#if cloudLoading}
          <p class="muted">Listing Drive backups…</p>
        {/if}
        {#if cloudError}
          <div class="err" style="margin-top: 0.6rem;">⚠ {cloudError}</div>
        {/if}
        {#if cloudList}
          {#if cloudList.length === 0}
            <p class="muted">No Postern backups found in that Drive folder.</p>
          {:else}
            <ul class="cloud-list">
              {#each cloudList as b (b.file_id)}
                <li>
                  <div class="cloud-meta">
                    <span class="cloud-name">{b.name}</span>
                    <span class="muted">
                      {b.size ? humanSize(b.size) : '—'} · {cloudDate(b.modified_time)}
                    </span>
                  </div>
                  <button
                    class="btn primary"
                    onclick={() => chooseCloudBackup(cloudDestId!, b.file_id, b.name)}
                  >
                    Restore
                  </button>
                </li>
              {/each}
            </ul>
          {/if}
        {/if}
      </div>
    {/if}
  {:else if step === 'staging'}
    <p class="muted">
      {#if stagingFromCloud}
        Downloading <strong>{restoreLabel}</strong> from Google Drive to the
        server… this can take a few minutes for a large backup. Leave this
        tab open.
      {:else}
        Preparing <strong>{restoreLabel}</strong>…
      {/if}
    </p>
  {:else if step === 'password'}
    <p>
      Selected <strong>{restoreLabel}</strong>. Enter the
      <strong>master password from when this backup was made</strong> to verify
      the file isn't corrupt and matches a real Postern install.
    </p>
    <form
      class="password-row"
      onsubmit={(e: SubmitEvent) => {
        e.preventDefault();
        void submitRestorePassword();
      }}
    >
      <input
        type="password"
        autocomplete="off"
        placeholder="master password used to encrypt the backup"
        bind:value={restorePassword}
      />
      <button type="submit" class="btn primary" disabled={!restorePassword}>Verify</button>
      <button type="button" class="btn ghost" onclick={cancelRestore}>Cancel</button>
    </form>
    {#if restoreError}
      <div class="err" style="margin-top: 0.6rem;">⚠ {restoreError}</div>
    {/if}
  {:else if step === 'validating'}
    <p class="muted">Decrypting and counting rows… (may take a minute on large backups)</p>
  {:else if step === 'review'}
    {#if restoreSummary}
      <div class="summary">
        <h4>Backup verified</h4>
        <ul>
          <li><strong>{restoreSummary.accounts}</strong> mailbox{restoreSummary.accounts === 1 ? '' : 'es'}</li>
          <li><strong>{restoreSummary.messages.toLocaleString()}</strong> messages</li>
          <li><strong>{restoreSummary.blobs.toLocaleString()}</strong> blob files (bodies + attachments)</li>
          <li>File size: {humanSize(restoreSummary.size_bytes)}</li>
          <li>Backup date: {formatDate(restoreSummary.created_at)}</li>
        </ul>
        <p class="warn-line">
          ⚠ Applying this backup will <strong>replace</strong> your current
          mailbox data. Your existing data is moved to
          <code>.pre-restore-*</code> on disk for one-boot recovery, then
          cleaned up after the next successful boot.
        </p>
        <p class="warn-line">
          After restart, log in with the master password from when this
          backup was made (the one you just typed above).
        </p>
        <div class="action-row">
          <button class="btn primary" onclick={applyRestore}>Apply backup &amp; restart</button>
          <button class="btn ghost" onclick={cancelRestore}>Cancel</button>
        </div>
      </div>
    {/if}
    {#if restoreError}
      <div class="err" style="margin-top: 0.6rem;">⚠ {restoreError}</div>
    {/if}
  {:else if step === 'applying'}
    <div class="applying">
      <h4>Restoring — Postern is restarting…</h4>
      <p>
        The server is exiting and your orchestrator should bring it back
        within a few seconds. Once it's up, refresh this page and log
        in with the master password from your backup.
      </p>
    </div>
  {/if}
</div>

<style>
  /* Shared classes (.panel, .btn, .muted) come from the parent
     under :global(). Restore-section visuals stay scoped here. */
  .restore-section { margin-top: 2rem; padding: 1.2rem 1.4rem 1.4rem; }
  .restore-section h3 { margin: 0 0 0.5rem; font-size: 0.95rem; font-weight: 650; letter-spacing: -0.01em; }
  .restore-section h4 { margin: 0 0 0.5rem; font-size: 0.88rem; font-weight: 600; }
  .restore-section p { font-size: 0.85rem; line-height: 1.55; margin: 0 0 0.65rem; }
  .restore-section .muted { font-size: 0.85rem; }

  .cloud-restore {
    margin-top: 1.4rem;
    padding-top: 1.1rem;
    border-top: 1px solid var(--border);
  }
  .cloud-restore h4 { margin: 0 0 0.4rem; font-size: 0.88rem; font-weight: 600; }
  .cloud-dest-row { display: flex; gap: 0.5rem; flex-wrap: wrap; margin: 0.5rem 0; }
  .cloud-list { list-style: none; margin: 0.6rem 0 0; padding: 0; }
  .cloud-list li {
    display: flex; align-items: center; justify-content: space-between;
    gap: 0.75rem; padding: 0.55rem 0.7rem;
    border: 1px solid var(--border); border-radius: 0.45rem;
    margin-bottom: 0.4rem;
  }
  .cloud-meta { display: flex; flex-direction: column; gap: 0.1rem; min-width: 0; }
  .cloud-name {
    font-size: 0.82rem; font-weight: 550;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .cloud-meta .muted { font-size: 0.74rem; }

  .file-pick { display: inline-block; cursor: pointer; }
  .file-pick input[type='file'] {
    position: absolute; width: 1px; height: 1px; opacity: 0;
    overflow: hidden; clip: rect(0 0 0 0);
  }

  .password-row {
    display: flex; gap: 0.5rem; align-items: center;
    margin-top: 0.65rem; flex-wrap: wrap;
  }
  .password-row input[type='password'] {
    font: inherit; padding: 0.5rem 0.75rem; min-width: 18rem;
    border: 1px solid var(--border); border-radius: 0.4rem;
    background: var(--surface);
  }

  .summary {
    margin-top: 0.9rem; padding: 0.9rem 1.1rem;
    background: color-mix(in oklab, var(--accent) 6%, transparent);
    border: 1px solid color-mix(in oklab, var(--accent) 25%, transparent);
    border-radius: 0.6rem;
  }
  .summary ul { margin: 0 0 0.85rem; padding-left: 1.25rem; font-size: 0.85rem; line-height: 1.7; }
  .warn-line {
    font-size: 0.82rem; line-height: 1.55;
    color: color-mix(in oklab, currentColor 80%, transparent);
  }
  .warn-line code { font-family: ui-monospace, monospace; font-size: 0.78rem; }
  .action-row { display: flex; gap: 0.5rem; flex-wrap: wrap; margin-top: 0.85rem; }

  .applying {
    margin-top: 0.9rem; padding: 1rem 1.2rem;
    background: color-mix(in oklab, gold 8%, transparent);
    border-left: 3px solid color-mix(in oklab, gold 60%, currentColor);
    border-radius: 0 0.4rem 0.4rem 0;
  }
  .applying h4 { font-size: 0.9rem; font-weight: 600; margin: 0 0 0.4rem; }
  .applying p { font-size: 0.82rem; }

  .err {
    position: relative;
    margin-top: 0.75rem; padding: 0.55rem 2rem 0.55rem 0.75rem;
    background: color-mix(in oklab, crimson 10%, transparent);
    border-left: 2px solid crimson;
    border-radius: 0 0.3rem 0.3rem 0;
    font-size: 0.85rem;
  }
</style>
