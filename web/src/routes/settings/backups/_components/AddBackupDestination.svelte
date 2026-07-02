<script lang="ts">
  import { onMount } from 'svelte';
  import {
    api,
    type Integrations,
    type NewSftpDestination
  } from '$lib/api';

  interface Props {
    integrations: Integrations | null;
    /// Parent re-fetches the destinations list after a successful add.
    onAdded: () => Promise<void> | void;
  }

  let { integrations, onAdded }: Props = $props();

  // ─── State ──────────────────────────────────────────────────────────────
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

  // GDrive OAuth callback flashes — surfaced from the redirect target
  // (?gdrive_connected=… or ?gdrive_error=…) when the user comes back
  // from Google's consent screen.
  let gdriveConnectedFlash = $state<string | null>(null);
  let gdriveErrorFlash = $state<string | null>(null);

  onMount(() => {
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
    if (
      !newDest.label.trim() ||
      !newDest.host.trim() ||
      !newDest.username.trim() ||
      !newDest.remote_dir.trim()
    ) {
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
      await onAdded();
    } catch (e) {
      addDestError = e instanceof Error ? e.message : String(e);
    } finally {
      addingDest = false;
    }
  }
</script>

<!-- "Add" buttons live above the form. Toggling one closes the other. -->
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

<style>
  /* Shared base styles (.btn, .panel, .muted) come from the parent
     under :global(); local form layout + flashes are scoped here. */
  .dest-head-actions { display: flex; gap: 0.4rem; flex-wrap: wrap; }

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

  .success {
    position: relative;
    padding: 0.6rem 2rem 0.6rem 0.85rem;
    background: color-mix(in oklab, forestgreen 10%, transparent);
    border-left: 2px solid forestgreen;
    border-radius: 0 0.3rem 0.3rem 0;
    font-size: 0.85rem;
  }
  .err {
    position: relative;
    padding: 0.55rem 2rem 0.55rem 0.75rem;
    background: color-mix(in oklab, crimson 10%, transparent);
    border-left: 2px solid crimson;
    border-radius: 0 0.3rem 0.3rem 0;
    font-size: 0.85rem;
  }
  .dismiss {
    position: absolute; top: 0.3rem; right: 0.4rem;
    background: transparent; border: 0; cursor: pointer;
    font-size: 1.1rem; line-height: 1; color: inherit; opacity: 0.5;
  }
  .dismiss:hover { opacity: 1; }
  .muted.small { font-size: 0.78rem; }
</style>
