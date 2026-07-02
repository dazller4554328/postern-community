<script lang="ts">
  import { lightbox } from '$lib/lightbox.svelte';
  import Icon from './Icon.svelte';

  interface Props {
    src: string;
    alt: string;
    radius?: string;
  }
  let { src, alt, radius = 'var(--r-md)' }: Props = $props();

  const sourceSet = (format: 'avif' | 'webp') => {
    const base = src.replace(/\.(png|jpe?g)$/i, '');
    if (src.includes('/mobile/')) {
      return `${base}-720.${format} 720w, ${base}.${format} 864w`;
    }
    return `${base}-720.${format} 720w, ${base}-1200.${format} 1200w, ${base}.${format} 1920w`;
  };
</script>

<button
  class="zoomimg"
  style="--radius:{radius}"
  onclick={() => lightbox.open(src, alt)}
  aria-label={`View full size: ${alt}`}
>
  <picture>
    <source type="image/avif" srcset={sourceSet('avif')} sizes="(max-width: 960px) 100vw, 33vw" />
    <source type="image/webp" srcset={sourceSet('webp')} sizes="(max-width: 960px) 100vw, 33vw" />
    <img {src} {alt} loading="lazy" decoding="async" />
  </picture>
  <span class="badge"><Icon name="expand" size={15} /></span>
</button>

<style>
  .zoomimg {
    display: block;
    position: relative;
    width: 100%;
    height: 100%;
    padding: 0;
    border: 0;
    background: none;
    cursor: zoom-in;
    border-radius: var(--radius);
    overflow: hidden;
  }
  picture,
  img {
    display: block;
    width: 100%;
    height: 100%;
  }
  picture {
    overflow: hidden;
    border-radius: var(--radius);
  }
  img {
    object-fit: cover;
    object-position: top center;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    transition: transform var(--dur) var(--ease);
  }
  .zoomimg:hover img {
    transform: scale(1.02);
  }
  .badge {
    position: absolute;
    top: 0.6rem;
    right: 0.6rem;
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    border-radius: var(--r-pill);
    background: color-mix(in oklab, #04070d 65%, transparent);
    border: 1px solid var(--border-strong);
    color: #fff;
    opacity: 0;
    transform: translateY(-4px);
    transition:
      opacity var(--dur) var(--ease),
      transform var(--dur) var(--ease);
    backdrop-filter: blur(6px);
  }
  .zoomimg:hover .badge,
  .zoomimg:focus-visible .badge {
    opacity: 1;
    transform: none;
  }
</style>
