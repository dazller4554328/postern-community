<script lang="ts">
  // Curated view (Phase 1) — engagement-ranked list of messages.
  // Backed by /api/curated/messages, which scores each row by
  // (recency × sender-engagement). Renders with the same row
  // template the inbox uses so it looks like a familiar mail list,
  // just sorted differently.
  //
  // No rules / no AI yet — that's Phase 2. Phase 1 ships purely
  // off the behavioural signal of "did you reply / send to this
  // person", which is enough to surface the mail you actually
  // care about.

  import { goto } from '$app/navigation';
  import { onMount } from 'svelte';
  import { api, type CuratedListItem } from '$lib/api';
  import { formatRelative, formatSender } from '$lib/format';
  import SenderAvatar from '$lib/components/SenderAvatar.svelte';

  let items = $state<CuratedListItem[]>([]);
  let loading = $state(true);
  let err = $state<string | null>(null);

  onMount(async () => {
    try {
      items = await api.listCurated({ limit: 80 });
    } catch (e) {
      err = e instanceof Error ? e.message : String(e);
    } finally {
      loading = false;
    }
  });

  function open(id: number) {
    // Curated jumps into the inbox layout's reader — same UX as
    // clicking any other message. Keeps "back" semantics consistent.
    goto(`/inbox?m=${id}`);
  }
</script>

<svelte:head>
  <title>Curated · Postern</title>
</svelte:head>

<div class="shell">
  <div class="page-top">
    <a class="back" href="/inbox">← Inbox</a>
  </div>

  <header class="hero">
    <div class="hero-copy">
      <span class="eyebrow">Curated view</span>
      <h1>Likely worth reading</h1>
      <p>
        Mail from people you actually correspond with, ranked above
        the noise. Built from your reply / send history — no LLM
        involved yet. Rules and AI-driven categories arrive in a
        follow-up.
      </p>
    </div>
  </header>

  {#if loading}
    <div class="state">Loading curated mail…</div>
  {:else if err}
    <div class="state error">Couldn't load: {err}</div>
  {:else if items.length === 0}
    <div class="state empty">
      Nothing curated yet — send or reply to a few addresses and the
      list will start filling in. The first unlock also seeds from
      your existing sent mail in the background.
    </div>
  {:else}
    <ul class="list">
      {#each items as m (m.id)}
        <li>
          <button class="row" class:unread={!m.is_read} onclick={() => open(m.id)}>
            <SenderAvatar email={m.from_addr} size={32} fetchRemote={true} />
            <div class="body">
              <div class="line1">
                <span class="sender">{formatSender(m.from_addr)}</span>
                <span class="time">{formatRelative(m.date_utc)}</span>
              </div>
              <div class="line2">
                <span class="subject">{m.subject || '(no subject)'}</span>
              </div>
              <div class="line3">
                <span class="snippet">{m.snippet || ''}</span>
                <span class="score" title="Composite score: recency × engagement">
                  {(m.curated_score * 100).toFixed(0)}
                </span>
              </div>
            </div>
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  /* Match the known-good notes-shell pattern: min-height fills the
     viewport so the list pane gets actual room, gap stays consistent,
     padding obeys the layout's centered-main wrapper. */
  .shell {
    width: 100%;
    max-width: clamp(56rem, 92vw, 92rem);
    margin: 0 auto;
    padding: 1.25rem 1.5rem 2rem;
    box-sizing: border-box;
    display: flex;
    flex-direction: column;
    min-height: 100dvh;
    gap: 1rem;
  }
  /* Mobile: the layout's <main class:centered> drops its padding
     under 900px (handled by main.centered's media query), and we
     live inside MobileShell's `.pass-through` which scrolls. So
     drop our own outer padding too — otherwise the content sits
     inside two padding layers. */
  @media (max-width: 900px) {
    .shell {
      padding: 0.75rem 0.85rem 1.5rem;
      min-height: auto;
    }
  }
  .page-top { margin-bottom: -0.25rem; }
  .back {
    color: var(--muted);
    text-decoration: none;
    font-size: 0.85rem;
  }
  .back:hover { color: var(--fg); }

  .hero {
    background: color-mix(in oklab, var(--surface) 96%, transparent);
    border: 1px solid var(--border);
    border-radius: 1rem;
    padding: 1.4rem 1.5rem;
  }
  .hero-copy { max-width: 60ch; }
  .eyebrow {
    font-size: 0.72rem;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--accent);
    font-weight: 600;
  }
  .hero h1 {
    font-size: 1.6rem;
    margin: 0.25rem 0 0.4rem;
  }
  .hero p {
    color: var(--muted);
    font-size: 0.92rem;
    line-height: 1.5;
    margin: 0;
  }

  .state {
    padding: 2rem 1rem;
    color: var(--muted);
    text-align: center;
    font-size: 0.95rem;
  }
  .state.error { color: #d6483c; }

  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: 1rem;
    overflow: hidden;
    background: var(--surface);
  }
  .list li + li { border-top: 1px solid var(--border); }

  .row {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 0.85rem;
    align-items: center;
    width: 100%;
    padding: 0.7rem 1rem;
    background: transparent;
    border: 0;
    color: inherit;
    text-align: left;
    cursor: pointer;
    font: inherit;
  }
  .row:hover { background: var(--row-hover); }
  .row.unread { font-weight: 500; }

  .body { min-width: 0; display: flex; flex-direction: column; gap: 2px; }
  .line1 {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 0.5rem;
  }
  .sender {
    font-size: 0.92rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1 1 auto;
  }
  .time {
    font-size: 0.76rem;
    color: var(--muted);
    flex: 0 0 auto;
  }
  .line2 .subject {
    font-size: 0.88rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    display: block;
  }
  .line3 {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
  }
  .snippet {
    flex: 1 1 auto;
    min-width: 0;
    color: var(--muted);
    font-size: 0.82rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .score {
    flex: 0 0 auto;
    font-variant-numeric: tabular-nums;
    font-size: 0.7rem;
    color: var(--muted);
    background: color-mix(in oklab, var(--accent) 10%, transparent);
    padding: 0.1rem 0.45rem;
    border-radius: 0.5rem;
  }
</style>
