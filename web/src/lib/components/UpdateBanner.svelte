<script lang="ts">
  // Sticky top-of-app banner that appears when an update is
  // available. Clicking "Install" fires a confirmation modal and,
  // once confirmed, triggers /api/updates/apply directly from the
  // banner — no trip to Settings required. Still dismissible per
  // commit so users who aren't ready can ignore until the next push.

  import { onMount } from 'svelte';
  import {
    api,
    type UpdateCheckResult,
    type UpdateStatusResult
  } from '$lib/api';

  const POLL_MS = 30 * 60 * 1000;
  const DISMISSED_KEY = 'postern.update-banner.dismissed';

  let check = $state<UpdateCheckResult | null>(null);
  let dismissed = $state<string | null>(null);
  let confirmOpen = $state(false);
  let applyBusy = $state(false);
  let applyError = $state<string | null>(null);
  let status = $state<UpdateStatusResult | null>(null);
  let statusPoll: ReturnType<typeof setInterval> | null = null;

  // Tracks the moment the current user kicked off an install. Used to
  // drive the safety-net auto-reload: if the container restart takes
  // long enough that status polls 401 / time out entirely, we still
  // reload after SAFETY_RELOAD_MS so the user isn't stranded on a
  // stale "installing" banner.
  let installStartedAt = $state<number | null>(null);
  const SAFETY_RELOAD_MS = 4 * 60 * 1000;

  function hardReload() {
    // Navigate to root so the user lands on the login screen with the
    // new build's bundle. Reusing the current URL (even with a cache-
    // bust param) sometimes routed through cloudflared into a stale
    // path and hit a tunnel error page; the Settings → Updates path
    // sidesteps that by always going to '/' and we mirror that here.
    if (typeof window === 'undefined') return;
    try {
      window.location.replace('/');
    } catch {
      window.location.href = '/';
    }
  }

  $effect(() => {
    try { dismissed = localStorage.getItem(DISMISSED_KEY); } catch { /* SSR */ }
  });

  async function poll() {
    try {
      const r = await api.updatesCheck();
      check = r.update_available && r.latest_commit ? r : null;
    } catch {
      // No license, unreachable server, vault locked — stay quiet;
      // the full error surfaces in Settings → Updates where the
      // user can act on it.
      check = null;
    }
  }

  async function refreshStatus() {
    try { status = await api.updatesStatus(); } catch { /* noop */ }
  }

  function startStatusPoll() {
    if (statusPoll) return;
    statusPoll = setInterval(async () => {
      await refreshStatus();

      // When we see the update landed, reload the page. The new
      // container has the new web build baked in, so a full reload is
      // the only honest way to surface it — otherwise we'd be running
      // old JS against a new API.
      if (status?.state === 'success') {
        if (statusPoll) { clearInterval(statusPoll); statusPoll = null; }
        hardReload();
        return;
      }
      if (status?.state === 'failed') {
        if (statusPoll) { clearInterval(statusPoll); statusPoll = null; }
        return;
      }

      // Safety net: the container restart can disrupt status polling
      // just as the updater writes "success". If we've been installing
      // for a long time with no terminal state, force a reload
      // anyway — the user is more likely than not looking at a stale
      // "installing" banner while the new container is healthy.
      if (
        installStartedAt &&
        Date.now() - installStartedAt > SAFETY_RELOAD_MS
      ) {
        if (statusPoll) { clearInterval(statusPoll); statusPoll = null; }
        hardReload();
      }
    }, 3000);
  }

  function openConfirm() {
    applyError = null;
    confirmOpen = true;
  }
  function closeConfirm() {
    if (applyBusy) return;
    confirmOpen = false;
  }

  async function installNow() {
    applyBusy = true;
    applyError = null;
    try {
      await api.updatesApply();
      confirmOpen = false;
      installStartedAt = Date.now();
      await refreshStatus();
      startStatusPoll();
    } catch (e) {
      applyError = e instanceof Error ? e.message : String(e);
    } finally {
      applyBusy = false;
    }
  }

  function dismiss() {
    const sha = check?.latest_commit;
    if (!sha) return;
    try { localStorage.setItem(DISMISSED_KEY, sha); } catch {}
    dismissed = sha;
  }

  let handle: ReturnType<typeof setInterval> | null = null;
  onMount(() => {
    void poll();
    void (async () => {
      await refreshStatus();
      // Pick up an in-progress install that started in another tab or
      // before this page loaded, so the banner reflects live state
      // instead of silently ignoring it. Use finished_at (if the
      // status file was last touched) as a reasonable surrogate for
      // install start — the safety-net timer only cares about a
      // baseline so it doesn't force-reload forever.
      if (status && (status.state === 'running' || status.trigger_pending)) {
        installStartedAt = status.finished_at ? status.finished_at * 1000 : Date.now();
        startStatusPoll();
      }
    })();
    handle = setInterval(poll, POLL_MS);
    return () => {
      if (handle) clearInterval(handle);
      if (statusPoll) clearInterval(statusPoll);
    };
  });

  let installing = $derived(
    status?.state === 'running' || status?.trigger_pending
  );
  let visible = $derived(
    installing ||
      (!!check && !!check.latest_commit && check.latest_commit !== dismissed)
  );

  // Elapsed-time readout for the installing banner. `now` ticks every
  // second while an install is running; the derived values read it
  // and Svelte recomputes. Drives the "HH:MM elapsed" label and the
  // stuck-hint that appears after 90s.
  let now = $state(Date.now());
  $effect(() => {
    if (!installing) return;
    const id = setInterval(() => (now = Date.now()), 1000);
    return () => clearInterval(id);
  });
  let elapsedSecs = $derived(
    installStartedAt ? Math.max(0, Math.floor((now - installStartedAt) / 1000)) : 0
  );
  let elapsedLabel = $derived.by(() => {
    if (!installStartedAt) return '';
    const s = elapsedSecs;
    if (s < 60) return `${s}s`;
    const m = Math.floor(s / 60);
    const r = s % 60;
    return `${m}m ${r}s`;
  });
  let showStuckHint = $derived(installing && elapsedSecs >= 90);

  function fmtBytes(n: number | null | undefined): string {
    if (!n) return '';
    const mb = n / (1024 * 1024);
    return mb >= 1 ? `${mb.toFixed(1)} MB` : `${(n / 1024).toFixed(1)} KB`;
  }
