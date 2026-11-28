<script lang="ts">
  import './audit.css';
  import { onMount } from 'svelte';
  import { page } from '$app/stores';
  import { api, type AuditEntry, type AuditCategory } from '$lib/api';
  import { formatDate } from '$lib/format';
  import { icon, label, severity, severityLabel, HERO } from './_lib/auditMeta';

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

