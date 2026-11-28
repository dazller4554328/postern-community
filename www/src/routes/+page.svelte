<script lang="ts">
  import Seo from '$lib/components/Seo.svelte';
  import Icon from '$lib/components/Icon.svelte';
  import Frame from '$lib/components/Frame.svelte';
  import ZoomImage from '$lib/components/ZoomImage.svelte';
  import FlowDiagram from '$lib/components/FlowDiagram.svelte';
  import Logo from '$lib/components/Logo.svelte';
  import { reveal } from '$lib/reveal';
  import { PILLARS, BENTO, TRUST_CHIPS, URLS } from '$lib/site';

  // Theme showcase toggle (independent of the site theme).
  let shot: 'light' | 'cyber' = $state('cyber');
</script>

<Seo
  title="Postern — self-hosted email with your keys on your server"
  description="A privacy-first, self-hosted email client. PGP keys live on your machine, Autocrypt handles the handshake, and your mail lives in an encrypted vault you own. Works with any IMAP provider."
  path="/"
/>

<!-- ============ HERO ============ -->
<section class="hero shell">
  <div class="hero__copy">
    <span class="eyebrow" use:reveal>Postern · self-hosted · open source</span>
    <h1 use:reveal={60}>
      Postern keeps email on <span class="accent-teal">your server.</span>
    </h1>
    <p class="lead" use:reveal={140}>
      Postern is a privacy-first email client you run yourself. Your PGP keys never leave your
      machine, <a href={URLS.autocrypt} target="_blank" rel="noopener">Autocrypt</a> handles the
      handshake, and every message lives in an encrypted vault you own — on top of whichever IMAP
      provider you already use.
    </p>
    <div class="hero__cta" use:reveal={220}>
      <a class="btn btn--primary btn--lg" href="/download">
        Install Community <Icon name="arrow" size={18} />
      </a>
      <a class="btn btn--ghost btn--lg" href="/pricing">Compare editions</a>
      <a class="btn btn--ghost btn--lg" href={URLS.github} target="_blank" rel="noopener">
        <Icon name="github" size={18} /> Source
      </a>
    </div>
    <ul class="chips" use:reveal={300}>
      {#each TRUST_CHIPS as chip}
        <li>{chip}</li>
      {/each}
    </ul>
  </div>

  <div class="hero__shot" use:reveal={180}>
    <Frame
      src="/img/app/main/main_inbox.png"
      alt="Postern unified inbox in the dark theme"
      label="postern · unified inbox"
      loading="eager"
      fetchpriority="high"
    />
    <div class="hero__phone" use:reveal={340}>
      <picture>
        <source
          type="image/avif"
          srcset="/img/app/mobile/mobile_inbox-720.avif 720w, /img/app/mobile/mobile_inbox.avif 864w"
          sizes="(max-width: 960px) 26vw, 17vw"
        />
        <source
          type="image/webp"
          srcset="/img/app/mobile/mobile_inbox-720.webp 720w, /img/app/mobile/mobile_inbox.webp 864w"
          sizes="(max-width: 960px) 26vw, 17vw"
        />
        <img
          src="/img/app/mobile/mobile_inbox.png"
          alt="The same unified inbox on a phone"
          loading="eager"
          decoding="async"
        />
      </picture>
    </div>
  </div>
</section>

<!-- ============ PILLARS ============ -->
<section class="section shell">
  <div class="pillars">
    {#each PILLARS as p, i}
      <article class="card pillar" use:reveal={i * 90}>
        <span class="pillar__icon"><Icon name={p.icon} size={26} /></span>
        <h3>{p.title}</h3>
        <p>{p.blurb}</p>
      </article>
    {/each}
  </div>
</section>

<!-- ============ HOW IT WORKS ============ -->
<section class="section shell">
  <header class="sec-head" use:reveal>
    <span class="eyebrow">How it works</span>
    <h2>Your provider becomes a relay.</h2>
    <p class="lead">
      Postern keeps using your existing mailbox to send and receive — then quietly moves the only
      lasting copy of your mail into a vault on your own hardware.
    </p>
  </header>

  <ol class="flow">
    <li class="card" use:reveal={0}>
      <span class="flow__n">01</span>
      <h3>Connect any mailbox</h3>
      <p>Point Postern at Gmail, Fastmail, iCloud, or your own server over standard IMAP & SMTP.</p>
    </li>
    <li class="flow__arrow" aria-hidden="true"><Icon name="arrow" size={22} /></li>
    <li class="card" use:reveal={120}>
      <span class="flow__n">02</span>
      <h3>Pull into the vault</h3>
      <p>Every message is copied into a SQLCipher-encrypted store on your machine — then cleared from the provider if you want.</p>
    </li>
    <li class="flow__arrow" aria-hidden="true"><Icon name="arrow" size={22} /></li>
    <li class="card" use:reveal={240}>
      <span class="flow__n">03</span>
      <h3>Read & write privately</h3>
      <p>PGP encrypts messages to compatible keyholders. Providers still route headers, but encrypted bodies stay sealed.</p>
    </li>
  </ol>
</section>

<!-- ============ TOPOLOGY DIAGRAM ============ -->
<section class="section shell">
  <header class="sec-head" use:reveal>
    <span class="eyebrow">See the difference</span>
    <h2>Where your mail actually flows.</h2>
    <p class="lead">
      Same mailbox, two topologies. Community runs on your own machine; Pro puts Postern on a VPS you
      reach over a private mesh, with VPN egress. Flip between them:
    </p>
  </header>
  <div use:reveal>
    <FlowDiagram />
  </div>
</section>

<!-- ============ BENTO ============ -->
<section class="section shell">
  <header class="sec-head" use:reveal>
    <span class="eyebrow">What you get</span>
    <h2>A full email client — not a privacy demo.</h2>
    <p class="lead">Everything you'd expect from a daily driver, with the privacy baked in rather than bolted on.</p>
  </header>

  <div class="bento">
    {#each BENTO as b, i}
      <article class="card bento__cell {b.span ?? 'normal'}" class:has-img={!!b.img} use:reveal={(i % 3) * 80}>
        <div class="bento__text">
          <h3>{b.title}</h3>
          <p>{b.blurb}</p>
        </div>
        {#if b.img}
          <div class="bento__img">
            <ZoomImage src={b.img} alt={b.alt ?? b.title} />
          </div>
        {/if}
      </article>
    {/each}
  </div>

  <p class="more" use:reveal>
    <a class="textlink" href="/features">See every feature <Icon name="arrow" size={16} /></a>
  </p>
</section>

<!-- ============ THEME SHOWCASE ============ -->
<section class="section shell">
  <header class="sec-head" use:reveal>
    <span class="eyebrow">Make it yours</span>
    <h2>Themes with a point of view.</h2>
    <p class="lead">From a calm light workspace to a full cyber theme. Switch the preview:</p>
    <div class="seg" role="tablist" aria-label="Theme preview">
      <button role="tab" aria-selected={shot === 'cyber'} class:on={shot === 'cyber'} onclick={() => (shot = 'cyber')}>Cyber</button>
      <button role="tab" aria-selected={shot === 'light'} class:on={shot === 'light'} onclick={() => (shot = 'light')}>Light</button>
    </div>
  </header>

  <div class="showcase" use:reveal>
    {#if shot === 'cyber'}
      <Frame src="/img/app/main/main_cyber_theme.png" alt="Postern inbox in the cyber theme" label="postern · cyber theme" />
    {:else}
      <Frame src="/img/app/main/main_light_theme.png" alt="Postern inbox in the light theme" label="postern · light theme" />
    {/if}
  </div>
</section>

<!-- ============ SECURITY STRIP ============ -->
<section class="section shell">
  <div class="card security" use:reveal>
    <div class="security__head">
      <span class="eyebrow">Built on real crypto</span>
      <h2>No "trust us." Just primitives you can audit.</h2>
      <p class="lead">The Community client is open source under Apache 2.0. Postern keeps durable mail storage under your control and makes network access explicit.</p>
    </div>
    <ul class="security__list">
      <li><Icon name="lock" size={20} /><div><strong>SQLCipher vault</strong><span>AES-256 at rest, unlocked by your master password.</span></div></li>
      <li><Icon name="key" size={20} /><div><strong>Argon2id + ChaCha20</strong><span>Memory-hard key derivation, modern AEAD for secrets.</span></div></li>
      <li><Icon name="shield" size={20} /><div><strong>OpenPGP + Autocrypt</strong><span>End-to-end mail with automatic key exchange.</span></div></li>
      <li><Icon name="eyeoff" size={20} /><div><strong>No telemetry</strong><span>No trackers, no pixel fetches, no phone-home.</span></div></li>
    </ul>
  </div>
</section>

<!-- ============ THREAT MODEL ============ -->
<section class="section shell">
  <div class="threat card" use:reveal>
    <header>
      <span class="eyebrow">Threat model</span>
      <h2>Clear boundaries, not magic promises.</h2>
      <p class="lead">
        Postern is built to reduce durable provider storage, device exposure, tracker leakage, and
        key custody risk. It cannot hide email metadata from the providers that route your messages,
        and end-to-end privacy requires recipients with compatible encryption keys.
      </p>
    </header>
    <div class="threat__grid">
      <div>
        <h3>Designed to protect</h3>
        <ul>
          <li><Icon name="check" size={16} /> Mail archives at rest in your vault</li>
          <li><Icon name="check" size={16} /> Private keys and account secrets</li>
          <li><Icon name="check" size={16} /> Remote image and tracking-pixel fetches</li>
        </ul>
      </div>
      <div>
        <h3>Still visible to mail infrastructure</h3>
        <ul>
          <li><Icon name="cross" size={16} /> Sender, recipient, and routing headers</li>
          <li><Icon name="cross" size={16} /> Unencrypted mail sent to non-keyholders</li>
          <li><Icon name="cross" size={16} /> Provider account access logs and policy controls</li>
        </ul>
      </div>
    </div>
  </div>
</section>

<!-- ============ WHO IT'S FOR ============ -->
<section class="section shell">
  <div class="audience">
    <header class="audience__head" use:reveal>
      <span class="eyebrow">Who it's for</span>
      <h2>For people who can't afford a leak.</h2>
      <p class="lead">
        Keep the archive off the laptop or phone you travel with. Your encrypted vault can stay on a
        server you control, reached over a private mesh and unlocked only when you choose. Cross a
        border, lose a device, or rotate hardware without carrying years of mail locally.
      </p>
      <ul class="audience__points">
        <li><Icon name="check" size={16} /> No mail archive sitting on the laptop you travel with</li>
        <li><Icon name="check" size={16} /> Access over Tailscale — no public address to find or block</li>
        <li><Icon name="check" size={16} /> Lock access remotely; the vault still needs your master password</li>
      </ul>
    </header>
    <ul class="audience__grid">
      <li class="card" use:reveal={0}>
        <span class="audience__icon"><Icon name="pen" size={22} /></span>
        <h3>Journalists &amp; sources</h3>
        <p>Keep correspondence — and the people in it — off devices that get searched at airports and borders.</p>
      </li>
      <li class="card" use:reveal={80}>
        <span class="audience__icon"><Icon name="scale" size={22} /></span>
        <h3>Lawyers &amp; clients</h3>
        <p>Privileged archives stay in your own encrypted vault instead of living permanently on every device.</p>
      </li>
      <li class="card" use:reveal={160}>
        <span class="audience__icon"><Icon name="shield" size={22} /></span>
        <h3>Activists &amp; researchers</h3>
        <p>End-to-end PGP where recipients support it, plus a vault you host yourself for durable storage.</p>
      </li>
      <li class="card" use:reveal={240}>
        <span class="audience__icon"><Icon name="plane" size={22} /></span>
        <h3>Frequent travelers</h3>
        <p>Carry a clean device. Your inbox is waiting at home, reachable over your private mesh when you land.</p>
      </li>
    </ul>
  </div>
</section>

<!-- ============ PRICING TEASER ============ -->
<section class="section shell">
  <div class="teaser">
    <div class="teaser__free card" use:reveal>
      <h3>Community</h3>
      <p class="teaser__price">Free <span>· open source</span></p>
      <p>Up to 3 mailboxes, the encrypted vault, PGP, and theming. Self-host it and own your mail forever.</p>
      <a class="btn btn--ghost" href="/pricing">Compare editions</a>
    </div>
    <div class="teaser__pro card" use:reveal={120}>
      <span class="teaser__tag">Pro</span>
      <h3>Everything, unlocked</h3>
      <p>Unlimited mailboxes, Tailscale mesh, local-first AI, calendars, per-device sessions, and one-click updates.</p>
      <div class="teaser__actions">
        <a class="btn btn--primary" href="/pricing">See pricing <Icon name="arrow" size={16} /></a>
        <a class="btn btn--ghost" href={URLS.storePro} target="_blank" rel="noopener">Buy or manage license</a>
      </div>
    </div>
  </div>
</section>

<!-- ============ FINAL CTA ============ -->
<section class="section shell">
  <div class="cta-final card" use:reveal>
    <Logo size={44} wordmark={false} />
    <h2>Take your mailbox back.</h2>
    <p class="lead">Run Postern on a spare box in an afternoon. Your keys, your data, your rules.</p>
    <div class="cta-final__btns">
      <a class="btn btn--primary btn--lg" href="/download">Get started <Icon name="arrow" size={18} /></a>
      <a class="btn btn--ghost btn--lg" href={URLS.docs} target="_blank" rel="noopener">Read the docs</a>
    </div>
  </div>
</section>

<style>
  /* ---- hero ---- */
  .hero {
    display: grid;
    grid-template-columns: 1.05fr 1.25fr;
    align-items: center;
    gap: clamp(2rem, 1rem + 4vw, 4.5rem);
    padding-top: clamp(3rem, 1.5rem + 5vw, 6rem);
    padding-bottom: clamp(3rem, 2rem + 4vw, 5rem);
  }
  .hero h1 {
    font-size: var(--text-hero);
    margin: 1.1rem 0 1.4rem;
    font-weight: 700;
  }
  .hero .lead a {
    color: var(--accent);
    text-decoration: underline;
    text-underline-offset: 3px;
  }
  .hero__cta {
    display: flex;
    flex-wrap: wrap;
    gap: 0.8rem;
    margin-top: 2rem;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
    margin-top: 2rem;
  }
  .chips li {
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    letter-spacing: 0.03em;
    color: var(--text-muted);
    padding: 0.32rem 0.7rem;
    border: 1px solid var(--border);
    border-radius: var(--r-pill);
    background: var(--surface);
  }
  .hero__shot {
    position: relative;
  }
  /* mobile screenshot tucked into the corner — a phone next to the desktop */
  .hero__phone {
    position: absolute;
    right: -2%;
    bottom: -9%;
    width: clamp(116px, 19%, 188px);
    z-index: 2;
    padding: 5px;
    border-radius: 1.5rem;
    background: linear-gradient(155deg, #20283a, #0a0f1a 70%);
    border: 1px solid var(--border-strong);
    box-shadow: 0 28px 55px -16px rgba(0, 0, 0, 0.7);
    pointer-events: none;
  }
  .hero__phone picture {
    display: block;
  }
  .hero__phone img {
    display: block;
    width: 100%;
    border-radius: 1.15rem;
  }
  .hero__shot::after {
    content: '';
    position: absolute;
    inset: auto -6% -14% -6%;
    height: 60%;
    background: radial-gradient(ellipse at 50% 100%, var(--glow-teal), transparent 70%);
    filter: blur(30px);
    z-index: -1;
  }

  /* ---- section heads ---- */
  .sec-head {
    max-width: 60ch;
    margin-bottom: clamp(2rem, 1.5rem + 2vw, 3rem);
  }
  .sec-head h2 {
    font-size: var(--text-2xl);
    margin: 0.9rem 0 0.9rem;
  }

  /* ---- pillars ---- */
  .pillars {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 1.2rem;
  }
  .pillar {
    padding: 1.8rem;
  }
  .pillar__icon {
    display: inline-grid;
    place-items: center;
    width: 52px;
    height: 52px;
    border-radius: var(--r-md);
    background: color-mix(in oklab, var(--accent) 16%, transparent);
    color: var(--accent);
    margin-bottom: 1.2rem;
  }
  .pillar h3 {
    font-size: var(--text-xl);
    margin-bottom: 0.6rem;
  }
  .pillar p {
    color: var(--text-muted);
    font-size: var(--text-sm);
  }

  /* ---- flow ---- */
  .flow {
    display: grid;
    grid-template-columns: 1fr auto 1fr auto 1fr;
    align-items: stretch;
    gap: 1rem;
  }
  .flow .card {
    padding: 1.6rem;
  }
  .flow__n {
    font-family: var(--font-mono);
    font-size: var(--text-sm);
    color: var(--accent);
  }
  .flow h3 {
    font-size: var(--text-lg);
    margin: 0.7rem 0 0.5rem;
  }
  .flow p {
    color: var(--text-muted);
    font-size: var(--text-sm);
  }
  .flow__arrow {
    display: grid;
    place-items: center;
    color: var(--text-faint);
  }

  /* ---- bento ---- */
  .bento {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    grid-auto-rows: minmax(200px, auto);
    gap: 1.2rem;
  }
  .bento__cell {
    display: flex;
    flex-direction: column;
    padding: 1.6rem;
    overflow: hidden;
  }
  .bento__cell.wide {
    grid-column: span 2;
  }
  .bento__cell.tall {
    grid-row: span 2;
  }
  .bento__text h3 {
    font-size: var(--text-lg);
    margin-bottom: 0.5rem;
  }
  .bento__text p {
    color: var(--text-muted);
    font-size: var(--text-sm);
    max-width: 46ch;
  }
  .bento__img {
    margin-top: auto;
    padding-top: 1.3rem;
    flex: 1;
    display: flex;
    align-items: flex-end;
    min-height: 0;
  }
  .bento__img :global(.zoomimg) {
    width: 100%;
  }
  .bento__cell.tall .bento__img {
    align-items: stretch;
  }
  .more {
    margin-top: 1.8rem;
  }
  .textlink {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    color: var(--accent);
    font-family: var(--font-display);
    font-weight: 600;
    font-size: var(--text-sm);
  }
  .textlink:hover {
    gap: 0.65rem;
  }

  /* ---- theme showcase ---- */
  .seg {
    display: inline-flex;
    gap: 0.25rem;
    margin-top: 1.4rem;
    padding: 0.25rem;
    border: 1px solid var(--border);
    border-radius: var(--r-pill);
    background: var(--surface);
  }
  .seg button {
    border: 0;
    background: transparent;
    color: var(--text-muted);
    font-family: var(--font-display);
    font-weight: 600;
    font-size: var(--text-sm);
    padding: 0.45rem 1.1rem;
    border-radius: var(--r-pill);
    cursor: pointer;
    transition:
      color var(--dur-fast) var(--ease),
      background-color var(--dur-fast) var(--ease);
  }
  .seg button.on {
    color: var(--accent-ink);
    background: var(--accent);
  }
  .showcase {
    max-width: 980px;
    margin-inline: auto;
  }

  /* ---- security ---- */
  .security {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: clamp(1.5rem, 1rem + 3vw, 3.5rem);
    padding: clamp(1.8rem, 1.4rem + 2vw, 3rem);
  }
  .security__head h2 {
    font-size: var(--text-2xl);
    margin: 0.8rem 0;
  }
  .security__list {
    display: grid;
    gap: 1.2rem;
    align-content: center;
  }
  .security__list li {
    display: flex;
    gap: 0.9rem;
    align-items: flex-start;
    color: var(--accent);
  }
  .security__list div {
    display: flex;
    flex-direction: column;
  }
  .security__list strong {
    color: var(--text);
    font-family: var(--font-display);
    font-size: var(--text-base);
  }
  .security__list span {
    color: var(--text-muted);
    font-size: var(--text-sm);
  }

  /* ---- threat model ---- */
  .threat {
    padding: clamp(1.8rem, 1.4rem + 2vw, 3rem);
  }
  .threat header {
    max-width: 68ch;
  }
  .threat h2 {
    font-size: var(--text-2xl);
    margin: 0.8rem 0;
  }
  .threat__grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 1.2rem;
    margin-top: 2rem;
  }
  .threat__grid > div {
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    padding: 1.3rem;
    background: color-mix(in oklab, var(--surface) 78%, transparent);
  }
  .threat h3 {
    font-size: var(--text-base);
    margin-bottom: 0.9rem;
  }
  .threat ul {
    display: grid;
    gap: 0.7rem;
  }
  .threat li {
    display: flex;
    align-items: flex-start;
    gap: 0.55rem;
    color: var(--text-muted);
    font-size: var(--text-sm);
  }
  .threat :global(svg) {
    color: var(--accent);
    margin-top: 3px;
  }

  /* ---- audience / who it's for ---- */
  .audience {
    display: grid;
    grid-template-columns: 0.95fr 1.05fr;
    gap: clamp(2rem, 1rem + 4vw, 4rem);
    align-items: center;
  }
  .audience__head h2 {
    font-size: var(--text-2xl);
    margin: 0.8rem 0 1rem;
  }
  .audience__points {
    display: grid;
    gap: 0.65rem;
    margin-top: 1.6rem;
  }
  .audience__points li {
    display: flex;
    align-items: flex-start;
    gap: 0.6rem;
    font-size: var(--text-sm);
    color: var(--text);
  }
  .audience__points :global(svg) {
    color: var(--accent);
    margin-top: 3px;
    flex: none;
  }
  .audience__grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 1.1rem;
  }
  .audience__grid .card {
    padding: 1.5rem;
  }
  .audience__icon {
    display: inline-grid;
    place-items: center;
    width: 46px;
    height: 46px;
    border-radius: var(--r-md);
    background: color-mix(in oklab, var(--brand-orange) 16%, transparent);
    color: var(--brand-orange);
    margin-bottom: 1rem;
  }
  .audience__grid h3 {
    font-size: var(--text-base);
    font-family: var(--font-display);
    margin-bottom: 0.4rem;
  }
  .audience__grid p {
    color: var(--text-muted);
    font-size: var(--text-sm);
  }

  /* ---- teaser ---- */
  .teaser {
    display: grid;
    grid-template-columns: 0.85fr 1.15fr;
    gap: 1.2rem;
  }
  .teaser .card {
    padding: 2rem;
  }
  .teaser h3 {
    font-size: var(--text-xl);
  }
  .teaser p {
    color: var(--text-muted);
    margin: 0.8rem 0 1.4rem;
  }
  .teaser__price {
    font-family: var(--font-display);
    font-size: var(--text-2xl);
    color: var(--text) !important;
    margin: 0.3rem 0 1rem !important;
  }
  .teaser__price span {
    font-size: var(--text-sm);
    color: var(--text-muted);
    font-family: var(--font-body);
  }
  .teaser__pro {
    position: relative;
    background:
      radial-gradient(120% 140% at 100% 0%, var(--glow-teal), transparent 55%), var(--surface);
    border-color: var(--border-strong);
  }
  .teaser__tag {
    display: inline-block;
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    letter-spacing: 0.12em;
    text-transform: uppercase;
    color: var(--accent);
    margin-bottom: 0.6rem;
  }
  .teaser__actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.7rem;
  }

  /* ---- final cta ---- */
  .cta-final {
    text-align: center;
    padding: clamp(2.5rem, 2rem + 4vw, 5rem) 2rem;
    background:
      radial-gradient(80% 130% at 50% 0%, var(--glow-teal), transparent 60%), var(--surface);
  }
  .cta-final h2 {
    font-size: var(--text-2xl);
    margin: 1.2rem 0 0.8rem;
  }
  .cta-final .lead {
    margin-inline: auto;
  }
  .cta-final__btns {
    display: flex;
    justify-content: center;
    flex-wrap: wrap;
    gap: 0.8rem;
    margin-top: 1.8rem;
  }

  /* ---- responsive ---- */
  @media (max-width: 960px) {
    .hero {
      grid-template-columns: 1fr;
    }
    .pillars,
    .bento {
      grid-template-columns: 1fr 1fr;
    }
    .bento__cell.wide,
    .bento__cell.tall {
      grid-column: span 2;
      grid-row: auto;
    }
    .security,
    .teaser,
    .audience,
    .threat__grid {
      grid-template-columns: 1fr;
    }
    .flow {
      grid-template-columns: 1fr;
    }
    .flow__arrow {
      transform: rotate(90deg);
    }
  }
  @media (max-width: 560px) {
    .pillars,
    .bento,
    .audience__grid {
      grid-template-columns: 1fr;
    }
    .bento__cell.wide,
    .bento__cell.tall {
      grid-column: auto;
    }
    /* keep the corner phone on small screens — just size it down a touch
       and keep it inside the frame so it can't cause horizontal scroll */
    .hero__phone {
      width: 33%;
      right: 2%;
      bottom: -6%;
      padding: 4px;
      border-radius: 1.15rem;
    }
    .hero__phone img {
      border-radius: 0.85rem;
    }
  }
</style>
