<script lang="ts">
  interface TrackerBlocked {
    host: string;
    service: string;
  }

  interface Props {
    trackers: TrackerBlocked[];
    hasRemoteContent: boolean;
    remoteHosts: string[];
    allowRemote: boolean;
  }
  let {
    trackers,
    hasRemoteContent,
    remoteHosts,
    allowRemote = $bindable(),
  }: Props = $props();

  /// "Mailchimp, SendGrid (+2 more)" — caps the rendered services so
  /// long lists stay readable. Deduplicated so a sender using three
  /// Mailchimp pixels reads as one service.
  function summarize(t: TrackerBlocked[]): string {
    const unique = Array.from(new Set(t.map((x) => x.service)));
    if (unique.length === 0) return '';
    if (unique.length <= 2) return unique.join(', ');
    return `${unique.slice(0, 2).join(', ')} (+${unique.length - 2} more)`;
  }
</script>

{#if trackers.length > 0}
  <div class="banner tracker" title={trackers.map((t) => `${t.service} — ${t.host}`).join('\n')}>
    <span>
      🛡 Blocked <strong>{trackers.length}</strong>
      tracker{trackers.length === 1 ? '' : 's'}
      {#if summarize(trackers)}
        — <em>{summarize(trackers)}</em>
      {/if}
    </span>
  </div>
{/if}
{#if hasRemoteContent}
  <div class="banner" class:active={allowRemote}>
    {#if !allowRemote}
      <span>🚫 Blocks remote content from
        <strong>{remoteHosts.slice(0, 3).join(', ')}{remoteHosts.length > 3 ? '…' : ''}</strong>
      </span>
      <button onclick={() => (allowRemote = true)}>Show</button>
    {:else}
      <span>👁 Loaded via proxy</span>
      <button onclick={() => (allowRemote = false)}>Block</button>
    {/if}
  </div>
{/if}

<style>
  .banner {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    padding: 0.5rem 0.75rem;
    margin-bottom: 0.75rem;
    border: 1px solid var(--border);
    border-radius: 0.8rem;
    font-size: 0.8rem;
    background: color-mix(in oklab, orange 10%, transparent);
  }
  .banner.active { background: color-mix(in oklab, dodgerblue 10%, transparent); }
  .banner.tracker { background: color-mix(in oklab, forestgreen 10%, transparent); }
  .banner button {
    margin-left: auto;
    font: inherit;
    font-size: 0.78rem;
    padding: 0.3rem 0.7rem;
    border: 1px solid var(--border);
    background: var(--surface);
    color: inherit;
    border-radius: 999px;
    cursor: pointer;
  }
  .banner button:hover {
    background: color-mix(in oklab, currentColor 8%, transparent);
  }

  @media (max-width: 900px) {
    .banner {
      align-items: flex-start;
      flex-wrap: wrap;
      gap: 0.5rem;
    }
    .banner button {
      margin-left: 0;
      min-height: 40px;
    }
  }
</style>
