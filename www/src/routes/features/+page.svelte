<script lang="ts">
  import Seo from '$lib/components/Seo.svelte';
  import Icon from '$lib/components/Icon.svelte';
  import Frame from '$lib/components/Frame.svelte';
  import ZoomImage from '$lib/components/ZoomImage.svelte';
  import { reveal } from '$lib/reveal';
  import { URLS } from '$lib/site';

  interface Row {
    icon: string;
    kicker: string;
    title: string;
    body: string;
    points: string[];
    img: string;
    alt: string;
    label: string;
  }

  const rows: Row[] = [
    {
      icon: 'lock',
      kicker: 'Encrypted vault',
      title: 'The only copy lives on your hardware.',
      body: 'Postern can pull every message off your provider and store it in a SQLCipher-encrypted database on your own machine. After that, the provider is just a relay — your archive never has to sit on someone else’s disk again.',
      points: [
        'AES-256 at rest via SQLCipher',
        'Unlocked by a master password, never stored',
        'Optionally clears mail from the provider after sync'
      ],
      img: '/img/app/settings/backups_mailbox.png',
      alt: 'Postern mailbox backup settings',
      label: 'postern · backups'
    },
    {
      icon: 'shield',
      kicker: 'PGP + Autocrypt',
      title: 'End-to-end encryption that sets itself up.',
      body: 'Your private keys are generated and kept locally. Autocrypt exchanges public keys in the background, so anything you send to another Autocrypt-aware client — Postern, Thunderbird, ProtonMail — is sealed end-to-end without manual key juggling.',
      points: [
        'Private keys never touch a provider',
        'Automatic key exchange between clients',
        'Provider sees only ciphertext and routing headers'
      ],
      img: '/img/app/settings/security_settings.png',
      alt: 'Postern security settings with two-factor and trusted devices',
      label: 'postern · security'
    },
    {
      icon: 'server',
      kicker: 'Bring any provider',
      title: 'Keep the mailbox you already have.',
      body: 'Gmail, Fastmail, iCloud, your own Postfix — if it speaks IMAP and SMTP, Postern works with it. Connect several accounts and read them in one unified inbox, or keep them cleanly separated.',
      points: ['Standard IMAP & SMTP', 'Multiple accounts, one unified inbox', 'No vendor lock-in, ever'],
      img: '/img/app/settings/settings_mailbox.png',
      alt: 'Postern mailbox account settings',
      label: 'postern · accounts'
    },
    {
      icon: 'spark',
      kicker: 'Local-first AI',
      title: 'AI you can turn on — kept deliberately simple.',
      body: 'AI is opt-in and off by default, and we keep it minimal on purpose. For privacy, the everyday tools are just voice dictation and a “polish this” rewrite with grammar and spell-check — running locally against your own Ollama box. Point it at a cloud model with your own key only if you want to. Nothing switches on until you say so.',
      points: ['Voice dictation & one-tap polish', 'Local grammar / spell-check, no cloud needed', 'Self-hosted Ollama by default; cloud optional, never required'],
      img: '/img/app/settings/ai_settings.png',
      alt: 'Postern AI settings panel',
      label: 'postern · ai'
    },
    {
      icon: 'eyeoff',
      kicker: 'Privacy controls',
      title: 'Nothing loads behind your back.',
      body: 'Remote images, tracking pixels, and third-party fetches are gated. Postern proxies and classifies remote content so a marketing email can’t quietly report that you opened it.',
      points: ['Tracking-pixel detection & blocking', 'Remote content proxied, not direct', 'Avatar and link privacy options'],
      img: '/img/app/settings/privacy_settings.png',
      alt: 'Postern privacy settings',
      label: 'postern · privacy'
    },
    {
      icon: 'cal',
      kicker: 'Calendars & contacts',
      title: 'More than an inbox.',
      body: 'Keep calendars and contacts alongside your mail, in the same private vault. Autocomplete pulls from people you actually correspond with — no cloud address book required.',
      points: ['Local calendars', 'Contact autocomplete from your mail', 'Stored in the same encrypted vault'],
      img: '/img/app/settings/calendars_settings.png',
      alt: 'Postern calendar settings',
      label: 'postern · calendars'
    },
    {
      icon: 'import',
      kicker: 'Import & migration',
      title: 'Bring your whole history with you.',
      body: 'Migrating from Mailpile or starting fresh from Gmail? Postern bulk-imports your existing mail straight into the vault so you don’t leave years of archive behind.',
      points: ['Mailpile migration', 'Bulk Gmail import', 'History lands directly in the vault'],
      img: '/img/app/settings/import_settings.png',
      alt: 'Postern import settings',
      label: 'postern · import'
    }
  ];
</script>

<Seo
  title="Features"
  description="PGP and Autocrypt, an encrypted SQLCipher vault, local-first AI, privacy controls, calendars, and migration tools — everything inside Postern, the self-hosted email client."
  path="/features"
/>

