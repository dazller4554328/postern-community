<script lang="ts">
  import { onDestroy, onMount, tick } from 'svelte';
  import { goto } from '$app/navigation';
  import {
    api,
    type AiCitation,
    type AiStatus,
    type PrivacyPosture
  } from '$lib/api';
  import { formatDate } from '$lib/format';

  // Account scope flows in from the parent (the inbox knows which
  // mailbox is currently selected). Null = unified, all accounts.
  // `fullscreen` swaps the panel-with-resize-handle layout for a
  // 100%-of-parent-height layout — used on mobile and the /datas
  // route, where there's no inbox underneath to share space with.
  let {
    accountId,
    onClose,
    fullscreen = false
  }: { accountId: number | null; onClose: () => void; fullscreen?: boolean } =
    $props();

  // One-shot status fetch. Surfaces privacy posture before the user
  // even types so the prompt header reflects "Local only" vs cloud.
  let status = $state<AiStatus | null>(null);
  let statusLoaded = $state(false);

  type Turn = {
    id: number;
    question: string;
    answer: string | null;
    citations: AiCitation[];
    posture: PrivacyPosture | null;
    error: string | null;
    pending: boolean;
    elapsedMs: number;
  };

  let turns = $state<Turn[]>([]);
  let nextTurnId = 0;
  let input = $state('');
  let pending = $state(false);
  let scrollEl: HTMLDivElement | null = $state(null);
  let inputEl: HTMLTextAreaElement | null = $state(null);

  // ── Resize state ─────────────────────────────────────────────
  // Persisted height in pixels. We store on document.documentElement
  // as a CSS variable so the .shell grid-template-rows can react;
  // the inbox doesn't have to know anything about the AskBox's state.
  const STORAGE_KEY = 'postern.askbox.height';
  const MIN_PX = 140;
  let height = $state<number>(loadHeight());

  function loadHeight(): number {
    if (typeof window === 'undefined') return 360;
    const raw = window.localStorage.getItem(STORAGE_KEY);
    const parsed = raw ? parseInt(raw, 10) : NaN;
    if (!Number.isFinite(parsed)) return 360;
    return clampHeight(parsed);
  }

  function clampHeight(px: number): number {
    if (typeof window === 'undefined') return px;
    const max = Math.floor(window.innerHeight * 0.45);
    return Math.max(MIN_PX, Math.min(max, px));
  }

  function saveHeight(px: number): void {
    if (typeof window === 'undefined') return;
    window.localStorage.setItem(STORAGE_KEY, String(Math.round(px)));
  }

  // Mirror current height into the document so the shell grid sizes
  // its third row to match. Cleared on destroy below so closing the
  // panel collapses the slot back to 0.
  $effect(() => {
    if (typeof document === 'undefined') return;
    document.documentElement.style.setProperty('--askbox-h', `${height}px`);
  });

  onDestroy(() => {
    if (typeof document !== 'undefined') {
      document.documentElement.style.removeProperty('--askbox-h');
    }
  });

  // Pointer-driven resize. Capture the pointer on the handle so a
  // mouse leaving the handle still drives the drag — otherwise on a
  // fast vertical drag the cursor outpaces the 6px handle and the
  // resize stalls. Also pin user-select: none on body during the
  // drag so the inbox text doesn't accidentally select.
  let dragging = $state(false);
  let dragStartY = 0;
  let dragStartHeight = 0;

  function onHandlePointerDown(e: PointerEvent) {
    if (e.button !== 0) return;
    e.preventDefault();
    dragging = true;
    dragStartY = e.clientY;
    dragStartHeight = height;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    document.body.style.userSelect = 'none';
    document.body.style.cursor = 'ns-resize';
  }

  function onHandlePointerMove(e: PointerEvent) {
    if (!dragging) return;
    // Drag UP grows the panel, drag DOWN shrinks it.
    const delta = dragStartY - e.clientY;
    height = clampHeight(dragStartHeight + delta);
  }

  function onHandlePointerUp(e: PointerEvent) {
    if (!dragging) return;
    dragging = false;
    (e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
    document.body.style.removeProperty('user-select');
    document.body.style.removeProperty('cursor');
    saveHeight(height);
  }

  // Re-clamp on viewport resize so a previously-saved 600px height
  // doesn't push past 85vh after the user switches to a small window.
  function onWindowResize() {
    height = clampHeight(height);
  }

  onMount(async () => {
    try {
      status = await api.aiStatus();
    } catch (e) {
      // Status is best-effort; failure here just leaves the UI in
      // its "checking" state until the user tries to ask.
      console.warn('ai status fetch failed', e);
    } finally {
      statusLoaded = true;
    }
    inputEl?.focus();
    autosizeInput();
    window.addEventListener('resize', onWindowResize);
  });

  onDestroy(() => {
    window.removeEventListener('resize', onWindowResize);
  });

  function postureLabel(p: PrivacyPosture | null | undefined): string {
    if (!p) return 'unknown';
    if (p === 'local_only') return 'Local only';
    if (p === 'user_controlled_remote') return 'Your remote box';
    return 'Third-party cloud';
  }

  function postureClass(p: PrivacyPosture | null | undefined): string {
    if (!p) return '';
    if (p === 'local_only') return 'posture-local';
    if (p === 'user_controlled_remote') return 'posture-self';
    return 'posture-cloud';
  }

  async function ask() {
    const question = input.trim();
    if (!question || pending) return;
    if (!status?.enabled) {
      // Surface the operator hint when AI isn't configured.
      turns = [
        ...turns,
        {
          id: nextTurnId++,
          question,
          answer: null,
          citations: [],
          posture: null,
          error:
            'AI is not configured. Open Settings → AI to pick a provider.',
          pending: false,
          elapsedMs: 0
        }
      ];
      input = '';
      autosizeInput();
      await scrollToEnd();
      return;
    }

    const turn: Turn = {
      id: nextTurnId++,
      question,
      answer: '',
      citations: [],
      posture: status?.privacy_posture ?? null,
      error: null,
      pending: true,
      elapsedMs: 0
    };
    turns = [...turns, turn];
    const turnId = turn.id;
    input = '';
    pending = true;
    autosizeInput();
    await scrollToEnd();

    // Stream tokens via the NDJSON endpoint. Each line is a JSON
    // object with a `type` field: meta (citations + posture sent
    // up front), token (incremental answer text), done (final
    // timing), error (terminal failure). Cloudflare's 100-second
    // origin timeout doesn't trip because bytes flow continuously.
    try {
      const resp = await fetch('/api/ai/ask/stream', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ question, account_id: accountId })
      });
      if (!resp.ok || !resp.body) {
        const errText = await resp.text().catch(() => 'streaming request failed');
        throw new Error(errText);
      }
      const reader = resp.body.getReader();
      const decoder = new TextDecoder();
      let buf = '';
      while (true) {
        const { value, done } = await reader.read();
        if (done) break;
        buf += decoder.decode(value, { stream: true });
        let nl = buf.indexOf('\n');
        while (nl !== -1) {
          const line = buf.slice(0, nl).trim();
          buf = buf.slice(nl + 1);
          if (line) handleStreamLine(turnId, line);
          nl = buf.indexOf('\n');
        }
      }
    } catch (e) {
      turns = turns.map((t) =>
        t.id === turnId
          ? {
              ...t,
              error: e instanceof Error ? e.message : String(e),
              pending: false
            }
          : t
      );
    } finally {
      pending = false;
      // Stamp pending=false even on the success path in case the
      // stream ended without a `done` event.
      turns = turns.map((t) => (t.id === turnId ? { ...t, pending: false } : t));
      await scrollToEnd();
      inputEl?.focus();
    }
  }

  function handleStreamLine(turnId: number, line: string): void {
    let evt: { type: string; [key: string]: unknown };
    try {
      evt = JSON.parse(line);
    } catch {
      return; // skip malformed line
    }
    if (evt.type === 'meta') {
      const cits = (evt.citations as AiCitation[]) ?? [];
      const posture = (evt.privacy_posture as PrivacyPosture) ?? null;
      turns = turns.map((t) =>
        t.id === turnId ? { ...t, citations: cits, posture } : t
      );
      void scrollToEnd();
    } else if (evt.type === 'token') {
      const content = (evt.content as string) ?? '';
      turns = turns.map((t) =>
        t.id === turnId ? { ...t, answer: (t.answer ?? '') + content } : t
      );
      void scrollToEnd();
    } else if (evt.type === 'done') {
      const elapsedMs = Number(evt.elapsed_ms ?? 0);
      turns = turns.map((t) =>
        t.id === turnId ? { ...t, elapsedMs, pending: false } : t
      );
    } else if (evt.type === 'error') {
      const message = (evt.message as string) ?? 'stream error';
      turns = turns.map((t) =>
        t.id === turnId ? { ...t, error: message, pending: false } : t
      );
    }
  }

  async function scrollToEnd(): Promise<void> {
    await tick();
    if (scrollEl) {
      scrollEl.scrollTop = scrollEl.scrollHeight;
    }
  }

  // Auto-grow the prompt textarea up to ~6 lines as the user types.
  // Beyond that we let it scroll inside itself so the prompt strip
  // doesn't take over the whole panel.
  function autosizeInput() {
    if (!inputEl) return;
    inputEl.style.height = 'auto';
    const max = parseFloat(getComputedStyle(inputEl).lineHeight) * 6;
    inputEl.style.height = `${Math.min(inputEl.scrollHeight, max)}px`;
  }

  function onKey(e: KeyboardEvent) {
    // Enter sends; Shift+Enter inserts newline. Same convention as
    // every modern terminal-style chat (Claude Code, ChatGPT, etc).
    // Cmd/Ctrl+Enter is the legacy alias and still works.
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      void ask();
      return;
    }
    if (e.key === 'Escape') {
      e.preventDefault();
      onClose();
    }
  }

  function openCitation(c: AiCitation) {
    void goto(`/inbox/m/${c.message_id}`);
  }

  /// Click anywhere in the body — even on a previous answer — and
  /// the prompt grabs focus, like a real terminal. Skip the focus
  /// grab when the user has an active text selection (otherwise
  /// focusing the textarea collapses the selection and you can't
  /// copy a previous answer or error). Also skip clicks on
  /// interactive elements (citation chips, links).
  function onBodyClick(e: MouseEvent) {
    const target = e.target as HTMLElement | null;
    if (target?.closest('button, a, textarea')) return;
    const sel = window.getSelection();
    if (sel && !sel.isCollapsed && sel.toString().trim().length > 0) {
      return;
    }
    inputEl?.focus();
  }
