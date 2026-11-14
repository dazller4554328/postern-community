<script lang="ts">
  import Seo from '$lib/components/Seo.svelte';
  import Icon from '$lib/components/Icon.svelte';
  import { reveal } from '$lib/reveal';
  import { URLS } from '$lib/site';

  // Direct WHMCS order links: free 7-day trial (lead) + the paid license.
  const trialUrl = URLS.storePgpTrial;
  const buyUrl = URLS.storePgp;
  const docsUrl = `${URLS.docs}guides/whmcs-pgp/`;

  const steps = [
    {
      n: '01',
      title: 'Install the module',
      blurb: 'Drop the addon and mail provider into your WHMCS modules folder and activate them. No core edits.'
    },
    {
      n: '02',
      title: 'Pick it as your mail provider',
      blurb: 'Select “Postern PGP” under System Settings → Mail and point it at your existing SMTP.'
    },
    {
      n: '03',
      title: 'Mail encrypts itself',
      blurb: 'Every outbound message to a key-holding recipient goes out PGP-encrypted. Everyone else gets normal mail.'
    }
  ];

  const features = [
    {
      icon: 'shield',
      title: 'Automatic encryption',
      blurb:
        'Sits in the transport path for every outbound email — invoices, password resets, support replies — and seals each one to the recipient’s key when it has one.'
    },
    {
      icon: 'key',
      title: 'Opportunistic key discovery',
      blurb:
        'Finds recipient public keys via keys.openpgp.org and WKD, caches the result, and encrypts without you pasting a thing.'
    },
    {
      icon: 'server',
      title: 'Covers all WHMCS mail',
      blurb:
        'Templates and custom messages alike — including the ones older hook-based tools can’t see. Attachments are encrypted too, full PGP/MIME.'
    },
    {
      icon: 'check',
      title: 'Your SMTP, your keys',
      blurb:
        'Runs through the transport you already configure. Nothing routes through a third party; no key escrow, no telemetry.'
    }
  ];

  const faqs = [
    {
      q: 'How is it licensed?',
      a: 'Start with a free 7-day trial — no commitment. After that it’s a one-off £50 payment for a single WHMCS installation, backed by a 30-day money-back guarantee. The license locks to your WHMCS domain on first activation; if you migrate servers you can re-bind it yourself in one click.'
    },
    {
      q: 'What are the requirements?',
      a: 'WHMCS 8.6+ on PHP 8.1–8.4 with the ionCube Loader (free, and already present on most WHMCS hosts), plus the PHP gnupg extension (sudo apt install php-gnupg). Without gnupg, mail still sends — just unencrypted.'
    },
    {
      q: 'Do my clients need anything?',
      a: 'No. If a recipient publishes a PGP key (keys.openpgp.org or WKD) it’s used automatically. Recipients without a key keep getting normal email, plus an optional invite to set PGP up.'
    },
    {
      q: 'Does it change how mail is sent?',
      a: 'Only the encryption. Delivery still goes over your own SMTP. Encryption is best-effort: if anything can’t be encrypted it falls back to a normal send, so mail never fails to go out.'
    },
    {
      q: 'Is it tied to the Postern app?',
      a: 'No — it’s a standalone WHMCS module. It uses the same OpenPGP approach as the Postern email client, but you don’t need to run Postern to use it.'
    }
  ];

  const productJsonLd = JSON.stringify({
    '@context': 'https://schema.org',
    '@type': 'Product',
    name: 'Postern PGP Mailer for WHMCS',
    description:
      'A WHMCS module that automatically PGP-encrypts outbound email to recipients whose public key is known or discoverable.',
    brand: { '@type': 'Brand', name: 'Postern' },
    url: 'https://postern.email/whmcs-pgp',
    image: 'https://postern.email/img/og/postern-og.jpg',
    offers: {
      '@type': 'Offer',
      price: '50',
      priceCurrency: 'GBP',
      url: 'https://postern.email/whmcs-pgp',
      availability: 'https://schema.org/InStock',
      // Digital download: delivered instantly, no shipping. 30-day money-back guarantee.
      hasMerchantReturnPolicy: {
        '@type': 'MerchantReturnPolicy',
        applicableCountry: 'GB',
        returnPolicyCategory: 'https://schema.org/MerchantReturnFiniteReturnWindow',
        merchantReturnDays: 30,
        returnMethod: 'https://schema.org/ReturnByMail',
        returnFees: 'https://schema.org/FreeReturn'
      },
      shippingDetails: {
        '@type': 'OfferShippingDetails',
        shippingRate: { '@type': 'MonetaryAmount', value: '0', currency: 'GBP' },
        shippingDestination: { '@type': 'DefinedRegion', addressCountry: 'GB' },
        deliveryTime: {
          '@type': 'ShippingDeliveryTime',
          handlingTime: { '@type': 'QuantitativeValue', minValue: 0, maxValue: 0, unitCode: 'DAY' },
          transitTime: { '@type': 'QuantitativeValue', minValue: 0, maxValue: 0, unitCode: 'DAY' }
        }
      }
    }
  });
