<script lang="ts">
  import { onMount } from 'svelte';
  import { prefs } from '$lib/prefs';
  import { api, type AuditEntry } from '$lib/api';
  import { formatDate } from '$lib/format';

  /// Polling cadence. Longer than the scheduler's 60s sync interval so
  /// we don't miss a quiet window, but short enough to feel live. The
  /// ticker only *displays* new events — existing history never plays.
  const POLL_MS = 15_000;

  let enabled = $state(false);
  // The highest id we've seen. Anything newer gets pushed into the queue.
  let lastId = $state<number | null>(null);
  // FIFO — each event plays once then pops.
  let queue = $state<AuditEntry[]>([]);
  let current = $state<AuditEntry | null>(null);
  let pollHandle: ReturnType<typeof setInterval> | null = null;
  // Re-bump key on every entry so Svelte restarts the CSS animation.
  let animKey = $state(0);

  $effect(() => {
    const unsub = prefs.subscribe((p) => {
      enabled = p.eventTicker;
    });
    return unsub;
  });

  async function primeBaseline() {
    try {
      const top = await api.auditLog({ limit: 1 });
      lastId = top[0]?.id ?? 0;
    } catch {
      lastId = 0;
    }
  }

  async function poll() {
    if (!enabled || lastId === null) return;
    try {
      const batch = await api.auditLog({ limit: 30 });
      // API returns newest-first. Reverse so we queue in chronological
      // order and oldest-unseen plays first.
      const fresh = batch
        .filter((e) => e.id > (lastId ?? 0))
        .sort((a, b) => a.id - b.id);
      if (fresh.length === 0) return;
      lastId = fresh[fresh.length - 1].id;
      queue = [...queue, ...fresh];
      tryAdvance();
    } catch {
      // Swallow — next poll will retry. Don't flash the user with
      // transient network errors.
    }
  }

  function tryAdvance() {
    if (current !== null) return;
    if (queue.length === 0) return;
    current = queue[0];
    queue = queue.slice(1);
    animKey += 1;
  }

  function onAnimationEnd() {
    current = null;
    tryAdvance();
  }

  onMount(() => {
    (async () => {
      // Wait until we know the baseline before polling — otherwise
      // every historical event would parade across the screen on first
      // load.
      await primeBaseline();
      if (!enabled) return;
      pollHandle = setInterval(poll, POLL_MS);
    })();
    return () => {
      if (pollHandle) clearInterval(pollHandle);
    };
  });

  // Start/stop polling as the setting flips during the session. Also
  // drop any queued events when disabled so re-enabling doesn't dump a
  // backlog.
  $effect(() => {
    if (enabled) {
      if (!pollHandle && lastId !== null) {
        pollHandle = setInterval(poll, POLL_MS);
      }
    } else {
      if (pollHandle) {
        clearInterval(pollHandle);
        pollHandle = null;
      }
      queue = [];
      current = null;
    }
  });

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
    folder_created: '📁',
    folder_renamed: '✎',
    folder_deleted: '🗑',
    sync_interval_changed: '⏱',
    sync_policy_changed: '🔄',
    archive_folder_changed: '📦',
    archive_strategy_changed: '📦',
    archive_enabled_changed: '📦',
    auto_archive_changed: '⟳',
    avatar_changed: '👤',
    trusted_device_added: '✓',
    trusted_device_revoked: '✗',
    trusted_devices_revoked_all: '✗',
    // Activity
    sync_started: '⟳',
    sync_completed: '✓',
    sync_error: '⚠',
    folder_sync_error: '⚠',
    smtp_send: '📤',
    smtp_error: '✗',
    imap_error: '✗',
    auto_archive_completed: '📦',
    auto_archive_error: '⚠'
  };
  function icon(t: string) {
    return ICONS[t] ?? '•';
  }
  function label(t: string) {
    return t.replace(/_/g, ' ').replace(/\b\w/g, (c) => c.toUpperCase());
  }
  function severity(e: AuditEntry): string {
    const t = e.event_type;
    if (t.includes('failed') || t.includes('error')) return 'warn';
    if (t === 'sync_completed' || t === 'smtp_send' || t === 'auto_archive_completed') return 'success';
    if (e.category === 'security') return 'elevated';
    return 'normal';
  }
</script>

{#if enabled && current}
  <div class="ticker" role="status" aria-live="polite">
    {#key animKey}
      <div
        class="item sev-{severity(current)}"
        onanimationend={onAnimationEnd}
      >
        <span class="icon">{icon(current.event_type)}</span>
        <span class="cat">{current.category}</span>
        <strong>{label(current.event_type)}</strong>
        {#if current.detail}
          <span class="detail">— {current.detail}</span>
        {/if}
        <span class="time">{formatDate(current.ts_utc)}</span>
      </div>
    {/key}
  </div>
{/if}

<style>
  .ticker {
    position: fixed;
    left: 0;
    right: 0;
    bottom: 0;
    height: 1.9rem;
    z-index: 90;
    overflow: hidden;
    pointer-events: none;
    background: color-mix(in oklab, var(--surface) 94%, transparent);
    border-top: 1px solid var(--border);
    backdrop-filter: blur(12px);
    -webkit-backdrop-filter: blur(12px);
    box-shadow: 0 -8px 24px rgba(0, 0, 0, 0.12);
    /* Respect iOS home indicator. */
    padding-bottom: env(safe-area-inset-bottom);
  }
  .item {
    position: absolute;
    top: 0;
    left: 0;
    height: 100%;
    display: inline-flex;
    align-items: center;
    gap: 0.55rem;
    padding: 0 1rem;
    font-size: 0.78rem;
    line-height: 1;
    white-space: nowrap;
    /* Start just off the right edge, end just past the left. Duration
       scaled so long detail strings stay readable without dragging. */
    animation: ticker-scroll 16s linear forwards;
  }
  @keyframes ticker-scroll {
    from {
      transform: translateX(100vw);
    }
    to {
      transform: translateX(-100%);
    }
  }
  .item strong {
    font-weight: 650;
  }
  .item .icon {
    font-size: 0.95rem;
  }
  .item .cat {
    font-size: 0.65rem;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    padding: 0.1rem 0.4rem;
    border-radius: 999px;
    background: color-mix(in oklab, currentColor 8%, transparent);
    opacity: 0.65;
  }
  .item .detail {
    opacity: 0.85;
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 0.72rem;
  }
  .item .time {
    opacity: 0.55;
    font-size: 0.7rem;
    margin-left: 0.4rem;
  }
  .item.sev-warn {
    color: color-mix(in oklab, crimson 70%, var(--fg) 30%);
  }
  .item.sev-success {
    color: color-mix(in oklab, seagreen 65%, var(--fg) 35%);
  }
  .item.sev-elevated {
    color: color-mix(in oklab, goldenrod 62%, var(--fg) 38%);
  }

  @media (max-width: 900px) {
    .ticker {
      height: 1.7rem;
    }
    .item {
      font-size: 0.72rem;
      padding: 0 0.65rem;
      gap: 0.4rem;
    }
    .item .cat {
      display: none;
    }
    @keyframes ticker-scroll {
      from {
        transform: translateX(100vw);
      }
      to {
        transform: translateX(-100%);
      }
    }
  }
</style>
