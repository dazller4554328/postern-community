<script lang="ts">
  import Seo from '$lib/components/Seo.svelte';
  import Icon from '$lib/components/Icon.svelte';
  import { reveal } from '$lib/reveal';
  import { PLAN_FEATURES, URLS } from '$lib/site';

  const communityIncludes = PLAN_FEATURES.filter((f) => f.community === true || typeof f.community === 'string');
  const faqs = [
    {
      q: 'Is the Community edition really free?',
      a: 'Yes. Postern Community is open source under Apache 2.0 with no time limit and no feature nag. The 3-mailbox cap is the only difference in scope.'
    },
    {
      q: 'What does the Pro license cost?',
      a: 'Pro is a one-off £100 payment with 3 years of updates. It unlocks unlimited mailboxes plus connected features: Tailscale mesh access, opt-in local-first AI, calendars and contacts, per-device sessions, automatic VPN egress, and one-click in-app updates.'
    },
    {
      q: 'Do I still self-host on Pro?',
      a: 'Always. Both editions run on your own hardware. A Pro license unlocks features in the same binary — your mail never routes through us.'
    },
    {
      q: 'Can I upgrade later without losing data?',
      a: 'Yes. The schema is identical between editions, so you can start on Community and apply a Pro license in place — your vault carries straight over.'
    }
  ];
</script>

<Seo
  title="Pricing"
  description="Postern Community is free and open source with up to 3 mailboxes. Pro is a one-off £100 payment with 3 years of updates and unlocks unlimited mailboxes, mesh access, local-first AI, calendars, and one-click updates."
  path="/pricing"
/>

<section class="head shell">
  <span class="eyebrow" use:reveal>Pricing</span>
  <h1 use:reveal={60}>Own your mail. <span class="accent-teal">Free</span> or <span class="accent-ern">Pro</span>.</h1>
  <p class="lead" use:reveal={120}>
    Both editions are self-hosted and built on the same encrypted core. Start free, upgrade in place
    when you want the connected features.
  </p>
</section>

