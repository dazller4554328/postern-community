<script lang="ts">
  import Seo from '$lib/components/Seo.svelte';
  import Icon from '$lib/components/Icon.svelte';
  import CodeBlock from '$lib/components/CodeBlock.svelte';
  import { reveal } from '$lib/reveal';
  import { URLS } from '$lib/site';

  const PRO_INSTALLER_URL = 'https://updates.postern.email/install.sh';
  const COMMUNITY_INSTALLER_URL = 'https://raw.githubusercontent.com/dazller4554328/postern-community/main/install.sh';
  const PRO_CMD = `curl -fsSL ${PRO_INSTALLER_URL} \
  | sudo LICENSE=PSTN-XXXX-XXXX-XXXX-XXXX bash`;
  const COMMUNITY_CMD = `curl -fsSL ${COMMUNITY_INSTALLER_URL} | bash`;
  const TAILSCALE_CMD = 'curl -fsSL https://tailscale.com/install.sh | sh\nsudo tailscale up';

  const reqs = [
    ['OS', 'Ubuntu 22.04 / 24.04 LTS or Debian 12'],
    ['CPU', '2 vCPU minimum (4 recommended)'],
    ['RAM', '2 GB + 4 GB swap (4 GB recommended)'],
    ['Disk', '20 GB minimum (40 GB+ recommended)'],
    ['Network', 'Outbound only — no inbound ports']
  ];

  const steps = [
    {
      n: '01',
      title: 'Pick a host',
      body: 'A small VPS (Hetzner CX22 ~€4/mo) or a home server / NUC / Pi 5. The first Rust build peaks near 3 GB, so add swap on a 2 GB box.'
    },
    {
      n: '02',
      title: 'Install Tailscale',
      body: 'This is how you reach Postern from your phone — no public DNS, no port-forwarding. It also handles HTTPS for you.'
    },
    {
      n: '03',
      title: 'Run the installer',
      body: 'A single license-gated command builds and starts Postern. Then open it on your tailnet and connect your first mailbox.'
    }
  ];
</script>

<Seo
  title="Download"
  description="Install Postern on your own server in about 15 minutes. A license-gated one-liner for Pro, or the free Community edition straight from GitHub."
  path="/download"
/>

<section class="head shell">
  <span class="eyebrow" use:reveal>Get started</span>
  <h1 use:reveal={60}>Up and running in <span class="accent-teal">~15 minutes</span>.</h1>
  <p class="lead" use:reveal={120}>
    Postern installs on a clean Ubuntu or Debian box with a single command. Pick an edition, run the
    installer, and connect your mailbox.
  </p>
</section>

<!-- edition cards -->
<section class="shell editions">
  <article class="card edition" use:reveal>
    <header>
      <h2>Community</h2>
      <span class="tag">Free · Apache 2.0</span>
    </header>
    <p>Up to 3 mailboxes, fully self-hosted, no license. Install straight from the public repo:</p>
    <CodeBlock code={COMMUNITY_CMD} />
    <p class="hint">Community runs without Tailscale — reach it on your LAN, or add a mesh yourself. Inspect the public installer before running it.</p>
    <div class="edition__btns">
      <a class="btn btn--ghost" href={URLS.github} target="_blank" rel="noopener"><Icon name="github" size={16} /> View on GitHub</a>
      <a class="btn btn--ghost" href={COMMUNITY_INSTALLER_URL} target="_blank" rel="noopener">Inspect installer</a>
      <a class="btn btn--ghost" href={URLS.docsCommunity} target="_blank" rel="noopener">Community guide</a>
    </div>
  </article>

  <article class="card edition" use:reveal={120}>
    <header>
      <h2>Pro</h2>
      <span class="tag tag--pro">License required</span>
    </header>
    <p>Unlimited mailboxes and every connected feature. Grab a key from the billing portal, then run:</p>
    <CodeBlock code={PRO_CMD} />
    <p class="hint">
      Replace the key with your license (<code>PSTN-XXXX-...</code>) from the
      <a href={URLS.billing} target="_blank" rel="noopener">billing portal</a>.
      Review the installer before running it.
    </p>
    <div class="edition__btns">
      <a class="btn btn--primary" href={URLS.storePro} target="_blank" rel="noopener">Get a license <Icon name="arrow" size={16} /></a>
      <a class="btn btn--ghost" href={PRO_INSTALLER_URL} target="_blank" rel="noopener">Inspect installer</a>
      <a class="btn btn--ghost" href={URLS.docsInstall} target="_blank" rel="noopener">Manual guide</a>
    </div>
  </article>
