<script lang="ts">
  import { onMount } from 'svelte';
  import { api, type TrustedDevice } from '$lib/api';
  import { prefs } from '$lib/prefs';
  import { lockVault, clearLocalTrustedDevice } from '$lib/vault';
  import { formatDate } from '$lib/format';
  import InfoBubble from '$lib/components/InfoBubble.svelte';
  import { tier } from '$lib/tier';

  let devices = $state<TrustedDevice[]>([]);
  let devicesLoaded = $state(false);
  let devicesLoading = $state(false);
  // Pagination — list grows unbounded on long-running deployments
  // (each phone-WiFi-cellular-flip can mint a new persistent token
  // if the user has remember-me ticked). Slice client-side because
  // listTrustedDevices returns the whole table; that's still cheap
  // (a row is ~200 bytes) but rendering hundreds of <li>s tanks the
  // panel.
  const DEVICES_PAGE_SIZE = 8;
  let devicesPage = $state(0);
  let totalDevicePages = $derived(
    Math.max(1, Math.ceil(devices.length / DEVICES_PAGE_SIZE))
  );
  let pagedDevices = $derived(
    devices.slice(
      devicesPage * DEVICES_PAGE_SIZE,
      (devicesPage + 1) * DEVICES_PAGE_SIZE
    )
  );
  function devicesPrev() {
    if (devicesPage > 0) devicesPage -= 1;
  }
  function devicesNext() {
    if (devicesPage < totalDevicePages - 1) devicesPage += 1;
  }
  // Reset to page 0 whenever the list shrinks past the current
  // window — e.g. after Revoke removes the last item on page 3.
  $effect(() => {
    if (devicesPage >= totalDevicePages) {
      devicesPage = Math.max(0, totalDevicePages - 1);
    }
  });

  // Two-factor (TOTP) state. Loaded on mount + after every action so
  // the panel reflects server reality (other tabs / browsers may
  // have just enabled or disabled it).
  let totpEnabled = $state(false);
  let totpPending = $state(false);
  let recoveryRemaining = $state(0);
  let totpLoading = $state(false);
  // Enrollment-in-progress payload. While set, the panel shows the
  // QR + manual code + confirm form. Cleared on confirm-success or
  // explicit cancel.
  let enrollPayload = $state<{
    secret: string;
    otpauth_url: string;
    qr_png_data_url: string;
  } | null>(null);
  let confirmCode = $state('');
  let confirmBusy = $state(false);
  let confirmErr = $state<string | null>(null);
  // Recovery codes shown ONCE after a successful enrollment confirm.
  // The user must save these before navigating away — once the
  // panel re-renders without them, they're lost (we only keep
  // hashes server-side).
  let revealedRecoveryCodes = $state<string[]>([]);
  // Disable flow: requires either a current TOTP code or a recovery
  // code so the user proves they still hold a factor.
  let disableCode = $state('');
  let disableUseRecovery = $state(false);
  let disableRecoveryCode = $state('');
  let disableBusy = $state(false);
  let disableErr = $state<string | null>(null);

  // Mirror key in localStorage. VaultGate (the unlock screen) reads
  // this on mount because /api/auth/totp/status hits the encrypted
  // DB and fails while the vault is locked — without the mirror,
  // first refresh after a logout shows only the password field.
  const TOTP_LOCAL_KEY = 'postern.totpEnabled';

  function syncTotpMirror(enabled: boolean) {
    try {
      localStorage.setItem(TOTP_LOCAL_KEY, enabled ? '1' : '0');
    } catch {
      /* private mode / quota — non-fatal */
    }
  }

  async function loadTotpStatus() {
    totpLoading = true;
    try {
      const s = await api.authTotpStatus();
      totpEnabled = s.enabled;
      totpPending = s.pending;
      recoveryRemaining = s.recovery_codes_remaining;
      syncTotpMirror(s.enabled);
    } catch (e) {
      console.error('totp status load failed', e);
    } finally {
      totpLoading = false;
    }
  }

  async function startEnrollment() {
    confirmErr = null;
    confirmCode = '';
    revealedRecoveryCodes = [];
    try {
      enrollPayload = await api.authTotpInit();
      totpPending = true;
    } catch (e) {
      confirmErr = e instanceof Error ? e.message : String(e);
    }
  }

  async function confirmEnrollment() {
    if (!enrollPayload || !/^\d{6}$/.test(confirmCode.trim())) {
      confirmErr = 'Enter the 6-digit code shown in your authenticator app.';
      return;
    }
    confirmBusy = true;
    confirmErr = null;
    try {
      const r = await api.authTotpConfirm(confirmCode.trim());
      revealedRecoveryCodes = r.recovery_codes;
      enrollPayload = null;
      confirmCode = '';
      await loadTotpStatus();
    } catch (e) {
      confirmErr = e instanceof Error ? e.message : String(e);
    } finally {
      confirmBusy = false;
    }
  }

  function cancelEnrollment() {
    enrollPayload = null;
    confirmCode = '';
    confirmErr = null;
  }

  async function disableTotp() {
    disableErr = null;
    if (disableUseRecovery) {
      if (!disableRecoveryCode.trim()) {
        disableErr = 'Enter a recovery code.';
        return;
      }
    } else if (!/^\d{6}$/.test(disableCode.trim())) {
      disableErr = 'Enter your current 6-digit code.';
      return;
    }
    if (
      !confirm(
        'Disable two-factor authentication?\n\n' +
          'You will only need your master password to unlock the vault from now on.'
      )
    )
      return;
    disableBusy = true;
    try {
      await api.authTotpDisable({
        code: !disableUseRecovery ? disableCode.trim() : undefined,
        recoveryCode: disableUseRecovery ? disableRecoveryCode.trim() : undefined
      });
      disableCode = '';
      disableRecoveryCode = '';
      await loadTotpStatus();
    } catch (e) {
      disableErr = e instanceof Error ? e.message : String(e);
    } finally {
      disableBusy = false;
    }
  }

  function copyRecoveryCodes() {
    const text = revealedRecoveryCodes.join('\n');
    navigator.clipboard
      ?.writeText(text)
      .catch(() => {
        /* clipboard blocked — user can still select + copy manually */
      });
  }

  // Read eventTicker reactively from the prefs store; write back via
  // prefs.update so other tabs see the change too.
  let eventTicker = $state(false);
  $effect(() => {
    const unsub = prefs.subscribe((p) => {
      eventTicker = p.eventTicker;
    });
    return unsub;
  });
  function setEventTicker(v: boolean) {
    prefs.update((p) => ({ ...p, eventTicker: v }));
  }

  async function loadDevices() {
    devicesLoading = true;
    try {
      devices = await api.listTrustedDevices();
      devicesLoaded = true;
    } catch (e) {
      console.error('trusted devices load failed', e);
    } finally {
      devicesLoading = false;
    }
  }

  async function revokeDevice(id: number) {
    if (!confirm('Revoke this device? It will need the master password on its next request.')) return;
    try {
      const res = await api.revokeTrustedDevice(id);
      if (res.self) {
        // We just pulled our own cookie out from under us. A full reload
        // is the cleanest way to land back on the unlock screen. Also
        // drop the client-side enrol flag so the "Remember this device"
        // checkbox comes back on the next render.
        clearLocalTrustedDevice();
        window.location.reload();
        return;
      }
      await loadDevices();
    } catch (e) {
      alert(e instanceof Error ? e.message : String(e));
    }
  }

  async function revokeAllDevices() {
    if (!confirm('Revoke every trusted device? You and every other phone/browser will need the master password again.')) return;
    try {
      await api.revokeAllTrustedDevices();
      // This browser's own enrolment just got wiped server-side —
      // clear the local mirror too so the checkbox reappears.
      clearLocalTrustedDevice();
      window.location.reload();
    } catch (e) {
      alert(e instanceof Error ? e.message : String(e));
    }
  }

  function deviceShortUA(ua: string | null): string {
    if (!ua) return 'Unknown browser';
    // Heuristic: pluck browser + platform from a UA string.
    const isMobile = /Mobi|Android|iPhone|iPad/i.test(ua);
    let browser = 'Browser';
    if (/Firefox/.test(ua)) browser = 'Firefox';
    else if (/Edg/.test(ua)) browser = 'Edge';
    else if (/Brave/.test(ua)) browser = 'Brave';
    else if (/Chrome/.test(ua)) browser = 'Chrome';
    else if (/Safari/.test(ua)) browser = 'Safari';
    let os = '';
    if (/Windows/.test(ua)) os = 'Windows';
    else if (/Mac OS X/.test(ua) || /Macintosh/.test(ua)) os = 'macOS';
    else if (/iPhone|iPad|iOS/i.test(ua)) os = 'iOS';
    else if (/Android/.test(ua)) os = 'Android';
    else if (/Linux/.test(ua)) os = 'Linux';
    const suffix = isMobile && os ? `${os} (mobile)` : os;
    return suffix ? `${browser} — ${suffix}` : browser;
  }

  onMount(() => {
    loadDevices();
    loadTotpStatus();
  });
