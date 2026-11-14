<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import { prefs } from '$lib/prefs';
  import { UNIFIED_LABELS } from '$lib/unified';
  import {
    toasts,
    notifyNewMail,
    dismissToast
  } from '$lib/notifications';

  // 30s matches the inbox's own silent-poll cadence. Shorter than the
  // server-side sync interval (60s default) so we never miss a cycle,
  // but not aggressive enough to bother the API.
  const POLL_MS = 30_000;

  // Per-account baseline of "inbox unread" we've already observed.
  // First poll primes this map and intentionally fires no notification.
  // Subsequent polls compare and fire on positive delta only.
  let baseline: Map<number, number> | null = null;
  let enabled = false;
  let pollHandle: ReturnType<typeof setInterval> | null = null;

  $effect(() => {
    const unsub = prefs.subscribe((p) => {
      enabled = p.notifyNewMail;
    });
    return unsub;
  });

  function inboxUnreadFor(accountId: number, accounts: Awaited<ReturnType<typeof api.folders>>['accounts']): number {
    const inboxNames = new Set(UNIFIED_LABELS.inbox);
    const acct = accounts.find((a) => a.account_id === accountId);
    if (!acct) return 0;
    let sum = 0;
    for (const f of [...acct.system, ...acct.user]) {
      if (inboxNames.has(f.name)) sum += f.unread;
    }
    return sum;
  }

  function displayFrom(fromAddr: string | null): string | undefined {
    // "Name <user@host>" → "Name". Bare "user@host" passes through.
    if (!fromAddr) return undefined;
    const m = fromAddr.match(/^\s*"?([^"<]+?)"?\s*<[^>]+>\s*$/);
    const picked = (m ? m[1] : fromAddr).trim();
    return picked || undefined;
  }

  async function topInboxPreview(delta: number): Promise<{ subject?: string; from?: string }> {
    // One lightweight peek at the top of the unified inbox so the toast
    // (and OS banner, if enabled) can say something more useful than a
    // bare count. Best-effort — on failure we notify with counts only.
    try {
      const rows = await api.listMessages({
        labels: UNIFIED_LABELS.inbox,
        limit: Math.min(delta, 1),
        offset: 0,
        sort: 'date_desc'
      });
      const top = rows[0];
      if (!top) return {};
      return {
        subject: top.subject?.trim() || undefined,
        from: displayFrom(top.from_addr)
      };
    } catch {
      return {};
    }
  }

  async function poll() {
    if (!enabled) return;
    // Deliberately NOT gating on document.visibilityState — the whole
    // point of notifications is to surface new mail while the user is
    // on another tab. Title flash and OS toast only mean anything when
    // the tab is backgrounded, so we must keep polling then.
    let resp;
    try {
      resp = await api.folders();
    } catch {
      // Locked vault, transient network, etc. — leave baseline intact
      // so we don't double-notify when access is restored.
      return;
    }
    const fresh = new Map<number, number>();
    for (const a of resp.accounts) {
      fresh.set(a.account_id, inboxUnreadFor(a.account_id, resp.accounts));
    }
    if (baseline === null) {
      baseline = fresh;
      return;
    }
    let totalDelta = 0;
    for (const [accountId, count] of fresh.entries()) {
      const prev = baseline.get(accountId) ?? count;
      if (count > prev) totalDelta += count - prev;
    }
    baseline = fresh;
    if (totalDelta > 0) {
      const preview = await topInboxPreview(totalDelta);
      notifyNewMail({ count: totalDelta, subject: preview.subject, from: preview.from });
    }
  }

  onMount(() => {
    // Kick a priming poll immediately so baseline is ready as soon as
    // possible; then settle into the regular cadence.
    void poll();
    pollHandle = setInterval(poll, POLL_MS);

    // Re-check when the tab becomes visible again — users typically
    // want the freshest state right after they refocus.
    const onVisible = () => {
      if (document.visibilityState === 'visible') void poll();
    };
    document.addEventListener('visibilitychange', onVisible);

    return () => {
      if (pollHandle) clearInterval(pollHandle);
      document.removeEventListener('visibilitychange', onVisible);
    };
  });
</script>

{#if $toasts.length > 0}
  <div class="stack" aria-live="polite" aria-label="Notifications">
    {#each $toasts as t (t.id)}
      <button
        class="toast"
        onclick={() => {
          dismissToast(t.id);
          if (typeof window !== 'undefined') window.location.href = '/inbox';
        }}
      >
        <span class="dot" aria-hidden="true"></span>
        <div class="body">
          <strong class="title">
            {t.count === 1 ? 'New mail' : `${t.count} new messages`}
          </strong>
          {#if t.subject || t.from}
            <span class="preview">
              {#if t.from}<em class="from">{t.from}</em>{/if}
              {#if t.subject}<span class="subject">{t.subject}</span>{/if}
            </span>
          {/if}
        </div>
        <span
          class="close"
          role="presentation"
          aria-hidden="true"
          onclick={(e) => {
            e.stopPropagation();
            dismissToast(t.id);
          }}
        >×</span>
      </button>
    {/each}
  </div>
{/if}

<style>
  .stack {
    position: fixed;
    top: max(1rem, env(safe-area-inset-top));
    right: 1rem;
    z-index: 1000;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    max-width: min(22rem, calc(100vw - 2rem));
    pointer-events: none;
  }

  .toast {
    pointer-events: auto;
    display: grid;
    grid-template-columns: auto 1fr auto;
    align-items: start;
    gap: 0.65rem;
    padding: 0.7rem 0.85rem;
    border-radius: 0.65rem;
    background: color-mix(in oklab, var(--surface) 94%, transparent);
    color: var(--fg);
    border: 1px solid var(--border);
    box-shadow:
      0 10px 24px rgba(0, 0, 0, 0.22),
      0 2px 4px rgba(0, 0, 0, 0.06);
    backdrop-filter: blur(14px);
    -webkit-backdrop-filter: blur(14px);
    font: inherit;
    text-align: left;
    cursor: pointer;
    animation: toast-in 220ms cubic-bezier(0.16, 1, 0.3, 1);
  }
  .toast:hover {
    border-color: color-mix(in oklab, var(--accent) 40%, var(--border));
  }

  .dot {
    width: 0.55rem;
    height: 0.55rem;
    margin-top: 0.3rem;
    border-radius: 50%;
    background: var(--accent);
    box-shadow: 0 0 0 3px color-mix(in oklab, var(--accent) 22%, transparent);
  }

  .body {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
    min-width: 0;
  }

  .title {
    font-size: 0.92rem;
    font-weight: 650;
    line-height: 1.2;
  }

  .preview {
    display: flex;
    flex-direction: column;
    gap: 0.05rem;
    font-size: 0.82rem;
    color: var(--muted);
    min-width: 0;
  }
  .from {
    font-style: normal;
    font-weight: 600;
    color: color-mix(in oklab, var(--fg) 85%, transparent);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .subject {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .close {
    align-self: start;
    line-height: 1;
    font-size: 1rem;
    color: var(--muted);
    padding: 0.1rem 0.3rem;
    border-radius: 0.3rem;
    cursor: pointer;
  }
  .close:hover {
    background: color-mix(in oklab, currentColor 10%, transparent);
    color: var(--fg);
  }

  @keyframes toast-in {
    from {
      opacity: 0;
      transform: translateX(12px);
    }
    to {
      opacity: 1;
      transform: translateX(0);
    }
  }

  @media (max-width: 900px) {
    .stack {
      top: max(0.5rem, env(safe-area-inset-top));
      right: 0.5rem;
      left: 0.5rem;
      max-width: unset;
    }
  }
</style>