</section>

<!-- trust -->
<section class="section shell trust-section">
  <div class="trust card" use:reveal>
    <header>
      <span class="eyebrow">Before sudo</span>
      <h2>Install with your eyes open.</h2>
      <p class="lead">
        The one-line installer is for convenience. You can inspect the shell script first, follow the
        manual guide, and keep the host private with outbound-only networking.
      </p>
    </header>
    <ul>
      <li><Icon name="check" size={16} /> Public Community installer source and Apache 2.0 code</li>
      <li><Icon name="check" size={16} /> Manual install docs for users who do not want pipe-to-shell</li>
      <li><Icon name="check" size={16} /> No inbound ports required for the recommended Pro mesh setup</li>
    </ul>
  </div>
</section>

<!-- steps -->
<section class="section shell">
  <header class="sec-head" use:reveal>
    <span class="eyebrow">Three steps</span>
    <h2>How the install goes.</h2>
  </header>
  <ol class="steps">
    {#each steps as s, i}
      <li class="card" use:reveal={i * 90}>
        <span class="steps__n">{s.n}</span>
        <h3>{s.title}</h3>
        <p>{s.body}</p>
      </li>
    {/each}
  </ol>
  <div class="ts card" use:reveal>
    <div>
      <h3>Install Tailscale first</h3>
      <p>Run this on the host before the Postern installer (Pro). It joins the box to your private mesh.</p>
    </div>
    <CodeBlock code={TAILSCALE_CMD} />
  </div>
</section>

<!-- requirements -->
<section class="section shell">
  <div class="req-grid">
    <header class="sec-head" use:reveal>
      <span class="eyebrow">Before you start</span>
      <h2>System requirements.</h2>
      <p class="lead">Modest hardware is fine. The only spike is the first compile, which is memory-hungry for a few minutes.</p>
      <a class="textlink" href={URLS.docsHomeServer} target="_blank" rel="noopener">
        Home server / NUC guide <Icon name="arrow" size={16} />
      </a>
    </header>
    <dl class="reqs card" use:reveal={100}>
      {#each reqs as [k, v]}
        <div class="reqs__row">
          <dt>{k}</dt>
          <dd>{v}</dd>
        </div>
      {/each}
    </dl>
  </div>
</section>

<section class="section shell">
  <div class="closer card" use:reveal>
    <h2>Stuck? The docs have you.</h2>
    <p class="lead">Step-by-step install, server hardening, and provider-specific setup guides.</p>
    <div class="closer__btns">
      <a class="btn btn--primary btn--lg" href={URLS.docs} target="_blank" rel="noopener">Open documentation <Icon name="arrow" size={18} /></a>
      <a class="btn btn--ghost btn--lg" href={URLS.docsHardening} target="_blank" rel="noopener">Harden the server</a>
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

  .editions {
    display: grid;
    grid-template-columns: 1fr;
    gap: 1.4rem;
  }
  .edition {
    padding: clamp(1.6rem, 1.3rem + 1.5vw, 2.4rem);
    display: flex;
    flex-direction: column;
    gap: 1.1rem;
  }
  .edition header {
    display: flex;
    align-items: center;
    gap: 0.8rem;
  }
  .edition h2 {
    font-size: var(--text-xl);
  }
  .tag {
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    letter-spacing: 0.06em;
    color: var(--text-muted);
    border: 1px solid var(--border);
    border-radius: var(--r-pill);
    padding: 0.25rem 0.7rem;
  }
  .tag--pro {
    color: var(--accent);
    border-color: var(--accent);
  }
  .edition > p {
    color: var(--text-muted);
    font-size: var(--text-sm);
  }
  .hint {
    color: var(--text-faint);
    font-size: var(--text-xs);
  }
  .hint code {
    font-family: var(--font-mono);
    color: var(--text-muted);
  }
  .hint a {
    color: var(--accent);
  }
  .edition__btns {
    display: flex;
    flex-wrap: wrap;
    gap: 0.7rem;
    margin-top: auto;
    padding-top: 0.4rem;
  }

  .trust-section {
    padding-bottom: 0;
  }
  .trust {
    padding: clamp(1.8rem, 1.4rem + 2vw, 2.8rem);
    display: grid;
    grid-template-columns: 1.05fr 0.95fr;
    gap: 1.4rem;
    align-items: center;
  }
  .trust h2 {
    font-size: var(--text-2xl);
    margin: 0.7rem 0 0.8rem;
  }
  .trust ul {
    display: grid;
    gap: 0.8rem;
  }
  .trust li {
    display: flex;
    gap: 0.6rem;
    color: var(--text-muted);
    font-size: var(--text-sm);
  }
  .trust :global(svg) {
    color: var(--accent);
    margin-top: 3px;
  }

  .sec-head {
    max-width: 58ch;
    margin-bottom: 2.2rem;
  }
  .sec-head h2 {
    font-size: var(--text-2xl);
    margin: 0.7rem 0 0.8rem;
  }
  .textlink {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    color: var(--accent);
    font-family: var(--font-display);
    font-weight: 600;
    font-size: var(--text-sm);
    margin-top: 0.5rem;
  }

  .steps {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 1.2rem;
  }
  .steps li {
    padding: 1.6rem;
  }
  .steps__n {
    font-family: var(--font-mono);
    color: var(--accent);
    font-size: var(--text-sm);
  }
  .steps h3 {
    font-size: var(--text-lg);
    margin: 0.6rem 0 0.5rem;
  }
  .steps p {
    color: var(--text-muted);
    font-size: var(--text-sm);
  }
  .ts {
    margin-top: 1.2rem;
    padding: 1.6rem;
    display: grid;
    grid-template-columns: 0.8fr 1.2fr;
    gap: 1.4rem;
    align-items: center;
  }
  .ts h3 {
    font-size: var(--text-lg);
    margin-bottom: 0.4rem;
  }
  .ts p {
    color: var(--text-muted);
    font-size: var(--text-sm);
  }

  .req-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: clamp(1.5rem, 1rem + 3vw, 3.5rem);
    align-items: start;
  }
  .reqs {
    padding: 0.5rem 1.6rem;
  }
  .reqs__row {
    display: flex;
    justify-content: space-between;
    gap: 1rem;
    padding: 1rem 0;
    border-bottom: 1px solid var(--border);
  }
  .reqs__row:last-child {
    border-bottom: 0;
  }
  .reqs dt {
    font-family: var(--font-display);
    font-weight: 600;
    color: var(--text);
  }
  .reqs dd {
    margin: 0;
    color: var(--text-muted);
    font-size: var(--text-sm);
    text-align: right;
  }

  .closer {
    text-align: center;
    padding: clamp(2.5rem, 2rem + 3vw, 4.5rem) 2rem;
    background: radial-gradient(80% 130% at 50% 0%, var(--glow-teal), transparent 60%), var(--surface);
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

  @media (max-width: 820px) {
    .editions,
    .steps,
    .ts,
    .req-grid,
    .trust {
      grid-template-columns: 1fr;
    }
  }
</style>
