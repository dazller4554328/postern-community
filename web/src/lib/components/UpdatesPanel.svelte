<script lang="ts">
  // Settings → Updates panel. License key + current version +
  // update check/apply with confirmation modal.

  import { onMount } from 'svelte';
  import {
    api,
    type LicenseInfo,
    type UpdateCheckResult,
    type UpdateStatusResult
  } from '$lib/api';
  import { tier } from '$lib/tier';

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
    // "Verify" now means "re-check". For the initial bind use
    // saveLicense → activate. This keeps the "Verify" button useful
    // for re-checking entitlement after, e.g., a billing event.
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
          <button class="btn" disabled={!license?.license_key_masked || verifyBusy} onclick={verify}>
            {verifyBusy ? 'Checking…' : 'Verify'}
          </button>
          <button
            class="btn"
            disabled={!license?.license_key_masked || verifyBusy}
            onclick={() => activate(false)}
            title="Bind this install to the saved licence (server-side seat lock). Idempotent — safe to retry."
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

{#if confirmOpen}
  <div
    class="modal-backdrop"
    role="presentation"
    onclick={closeConfirm}
    onkeydown={(e) => { if (e.key === 'Escape') closeConfirm(); }}
  >
    <div
      class="modal"
      role="dialog"
      aria-modal="true"
      aria-labelledby="update-confirm-title"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
    >
      <h3 id="update-confirm-title">Install update to {checkResult?.latest_commit}?</h3>
      <p>
        Postern will download the new release, verify it, back up your database,
        then rebuild the container. The mail server will be unreachable for
        roughly 30–60 seconds while the new container starts.
      </p>
      {#if checkResult?.release_notes}
        <p class="notes small"><em>{checkResult.release_notes}</em></p>
      {/if}
      <div class="modal-actions">
        <button class="btn" onclick={closeConfirm} disabled={applyBusy}>Cancel</button>
        <button class="btn primary" onclick={installUpdate} disabled={applyBusy}>
          {applyBusy ? 'Queuing…' : 'Install now'}
        </button>
      </div>
    </div>
  </div>
{/if}

<!-- ── Restart-recovery overlay ───────────────────────────────
     Visible from the moment the apply is queued until the new
     container responds. The overlay covers the screen so the
     user can't accidentally interact with stale UI during the
     restart, and the status text walks them through the phases
     so they don't think the app crashed. -->
{#if restartingOverlay}
  <div class="restart-overlay" role="alertdialog" aria-live="polite" aria-label="Update installing">
    <div class="restart-card">
      <div class="restart-spinner" aria-hidden="true">
        <svg viewBox="0 0 24 24" width="42" height="42" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <path d="M21 12a9 9 0 1 1-9-9" />
          <path d="M21 3v6h-6" />
        </svg>
      </div>
      <h3>Installing update</h3>
      <p class="restart-status">
        {#if recoveryStatus === 'queued'}
          Update queued — handing off to the host updater…
        {:else if recoveryStatus === 'restarting'}
          Container restarting. This usually takes 30–90 seconds.
        {:else if recoveryStatus === 'recovered'}
          New build is up. Reloading to the login page…
        {:else if recoveryStatus === 'timeout'}
          Update is taking longer than expected — reloading now. If the page errors, refresh manually.
        {/if}
      </p>
      <p class="restart-hint">
        You'll be returned to the login screen once the container is back. Keep this tab open.
      </p>
    </div>
  </div>
{/if}

<style>
  .panel-body { display: flex; flex-direction: column; gap: 1rem; }

  .card {
    border: 1px solid var(--border);
    background: var(--surface);
    border-radius: 0.85rem;
    padding: 1rem 1.2rem 1.1rem;
    display: flex;
    flex-direction: column;
    gap: 0.7rem;
  }
  .card header h3 { margin: 0 0 0.25rem; font-size: 1rem; }
  .card header p { margin: 0; color: var(--muted); font-size: 0.88rem; }

  .row {
    display: flex;
    flex-wrap: wrap;
    gap: 0.7rem;
    align-items: center;
    min-height: 2rem;
  }
  .row.stacked { flex-direction: column; align-items: stretch; gap: 0.4rem; }
  .label {
    min-width: 10rem;
    color: var(--muted);
    font-size: 0.85rem;
    font-weight: 500;
  }

  .key-input {
    display: grid;
    grid-template-columns: 1fr auto auto;
    gap: 0.45rem;
  }
  .key-input input {
    padding: 0.5rem 0.7rem;
    border: 1px solid var(--border);
    border-radius: 0.5rem;
    background: var(--surface-2);
    color: var(--fg);
    font: inherit;
    font-size: 0.9rem;
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  }

  .btn {
    padding: 0.45rem 0.9rem;
    border: 1px solid var(--border);
    background: var(--surface-2);
    color: var(--fg);
    border-radius: 0.5rem;
    cursor: pointer;
    font: inherit;
    font-size: 0.88rem;
  }
  .btn:disabled { opacity: 0.55; cursor: progress; }
  .btn.primary {
    background: var(--accent);
    color: white;
    border-color: transparent;
    font-weight: 600;
  }

  .mono {
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 0.85rem;
  }
  .small { font-size: 0.8rem; }
  .muted { color: var(--muted); }

  .pill {
    display: inline-block;
    padding: 0.08rem 0.55rem;
    border-radius: 999px;
    font-size: 0.72rem;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    background: color-mix(in oklab, var(--accent) 18%, var(--surface-2));
    color: color-mix(in oklab, var(--accent) 70%, var(--fg) 30%);
    margin-right: 0.4rem;
  }
  .pill.muted {
    background: var(--surface-2);
    color: var(--muted);
  }
  .pill.accent {
    background: var(--accent);
    color: white;
  }
  .pill.status-active {
    background: color-mix(in oklab, #22c55e 22%, var(--surface-2));
    color: color-mix(in oklab, #22c55e 70%, var(--fg) 30%);
  }
  .pill.status-malformed,
  .pill.status-expired,
  .pill.status-revoked,
  .pill.status-not_found,
  .pill.status-missing,
  .pill.status-error {
    background: color-mix(in oklab, crimson 22%, var(--surface-2));
    color: color-mix(in oklab, crimson 70%, var(--fg) 30%);
  }

  .inline-msg { margin: 0; font-size: 0.85rem; }
  .inline-msg.ok { color: color-mix(in oklab, #22c55e 70%, var(--fg) 30%); }
  .inline-msg.err { color: color-mix(in oklab, crimson 70%, var(--fg) 30%); }
  .transfer-card {
    margin-top: 0.6rem;
    padding: 0.85rem 1rem;
    border: 1px solid color-mix(in oklab, var(--accent) 35%, transparent);
    border-radius: 0.7rem;
    background: color-mix(in oklab, var(--accent) 6%, var(--surface-2));
  }
  .transfer-card h4 { margin: 0 0 0.35rem; font-size: 0.95rem; }
  .transfer-card p { margin: 0 0 0.6rem; font-size: 0.85rem; line-height: 1.45; }
  .transfer-card code { font-size: 0.8rem; }
  .transfer-actions {
    display: flex;
    gap: 0.5rem;
    justify-content: flex-end;
  }
  .window-line { margin: 0.5rem 0 0; }

  .err-bubble {
    padding: 0.55rem 0.85rem;
    background: color-mix(in oklab, crimson 10%, var(--surface));
    color: color-mix(in oklab, crimson 70%, var(--fg) 30%);
    border-radius: 0.5rem;
    font-size: 0.88rem;
    margin: 0;
  }
  .ok-bubble {
    padding: 0.55rem 0.85rem;
    background: color-mix(in oklab, #22c55e 10%, var(--surface));
    color: color-mix(in oklab, #22c55e 70%, var(--fg) 30%);
    border-radius: 0.5rem;
    font-size: 0.88rem;
    margin: 0;
  }

  .update-card {
    padding: 0.9rem 1rem;
    border: 1px solid color-mix(in oklab, var(--accent) 40%, var(--border));
    border-left: 3px solid var(--accent);
    border-radius: 0.6rem;
    background: color-mix(in oklab, var(--accent) 6%, var(--surface));
    display: flex;
    flex-direction: column;
    gap: 0.55rem;
  }
  .update-head { display: flex; gap: 0.55rem; align-items: baseline; }
  .update-meta { display: flex; flex-wrap: wrap; gap: 0.2rem; color: var(--muted); font-size: 0.82rem; }
  .notes { margin: 0; color: var(--fg); line-height: 1.45; }

  .status-card {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
    align-items: baseline;
    padding: 0.6rem 0.85rem;
    border-radius: 0.5rem;
    font-size: 0.88rem;
    border: 1px solid var(--border);
  }
  .status-card.status-running { background: color-mix(in oklab, var(--accent) 10%, var(--surface)); }
  .status-card.status-success { background: color-mix(in oklab, #22c55e 10%, var(--surface)); }
  .status-card.status-failed {
    background: color-mix(in oklab, crimson 10%, var(--surface));
    border-color: color-mix(in oklab, crimson 40%, var(--border));
  }

  .modal-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.45);
    backdrop-filter: blur(3px);
    display: grid;
    place-items: center;
    z-index: 200;
    padding: 1rem;
  }
  .modal {
    max-width: 34rem;
    width: 100%;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 0.9rem;
    padding: 1.2rem 1.4rem 1.3rem;
    box-shadow: 0 24px 64px rgba(0, 0, 0, 0.3);
  }
  .modal h3 { margin: 0 0 0.7rem; font-size: 1.05rem; }
  .modal p { margin: 0 0 0.7rem; color: var(--fg); line-height: 1.5; }
  .modal-actions { display: flex; justify-content: flex-end; gap: 0.55rem; margin-top: 0.5rem; }

  @media (max-width: 900px) {
    .key-input { grid-template-columns: 1fr; }
    .label { min-width: 0; }
  }

  /* Restart-recovery overlay — full-screen modal that covers
     the SPA while the container is being torn down + brought
     back up. Visual weight is high on purpose: the user just
     queued a destructive (well, redeployment) operation and we
     want them to see clear progress, not a half-rendered SPA
     with stale data. */
  .restart-overlay {
    position: fixed;
    inset: 0;
    z-index: 9999;
    display: grid;
    place-items: center;
    background:
      radial-gradient(circle at 50% 35%, color-mix(in oklab, var(--accent) 22%, transparent), transparent 60%),
      color-mix(in oklab, var(--bg) 96%, black);
    backdrop-filter: blur(6px);
    animation: overlay-fade 240ms ease-out;
  }
  @keyframes overlay-fade {
    from { opacity: 0; }
    to   { opacity: 1; }
  }
  .restart-card {
    max-width: 28rem;
    padding: 2rem 2.2rem;
    border-radius: 1.2rem;
    background: var(--surface);
    border: 1px solid var(--border);
    box-shadow: 0 20px 60px rgba(0, 0, 0, 0.25);
    text-align: center;
  }
  .restart-card h3 {
    margin: 0.6rem 0 0.5rem;
    font-size: 1.2rem;
    font-weight: 650;
  }
  .restart-status {
    margin: 0 0 0.75rem;
    font-size: 0.95rem;
    color: var(--fg);
    line-height: 1.5;
    min-height: 1.5em; /* avoid layout jitter as text changes */
  }
  .restart-hint {
    margin: 0;
    font-size: 0.8rem;
    color: var(--muted);
    line-height: 1.5;
  }
  .restart-spinner {
    color: var(--accent);
    animation: spin 1.4s linear infinite;
    transform-origin: center;
    display: inline-flex;
    margin-bottom: 0.4rem;
  }
  @keyframes spin {
    to { transform: rotate(360deg); }
  }
</style>
