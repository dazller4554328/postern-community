<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import {
    vaultState,
    trustedDevice,
    localTrustedDevice,
    refreshVaultState,
    initVault,
    unlockVault
  } from '$lib/vault';

  let { children } = $props();

  let loading = $state(true);
  let password = $state('');
  let confirm = $state('');
  let show = $state(false);
  let err = $state<string | null>(null);
  let busy = $state(false);
  // Default to on for touch devices — those are the ones that would
  // get booted constantly by the IP check. Desktop users decide.
  let rememberDevice = $state(
    typeof window !== 'undefined' && window.matchMedia?.('(pointer: coarse)').matches
  );

  // 2FA — fetched at mount from /api/auth/totp/status, but seeded
  // from a localStorage mirror first because the server endpoint
  // queries the SQLCipher-encrypted DB and 500s while the vault is
  // locked. Without the mirror, the unlock form shows only the
  // password field on first refresh, the user submits, and the
  // server then rejects with "missing 2FA code". The mirror is
  // best-effort UX state — the server is still authoritative on
  // whether the submitted code is correct.
  const TOTP_LOCAL_KEY = 'postern.totpEnabled';
  let totpEnabled = $state(
    typeof localStorage !== 'undefined' && localStorage.getItem(TOTP_LOCAL_KEY) === '1'
  );
  let totpCode = $state('');
  let useRecovery = $state(false);
  let recoveryCode = $state('');

  // Hide the checkbox once the device is already enrolled from either
  // side — the server's cookie check (`trustedDevice`) or the
  // client-side mirror (`localTrustedDevice`). The mirror catches the
  // pre-unlock case where the server can't read the encrypted
  // devices table.
  let hideRemember = $derived($trustedDevice || $localTrustedDevice);

  onMount(async () => {
    try {
      await refreshVaultState();
      try {
        const ts = await api.authTotpStatus();
        totpEnabled = ts.enabled;
        // Sync the mirror so a future locked-state load gets the
        // correct field on first paint without needing a roundtrip.
        try {
          localStorage.setItem(TOTP_LOCAL_KEY, ts.enabled ? '1' : '0');
        } catch {
          /* private mode / quota — non-fatal */
        }
      } catch {
        // Endpoint unreachable (most commonly: vault locked, the
        // status query against the encrypted DB 500s). Keep
        // whatever the localStorage mirror seeded — if it was on,
        // show the field; if off, hide it.
      }
    } catch (e) {
      err = e instanceof Error ? e.message : String(e);
    } finally {
      loading = false;
    }
  });

  async function onSubmit(e: Event) {
    e.preventDefault();
    err = null;
    if ($vaultState === 'uninitialized') {
      if (password.length < 8) {
        err = 'Pick at least 8 characters. Longer is better.';
        return;
      }
      if (password !== confirm) {
        err = 'The two passwords do not match.';
        return;
      }
    } else {
      if (!password) {
        err = 'Enter your master password.';
        return;
      }
      if (totpEnabled) {
        if (useRecovery) {
          if (!recoveryCode.trim()) {
            err = 'Enter a recovery code.';
            return;
          }
        } else if (!/^\d{6}$/.test(totpCode.trim())) {
          err = 'Enter the 6-digit code from your authenticator app.';
          return;
        }
      }
    }
    busy = true;
    try {
      if ($vaultState === 'uninitialized') {
        await initVault(password);
      } else {
        // When the checkbox is hidden (already-enrolled device), we
        // still force remember=true so the server re-issues a fresh
        // 30-day cookie on every unlock. That keeps the trust rolling
        // forward instead of silently expiring after 30 days.
        const effectiveRemember = hideRemember ? true : rememberDevice;
        await unlockVault(password, effectiveRemember, {
          totpCode: !useRecovery && totpEnabled ? totpCode.trim() : undefined,
          recoveryCode:
            useRecovery && totpEnabled ? recoveryCode.trim() : undefined
        });
      }
      password = '';
      confirm = '';
      totpCode = '';
      recoveryCode = '';
    } catch (e) {
      err = e instanceof Error ? e.message : String(e);
      // Self-heal: if the server says we missed a 2FA code, the
      // user has TOTP enabled but our pre-unlock probe (DB read
      // while vault locked) couldn't tell us. Flip the field on
      // and persist so future loads of the unlock screen pick it
      // up immediately.
      if (err && /two-factor|2fa/i.test(err) && !totpEnabled) {
        totpEnabled = true;
        try {
          localStorage.setItem(TOTP_LOCAL_KEY, '1');
        } catch {
          /* private mode / quota — non-fatal */
        }
      }
    } finally {
      busy = false;
    }
  }
