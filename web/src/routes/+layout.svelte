<script lang="ts">
  import { page } from '$app/stores';
  import { onMount } from 'svelte';
  // Importing the module triggers its subscriber and ensures the theme/font
  // tokens get reapplied on any pref change from the settings page.
  import '$lib/prefs';
  import '$lib/cyberpunk.css';
  import { ensureTierLoaded } from '$lib/tier';
  ensureTierLoaded();
  import VaultGate from '$lib/components/VaultGate.svelte';
  import EventTicker from '$lib/components/EventTicker.svelte';
  import NotificationHost from '$lib/components/NotificationHost.svelte';
  import ReminderWatcher from '$lib/components/ReminderWatcher.svelte';
  import UpdateBanner from '$lib/components/UpdateBanner.svelte';
  import MobileShell from '$lib/components/mobile/MobileShell.svelte';

  let { children } = $props();

  // Inbox is the only app-shell page that owns its own viewport.
  // Everything else goes through the centered wrapper which gives it
  // breathing room and lets the page set its own clamp() max-width.
  let centered = $derived(!$page.url.pathname.startsWith('/inbox'));

  // Popup mode (?popup=1) — used by the Notes window opened from
  // compose. Strips the mobile shell, update banner, ticker, and
  // centered-main padding so the route fills the small popup chromelessly.
  let isPopup = $derived($page.url.searchParams.get('popup') === '1');

  // Mobile gate — below 900px we swap the entire chrome for a
  // BlueMail-style mobile shell. Desktop is untouched. We start as
  // `false` so SSR renders the desktop markup; the shell only takes
  // over after hydration when window is available.
  let isMobile = $state(false);
  onMount(() => {
    const mq = window.matchMedia('(max-width: 900px)');
    const sync = () => { isMobile = mq.matches; };
    sync();
    mq.addEventListener('change', sync);
    return () => mq.removeEventListener('change', sync);
  });
</script>

<svelte:head>
  <link rel="icon" type="image/svg+xml" href="/favicon.svg" />
  <link rel="apple-touch-icon" href="/logo-light.png" />
</svelte:head>

