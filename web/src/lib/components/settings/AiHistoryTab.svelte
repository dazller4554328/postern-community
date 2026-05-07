<script lang="ts">
  import { onMount } from 'svelte';
  import { api, type AiHistoryEntry } from '$lib/api';

  const PAGE_SIZE = 25;

  let rows = $state<AiHistoryEntry[]>([]);
  let page = $state(0);
  let loading = $state(true);
  let err = $state<string | null>(null);
  let hasNext = $state(false);

  async function load() {
    loading = true;
    err = null;
    try {
      const list = await api.aiHistory({
        limit: PAGE_SIZE + 1,
        offset: page * PAGE_SIZE
      });
      rows = list.slice(0, PAGE_SIZE);
      hasNext = list.length > PAGE_SIZE;
    } catch (e) {
      err = e instanceof Error ? e.message : String(e);
    } finally {
      loading = false;
    }
  }

  function fmtTs(unix: number): string {
    return new Date(unix * 1000).toLocaleString();
  }

  function shortAnswer(answer: string): string {
    const compact = answer.replace(/\s+/g, ' ').trim();
    return compact.length > 240 ? `${compact.slice(0, 240)}...` : compact;
  }

  function prevPage() {
    if (page === 0 || loading) return;
    page -= 1;
    void load();
  }

  function nextPage() {
    if (!hasNext || loading) return;
    page += 1;
    void load();
  }

  onMount(load);
</script>

<section class="history">
  <div class="history-head">
    <div>
      <h3>Chat history</h3>
      <p class="muted">Stored Datas question-and-answer rounds, newest first.</p>
    </div>
    <div class="pager" aria-label="History pages">
      <button type="button" disabled={page === 0 || loading} onclick={prevPage}>Previous</button>
      <span>Page {page + 1}</span>
      <button type="button" disabled={!hasNext || loading} onclick={nextPage}>Next</button>
    </div>
  </div>

  {#if err}
    <p class="status-line err">⚠ {err}</p>
  {/if}

  {#if loading}
    <p class="muted">Loading...</p>
  {:else if rows.length === 0}
    <p class="muted">No stored conversations on this page.</p>
  {:else}
    <div class="history-list">
      {#each rows as r (r.id)}
        <article class="history-row">
          <header>
            <time title={fmtTs(r.created_at)}>{fmtTs(r.created_at)}</time>
            <span class="pill">{r.provider}</span>
            <code>{r.chat_model}</code>
          </header>
          <h4>{r.question}</h4>
          <p>{shortAnswer(r.answer)}</p>
          {#if r.cited_message_ids.length > 0}
            <footer>{r.cited_message_ids.length} source{r.cited_message_ids.length === 1 ? '' : 's'}</footer>
          {/if}
        </article>
      {/each}
    </div>
  {/if}
</section>

<style>
  .history {
    display: flex;
    flex-direction: column;
    gap: 0.85rem;
    min-width: 0;
  }
  .history-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 1rem;
    flex-wrap: wrap;
  }
  .history-head h3 {
    margin: 0 0 0.15rem;
    font-size: 0.95rem;
  }
  .muted {
    color: var(--muted);
    margin: 0;
  }
  .pager {
    display: inline-flex;
    align-items: center;
    gap: 0.45rem;
    font-size: 0.8rem;
  }
  .pager button {
    font: inherit;
    padding: 0.35rem 0.7rem;
    border: 1px solid color-mix(in oklab, currentColor 14%, transparent);
    border-radius: 999px;
    background: transparent;
    color: inherit;
    cursor: pointer;
  }
  .pager button:hover:not(:disabled) {
    background: color-mix(in oklab, currentColor 6%, transparent);
  }
  .pager button:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }
  .history-list {
    display: grid;
    gap: 0.65rem;
  }
  .history-row {
    min-width: 0;
    padding: 0.75rem 0.85rem;
    border: 1px solid var(--border);
    border-radius: 0.8rem;
    background: color-mix(in oklab, var(--surface-2) 42%, transparent);
  }
  .history-row header {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    min-width: 0;
    color: var(--muted);
    font-size: 0.75rem;
    flex-wrap: wrap;
  }
  .history-row h4 {
    margin: 0.55rem 0 0.35rem;
    font-size: 0.9rem;
    line-height: 1.35;
  }
  .history-row p {
    margin: 0;
    color: var(--muted);
    font-size: 0.84rem;
    line-height: 1.45;
    overflow-wrap: anywhere;
  }
  .history-row footer {
    margin-top: 0.55rem;
    color: var(--muted);
    font-size: 0.75rem;
  }
  .pill,
  code {
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .pill {
    padding: 0.08rem 0.45rem;
    border-radius: 999px;
    background: color-mix(in oklab, var(--accent) 14%, transparent);
    color: color-mix(in oklab, var(--accent) 75%, var(--fg));
    font-weight: 700;
  }
  code {
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 0.74rem;
  }
  .status-line.err {
    color: #c83333;
    font-size: 0.86rem;
  }
</style>
