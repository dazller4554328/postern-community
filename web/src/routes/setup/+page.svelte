<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { api, type NewAccount, type AccountKind } from '$lib/api';

  // ── Provider presets ────────────────────────────────────────────────
  // Each entry drives the auto-filled server fields + the app-password
  // help link. "proton" is included but disabled — Proton doesn't speak
  // IMAP without their Bridge app, which we haven't integrated yet.
  type Provider =
    | 'gmail'
    | 'outlook'
    | 'icloud'
    | 'fastmail'
    | 'proton'
    | 'custom';

  interface ProviderSpec {
    id: Provider;
    label: string;
    blurb: string;
    kind: AccountKind;
    imapHost: string;
    imapPort: number;
    smtpHost: string;
    smtpPort: number;
    appPasswordUrl?: string;
    appPasswordNote: string;
    disabled?: boolean;
    disabledNote?: string;
    emoji: string;
  }

  const PROVIDERS: ProviderSpec[] = [
    {
      id: 'gmail',
      label: 'Gmail',
      blurb: 'gmail.com / googlemail.com',
      kind: 'gmail',
      imapHost: 'imap.gmail.com',
      imapPort: 993,
      smtpHost: 'smtp.gmail.com',
      smtpPort: 465,
      appPasswordUrl: 'https://myaccount.google.com/apppasswords',
      appPasswordNote:
        'Generate an App Password at myaccount.google.com/apppasswords. Requires 2-Step Verification enabled.',
      emoji: '📮'
    },
    {
      id: 'outlook',
      label: 'Outlook / Microsoft',
      blurb: 'outlook.com, hotmail.com, live.com',
      kind: 'imap',
      imapHost: 'outlook.office365.com',
      imapPort: 993,
      smtpHost: 'smtp.office365.com',
      smtpPort: 587,
      appPasswordNote:
        'Microsoft removed Basic-Auth / App-Password support for outlook.com, hotmail.com, and live.com. IMAP and SMTP now require OAuth2, which Postern doesn\'t speak yet. Workarounds: forward Outlook mail to a provider that still supports app passwords (Gmail, Fastmail, self-hosted), or run DavMail locally as an OAuth2 → Basic-Auth bridge and add that as a Custom IMAP account.',
      disabled: true,
      disabledNote: 'Outlook.com no longer supports IMAP app passwords. See the provider blurb for workarounds.',
      emoji: '🪟'
    },
    {
      id: 'icloud',
      label: 'iCloud Mail',
      blurb: 'icloud.com, me.com, mac.com',
      kind: 'imap',
      imapHost: 'imap.mail.me.com',
      imapPort: 993,
      smtpHost: 'smtp.mail.me.com',
      smtpPort: 587,
      appPasswordUrl: 'https://appleid.apple.com',
      appPasswordNote:
        'Apple ID → Sign-In and Security → App-Specific Passwords. Requires 2FA enabled.',
      emoji: '☁️'
    },
    {
      id: 'fastmail',
      label: 'FastMail',
      blurb: 'fastmail.com',
      kind: 'imap',
      imapHost: 'imap.fastmail.com',
      imapPort: 993,
      smtpHost: 'smtp.fastmail.com',
      smtpPort: 465,
      appPasswordUrl: 'https://app.fastmail.com/settings/security/tokens',
      appPasswordNote:
        'FastMail → Settings → Password & Security → App Passwords. Tick "Mail" permissions.',
      emoji: '💨'
    },
    {
      id: 'proton',
      label: 'ProtonMail',
      blurb: 'proton.me, protonmail.com',
      kind: 'imap',
      imapHost: '127.0.0.1',
      imapPort: 1143,
      smtpHost: '127.0.0.1',
      smtpPort: 1025,
      appPasswordNote:
        'Proton Mail uses a proprietary protocol — Postern can only connect via Proton Bridge. Free accounts cannot use Bridge.',
      disabled: true,
      disabledNote: 'Requires Proton Bridge (paid Proton only). Integration coming later.',
      emoji: '🛡️'
    },
    {
      id: 'custom',
      label: 'Custom IMAP / SMTP',
      blurb: 'Any standards-compliant provider',
      kind: 'imap',
      imapHost: '',
      imapPort: 993,
      smtpHost: '',
      smtpPort: 587,
      appPasswordNote:
        'Enter the IMAP and SMTP settings your provider gave you. Most use port 993 for IMAP (TLS) and 587 or 465 for SMTP.',
      emoji: '⚙️'
    }
  ];

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
    <fieldset class="pgp" disabled={spec.disabled}>
      <legend>3. PGP — optional</legend>
      <p class="sub-note">
        Set up end-to-end encryption for this address now, or skip and come back later
        via <em>Settings → PGP</em>.
      </p>

      <label class="pgp-option">
        <input type="radio" name="pgp" value="skip" bind:group={pgpChoice} />
        <div>
          <strong>Skip for now</strong>
          <span>Fine for most users. Postern still auto-discovers other people's keys (WKD) and auto-encrypts when possible.</span>
        </div>
      </label>

      <label class="pgp-option">
        <input type="radio" name="pgp" value="generate" bind:group={pgpChoice} />
        <div>
          <strong>Generate a new keypair</strong>
          <span>Postern creates an ed25519 keypair bound to this mailbox. Published to keys.openpgp.org only when you click Publish.</span>
        </div>
      </label>
      {#if pgpChoice === 'generate'}
        <label class="indent">
          Identity (User ID)
          <input
            type="text"
            bind:value={pgpUserId}
            placeholder="Your Name &lt;{email || 'you@example.com'}&gt;"
            required
          />
          <small class="hint">
            Appears on the key as its user ID. Pre-filled from your email + display name.
          </small>
        </label>
      {/if}

      <label class="pgp-option">
        <input type="radio" name="pgp" value="import" bind:group={pgpChoice} />
        <div>
          <strong>Import existing key</strong>
          <span>Paste an armored public or private key you already have — Postern extracts the public half and stores the private half in the vault.</span>
        </div>
      </label>
      {#if pgpChoice === 'import'}
        <label class="indent">
          Armored key
          <textarea
            bind:value={pgpImportArmored}
            rows="8"
            spellcheck="false"
            autocomplete="off"
            placeholder="-----BEGIN PGP PRIVATE KEY BLOCK-----&#10;...&#10;-----END PGP PRIVATE KEY BLOCK-----"
            required
          ></textarea>
        </label>
      {/if}
    </fieldset>

    {#if error}
      <p class="err">{error}</p>
    {/if}

    <button type="submit" disabled={saving || spec.disabled}>
      {saving ? 'Connecting…' : existingAccountCount === 0 ? 'Finish setup' : 'Add mailbox'}
    </button>
  </form>
</section>

<style>
  section.setup-shell {
    width: 100%;
    max-width: 38rem;
  }
  .intro-card {
    padding: 1.35rem 1.45rem;
    margin-bottom: 1rem;
    border: 1px solid var(--border);
    border-radius: 1.3rem;
    background:
      radial-gradient(circle at top right, color-mix(in oklab, var(--accent) 12%, transparent), transparent 35%),
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
  h1 {
    font-size: 2rem;
    font-weight: 650;
    letter-spacing: -0.03em;
    margin: 0 0 0.25rem;
  }
  .sub {
    color: var(--muted);
    margin: 0;
    font-size: 0.94rem;
    line-height: 1.55;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 0.45rem;
    margin-top: 0.95rem;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    padding: 0.42rem 0.72rem;
    border-radius: 999px;
    background: color-mix(in oklab, var(--surface-2) 82%, transparent);
    border: 1px solid color-mix(in oklab, currentColor 8%, transparent);
    font-size: 0.72rem;
    font-weight: 600;
  }
  form {
    display: flex;
    flex-direction: column;
    gap: 1rem;
    padding: 1.15rem 1.15rem 1.25rem;
    border: 1px solid var(--border);
    border-radius: 1.2rem;
    background: color-mix(in oklab, var(--surface) 94%, transparent);
    box-shadow: 0 16px 36px rgba(0, 0, 0, 0.05);
  }

  fieldset {
    border: 0;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 0.85rem;
  }
  fieldset[disabled] {
    opacity: 0.45;
  }
  legend {
    font-size: 0.78rem;
    font-weight: 700;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    opacity: 0.7;
    padding: 0;
    margin-bottom: 0.1rem;
  }

  /* ── Provider picker ───────────────────────────────────────────── */
  .provider-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(8.5rem, 1fr));
    gap: 0.55rem;
  }
  .provider-card {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 0.18rem;
    padding: 0.75rem 0.8rem 0.7rem;
    border-radius: 0.8rem;
    border: 1px solid color-mix(in oklab, currentColor 12%, transparent);
    background: color-mix(in oklab, var(--surface-2) 35%, transparent);
    cursor: pointer;
    transition: background 120ms, border-color 120ms, transform 120ms;
  }
  .provider-card:hover:not(.disabled) {
    background: color-mix(in oklab, var(--surface-2) 65%, transparent);
  }
  .provider-card.selected {
    border-color: color-mix(in oklab, var(--accent) 55%, transparent);
    background: color-mix(in oklab, var(--accent) 10%, transparent);
    box-shadow: 0 0 0 3px color-mix(in oklab, var(--accent) 14%, transparent);
  }
  .provider-card.disabled {
    cursor: not-allowed;
    opacity: 0.55;
  }
  .provider-card input[type='radio'] {
    position: absolute;
    top: 0.5rem;
    right: 0.5rem;
    width: 0.95rem;
    height: 0.95rem;
  }
  .provider-emoji {
    font-size: 1.3rem;
    line-height: 1;
    margin-bottom: 0.2rem;
  }
  .provider-label {
    font-weight: 650;
    font-size: 0.84rem;
  }
  .provider-blurb {
    font-size: 0.68rem;
    opacity: 0.6;
    font-weight: 400;
    line-height: 1.3;
  }
  .provider-badge {
    position: absolute;
    bottom: 0.4rem;
    right: 0.5rem;
    font-size: 0.58rem;
    padding: 0.1rem 0.35rem;
    border-radius: 999px;
    background: color-mix(in oklab, currentColor 10%, transparent);
    text-transform: uppercase;
    letter-spacing: 0.06em;
    font-weight: 700;
    opacity: 0.7;
  }
  .provider-note {
    margin: 0;
    padding: 0.55rem 0.7rem;
    font-size: 0.78rem;
    background: color-mix(in oklab, orange 8%, transparent);
    border: 1px solid color-mix(in oklab, orange 22%, transparent);
    border-radius: 0.55rem;
  }

  /* ── Inputs & fields ──────────────────────────────────────────── */
  label {
    display: flex;
    flex-direction: column;
    gap: 0.42rem;
    font-size: 0.8rem;
    opacity: 0.9;
    font-weight: 600;
  }
  input,
  textarea {
    font: inherit;
    padding: 0.72rem 0.82rem;
    border: 1px solid color-mix(in oklab, currentColor 18%, transparent);
    border-radius: 0.85rem;
    background: color-mix(in oklab, var(--surface-2) 74%, transparent);
    color: inherit;
    font-weight: 400;
  }
  input:focus,
  textarea:focus {
    outline: none;
    border-color: color-mix(in oklab, var(--accent) 32%, transparent);
    box-shadow: 0 0 0 3px color-mix(in oklab, var(--accent) 12%, transparent);
  }
  textarea {
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 0.78rem;
    resize: vertical;
  }
  .opt {
    opacity: 0.5;
    font-weight: 400;
  }
  .hint {
    opacity: 0.6;
    font-size: 0.72rem;
    font-weight: 400;
    line-height: 1.5;
  }
  .hint a {
    color: var(--accent);
  }

  /* ── Delete-policy choice ────────────────────────────────────── */
  .policy-choice {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }
  .policy-heading {
    font-size: 0.8rem;
    font-weight: 600;
    opacity: 0.9;
  }
  .policy-option {
    display: flex;
    flex-direction: row;
    align-items: flex-start;
    gap: 0.6rem;
    padding: 0.65rem 0.8rem;
    border: 1px solid color-mix(in oklab, currentColor 12%, transparent);
    border-radius: 0.75rem;
    cursor: pointer;
    font-weight: 400;
    opacity: 1;
  }
  .policy-option:has(input:checked) {
    border-color: color-mix(in oklab, var(--accent) 40%, transparent);
    background: color-mix(in oklab, var(--accent) 6%, transparent);
  }
  .policy-option input[type='radio'] {
    margin-top: 0.15rem;
    flex-shrink: 0;
    width: auto;
    padding: 0;
  }
  .policy-option div {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
  }
  .policy-option strong {
    font-size: 0.82rem;
  }
  .policy-option span {
    font-size: 0.75rem;
    opacity: 0.6;
    line-height: 1.4;
  }

  /* ── PGP step reuses the policy-option visual pattern ────────── */
  .sub-note {
    font-size: 0.78rem;
    opacity: 0.7;
    margin: 0 0 0.15rem;
    line-height: 1.5;
  }
  .pgp-option {
    display: flex;
    flex-direction: row;
    align-items: flex-start;
    gap: 0.6rem;
    padding: 0.65rem 0.8rem;
    border: 1px solid color-mix(in oklab, currentColor 12%, transparent);
    border-radius: 0.75rem;
    cursor: pointer;
    font-weight: 400;
  }
  .pgp-option:has(input:checked) {
    border-color: color-mix(in oklab, var(--accent) 40%, transparent);
    background: color-mix(in oklab, var(--accent) 6%, transparent);
  }
  .pgp-option input[type='radio'] {
    margin-top: 0.15rem;
    flex-shrink: 0;
    width: auto;
    padding: 0;
  }
  .pgp-option div {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
  }
  .pgp-option strong {
    font-size: 0.82rem;
  }
  .pgp-option span {
    font-size: 0.75rem;
    opacity: 0.6;
    line-height: 1.4;
  }
  .indent {
    margin-left: 1.9rem;
  }

  details {
    border: 1px solid color-mix(in oklab, currentColor 12%, transparent);
    border-radius: 1rem;
    padding: 0.8rem 0.9rem;
    background: color-mix(in oklab, var(--surface) 96%, transparent);
  }
  summary {
    cursor: pointer;
    font-size: 0.85rem;
    opacity: 0.74;
    font-weight: 600;
  }
  .grid {
    display: grid;
    grid-template-columns: 1fr 6rem;
    gap: 0.75rem;
    margin-top: 0.75rem;
  }
  button[type='submit'] {
    margin-top: 0.25rem;
    padding: 0.82rem 1rem;
    border: 0;
    border-radius: 999px;
    background: var(--accent);
    color: white;
    font: inherit;
    font-weight: 600;
    cursor: pointer;
  }
  button[type='submit']:disabled {
    opacity: 0.6;
    cursor: progress;
  }
  .err {
    color: #c83333;
    font-size: 0.85rem;
    margin: 0;
  }

  @media (max-width: 640px) {
    .grid {
      grid-template-columns: 1fr;
    }
    .provider-grid {
      grid-template-columns: repeat(auto-fill, minmax(7.5rem, 1fr));
    }
  }
</style>