<VaultGate>
  {#if isPopup}
    <main class="popup">
      {@render children()}
    </main>
  {:else}
    <UpdateBanner />
    {#if isMobile}
      <MobileShell {children} />
    {:else}
      <main class:centered>
        {@render children()}
      </main>
      <EventTicker />
    {/if}
    <NotificationHost />
    <ReminderWatcher />
  {/if}
</VaultGate>

<style>
  /* Theme tokens. data-theme overrides the OS preference; when absent we
     follow `prefers-color-scheme`. */

  /* ── Light — "Airy" ──────────────────────────────────────────────────
     Cool near-white canvas with pure-white surface cards. Meaningful
     bg/surface contrast so cards visibly float off the page. */
  :global(html) {
    --bg: #f6f8fb;
    --fg: #1b232a;
    --surface: #ffffff;
    --surface-2: #eef2f6;
    --border: rgba(24, 31, 40, 0.10);
    --muted: rgba(27, 35, 42, 0.58);
    --row-alt: rgba(24, 31, 40, 0.022);
    --row-hover: rgba(24, 31, 40, 0.05);
    --row-selected: rgba(13, 122, 90, 0.10);
    --accent: #0d7a5a;
    --font-ui:
      'Avenir Next',
      'Segoe UI Variable',
      'IBM Plex Sans',
      'Inter',
      'Helvetica Neue',
      sans-serif;
    color-scheme: light;
  }

  /* ── Dark — "OLED" ───────────────────────────────────────────────────
     Near-black canvas with clear step-ups in surface and surface-2 so
     cards read as distinct layers rather than flat. Ink is neutral
     white, no green cast. */
  :global(html[data-theme='dark']) {
    --bg: #060a10;
    --fg: #e8ecf1;
    --surface: #11171e;
    --surface-2: #1c242d;
    --border: rgba(255, 255, 255, 0.09);
    --muted: rgba(232, 236, 241, 0.58);
    --row-alt: rgba(255, 255, 255, 0.022);
    --row-hover: rgba(255, 255, 255, 0.06);
    --row-selected: rgba(77, 218, 168, 0.15);
    --accent: #4ddaa8;
    color-scheme: dark;
  }
  @media (prefers-color-scheme: dark) {
    :global(html:not([data-theme])) {
      --bg: #060a10;
      --fg: #e8ecf1;
      --surface: #11171e;
      --surface-2: #1c242d;
      --border: rgba(255, 255, 255, 0.09);
      --muted: rgba(232, 236, 241, 0.58);
      --row-alt: rgba(255, 255, 255, 0.022);
      --row-hover: rgba(255, 255, 255, 0.06);
      --row-selected: rgba(77, 218, 168, 0.15);
      --accent: #4ddaa8;
      color-scheme: dark;
    }
  }

  /* ── Cyberpunk — "Tokyo Night" ───────────────────────────────────────
     Blue-violet night canvas with cyan/purple accents instead of the
     old cyan/hot-pink combo. Less shouty than classic cyberpunk but
     still distinctly themed and readable during long sessions. */
  :global(html[data-theme='cyberpunk']) {
    --bg: #0b0f23;
    --fg: #c0caf5;
    --surface: #141a30;
    --surface-2: #1e2743;
    --border: rgba(122, 162, 247, 0.16);
    --muted: rgba(192, 202, 245, 0.62);
    --row-alt: rgba(122, 162, 247, 0.035);
    --row-hover: rgba(122, 162, 247, 0.085);
    --row-selected: rgba(187, 154, 247, 0.18);
    --accent: #7aa2f7;
    --accent-2: #bb9af7;
    --accent-3: #e0af68;
    --neon-glow: 0 0 8px rgba(122, 162, 247, 0.35);
    --neon-glow-strong: 0 0 16px rgba(122, 162, 247, 0.5), 0 0 40px rgba(122, 162, 247, 0.15);
    --neon-pink-glow: 0 0 8px rgba(187, 154, 247, 0.4);
    color-scheme: dark;
  }

  /* Cyberpunk — global element overrides */
  :global(html[data-theme='cyberpunk'] *::selection) {
    background: rgba(187, 154, 247, 0.38);
    color: #fff;
  }
  :global(html[data-theme='cyberpunk'] a) {
    color: var(--accent);
  }
  :global(html[data-theme='cyberpunk'] input:focus),
  :global(html[data-theme='cyberpunk'] select:focus),
  :global(html[data-theme='cyberpunk'] textarea:focus) {
    outline: 1px solid var(--accent);
    outline-offset: 0;
    box-shadow: var(--neon-glow);
  }
  :global(html[data-theme='cyberpunk'] button:focus-visible) {
    outline: 1px solid var(--accent);
    box-shadow: var(--neon-glow);
  }
  /* Scrollbar — thin neon track */
  :global(html[data-theme='cyberpunk'] ::-webkit-scrollbar) {
    width: 6px;
    height: 6px;
  }
  :global(html[data-theme='cyberpunk'] ::-webkit-scrollbar-track) {
    background: #0a0d1a;
  }
  :global(html[data-theme='cyberpunk'] ::-webkit-scrollbar-thumb) {
    background: rgba(122, 162, 247, 0.3);
    border-radius: 3px;
  }
  :global(html[data-theme='cyberpunk'] ::-webkit-scrollbar-thumb:hover) {
    background: rgba(122, 162, 247, 0.55);
  }
  /* Cyberpunk borders get a faint cool glow */
  :global(html[data-theme='cyberpunk'] section),
  :global(html[data-theme='cyberpunk'] aside) {
    border-color: rgba(122, 162, 247, 0.12);
  }

  /* ── Solarized Light — Ethan Schoonover's balanced warm light ─────── */
  :global(html[data-theme='solarized-light']) {
    --bg: #fdf6e3;
    --fg: #073642;
    --surface: #fffcf0;
    --surface-2: #eee8d5;
    --border: rgba(7, 54, 66, 0.12);
    --muted: rgba(7, 54, 66, 0.58);
    --row-alt: rgba(7, 54, 66, 0.02);
    --row-hover: rgba(38, 139, 210, 0.08);
    --row-selected: rgba(38, 139, 210, 0.14);
    --accent: #268bd2;
    color-scheme: light;
  }

  /* ── Solarized Dark ───────────────────────────────────────────────── */
  :global(html[data-theme='solarized-dark']) {
    --bg: #002b36;
    --fg: #eee8d5;
    --surface: #073642;
    --surface-2: #0e4651;
    --border: rgba(238, 232, 213, 0.10);
    --muted: rgba(238, 232, 213, 0.58);
    --row-alt: rgba(238, 232, 213, 0.022);
    --row-hover: rgba(42, 161, 152, 0.12);
    --row-selected: rgba(42, 161, 152, 0.22);
    --accent: #2aa198;
    color-scheme: dark;
  }

  /* ── Dracula ──────────────────────────────────────────────────────── */
  :global(html[data-theme='dracula']) {
    --bg: #282a36;
    --fg: #f8f8f2;
    --surface: #343746;
    --surface-2: #44475a;
    --border: rgba(248, 248, 242, 0.10);
    --muted: rgba(248, 248, 242, 0.58);
    --row-alt: rgba(248, 248, 242, 0.022);
    --row-hover: rgba(189, 147, 249, 0.12);
    --row-selected: rgba(189, 147, 249, 0.22);
    --accent: #bd93f9;
    color-scheme: dark;
  }

  /* ── Nord — arctic, cool, quiet ───────────────────────────────────── */
  :global(html[data-theme='nord']) {
    --bg: #2e3440;
    --fg: #eceff4;
    --surface: #3b4252;
    --surface-2: #434c5e;
    --border: rgba(236, 239, 244, 0.10);
    --muted: rgba(236, 239, 244, 0.58);
    --row-alt: rgba(236, 239, 244, 0.022);
    --row-hover: rgba(136, 192, 208, 0.12);
    --row-selected: rgba(136, 192, 208, 0.22);
    --accent: #88c0d0;
    color-scheme: dark;
  }

  /* ── Gruvbox — retro warm ─────────────────────────────────────────── */
  :global(html[data-theme='gruvbox']) {
    --bg: #282828;
    --fg: #ebdbb2;
    --surface: #32302f;
    --surface-2: #3c3836;
    --border: rgba(235, 219, 178, 0.10);
    --muted: rgba(235, 219, 178, 0.58);
    --row-alt: rgba(235, 219, 178, 0.022);
    --row-hover: rgba(254, 128, 25, 0.12);
    --row-selected: rgba(254, 128, 25, 0.20);
    --accent: #fabd2f;
    color-scheme: dark;
  }

  /* ── Monokai — classic editor ─────────────────────────────────────── */
  :global(html[data-theme='monokai']) {
    --bg: #272822;
    --fg: #f8f8f2;
    --surface: #30312a;
    --surface-2: #3e3f33;
    --border: rgba(248, 248, 242, 0.10);
    --muted: rgba(248, 248, 242, 0.58);
    --row-alt: rgba(248, 248, 242, 0.022);
    --row-hover: rgba(166, 226, 46, 0.10);
    --row-selected: rgba(166, 226, 46, 0.20);
    --accent: #a6e22e;
    color-scheme: dark;
  }

  /* ── Sunset — warm peach light ────────────────────────────────────── */
  :global(html[data-theme='sunset']) {
    --bg: #fff3e6;
    --fg: #3a1f1a;
    --surface: #fffaf3;
    --surface-2: #ffe4cc;
    --border: rgba(58, 31, 26, 0.10);
    --muted: rgba(58, 31, 26, 0.58);
    --row-alt: rgba(58, 31, 26, 0.022);
    --row-hover: rgba(230, 92, 60, 0.08);
    --row-selected: rgba(230, 92, 60, 0.14);
    --accent: #e2633a;
    color-scheme: light;
  }

  /* ── Forest — deep green, earthy ──────────────────────────────────── */
  :global(html[data-theme='forest']) {
    --bg: #0f1a15;
    --fg: #d8e0d6;
    --surface: #16241d;
    --surface-2: #1f3027;
    --border: rgba(216, 224, 214, 0.10);
    --muted: rgba(216, 224, 214, 0.58);
    --row-alt: rgba(216, 224, 214, 0.022);
    --row-hover: rgba(127, 196, 120, 0.10);
    --row-selected: rgba(127, 196, 120, 0.18);
    --accent: #7fc478;
    color-scheme: dark;
  }

  /* ── Rosé Pine — dusky pink, muted ────────────────────────────────── */
  :global(html[data-theme='rose-pine']) {
    --bg: #191724;
    --fg: #e0def4;
    --surface: #1f1d2e;
    --surface-2: #26233a;
    --border: rgba(224, 222, 244, 0.10);
    --muted: rgba(224, 222, 244, 0.58);
    --row-alt: rgba(224, 222, 244, 0.022);
    --row-hover: rgba(235, 188, 186, 0.10);
    --row-selected: rgba(235, 188, 186, 0.18);
    --accent: #ebbcba;
    color-scheme: dark;
  }

  /* ── Sepia — aged paper, reading-focused ──────────────────────────── */
  :global(html[data-theme='sepia']) {
    --bg: #f4ecd8;
    --fg: #3b2f1e;
    --surface: #fbf5e6;
    --surface-2: #ede1c6;
    --border: rgba(59, 47, 30, 0.12);
    --muted: rgba(59, 47, 30, 0.58);
    --row-alt: rgba(59, 47, 30, 0.022);
    --row-hover: rgba(123, 87, 42, 0.08);
    --row-selected: rgba(123, 87, 42, 0.14);
    --accent: #7b572a;
    color-scheme: light;
  }

  /* ── Acid Rain — toxic neon storm ─────────────────────────────────── */
  :global(html[data-theme='acid-rain']) {
    --bg: #07110b;
    --fg: #efffe4;
    --surface: #102016;
    --surface-2: #193120;
    --border: rgba(208, 255, 72, 0.18);
    --muted: rgba(239, 255, 228, 0.62);
    --row-alt: rgba(208, 255, 72, 0.035);
    --row-hover: rgba(208, 255, 72, 0.11);
    --row-selected: rgba(181, 255, 57, 0.22);
    --accent: #d0ff48;
    --accent-2: #00f5d4;
    --accent-3: #ff2bd6;
    color-scheme: dark;
  }

  /* ── Synth Candy — bubblegum pop light ────────────────────────────── */
  :global(html[data-theme='synth-candy']) {
    --bg: #fff2fb;
    --fg: #311433;
    --surface: #ffffff;
    --surface-2: #ffe0f6;
    --border: rgba(146, 45, 137, 0.16);
    --muted: rgba(49, 20, 51, 0.60);
    --row-alt: rgba(0, 184, 217, 0.035);
    --row-hover: rgba(255, 69, 177, 0.10);
    --row-selected: rgba(0, 184, 217, 0.16);
    --accent: #f72585;
    --accent-2: #00b8d9;
    --accent-3: #ffbe0b;
    color-scheme: light;
  }

  /* ── Volcanic — lava glass dark ───────────────────────────────────── */
  :global(html[data-theme='volcanic']) {
    --bg: #120705;
    --fg: #ffe8d6;
    --surface: #21100c;
    --surface-2: #341811;
    --border: rgba(255, 106, 0, 0.17);
    --muted: rgba(255, 232, 214, 0.60);
    --row-alt: rgba(255, 106, 0, 0.035);
    --row-hover: rgba(255, 106, 0, 0.12);
    --row-selected: rgba(255, 55, 0, 0.22);
    --accent: #ff6a00;
    --accent-2: #ff2e00;
    --accent-3: #ffd166;
    color-scheme: dark;
  }

  /* ── Abyssal — deep sea glow ──────────────────────────────────────── */
  :global(html[data-theme='abyssal']) {
    --bg: #02131d;
    --fg: #defbff;
    --surface: #062435;
    --surface-2: #0a3148;
    --border: rgba(76, 201, 240, 0.17);
    --muted: rgba(222, 251, 255, 0.60);
    --row-alt: rgba(76, 201, 240, 0.03);
    --row-hover: rgba(76, 201, 240, 0.10);
    --row-selected: rgba(114, 9, 183, 0.24);
    --accent: #4cc9f0;
    --accent-2: #7209b7;
    --accent-3: #80ffdb;
    color-scheme: dark;
  }

  /* ── Arcade — electric cabinet dark ───────────────────────────────── */
  :global(html[data-theme='arcade']) {
    --bg: #080718;
    --fg: #f5f7ff;
    --surface: #11102b;
    --surface-2: #1d1a44;
    --border: rgba(255, 0, 110, 0.18);
    --muted: rgba(245, 247, 255, 0.60);
    --row-alt: rgba(58, 134, 255, 0.035);
    --row-hover: rgba(58, 134, 255, 0.11);
    --row-selected: rgba(255, 0, 110, 0.23);
    --accent: #3a86ff;
    --accent-2: #ff006e;
    --accent-3: #ffbe0b;
    color-scheme: dark;
  }

  /* Body-level ambient washes for the new themes — kept subtle so the
     surface cards still read as the dominant layer. */
  :global(html[data-theme='solarized-light'], html[data-theme='solarized-light'] body) {
    background:
      radial-gradient(circle at top left, rgba(38, 139, 210, 0.07), transparent 28%),
      radial-gradient(circle at 85% 14%, rgba(181, 137, 0, 0.04), transparent 26%),
      var(--bg);
  }
  :global(html[data-theme='solarized-dark'], html[data-theme='solarized-dark'] body) {
    background:
      radial-gradient(circle at top left, rgba(42, 161, 152, 0.06), transparent 28%),
      radial-gradient(circle at bottom right, rgba(38, 139, 210, 0.05), transparent 30%),
      var(--bg);
  }
  :global(html[data-theme='dracula'], html[data-theme='dracula'] body) {
    background:
      radial-gradient(circle at 15% 0%, rgba(189, 147, 249, 0.10), transparent 26%),
      radial-gradient(circle at 88% 14%, rgba(255, 121, 198, 0.07), transparent 22%),
      var(--bg);
  }
  :global(html[data-theme='nord'], html[data-theme='nord'] body) {
    background:
      radial-gradient(circle at top left, rgba(136, 192, 208, 0.08), transparent 28%),
      radial-gradient(circle at bottom right, rgba(94, 129, 172, 0.06), transparent 30%),
      var(--bg);
  }
  :global(html[data-theme='gruvbox'], html[data-theme='gruvbox'] body) {
    background:
      radial-gradient(circle at top left, rgba(250, 189, 47, 0.06), transparent 28%),
      radial-gradient(circle at bottom right, rgba(204, 36, 29, 0.04), transparent 30%),
      var(--bg);
  }
  :global(html[data-theme='monokai'], html[data-theme='monokai'] body) {
    background:
      radial-gradient(circle at top left, rgba(166, 226, 46, 0.06), transparent 28%),
      radial-gradient(circle at bottom right, rgba(249, 38, 114, 0.05), transparent 30%),
      var(--bg);
  }
  :global(html[data-theme='sunset'], html[data-theme='sunset'] body) {
    background:
      radial-gradient(circle at top left, rgba(230, 92, 60, 0.09), transparent 26%),
      radial-gradient(circle at 85% 14%, rgba(240, 172, 70, 0.08), transparent 24%),
      var(--bg);
  }
  :global(html[data-theme='forest'], html[data-theme='forest'] body) {
    background:
      radial-gradient(circle at top left, rgba(127, 196, 120, 0.07), transparent 28%),
      radial-gradient(circle at bottom right, rgba(48, 82, 58, 0.08), transparent 30%),
      var(--bg);
  }
  :global(html[data-theme='rose-pine'], html[data-theme='rose-pine'] body) {
    background:
      radial-gradient(circle at 15% 0%, rgba(235, 188, 186, 0.08), transparent 26%),
      radial-gradient(circle at 88% 14%, rgba(156, 207, 216, 0.06), transparent 24%),
      var(--bg);
  }
  :global(html[data-theme='sepia'], html[data-theme='sepia'] body) {
    background:
      radial-gradient(circle at top left, rgba(123, 87, 42, 0.05), transparent 28%),
      radial-gradient(circle at 85% 14%, rgba(183, 129, 73, 0.04), transparent 26%),
      var(--bg);
  }
  :global(html[data-theme='acid-rain'], html[data-theme='acid-rain'] body) {
    background:
      radial-gradient(circle at 12% 0%, rgba(208, 255, 72, 0.15), transparent 24%),
      radial-gradient(circle at 92% 12%, rgba(255, 43, 214, 0.10), transparent 22%),
      radial-gradient(circle at 42% 100%, rgba(0, 245, 212, 0.07), transparent 32%),
      var(--bg);
  }
  :global(html[data-theme='synth-candy'], html[data-theme='synth-candy'] body) {
    background:
      radial-gradient(circle at 10% 4%, rgba(247, 37, 133, 0.16), transparent 25%),
      radial-gradient(circle at 88% 12%, rgba(0, 184, 217, 0.14), transparent 24%),
      radial-gradient(circle at 50% 100%, rgba(255, 190, 11, 0.10), transparent 30%),
      var(--bg);
  }
  :global(html[data-theme='volcanic'], html[data-theme='volcanic'] body) {
    background:
      radial-gradient(circle at 14% 0%, rgba(255, 106, 0, 0.16), transparent 26%),
      radial-gradient(circle at 86% 16%, rgba(255, 46, 0, 0.11), transparent 24%),
      radial-gradient(circle at 52% 100%, rgba(255, 209, 102, 0.06), transparent 34%),
      var(--bg);
  }
  :global(html[data-theme='abyssal'], html[data-theme='abyssal'] body) {
    background:
      radial-gradient(circle at 12% 0%, rgba(76, 201, 240, 0.12), transparent 26%),
      radial-gradient(circle at 88% 16%, rgba(114, 9, 183, 0.12), transparent 24%),
      radial-gradient(circle at 50% 100%, rgba(128, 255, 219, 0.06), transparent 34%),
      var(--bg);
  }
  :global(html[data-theme='arcade'], html[data-theme='arcade'] body) {
    background:
      radial-gradient(circle at 14% 0%, rgba(58, 134, 255, 0.14), transparent 25%),
      radial-gradient(circle at 88% 12%, rgba(255, 0, 110, 0.13), transparent 23%),
      radial-gradient(circle at 50% 100%, rgba(255, 190, 11, 0.06), transparent 34%),
      var(--bg);
  }

  :global(html, body) {
    margin: 0;
    padding: 0;
    font-family: var(--font-ui);
    background:
      radial-gradient(circle at top left, color-mix(in oklab, var(--accent) 6%, transparent), transparent 28%),
      radial-gradient(circle at bottom right, rgba(64, 123, 255, 0.035), transparent 34%),
      var(--bg);
    color: var(--fg);
  }
  :global(html[data-theme='light'], html[data-theme='light'] body) {
    /* Very subtle emerald + cool-grey wash so the airy white canvas
       isn't flat — but light enough that the surface cards stay
       dominant. */
    background:
      radial-gradient(circle at top left, rgba(13, 122, 90, 0.055), transparent 28%),
      radial-gradient(circle at 85% 18%, rgba(80, 130, 200, 0.035), transparent 26%),
      var(--bg);
  }
  :global(html[data-theme='dark'], html[data-theme='dark'] body) {
    /* OLED-style base — keep washes faint so cards don't lose their
       layered feel. Pure black-ish canvas, minimal ambient tint. */
    background:
      radial-gradient(circle at top left, rgba(77, 218, 168, 0.05), transparent 28%),
      radial-gradient(circle at bottom right, rgba(90, 140, 255, 0.035), transparent 30%),
      var(--bg);
  }
  @media (prefers-color-scheme: dark) {
    :global(html:not([data-theme]), html:not([data-theme]) body) {
      background:
        radial-gradient(circle at top left, rgba(77, 218, 168, 0.05), transparent 28%),
        radial-gradient(circle at bottom right, rgba(90, 140, 255, 0.035), transparent 30%),
        var(--bg);
    }
  }
  :global(html[data-theme='cyberpunk'], html[data-theme='cyberpunk'] body) {
    /* Tokyo Night: blue-violet gradients, no hot pink. The purple pool
       in the top-right gives that late-night "synthwave but calmer"
       feel without the neon-sign intensity of the old cyberpunk. */
    background:
      radial-gradient(circle at 15% 0%, rgba(122, 162, 247, 0.12), transparent 24%),
      radial-gradient(circle at 88% 14%, rgba(187, 154, 247, 0.09), transparent 22%),
      radial-gradient(circle at 50% 100%, rgba(122, 162, 247, 0.04), transparent 35%),
      var(--bg);
  }

  /* Document-style pages live inside this wrapper. We deliberately
     do NOT center horizontally here (no `place-items: center`) — that
     was collapsing every page to its intrinsic content width on
     widescreen, making `max-width` look like a hard letterbox. Pages
     center themselves with `margin: 0 auto` and use clamp() max-widths
     so they grow with the viewport instead of pinning at a fixed cap. */
  main.centered {
    min-height: 100dvh;
    padding: 1.25rem 1.5rem 2rem;
    box-sizing: border-box;
  }

  /* Popup window for the Notes companion. Fills the popup edge to
     edge — no centered padding, no big bottom-of-page spacer. */
  main.popup {
    min-height: 100dvh;
    padding: 0;
    box-sizing: border-box;
  }

  @media (max-width: 900px) {
    main.centered {
      min-height: 100dvh;
      padding: 0;
    }
  }
</style>
