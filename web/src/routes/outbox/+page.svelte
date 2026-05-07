<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { api, type OutboxListItem } from '$lib/api';
  import { formatDate } from '$lib/format';

  let active = $state<OutboxListItem[]>([]);
  let failures = $state<OutboxListItem[]>([]);
  let loading = $state(true);
  let err = $state<string | null>(null);
  let busy = $state<Record<number, boolean>>({});
  let clearingFailures = $state(false);

  // 3s poll matches the worker tick grain, so status transitions (and
  // the live countdown on scheduled rows) stay responsive.
  let pollHandle: ReturnType<typeof setInterval> | null = null;
  let nowSecs = $state(Math.floor(Date.now() / 1000));
  let clockHandle: ReturnType<typeof setInterval> | null = null;

  async function load() {
    try {
      const [a, f] = await Promise.all([
        api.outboxList(),
        api.outboxRecentFailures()
      ]);
      active = a;
      failures = f;
      err = null;
    } catch (e) {
      err = e instanceof Error ? e.message : String(e);
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    void load();
    pollHandle = setInterval(load, 3000);
    clockHandle = setInterval(() => {
      nowSecs = Math.floor(Date.now() / 1000);
    }, 1000);
  });

  onDestroy(() => {
    if (pollHandle) clearInterval(pollHandle);
    if (clockHandle) clearInterval(clockHandle);
  });

  async function cancel(id: number) {
    busy = { ...busy, [id]: true };
    try {
      await api.outboxCancel(id);
      await load();
    } catch (e) {
      // Most likely 409 — the worker dispatched while the user was
      // clicking. Surface it next to the row so they know undo failed.
      err = e instanceof Error ? e.message : String(e);
    } finally {
      busy = { ...busy, [id]: false };
    }
  }

  async function sendNow(id: number) {
    busy = { ...busy, [id]: true };
    try {
      await api.outboxReschedule(id, Math.floor(Date.now() / 1000));
      await load();
    } catch (e) {
      err = e instanceof Error ? e.message : String(e);
    } finally {
      busy = { ...busy, [id]: false };
    }
  }

  async function clearFailures() {
    if (failures.length === 0) return;
    const ok = confirm(
      `Permanently delete ${failures.length} failed message${
        failures.length === 1 ? '' : 's'
      }? They won't be retried.`
    );
    if (!ok) return;
    clearingFailures = true;
    try {
      await api.outboxClearFailures();
      await load();
    } catch (e) {
      err = e instanceof Error ? e.message : String(e);
    } finally {
      clearingFailures = false;
    }
  }

  function etaLabel(row: OutboxListItem): string {
    if (row.status === 'sending') return 'Dispatching now…';
    const diff = row.scheduled_at - nowSecs;
    if (diff <= 0) return 'Dispatching imminently';
    if (diff < 60) return `In ${diff}s`;
    if (diff < 3600) return `In ${Math.round(diff / 60)} min`;
    if (diff < 86400) return `In ${Math.round(diff / 3600)} h`;
    return new Date(row.scheduled_at * 1000).toLocaleString();
  }
</script>

