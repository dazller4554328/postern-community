<script lang="ts">
  import { page } from '$app/stores';
  import { onMount } from 'svelte';
  // Importing the module triggers its subscriber and ensures the theme/font
  // tokens get reapplied on any pref change from the settings page.
  import '$lib/prefs';
  import '$lib/themes.css';
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
  <link rel="icon" type="image/svg+xml" href="/favicon.svg?v=2" />
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
  /* Theme tokens + background washes live in $lib/themes.css. */

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
