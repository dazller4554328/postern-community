<script lang="ts">
  interface Props {
    logoSrc: string;
    compact: boolean;
    onSearch: () => void;
    onRefresh: () => void;
    onLock: () => void;
  }
  let { logoSrc, compact, onSearch, onRefresh, onLock }: Props = $props();
</script>

<header class="brand-bar" class:compact>
  <a href="/inbox" class="brand-link" aria-label="Postern home">
    <span class="brand-dot" aria-hidden="true"></span>
    <img src={logoSrc} alt="Postern" class="brand-logo" />
  </a>
  <div class="brand-actions">
    <button class="icon-btn" onclick={onSearch} aria-label="search">
      <svg width="22" height="22" viewBox="0 0 24 24" aria-hidden="true">
        <circle cx="10.5" cy="10.5" r="6.5" fill="none" stroke="currentColor" stroke-width="2.2" />
        <path d="m15.5 15.5 5 5" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" />
      </svg>
    </button>
    <button class="icon-btn" onclick={onRefresh} aria-label="refresh">
      <svg width="22" height="22" viewBox="0 0 24 24" aria-hidden="true">
        <path d="M20 12a8 8 0 1 1-2.34-5.66M20 4v5h-5" stroke="currentColor" stroke-width="2.4" fill="none" stroke-linecap="round" stroke-linejoin="round" />
      </svg>
    </button>
    <button class="icon-btn" onclick={onLock} aria-label="lock vault" title="Lock vault">
      <svg width="22" height="22" viewBox="0 0 24 24" aria-hidden="true">
        <rect x="5" y="11" width="14" height="9" rx="1.6" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linejoin="round" />
        <path d="M8 11V7.5a4 4 0 0 1 8 0V11" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" />
      </svg>
    </button>
  </div>
</header>

<style>
  .brand-bar {
    flex: 0 0 auto;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    min-height: calc(3.5rem + env(safe-area-inset-top));
    max-height: calc(3.5rem + env(safe-area-inset-top));
    padding: 0.48rem 0.7rem 0.5rem;
    padding-top: max(0.48rem, env(safe-area-inset-top));
    background: var(--surface);
    position: relative;
    z-index: 41;
    overflow: hidden;
    transition:
      max-height 180ms ease,
      min-height 180ms ease,
      padding 180ms ease,
      opacity 140ms ease,
      transform 180ms ease;
    will-change: max-height, transform, opacity;
  }
  .brand-bar.compact {
    min-height: 0;
    max-height: 0;
    padding-top: 0;
    padding-bottom: 0;
    opacity: 0;
    transform: translateY(-10px);
    pointer-events: none;
  }
  .brand-link {
    display: inline-flex;
    align-items: center;
    gap: 0.55rem;
    min-width: 0;
    text-decoration: none;
    color: inherit;
    padding: 0.12rem 0.28rem 0.12rem 0.1rem;
    border-radius: 0.8rem;
  }
  .brand-link:active { background: var(--row-hover); }
  .brand-dot {
    width: 0.92rem;
    height: 0.92rem;
    border-radius: 0.26rem;
    background:
      linear-gradient(135deg, var(--accent), color-mix(in oklab, var(--accent) 40%, white 60%));
    box-shadow:
      0 0 0 1px color-mix(in oklab, var(--accent) 18%, transparent),
      0 8px 18px color-mix(in oklab, var(--accent) 18%, transparent);
    flex: 0 0 auto;
  }
  .brand-logo {
    display: block;
    height: 29px;
    width: auto;
    max-width: min(45vw, 168px);
    object-fit: contain;
    object-position: left center;
    min-width: 0;
  }
  .brand-actions {
    display: inline-flex;
    align-items: center;
    gap: 0.1rem;
    flex-shrink: 0;
  }
  .icon-btn {
    background: transparent;
    border: 0;
    color: var(--fg);
    width: 46px;
    height: 46px;
    padding: 0;
    border-radius: 999px;
    cursor: pointer;
    touch-action: manipulation;
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }
  .icon-btn svg {
    width: 24px;
    height: 24px;
  }
  .icon-btn:hover,
  .icon-btn:active { background: var(--row-hover); }

  @media (max-width: 380px) {
    .brand-bar {
      gap: 0.25rem;
      padding-left: 0.55rem;
      padding-right: 0.5rem;
    }
    .brand-logo {
      height: 27px;
      max-width: 130px;
    }
    .icon-btn {
      width: 43px;
      height: 43px;
    }
  }
</style>
