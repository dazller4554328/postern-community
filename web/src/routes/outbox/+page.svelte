<script lang="ts">
  import './outbox.css';
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

