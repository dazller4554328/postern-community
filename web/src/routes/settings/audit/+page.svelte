<script lang="ts">
  import { onMount } from 'svelte';
  import { page } from '$app/stores';
  import { api, type AuditEntry, type AuditCategory } from '$lib/api';
  import { formatDate } from '$lib/format';

  let entries = $state<AuditEntry[]>([]);
  let loading = $state(true);
  let hasMore = $state(true);
  let tab = $state<AuditCategory>('security');

  const PAGE = 50;

  onMount(async () => {
    const initial = $page.url.searchParams.get('tab');
    if (initial === 'activity' || initial === 'security') {
      tab = initial;
    }
    await load();
    loading = false;
  });

  async function load(append = false) {
    const batch = await api.auditLog({
      limit: PAGE,
      offset: append ? entries.length : 0,
      category: tab
    });
    entries = append ? [...entries, ...batch] : batch;
    hasMore = batch.length === PAGE;
  }

  async function switchTab(next: AuditCategory) {
    if (next === tab) return;
    tab = next;
    loading = true;
    entries = [];
    hasMore = true;
    await load();
    loading = false;
  }

  const ICONS: Record<string, string> = {
    // Security
    vault_init: '🔐',
    vault_unlock: '🔓',
    vault_unlock_failed: '🚫',
    vault_lock: '🔒',
    ip_change_lock: '🌐',
    password_changed: '🔑',
    account_added: '📬',
    account_deleted: '🗑',
    rule_created: '📋',
    sync_interval_changed: '⏱',
    sync_policy_changed: '🔄',
    // Activity
    sync_started: '⟳',
    sync_completed: '✓',
    sync_error: '⚠',
    folder_sync_error: '⚠',
    smtp_send: '📤',
    smtp_error: '✗',
    imap_error: '✗'
  };

  function icon(t: string) { return ICONS[t] ?? '📝'; }
  function label(t: string) {
    return t.replace(/_/g, ' ').replace(/\b\w/g, c => c.toUpperCase());
  }

  function severity(t: string) {
    if (t.includes('failed') || t.includes('error') || t.includes('ip_change')) return 'warn';
    if (t.includes('password') || t.includes('deleted')) return 'elevated';
    if (t === 'sync_completed' || t === 'smtp_send') return 'success';
    return 'normal';
  }

  function severityLabel(t: string) {
    const s = severity(t);
    if (s === 'warn') return 'Alert';
    if (s === 'elevated') return 'Sensitive';
    if (s === 'success') return 'OK';
    return 'Normal';
  }

  const HERO = {
    security: {
      eyebrow: 'Event Ledger',
      title: 'Security Audit Log',
      body:
        'Review the local security trail for vault access, identity changes, mailbox mutations, and network anomalies.',
      chips: ['Local-only event history', 'Vault lifecycle tracking', 'IP anomaly visibility'],
      emptyHint:
        'No events logged yet. Entries will appear after vault or account security operations.',
      tableSub: 'Newest entries first. Elevated rows highlight failed access and suspicious network changes.'
    },
    activity: {
      eyebrow: 'Server Activity',
      title: 'Sync & Send Activity',
      body:
        'See what the server is doing under the hood — mail sync cycles, messages fetched, send outcomes, and errors.',
      chips: ['Sync cycle timing', 'Send / receive outcomes', 'Live error surfacing'],
      emptyHint:
        'No activity recorded yet. Events will appear once the scheduler runs or you send a message.',
      tableSub: 'Newest first. Red rows are errors; green rows are successful sync/send operations.'
    }
  } as const;

  const hero = $derived(HERO[tab]);
</script>

