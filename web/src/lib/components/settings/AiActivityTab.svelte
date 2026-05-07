<script lang="ts">
  import { onMount } from 'svelte';
  import {
    api,
    type AiActivityDetail,
    type AiActivityRow,
    type AiActivitySummary
  } from '$lib/api';

  /// Filter chips drive these. `provider` is null = "any provider".
  let kindFilter = $state<'all' | 'chat' | 'embed'>('all');
  let providerFilter = $state<string | null>(null);
  let errorsOnly = $state(false);
  let summaryWindow = $state<'hour' | 'day' | 'month'>('day');
  const PAGE_SIZE = 50;

  let rows = $state<AiActivityRow[]>([]);
  let summary = $state<AiActivitySummary | null>(null);
  let loading = $state(true);
  let page = $state(0);
  let hasNext = $state(false);
  let detail = $state<AiActivityDetail | null>(null);
  let detailLoading = $state(false);
  let clearing = $state(false);
  let err = $state<string | null>(null);

  /// Distinct providers seen in the current rows + summary.
  /// Drives the per-provider chip row dynamically — we don't
  /// hardcode 'openai' / 'ollama' since the user might use
  /// Anthropic or a self-hosted vLLM.
  let knownProviders = $derived(
    Array.from(
      new Set([
        ...(rows ?? []).map((r) => r.provider),
        ...(summary?.buckets ?? []).map((b) => b.provider)
      ])
    ).sort()
  );

  async function refresh() {
    loading = true;
    err = null;
    try {
      const [list, sum] = await Promise.all([
        api.aiActivity({
          kind: kindFilter === 'all' ? undefined : (
            kindFilter === 'chat' ? 'chat_stream' : 'embed'
          ),
          provider: providerFilter ?? undefined,
          errors_only: errorsOnly,
          limit: PAGE_SIZE + 1,
          offset: page * PAGE_SIZE
        }),
        api.aiActivitySummary(summaryWindow)
      ]);
      rows = list.slice(0, PAGE_SIZE);
      hasNext = list.length > PAGE_SIZE;
      summary = sum;
    } catch (e) {
      err = e instanceof Error ? e.message : String(e);
    } finally {
      loading = false;
    }
  }

  /// Polling tick — keeps the table live during indexing, but
  /// only when the tab is mounted. 5s cadence balances freshness
  /// vs query load.
  let pollHandle: ReturnType<typeof setInterval> | null = null;
  function startPolling() {
    if (pollHandle) return;
    pollHandle = setInterval(refresh, 5000);
  }
  function stopPolling() {
    if (pollHandle) {
      clearInterval(pollHandle);
      pollHandle = null;
    }
  }

  onMount(() => {
    void refresh();
    startPolling();
    return stopPolling;
  });

  $effect(() => {
    // Re-fetch whenever filters change. Cheap because the table
    // is capped at 1000 rows server-side.
    kindFilter;
    providerFilter;
    errorsOnly;
    summaryWindow;
    page;
    void refresh();
  });

  function resetPage() {
    page = 0;
  }

  function prevPage() {
    if (page === 0 || loading) return;
    page -= 1;
  }

  function nextPage() {
    if (!hasNext || loading) return;
    page += 1;
  }

  async function openDetail(id: number) {
    detail = null;
    detailLoading = true;
    try {
      detail = await api.aiActivityDetail(id);
    } catch (e) {
      detail = null;
      err = e instanceof Error ? e.message : String(e);
    } finally {
      detailLoading = false;
    }
  }

  async function clearAll() {
    if (!rows.length) return;
    if (!confirm('Wipe the entire activity log? This cannot be undone — chat history (a different table) is unaffected.')) return;
    clearing = true;
    try {
      await api.aiClearActivity();
      await refresh();
    } catch (e) {
      err = e instanceof Error ? e.message : String(e);
    } finally {
      clearing = false;
    }
  }

  function fmtTs(unix: number): string {
    const d = new Date(unix * 1000);
    return d.toLocaleString();
  }
  function fmtTime(unix: number): string {
    const d = new Date(unix * 1000);
    return d.toLocaleTimeString();
  }

  function fmtMs(ms: number): string {
    if (ms < 1000) return `${ms}ms`;
    return `${(ms / 1000).toFixed(1)}s`;
  }

  function fmtBytes(b: number): string {
    if (b < 1024) return `${b}B`;
    if (b < 1024 * 1024) return `${(b / 1024).toFixed(1)}KB`;
    return `${(b / (1024 * 1024)).toFixed(2)}MB`;
  }

  /// USD cost for one bucket using the matching rate row. Returns
  /// null when no rate is published for that model — the UI shows
  /// "—" rather than fabricating $0.
  function bucketCost(b: AiActivitySummary['buckets'][number]): number | null {
    if (!summary) return null;
    const rate = summary.rates.find(
      (r) => r.provider === b.provider && r.model === b.model
    );
    if (!rate) return null;
    if (rate.prompt_per_1m_usd === null && rate.completion_per_1m_usd === null) return null;
    const promptCost = (rate.prompt_per_1m_usd ?? 0) * (b.sum_prompt_tokens / 1_000_000);
    const compCost = (rate.completion_per_1m_usd ?? 0) * (b.sum_completion_tokens / 1_000_000);
    return promptCost + compCost;
  }

  function fmtCost(usd: number | null): string {
    if (usd === null) return '—';
    if (usd === 0) return '$0';
    if (usd < 0.01) return `~$${usd.toFixed(4)}`;
    if (usd < 1) return `$${usd.toFixed(3)}`;
    return `$${usd.toFixed(2)}`;
  }

  /// Aggregate cost across all buckets of the active summary.
  let totalCost = $derived(
    summary
      ? summary.buckets.reduce((acc, b) => {
          const c = bucketCost(b);
          return c === null ? acc : acc + c;
        }, 0)
      : 0
  );
  let totalCalls = $derived(
    summary ? summary.buckets.reduce((acc, b) => acc + b.calls, 0) : 0
  );
  let totalErrors = $derived(
    summary ? summary.buckets.reduce((acc, b) => acc + b.errors, 0) : 0
  );