<section class="shell plans">
  <article class="card plan" use:reveal>
    <header>
      <h2>Community</h2>
      <p class="price">Free</p>
      <p class="price-note">Open source · Apache 2.0</p>
    </header>
    <p class="plan__lead">Everything you need to take your mailbox back, for up to three accounts.</p>
    <a class="btn btn--ghost btn--lg full" href="/download">Download &amp; self-host</a>
    <ul class="incl">
      {#each communityIncludes as f}
        <li>
          <Icon name="check" size={16} />
          <span>{f.label}{#if typeof f.community === 'string'} — <em>{f.community}</em>{/if}</span>
        </li>
      {/each}
    </ul>
  </article>

  <article class="card plan plan--pro" use:reveal={120}>
    <span class="badge">Most complete</span>
    <header>
      <h2>Pro</h2>
      <p class="price">£100</p>
      <p class="price-note">One-off payment · 3 years of updates</p>
    </header>
    <p class="plan__lead">The full client: unlimited mailboxes, every connected feature, and updates through the license term.</p>
    <a class="btn btn--primary btn--lg full" href={URLS.storePro} target="_blank" rel="noopener">
      Buy or manage Pro <Icon name="arrow" size={16} />
    </a>
    <ul class="incl">
      <li><Icon name="check" size={16} /><span><strong>Everything in Community</strong></span></li>
      {#each PLAN_FEATURES.filter((f) => f.pro === true && f.community === false) as f}
        <li><Icon name="check" size={16} /><span>{f.label}</span></li>
      {/each}
      <li><Icon name="check" size={16} /><span>Unlimited mailboxes</span></li>
    </ul>
  </article>
</section>

<!-- comparison -->
<section class="section shell">
  <header class="sec-head" use:reveal>
    <span class="eyebrow">Side by side</span>
    <h2>Compare editions.</h2>
  </header>

  <div class="table-wrap card" use:reveal>
    <table>
      <thead>
        <tr>
          <th scope="col">Feature</th>
          <th scope="col">Community</th>
          <th scope="col" class="pro-col">Pro</th>
        </tr>
      </thead>
      <tbody>
        {#each PLAN_FEATURES as f}
          <tr>
            <th scope="row">{f.label}</th>
            <td>
              {#if f.community === true}
                <Icon name="check" size={18} /><span class="visually-hidden">Included</span>
              {:else if f.community === false}
                <span class="no"><Icon name="cross" size={16} /></span><span class="visually-hidden">Not included</span>
              {:else}
                <span class="val">{f.community}</span>
              {/if}
            </td>
            <td class="pro-col">
              {#if f.pro === true}
                <span class="yes"><Icon name="check" size={18} /></span><span class="visually-hidden">Included</span>
              {:else if f.pro === false}
                <span class="no"><Icon name="cross" size={16} /></span>
              {:else}
                <span class="val">{f.pro}</span>
              {/if}
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
</section>

<!-- faq -->
<section class="section shell">
  <header class="sec-head" use:reveal>
    <span class="eyebrow">Questions</span>
    <h2>Good to know.</h2>
  </header>
  <div class="faq">
    {#each faqs as f, i}
      <details class="card" use:reveal={(i % 2) * 80}>
        <summary>{f.q}<Icon name="arrow" size={18} /></summary>
        <p>{f.a}</p>
      </details>
    {/each}
  </div>
</section>

<style>
  .head {
    padding-top: clamp(2.5rem, 1.5rem + 4vw, 5rem);
    max-width: 62ch;
  }
  .head h1 {
    font-size: var(--text-3xl);
    margin: 1rem 0 1.2rem;
  }

  .plans {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 1.4rem;
    align-items: start;
  }
  .plan {
    padding: clamp(1.8rem, 1.4rem + 1.5vw, 2.6rem);
    display: flex;
    flex-direction: column;
  }
  .plan header h2 {
    font-size: var(--text-xl);
  }
  .price {
    font-family: var(--font-display);
    font-size: var(--text-3xl);
    font-weight: 700;
    margin-top: 0.4rem;
  }
  .price-note {
    color: var(--text-muted);
    font-size: var(--text-sm);
    margin-top: 0.2rem;
  }
  .plan__lead {
    color: var(--text-muted);
    margin: 1.2rem 0 1.6rem;
  }
  .full {
    width: 100%;
  }
  .plan--pro {
    position: relative;
    border-color: var(--border-strong);
    background: radial-gradient(120% 120% at 100% 0%, var(--glow-teal), transparent 55%), var(--surface);
  }
  .badge {
    position: absolute;
    top: 1.4rem;
    right: 1.4rem;
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    letter-spacing: 0.1em;
    text-transform: uppercase;
    color: var(--accent);
    border: 1px solid var(--accent);
    border-radius: var(--r-pill);
    padding: 0.25rem 0.7rem;
  }
  .incl {
    display: grid;
    gap: 0.75rem;
    margin-top: 1.8rem;
  }
  .incl li {
    display: flex;
    gap: 0.6rem;
    align-items: flex-start;
    font-size: var(--text-sm);
    color: var(--text);
  }
  .incl :global(svg) {
    color: var(--accent);
    margin-top: 2px;
    flex: none;
  }
  .incl em {
    color: var(--text-muted);
    font-style: normal;
  }

  .sec-head h2 {
    font-size: var(--text-2xl);
    margin-top: 0.7rem;
  }

  /* table */
  .table-wrap {
    overflow-x: auto;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    min-width: 440px;
  }
  th,
  td {
    text-align: left;
    padding: 0.95rem 1.2rem;
    border-bottom: 1px solid var(--border);
  }
  thead th {
    font-family: var(--font-display);
    font-size: var(--text-sm);
    color: var(--text-muted);
  }
  tbody th {
    font-weight: 500;
    font-size: var(--text-sm);
  }
  td {
    text-align: center;
    width: 22%;
    color: var(--accent);
  }
  .pro-col {
    background: color-mix(in oklab, var(--accent) 6%, transparent);
  }
  thead .pro-col {
    color: var(--accent);
  }
  td .no {
    color: var(--text-faint);
    display: inline-grid;
    place-items: center;
  }
  td .val {
    color: var(--text);
    font-size: var(--text-sm);
    font-weight: 500;
  }
  tbody tr:last-child th,
  tbody tr:last-child td {
    border-bottom: 0;
  }

  /* faq */
  .faq {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 1rem;
  }
  details {
    padding: 0 1.4rem;
  }
  summary {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    padding: 1.2rem 0;
    cursor: pointer;
    list-style: none;
    font-family: var(--font-display);
    font-weight: 600;
    font-size: var(--text-base);
  }
  summary::-webkit-details-marker {
    display: none;
  }
  summary :global(svg) {
    color: var(--text-muted);
    transform: rotate(90deg);
    transition: transform var(--dur) var(--ease);
    flex: none;
  }
  details[open] summary :global(svg) {
    transform: rotate(-90deg);
    color: var(--accent);
  }
  details p {
    color: var(--text-muted);
    font-size: var(--text-sm);
    padding-bottom: 1.3rem;
    max-width: 52ch;
  }

  @media (max-width: 760px) {
    .plans,
    .faq {
      grid-template-columns: 1fr;
    }
  }
</style>