<article class="outbox-shell">
  <div class="page-top">
    <a class="back" href="/inbox">← Inbox</a>
  </div>

  <header class="hero">
    <span class="eyebrow">Outbox</span>
    <h1>Pending &amp; scheduled sends</h1>
    <p>
      Every send flows through this queue — that's what powers the
      Undo window and Send Later. Rows clear as the worker dispatches.
    </p>
  </header>

  {#if err}
    <p class="err">⚠ {err}</p>
  {/if}

  <section class="panel">
    <div class="section-head">
      <h2>Active {active.length > 0 ? `(${active.length})` : ''}</h2>
      <p>Pending sends waiting for their scheduled time.</p>
    </div>
    {#if loading && active.length === 0}
      <p class="muted">Loading…</p>
    {:else if active.length === 0}
      <p class="muted">Queue is empty. Hit compose to draft something.</p>
    {:else}
      <ul class="rows">
        {#each active as r (r.id)}
          <li class="row status-{r.status}">
            <div class="summary">
              <strong class="subject">{r.summary_subject || '(no subject)'}</strong>
              <span class="to">→ {r.summary_to || '(no recipient)'}</span>
              <span class="eta">{etaLabel(r)}</span>
            </div>
            <div class="actions">
              <button
                class="btn ghost"
                disabled={busy[r.id] || r.status !== 'pending'}
                onclick={() => sendNow(r.id)}
                title="Dispatch on the next worker tick"
              >Send now</button>
              <button
                class="btn danger"
                disabled={busy[r.id] || r.status !== 'pending'}
                onclick={() => cancel(r.id)}
              >{busy[r.id] ? '…' : 'Cancel'}</button>
            </div>
          </li>
        {/each}
      </ul>
    {/if}
  </section>

  {#if failures.length > 0}
    <section class="panel">
      <div class="section-head with-action">
        <div>
          <h2>Recent failures ({failures.length})</h2>
          <p>
            SMTP didn't accept these — usually a transient network issue
            or a rejected recipient. Re-drafting is manual for v1; open
            the compose window and retype.
          </p>
        </div>
        <button
          type="button"
          class="btn danger"
          disabled={clearingFailures}
          onclick={clearFailures}
        >
          {clearingFailures ? 'Clearing…' : 'Clear all'}
        </button>
      </div>
      <ul class="rows failures">
        {#each failures as r (r.id)}
          <li class="row status-failed">
            <div class="summary">
              <strong class="subject">{r.summary_subject || '(no subject)'}</strong>
              <span class="to">→ {r.summary_to || '(no recipient)'}</span>
              <span class="when">{formatDate(r.updated_at)}</span>
            </div>
            {#if r.last_error}
              <div class="err-detail"><code>{r.last_error}</code></div>
            {/if}
          </li>
        {/each}
      </ul>
    </section>
  {/if}
</article>

<style>
  article.outbox-shell {
    max-width: clamp(54rem, 92vw, 96rem);
    width: 100%;
    margin: 0 auto;
    padding: 0 0 2rem;
    box-sizing: border-box;
  }
  .page-top {
    margin-bottom: 0.55rem;
  }
  .back {
    color: var(--muted);
    text-decoration: none;
    font-size: 0.85rem;
  }
  .back:hover {
    color: var(--fg);
  }
  .hero {
    padding: 1.3rem 1.4rem;
    margin-bottom: 1rem;
    border: 1px solid var(--border);
    border-radius: 1.3rem;
    background:
      radial-gradient(circle at top right, color-mix(in oklab, var(--accent) 12%, transparent), transparent 35%),
      linear-gradient(180deg, color-mix(in oklab, var(--surface) 88%, white 12%), var(--surface));
    box-shadow: 0 18px 42px rgba(0, 0, 0, 0.08);
  }
  .hero .eyebrow {
    display: inline-block;
    margin-bottom: 0.55rem;
    font-size: 0.7rem;
    text-transform: uppercase;
    letter-spacing: 0.12em;
    color: var(--muted);
    font-weight: 700;
  }
  .hero h1 {
    margin: 0 0 0.3rem;
    font-size: 1.75rem;
  }
  .hero p {
    margin: 0;
    color: var(--muted);
    max-width: 44rem;
  }
  .panel {
    border: 1px solid var(--border);
    border-radius: 1rem;
    padding: 1rem 1.25rem;
    background: var(--surface);
    margin-bottom: 1rem;
  }
  .section-head h2 {
    margin: 0 0 0.2rem;
    font-size: 1.05rem;
  }
  .section-head p {
    margin: 0 0 0.8rem;
    font-size: 0.85rem;
    color: var(--muted);
  }
  .section-head.with-action {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 1rem;
  }
  .section-head.with-action > div {
    flex: 1 1 auto;
    min-width: 0;
  }
  .section-head.with-action .btn {
    flex-shrink: 0;
    margin-top: 0.1rem;
  }
  .rows {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }
  .row {
    border: 1px solid var(--border);
    border-radius: 0.65rem;
    padding: 0.6rem 0.9rem;
    display: flex;
    gap: 0.8rem;
    align-items: center;
    justify-content: space-between;
    background: var(--surface);
  }
  .row.status-sending {
    border-color: color-mix(in oklab, var(--accent) 50%, transparent);
    background: color-mix(in oklab, var(--accent) 7%, var(--surface));
  }
  .row.status-failed {
    border-color: color-mix(in oklab, crimson 35%, transparent);
    background: color-mix(in oklab, crimson 6%, var(--surface));
    flex-direction: column;
    align-items: stretch;
    gap: 0.45rem;
  }
  .summary {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
    min-width: 0;
    flex: 1;
  }
  .summary .subject {
    font-size: 0.95rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .summary .to {
    font-size: 0.82rem;
    color: var(--muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .summary .eta {
    font-size: 0.78rem;
    color: color-mix(in oklab, var(--accent) 60%, var(--fg) 40%);
    font-variant-numeric: tabular-nums;
  }
  .summary .when {
    font-size: 0.78rem;
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }
  .actions {
    display: flex;
    gap: 0.4rem;
    flex-shrink: 0;
  }
  .btn {
    padding: 0.4rem 0.75rem;
    border-radius: 0.45rem;
    border: 1px solid var(--border);
    background: var(--surface-2);
    color: var(--fg);
    cursor: pointer;
    font-size: 0.82rem;
  }
  .btn.ghost {
    background: transparent;
  }
  .btn.danger {
    border-color: color-mix(in oklab, crimson 45%, transparent);
    color: color-mix(in oklab, crimson 70%, var(--fg) 30%);
  }
  .btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .err {
    padding: 0.6rem 0.9rem;
    border-radius: 0.6rem;
    background: color-mix(in oklab, crimson 10%, var(--surface));
    color: color-mix(in oklab, crimson 70%, var(--fg) 30%);
    margin-bottom: 1rem;
  }
  .err-detail {
    font-size: 0.78rem;
    color: var(--muted);
  }
  .err-detail code {
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    background: color-mix(in oklab, currentColor 5%, transparent);
    padding: 0.05rem 0.3rem;
    border-radius: 0.2em;
  }
  .muted {
    color: var(--muted);
    margin: 0;
  }
</style>
