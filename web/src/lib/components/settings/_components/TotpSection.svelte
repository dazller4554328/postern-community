<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import InfoBubble from '$lib/components/InfoBubble.svelte';

  // Mirror key in localStorage. VaultGate (the unlock screen) reads
  // this on mount because /api/auth/totp/status hits the encrypted
  // DB and fails while the vault is locked — without the mirror,
  // first refresh after a logout shows only the password field.
  const TOTP_LOCAL_KEY = 'postern.totpEnabled';

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
  // The user must save these before navigating away — once the panel
  // re-renders without them, they're lost (we only keep hashes
  // server-side).
  let revealedRecoveryCodes = $state<string[]>([]);

  let disableCode = $state('');
  let disableUseRecovery = $state(false);
  let disableRecoveryCode = $state('');
  let disableBusy = $state(false);
  let disableErr = $state<string | null>(null);

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

  onMount(loadTotpStatus);

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
        recoveryCode: disableUseRecovery ? disableRecoveryCode.trim() : undefined,
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
    navigator.clipboard?.writeText(text).catch(() => {
      /* clipboard blocked — user can still select + copy manually */
    });
  }
</script>

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

<style>
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
</style>