<section class="head shell">
  <span class="eyebrow" use:reveal>Features</span>
  <h1 use:reveal={60}>Everything in <span class="accent-teal">Postern</span>.</h1>
  <p class="lead" use:reveal={120}>
    A complete email client built around one idea: you should hold the keys and the data. Here’s
    what that looks like in practice.
  </p>
</section>

<!-- mobile band -->
<section class="section shell mobile-band">
  <header class="sec-head" use:reveal>
    <span class="eyebrow"><Icon name="phone" size={14} /> On every device</span>
    <h2>A web UI built for the phone, not just shrunk to fit.</h2>
    <p class="lead">No app to install — open Postern in your phone’s browser over your own private mesh. The mobile layout is responsive and touch-first, with the same encrypted vault behind it.</p>
  </header>
  <div class="phones">
    {#each [['/img/app/mobile/mobile_login.png', 'Postern mobile unlock screen'], ['/img/app/mobile/mobile_inbox.png', 'Postern mobile inbox'], ['/img/app/mobile/mobile_mail_send_features.png', 'Postern mobile compose and send features']] as [src, alt], i}
      <div class="phone" use:reveal={i * 100}>
        <ZoomImage {src} {alt} radius="var(--r-lg)" />
      </div>
    {/each}
  </div>
</section>

<!-- alternating rows -->
{#each rows as r, i}
  <section class="section shell">
    <div class="row" class:flip={i % 2 === 1}>
      <div class="row__copy" use:reveal>
        <span class="row__kicker"><Icon name={r.icon} size={16} /> {r.kicker}</span>
        <h2>{r.title}</h2>
        <p>{r.body}</p>
        <ul class="ticks">
          {#each r.points as pt}
            <li><Icon name="check" size={16} /> {pt}</li>
          {/each}
        </ul>
      </div>
      <div class="row__shot" use:reveal={80}>
        <Frame src={r.img} alt={r.alt} label={r.label} />
      </div>
    </div>
  </section>
{/each}

<section class="section shell">
  <div class="closer card" use:reveal>
    <h2>Ready when you are.</h2>
    <p class="lead">Install Postern on a spare machine and connect your first mailbox today.</p>
    <div class="closer__btns">
      <a class="btn btn--primary btn--lg" href="/download">Get started <Icon name="arrow" size={18} /></a>
      <a class="btn btn--ghost btn--lg" href={URLS.docs} target="_blank" rel="noopener">Read the docs</a>
    </div>
  </div>
</section>

<style>
  .head {
    padding-top: clamp(2.5rem, 1.5rem + 4vw, 5rem);
    max-width: 60ch;
  }
  .head h1 {
    font-size: var(--text-3xl);
    margin: 1rem 0 1.2rem;
  }

  .sec-head {
    max-width: 60ch;
    margin-bottom: 2.4rem;
  }
  .sec-head h2 {
    font-size: var(--text-2xl);
    margin: 0.8rem 0;
  }

  /* mobile band */
  .mobile-band {
    border-top: 1px solid var(--border);
  }
  .phones {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 1.4rem;
    max-width: 880px;
  }
  .phone {
    border: 1px solid var(--border-strong);
    border-radius: var(--r-lg);
    overflow: hidden;
    background: var(--surface-solid);
    box-shadow: var(--shadow-card);
  }

  /* rows */
  .row {
    display: grid;
    grid-template-columns: 1fr 1.1fr;
    align-items: center;
    gap: clamp(2rem, 1rem + 4vw, 4.5rem);
  }
  .row.flip .row__copy {
    order: 2;
  }
  .row__kicker {
    display: inline-flex;
    align-items: center;
    gap: 0.45rem;
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    letter-spacing: 0.12em;
    text-transform: uppercase;
    color: var(--accent);
  }
  .row__copy h2 {
    font-size: var(--text-2xl);
    margin: 1rem 0;
  }
  .row__copy > p {
    color: var(--text-muted);
    max-width: 48ch;
  }
  .ticks {
    display: grid;
    gap: 0.7rem;
    margin-top: 1.6rem;
  }
  .ticks li {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    font-size: var(--text-sm);
    color: var(--text);
  }
  .ticks :global(svg) {
    color: var(--accent);
  }

  /* closer */
  .closer {
    text-align: center;
    padding: clamp(2.5rem, 2rem + 3vw, 4.5rem) 2rem;
    background: radial-gradient(80% 130% at 50% 0%, var(--glow-orange), transparent 60%), var(--surface);
  }
  .closer h2 {
    font-size: var(--text-2xl);
    margin-bottom: 0.8rem;
  }
  .closer .lead {
    margin-inline: auto;
  }
  .closer__btns {
    display: flex;
    justify-content: center;
    flex-wrap: wrap;
    gap: 0.8rem;
    margin-top: 1.6rem;
  }

  @media (max-width: 860px) {
    .row {
      grid-template-columns: 1fr;
    }
    .row.flip .row__copy {
      order: 0;
    }
  }
  @media (max-width: 560px) {
    .phones {
      grid-template-columns: 1fr;
      max-width: 280px;
      margin-inline: auto;
    }
  }
</style>
