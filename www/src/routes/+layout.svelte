<script lang="ts">
  import '$lib/styles/global.css';
  import Nav from '$lib/components/Nav.svelte';
  import Footer from '$lib/components/Footer.svelte';
  import Lightbox from '$lib/components/Lightbox.svelte';

  let { children } = $props();
</script>

<div class="atmosphere" aria-hidden="true">
  <div class="grid"></div>
  <div class="glow glow--teal"></div>
  <div class="glow glow--orange"></div>
</div>

<a href="#main" class="skip">Skip to content</a>
<Nav />
<main id="main">
  {@render children()}
</main>
<Footer />
<Lightbox />

<style>
  .atmosphere {
    position: fixed;
    inset: 0;
    z-index: -1;
    overflow: hidden;
    pointer-events: none;
  }
  .grid {
    position: absolute;
    inset: -2px;
    background-image:
      linear-gradient(var(--bg-grid) 1px, transparent 1px),
      linear-gradient(90deg, var(--bg-grid) 1px, transparent 1px);
    background-size: 54px 54px;
    mask-image: radial-gradient(circle at 50% 0%, black, transparent 72%);
  }
  .glow {
    position: absolute;
    border-radius: 50%;
    filter: blur(90px);
    opacity: 0.8;
  }
  .glow--teal {
    top: -180px;
    left: -120px;
    width: 620px;
    height: 620px;
    background: radial-gradient(circle, var(--glow-teal), transparent 65%);
  }
  .glow--orange {
    top: 220px;
    right: -160px;
    width: 520px;
    height: 520px;
    background: radial-gradient(circle, var(--glow-orange), transparent 65%);
  }
  .skip {
    position: absolute;
    left: -9999px;
    top: 0;
    z-index: 100;
    padding: 0.7rem 1rem;
    background: var(--accent);
    color: var(--accent-ink);
    border-radius: var(--r-sm);
  }
  .skip:focus {
    left: 1rem;
    top: 1rem;
  }
  main {
    min-height: 50vh;
  }
</style>
