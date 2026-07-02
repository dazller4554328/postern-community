<script lang="ts">
  // Settings → Updates panel. License key + current version +
  // update check/apply with confirmation modal.

  import './UpdatesPanel.css';
  import { onMount } from 'svelte';
  import {
    api,
    type LicenseInfo,
    type UpdateCheckResult,
    type UpdateStatusResult
  } from '$lib/api';
  import { tier } from '$lib/tier';
  import UpdateConfirmDialog from './updates/UpdateConfirmDialog.svelte';
  import RestartingOverlay from './updates/RestartingOverlay.svelte';

  let license = $state<LicenseInfo | null>(null);
  let version = $state<string>('…');
  let keyDraft = $state('');
  let savingKey = $state(false);
  let verifyBusy = $state(false);
  let verifyMessage = $state<string | null>(null);
  let verifyOk = $state<boolean | null>(null);

  // Activation / transfer state. When the server says the key is
  // already bound to a different install, we surface the masked
  // existing-install fingerprint and prompt before taking it over.
  let transferPrompt = $state<{
    boundMasked: string | null;
    lastSeenWeek: string | null;
  } | null>(null);
  let transferBusy = $state(false);
  let updateWindowUntil = $state<number | null>(null);

  let checkBusy = $state(false);
  let checkResult = $state<UpdateCheckResult | null>(null);
  let checkError = $state<string | null>(null);

  let confirmOpen = $state(false);
  let applyBusy = $state(false);
  let status = $state<UpdateStatusResult | null>(null);
  let statusPoll: ReturnType<typeof setInterval> | null = null;

  onMount(() => {
    void loadAll();
    return () => { if (statusPoll) clearInterval(statusPoll); };
  });

  async function loadAll() {
    try {
      const [l, v, s] = await Promise.all([
        api.licenseGet(),
        api.updatesVersion(),
        api.updatesStatus()
      ]);
      license = l;
      version = v.commit;
      status = s;
      // Kick off a status poll whenever an update is in flight so the
      // UI reflects progress (running → success|failed) without a
      // manual refresh.
      if (s.state === 'running' || s.trigger_pending) startStatusPoll();

      // Auto-activate the bootstrap-seeded license on first load.
      // The bootstrap installer drops the key straight into
      // app_meta.license_key, but only /license/activate binds
      // install_id server-side — the user shouldn't have to re-paste
      // the key just to trigger that. license_verified_at_utc is null
      // before any successful /check or /activate, so it's a reliable
      // "we've never reached the license server with this key" signal.
      if (l?.license_key_masked && l?.license_verified_at_utc == null) {
        void activate(false);
      }
    } catch (e) {
      console.warn('updates panel load failed', e);
    }
  }

  function startStatusPoll() {
    if (statusPoll) return;
    statusPoll = setInterval(async () => {
      try {
        status = await api.updatesStatus();
        if (status.state !== 'running' && !status.trigger_pending) {
          if (statusPoll) { clearInterval(statusPoll); statusPoll = null; }
          // Refresh version in case the container came back on the
          // new commit already.
          try { version = (await api.updatesVersion()).commit; } catch {}
        }
      } catch {
        // Treat a failed poll as a transient network blip — the
        // container is probably restarting. Keep polling.
      }
    }, 3000);
  }

  async function saveLicense() {
    savingKey = true;
    verifyMessage = null;
    verifyOk = null;
    transferPrompt = null;
    try {
      const next = keyDraft.trim() || null;
      license = await api.licenseSet(next);
      keyDraft = '';
      if (next) {
        // Save → activate (binds install_id) → verify (sanity-check
        // status). If activate prompts for a transfer we stop here
        // and let the user confirm explicitly.
        await activate(false);
      }
    } catch (e) {
      verifyMessage = e instanceof Error ? e.message : String(e);
      verifyOk = false;
    } finally {
      savingKey = false;
    }
  }

  /// Tries to activate the saved license against this device's
  /// install_id. Branches on the server's status code:
  ///   - `activated` → done, license is now bound here.
  ///   - `needs_transfer_confirm` → opens the transfer prompt; user
  ///     decides whether to take the seat over.
  ///   - anything else → surface as a verify error.
  async function activate(confirmTransfer: boolean) {
    verifyBusy = true;
    verifyMessage = null;
    verifyOk = null;
    try {
      const r = await api.licenseActivate(confirmTransfer);
      if (r.ok) {
        verifyOk = true;
        verifyMessage = r.message ?? 'License activated on this install.';
        updateWindowUntil = r.update_window_until;
        transferPrompt = null;
      } else if (r.status === 'needs_transfer_confirm') {
        // Surface the prompt — the user explicitly clicks "Transfer"
        // to call activate() again with confirmTransfer=true.
        transferPrompt = {
          boundMasked: r.bound_install_masked,
          lastSeenWeek: r.last_seen_week
        };
        verifyOk = false;
        verifyMessage = r.message ?? 'License is already in use on a different install.';
      } else {
        verifyOk = false;
        verifyMessage = r.message ?? `Activation failed (${r.status}).`;
      }
      license = await api.licenseGet();
    } catch (e) {
      verifyOk = false;
      verifyMessage = e instanceof Error ? e.message : String(e);
    } finally {
      verifyBusy = false;
    }
  }

  async function confirmTransfer() {
    transferBusy = true;
    try {
      await activate(true);
    } finally {
      transferBusy = false;
    }
  }

  async function verify() {
    // Paste-then-Verify should "just work": if the input holds an
    // unsaved key, save + activate it instead of silently re-checking
    // the previously-saved key (a common support snag — users paste a
    // new key and click Verify expecting it to be read).
    if (keyDraft.trim()) {
      await saveLicense();
      return;
    }
    // Otherwise "Verify" means "re-check the saved key" — useful after,
    // e.g., a billing event.
    verifyBusy = true;
    verifyMessage = null;
    verifyOk = null;
    try {
      const r = await api.licenseVerify();
      verifyOk = r.valid;
      verifyMessage = r.message ?? (r.valid ? `License accepted (${r.tier ?? 'standard'}).` : 'License not accepted.');
      license = await api.licenseGet();
    } catch (e) {
      verifyOk = false;
      verifyMessage = e instanceof Error ? e.message : String(e);
    } finally {
      verifyBusy = false;
    }
  }

  async function checkForUpdates() {
    checkBusy = true;
    checkError = null;
    try {
      checkResult = await api.updatesCheck();
      // /check carries the canonical entitlement window. Update our
      // cached display value so the "Updates through …" line refreshes
      // every time the user pokes the check button.
      if (checkResult.update_window_until !== undefined) {
        updateWindowUntil = checkResult.update_window_until;
      }
    } catch (e) {
      checkError = e instanceof Error ? e.message : String(e);
      checkResult = null;
    } finally {
      checkBusy = false;
    }
  }

  function openConfirm() { confirmOpen = true; }
  function closeConfirm() { confirmOpen = false; }

  /// Restart-recovery overlay state. Once the apply is queued the
  /// container will go down. We watch /api/updates/status and only
  /// hard-reload the browser once it's reachable again — the
  /// previous setTimeout(1500) approach raced the container
  /// teardown and left users on a stale URL with a "page not
  /// found" error even after recovery.
  let restartingOverlay = $state(false);
  let recoveryStatus = $state<'queued' | 'restarting' | 'recovered' | 'timeout'>('queued');

  async function installUpdate() {
    applyBusy = true;
    try {
      await api.updatesApply();
      confirmOpen = false;
      status = await api.updatesStatus();
      startStatusPoll();
      // Show the recovery overlay so the user sees clear feedback
      // during the restart window. Then poll until the new
      // container responds, then hard-reload to / so the browser
      // picks up the fresh static-asset hashes (a chunk-hash
      // mismatch between the cached SPA and the new build is what
      // produced the "page not found" error in earlier reports).
      restartingOverlay = true;
      recoveryStatus = 'queued';
      void waitForRecoveryThenReload();
    } catch (e) {
      checkError = e instanceof Error ? e.message : String(e);
      restartingOverlay = false;
    } finally {
      applyBusy = false;
    }
  }

  /// Two-phase poll loop:
  ///   1. Wait for the status fetch to FAIL — that's when the
  ///      container has actually gone down for restart.
  ///   2. Wait for it to SUCCEED again — the new container is up.
  ///   3. Hard-reload to / so we land on the login page with the
  ///      new build's bundle.
  ///
  /// If the host updater never fired (e.g. the trigger file write
  /// failed), step 1 never completes and the 90-second deadline
  /// kicks in to navigate anyway. The user lands on / either way.
  async function waitForRecoveryThenReload() {
    const POLL_INTERVAL_MS = 1500;
    const MAX_WAIT_MS = 120_000;
    const startTime = Date.now();
    let sawFailure = false;
    let postFailureSuccess = 0;

    while (Date.now() - startTime < MAX_WAIT_MS) {
      try {
        await api.updatesStatus();
        if (sawFailure) {
          postFailureSuccess += 1;
          recoveryStatus = 'recovered';
          // Two consecutive successes after a failure burst means
          // the container is genuinely back, not a transient blip
          // during the restart sequence.
          if (postFailureSuccess >= 2) {
            hardReloadToRoot();
            return;
          }
        }
      } catch {
        sawFailure = true;
        postFailureSuccess = 0;
        recoveryStatus = 'restarting';
      }
      await new Promise((r) => setTimeout(r, POLL_INTERVAL_MS));
    }
    // Deadline hit — navigate anyway. The user's session is stale
    // by now and they'd want to land at the login screen rather
    // than continue to see this overlay.
    recoveryStatus = 'timeout';
    hardReloadToRoot();
  }

  function hardReloadToRoot() {
    // Use replace so the back button doesn't return to the broken
    // post-update state. Wrapped in try/catch because some sandbox
    // harnesses block replace() — the href fallback always works.
    try {
      window.location.replace('/');
    } catch {
      window.location.href = '/';
    }
  }

  function fmtBytes(n: number | null | undefined): string {
    if (!n) return '';
    const mb = n / (1024 * 1024);
    return mb >= 1 ? `${mb.toFixed(1)} MB` : `${(n / 1024).toFixed(1)} KB`;
  }

  function fmtTime(unix: number | null): string {
    if (!unix) return 'never';
    return new Date(unix * 1000).toLocaleString();
  }
