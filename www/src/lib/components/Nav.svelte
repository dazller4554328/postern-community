<script lang="ts">
  import { page } from '$app/stores';
  import { NAV, URLS } from '$lib/site';
  import Logo from './Logo.svelte';
  import Icon from './Icon.svelte';
  import ThemeToggle from './ThemeToggle.svelte';

  let open = $state(false);
  let scrolled = $state(false);

  $effect(() => {
    const onScroll = () => (scrolled = window.scrollY > 12);
    onScroll();
    window.addEventListener('scroll', onScroll, { passive: true });
    return () => window.removeEventListener('scroll', onScroll);
  });

  // Close the mobile menu whenever the route changes.
  $effect(() => {
    void $page.url.pathname;
    open = false;
  });

  let pathname = $derived($page.url.pathname);
</script>

<header class="nav" class:scrolled>
  <div class="shell bar">
    <a href="/" class="brand" aria-label="Postern home">
      <Logo size={28} />
    </a>

    <nav class="links" aria-label="Primary">
      {#each NAV as item}
        <a
          href={item.href}
          class="link"
          class:active={!('external' in item) && pathname === item.href}
          target={'external' in item ? '_blank' : undefined}
          rel={'external' in item ? 'noopener' : undefined}
        >
          {item.label}{#if 'external' in item}<span class="ext" aria-hidden="true">↗</span>{/if}
        </a>
      {/each}
    </nav>

    <div class="actions">
      <a href={URLS.github} class="iconlink" target="_blank" rel="noopener" aria-label="Postern on GitHub">
        <Icon name="github" size={19} />
      </a>
      <ThemeToggle />
      <a href="/download" class="btn btn--primary cta">Get started</a>
      <button
        type="button"
        class="burger"
        aria-expanded={open}
        aria-label="Toggle menu"
        onclick={() => (open = !open)}
      >
        <span class:x={open}></span>
        <span class:x={open}></span>
      </button>
    </div>
  </div>

  {#if open}
    <div class="sheet">
      <div class="shell sheet-inner">
        {#each NAV as item}
          <a
            href={item.href}
            target={'external' in item ? '_blank' : undefined}
            rel={'external' in item ? 'noopener' : undefined}
          >
            {item.label}{#if 'external' in item}<span class="ext" aria-hidden="true">↗</span>{/if}
          </a>
        {/each}
        <a class="btn btn--primary" href="/download">Get started</a>
      </div>
    </div>
  {/if}
</header>

<style>
  .nav {
    position: sticky;
    top: 0;
    z-index: 50;
    transition:
      background-color var(--dur) var(--ease),
      border-color var(--dur) var(--ease),
      backdrop-filter var(--dur) var(--ease);
    border-bottom: 1px solid transparent;
  }
  .nav.scrolled {
    background: color-mix(in oklab, var(--bg) 78%, transparent);
    backdrop-filter: blur(14px) saturate(140%);
    border-bottom-color: var(--border);
  }
  .bar {
    display: flex;
    align-items: center;
    gap: 1.5rem;
    height: 68px;
  }
  .brand {
    display: inline-flex;
  }
  .links {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    margin-left: auto;
  }
  .link {
    position: relative;
    padding: 0.45rem 0.7rem;
    font-size: var(--text-sm);
    font-weight: 500;
    color: var(--text-muted);
    border-radius: var(--r-sm);
    transition: color var(--dur-fast) var(--ease);
  }
  .link:hover {
    color: var(--text);
  }
  .link.active {
    color: var(--text);
  }
  .link.active::after {
    content: '';
    position: absolute;
    left: 0.7rem;
    right: 0.7rem;
    bottom: 0.1rem;
    height: 2px;
    border-radius: 2px;
    background: var(--accent);
  }
  .ext {
    font-size: 0.75em;
    opacity: 0.6;
    margin-left: 0.15em;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 0.65rem;
  }
  .iconlink {
    display: inline-grid;
    place-items: center;
    width: 38px;
    height: 38px;
    color: var(--text-muted);
    border-radius: var(--r-pill);
    transition: color var(--dur-fast) var(--ease);
  }
  .iconlink:hover {
    color: var(--text);
  }
  .burger {
    display: none;
    flex-direction: column;
    justify-content: center;
    gap: 5px;
    width: 40px;
    height: 38px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    cursor: pointer;
  }
  .burger span {
    display: block;
    width: 18px;
    height: 2px;
    margin-inline: auto;
    background: var(--text);
    border-radius: 2px;
    transition: transform var(--dur) var(--ease);
  }
  .burger span.x:first-child {
    transform: translateY(3.5px) rotate(45deg);
  }
  .burger span.x:last-child {
    transform: translateY(-3.5px) rotate(-45deg);
  }
  .sheet {
    border-bottom: 1px solid var(--border);
    background: color-mix(in oklab, var(--bg) 92%, transparent);
    backdrop-filter: blur(14px);
  }
  .sheet-inner {
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
    padding-block: 1rem 1.4rem;
  }
  .sheet-inner a:not(.btn) {
    padding: 0.7rem 0.2rem;
    font-family: var(--font-display);
    font-size: var(--text-lg);
    color: var(--text);
    border-bottom: 1px solid var(--border);
  }
  .sheet-inner .btn {
    margin-top: 0.6rem;
  }

  @media (max-width: 820px) {
    .links,
    .cta,
    .iconlink {
      display: none;
    }
    .burger {
      display: flex;
    }
  }
</style>