</script>

<section class="panel">
  <div class="section-head">
    <h2>Security</h2>
    <p>Master password and the local security audit trail.</p>
  </div>

  <div class="field">
    <div class="field-label">
      <label>
        Change master password
        <InfoBubble text="Re-wraps every stored app password and PGP private key under a new key, and re-encrypts the database. You'll need the current password and be logged out at the end." />
      </label>
    </div>
    <form class="change-pw" onsubmit={async (e) => {
      e.preventDefault();
      const form = e.currentTarget as HTMLFormElement;
      const data = new FormData(form);
      const old_pw = data.get('old_pw') as string;
      const new_pw = data.get('new_pw') as string;
      const confirm_pw = data.get('confirm_pw') as string;
      if (new_pw.length < 8) { alert('New password must be at least 8 characters.'); return; }
      if (new_pw !== confirm_pw) { alert('New passwords do not match.'); return; }
      try {
        await api.vaultChangePassword(old_pw, new_pw);
        alert('Master password changed. You will be locked out — please unlock with the new password.');
        await lockVault();
      } catch (err) {
        alert(err instanceof Error ? err.message : String(err));
      }
    }}>
      <input type="password" name="old_pw" placeholder="Current password" required />
      <input type="password" name="new_pw" placeholder="New password (min 8 chars)" required />
      <input type="password" name="confirm_pw" placeholder="Confirm new password" required />
      <button type="submit" class="btn danger">Change password</button>
    </form>
  </div>

  <!-- ─────────── Two-factor authentication ─────────── -->
  <div class="field">
    <div class="field-label">
      <label>
        Two-factor authentication (TOTP)
        <InfoBubble text="Adds a 6-digit code from your authenticator app to vault unlock. Standard TOTP (RFC 6238) — works with Google Authenticator, 1Password, Bitwarden, Aegis, your phone's keychain, etc. Nothing leaves your machine to set this up." />
      </label>
    </div>

    {#if totpLoading && !totpEnabled && !enrollPayload}
      <p class="muted">Loading…</p>
    {:else if revealedRecoveryCodes.length > 0}
      <!-- Just-enrolled state: show the recovery codes once. -->
      <p class="totp-good">✓ Two-factor authentication is on.</p>
      <div class="totp-recovery-block">
        <p>
          <strong>Save these recovery codes now.</strong> They let you back in if
          you lose your authenticator device. Each works once.
          <em>This is the only time you'll see them.</em>
        </p>
        <ul class="recovery-list">
          {#each revealedRecoveryCodes as code (code)}
            <li><code>{code}</code></li>
          {/each}
        </ul>
        <div class="actions">
          <button type="button" class="btn" onclick={copyRecoveryCodes}>Copy all</button>
          <button
            type="button"
            class="btn primary"
            onclick={() => {
              if (
                confirm(
                  "Have you saved the recovery codes somewhere safe? They won't be shown again."
                )
              ) {
                revealedRecoveryCodes = [];
              }
            }}
          >I've saved them</button>
        </div>
      </div>
    {:else if enrollPayload}
      <!-- Enrollment-in-progress: QR + manual code + confirm. -->
      <div class="totp-enroll-block">
        <p>
          Scan this QR with your authenticator app, or paste the code below
          manually. Then enter the 6-digit code your app generates to confirm.
        </p>
        <div class="enroll-row">
          <img class="qr" src={enrollPayload.qr_png_data_url} alt="TOTP QR code" />
          <div class="manual">
            <span class="muted">Manual setup code:</span>
            <code class="manual-secret">{enrollPayload.secret}</code>
            <button
              type="button"
              class="btn small"
              onclick={() => navigator.clipboard?.writeText(enrollPayload?.secret ?? '')}
            >Copy code</button>
          </div>
        </div>
        <form
          class="confirm-row"
          onsubmit={(e) => {
            e.preventDefault();
            confirmEnrollment();
          }}
        >
          <input
            type="text"
            inputmode="numeric"
            pattern="[0-9]*"
            maxlength="6"
            placeholder="6-digit code"
            bind:value={confirmCode}
            autocomplete="one-time-code"
          />
          <button type="submit" class="btn primary" disabled={confirmBusy}>
            {confirmBusy ? 'Confirming…' : 'Confirm &amp; enable'}
          </button>
          <button type="button" class="btn" onclick={cancelEnrollment} disabled={confirmBusy}>
            Cancel
          </button>
        </form>
        {#if confirmErr}
          <p class="err-bubble">⚠ {confirmErr}</p>
        {/if}
      </div>
    {:else if totpEnabled}
      <!-- Enabled state: show status + disable form. -->
      <p class="totp-good">
        ✓ Two-factor authentication is on.
        {recoveryRemaining} recovery code{recoveryRemaining === 1 ? '' : 's'} unused.
      </p>
      <div class="totp-disable-block">
        <p>To turn it off, enter a current code (or a recovery code) to confirm.</p>
        <form
          class="confirm-row"
          onsubmit={(e) => {
            e.preventDefault();
            disableTotp();
          }}
        >
          {#if !disableUseRecovery}
            <input
              type="text"
              inputmode="numeric"
              pattern="[0-9]*"
              maxlength="6"
              placeholder="6-digit code"
              bind:value={disableCode}
              autocomplete="one-time-code"
            />
          {:else}
            <input
              type="text"
              autocomplete="off"
              spellcheck="false"
              placeholder="ABCD-EFGH-IJKL-MNOP-QRST-UV"
              bind:value={disableRecoveryCode}
            />
          {/if}
          <button type="submit" class="btn danger" disabled={disableBusy}>
            {disableBusy ? 'Disabling…' : 'Disable'}
          </button>
          <button
            type="button"
            class="btn"
            onclick={() => {
              disableUseRecovery = !disableUseRecovery;
              disableErr = null;
            }}
          >
            {disableUseRecovery ? 'Use authenticator code' : 'Use recovery code'}
          </button>
        </form>
        {#if disableErr}
          <p class="err-bubble">⚠ {disableErr}</p>
        {/if}
      </div>
    {:else}
      <!-- Off state: enable button. -->
      <p class="muted">
        Off. Vault unlock asks for your master password only.
        {#if totpPending}
          <span class="warn-inline">A previous setup was started but not confirmed — starting again will replace it.</span>
        {/if}
      </p>
      <button type="button" class="btn primary" onclick={startEnrollment}>
        Enable two-factor authentication
      </button>
    {/if}
  </div>

  <div class="row">
    <div class="label">
      <strong class="inline">
        Security audit log
        <InfoBubble text="Local ledger of vault access, IP shifts, account changes, and policy adjustments. View the Activity tab from here for sync/send events instead." />
      </strong>
    </div>
    <a class="btn" href="/settings/audit">Inspect →</a>
  </div>

  <div class="row">
    <div class="label">
      <strong class="inline">
        Event ticker
        <InfoBubble text="Scroll every new audit + activity event once along the bottom of the screen — a live-feed style monitor for sync cycles, send outcomes, folder changes, and security events. Only events that happen after you enable this appear; historical entries never replay." />
      </strong>
      <span class="field-sub">
        {eventTicker ? 'On — new events scroll along the bottom.' : 'Off.'}
      </span>
    </div>
    <label class="switch" title={eventTicker ? 'Turn off' : 'Turn on'}>
      <input type="checkbox" checked={eventTicker} onchange={(e) => setEventTicker((e.currentTarget as HTMLInputElement).checked)} />
      <span class="track"></span>
    </label>
  </div>

  <!-- Trusted devices — browsers that skip the IP-change auto-lock.
       Pro-only: the concept is meaningless on a localhost build. -->
  {#if $tier.features.trusted_devices}
  <div class="field">
    <div class="field-label">
      <label>
        Trusted devices
        <InfoBubble text="Browsers that ticked 'Remember this device' at unlock. Each token is random, server-verified, and revocable — the actual value only lives in the cookie. IP-change auto-lock is skipped while a valid token is present. Tokens expire after 30 days regardless." />
      </label>
    </div>

    {#if devicesLoading && !devicesLoaded}
      <p class="muted">Loading devices…</p>
    {:else if devices.length === 0}
      <p class="muted">No remembered devices yet. Tick "Remember this device" next time you unlock on a browser you trust.</p>
    {:else}
      <ul class="device-list">
        {#each pagedDevices as d (d.id)}
          <li>
            <div class="device-copy">
              <strong>{deviceShortUA(d.user_agent)}</strong>
              <span class="device-meta">
                {#if d.last_seen_at}
                  Last active {formatDate(d.last_seen_at)}
                  {#if d.last_seen_ip}· {d.last_seen_ip}{/if}
                {:else}
                  Enrolled {formatDate(d.created_at)}
                {/if}
                · expires {formatDate(d.expires_at)}
              </span>
            </div>
            <button class="btn danger" onclick={() => revokeDevice(d.id)}>Revoke</button>
          </li>
        {/each}
      </ul>
      {#if totalDevicePages > 1}
        <div class="pager" aria-label="Device pages">
          <button
            type="button"
            class="btn ghost"
            disabled={devicesPage === 0}
            onclick={devicesPrev}
          >Previous</button>
          <span class="pager-status">
            Page {devicesPage + 1} of {totalDevicePages}
            <span class="pager-detail">— {devices.length} devices total</span>
          </span>
          <button
            type="button"
            class="btn ghost"
            disabled={devicesPage >= totalDevicePages - 1}
            onclick={devicesNext}
          >Next</button>
        </div>
      {/if}
      <div class="device-actions">
        <button class="btn ghost" type="button" onclick={loadDevices}>Refresh</button>
        <button class="btn danger" type="button" onclick={revokeAllDevices}>
          Revoke all devices
        </button>
      </div>
    {/if}
  </div>
  {/if}
</section>

<style>
  /* TOTP enrollment + status — scoped to this component so the
     existing global .btn / .field / .muted styles still apply. */
  .totp-good {
    color: #10b981;
    font-weight: 500;
    margin: 0.5rem 0;
  }
  .totp-recovery-block,
  .totp-enroll-block,
  .totp-disable-block {
    margin-top: 0.6rem;
    padding: 0.85rem 1rem;
    border: 1px solid color-mix(in oklab, currentColor 12%, transparent);
    border-radius: 0.7rem;
    background: color-mix(in oklab, currentColor 3%, transparent);
  }
  .totp-recovery-block p {
    margin: 0 0 0.6rem;
    line-height: 1.5;
  }
  .totp-recovery-block em {
    color: color-mix(in oklab, tomato 80%, currentColor);
    font-style: normal;
    font-weight: 500;
  }
  .recovery-list {
    list-style: none;
    margin: 0 0 0.7rem;
    padding: 0;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(13rem, 1fr));
    gap: 0.45rem;
  }
  .recovery-list li {
    margin: 0;
  }
  .recovery-list code {
    display: block;
    padding: 0.5rem 0.7rem;
    background: var(--surface, #f5f5f7);
    border: 1px solid color-mix(in oklab, currentColor 10%, transparent);
    border-radius: 0.4rem;
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 0.84rem;
    user-select: all;
  }
  .actions {
    display: inline-flex;
    gap: 0.5rem;
  }
  .enroll-row {
    display: flex;
    flex-wrap: wrap;
    gap: 1.2rem;
    align-items: center;
    margin: 0.8rem 0;
  }
  .qr {
    width: 200px;
    height: 200px;
    border: 1px solid color-mix(in oklab, currentColor 10%, transparent);
    border-radius: 0.5rem;
    background: white;
    padding: 0.4rem;
    flex-shrink: 0;
  }
  .manual {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
    min-width: 0;
    flex: 1;
  }
  .manual-secret {
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 0.85rem;
    padding: 0.5rem 0.7rem;
    background: var(--surface, #f5f5f7);
    border: 1px solid color-mix(in oklab, currentColor 10%, transparent);
    border-radius: 0.4rem;
    word-break: break-all;
    user-select: all;
  }
  .confirm-row {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
    align-items: center;
    margin-top: 0.4rem;
  }
  .confirm-row input[type='text'] {
    flex: 1;
    min-width: 12rem;
    font: inherit;
    font-size: 0.9rem;
    padding: 0.55rem 0.75rem;
    border: 1px solid color-mix(in oklab, currentColor 18%, transparent);
    border-radius: 0.5rem;
    background: var(--surface, #f5f5f7);
    color: inherit;
  }
  .err-bubble {
    margin-top: 0.5rem;
    padding: 0.5rem 0.7rem;
    background: color-mix(in oklab, tomato 12%, transparent);
    border-left: 2px solid tomato;
    border-radius: 0 0.5rem 0.5rem 0;
    font-size: 0.84rem;
  }
  .warn-inline {
    display: block;
    margin-top: 0.4rem;
    color: color-mix(in oklab, orange 75%, currentColor);
    font-size: 0.82rem;
  }
  /* Existing global .btn doesn't define a 'small' modifier — add a
     local one so the manual-code copy button doesn't dwarf the
     paragraph next to it. */
  :global(.btn.small) {
    padding: 0.35rem 0.7rem;
    font-size: 0.78rem;
  }

  /* Trusted-devices pager. Shares the visual rhythm of the AI
     activity / history pagers so the panels feel of-a-set. */
  .pager {
    display: flex;
    align-items: center;
    gap: 0.7rem;
    margin: 0.6rem 0 0.4rem;
    padding: 0.55rem 0.75rem;
    background: color-mix(in oklab, currentColor 4%, transparent);
    border-radius: 0.5rem;
    font-size: 0.82rem;
  }
  .pager-status {
    flex: 1 1 auto;
    text-align: center;
    color: color-mix(in oklab, currentColor 70%, transparent);
  }
  .pager-detail {
    opacity: 0.7;
    font-size: 0.78rem;
    margin-left: 0.3rem;
  }
</style>