<article class="audit-shell">
  <div class="page-top">
    <a class="back" href="/settings">← Settings</a>
  </div>

  <header class="hero">
    <div class="hero-copy">
      <span class="eyebrow">{hero.eyebrow}</span>
      <h1>{hero.title}</h1>
      <p>{hero.body}</p>
    </div>
    <div class="hero-badges">
      {#each hero.chips as chip}
        <span class="hero-chip">{chip}</span>
      {/each}
    </div>
  </header>

  <nav class="tabs" role="tablist" aria-label="Log category">
    <button
      type="button"
      role="tab"
      class="tab"
      class:active={tab === 'security'}
      aria-selected={tab === 'security'}
      onclick={() => switchTab('security')}
    >
      <span class="tab-icon">⛨</span>
      <span>Security</span>
    </button>
    <button
      type="button"
      role="tab"
      class="tab"
      class:active={tab === 'activity'}
      aria-selected={tab === 'activity'}
      onclick={() => switchTab('activity')}
    >
      <span class="tab-icon">⟳</span>
      <span>Activity</span>
    </button>
  </nav>

  {#if loading}
    <section class="panel empty-state">
      <p class="muted">Loading {tab === 'activity' ? 'activity' : 'audit'} trail…</p>
    </section>
  {:else if entries.length === 0}
    <section class="panel empty-state">
      <p class="muted">{hero.emptyHint}</p>
    </section>
  {:else}
    <section class="panel table-shell">
      <div class="table-head">
        <div>
          <h2>Recorded events</h2>
          <p>{hero.tableSub}</p>
        </div>
        <div class="table-badges">
          <span class="count-chip">{entries.length} loaded</span>
          {#if hasMore}
            <span class="count-chip subtle">More available</span>
          {/if}
        </div>
      </div>

      <div class="table-wrap">
        <table>
          <thead>
            <tr>
              <th class="col-icon"></th>
              <th class="col-time">Time</th>
              <th class="col-event">Event</th>
              <th class="col-detail">Detail</th>
              <th class="col-ip">IP</th>
            </tr>
          </thead>
          <tbody>
            {#each entries as e (e.id)}
              <tr
                class:warn={severity(e.event_type) === 'warn'}
                class:elevated={severity(e.event_type) === 'elevated'}
                class:success={severity(e.event_type) === 'success'}
              >
                <td class="col-icon">
                  <span class="event-glyph">{icon(e.event_type)}</span>
                </td>
                <td class="col-time">
                  <time title={new Date(e.ts_utc * 1000).toLocaleString()}>
                    {formatDate(e.ts_utc)}
                  </time>
                </td>
                <td class="col-event">
                  <strong>{label(e.event_type)}</strong>
                  <span class="event-type">{severityLabel(e.event_type)}</span>
                </td>
                <td class="col-detail">{e.detail ?? '—'}</td>
                <td class="col-ip"><code>{e.ip ?? '—'}</code></td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>

      {#if hasMore}
        <div class="more">
          <button onclick={() => load(true)}>Load more</button>
        </div>
      {/if}
    </section>

    <section class="panel notes">
      <div class="section-head">
        <span class="section-icon">{tab === 'activity' ? '⟳' : '⛨'}</span>
        <div>
          <h2>What to watch</h2>
          <p>
            {tab === 'activity'
              ? 'Use the activity stream to verify the scheduler is ticking and see where messages are ending up.'
              : 'Use the audit stream to spot unusual access behavior quickly.'}
          </p>
        </div>
      </div>
      {#if tab === 'activity'}
        <div class="note-grid">
          <div class="note-card">
            <strong>Sync cycles</strong>
            <span>`sync_started` / `sync_completed` should appear every minute per account. Gaps usually mean the vault is locked or the account is offline.</span>
          </div>
          <div class="note-card">
            <strong>Folder errors</strong>
            <span>`folder_sync_error` isolates which mailbox failed — Gmail categories, trash, and user labels show up here when a fetch fails.</span>
          </div>
          <div class="note-card">
            <strong>Send outcomes</strong>
            <span>`smtp_send` confirms a message left the server; `smtp_error` captures the failure reason exactly as the SMTP host returned it.</span>
          </div>
        </div>
      {:else}
        <div class="note-grid">
          <div class="note-card">
            <strong>Failed unlocks</strong>
            <span>Repeated `vault_unlock_failed` entries usually indicate a mistyped password or unauthorized access attempt.</span>
          </div>
          <div class="note-card">
            <strong>IP change lock</strong>
            <span>`ip_change_lock` means Postern locked the vault after a source IP change and expects an explicit unlock.</span>
          </div>
          <div class="note-card">
            <strong>Mailbox churn</strong>
            <span>Track `account_added`, `account_deleted`, and rule activity here after any admin-side configuration changes.</span>
          </div>
        </div>
      {/if}
    </section>
  {/if}
</article>

<style>
  article.audit-shell {
    width: 100%;
    max-width: clamp(60rem, 94vw, 110rem);
    margin: 0 auto;
    padding: 1.25rem 2rem 2.75rem;
    box-sizing: border-box;
  }
  .page-top { margin-bottom: 0.9rem; }
  .back { display: inline-block; color: inherit; opacity: 0.62; text-decoration: none; font-size: 0.85rem; }
  .back:hover { opacity: 1; }
  .hero {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 1rem;
    padding: 1.4rem 1.5rem;
    margin-bottom: 1rem;
    border: 1px solid var(--border);
    border-radius: 1.35rem;
    background:
      radial-gradient(circle at top right, color-mix(in oklab, var(--accent) 12%, transparent), transparent 32%),
      linear-gradient(180deg, color-mix(in oklab, var(--surface) 88%, white 12%), var(--surface));
    box-shadow: 0 18px 42px rgba(0, 0, 0, 0.08);
  }
  .eyebrow {
    display: inline-block;
    margin-bottom: 0.55rem;
    font-size: 0.7rem;
    text-transform: uppercase;
    letter-spacing: 0.12em;
    color: var(--muted);
    font-weight: 700;
  }
  .hero h1 { font-size: 2rem; font-weight: 650; margin: 0 0 0.4rem; letter-spacing: -0.03em; }
  .hero p { font-size: 0.9rem; color: var(--muted); margin: 0; line-height: 1.55; max-width: 46rem; }
  .hero-badges { display: flex; flex-wrap: wrap; gap: 0.45rem; align-content: start; justify-content: flex-end; }
  .hero-chip {
    display: inline-flex;
    align-items: center;
    padding: 0.42rem 0.72rem;
    border-radius: 999px;
    background: color-mix(in oklab, var(--surface-2) 82%, transparent);
    border: 1px solid color-mix(in oklab, currentColor 8%, transparent);
    font-size: 0.72rem;
    font-weight: 600;
  }
  .panel {
    border: 1px solid var(--border);
    border-radius: 1.1rem;
    background: color-mix(in oklab, var(--surface) 94%, transparent);
    box-shadow: 0 14px 32px rgba(0, 0, 0, 0.05);
  }
  .empty-state {
    padding: 1.25rem 1.35rem;
  }
  .muted { opacity: 0.55; font-size: 0.88rem; }
  .table-shell {
    overflow: hidden;
    margin-bottom: 1rem;
  }
  .table-head {
    display: flex;
    justify-content: space-between;
    gap: 1rem;
    align-items: flex-start;
    padding: 1.15rem 1.25rem 0.85rem;
    border-bottom: 1px solid var(--border);
  }
  .table-head h2 {
    margin: 0 0 0.3rem;
    font-size: 1rem;
    font-weight: 650;
    letter-spacing: -0.02em;
  }
  .table-head p {
    margin: 0;
    font-size: 0.83rem;
    color: var(--muted);
    line-height: 1.5;
  }
  .table-badges {
    display: flex;
    flex-wrap: wrap;
    gap: 0.45rem;
  }
  .count-chip {
    display: inline-flex;
    align-items: center;
    padding: 0.42rem 0.7rem;
    border-radius: 999px;
    background: color-mix(in oklab, var(--accent) 12%, transparent);
    color: color-mix(in oklab, var(--fg) 82%, var(--accent));
    font-size: 0.72rem;
    font-weight: 700;
    border: 1px solid color-mix(in oklab, var(--accent) 18%, var(--border));
  }
  .count-chip.subtle {
    background: color-mix(in oklab, var(--surface-2) 84%, transparent);
    color: var(--muted);
  }
  .table-wrap {
    overflow-x: auto;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.83rem;
  }
  thead th {
    text-align: left;
    font-weight: 600;
    font-size: 0.72rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    opacity: 0.5;
    padding: 0.5rem 0.75rem;
    border-bottom: 1px solid var(--border);
  }
  tbody tr { border-bottom: 1px solid var(--border); }
  tbody tr:hover {
    background: var(--row-hover);
  }
  tbody tr.warn {
    background: color-mix(in oklab, crimson 6%, transparent);
  }
  tbody tr.warn:hover {
    background: color-mix(in oklab, crimson 10%, transparent);
  }
  tbody tr.elevated {
    background: color-mix(in oklab, goldenrod 8%, transparent);
  }
  tbody tr.elevated:hover {
    background: color-mix(in oklab, goldenrod 12%, transparent);
  }
  tbody tr.success {
    background: color-mix(in oklab, seagreen 5%, transparent);
  }
  tbody tr.success:hover {
    background: color-mix(in oklab, seagreen 9%, transparent);
  }
  tbody tr.success .event-type {
    background: color-mix(in oklab, seagreen 14%, transparent);
    color: color-mix(in oklab, seagreen 60%, var(--fg));
    border-color: color-mix(in oklab, seagreen 18%, transparent);
  }

  .tabs {
    display: inline-flex;
    gap: 0.3rem;
    padding: 0.3rem;
    margin-bottom: 1rem;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: color-mix(in oklab, var(--surface-2) 72%, transparent);
  }
  .tab {
    display: inline-flex;
    align-items: center;
    gap: 0.45rem;
    padding: 0.5rem 1rem;
    border: 0;
    background: transparent;
    color: inherit;
    font: inherit;
    font-size: 0.82rem;
    font-weight: 600;
    border-radius: 999px;
    cursor: pointer;
    opacity: 0.65;
    transition: background 120ms, opacity 120ms;
  }
  .tab:hover { opacity: 0.88; }
  .tab.active {
    background: var(--surface);
    opacity: 1;
    box-shadow: 0 4px 14px rgba(0, 0, 0, 0.08);
  }
  .tab-icon {
    font-size: 0.95rem;
    line-height: 1;
  }
  td {
    padding: 0.7rem 0.8rem;
    vertical-align: top;
  }
  .col-icon { width: 2rem; text-align: center; }
  .event-glyph {
    display: inline-flex;
    width: 1.85rem;
    height: 1.85rem;
    align-items: center;
    justify-content: center;
    border-radius: 999px;
    background: color-mix(in oklab, var(--surface-2) 82%, transparent);
    border: 1px solid color-mix(in oklab, currentColor 7%, transparent);
    font-size: 0.9rem;
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.08);
  }
  .col-time { width: 8.4rem; opacity: 0.78; white-space: nowrap; }
  .col-event strong {
    display: block;
    font-weight: 650;
    margin-bottom: 0.2rem;
  }
  .event-type {
    display: inline-flex;
    align-items: center;
    padding: 0.18rem 0.45rem;
    border-radius: 999px;
    background: color-mix(in oklab, var(--surface-2) 88%, transparent);
    border: 1px solid color-mix(in oklab, currentColor 8%, transparent);
    font-size: 0.66rem;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--muted);
  }
  tbody tr.warn .event-type {
    background: color-mix(in oklab, crimson 14%, transparent);
    color: color-mix(in oklab, crimson 60%, var(--fg));
    border-color: color-mix(in oklab, crimson 18%, transparent);
  }
  tbody tr.elevated .event-type {
    background: color-mix(in oklab, goldenrod 18%, transparent);
    color: color-mix(in oklab, goldenrod 58%, var(--fg));
    border-color: color-mix(in oklab, goldenrod 18%, transparent);
  }
  .col-detail { opacity: 0.8; line-height: 1.45; }
  .col-ip code {
    font-family: ui-monospace, monospace;
    font-size: 0.78rem;
    background: color-mix(in oklab, currentColor 6%, transparent);
    padding: 0.22em 0.45em;
    border-radius: 999px;
  }

  .more {
    padding: 1rem 1.25rem 1.2rem;
    text-align: center;
  }
  .more button {
    font: inherit;
    font-size: 0.84rem;
    font-weight: 600;
    padding: 0.58rem 1.15rem;
    border: 1px solid var(--border);
    background: color-mix(in oklab, var(--surface-2) 78%, transparent);
    color: inherit;
    border-radius: 999px;
    cursor: pointer;
  }
  .more button:hover {
    background: color-mix(in oklab, var(--surface-2) 92%, transparent);
  }
  .notes {
    padding: 1.15rem 1.25rem 1.25rem;
  }
  .section-head {
    display: flex;
    gap: 0.9rem;
    align-items: flex-start;
    margin-bottom: 1rem;
  }
  .section-icon {
    display: inline-flex;
    width: 2.2rem;
    height: 2.2rem;
    align-items: center;
    justify-content: center;
    border-radius: 0.8rem;
    background: color-mix(in oklab, var(--accent) 12%, transparent);
    color: color-mix(in oklab, var(--fg) 82%, var(--accent));
    font-size: 1rem;
    flex: 0 0 auto;
  }
  .section-head h2 {
    margin: 0 0 0.25rem;
    font-size: 1rem;
    font-weight: 650;
    letter-spacing: -0.02em;
  }
  .section-head p {
    margin: 0;
    color: var(--muted);
    font-size: 0.84rem;
    line-height: 1.5;
  }
  .note-grid {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 0.85rem;
  }
  .note-card {
    display: grid;
    gap: 0.35rem;
    padding: 0.95rem 1rem;
    border-radius: 0.95rem;
    background: color-mix(in oklab, var(--surface-2) 76%, transparent);
    border: 1px solid color-mix(in oklab, currentColor 6%, transparent);
  }
  .note-card strong {
    font-size: 0.88rem;
    font-weight: 650;
  }
  .note-card span {
    font-size: 0.8rem;
    color: var(--muted);
    line-height: 1.5;
  }
  @media (max-width: 820px) {
    article.audit-shell {
      padding: 1.2rem 1rem 2rem;
    }
    .hero {
      grid-template-columns: 1fr;
      padding: 1.15rem 1.1rem;
    }
    .hero-badges {
      justify-content: flex-start;
    }
    .table-head {
      flex-direction: column;
    }
    .note-grid {
      grid-template-columns: 1fr;
    }
  }
  @media (max-width: 640px) {
    table {
      min-width: 42rem;
    }
  }
</style>