</script>

{#if visible}
  <div class="banner" class:installing>
    {#if installing}
      <span class="spinner" aria-hidden="true"></span>
      <div class="body">
        <strong>Installing update…</strong>
        {#if status?.message}<span class="notes">{status.message}</span>{/if}
        {#if installStartedAt}<span class="elapsed" title="Elapsed">{elapsedLabel}</span>{/if}
        {#if showStuckHint}
          <span class="stuck-hint">
            · Taking a while? The page will reload automatically when it's ready.
            If it seems stuck, press <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>R</kbd>
            (<kbd>⌘</kbd>+<kbd>Shift</kbd>+<kbd>R</kbd> on Mac).
          </span>
        {/if}
      </div>
      <button class="secondary" onclick={hardReload} title="Reload now">Reload</button>
    {:else if status?.state === 'failed'}
      <span class="dot failed" aria-hidden="true"></span>
      <div class="body">
        <strong>Update failed</strong>
        {#if status.message}<span class="notes">— {status.message}</span>{/if}
      </div>
      <button class="cta" onclick={openConfirm}>Try again</button>
      <button class="close" aria-label="Dismiss" onclick={dismiss}>×</button>
    {:else if check}
      <span class="dot" aria-hidden="true"></span>
      <div class="body">
        <strong>Update available:</strong>
        <span class="commit">{check.latest_commit}</span>
        {#if check.release_notes}
          <span class="notes">— {check.release_notes}</span>
        {/if}
      </div>
      <a class="secondary" href="/settings?tab=updates" title="Release details">Details</a>
      <button class="cta" onclick={openConfirm}>Update</button>
      <button class="close" aria-label="Dismiss" onclick={dismiss}>×</button>
    {/if}
  </div>
{/if}

{#if confirmOpen && check}
  <div
    class="modal-backdrop"
    role="presentation"
    onclick={closeConfirm}
    onkeydown={(e) => { if (e.key === 'Escape') closeConfirm(); }}
  >
    <div
      class="modal"
      role="dialog"
      aria-modal="true"
      aria-labelledby="banner-confirm-title"
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
    >
      <h3 id="banner-confirm-title">Install update to {check.latest_commit}?</h3>
      <p>
        Postern will download the new release, verify it, back up your
        database, then rebuild the container. The mail server will be
        unreachable for roughly 30–60 seconds while the new container
        starts. Your mail, vault key, and settings stay on disk.
      </p>
      {#if check.release_notes}
        <p class="notes small"><em>{check.release_notes}</em></p>
      {/if}
      {#if check.size_bytes}
        <p class="meta small">Download size: {fmtBytes(check.size_bytes)}</p>
      {/if}
      {#if applyError}
        <p class="err-bubble">⚠ {applyError}</p>
      {/if}
      <div class="modal-actions">
        <button class="btn" onclick={closeConfirm} disabled={applyBusy}>Cancel</button>
        <button class="btn primary" onclick={installNow} disabled={applyBusy}>
          {applyBusy ? 'Starting…' : 'Install now'}
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .banner {
    position: sticky;
    top: 0;
    z-index: 50;
    display: flex;
    align-items: center;
    gap: 0.65rem;
    padding: 0.55rem 0.9rem;
    background: color-mix(in oklab, var(--accent) 14%, var(--surface));
    color: var(--fg);
    border-bottom: 1px solid color-mix(in oklab, var(--accent) 40%, var(--border));
    font-size: 0.88rem;
  }
  .banner.installing {
    background: color-mix(in oklab, var(--accent) 10%, var(--surface));
  }
  .dot {
    width: 0.5rem;
    height: 0.5rem;
    border-radius: 50%;
    background: var(--accent);
    box-shadow: 0 0 0 3px color-mix(in oklab, var(--accent) 22%, transparent);
    flex-shrink: 0;
  }
  .dot.failed {
    background: crimson;
    box-shadow: 0 0 0 3px color-mix(in oklab, crimson 22%, transparent);
  }
  .spinner {
    width: 0.9rem;
    height: 0.9rem;
    border: 2px solid color-mix(in oklab, var(--accent) 30%, transparent);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: spin 0.9s linear infinite;
    flex-shrink: 0;
  }
  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  .body {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .commit {
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 0.82rem;
    padding: 0.05rem 0.4rem;
    border-radius: 0.3rem;
    background: color-mix(in oklab, var(--accent) 22%, transparent);
    color: color-mix(in oklab, var(--accent) 75%, var(--fg) 25%);
    margin: 0 0.35rem;
  }
  .notes { color: var(--muted); }

  .elapsed {
    color: var(--muted);
    font-variant-numeric: tabular-nums;
    font-size: 0.78rem;
    margin-left: 0.45rem;
    padding: 0.05rem 0.35rem;
    border-radius: 0.3rem;
    background: color-mix(in oklab, currentColor 6%, transparent);
  }

  .stuck-hint {
    color: var(--muted);
    font-size: 0.82rem;
    margin-left: 0.35rem;
    white-space: normal;
  }
  .stuck-hint kbd {
    display: inline-block;
    padding: 0 0.35rem;
    border-radius: 0.25rem;
    border: 1px solid var(--border);
    background: var(--surface-2);
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 0.75rem;
    color: var(--fg);
    margin: 0 0.08rem;
  }

  .cta {
    padding: 0.32rem 0.85rem;
    border-radius: 0.4rem;
    background: var(--accent);
    color: white;
    border: 0;
    font-weight: 600;
    font-size: 0.84rem;
    white-space: nowrap;
    cursor: pointer;
    font: inherit;
    font-size: 0.84rem;
  }
  .cta:hover { filter: brightness(0.94); }

  .secondary {
    padding: 0.3rem 0.7rem;
    border-radius: 0.4rem;
    background: transparent;
    color: var(--fg);
    text-decoration: none;
    font-size: 0.82rem;
    border: 1px solid color-mix(in oklab, var(--accent) 40%, var(--border));
    white-space: nowrap;
  }
  .secondary:hover {
    background: color-mix(in oklab, var(--accent) 10%, transparent);
  }

  .close {
    background: transparent;
    border: 0;
    color: var(--muted);
    font-size: 1.1rem;
    line-height: 1;
    padding: 0.2rem 0.4rem;
    cursor: pointer;
    border-radius: 0.3rem;
  }
  .close:hover {
    background: color-mix(in oklab, currentColor 10%, transparent);
    color: var(--fg);
  }

  /* --- modal --- */
  .modal-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.45);
    backdrop-filter: blur(3px);
    display: grid;
    place-items: center;
    z-index: 200;
    padding: 1rem;
  }
  .modal {
    max-width: 34rem;
    width: 100%;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 0.9rem;
    padding: 1.2rem 1.4rem 1.3rem;
    box-shadow: 0 24px 64px rgba(0, 0, 0, 0.3);
    color: var(--fg);
  }
  .modal h3 { margin: 0 0 0.7rem; font-size: 1.05rem; }
  .modal p { margin: 0 0 0.7rem; color: var(--fg); line-height: 1.5; }
  .modal .small { font-size: 0.85rem; }
  .modal .meta { color: var(--muted); }

  .btn {
    padding: 0.45rem 0.9rem;
    border: 1px solid var(--border);
    background: var(--surface-2);
    color: var(--fg);
    border-radius: 0.5rem;
    cursor: pointer;
    font: inherit;
    font-size: 0.88rem;
  }
  .btn.primary {
    background: var(--accent);
    color: white;
    border-color: transparent;
    font-weight: 600;
  }
  .btn:disabled { opacity: 0.55; cursor: progress; }

  .err-bubble {
    padding: 0.55rem 0.85rem;
    background: color-mix(in oklab, crimson 10%, var(--surface));
    color: color-mix(in oklab, crimson 70%, var(--fg) 30%);
    border-radius: 0.5rem;
    font-size: 0.88rem;
    margin: 0 0 0.6rem;
  }

  .modal-actions {
    display: flex;
    justify-content: flex-end;
    gap: 0.55rem;
    margin-top: 0.5rem;
  }

  @media (max-width: 900px) {
    .notes { display: none; }
    .secondary { display: none; } /* keep only primary Update on narrow screens */
  }
</style>