</script>

<section class="activity">
  <!-- ─────────── Summary strip ─────────── -->
  <div class="summary">
    <div class="window-picker" role="group" aria-label="Summary window">
      <button
        type="button"
        class:active={summaryWindow === 'hour'}
        onclick={() => { summaryWindow = 'hour'; resetPage(); }}
      >Last hour</button>
      <button
        type="button"
        class:active={summaryWindow === 'day'}
        onclick={() => { summaryWindow = 'day'; resetPage(); }}
      >Last 24h</button>
      <button
        type="button"
        class:active={summaryWindow === 'month'}
        onclick={() => { summaryWindow = 'month'; resetPage(); }}
      >Last 30d</button>
    </div>

    {#if summary}
      <div class="summary-headline">
        <span class="metric"><strong>{totalCalls.toLocaleString()}</strong> calls</span>
        <span class="metric"><strong>{fmtCost(totalCost)}</strong> est.</span>
        {#if totalErrors > 0}
          <span class="metric err"><strong>{totalErrors}</strong> errors</span>
        {/if}
      </div>

      {#if summary.buckets.length > 0}
        <div class="table-wrap">
        <table class="buckets">
          <thead>
            <tr>
              <th>Provider</th>
              <th>Kind</th>
              <th>Model</th>
              <th class="num">Calls</th>
              <th class="num">Tokens in</th>
              <th class="num">Tokens out</th>
              <th class="num">Errors</th>
              <th class="num">Est. cost</th>
            </tr>
          </thead>
          <tbody>
            {#each summary.buckets as b (b.provider + b.kind + b.model)}
              <tr>
                <td><code>{b.provider}</code></td>
                <td>{b.kind}</td>
                <td><code class="model">{b.model}</code></td>
                <td class="num">{b.calls.toLocaleString()}</td>
                <td class="num">{b.sum_prompt_tokens.toLocaleString()}</td>
                <td class="num">{b.sum_completion_tokens.toLocaleString()}</td>
                <td class="num" class:err={b.errors > 0}>{b.errors}</td>
                <td class="num">{fmtCost(bucketCost(b))}</td>
              </tr>
            {/each}
          </tbody>
        </table>
        </div>
      {/if}
    {/if}
  </div>

  <!-- ─────────── Filters ─────────── -->
  <div class="filters">
    <div class="chip-group" role="tablist" aria-label="Kind">
      <button
        role="tab"
        aria-selected={kindFilter === 'all'}
        class:active={kindFilter === 'all'}
        onclick={() => { kindFilter = 'all'; resetPage(); }}
      >All</button>
      <button
        role="tab"
        aria-selected={kindFilter === 'chat'}
        class:active={kindFilter === 'chat'}
        onclick={() => { kindFilter = 'chat'; resetPage(); }}
      >Chat</button>
      <button
        role="tab"
        aria-selected={kindFilter === 'embed'}
        class:active={kindFilter === 'embed'}
        onclick={() => { kindFilter = 'embed'; resetPage(); }}
      >Embed</button>
    </div>

    {#if knownProviders.length > 0}
      <div class="chip-group" role="tablist" aria-label="Provider">
        <button
          role="tab"
          aria-selected={providerFilter === null}
          class:active={providerFilter === null}
          onclick={() => { providerFilter = null; resetPage(); }}
        >Any</button>
        {#each knownProviders as p (p)}
          <button
            role="tab"
            aria-selected={providerFilter === p}
            class:active={providerFilter === p}
            onclick={() => { providerFilter = p; resetPage(); }}
          >{p}</button>
        {/each}
      </div>
    {/if}

    <label class="errors-toggle">
      <input type="checkbox" bind:checked={errorsOnly} onchange={resetPage} />
      <span>Errors only</span>
    </label>

    <span class="filler"></span>

    <div class="pager" aria-label="Activity pages">
      <button type="button" disabled={page === 0 || loading} onclick={prevPage}>Previous</button>
      <span>Page {page + 1}</span>
      <button type="button" disabled={!hasNext || loading} onclick={nextPage}>Next</button>
    </div>

    <button
      type="button"
      class="btn ghost"
      disabled={!rows.length || clearing}
      onclick={clearAll}
    >{clearing ? 'Clearing…' : 'Clear log'}</button>
  </div>

  <!-- ─────────── Table ─────────── -->
  {#if err}
    <p class="status-line err">⚠ {err}</p>
  {/if}

  {#if loading && rows.length === 0}
    <p class="muted">Loading…</p>
  {:else if rows.length === 0}
    <p class="muted">
      No activity yet. Once Datas runs an indexing pass or answers a
      question, every call shows here.
    </p>
  {:else}
    <div class="table-wrap">
    <table class="rows">
      <thead>
        <tr>
          <th>When</th>
          <th>Kind</th>
          <th>Provider</th>
          <th>Model</th>
          <th class="num">Latency</th>
          <th class="num">Tokens</th>
          <th class="num">Size</th>
          <th>Status</th>
        </tr>
      </thead>
      <tbody>
        {#each rows as r (r.id)}
          <tr
            class:err={r.status === 'error'}
            tabindex="0"
            role="button"
            onclick={() => openDetail(r.id)}
            onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') openDetail(r.id); }}
            title="Click for full request/response"
          >
            <td title={fmtTs(r.ts_utc)}>{fmtTime(r.ts_utc)}</td>
            <td>{r.kind}</td>
            <td><code>{r.provider}</code></td>
            <td><code class="model">{r.model || '—'}</code></td>
            <td class="num">{fmtMs(r.elapsed_ms)}</td>
            <td class="num">
              {#if r.prompt_tokens || r.completion_tokens}
                {r.prompt_tokens.toLocaleString()} / {r.completion_tokens.toLocaleString()}
              {:else}
                —
              {/if}
            </td>
            <td class="num">{fmtBytes(r.input_bytes)}{r.output_bytes ? ` / ${fmtBytes(r.output_bytes)}` : ''}</td>
            <td>
              {#if r.status === 'ok'}
                <span class="status-pill ok">ok</span>
              {:else}
                <span class="status-pill err" title={r.error_message ?? ''}>error</span>
              {/if}
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
    </div>
  {/if}
</section>

<!-- ─────────── Detail drawer ─────────── -->
{#if detail || detailLoading}
  <div class="drawer-scrim" onclick={() => (detail = null)} aria-hidden="true"></div>
  <aside class="drawer" role="dialog" aria-label="Activity detail">
    <header class="drawer-head">
      <strong>Activity #{detail?.id ?? '…'}</strong>
      <button
        type="button"
        class="drawer-close"
        onclick={() => (detail = null)}
        aria-label="Close"
      >✕</button>
    </header>
    {#if detailLoading}
      <p class="muted">Loading…</p>
    {:else if detail}
      <dl class="meta">
        <dt>When</dt><dd>{fmtTs(detail.ts_utc)}</dd>
        <dt>Kind</dt><dd>{detail.kind}</dd>
        <dt>Provider</dt><dd><code>{detail.provider}</code></dd>
        <dt>Model</dt><dd><code>{detail.model || '—'}</code></dd>
        <dt>Status</dt>
        <dd>
          {#if detail.status === 'ok'}
            <span class="status-pill ok">ok</span>
          {:else}
            <span class="status-pill err">error</span>
          {/if}
        </dd>
        <dt>Latency</dt><dd>{fmtMs(detail.elapsed_ms)}</dd>
        <dt>Tokens</dt>
        <dd>{detail.prompt_tokens.toLocaleString()} in · {detail.completion_tokens.toLocaleString()} out</dd>
        <dt>Sizes</dt>
        <dd>
          {fmtBytes(detail.input_bytes)} req · {fmtBytes(detail.output_bytes)} resp
          {#if detail.input_bytes > 4096 || detail.output_bytes > 4096}
            <span class="muted">(samples below truncated to 4 KB each)</span>
          {/if}
        </dd>
        {#if detail.error_message}
          <dt>Error</dt>
          <dd class="err-text">{detail.error_message}</dd>
        {/if}
      </dl>

      {#if detail.request_sample}
        <div class="payload-block">
          <h4>Request payload</h4>
          <pre>{detail.request_sample}</pre>
        </div>
      {/if}
      {#if detail.response_sample}
        <div class="payload-block">
          <h4>Response payload</h4>
          <pre>{detail.response_sample}</pre>
        </div>
      {/if}
    {/if}
  </aside>
{/if}

<style>
  .activity {
    display: flex;
    flex-direction: column;
    gap: 1rem;
    min-width: 0;
  }

  .summary {
    border: 1px solid var(--border);
    border-radius: 0.85rem;
    padding: 0.7rem 0.95rem;
    background: color-mix(in oklab, var(--surface-2) 60%, transparent);
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
  }
  .window-picker {
    display: inline-flex;
    align-items: center;
    gap: 0.25rem;
    align-self: flex-start;
  }
  .window-picker button {
    font: inherit;
    font-size: 0.78rem;
    padding: 0.3rem 0.7rem;
    border-radius: 999px;
    border: 1px solid color-mix(in oklab, currentColor 16%, transparent);
    background: transparent;
    color: inherit;
    cursor: pointer;
  }
  .window-picker button.active {
    background: var(--accent);
    border-color: var(--accent);
    color: white;
  }

  .summary-headline {
    display: inline-flex;
    flex-wrap: wrap;
    gap: 1.4rem;
    font-size: 0.9rem;
  }
  .summary-headline .metric strong {
    font-size: 1.05rem;
    margin-right: 0.3em;
    font-variant-numeric: tabular-nums;
  }
  .summary-headline .metric.err strong {
    color: #c83333;
  }

  table.buckets,
  table.rows {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.84rem;
  }
  .table-wrap {
    width: 100%;
    overflow-x: auto;
    border-radius: 0.65rem;
  }
  table.buckets th,
  table.rows th {
    text-align: left;
    font-size: 0.7rem;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--muted);
    padding: 0.35rem 0.5rem;
    border-bottom: 1px solid var(--border);
  }
  table.buckets td,
  table.rows td {
    padding: 0.4rem 0.5rem;
    border-bottom: 1px solid color-mix(in oklab, var(--border) 50%, transparent);
    vertical-align: middle;
  }
  table.rows tr {
    cursor: pointer;
  }
  table.rows tr:hover td {
    background: color-mix(in oklab, currentColor 5%, transparent);
  }
  table.rows tr:focus-visible td {
    background: color-mix(in oklab, var(--accent) 10%, transparent);
    outline: 2px solid color-mix(in oklab, var(--accent) 40%, transparent);
    outline-offset: -2px;
  }
  td.num,
  th.num {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  code.model {
    font-size: 0.78rem;
  }
  td.err,
  td .err {
    color: #c83333;
  }

  .filters {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.6rem;
  }
  .filler { flex: 1; }
  .pager {
    display: inline-flex;
    align-items: center;
    gap: 0.45rem;
    font-size: 0.8rem;
  }
  .pager button {
    font: inherit;
    padding: 0.32rem 0.7rem;
    border-radius: 999px;
    border: 1px solid color-mix(in oklab, currentColor 16%, transparent);
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
  .chip-group {
    display: inline-flex;
    align-items: center;
    border-radius: 999px;
    border: 1px solid var(--border);
    padding: 0.18rem;
  }
  .chip-group button {
    font: inherit;
    font-size: 0.78rem;
    padding: 0.32rem 0.75rem;
    background: transparent;
    border: 0;
    border-radius: 999px;
    color: inherit;
    cursor: pointer;
  }
  .chip-group button.active {
    background: color-mix(in oklab, var(--accent) 90%, transparent);
    color: white;
    font-weight: 600;
  }
  .errors-toggle {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    font-size: 0.84rem;
    user-select: none;
  }

  .status-pill {
    display: inline-block;
    padding: 0.08rem 0.45rem;
    border-radius: 999px;
    font-size: 0.72rem;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .status-pill.ok {
    background: color-mix(in oklab, mediumseagreen 18%, transparent);
    color: color-mix(in oklab, mediumseagreen 70%, var(--fg));
  }
  .status-pill.err {
    background: color-mix(in oklab, tomato 18%, transparent);
    color: color-mix(in oklab, tomato 75%, var(--fg));
  }
  .status-line.err {
    color: #c83333;
    font-size: 0.86rem;
  }

  .btn {
    font: inherit;
    font-size: 0.8rem;
    padding: 0.4rem 0.85rem;
    border-radius: 999px;
    cursor: pointer;
    border: 1px solid color-mix(in oklab, currentColor 18%, transparent);
    background: transparent;
    color: inherit;
  }
  .btn.ghost:hover:not(:disabled) {
    background: color-mix(in oklab, currentColor 6%, transparent);
  }
  .btn:disabled { opacity: 0.5; cursor: not-allowed; }

  /* ─────────── Detail drawer ─────────── */
  .drawer-scrim {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.4);
    z-index: 60;
    backdrop-filter: blur(2px);
  }
  .drawer {
    position: fixed;
    top: 0;
    right: 0;
    bottom: 0;
    width: min(48rem, 90vw);
    z-index: 61;
    background: var(--surface);
    border-left: 1px solid var(--border);
    box-shadow: -8px 0 30px rgba(0, 0, 0, 0.18);
    overflow-y: auto;
    padding: 1rem 1.2rem 1.5rem;
    display: flex;
    flex-direction: column;
    gap: 0.85rem;
  }
  .drawer-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .drawer-close {
    background: transparent;
    border: 0;
    color: inherit;
    font-size: 1.05rem;
    cursor: pointer;
    padding: 0.2rem 0.45rem;
    border-radius: 0.4rem;
  }
  .drawer-close:hover {
    background: color-mix(in oklab, currentColor 8%, transparent);
  }
  dl.meta {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 0.3rem 0.85rem;
    margin: 0;
    font-size: 0.86rem;
  }
  dl.meta dt {
    color: var(--muted);
    font-weight: 600;
    font-size: 0.74rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    align-self: center;
  }
  dl.meta dd {
    margin: 0;
    word-break: break-word;
  }
  dl.meta dd.err-text {
    color: color-mix(in oklab, tomato 75%, var(--fg));
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 0.78rem;
  }
  dl.meta .muted {
    color: var(--muted);
    font-size: 0.78rem;
    margin-left: 0.4rem;
  }
  .payload-block h4 {
    margin: 0.4rem 0 0.3rem;
    font-size: 0.78rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--muted);
  }
  .payload-block pre {
    margin: 0;
    padding: 0.6rem 0.8rem;
    background: color-mix(in oklab, var(--surface-2) 80%, transparent);
    border: 1px solid var(--border);
    border-radius: 0.55rem;
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 0.74rem;
    line-height: 1.5;
    overflow-x: auto;
    white-space: pre-wrap;
    word-break: break-word;
    max-height: 24rem;
    overflow-y: auto;
  }
  .muted { color: var(--muted); }
</style>
