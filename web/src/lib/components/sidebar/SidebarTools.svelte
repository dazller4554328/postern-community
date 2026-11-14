<script lang="ts">
  import { onMount } from 'svelte';
  import { page } from '$app/stores';
  import { api } from '$lib/api';

  // --- Outbox pending-count badge -----------------------------------------
  //
  // Poll /api/outbox every 30s and count items still in flight (pending or
  // sending). The number renders as a badge on the Outbox row so users can
  // spot scheduled / undo-able sends at a glance. Failures (vault locked,
  // 401 over the tunnel, etc.) leave the old count in place — next tick
  // retries. Failed sends are surfaced separately on the Outbox page; we
  // stay quiet here rather than double-alarm the count.
  let outboxPendingCount = $state(0);
  async function refreshOutboxCount() {
    try {
      const rows = await api.outboxList();
      outboxPendingCount = rows.filter(
        (r) => r.status === 'pending' || r.status === 'sending'
      ).length;
    } catch {
      /* leave count untouched */
    }
  }
  onMount(() => {
    void refreshOutboxCount();
    const t = setInterval(refreshOutboxCount, 30_000);
    return () => clearInterval(t);
  });

  // --- Tool-row active-state detection ------------------------------------
  function isToolActive(prefix: string): boolean {
    return $page.url.pathname === prefix || $page.url.pathname.startsWith(`${prefix}/`);
  }
  function isActivityActive(): boolean {
    return $page.url.pathname === '/settings/audit'
      && $page.url.searchParams.get('tab') === 'activity';
  }
  function isAuditActive(): boolean {
    return $page.url.pathname === '/settings/audit'
      && $page.url.searchParams.get('tab') !== 'activity';
  }
</script>

<!-- Tools: cross-mailbox and operational surfaces that were cluttering
     the footer. Outbox sits first because it's the most time-sensitive
     (users come here to cancel a scheduled or undo-able send). -->
