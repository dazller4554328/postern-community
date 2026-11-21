<script lang="ts">
  import { URLS, YEAR } from '$lib/site';
  import Logo from './Logo.svelte';
  import Icon from './Icon.svelte';

  const cols = [
    {
      title: 'Product',
      links: [
        { label: 'Features', href: '/features' },
        { label: 'Pricing', href: '/pricing' },
        { label: 'Download', href: '/download' }
      ]
    },
    {
      title: 'Resources',
      links: [
        { label: 'Documentation', href: URLS.docs, external: true },
        { label: 'Install guide', href: URLS.docsInstall, external: true },
        { label: 'Server hardening', href: URLS.docsHardening, external: true },
        { label: 'Community edition', href: URLS.docsCommunity, external: true }
      ]
    },
    {
      title: 'Project',
      links: [
        { label: 'GitHub', href: URLS.github, external: true },
        { label: 'Billing & licenses', href: URLS.billing, external: true }
      ]
    }
  ];
</script>

<footer class="footer">
  <div class="shell grid">
    <div class="about">
      <Logo size={30} />
      <p>Your email, your keys, your server. A privacy-first, self-hosted email client.</p>
      <a class="gh" href={URLS.github} target="_blank" rel="noopener" aria-label="Postern on GitHub">
        <Icon name="github" size={20} />
      </a>
    </div>

    {#each cols as col}
      <nav class="col" aria-label={col.title}>
        <h3>{col.title}</h3>
        <ul>
          {#each col.links as l}
            <li>
              <a
                href={l.href}
                target={'external' in l && l.external ? '_blank' : undefined}
                rel={'external' in l && l.external ? 'noopener' : undefined}>{l.label}</a
              >
            </li>
          {/each}
        </ul>
      </nav>
    {/each}
  </div>

  <div class="shell base">
    <span>© {YEAR} Postern. Source under Apache 2.0.</span>
    <span class="ghost">Not affiliated with your mail provider. Bring your own IMAP.</span>
  </div>
</footer>

<style>
  .footer {
    border-top: 1px solid var(--border);
    background: var(--bg-1);
    padding-block: clamp(3rem, 2rem + 4vw, 5rem) 2rem;
  }
  .grid {
    display: grid;
    grid-template-columns: 1.6fr repeat(3, 1fr);
    gap: 2.5rem;
  }
  .about p {
    margin-top: 1rem;
    color: var(--text-muted);
    font-size: var(--text-sm);
    max-width: 30ch;
  }
  .gh {
    display: inline-grid;
    place-items: center;
    width: 40px;
    height: 40px;
    margin-top: 1.2rem;
    border: 1px solid var(--border);
    border-radius: var(--r-pill);
    color: var(--text-muted);
    transition:
      color var(--dur-fast) var(--ease),
      border-color var(--dur-fast) var(--ease);
  }
  .gh:hover {
    color: var(--text);
    border-color: var(--accent);
  }
  .col h3 {
    font-family: var(--font-display);
    font-size: var(--text-sm);
    letter-spacing: 0.04em;
    margin-bottom: 1rem;
  }
  .col ul {
    display: flex;
    flex-direction: column;
    gap: 0.7rem;
  }
  .col a {
    color: var(--text-muted);
    font-size: var(--text-sm);
    transition: color var(--dur-fast) var(--ease);
  }
  .col a:hover {
    color: var(--accent);
  }
  .base {
    display: flex;
    flex-wrap: wrap;
    justify-content: space-between;
    gap: 0.5rem;
    margin-top: clamp(2.5rem, 2rem + 3vw, 4rem);
    padding-top: 1.6rem;
    border-top: 1px solid var(--border);
    color: var(--text-faint);
    font-size: var(--text-xs);
  }
  .ghost {
    color: var(--text-faint);
  }

  @media (max-width: 820px) {
    .grid {
      grid-template-columns: 1fr 1fr;
      gap: 2rem;
    }
    .about {
      grid-column: 1 / -1;
    }
  }
  @media (max-width: 520px) {
    .grid {
      grid-template-columns: 1fr;
    }
  }
</style>