</script>

{#if loading}
  <div class="loading">Checking vault…</div>
{:else if $vaultState === 'unlocked'}
  {@render children?.()}
{:else}
  <div class="screen">
    <div class="card">
      <div class="brand-top">
        <span class="brand-mark" aria-hidden="true"></span>
        <img src="/logo-light.png" alt="Postern" class="logo logo-light" />
        <img src="/logo-dark.png" alt="" class="logo logo-dark" aria-hidden="true" />
        <img src="/logo-cyberpunk.png" alt="" class="logo logo-cyberpunk" aria-hidden="true" />
      </div>
      <div class="status-chips">
        <span class="status-chip">Argon2id</span>
        <span class="status-chip">ChaCha20-Poly1305</span>
        <span class="status-chip">Local secrets only</span>
      </div>
      <h1>
        {#if $vaultState === 'uninitialized'}
          Set your master password
        {:else}
          Unlock Postern
        {/if}
      </h1>
      <p class="lede">
        {#if $vaultState === 'uninitialized'}
          Pick a master password. It encrypts your stored credentials and PGP keys
          and is never written to disk.
          <strong>If you forget it, your secrets can't be recovered.</strong>
        {:else}
          Enter your password.
        {/if}
      </p>

      <form onsubmit={onSubmit}>
        <label>
          <span>Master password</span>
          <div class="pw-wrap">
            <input
              type={show ? 'text' : 'password'}
              autocomplete={$vaultState === 'uninitialized' ? 'new-password' : 'current-password'}
              bind:value={password}
              required
            />
            <button
              type="button"
              class="eye"
              aria-label={show ? 'Hide password' : 'Show password'}
              title={show ? 'Hide' : 'Show'}
              onclick={() => (show = !show)}
            >
              {#if show}
                <svg viewBox="0 0 20 20" width="16" height="16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
                  <path d="M3 3l14 14"/>
                  <path d="M10.6 10.6a2 2 0 0 1-2.8-2.8"/>
                  <path d="M6.2 6.3C3.6 7.8 2 10 2 10s3 5.5 8 5.5c1.4 0 2.7-.4 3.8-1.1"/>
                  <path d="M9.2 4.6c.3 0 .5-.1.8-.1 5 0 8 5.5 8 5.5a14 14 0 0 1-2.2 2.7"/>
                </svg>
              {:else}
                <svg viewBox="0 0 20 20" width="16" height="16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
                  <path d="M2 10s3-5.5 8-5.5S18 10 18 10s-3 5.5-8 5.5S2 10 2 10z"/>
                  <circle cx="10" cy="10" r="2.2"/>
                </svg>
              {/if}
            </button>
          </div>
        </label>
        {#if $vaultState === 'uninitialized'}
          <label>
            <span>Confirm password</span>
            <input
              type={show ? 'text' : 'password'}
              autocomplete="new-password"
              bind:value={confirm}
              required
            />
          </label>
        {/if}
        {#if $vaultState !== 'uninitialized' && totpEnabled}
          {#if !useRecovery}
            <label>
              <span>Two-factor code</span>
              <input
                type="text"
                inputmode="numeric"
                autocomplete="one-time-code"
                pattern="[0-9]*"
                maxlength="6"
                placeholder="6-digit code"
                bind:value={totpCode}
              />
            </label>
            <button
              type="button"
              class="linklike"
              onclick={() => {
                useRecovery = true;
                totpCode = '';
              }}
            >Use a recovery code instead</button>
          {:else}
            <label>
              <span>Recovery code</span>
              <input
                type="text"
                autocomplete="off"
                spellcheck="false"
                placeholder="ABCD-EFGH-IJKL-MNOP-QRST-UV"
                bind:value={recoveryCode}
              />
            </label>
            <button
              type="button"
              class="linklike"
              onclick={() => {
                useRecovery = false;
                recoveryCode = '';
              }}
            >Back to authenticator code</button>
          {/if}
        {/if}
        {#if $vaultState !== 'uninitialized' && !hideRemember}
          <label class="remember">
            <input type="checkbox" bind:checked={rememberDevice} />
            <span>Remember this device for 30 days</span>
          </label>
        {/if}
        {#if err}
          <div class="err">⚠ {err}</div>
        {/if}
        <button type="submit" disabled={busy}>
          {#if busy}
            Working…
          {:else if $vaultState === 'uninitialized'}
            Set password &amp; unlock
          {:else}
            Unlock
          {/if}
        </button>
      </form>
    </div>
  </div>
{/if}

<style>
  .loading, .screen {
    min-height: 100dvh;
    display: grid;
    place-items: center;
    padding: 2rem;
  }
  .loading { opacity: 0.5; }

  .card {
    width: 100%;
    max-width: 31rem;
    padding: 2rem 2.1rem;
    background:
      radial-gradient(circle at top right, color-mix(in oklab, var(--accent) 10%, transparent), transparent 30%),
      var(--surface);
    border: 1px solid var(--border);
    border-radius: 1.35rem;
    box-shadow: 0 20px 40px rgba(0, 0, 0, 0.12);
  }
  .brand-top {
    display: flex;
    align-items: center;
    gap: 0.8rem;
    margin-bottom: 0.9rem;
  }
  .brand-mark {
    width: 1.05rem;
    height: 1.05rem;
    border-radius: 0.3rem;
    background:
      linear-gradient(135deg, var(--accent), color-mix(in oklab, var(--accent) 40%, white 60%));
    box-shadow:
      0 0 0 1px color-mix(in oklab, var(--accent) 18%, transparent),
      0 10px 22px color-mix(in oklab, var(--accent) 18%, transparent);
    flex-shrink: 0;
  }
  .logo {
    display: block;
    height: 32px;
    width: auto;
  }
  .logo-dark, .logo-cyberpunk { display: none; }
  /* Dark-canvas themes all use the dark logo. Cyberpunk keeps its custom logo. */
  :global([data-theme='dark']) .logo-light,
  :global([data-theme='solarized-dark']) .logo-light,
  :global([data-theme='dracula']) .logo-light,
  :global([data-theme='nord']) .logo-light,
  :global([data-theme='gruvbox']) .logo-light,
  :global([data-theme='monokai']) .logo-light,
  :global([data-theme='forest']) .logo-light,
  :global([data-theme='rose-pine']) .logo-light,
  :global([data-theme='acid-rain']) .logo-light,
  :global([data-theme='volcanic']) .logo-light,
  :global([data-theme='abyssal']) .logo-light,
  :global([data-theme='arcade']) .logo-light { display: none; }
  :global([data-theme='dark']) .logo-dark,
  :global([data-theme='solarized-dark']) .logo-dark,
  :global([data-theme='dracula']) .logo-dark,
  :global([data-theme='nord']) .logo-dark,
  :global([data-theme='gruvbox']) .logo-dark,
  :global([data-theme='monokai']) .logo-dark,
  :global([data-theme='forest']) .logo-dark,
  :global([data-theme='rose-pine']) .logo-dark,
  :global([data-theme='acid-rain']) .logo-dark,
  :global([data-theme='volcanic']) .logo-dark,
  :global([data-theme='abyssal']) .logo-dark,
  :global([data-theme='arcade']) .logo-dark { display: block; }
  :global([data-theme='cyberpunk']) .logo-light { display: none; }
  :global([data-theme='cyberpunk']) .logo-cyberpunk { display: block; }
  @media (prefers-color-scheme: dark) {
    :global(html:not([data-theme])) .logo-light { display: none; }
    :global(html:not([data-theme])) .logo-dark { display: block; }
  }

  .status-chips {
    display: flex;
    flex-wrap: wrap;
    gap: 0.45rem;
    margin-bottom: 1rem;
  }
  .status-chip {
    display: inline-flex;
    align-items: center;
    padding: 0.42rem 0.72rem;
    border-radius: 999px;
    background: color-mix(in oklab, var(--surface-2) 82%, transparent);
    border: 1px solid color-mix(in oklab, currentColor 8%, transparent);
    font-size: 0.72rem;
    font-weight: 600;
  }

  h1 {
    margin: 0 0 0.5rem;
    font-weight: 650;
    font-size: 1.42rem;
    letter-spacing: -0.02em;
  }
  .lede {
    margin: 0 0 1.25rem;
    font-size: 0.85rem;
    opacity: 0.75;
    line-height: 1.55;
  }
  .lede strong {
    display: block;
    margin-top: 0.5rem;
    color: color-mix(in oklab, tomato 80%, currentColor);
    font-weight: 500;
  }
  form {
    display: flex;
    flex-direction: column;
    gap: 0.85rem;
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
    font-size: 0.78rem;
    opacity: 0.75;
  }
  input {
    font: inherit;
    font-size: 0.9rem;
    padding: 0.72rem 0.82rem;
    border: 1px solid color-mix(in oklab, currentColor 18%, transparent);
    background: color-mix(in oklab, var(--surface-2) 74%, transparent);
    color: inherit;
    border-radius: 0.85rem;
    width: 100%;
    box-sizing: border-box;
  }
  .pw-wrap {
    position: relative;
  }
  .pw-wrap input {
    padding-right: 2.25rem;
  }
  .eye {
    position: absolute;
    right: 0.3rem;
    top: 50%;
    transform: translateY(-50%);
    background: transparent;
    border: 0;
    color: inherit;
    opacity: 0.55;
    cursor: pointer;
    padding: 0.3rem 0.45rem;
    border-radius: 0.25rem;
    display: inline-flex;
    align-items: center;
  }
  .eye:hover {
    opacity: 1;
    background: color-mix(in oklab, currentColor 8%, transparent);
  }
  input:focus {
    outline: none;
    border-color: color-mix(in oklab, var(--accent) 32%, transparent);
    box-shadow: 0 0 0 3px color-mix(in oklab, var(--accent) 12%, transparent);
  }
  .err {
    padding: 0.55rem 0.75rem;
    background: color-mix(in oklab, tomato 12%, transparent);
    border-left: 2px solid tomato;
    border-radius: 0 0.8rem 0.8rem 0;
    font-size: 0.82rem;
  }
  .linklike {
    align-self: flex-start;
    background: transparent;
    border: 0;
    color: color-mix(in oklab, var(--accent) 75%, currentColor);
    padding: 0;
    margin: 0;
    font: inherit;
    font-size: 0.78rem;
    cursor: pointer;
    text-decoration: underline;
    text-underline-offset: 2px;
  }
  .linklike:hover { opacity: 0.85; }
  .remember {
    display: inline-flex;
    align-items: center;
    gap: 0.55rem;
    font-size: 0.8rem;
    opacity: 0.82;
    cursor: pointer;
  }
  .remember input {
    width: 1.05rem;
    height: 1.05rem;
    accent-color: var(--accent);
    cursor: pointer;
    padding: 0;
  }
  .remember span {
    user-select: none;
  }
  button {
    margin-top: 0.4rem;
    font: inherit;
    font-size: 0.9rem;
    font-weight: 600;
    padding: 0.82rem 1.2rem;
    background: var(--accent);
    color: white;
    border: 0;
    border-radius: 999px;
    cursor: pointer;
  }
  button:hover:not(:disabled) {
    filter: brightness(0.97);
  }
  button:disabled { opacity: 0.55; cursor: progress; }

  /* Cyberpunk vault gate — neon card */
  :global([data-theme='cyberpunk']) .card {
    background:
      radial-gradient(circle at top right, rgba(187, 154, 247, 0.12), transparent 32%),
      linear-gradient(180deg, #0b0f1f, #090d18);
    border: 1px solid rgba(122, 162, 247, 0.22);
    box-shadow:
      0 0 20px rgba(122, 162, 247, 0.08),
      0 20px 40px rgba(0, 0, 0, 0.4);
  }
  :global([data-theme='cyberpunk']) .brand-mark {
    background: linear-gradient(135deg, #7aa2f7, #bb9af7);
    box-shadow:
      0 0 0 1px rgba(122, 162, 247, 0.25),
      0 0 18px rgba(122, 162, 247, 0.18);
  }
  :global([data-theme='cyberpunk']) .status-chip {
    background: rgba(122, 162, 247, 0.08);
    border-color: rgba(122, 162, 247, 0.18);
    color: #b6f5ff;
  }
  :global([data-theme='cyberpunk']) .card h1 {
    color: #7aa2f7;
    text-shadow: 0 0 12px rgba(122, 162, 247, 0.3);
  }
  :global([data-theme='cyberpunk']) .card button[type='submit'] {
    background: #7aa2f7;
    color: #000;
    box-shadow: 0 0 10px rgba(122, 162, 247, 0.3);
  }
  :global([data-theme='cyberpunk']) .card button[type='submit']:hover:not(:disabled) {
    box-shadow: 0 0 20px rgba(122, 162, 247, 0.5), 0 0 40px rgba(122, 162, 247, 0.15);
  }
  :global([data-theme='cyberpunk']) .card input {
    background: #08090f;
    border-color: rgba(122, 162, 247, 0.15);
  }
  :global([data-theme='cyberpunk']) .card input:focus {
    border-color: #7aa2f7;
    box-shadow: 0 0 8px rgba(122, 162, 247, 0.35);
  }
  :global([data-theme='cyberpunk']) .screen {
    background:
      radial-gradient(circle at top left, rgba(122, 162, 247, 0.08), transparent 30%),
      radial-gradient(circle at bottom right, rgba(187, 154, 247, 0.08), transparent 26%),
      #08090f;
  }
</style>