</script>

<Seo
  title="WHMCS PGP Email Encryption"
  description="Postern PGP Mailer is a WHMCS add-on that automatically PGP-encrypts your outbound WHMCS email — invoices, password resets, support replies — to any recipient with a published key. One-off £50, per installation."
  path="/whmcs-pgp"
/>

<svelte:head>
  {@html `<script type="application/ld+json">${productJsonLd.replace(/</g, '\\u003c')}</script>`}
</svelte:head>

<!-- hero -->
<section class="head shell">
  <span class="eyebrow" use:reveal>WHMCS add-on</span>
  <h1 use:reveal={60}>
    PGP-encrypt your <span class="accent-teal">WHMCS</span> email — automatically.
  </h1>
  <p class="lead" use:reveal={120}>
    WHMCS sends invoices, password resets and support replies in plaintext. <strong>Postern PGP
    Mailer</strong> seals every outbound message to recipients with a published key, and sends
    normally to everyone else. No workflow changes, no per-message effort.
  </p>
  <div class="cta-row" use:reveal={180}>
    <a class="btn btn--primary btn--lg" href={trialUrl} target="_blank" rel="noopener">
      Start 7-day free trial <Icon name="arrow" size={16} />
    </a>
    <a class="btn btn--ghost btn--lg" href={buyUrl} target="_blank" rel="noopener">
      Buy for £50
    </a>
  </div>
  <p class="micro" use:reveal={220}>
    Free for 7 days · then £50 one-off · 30-day money-back guarantee
  </p>
</section>

