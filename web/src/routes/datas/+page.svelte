<script lang="ts">
  // Standalone Datas page. Mobile primarily — the bottom-nav 'Datas'
  // tab routes here. Desktop reaches Datas via the inbox sidebar
  // (the existing path), but this URL also works there if someone
  // bookmarks it. Renders AskBox in `fullscreen` mode so the panel
  // fills the viewport instead of sharing a row with an inbox grid
  // that doesn't exist on this route.

  import { goto } from '$app/navigation';
  import AskBox from '$lib/components/inbox/AskBox.svelte';

  function back() {
    goto('/inbox');
  }
</script>

<div class="datas-page">
  <header class="page-bar">
    <button class="icon-btn" onclick={back} aria-label="back to inbox">
      <svg viewBox="0 0 24 24" width="22" height="22" aria-hidden="true">
        <path d="M15 5 8 12l7 7" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round" />
      </svg>
    </button>
    <h1>Datas</h1>
    <span class="spacer" aria-hidden="true"></span>
  </header>
  <div class="askbox-wrap">
    <AskBox accountId={null} onClose={back} fullscreen={true} />
  </div>
</div>

<style>
  .datas-page {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    width: 100%;
  }
  .page-bar {
    flex: 0 0 auto;
    display: flex;
    align-items: center;
    gap: 0.4rem;
    padding: 0.4rem 0.55rem;
    padding-top: max(0.35rem, env(safe-area-inset-top));
    background: var(--surface);
    border-bottom: 1px solid var(--border);
  }
  .page-bar h1 {
    flex: 1 1 auto;
    margin: 0;
    text-align: center;
    font-size: 1rem;
    font-weight: 700;
    letter-spacing: -0.01em;
  }
  .icon-btn {
    background: transparent;
    border: 0;
    color: var(--fg);
    width: 42px;
    height: 42px;
    padding: 0;
    border-radius: 999px;
    cursor: pointer;
    touch-action: manipulation;
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }
  .icon-btn:active { background: var(--row-hover); }
  .spacer {
    width: 42px;
    flex: 0 0 42px;
  }
  .askbox-wrap {
    flex: 1 1 auto;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
</style>