<div class="section-label">Tools</div>
<div class="tools">
  <a
    class="row tool-row"
    class:active={isToolActive('/outbox')}
    href="/outbox"
    title="Scheduled and pending sends"
  >
    <span class="tool-icon" aria-hidden="true">
      <svg viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.65" stroke-linecap="round" stroke-linejoin="round">
        <path d="M3.5 5.5h13v9h-13z"/>
        <path d="m4 6 6 4.6L16 6"/>
        <path d="M13 3.5h4v4"/>
        <path d="M17 3.5 11.5 9"/>
      </svg>
    </span>
    <span class="label">Outbox</span>
    {#if outboxPendingCount > 0}
      <span class="unread-count" title="Pending sends">{outboxPendingCount}</span>
    {/if}
  </a>
  <a
    class="row tool-row"
    class:active={isToolActive('/calendar')}
    href="/calendar"
    title="Calendar"
  >
    <span class="tool-icon" aria-hidden="true">
      <svg viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.65" stroke-linecap="round" stroke-linejoin="round">
        <rect x="3" y="4.5" width="14" height="12" rx="1.6"/>
        <path d="M6.5 2.8v3.4M13.5 2.8v3.4M3 8h14"/>
        <path d="M6.5 11h.1M10 11h.1M13.5 11h.1M6.5 14h.1M10 14h.1"/>
      </svg>
    </span>
    <span class="label">Calendar</span>
  </a>
  <a
    class="row tool-row"
    class:active={isToolActive('/reminders')}
    href="/reminders"
    title="Local reminders"
  >
    <span class="tool-icon" aria-hidden="true">
      <svg viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.65" stroke-linecap="round" stroke-linejoin="round">
        <circle cx="10" cy="10.5" r="5.8"/>
        <path d="M7 3.2 4.8 2M13 3.2 15.2 2M10 7.2v3.5l2.4 1.5M6.3 16l-1.2 1.4M13.7 16l1.2 1.4"/>
      </svg>
    </span>
    <span class="label">Reminders</span>
  </a>
  <a
    class="row tool-row"
    class:active={isToolActive('/contacts')}
    href="/contacts"
    title="Address book — auto-collected from sync + send"
  >
    <span class="tool-icon" aria-hidden="true">
      <svg viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.65" stroke-linecap="round" stroke-linejoin="round">
        <circle cx="10" cy="7.5" r="3"/>
        <path d="M4.5 16.5c0-3 2.5-5 5.5-5s5.5 2 5.5 5"/>
      </svg>
    </span>
    <span class="label">Contacts</span>
  </a>
  <a
    class="row tool-row"
    class:active={isToolActive('/notes')}
    href="/notes"
    title="Secure notes — markdown, vault-encrypted at rest"
  >
    <span class="tool-icon" aria-hidden="true">
      <svg viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.65" stroke-linecap="round" stroke-linejoin="round">
        <path d="M5.5 3h6L15.5 7v10h-10z"/>
        <path d="M11.5 3v4h4"/>
        <path d="M7.5 11h5M7.5 13.5h5M7.5 8.5h2"/>
      </svg>
    </span>
    <span class="label">Notes</span>
  </a>
  <a
    class="row tool-row"
    class:active={isActivityActive()}
    href="/settings/audit?tab=activity"
    title="Server activity — sync cycles, sends, errors"
  >
    <span class="tool-icon activity" aria-hidden="true">
      <svg viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.65" stroke-linecap="round" stroke-linejoin="round">
        <path d="M3 11h3l2-5 4 8 2-4h3"/>
        <path d="M3 16h14"/>
      </svg>
    </span>
    <span class="label">Activity</span>
  </a>
  <a
    class="row tool-row"
    class:active={isAuditActive()}
    href="/settings/audit"
    title="Security audit log"
  >
    <span class="tool-icon audit" aria-hidden="true">
      <svg viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.65" stroke-linecap="round" stroke-linejoin="round">
        <path d="M10 17.5s-5.8-3.4-5.8-8.2V4.1L10 2.2l5.8 1.9v5.2c0 4.8-5.8 8.2-5.8 8.2z"/>
        <path d="m7.4 10 1.8 1.8 3.5-4"/>
      </svg>
    </span>
    <span class="label">Audit</span>
  </a>
</div>

<style>
  /* Tools section uses the same visual language as unified rows: small
     themed glyph, bordered row, and active color from the theme tokens.
     Shared base styles (.row, .label, .unread-count, .section-label)
     live in Sidebar.svelte under :global() so this child inherits them. */
  .tools {
    display: flex;
    flex-direction: column;
    gap: 0.18rem;
    padding: 0.1rem 0 0.2rem;
  }
  .tool-row {
    font-weight: 500;
    text-decoration: none;
    color: inherit;
    border: 1px solid color-mix(in oklab, currentColor 8%, transparent);
    background: color-mix(in oklab, var(--surface) 92%, transparent);
  }
  .tool-row:hover {
    background: var(--row-hover);
    border-color: color-mix(in oklab, currentColor 14%, transparent);
  }
  .tool-row.active {
    background: var(--row-selected);
    border-color: color-mix(in oklab, var(--accent) 28%, transparent);
  }
  .tool-icon {
    width: 16px;
    height: 16px;
    display: inline-grid;
    place-items: center;
    color: color-mix(in oklab, currentColor 78%, var(--accent));
  }
  .tool-icon svg {
    width: 16px;
    height: 16px;
    display: block;
  }
  .tool-icon.activity {
    color: color-mix(in oklab, mediumseagreen 70%, var(--fg));
  }
  .tool-icon.audit {
    color: color-mix(in oklab, var(--accent) 78%, var(--fg));
  }
  .tool-row.active .tool-icon,
  .tool-row:hover .tool-icon {
    color: var(--accent);
  }
</style>
