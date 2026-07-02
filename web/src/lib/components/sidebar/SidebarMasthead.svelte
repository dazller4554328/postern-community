<script lang="ts">
  import { onMount } from 'svelte';
  import { prefs, type Theme } from '$lib/prefs';

  // Logo file is theme-dependent (cyberpunk / dark / light variants).
  // Bumped on each release that ships a new logo asset so caches
  // don't pin the old one.
  const LOGO_VERSION = '4';

  let prefersDark = $state(false);
  let currentTheme = $state<Theme>('system');

  onMount(() => {
    const mql = window.matchMedia('(prefers-color-scheme: dark)');
    prefersDark = mql.matches;
    const handler = (e: MediaQueryListEvent) => (prefersDark = e.matches);
    mql.addEventListener('change', handler);
    const unsub = prefs.subscribe((p) => (currentTheme = p.theme));
    return () => {
      mql.removeEventListener('change', handler);
      unsub();
    };
  });

  let effectiveTheme = $derived(
    currentTheme === 'system' ? (prefersDark ? 'dark' : 'light') : currentTheme
  );
  let logoSrc = $derived(
    currentTheme === 'cyberpunk'
      ? `/logo-cyberpunk.png?v=${LOGO_VERSION}`
      : effectiveTheme === 'dark'
        ? `/logo-dark.png?v=${LOGO_VERSION}`
        : `/logo-light.png?v=${LOGO_VERSION}`
  );
</script>

<header class="masthead">
  <a class="brand" href="/inbox" aria-label="Postern home">
    <span class="brand-mark" aria-hidden="true"></span>
    <img src={logoSrc} alt="Postern" class="logo" />
  </a>
  <p class="brand-copy">Your mail. Your server. Your keys.</p>
  <div class="masthead-meta">
    <span class="vault-note">Vault-backed session</span>
  </div>
</header>

<style>
  .masthead {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 0.65rem;
    padding: 1.15rem 1rem 0.85rem;
    border-bottom: 1px solid var(--border);
    background:
      radial-gradient(circle at top left, color-mix(in oklab, var(--accent) 12%, transparent), transparent 48%),
      linear-gradient(180deg, color-mix(in oklab, var(--surface-2) 75%, transparent), transparent);
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    color: inherit;
    text-decoration: none;
    line-height: 0;
    min-width: 0;
  }
  .brand-mark {
    width: 1rem;
    height: 1rem;
    border-radius: 0.28rem;
    background:
      linear-gradient(135deg, var(--accent), color-mix(in oklab, var(--accent) 40%, white 60%));
    box-shadow:
      0 0 0 1px color-mix(in oklab, var(--accent) 18%, transparent),
      0 10px 22px color-mix(in oklab, var(--accent) 18%, transparent);
    flex-shrink: 0;
  }
  .brand .logo {
    display: block;
    height: 31px;
    width: auto;
    max-width: 100%;
    object-fit: contain;
    object-position: left center;
  }
  .brand-copy {
    margin: 0;
    font-size: 0.76rem;
    line-height: 1.45;
    color: var(--muted);
    max-width: 22ch;
  }
  .masthead-meta {
    display: flex;
    align-items: center;
    gap: 0.55rem;
    flex-wrap: wrap;
  }
  .vault-note {
    font-size: 0.68rem;
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--muted);
  }
</style>
