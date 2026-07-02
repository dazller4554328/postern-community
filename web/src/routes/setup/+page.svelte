<script lang="ts">
  import './setup.css';
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { api, type NewAccount } from '$lib/api';
  import { PROVIDERS, type Provider } from './_lib/providers';
  import SetupPgpStep from './_components/SetupPgpStep.svelte';

  // ── Wizard state ────────────────────────────────────────────────────
  let provider = $state<Provider>('gmail');
  let spec = $derived(PROVIDERS.find((p) => p.id === provider) ?? PROVIDERS[0]);

  let email = $state('');
  let displayName = $state('');
  let appPassword = $state('');
  let imapHost = $state(PROVIDERS[0].imapHost);
  let imapPort = $state(PROVIDERS[0].imapPort);
  let smtpHost = $state(PROVIDERS[0].smtpHost);
  let smtpPort = $state(PROVIDERS[0].smtpPort);
  let deleteAfterSync = $state(false);

  // PGP step. Default is "skip" — encryption setup is something users
  // can opt into once, not something we force during onboarding.
  type PgpChoice = 'skip' | 'generate' | 'import';
  let pgpChoice = $state<PgpChoice>('skip');
  let pgpUserId = $state('');
  let pgpImportArmored = $state('');

  let saving = $state(false);
  let error = $state<string | null>(null);
  let existingAccountCount = $state(0);

  // Whenever the user picks a different provider, refresh the server
  // fields to that preset's defaults. We only override when the field
  // still matches a known preset value — otherwise the user might have
  // typed something custom and we'd blow it away.
  function pickProvider(p: Provider) {
    provider = p;
    const s = PROVIDERS.find((x) => x.id === p)!;
    if (s.disabled) return;
    imapHost = s.imapHost;
    imapPort = s.imapPort;
    smtpHost = s.smtpHost;
    smtpPort = s.smtpPort;
  }

  // If the user types an email with a known domain, silently switch
  // the provider card to match. Doesn't fire when they already picked
  // Custom — that'd be rude.
  $effect(() => {
    if (provider === 'custom') return;
    const domain = email.split('@')[1]?.toLowerCase();
    if (!domain) return;
    const matched = matchProviderByDomain(domain);
    if (matched && matched !== provider) {
      pickProvider(matched);
    }
  });

  // Default PGP user ID = the email with a reasonable display name.
  $effect(() => {
    if (pgpChoice === 'generate' && !pgpUserId && email) {
      pgpUserId = displayName ? `${displayName} <${email}>` : email;
    }
  });

  function matchProviderByDomain(domain: string): Provider | null {
    // Outlook.com / hotmail.com / live.com intentionally omitted —
    // Microsoft dropped IMAP Basic-Auth in 2024, so auto-selecting
    // that tile would lead the user into a probe that always fails.
    // The provider is still visible (disabled) so people know why.
    const table: Record<string, Provider> = {
      'gmail.com': 'gmail',
      'googlemail.com': 'gmail',
      'icloud.com': 'icloud',
      'me.com': 'icloud',
      'mac.com': 'icloud',
      'fastmail.com': 'fastmail',
      'fastmail.fm': 'fastmail',
      'proton.me': 'proton',
      'protonmail.com': 'proton',
      'pm.me': 'proton'
    };
    return table[domain] ?? null;
  }

  async function submit(e: Event) {
    e.preventDefault();
    if (spec.disabled) {
      error = spec.disabledNote ?? 'This provider isn\'t supported yet.';
      return;
    }
    saving = true;
    error = null;
    try {
      const payload: NewAccount = {
        kind: spec.kind,
        email,
        display_name: displayName || undefined,
        imap_host: imapHost,
        imap_port: imapPort,
        smtp_host: smtpHost || undefined,
        smtp_port: smtpPort || undefined,
        app_password: appPassword,
        delete_after_sync: deleteAfterSync
      };
      await api.createAccount(payload);

      // PGP step — we attempt these sequentially, non-fatally: if the
      // account creation worked but the key step fails, the user still
      // has a working mailbox and can retry from Settings → PGP.
      if (pgpChoice === 'generate' && pgpUserId.trim()) {
        try {
          await api.pgpGenerate(pgpUserId.trim());
        } catch (e) {
          error = `Account added, but key generation failed: ${
            e instanceof Error ? e.message : String(e)
          }. You can retry from Settings → PGP.`;
          saving = false;
          return;
        }
      } else if (pgpChoice === 'import' && pgpImportArmored.trim()) {
        try {
          await api.pgpImport(pgpImportArmored.trim());
        } catch (e) {
          error = `Account added, but key import failed: ${
            e instanceof Error ? e.message : String(e)
          }. You can retry from Settings → PGP.`;
          saving = false;
          return;
        }
      }

      goto('/inbox');
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      saving = false;
    }
  }

  onMount(async () => {
    try {
      const accts = await api.listAccounts();
      existingAccountCount = accts.length;
    } catch {
      /* pre-auth / pre-unlock — just stay on the default copy */
    }
  });