<!-- features -->
<section class="section shell">
  <header class="sec-head" use:reveal>
    <span class="eyebrow">What it does</span>
    <h2>Encryption that takes care of itself.</h2>
  </header>
  <div class="grid">
    {#each features as f, i}
      <article class="card feat" use:reveal={(i % 2) * 80}>
        <span class="ic"><Icon name={f.icon} size={20} /></span>
        <h3>{f.title}</h3>
        <p>{f.blurb}</p>
      </article>
    {/each}
  </div>
</section>

<!-- how -->
<section class="section shell">
  <header class="sec-head" use:reveal>
    <span class="eyebrow">Setup in minutes</span>
    <h2>Three steps to encrypted mail.</h2>
  </header>
  <ol class="steps">
    {#each steps as s, i}
      <li class="card step" use:reveal={(i % 3) * 70}>
        <span class="n">{s.n}</span>
        <h3>{s.title}</h3>
        <p>{s.blurb}</p>
      </li>
    {/each}
  </ol>
</section>

<!-- pricing -->
<section class="section shell">
  <article class="card buy" use:reveal>
    <div class="buy__copy">
      <span class="eyebrow">Pricing</span>
      <h2>Try it free, then one price.</h2>
      <p>
        Start with a free 7-day trial — no commitment. Keep it and it’s a perpetual license for a
        single WHMCS install, locked to your domain and movable to a new server whenever you need.
        Backed by a 30-day money-back guarantee.
      </p>
      <ul class="incl">
        <li><Icon name="check" size={16} /><span>Free 7-day trial — try it before you buy</span></li>
        <li><Icon name="check" size={16} /><span>Automatic PGP/MIME encryption, body + attachments</span></li>
        <li><Icon name="check" size={16} /><span>Opportunistic key discovery (keys.openpgp.org + WKD)</span></li>
        <li><Icon name="check" size={16} /><span>Works with templates <em>and</em> custom emails</span></li>
        <li><Icon name="check" size={16} /><span>Self-service license move between servers</span></li>
        <li><Icon name="check" size={16} /><span>30-day money-back guarantee</span></li>
      </ul>
    </div>
    <div class="buy__price">
      <p class="price">£50</p>
      <p class="price-note">one-off · per installation</p>
      <a class="btn btn--primary btn--lg full" href={trialUrl} target="_blank" rel="noopener">
        Start free trial <Icon name="arrow" size={16} />
      </a>
      <a class="btn btn--ghost full" href={buyUrl} target="_blank" rel="noopener">Buy now · £50</a>
    </div>
  </article>
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
    max-width: 64ch;
  }
  .head h1 {
    font-size: var(--text-3xl);
    margin: 1rem 0 1.2rem;
  }
  .cta-row {
    display: flex;
    flex-wrap: wrap;
    gap: 0.8rem;
    margin-top: 1.8rem;
  }
  .micro {
    margin-top: 1rem;
    color: var(--text-muted);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    letter-spacing: 0.04em;
  }

  .sec-head h2 {
    font-size: var(--text-2xl);
    margin-top: 0.7rem;
  }

  /* features */
  .grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 1.2rem;
  }
  .feat {
    padding: clamp(1.5rem, 1.2rem + 1vw, 2.1rem);
  }
  .ic {
    display: inline-grid;
    place-items: center;
    width: 44px;
    height: 44px;
    border-radius: var(--r-md);
    color: var(--accent);
    background: color-mix(in oklab, var(--accent) 12%, transparent);
  }
  .feat h3 {
    font-size: var(--text-lg);
    margin: 1rem 0 0.5rem;
  }
  .feat p {
    color: var(--text-muted);
    font-size: var(--text-sm);
  }

  /* steps */
  .steps {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 1.2rem;
    counter-reset: step;
  }
  .step {
    padding: clamp(1.5rem, 1.2rem + 1vw, 2.1rem);
  }
  .step .n {
    font-family: var(--font-mono);
    font-size: var(--text-sm);
    color: var(--accent);
    letter-spacing: 0.1em;
  }
  .step h3 {
    font-size: var(--text-lg);
    margin: 0.7rem 0 0.5rem;
  }
  .step p {
    color: var(--text-muted);
    font-size: var(--text-sm);
  }

  /* buy */
  .buy {
    display: grid;
    grid-template-columns: 1.6fr 1fr;
    gap: clamp(1.5rem, 1rem + 3vw, 3rem);
    padding: clamp(1.8rem, 1.4rem + 1.5vw, 2.8rem);
    align-items: center;
    background: radial-gradient(120% 120% at 100% 0%, var(--glow-teal), transparent 55%), var(--surface);
    border-color: var(--border-strong);
  }
  .buy h2 {
    font-size: var(--text-xl);
    margin: 0.6rem 0 0.9rem;
  }
  .buy__copy > p {
    color: var(--text-muted);
    font-size: var(--text-sm);
    max-width: 52ch;
  }
  .buy__price {
    text-align: center;
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
  }
  .price {
    font-family: var(--font-display);
    font-size: var(--text-3xl);
    font-weight: 700;
  }
  .price-note {
    color: var(--text-muted);
    font-size: var(--text-sm);
    margin-top: -0.4rem;
  }
  .full {
    width: 100%;
  }
  .incl {
    display: grid;
    gap: 0.75rem;
    margin-top: 1.4rem;
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

  @media (max-width: 820px) {
    .grid,
    .steps,
    .buy,
    .faq {
      grid-template-columns: 1fr;
    }
  }
</style>
