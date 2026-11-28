<script lang="ts">
  import { lightbox } from '$lib/lightbox.svelte';
  import Icon from './Icon.svelte';

  let dialog = $state<HTMLDivElement | null>(null);

  // Lock body scroll while open; close on Escape.
  $effect(() => {
    if (lightbox.src) {
      const prev = document.body.style.overflow;
      document.body.style.overflow = 'hidden';
      dialog?.focus();
      const onKey = (e: KeyboardEvent) => {
        if (e.key === 'Escape') lightbox.close();
      };
      window.addEventListener('keydown', onKey);
      return () => {
        document.body.style.overflow = prev;
        window.removeEventListener('keydown', onKey);
      };
    }
  });
</script>

{#if lightbox.src}
  <div
    class="backdrop"
    role="dialog"
    aria-modal="true"
    aria-label={lightbox.alt || 'Full-size screenshot'}
    tabindex="-1"
    bind:this={dialog}
  >
    <!-- full-bleed close target behind the image -->
    <button class="cover" aria-label="Close full-size view" onclick={() => lightbox.close()}></button>
    <button class="close" aria-label="Close" onclick={() => lightbox.close()}>
      <Icon name="cross" size={22} />
    </button>
    <figure class="content">
      <img src={lightbox.src} alt={lightbox.alt} decoding="async" />
      {#if lightbox.alt}<figcaption>{lightbox.alt}</figcaption>{/if}
    </figure>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 200;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 1rem;
    padding: clamp(1rem, 2vw, 3rem);
    background: color-mix(in oklab, #04070d 86%, transparent);
    backdrop-filter: blur(10px);
    animation: fade var(--dur) var(--ease);
  }
  @keyframes fade {
    from {
      opacity: 0;
    }
  }
  .cover {
    position: absolute;
    inset: 0;
    z-index: 0;
    border: 0;
    background: none;
    cursor: zoom-out;
  }
  .content {
    position: relative;
    z-index: 1;
    margin: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 1rem;
    pointer-events: none;
  }
  img {
    max-width: min(1200px, 96vw);
    max-height: 84vh;
    width: auto;
    border-radius: var(--r-md);
    border: 1px solid var(--border-strong);
    box-shadow: var(--shadow-float);
    animation: pop var(--dur) var(--ease);
  }
  @keyframes pop {
    from {
      transform: scale(0.97);
      opacity: 0;
    }
  }
  figcaption {
    color: var(--text-muted);
    font-size: var(--text-sm);
    font-family: var(--font-mono);
    max-width: 60ch;
    text-align: center;
  }
  .close {
    position: absolute;
    z-index: 2;
    top: clamp(0.8rem, 2vw, 1.6rem);
    right: clamp(0.8rem, 2vw, 1.6rem);
    display: grid;
    place-items: center;
    width: 44px;
    height: 44px;
    border-radius: var(--r-pill);
    border: 1px solid var(--border-strong);
    background: var(--surface-2);
    color: var(--text);
    cursor: pointer;
    transition: border-color var(--dur-fast) var(--ease);
  }
  .close:hover {
    border-color: var(--accent);
  }
  @media (prefers-reduced-motion: reduce) {
    .backdrop,
    img {
      animation: none;
    }
  }
</style>