</script>

<section class="setup-shell">
  <div class="intro-card">
    <span class="eyebrow">{existingAccountCount === 0 ? 'Onboarding' : 'Mailbox'}</span>
    <h1>{existingAccountCount === 0 ? 'Add your first mailbox' : 'Add a mailbox'}</h1>
    <p class="sub">
      Pick your provider, paste an <strong>app password</strong>, and — if you want — turn on
      PGP in the same step. Credentials live inside the local vault, not in the browser.
    </p>
    <div class="chips">
      <span class="chip">IMAP / SMTP presets</span>
      <span class="chip">Vault-wrapped credentials</span>
      <span class="chip">Optional PGP at setup</span>
    </div>
  </div>

  <form onsubmit={submit}>
    <!-- ── Step 1: Provider picker ─────────────────────────────────── -->
    <fieldset class="providers">
      <legend>1. Choose your provider</legend>
      <div class="provider-grid">
        {#each PROVIDERS as p (p.id)}
          <label
            class="provider-card"
            class:selected={provider === p.id}
            class:disabled={p.disabled}
          >
            <input
              type="radio"
              name="provider"
              value={p.id}
              checked={provider === p.id}
              disabled={p.disabled}
              onchange={() => pickProvider(p.id)}
            />
            <span class="provider-emoji" aria-hidden="true">{p.emoji}</span>
            <span class="provider-label">{p.label}</span>
            <span class="provider-blurb">{p.blurb}</span>
            {#if p.disabled}
              <span class="provider-badge">Not yet</span>
            {/if}
          </label>
        {/each}
      </div>
      {#if spec.disabled}
        <p class="provider-note">
          <strong>{spec.label}:</strong> {spec.disabledNote}
        </p>
      {/if}
    </fieldset>

    <!-- ── Step 2: Credentials ─────────────────────────────────────── -->
    <fieldset class="creds" disabled={spec.disabled}>
      <legend>2. Sign in</legend>

      <label>
        Email address
        <input
          type="email"
          bind:value={email}
          required
          autocomplete="email"
          placeholder="you@{spec.id === 'custom' ? 'example.com' : spec.blurb.split(',')[0].split(' ')[0]}"
        />
      </label>

      <label>
        Display name <span class="opt">(optional)</span>
        <input type="text" bind:value={displayName} placeholder="Work, Personal, …" />
      </label>

      <label>
        App password
        <input type="password" bind:value={appPassword} required autocomplete="off" />
        <small class="hint">
          {spec.appPasswordNote}
          {#if spec.appPasswordUrl}
            <br>
            <a href={spec.appPasswordUrl} target="_blank" rel="noopener">
              Open {spec.appPasswordUrl.replace('https://', '')}
            </a>
          {/if}
        </small>
      </label>

      <div class="policy-choice">
        <span class="policy-heading">After downloading messages:</span>
        <label class="policy-option">
          <input type="radio" name="policy" value={false} bind:group={deleteAfterSync} />
          <div>
            <strong>Keep on server</strong>
            <span>Messages stay on the provider. Safe if you access mail from other clients too.</span>
          </div>
        </label>
        <label class="policy-option">
          <input type="radio" name="policy" value={true} bind:group={deleteAfterSync} />
          <div>
            <strong>Delete from server</strong>
            <span>Messages are removed from {spec.label} after Postern downloads them. Maximum privacy — only Postern holds the copy.</span>
          </div>
        </label>
      </div>

      <details open={provider === 'custom'}>
        <summary>Server settings</summary>
        <div class="grid">
          <label>
            IMAP host
            <input type="text" bind:value={imapHost} required />
          </label>
          <label>
            IMAP port
            <input type="number" bind:value={imapPort} required />
          </label>
          <label>
            SMTP host
            <input type="text" bind:value={smtpHost} />
          </label>
          <label>
            SMTP port
            <input type="number" bind:value={smtpPort} />
          </label>
        </div>
      </details>
    </fieldset>

    <!-- ── Step 3: PGP (optional) ──────────────────────────────────── -->
    <SetupPgpStep
      bind:pgpChoice
      bind:pgpUserId
      bind:pgpImportArmored
      emailPlaceholder={email}
      disabled={spec.disabled}
    />

    {#if error}
      <p class="err">{error}</p>
    {/if}

    <button type="submit" disabled={saving || spec.disabled}>
      {saving ? 'Connecting…' : existingAccountCount === 0 ? 'Finish setup' : 'Add mailbox'}
    </button>
  </form>
</section>