</script>

<aside
  class="askbox"
  class:dragging
  class:fullscreen
  style:height={fullscreen ? '100%' : `${height}px`}
  role="region"
  aria-label="Datas — your private inbox memory"
>
  {#if !fullscreen}
    <!-- Drag handle on the very top edge. Pointer events captured
         on the handle so a fast drag doesn't lose tracking. The
         handle is hidden in fullscreen mode (mobile + /datas route)
         since the panel fills its parent already. -->
    <div
      class="askbox-resize-handle"
      role="separator"
      aria-orientation="horizontal"
      aria-label="Resize Datas panel"
      onpointerdown={onHandlePointerDown}
      onpointermove={onHandlePointerMove}
      onpointerup={onHandlePointerUp}
      onpointercancel={onHandlePointerUp}
    ></div>
  {/if}

  <header class="askbox-bar">
    <span class="askbox-mark" aria-hidden="true">✶</span>
    <strong>Datas</strong>
    <span class="askbox-tagline">the man who knew everything</span>
    {#if statusLoaded && status?.provider}
      <span class="askbox-provider">{status.provider}</span>
    {/if}
    {#if statusLoaded && status?.privacy_posture}
      <span class="posture {postureClass(status.privacy_posture)}">
        {postureLabel(status.privacy_posture)}
      </span>
    {/if}
    <span class="askbox-spacer"></span>
    <span class="askbox-hint">Enter to send · Shift+Enter newline · Esc closes</span>
    <button
      class="askbox-close"
      type="button"
      onclick={onClose}
      aria-label="Close Datas panel"
      title="Close (Esc)"
    >✕</button>
  </header>

  <!-- Single scrollback area. No separate footer textbox — the
       prompt is rendered as the last "line" so the whole panel
       reads like one terminal. Click anywhere to focus the prompt. -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    class="askbox-term"
    bind:this={scrollEl}
    onclick={onBodyClick}
  >
    {#if !statusLoaded}
      <div class="line muted">checking ai status…</div>
    {:else if !status?.enabled}
      <div class="line muted">AI is not configured.</div>
      <div class="line muted">Open Settings → AI to pick a provider (Ollama, Claude, OpenAI, Grok, …).</div>
    {:else if turns.length === 0}
      <div class="line muted">Ask Datas anything about your mail. Examples:</div>
      <div class="line muted">  · When did Joe last pay via PayPal and how much?</div>
      <div class="line muted">  · Summarise my conversation with Sarah about Q3 budget.</div>
      <div class="line muted">  · Did the landlord reply about the leak yet?</div>
    {/if}

    {#each turns as t (t.id)}
      <div class="turn">
        <div class="line line-q">
          <span class="prompt-mark" aria-hidden="true">❯</span>
          <span class="line-q-text">{t.question}</span>
        </div>
        {#if t.pending}
          <div class="line line-a pending">
            <span class="dots">●●●</span> thinking…
          </div>
        {:else if t.error}
          <div class="line line-a error">⚠ {t.error}</div>
        {:else if t.answer}
          <div class="line line-a">{t.answer}</div>
          {#if t.citations.length > 0}
            <div class="line line-meta">
              <span class="meta-label">sources:</span>
              {#each t.citations as c (c.message_id)}
                <button
                  type="button"
                  class="citation"
                  title={c.subject ?? ''}
                  onclick={() => openCitation(c)}
                >
                  {c.from_addr ?? '(unknown)'} · {formatDate(c.date_utc)}
                </button>
              {/each}
            </div>
          {/if}
          <div class="line line-meta">
            <span class="meta-label">took</span> {(t.elapsedMs / 1000).toFixed(1)}s
            {#if t.posture}
              · <span class="posture-inline {postureClass(t.posture)}">
                {postureLabel(t.posture)}
              </span>
            {/if}
          </div>
        {/if}
      </div>
    {/each}

    <!-- The live prompt — last "line" of the terminal. The
         textarea is styled to sit flush in the scrollback so the
         whole panel reads as a single window. -->
    <div class="line line-prompt" class:disabled={pending || (statusLoaded && !status?.enabled)}>
      <span class="prompt-mark" aria-hidden="true">❯</span>
      <textarea
        bind:this={inputEl}
        bind:value={input}
        onkeydown={onKey}
        oninput={autosizeInput}
        placeholder={pending ? 'streaming…' : (statusLoaded && !status?.enabled ? 'ai disabled' : 'type a question and hit Enter')}
        rows="1"
        spellcheck="false"
        autocomplete="off"
        autocapitalize="sentences"
        disabled={pending || (statusLoaded && !status?.enabled)}
        aria-label="Ask a question"
      ></textarea>
    </div>
  </div>
</aside>

<style>
  .askbox {
    /* Sits in the third row of .shell. Height is driven by the
       inline style attribute so a drag adjusts the row height
       directly — no recompute on the parent. */
    display: flex;
    flex-direction: column;
    min-height: 140px;
    border: 1px solid var(--border);
    border-radius: 1.25rem;
    background:
      linear-gradient(180deg,
        color-mix(in oklab, var(--surface-2) 90%, transparent),
        color-mix(in oklab, var(--surface) 92%, transparent));
    box-shadow:
      0 1px 0 rgba(255, 255, 255, 0.04) inset,
      0 -10px 30px rgba(0, 0, 0, 0.10);
    overflow: hidden;
    /* Position relative so the absolute resize handle anchors here. */
    position: relative;
  }
  .askbox.dragging {
    user-select: none;
  }
  /* Fullscreen mode (mobile + /datas route): no drag handle, fills
     the parent flexbox-style. Drop the rounded corners and outer
     border so it reads as a page rather than a floating panel. */
  .askbox.fullscreen {
    flex: 1 1 auto;
    border-radius: 0;
    border: 0;
    box-shadow: none;
    min-height: 0;
  }

  /* Resize handle — a thin strip across the top edge. Visually
     subtle until hovered, then more prominent. */
  .askbox-resize-handle {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    height: 8px;
    cursor: ns-resize;
    z-index: 5;
    /* The grabber pill, drawn with ::before so the strip itself is
       fully clickable. */
  }
  .askbox-resize-handle::before {
    content: '';
    position: absolute;
    top: 3px;
    left: 50%;
    transform: translateX(-50%);
    width: 44px;
    height: 3px;
    border-radius: 999px;
    background: color-mix(in oklab, currentColor 20%, transparent);
    transition: background 120ms ease, width 120ms ease;
  }
  .askbox-resize-handle:hover::before,
  .askbox.dragging .askbox-resize-handle::before {
    background: color-mix(in oklab, var(--accent) 60%, transparent);
    width: 60px;
  }

  .askbox-bar {
    display: flex;
    align-items: center;
    gap: 0.55rem;
    padding: 0.5rem 0.85rem 0.45rem;
    /* Make space for the resize handle on top so the title bar
       doesn't sit under the grab strip. */
    padding-top: 0.85rem;
    border-bottom: 1px solid color-mix(in oklab, var(--border) 60%, transparent);
    font-size: 0.82rem;
    flex-shrink: 0;
  }
  .askbox-mark {
    color: var(--accent);
    font-size: 0.95rem;
  }
  .askbox-tagline {
    font-size: 0.72rem;
    color: var(--muted);
    font-style: italic;
    /* The Hitchcock nod is for people who notice; doesn't shout. */
    letter-spacing: 0.01em;
  }
  .askbox-provider {
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 0.74rem;
    padding: 0.1rem 0.45rem;
    border-radius: 0.4rem;
    background: color-mix(in oklab, currentColor 7%, transparent);
    color: var(--muted);
  }
  .askbox-spacer { flex: 1; }
  .askbox-hint {
    font-size: 0.74rem;
    color: var(--muted);
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .askbox-close {
    background: transparent;
    border: 0;
    color: inherit;
    opacity: 0.55;
    font-size: 1rem;
    cursor: pointer;
    padding: 0.2rem 0.45rem;
    border-radius: 0.35rem;
    line-height: 1;
  }
  .askbox-close:hover {
    opacity: 1;
    background: color-mix(in oklab, currentColor 8%, transparent);
  }

  /* The terminal scrollback — single scroll container, monospace
     font, all lines render here including the live prompt. */
  .askbox-term {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 0.65rem 0.95rem 0.85rem;
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 0.85rem;
    line-height: 1.55;
    color: var(--fg);
    cursor: text;
    /* Subtle terminal feel — slightly tinted background distinct
       from the .shell surface so the panel reads as its own
       device. Thin scrollbar so it doesn't fight the terminal
       aesthetic. */
    background:
      linear-gradient(180deg,
        color-mix(in oklab, var(--surface) 96%, black 4%),
        color-mix(in oklab, var(--surface) 99%, black 1%));
    scrollbar-width: thin;
    scrollbar-color: color-mix(in oklab, currentColor 16%, transparent) transparent;
  }
  .askbox-term::-webkit-scrollbar {
    width: 8px;
  }
  .askbox-term::-webkit-scrollbar-thumb {
    background: color-mix(in oklab, currentColor 16%, transparent);
    border-radius: 999px;
  }

  .line {
    display: block;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    margin: 0;
    padding: 0;
  }
  .line.muted { color: var(--muted); }

  .turn {
    /* Slim spacing between turns — a blank line equivalent at the
       end of the answer block for breathing room. */
    margin: 0 0 0.85rem;
  }

  .line-q {
    color: color-mix(in oklab, var(--accent) 75%, var(--fg));
    font-weight: 600;
    display: flex;
    gap: 0.5rem;
    align-items: flex-start;
  }
  .line-q-text {
    flex: 1;
    min-width: 0;
    white-space: pre-wrap;
  }

  .prompt-mark {
    flex-shrink: 0;
    width: 1ch;
    color: var(--accent);
    font-weight: 700;
    user-select: none;
  }

  .line-a {
    /* Indented under the prompt mark so the answer hangs visually
       below the question without needing a separate bubble. */
    padding-left: calc(1ch + 0.5rem);
    white-space: pre-wrap;
  }
  .line-a.pending {
    color: var(--muted);
    font-style: italic;
  }
  .line-a.error {
    color: color-mix(in oklab, tomato 75%, var(--fg));
  }

  .line-meta {
    padding-left: calc(1ch + 0.5rem);
    color: var(--muted);
    font-size: 0.78rem;
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem;
    align-items: center;
    margin-top: 0.1rem;
  }
  .meta-label {
    text-transform: uppercase;
    letter-spacing: 0.04em;
    font-weight: 700;
    margin-right: 0.05rem;
  }

  .citation {
    display: inline-flex;
    align-items: center;
    padding: 0.08rem 0.45rem;
    border-radius: 0.35rem;
    border: 1px solid color-mix(in oklab, currentColor 18%, transparent);
    background: transparent;
    color: inherit;
    font: inherit;
    font-size: 0.75rem;
    cursor: pointer;
    max-width: 22rem;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .citation:hover {
    background: color-mix(in oklab, var(--accent) 10%, transparent);
    border-color: color-mix(in oklab, var(--accent) 35%, transparent);
  }

  .dots {
    letter-spacing: 0.2em;
    animation: pulse 1.4s infinite;
  }
  @keyframes pulse {
    0%, 100% { opacity: 0.4; }
    50%      { opacity: 1; }
  }

  /* Live prompt — same baseline as the rendered ❯ lines so the
     transition between scrollback and "where you type" is invisible.
     The textarea has no border, no background, no padding — it's
     just a typeable extension of the terminal. */
  .line-prompt {
    display: flex;
    gap: 0.5rem;
    align-items: flex-start;
    color: var(--fg);
    font-weight: 600;
    margin-top: 0.2rem;
  }
  .line-prompt .prompt-mark {
    color: var(--accent);
    /* Idle/blink cue: a soft pulse on the caret-equivalent so the
       user knows where input lands. Only when not typing. */
  }
  .line-prompt textarea {
    flex: 1;
    min-width: 0;
    border: 0;
    padding: 0;
    margin: 0;
    background: transparent;
    color: inherit;
    font: inherit;
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 0.85rem;
    line-height: 1.55;
    resize: none;
    outline: none;
    overflow-y: auto;
    /* Strip the textarea's default blue focus ring — we want the
       prompt to feel like a terminal cursor, not a form field. */
  }
  .line-prompt textarea:disabled {
    opacity: 0.55;
  }
  .line-prompt textarea::placeholder {
    color: color-mix(in oklab, var(--muted) 75%, transparent);
    font-style: italic;
  }
  .line-prompt.disabled .prompt-mark {
    opacity: 0.35;
  }

  /* Privacy posture badges — same palette as the Settings panel so
     they read as the same UI element family. */
  .posture {
    font-size: 0.66rem;
    font-weight: 700;
    padding: 0.12rem 0.45rem;
    border-radius: 999px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    border: 1px solid transparent;
    white-space: nowrap;
  }
  .posture.posture-local {
    background: color-mix(in oklab, mediumseagreen 18%, transparent);
    color: color-mix(in oklab, mediumseagreen 70%, var(--fg));
    border-color: color-mix(in oklab, mediumseagreen 22%, transparent);
  }
  .posture.posture-self {
    background: color-mix(in oklab, gold 18%, transparent);
    color: color-mix(in oklab, gold 70%, var(--fg));
    border-color: color-mix(in oklab, gold 22%, transparent);
  }
  .posture.posture-cloud {
    background: color-mix(in oklab, tomato 18%, transparent);
    color: color-mix(in oklab, tomato 70%, var(--fg));
    border-color: color-mix(in oklab, tomato 22%, transparent);
  }
  .posture-inline {
    text-transform: uppercase;
    letter-spacing: 0.04em;
    font-weight: 700;
    padding: 0.02rem 0.3rem;
    border-radius: 0.3rem;
    font-size: 0.72rem;
  }
  .posture-inline.posture-local {
    background: color-mix(in oklab, mediumseagreen 14%, transparent);
  }
  .posture-inline.posture-self {
    background: color-mix(in oklab, gold 14%, transparent);
  }
  .posture-inline.posture-cloud {
    background: color-mix(in oklab, tomato 14%, transparent);
  }

  /* Mobile: hide the per-screen hint to save horizontal space; the
     rest still works. */
  @media (max-width: 700px) {
    .askbox-hint { display: none; }
    .askbox { border-radius: 1rem; }
  }
</style>