</script>

<div class="panel-body">
  <!-- License — two flavours depending on build tier. Pro shows the
       key-entry form; Community shows a short "this build is free"
       card with no inputs (the backend rejects setLicense on free). -->
  {#if $tier.features.licensed_updates}
    <section class="card">
      <header>
        <h3>License</h3>
        <p class="muted">
          A valid license unlocks update downloads. Demo key for development:
          <code>PSTN-DEMO-DEMO-DEMO-DEMO</code>.
        </p>
      </header>

      <div class="row">
        <div class="label">Install ID</div>
        <code class="mono small">{license?.install_id ?? '…'}</code>
      </div>

      <div class="row">
        <div class="label">Status</div>
        <div>
          {#if !license?.license_key_masked}
            <span class="pill muted">No license configured</span>
          {:else}
            <span class="pill status-{license.license_status}">
              {license.license_status}
            </span>
            {#if license.license_tier}<span class="pill muted">{license.license_tier}</span>{/if}
            <span class="muted small">Last verified: {fmtTime(license.license_verified_at_utc)}</span>
          {/if}
        </div>
      </div>

      <div class="row">
        <div class="label">Current key</div>
        <code class="mono">{license?.license_key_masked ?? '—'}</code>
      </div>

      <div class="row stacked">
        <label for="licInput" class="label">Set / replace key</label>
        <div class="key-input">
          <input
            id="licInput"
            type="text"
            bind:value={keyDraft}
            placeholder="PSTN-XXXX-XXXX-XXXX-XXXX"
            autocomplete="off"
          />
          <button class="btn" disabled={savingKey} onclick={saveLicense}>
            {savingKey ? 'Saving…' : 'Save'}
          </button>
          <button
            class="btn"
            disabled={(!license?.license_key_masked && !keyDraft.trim()) || verifyBusy}
            onclick={verify}
          >
            {verifyBusy ? 'Checking…' : 'Verify'}
          </button>
          <button
            class="btn"
            disabled={(!license?.license_key_masked && !keyDraft.trim()) || verifyBusy}
            onclick={() => (keyDraft.trim() ? saveLicense() : activate(false))}
            title="Bind this install to the licence (server-side seat lock). Idempotent — safe to retry."
          >
            {verifyBusy ? 'Activating…' : 'Activate this install'}
          </button>
        </div>
        {#if verifyMessage}
          <p class="inline-msg" class:ok={verifyOk === true} class:err={verifyOk === false}>
            {verifyMessage}
          </p>
        {/if}

        {#if transferPrompt}
          <div class="transfer-card">
            <h4>This license is already activated elsewhere</h4>
            <p>
              The key is currently bound to install
              <code>{transferPrompt.boundMasked ?? '••••••••'}</code>{#if transferPrompt.lastSeenWeek}
                (last checked {transferPrompt.lastSeenWeek})
              {/if}.
              If you've migrated to this server, transfer the seat. The
              previous install will lose update access immediately.
            </p>
            <div class="transfer-actions">
              <button class="btn" disabled={transferBusy} onclick={() => (transferPrompt = null)}>
                Cancel
              </button>
              <button class="btn primary" disabled={transferBusy} onclick={confirmTransfer}>
                {transferBusy ? 'Transferring…' : 'Transfer to this install'}
              </button>
            </div>
          </div>
        {/if}

        {#if updateWindowUntil}
          <p class="window-line muted small">
            Updates through {new Date(updateWindowUntil * 1000).toISOString().slice(0, 10)}.
          </p>
        {/if}
      </div>
    </section>
  {:else}
    <section class="card">
      <header>
        <h3>Postern Community</h3>
        <p class="muted">
          No license required. Source + updates are on GitHub under the
          Apache-2.0 licence. Upgrade to Postern if you need VPN kill-switch,
          trusted-device sign-in, more than 3 mailboxes, or send-later.
        </p>
      </header>
      <div class="row">
        <div class="label">Install ID</div>
        <code class="mono small">{license?.install_id ?? '…'}</code>
      </div>
    </section>
  {/if}

  <!-- Updates -->
  <section class="card">
    <header>
      <h3>Updates</h3>
      <p class="muted">
        Postern checks <code>updates.postern.email</code> for new releases. Updates
        are downloaded over TLS, verified by SHA-256, and installed by a
        host-side updater service on your server. A database backup is
        taken automatically before each update.
      </p>
    </header>

    <div class="row">
      <div class="label">Current version</div>
      <code class="mono">{version}</code>
    </div>

    <div class="row">
      <button class="btn primary" disabled={checkBusy || !license?.license_key_masked} onclick={checkForUpdates}>
        {checkBusy ? 'Checking…' : 'Check for updates'}
      </button>
      {#if !license?.license_key_masked}
        <span class="muted small">Add a license first.</span>
      {/if}
    </div>

    {#if checkError}
      <p class="err-bubble">⚠ {checkError}</p>
    {/if}

    {#if checkResult}
      {#if checkResult.update_available}
        <div class="update-card">
          <div class="update-head">
            <strong>Update available</strong>
            <span class="pill accent">{checkResult.latest_commit}</span>
          </div>
          {#if checkResult.release_notes}
            <p class="notes">{checkResult.release_notes}</p>
          {/if}
          <div class="update-meta">
            {#if checkResult.release_date}<span>Released {checkResult.release_date}</span>{/if}
            {#if checkResult.size_bytes}<span>· {fmtBytes(checkResult.size_bytes)}</span>{/if}
            {#if checkResult.sha256}
              <span class="mono small" title="SHA-256">· {checkResult.sha256.slice(0, 12)}…</span>
            {/if}
          </div>
          <button class="btn primary" onclick={openConfirm} disabled={status?.state === 'running'}>
            {status?.state === 'running' ? 'Install in progress…' : 'Install update'}
          </button>
        </div>
      {:else}
        <p class="ok-bubble">✓ Postern is up to date.</p>
      {/if}
    {/if}

    {#if status && (status.state === 'running' || status.state === 'success' || status.state === 'failed')}
      <div class="status-card status-{status.state}">
        <strong>
          {#if status.state === 'running'}⟳ Installing…{/if}
          {#if status.state === 'success'}✓ Updated{/if}
          {#if status.state === 'failed'}⚠ Failed{/if}
        </strong>
        {#if status.message}<span class="muted">{status.message}</span>{/if}
        {#if status.finished_at}<span class="muted small">at {fmtTime(status.finished_at)}</span>{/if}
      </div>
    {/if}
  </section>
</div>

<UpdateConfirmDialog
  open={confirmOpen}
  busy={applyBusy}
  commitLabel={checkResult?.latest_commit ?? ''}
  releaseNotes={checkResult?.release_notes ?? null}
  onClose={closeConfirm}
  onConfirm={installUpdate}
/>

<!-- Restart-recovery overlay: covers the SPA while the container
     restarts so the user can't interact with stale UI. -->
<RestartingOverlay open={restartingOverlay} status={recoveryStatus} />

