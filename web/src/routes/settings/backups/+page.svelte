<script lang="ts">
  import './backups.css';
  import { onDestroy, onMount } from 'svelte';
  import {
    api,
    type BackupDestination,
    type BackupJob,
    type BackupReport,
    type Integrations
  } from '$lib/api';
  import { formatDate } from '$lib/format';
  import AddBackupDestination from './_components/AddBackupDestination.svelte';
  import BackupDestinationCard from './_components/BackupDestinationCard.svelte';
  import BackupScheduleForm from './_components/BackupScheduleForm.svelte';
  import RestoreFlow from './_components/RestoreFlow.svelte';

  let backups = $state<BackupReport[]>([]);
  let loading = $state(true);
  let creating = $state(false);
  let err = $state<string | null>(null);
  let lastReport = $state<BackupReport | null>(null);
  /// Live status of the current/most-recent backup job. Polled while
  /// `running`; cleared once the user dismisses a finished result so
  /// it doesn't linger on the page indefinitely.
  let backupJob = $state<BackupJob | null>(null);
  let backupPoller: ReturnType<typeof setInterval> | null = null;

  // Restore-flow state + handlers live in <RestoreFlow />. Parent
  // tracks only the `restoreStep` for cross-section UI (disabling
  // the per-row Restore buttons on the local-backups table during a
  // restore-in-flight). The child mutates step bidirectionally.
  let restoreStep = $state<'idle' | 'staging' | 'password' | 'validating' | 'review' | 'applying'>('idle');
  let restoreFlow: RestoreFlow | undefined = $state(undefined);

  // ---- Off-site destinations ----
  // Per-destination busy + flash state lives inside BackupDestinationCard
  // now; the parent only tracks the list itself.
  let destinations = $state<BackupDestination[]>([]);
  let integrations = $state<Integrations | null>(null);
  // Add-destination state + handlers + OAuth-callback flashes live
  // inside <AddBackupDestination />.

  onMount(async () => {
    try {
      backups = await api.listBackups();
      const status = await api.getBackupStatus();
      if (status) {
        backupJob = status;
        if (status.state === 'running') {
          startBackupPoll();
        } else if (status.state === 'success') {
          lastReport = status.report;
        }
      }
      destinations = await api.listBackupDestinations();
      integrations = await api.backupIntegrations();
      // BackupScheduleForm loads + manages its own schedule.
      // AddBackupDestination picks up its own OAuth callback flashes
      // (?gdrive_connected / ?gdrive_error) on mount.
    } catch {} finally {
      loading = false;
    }
  });

  async function refreshDestinations() {
    try {
      destinations = await api.listBackupDestinations();
    } catch (e) {
      console.warn('refreshDestinations failed:', e);
    }
  }

  onDestroy(() => stopBackupPoll());

  function startBackupPoll() {
    if (backupPoller) return;
    backupPoller = setInterval(async () => {
      try {
        const status = await api.getBackupStatus();
        if (!status) {
          stopBackupPoll();
          return;
        }
        backupJob = status;
        if (status.state !== 'running') {
          stopBackupPoll();
          if (status.state === 'success') {
            lastReport = status.report;
            backups = await api.listBackups();
          } else if (status.state === 'failed') {
            err = status.error ?? 'backup failed';
          }
          creating = false;
        }
      } catch {
        // Transient error — keep polling.
      }
    }, 2000);
  }

  function stopBackupPoll() {
    if (backupPoller) {
      clearInterval(backupPoller);
      backupPoller = null;
    }
  }

  async function create() {
    creating = true;
    err = null;
    lastReport = null;
    backupJob = null;
    try {
      backupJob = await api.createBackup();
      // The handler returns immediately with state=running. Poll for
      // progress; the UI shows a "creating…" panel while it runs.
      startBackupPoll();
    } catch (e) {
      err = e instanceof Error ? e.message : String(e);
      creating = false;
    }
  }

  function dismissBackupStatus() {
    backupJob = null;
    lastReport = null;
    err = null;
  }

  async function remove(filename: string) {
    if (!confirm(`Delete backup ${filename}?`)) return;
    try {
      await api.deleteBackup(filename);
      backups = backups.filter(b => b.filename !== filename);
    } catch (e) {
      err = e instanceof Error ? e.message : String(e);
    }
  }

  function humanSize(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
    return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
  }

