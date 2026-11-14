<script lang="ts">
  import { lightbox } from '$lib/lightbox.svelte';
  import Icon from './Icon.svelte';

  interface Props {
    src: string;
    alt: string;
    label?: string;
    loading?: 'eager' | 'lazy';
    fetchpriority?: 'high' | 'auto';
    zoomable?: boolean;
  }
  let {
    src,
    alt,
    label = 'postern',
    loading = 'lazy',
    fetchpriority = 'auto',
    zoomable = true
  }: Props = $props();

  const sourceSet = (format: 'avif' | 'webp') => {
    const base = src.replace(/\.(png|jpe?g)$/i, '');
    if (src.includes('/mobile/')) {
      return `${base}-720.${format} 720w, ${base}.${format} 864w`;
    }
    return `${base}-720.${format} 720w, ${base}-1200.${format} 1200w, ${base}.${format} 1920w`;
  };
</script>

<figure class="frame">
  <div class="bar">
    <span class="dots"><i></i><i></i><i></i></span>
    <span class="addr">{label}</span>
    <span class="spacer"></span>
  </div>
  {#if zoomable}
    <button class="shot" onclick={() => lightbox.open(src, alt)} aria-label={`View full size: ${alt}`}>
      <picture>
        <source type="image/avif" srcset={sourceSet('avif')} sizes="(max-width: 960px) 100vw, 55vw" />
        <source type="image/webp" srcset={sourceSet('webp')} sizes="(max-width: 960px) 100vw, 55vw" />
        <img {src} {alt} {loading} {fetchpriority} decoding="async" />
      </picture>
      <span class="zoom"><Icon name="expand" size={16} /> View full size</span>
    </button>
  {:else}
    <picture>
      <source type="image/avif" srcset={sourceSet('avif')} sizes="(max-width: 960px) 100vw, 55vw" />
      <source type="image/webp" srcset={sourceSet('webp')} sizes="(max-width: 960px) 100vw, 55vw" />
      <img {src} {alt} {loading} {fetchpriority} decoding="async" />
    </picture>
  {/if}
</figure>

<style>
  .frame {
    margin: 0;
    border-radius: var(--r-lg);
    overflow: hidden;
    border: 1px solid var(--border-strong);
    background: var(--surface-solid);
    box-shadow: var(--shadow-float);
  }
  .bar {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    padding: 0.6rem 0.9rem;
    background: color-mix(in oklab, var(--bg-2) 80%, var(--bg));
    border-bottom: 1px solid var(--border);
  }
  .dots {
    display: inline-flex;
    gap: 6px;
  }
  .dots i {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: var(--border-strong);
  }
  .dots i:first-child {
    background: var(--brand-orange);
    opacity: 0.85;
  }
  .dots i:nth-child(2) {
    background: var(--accent);
    opacity: 0.7;
  }
  .addr {
    flex: 1;
    text-align: center;
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    color: var(--text-faint);
    letter-spacing: 0.04em;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .spacer {
    width: 42px;
  }
  picture,
  img {
    display: block;
    width: 100%;
  }
  .shot {
    display: block;
    position: relative;
    width: 100%;
    padding: 0;
    border: 0;
    background: none;
    cursor: zoom-in;
    color: var(--text);
  }
  .zoom {
    position: absolute;
    right: 0.8rem;
    bottom: 0.8rem;
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    font-family: var(--font-display);
    font-size: var(--text-xs);
    font-weight: 600;
    padding: 0.45rem 0.75rem;
    border-radius: var(--r-pill);
    background: color-mix(in oklab, #04070d 70%, transparent);
    border: 1px solid var(--border-strong);
    color: #fff;
    opacity: 0;
    transform: translateY(6px);
    transition:
      opacity var(--dur) var(--ease),
      transform var(--dur) var(--ease);
    pointer-events: none;
    backdrop-filter: blur(6px);
  }
  .shot:hover .zoom,
  .shot:focus-visible .zoom {
    opacity: 1;
    transform: none;
  }
</style>
