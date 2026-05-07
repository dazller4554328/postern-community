<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import {
    api,
    type BackupDestination,
    type BackupJob,
    type BackupReport,
    type BackupSchedule,
    type GDrivePublicConfig,
    type Integrations,
    type NewSftpDestination,
    type RestoreValidation,
    type SftpPublicConfig
  } from '$lib/api';
  import { formatDate } from '$lib/format';

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

  // ---- Schedule ----
  let schedule = $state<BackupSchedule | null>(null);
  let scheduleDraft = $state<{
    enabled: boolean;
    frequency: 'daily' | 'weekly';
    hour: number;
    minute: number;
    day_of_week: number;
    retention_count: number;
  } | null>(null);
  let scheduleSaving = $state(false);
  let scheduleSaved = $state(false);
  const DOW_NAMES = ['Sunday', 'Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday'];
  function scheduleDirty(): boolean {
    if (!schedule || !scheduleDraft) return false;
    return (
      scheduleDraft.enabled !== schedule.enabled ||
      scheduleDraft.frequency !== schedule.frequency ||
      scheduleDraft.hour !== schedule.hour ||
      scheduleDraft.minute !== schedule.minute ||
      scheduleDraft.day_of_week !== schedule.day_of_week ||
      scheduleDraft.retention_count !== schedule.retention_count
    );
  }
  async function saveSchedule() {
    if (!scheduleDraft) return;
    scheduleSaving = true;
    try {
      schedule = await api.setBackupSchedule(scheduleDraft);
      scheduleDraft = { ...scheduleDraft }; // refresh from saved
      scheduleSaved = true;
      setTimeout(() => (scheduleSaved = false), 2000);
    } catch (e) {
      alert(e instanceof Error ? e.message : String(e));
    } finally {
      scheduleSaving = false;
    }
  }
  function pad2(n: number): string {
    return n.toString().padStart(2, '0');
  }

  // ---- Restore flow state ----
  // Three phases mirror the API: stage (upload OR pick existing) →
  // validate (password prompt) → apply (confirm + restart).
  // `restoreStep` drives which UI block renders; `restoreLabel` is
  // the displayable filename so both staging paths share the
  // password/review UI without branching on source-of-file.
  type RestoreStep =
    | 'idle'
    | 'staging' // upload in flight OR copying server-side file
    | 'password'
    | 'validating'
    | 'review'
    | 'applying';
  let restoreStep = $state<RestoreStep>('idle');
  let restoreLabel = $state<string | null>(null);
  let restoreStagingId = $state<string | null>(null);
  let restorePassword = $state('');
  let restoreSummary = $state<RestoreValidation | null>(null);
  let restoreError = $state<string | null>(null);

  // ---- Off-site destinations ----
  let destinations = $state<BackupDestination[]>([]);
  let destBusy = $state<Record<number, boolean>>({});
  let destFlash = $state<Record<number, string>>({});
  let integrations = $state<Integrations | null>(null);
  /// Surfaced from the OAuth callback's redirect query string —
  /// "Connected gdrive as you@example.com" or "Google rejected the
  /// request: …". Cleared on dismiss.
  let gdriveConnectedFlash = $state<string | null>(null);
  let gdriveErrorFlash = $state<string | null>(null);
  let showAddDest = $state(false);
  let showAddGdrive = $state(false);
  let gdriveLabel = $state('');
  let addingDest = $state(false);
  let addDestError = $state<string | null>(null);
  let newDest = $state({
    label: '',
    host: '',
    port: 22,
    username: '',
    remote_dir: '',
    auth: 'password' as 'password' | 'key',
    password: '',
    key_pem: '',
    passphrase: ''
  });

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
      schedule = await api.getBackupSchedule();
      scheduleDraft = {
        enabled: schedule.enabled,
        frequency: schedule.frequency,
        hour: schedule.hour,
        minute: schedule.minute,
        day_of_week: schedule.day_of_week,
        retention_count: schedule.retention_count
      };
      // Pick up gdrive_connected / gdrive_error flash from the OAuth
      // redirect target.
      const url = new URL(window.location.href);
      const connected = url.searchParams.get('gdrive_connected');
      const errored = url.searchParams.get('gdrive_error');
      if (connected) {
        gdriveConnectedFlash = connected;
        url.searchParams.delete('gdrive_connected');
        history.replaceState({}, '', url.toString());
      }
      if (errored) {
        gdriveErrorFlash = errored;
        url.searchParams.delete('gdrive_error');
        history.replaceState({}, '', url.toString());
      }
    } catch {} finally {
      loading = false;
    }
  });

  function startGdriveConnect() {
    if (!gdriveLabel.trim()) {
      alert('Please enter a label first.');
      return;
    }
    // Full-page navigation — Google's consent screen won't render in
    // an iframe, and we need their final redirect to land back on us
    // with the same browser session cookies.
    window.location.href = api.gdriveOauthStartUrl(gdriveLabel.trim());
  }

  /// Type narrowing helpers — tagged by `kind` at runtime.
  function asSftp(d: BackupDestination): SftpPublicConfig {
    return d.public_config as SftpPublicConfig;
  }
  function asGdrive(d: BackupDestination): GDrivePublicConfig {
    return d.public_config as GDrivePublicConfig;
  }

  async function refreshDestinations() {
    try {
      destinations = await api.listBackupDestinations();
    } catch (e) {
      destFlash = { ...destFlash, 0: e instanceof Error ? e.message : String(e) };
    }
  }

  async function toggleDestEnabled(d: BackupDestination) {
    destBusy = { ...destBusy, [d.id]: true };
    try {
      await api.updateBackupDestination(d.id, { enabled: !d.enabled });
      await refreshDestinations();
    } catch (e) {
      alert(e instanceof Error ? e.message : String(e));
    } finally {
      destBusy = { ...destBusy, [d.id]: false };
    }
  }

  async function testDest(d: BackupDestination) {
    destBusy = { ...destBusy, [d.id]: true };
    destFlash = { ...destFlash, [d.id]: '' };
    try {
      const r = await api.testBackupDestination(d.id);
      // Response shape branches on kind. SFTP carries TOFU fingerprint
      // info; GDrive carries the verified folder + account.
      if (d.kind === 'sftp' && r.fingerprint) {
        const fpShort = r.fingerprint.length > 30
          ? r.fingerprint.slice(0, 30) + '…'
          : r.fingerprint;
        destFlash = {
          ...destFlash,
          [d.id]: r.first_use
            ? `✓ connected. Pinned hostkey ${fpShort} for future connects.`
            : `✓ connected. Hostkey verified (${fpShort}).`
        };
        if (r.first_use) await refreshDestinations();
      } else if (d.kind === 'gdrive') {
        destFlash = {
          ...destFlash,
          [d.id]: `✓ connected. Drive folder "${r.folder_name ?? 'Postern Backups'}" reachable.`
        };
      } else {
        destFlash = { ...destFlash, [d.id]: '✓ connected.' };
      }
    } catch (e) {
      destFlash = {
        ...destFlash,
        [d.id]: '⚠ ' + (e instanceof Error ? e.message : String(e))
      };
    } finally {
      destBusy = { ...destBusy, [d.id]: false };
    }
  }

  async function forgetDestFingerprint(d: BackupDestination) {
    if (
      !confirm(
        `Forget the pinned hostkey for "${d.label}"? The next connect will accept ` +
          `whatever key the server presents and re-pin. Only do this if you ` +
          `legitimately rotated the SSH host key on the destination box.`
      )
    ) return;
    destBusy = { ...destBusy, [d.id]: true };
    try {
      await api.forgetBackupDestinationFingerprint(d.id);
      destFlash = {
        ...destFlash,
        [d.id]: '↻ fingerprint forgotten — next test/push will TOFU-pin a new key'
      };
      await refreshDestinations();
    } catch (e) {
      alert(e instanceof Error ? e.message : String(e));
    } finally {
      destBusy = { ...destBusy, [d.id]: false };
    }
  }

  async function pushLatestToDest(d: BackupDestination) {
    if (!confirm(`Push the most recent backup to ${d.label}?`)) return;
    destBusy = { ...destBusy, [d.id]: true };
    destFlash = { ...destFlash, [d.id]: '' };
    try {
      const r = await api.pushBackupDestination(d.id);
      destFlash = {
        ...destFlash,
        [d.id]: `✓ pushed ${humanSize(r.bytes_uploaded)} → ${r.remote_path}`
      };
      await refreshDestinations();
    } catch (e) {
      destFlash = {
        ...destFlash,
        [d.id]: '⚠ ' + (e instanceof Error ? e.message : String(e))
      };
      await refreshDestinations();
    } finally {
      destBusy = { ...destBusy, [d.id]: false };
    }
  }

  async function removeDest(d: BackupDestination) {
    if (!confirm(`Remove destination "${d.label}"? This deletes the credentials too.`)) return;
    try {
      await api.deleteBackupDestination(d.id);
      await refreshDestinations();
    } catch (e) {
      alert(e instanceof Error ? e.message : String(e));
    }
  }

  function resetNewDest() {
    newDest = {
      label: '',
      host: '',
      port: 22,
      username: '',
      remote_dir: '',
      auth: 'password',
      password: '',
      key_pem: '',
      passphrase: ''
    };
    addDestError = null;
  }

  async function addDest() {
    if (!newDest.label.trim() || !newDest.host.trim() || !newDest.username.trim() || !newDest.remote_dir.trim()) {
      addDestError = 'Label, host, username, and remote dir are all required.';
      return;
    }
    if (newDest.auth === 'password' && !newDest.password) {
      addDestError = 'Password is required for password auth.';
      return;
    }
    if (newDest.auth === 'key' && !newDest.key_pem.trim()) {
      addDestError = 'Private key is required for key auth.';
      return;
    }
    addingDest = true;
    addDestError = null;
    const body: NewSftpDestination = {
      label: newDest.label,
      kind: 'sftp',
      sftp: {
        host: newDest.host,
        port: newDest.port,
        username: newDest.username,
        remote_dir: newDest.remote_dir,
        auth: newDest.auth,
        ...(newDest.auth === 'password'
          ? { password: newDest.password }
          : {
              key_pem: newDest.key_pem,
              passphrase: newDest.passphrase || undefined
            })
      }
    };
    try {
      // The server tests the connection before persisting, so a
      // success here means the destination is verified.
      await api.createBackupDestination(body);
      showAddDest = false;
      resetNewDest();
      await refreshDestinations();
    } catch (e) {
      addDestError = e instanceof Error ? e.message : String(e);
    } finally {
      addingDest = false;
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

  function resetRestore() {
    restoreStep = 'idle';
    restoreLabel = null;
    restoreStagingId = null;
    restorePassword = '';
    restoreSummary = null;
    restoreError = null;
  }

  async function pickRestoreFile(e: Event) {
    const input = e.currentTarget as HTMLInputElement;
    const f = input.files?.[0] ?? null;
    if (!f) return;
    restoreLabel = f.name;
    restoreError = null;
    restoreStep = 'staging';
    try {
      const r = await api.uploadRestoreBackup(f);
      restoreStagingId = r.staging_id;
      restoreStep = 'password';
    } catch (e) {
      restoreError = e instanceof Error ? e.message : String(e);
      restoreStep = 'idle';
    }
  }

  /// Click handler for the "Restore" button on a backups-table row.
  /// Skips the upload phase — the file is already on disk; the server
  /// hard-links it into a staging dir and we go straight to the
  /// password prompt.
  async function restoreFromRow(b: BackupReport) {
    if (
      backupJob?.state === 'running' ||
      restoreStep !== 'idle'
    ) {
      // Don't let the user start a second restore while one is in flight.
      return;
    }
    restoreLabel = b.filename;
    restoreError = null;
    restoreStep = 'staging';
    // Auto-scroll the restore section into view so the password
    // prompt isn't hidden below the fold.
    setTimeout(() => {
      document.getElementById('restore-section')?.scrollIntoView({
        behavior: 'smooth',
        block: 'start'
      });
    }, 50);
    try {
      const r = await api.restoreFromExistingBackup(b.filename);
      restoreStagingId = r.staging_id;
      restoreStep = 'password';
    } catch (e) {
      restoreError = e instanceof Error ? e.message : String(e);
      restoreStep = 'idle';
    }
  }

  async function submitRestorePassword() {
    if (!restoreStagingId || !restorePassword) return;
    restoreError = null;
    restoreStep = 'validating';
    try {
      restoreSummary = await api.validateRestoreBackup(restoreStagingId, restorePassword);
      restoreStep = 'review';
    } catch (e) {
      restoreError = e instanceof Error ? e.message : String(e);
      restoreStep = 'password';
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
    restoreStep = 'applying';
    restoreError = null;
    try {
      await api.applyRestoreBackup(restoreStagingId);
      // The server will exit ~2s after responding. There's no point
      // polling — connection drops and the orchestrator restarts us.
      // Show a static "restarting" panel; the user can refresh
      // manually or re-load the page once Postern comes back.
    } catch (e) {
      restoreError = e instanceof Error ? e.message : String(e);
      restoreStep = 'review';
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

  {#if scheduleDraft}
    <div class="schedule-section panel">
      <h3>Automatic backups</h3>
      <p class="muted">
        Postern fires a backup at the chosen time. Off-site destinations
        get pushed automatically afterwards. Retention prunes old local
        tarballs once the new one is written.
      </p>
      <div class="sched-grid">
        <label class="sched-field">
          <span class="sched-label">Frequency</span>
          <select bind:value={scheduleDraft.frequency}>
            <option value="daily">Daily</option>
            <option value="weekly">Weekly</option>
          </select>
        </label>
        {#if scheduleDraft.frequency === 'weekly'}
          <label class="sched-field">
            <span class="sched-label">Day</span>
            <select bind:value={scheduleDraft.day_of_week}>
              {#each DOW_NAMES as name, i}
                <option value={i}>{name}</option>
              {/each}
            </select>
          </label>
        {/if}
        <label class="sched-field">
          <span class="sched-label">Time <em>(server local)</em></span>
          <input
            type="time"
            value={`${pad2(scheduleDraft.hour)}:${pad2(scheduleDraft.minute)}`}
            oninput={(e) => {
              const [h, m] = (e.currentTarget as HTMLInputElement).value
                .split(':')
                .map(Number);
              if (!Number.isNaN(h) && !Number.isNaN(m) && scheduleDraft) {
                scheduleDraft = { ...scheduleDraft, hour: h, minute: m };
              }
            }}
          />
        </label>
        <label class="sched-field">
          <span class="sched-label">Keep latest <em>(backups)</em></span>
          <input
            type="number"
            min="0"
            max="365"
            bind:value={scheduleDraft.retention_count}
          />
        </label>
      </div>
      <div class="sched-status" class:on={scheduleDraft.enabled}>
        <label class="sched-toggle">
          <input
            type="checkbox"
            class="switch"
            bind:checked={scheduleDraft.enabled}
            aria-label="Enable scheduled backups"
          />
          <span class="track" aria-hidden="true">
            <span class="thumb"></span>
          </span>
        </label>
        <div class="sched-status-body">
          <strong>{scheduleDraft.enabled ? 'Scheduled backups on' : 'Scheduled backups off'}</strong>
          <span class="sched-status-detail">
            {#if scheduleDraft.enabled}
              Runs {scheduleDraft.frequency === 'weekly'
                ? `every ${DOW_NAMES[scheduleDraft.day_of_week]}`
                : 'every day'} at {pad2(scheduleDraft.hour)}:{pad2(scheduleDraft.minute)} server time, keeps the latest {scheduleDraft.retention_count} {scheduleDraft.retention_count === 1 ? 'backup' : 'backups'}.
            {:else}
              Only manual backups will run. Toggle on to enable an automatic schedule.
            {/if}
          </span>
        </div>
      </div>
      <div class="sched-actions">
        <button
          type="button"
          class="btn primary"
          disabled={!scheduleDirty() || scheduleSaving}
          onclick={saveSchedule}
        >
          {scheduleSaving ? 'Saving…' : 'Save schedule'}
        </button>
        {#if scheduleSaved}
          <span class="saved-flash">Saved ✓</span>
        {/if}
      </div>
    </div>
  {/if}

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
                onclick={() => restoreFromRow(b)}
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
      <div class="dest-head-actions">
        <button class="btn" onclick={() => { showAddDest = !showAddDest; showAddGdrive = false; }}>
          {showAddDest ? 'Cancel' : '+ SFTP'}
        </button>
        {#if integrations?.google_drive.configured}
          <button class="btn" onclick={() => { showAddGdrive = !showAddGdrive; showAddDest = false; }}>
            {showAddGdrive ? 'Cancel' : '+ Google Drive'}
          </button>
        {/if}
      </div>
    </div>
    <p class="muted">
      After every successful backup, Postern uploads the tarball to each
      enabled destination. The local copy in the table above is always
      written first; off-site failures are logged but never fail the
      local backup. Credentials are vault-encrypted at rest.
    </p>

    {#if integrations && !integrations.google_drive.configured}
      <p class="muted small">
        Google Drive integration is available but not configured on the
        server. Set <code>POSTERN_GDRIVE_CLIENT_ID</code>,
        <code>POSTERN_GDRIVE_CLIENT_SECRET</code>, and
        <code>POSTERN_GDRIVE_REDIRECT_URI</code> in the server env to
        enable it.
      </p>
    {/if}

    {#if gdriveConnectedFlash}
      <div class="success" style="margin-top: 0.6rem;">
        ✓ Connected Google Drive as <strong>{gdriveConnectedFlash}</strong>.
        Future backups will auto-push.
        <button class="dismiss" onclick={() => (gdriveConnectedFlash = null)} aria-label="Dismiss">×</button>
      </div>
    {/if}
    {#if gdriveErrorFlash}
      <div class="err" style="margin-top: 0.6rem;">
        ⚠ Google Drive connect failed: {gdriveErrorFlash}
        <button class="dismiss" onclick={() => (gdriveErrorFlash = null)} aria-label="Dismiss">×</button>
      </div>
    {/if}

    {#if showAddGdrive}
      <form
        class="add-form"
        onsubmit={(e: SubmitEvent) => {
          e.preventDefault();
          startGdriveConnect();
        }}
      >
        <div class="form-row">
          <label>
            <span>Label</span>
            <input
              type="text"
              bind:value={gdriveLabel}
              placeholder="My Google Drive"
              required
            />
          </label>
        </div>
        <p class="muted form-help">
          You'll be redirected to Google to grant Postern access. The
          requested scope is <code>drive.file</code> — Postern can only
          read or modify files it created itself, never your other
          Drive contents. Tarballs land in a folder called
          <strong>Postern Backups</strong> in your Drive root.
        </p>
        <div class="form-actions">
          <button type="submit" class="btn primary" disabled={!gdriveLabel.trim()}>
            Connect Google Drive →
          </button>
          <button
            type="button"
            class="btn ghost"
            onclick={() => { showAddGdrive = false; gdriveLabel = ''; }}
          >
            Cancel
          </button>
        </div>
      </form>
    {/if}

    {#if showAddDest}
      <form
        class="add-form"
        onsubmit={(e: SubmitEvent) => {
          e.preventDefault();
          void addDest();
        }}
      >
        <div class="form-row">
          <label>
            <span>Label</span>
            <input type="text" bind:value={newDest.label} placeholder="Hetzner box" required />
          </label>
        </div>
        <div class="form-row two">
          <label class="grow">
            <span>SFTP host</span>
            <input type="text" bind:value={newDest.host} placeholder="backup.example.com" required />
          </label>
          <label>
            <span>Port</span>
            <input type="number" bind:value={newDest.port} min="1" max="65535" required />
          </label>
        </div>
        <div class="form-row two">
          <label class="grow">
            <span>Username</span>
            <input type="text" bind:value={newDest.username} required />
          </label>
          <label class="grow">
            <span>Remote dir</span>
            <input type="text" bind:value={newDest.remote_dir} placeholder="/home/postern/backups" required />
          </label>
        </div>
        <div class="form-row">
          <label>
            <span>Auth</span>
            <select bind:value={newDest.auth}>
              <option value="password">Password</option>
              <option value="key">Private key (OpenSSH)</option>
            </select>
          </label>
        </div>
        {#if newDest.auth === 'password'}
          <div class="form-row">
            <label>
              <span>Password</span>
              <input type="password" bind:value={newDest.password} autocomplete="off" required />
            </label>
          </div>
        {:else}
          <div class="form-row">
            <label>
              <span>Private key (paste OpenSSH-format)</span>
              <textarea
                bind:value={newDest.key_pem}
                rows="6"
                placeholder="-----BEGIN OPENSSH PRIVATE KEY-----&#10;...&#10;-----END OPENSSH PRIVATE KEY-----"
                required
              ></textarea>
            </label>
          </div>
          <div class="form-row">
            <label>
              <span>Passphrase (if key is encrypted)</span>
              <input type="password" bind:value={newDest.passphrase} autocomplete="off" />
            </label>
          </div>
        {/if}
        {#if addDestError}
          <div class="err">⚠ {addDestError}</div>
        {/if}
        <div class="form-actions">
          <button type="submit" class="btn primary" disabled={addingDest}>
            {addingDest ? 'Testing connection…' : 'Test & save'}
          </button>
          <button type="button" class="btn ghost" onclick={() => { showAddDest = false; resetNewDest(); }}>
            Cancel
          </button>
        </div>
        <p class="muted form-help">
          Connection is tested before saving — the destination won't be
          added unless Postern can authenticate and stat the remote dir.
        </p>
      </form>
    {/if}

    {#if destinations.length === 0 && !showAddDest}
      <p class="muted">
        No destinations yet. Backups stay on this server only — if the
        VPS dies, both the live data and the backup go with it.
      </p>
    {:else if destinations.length > 0}
      <ul class="dest-list">
        {#each destinations as d (d.id)}
          <li class="dest-item" class:disabled={!d.enabled}>
            <div class="dest-id">
              <strong>{d.label}</strong>
              {#if d.kind === 'sftp'}
                {@const sftp = asSftp(d)}
                <span class="muted">
                  sftp://{sftp.username}@{sftp.host}:{sftp.port}{sftp.remote_dir}
                </span>
                {#if d.server_fingerprint}
                  <span class="fp pinned" title="Pinned via TOFU. Future connects must present this exact key.">
                    🔒 {d.server_fingerprint}
                  </span>
                {:else}
                  <span class="fp unpinned" title="No hostkey pinned. The next connect will accept any key and pin it.">
                    ⚠ no hostkey pinned (TOFU on next connect)
                  </span>
                {/if}
              {:else if d.kind === 'gdrive'}
                {@const g = asGdrive(d)}
                <span class="muted">
                  Google Drive{#if g.account_email} · {g.account_email}{/if} · {g.folder_name}
                </span>
              {/if}
            </div>
            <div class="dest-state">
              {#if d.last_push_status === 'ok'}
                <span class="status-ok">
                  ✓ pushed {d.last_push_filename} {d.last_push_at ? formatDate(d.last_push_at) : ''}
                </span>
              {:else if d.last_push_status === 'error'}
                <details class="err-details">
                  <summary class="status-err">
                    ⚠ last push failed: {d.last_push_error?.slice(0, 80) ?? 'unknown'}{(d.last_push_error?.length ?? 0) > 80 ? '…' : ''}
                  </summary>
                  <textarea
                    class="err-full"
                    readonly
                    rows={Math.min(8, Math.max(2, ((d.last_push_error ?? '').match(/\n/g)?.length ?? 0) + 2))}
                  >{d.last_push_error ?? ''}</textarea>
                </details>
              {:else}
                <span class="muted">no pushes yet</span>
              {/if}
              {#if destFlash[d.id]}
                <div class="dest-flash">{destFlash[d.id]}</div>
              {/if}
            </div>
            <div class="dest-actions">
              <button class="row-btn" onclick={() => testDest(d)} disabled={destBusy[d.id]}>Test</button>
              <button class="row-btn" onclick={() => pushLatestToDest(d)} disabled={destBusy[d.id]}>
                Push latest
              </button>
              <button class="row-btn" onclick={() => toggleDestEnabled(d)} disabled={destBusy[d.id]}>
                {d.enabled ? 'Disable' : 'Enable'}
              </button>
              {#if d.kind === 'sftp' && d.server_fingerprint}
                <button class="row-btn" onclick={() => forgetDestFingerprint(d)} disabled={destBusy[d.id]}
                        title="Clear pinned hostkey (only after a deliberate key rotation)">
                  Forget hostkey
                </button>
              {/if}
              <button class="del" onclick={() => removeDest(d)}>Remove</button>
            </div>
          </li>
        {/each}
      </ul>
    {/if}
  </div>

  <div class="restore-section panel" id="restore-section">
    <h3>Restore from an off-site / uploaded backup</h3>
    <p class="muted">
      Use this if your backup file lives off-machine — on your laptop, an
      external drive, or anywhere else. To restore one of the backups in
      the table above, just click <strong>Restore</strong> on that row;
      no upload needed.
    </p>

    {#if restoreStep === 'idle'}
      <label class="file-pick">
        <input type="file" accept=".tar.gz,.gz,application/gzip" onchange={pickRestoreFile} />
        <span class="btn">Choose backup file…</span>
      </label>
      {#if restoreError}
        <div class="err" style="margin-top: 0.6rem;">⚠ {restoreError}</div>
      {/if}
    {:else if restoreStep === 'staging'}
      <p class="muted">
        Preparing <strong>{restoreLabel}</strong>…
      </p>
    {:else if restoreStep === 'password'}
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
    {:else if restoreStep === 'validating'}
      <p class="muted">Decrypting and counting rows… (may take a minute on large backups)</p>
    {:else if restoreStep === 'review'}
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
    {:else if restoreStep === 'applying'}
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
</article>

<style>
  article.backups-shell { width: 100%; max-width: clamp(60rem, 94vw, 110rem); margin: 0 auto; padding: 1.25rem 2rem 2.75rem; box-sizing: border-box; }
  .page-top { margin-bottom: 0.9rem; }
  .back { display: inline-block; color: inherit; opacity: 0.62; text-decoration: none; font-size: 0.85rem; }
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
  .hero h1 { font-size: 2rem; font-weight: 650; margin: 0 0 0.4rem; letter-spacing: -0.03em; }
  .hero p { font-size: 0.9rem; color: var(--muted); margin: 0; line-height: 1.55; max-width: 44rem; }
  .hero-badges { display: flex; flex-wrap: wrap; gap: 0.45rem; align-content: start; justify-content: flex-end; }
  .hero-chip {
    display: inline-flex; align-items: center; padding: 0.42rem 0.72rem;
    border-radius: 999px; background: color-mix(in oklab, var(--surface-2) 82%, transparent);
    border: 1px solid color-mix(in oklab, currentColor 8%, transparent); font-size: 0.72rem; font-weight: 600;
  }
  .panel {
    border: 1px solid var(--border);
    border-radius: 1.1rem;
    background: color-mix(in oklab, var(--surface) 94%, transparent);
    box-shadow: 0 14px 32px rgba(0, 0, 0, 0.05);
  }

  /* ---- Schedule panel ---- */
  .schedule-section { margin-bottom: 1rem; padding: 1.1rem 1.25rem 1rem; }
  .schedule-section h3 { margin: 0 0 0.4rem; font-size: 0.95rem; font-weight: 650; }
  .schedule-section p.muted { font-size: 0.82rem; line-height: 1.5; margin: 0 0 0.85rem; }
  /* Schedule fields lay out as a responsive grid: each column is at
     least 9rem wide and grows to fill, so on a desktop the four
     fields sit on one row and on mobile they wrap two-per-row
     without overflow. Labels are a fixed-height row above the
     control so the inputs themselves all start at the same y, no
     matter how long any individual label happens to be. */
  .sched-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(9rem, 1fr));
    gap: 0.75rem 1rem;
    align-items: start;
  }
  .sched-field {
    display: grid;
    grid-template-rows: auto auto;
    gap: 0.3rem;
    font-size: 0.78rem;
    font-weight: 500;
  }
  .sched-label {
    display: block;
    line-height: 1.2;
    color: color-mix(in oklab, currentColor 70%, transparent);
    font-weight: 600;
    letter-spacing: 0.005em;
  }
  .sched-label em {
    font-style: normal;
    font-weight: 500;
    color: color-mix(in oklab, currentColor 50%, transparent);
    margin-left: 0.25rem;
  }
  .sched-field input,
  .sched-field select {
    font: inherit;
    font-size: 0.9rem;
    padding: 0.5rem 0.6rem;
    border: 1px solid var(--border);
    border-radius: 0.5rem;
    background: var(--surface);
    color: inherit;
    width: 100%;
    box-sizing: border-box;
    height: 2.4rem;
  }
  .sched-field input:focus,
  .sched-field select:focus {
    outline: none;
    border-color: color-mix(in oklab, var(--accent) 40%, transparent);
    box-shadow: 0 0 0 3px color-mix(in oklab, var(--accent) 14%, transparent);
  }

  /* Schedule status row — shows current effective state with a
     proper iOS-style switch instead of the system checkbox. */
  .sched-status {
    margin-top: 1rem;
    display: flex;
    align-items: center;
    gap: 0.85rem;
    padding: 0.8rem 1rem;
    background: color-mix(in oklab, currentColor 4%, transparent);
    border: 1px solid var(--border);
    border-radius: 0.7rem;
    transition: background-color 160ms ease, border-color 160ms ease;
  }
  .sched-status.on {
    background: color-mix(in oklab, var(--accent) 8%, transparent);
    border-color: color-mix(in oklab, var(--accent) 30%, var(--border));
  }
  .sched-status-body {
    display: flex;
    flex-direction: column;
    gap: 0.18rem;
    min-width: 0;
    flex: 1 1 auto;
  }
  .sched-status-body strong {
    font-size: 0.92rem;
    font-weight: 600;
  }
  .sched-status-detail {
    font-size: 0.8rem;
    line-height: 1.4;
    color: color-mix(in oklab, currentColor 65%, transparent);
  }

  /* Custom toggle switch. Native checkbox stays for a11y / form
     semantics but is positioned-absolutely off-screen behind the
     visible track. */
  .sched-toggle {
    position: relative;
    display: inline-flex;
    align-items: center;
    cursor: pointer;
    flex: 0 0 auto;
  }
  .sched-toggle .switch {
    position: absolute;
    opacity: 0;
    width: 100%;
    height: 100%;
    margin: 0;
    cursor: pointer;
  }
  .sched-toggle .track {
    width: 40px;
    height: 22px;
    border-radius: 999px;
    background: color-mix(in oklab, currentColor 20%, transparent);
    position: relative;
    transition: background-color 160ms ease;
    flex-shrink: 0;
  }
  .sched-toggle .thumb {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 18px;
    height: 18px;
    border-radius: 999px;
    background: var(--surface);
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.18);
    transition: transform 180ms cubic-bezier(0.2, 0.8, 0.2, 1);
  }
  .sched-toggle .switch:checked + .track {
    background: var(--accent);
  }
  .sched-toggle .switch:checked + .track .thumb {
    transform: translateX(18px);
  }
  .sched-toggle .switch:focus-visible + .track {
    box-shadow: 0 0 0 3px color-mix(in oklab, var(--accent) 30%, transparent);
  }

  .sched-actions { display: flex; gap: 0.6rem; align-items: center; margin-top: 0.85rem; }
  .saved-flash { font-size: 0.78rem; color: var(--c-success, #2f9e6b); }

  .create-section { margin-bottom: 1.5rem; padding: 1.2rem 1.25rem; }
  .create-btn {
    font: inherit; font-size: 0.9rem; font-weight: 600;
    padding: 0.6rem 1.2rem;
    background: var(--accent); border: 0; border-radius: 999px;
    color: white; cursor: pointer;
  }
  .create-btn:hover:not(:disabled) { filter: brightness(0.97); }
  .create-btn:disabled { opacity: 0.55; cursor: progress; }
  .success {
    position: relative;
    margin-top: 0.75rem; padding: 0.6rem 2rem 0.6rem 0.85rem;
    background: color-mix(in oklab, forestgreen 10%, transparent);
    border-left: 2px solid forestgreen;
    border-radius: 0 0.3rem 0.3rem 0;
    font-size: 0.85rem;
  }
  .job-status.running {
    margin-top: 0.75rem; padding: 0.6rem 0.85rem;
    background: color-mix(in oklab, var(--accent) 8%, transparent);
    border-left: 2px solid var(--accent);
    border-radius: 0 0.3rem 0.3rem 0;
    font-size: 0.85rem;
    line-height: 1.5;
  }
  .job-status.running strong { margin-right: 0.4rem; }
  .dismiss {
    position: absolute; top: 0.3rem; right: 0.4rem;
    background: transparent; border: 0; cursor: pointer;
    font-size: 1.1rem; line-height: 1; color: inherit; opacity: 0.5;
  }
  .dismiss:hover { opacity: 1; }
  .err {
    position: relative;
    margin-top: 0.75rem; padding: 0.55rem 2rem 0.55rem 0.75rem;
    background: color-mix(in oklab, crimson 10%, transparent);
    border-left: 2px solid crimson;
    border-radius: 0 0.3rem 0.3rem 0;
    font-size: 0.82rem;
  }

  .muted { opacity: 0.55; font-size: 0.88rem; }

  .table-wrap { overflow: hidden; }
  table { width: 100%; border-collapse: collapse; font-size: 0.83rem; }
  thead th {
    text-align: left; font-weight: 600; font-size: 0.72rem;
    text-transform: uppercase; letter-spacing: 0.05em; opacity: 0.5;
    padding: 0.5rem 0.75rem; border-bottom: 1px solid var(--border);
  }
  tbody tr { border-bottom: 1px solid var(--border); }
  tbody tr:hover { background: var(--row-hover); }
  td { padding: 0.55rem 0.75rem; }
  .name code { font-size: 0.82rem; }
  .row-actions {
    display: flex;
    gap: 0.4rem;
  }
  .row-btn {
    font: inherit; font-size: 0.75rem; padding: 0.2rem 0.6rem;
    border: 1px solid var(--border);
    background: transparent; color: inherit;
    border-radius: 0.25rem; cursor: pointer;
    text-decoration: none; display: inline-flex; align-items: center;
  }
  .row-btn:hover:not(:disabled) {
    background: color-mix(in oklab, currentColor 6%, transparent);
  }
  .row-btn.primary-ish {
    border-color: color-mix(in oklab, var(--accent) 50%, transparent);
    background: color-mix(in oklab, var(--accent) 8%, transparent);
  }
  .row-btn.primary-ish:hover:not(:disabled) {
    background: color-mix(in oklab, var(--accent) 18%, transparent);
  }
  .row-btn:disabled { opacity: 0.5; cursor: not-allowed; }
  .del {
    font: inherit; font-size: 0.75rem; padding: 0.2rem 0.5rem;
    border: 1px solid color-mix(in oklab, crimson 30%, transparent);
    background: transparent; color: color-mix(in oklab, crimson 80%, currentColor);
    border-radius: 0.25rem; cursor: pointer;
  }
  .del:hover { background: color-mix(in oklab, crimson 10%, transparent); }

  /* ---- Off-site destinations panel ---- */
  .destinations { margin-top: 1.5rem; padding: 1.2rem 1.4rem 1.4rem; }
  .dest-head {
    display: flex; align-items: center; justify-content: space-between;
    margin-bottom: 0.4rem;
  }
  .dest-head-actions { display: flex; gap: 0.4rem; flex-wrap: wrap; }
  .destinations p.muted.small { font-size: 0.78rem; opacity: 0.6; }
  .destinations h3 { margin: 0; font-size: 0.95rem; font-weight: 650; }
  .destinations p.muted { font-size: 0.82rem; line-height: 1.55; margin: 0 0 0.85rem; }

  .add-form {
    margin: 0.85rem 0;
    padding: 1rem 1.1rem;
    border: 1px solid var(--border);
    border-radius: 0.6rem;
    background: color-mix(in oklab, var(--surface) 70%, var(--surface-2));
  }
  .form-row { display: flex; gap: 0.65rem; margin-bottom: 0.7rem; }
  .form-row.two label { flex: 1; }
  .form-row .grow { flex: 1; }
  .form-row label {
    display: flex; flex-direction: column; gap: 0.25rem;
    font-size: 0.78rem; font-weight: 500; flex: 1;
  }
  .form-row input,
  .form-row select,
  .form-row textarea {
    font: inherit; font-size: 0.85rem;
    padding: 0.4rem 0.55rem;
    border: 1px solid var(--border); border-radius: 0.35rem;
    background: var(--surface); color: inherit;
  }
  .form-row textarea { font-family: ui-monospace, monospace; font-size: 0.75rem; }
  .form-actions { display: flex; gap: 0.5rem; margin-top: 0.6rem; }
  .form-help { margin-top: 0.5rem; font-size: 0.78rem; }

  .dest-list { list-style: none; margin: 0; padding: 0; }
  .dest-item {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr) auto;
    align-items: center;
    gap: 0.85rem;
    padding: 0.7rem 0.85rem;
    border: 1px solid var(--border); border-radius: 0.5rem;
    margin-bottom: 0.5rem;
    background: var(--surface);
  }
  .dest-item.disabled { opacity: 0.55; }
  .dest-id strong { font-size: 0.88rem; }
  .dest-id .muted { display: block; font-size: 0.74rem; margin-top: 0.15rem; opacity: 0.6; }
  .fp {
    display: block;
    margin-top: 0.2rem;
    font-family: ui-monospace, monospace;
    font-size: 0.7rem;
    word-break: break-all;
  }
  .fp.pinned { color: forestgreen; }
  .fp.unpinned { color: color-mix(in oklab, goldenrod 80%, currentColor); font-family: inherit; }
  .dest-state { font-size: 0.78rem; min-width: 0; }
  .dest-state .status-ok { color: forestgreen; }
  .dest-state .status-err { color: color-mix(in oklab, crimson 70%, currentColor); }
  .err-details { font-size: 0.78rem; }
  .err-details summary { cursor: pointer; list-style: revert; }
  .err-details .err-full {
    width: 100%; margin-top: 0.4rem; box-sizing: border-box;
    font: inherit; font-size: 0.74rem;
    font-family: ui-monospace, monospace;
    padding: 0.4rem 0.55rem;
    background: var(--surface-2, rgba(255,255,255,0.04));
    color: inherit;
    border: 1px solid var(--border, rgba(255,255,255,0.1));
    border-radius: 0.3rem;
    resize: vertical;
  }
  .dest-flash { margin-top: 0.25rem; font-size: 0.78rem; opacity: 0.85; }
  .dest-actions { display: flex; gap: 0.35rem; flex-wrap: wrap; justify-content: flex-end; }

  @media (max-width: 720px) {
    .dest-item { grid-template-columns: 1fr; }
    .dest-actions { justify-content: flex-start; }
  }

  .restore-section { margin-top: 2rem; padding: 1.2rem 1.4rem 1.4rem; }
  .restore-section h3 { margin: 0 0 0.5rem; font-size: 0.95rem; font-weight: 650; letter-spacing: -0.01em; }
  .restore-section h4 { margin: 0 0 0.5rem; font-size: 0.88rem; font-weight: 600; }
  .restore-section p { font-size: 0.85rem; line-height: 1.55; margin: 0 0 0.65rem; }
  .restore-section .muted { font-size: 0.85rem; }

  .file-pick { display: inline-block; cursor: pointer; }
  .file-pick input[type='file'] {
    position: absolute; width: 1px; height: 1px; opacity: 0;
    overflow: hidden; clip: rect(0 0 0 0);
  }
  .btn {
    display: inline-flex; align-items: center; gap: 0.4rem;
    font: inherit; font-size: 0.85rem; font-weight: 600;
    padding: 0.5rem 1rem; border: 1px solid var(--border);
    background: var(--surface-2); color: inherit;
    border-radius: 999px; cursor: pointer;
  }
  .btn:hover:not(:disabled) { background: color-mix(in oklab, var(--surface-2) 70%, var(--surface)); }
  .btn:disabled { opacity: 0.55; cursor: not-allowed; }
  .btn.primary { background: var(--accent); border-color: transparent; color: white; }
  .btn.primary:hover:not(:disabled) { filter: brightness(0.97); }
  .btn.ghost { background: transparent; }

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

  @media (max-width: 820px) {
    article.backups-shell { padding: 1rem; }
    .hero { grid-template-columns: 1fr; }
    .hero-badges { justify-content: flex-start; }
  }
</style>
