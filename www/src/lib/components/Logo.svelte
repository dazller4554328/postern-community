<script lang="ts">
  interface Props {
    size?: number; // rendered height in px
    wordmark?: boolean; // full wordmark (true) or just the gate mark (false)
    title?: string;
  }
  let { size = 30, wordmark = true, title = 'Postern' }: Props = $props();

  // Intrinsic ratios of the standardised brand assets (shared with WHMCS):
  //   wordmark 560x130, mark 130x112. Set aspect-ratio so the browser
  //   reserves width and avoids layout shift on load.
  let ratio = $derived(wordmark ? '560 / 130' : '130 / 112');
  let src = $derived(wordmark ? '/img/postern-wordmark.png' : '/img/postern-mark.png');
</script>

<img class="logo" {src} alt={title} height={size} style="height: {size}px; aspect-ratio: {ratio}" />

<style>
  .logo {
    display: block;
    width: auto;
    flex: none;
  }
</style>