</script>

<article class="backups-shell">
  <div class="page-top">
    <a class="back" href="/settings">← Settings</a>
  </div>
  <header class="hero">
    <div class="hero-copy">
      <span class="eyebrow">Recovery</span>
      <h1>Backups</h1>
      <p>
        Create point-in-time snapshots of the encrypted database, vault metadata,
        and all message blobs. Backups live on the server as compressed tarballs.
      </p>
    </div>
    <div class="hero-badges">
      <span class="hero-chip">Server-side archives</span>
      <span class="hero-chip">Blob-inclusive</span>
      <span class="hero-chip">Restore workflow documented</span>
    </div>
  </header>

  <BackupScheduleForm />

  <div class="create-section panel">
    <button class="create-btn" onclick={create} disabled={creating || backupJob?.state === 'running'}>
      {#if backupJob?.state === 'running'}
        Backup running…
      {:else if creating}
        Starting…
      {:else}
        Create backup now
      {/if}
    </button>

    {#if backupJob?.state === 'running'}
      <div class="job-status running">
        <strong>Backing up…</strong>
        VACUUM + blob copy + gzip on the data dir. This runs in the background;
        you can leave this page and come back. Started {formatDate(backupJob.started_at)}.
      </div>
    {:else if backupJob?.state === 'success' && lastReport}
      <div class="success">
        Backup created: <strong>{lastReport.filename}</strong> — {humanSize(lastReport.size_bytes)}
        ({lastReport.blob_count} blobs)
        <button class="dismiss" onclick={dismissBackupStatus} aria-label="Dismiss">×</button>
      </div>
    {:else if backupJob?.state === 'failed'}
      <div class="err">
        ⚠ Backup failed: {backupJob.error ?? 'unknown error'}
        <button class="dismiss" onclick={dismissBackupStatus} aria-label="Dismiss">×</button>
      </div>
    {:else if err}
      <div class="err">⚠ {err}</div>
    {/if}
  </div>

  {#if loading}
    <p class="muted">Loading…</p>
  {:else if backups.length === 0}
    <p class="muted">No backups yet.</p>
  {:else}
    <div class="table-wrap panel">
    <table>
      <thead>
        <tr>
          <th>Filename</th>
          <th>Size</th>
          <th>Created</th>
          <th>Actions</th>
        </tr>
      </thead>
      <tbody>
        {#each backups as b (b.filename)}
          <tr>
            <td class="name"><code>{b.filename}</code></td>
            <td>{humanSize(b.size_bytes)}</td>
            <td>{b.created_at ? formatDate(b.created_at) : '—'}</td>
            <td class="row-actions">
              <a
                class="row-btn"
                href={api.backupDownloadUrl(b.filename)}
                download={b.filename}
                title="Download tarball to your computer"
              >
                Download
              </a>
              <button
                class="row-btn primary-ish"
                onclick={() => void restoreFlow?.startFromExisting(b.filename)}
                disabled={restoreStep !== 'idle' || backupJob?.state === 'running'}
                title="Restore from this backup"
              >
                Restore
              </button>
              <button class="del" onclick={() => remove(b.filename)} title="Delete this backup">
                Delete
              </button>
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
    </div>
  {/if}

  <div class="destinations panel">
    <div class="dest-head">
      <h3>Off-site destinations</h3>
      <AddBackupDestination {integrations} onAdded={refreshDestinations} />
    </div>
    <p class="muted">
      After every successful backup, Postern uploads the tarball to each
      enabled destination. The local copy in the table above is always
      written first; off-site failures are logged but never fail the
      local backup. Credentials are vault-encrypted at rest.
    </p>

    {#if destinations.length === 0}
      <p class="muted">
        No destinations yet. Backups stay on this server only — if the
        VPS dies, both the live data and the backup go with it.
      </p>
    {:else if destinations.length > 0}
      <ul class="dest-list">
        {#each destinations as d (d.id)}
          <BackupDestinationCard destination={d} onChanged={refreshDestinations} />
        {/each}
      </ul>
    {/if}
  </div>

  <RestoreFlow bind:this={restoreFlow} bind:step={restoreStep} />
</article>

