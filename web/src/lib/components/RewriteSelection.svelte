<script lang="ts">
  // Compose-pane "AI polish" widget. Selection-only (v1) — operates
  // on whatever the user highlighted in the body textarea so token
  // spend is bounded by the highlight, not the full email.
  // No selection → button is disabled with a hint, so the user is
  // never surprised by what was sent.

  import { api, type AiRewriteTone } from '$lib/api';

  interface Props {
    /** Full body text — used to slice the selection. */
    text: string;
    selectionStart: number;
    selectionEnd: number;
    /** Splice (start, end) → replacement into the body. The compose
     *  page implements this so cursor + selection state stay correct. */
    onReplace: (start: number, end: number, replacement: string) => void;
  }

  let { text, selectionStart, selectionEnd, onReplace }: Props = $props();

  let tone = $state<AiRewriteTone>('professional');
  let busy = $state(false);
  let err = $state<string | null>(null);
  let lastInfo = $state<string | null>(null);

  let hasSelection = $derived(selectionEnd > selectionStart);
  let selected = $derived(
    hasSelection ? text.slice(selectionStart, selectionEnd) : ''
  );
  let charCount = $derived(selected.length);
  let tooLong = $derived(charCount > 4000);

  async function polish() {
    if (!hasSelection || busy || tooLong) return;
    err = null;
    lastInfo = null;
    busy = true;
    // Snapshot the range so concurrent typing while waiting on the
    // model doesn't make us splice into a different region than the
    // user highlighted.
    const start = selectionStart;
    const end = selectionEnd;
    const original = selected;
    try {
      const r = await api.aiRewrite({ text: original, tone });
      onReplace(start, end, r.rewritten);
      lastInfo = `${r.chat_model} · ${(r.elapsed_ms / 1000).toFixed(1)}s · ${
        r.prompt_tokens + r.completion_tokens
      } tokens`;
    } catch (e) {
      err = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
    }
  }
</script>

<section class="rewrite">
  <header>
    <span class="title">AI polish</span>
    {#if busy}
      <span class="status">Polishing…</span>
    {:else if tooLong}
      <span class="status err">{charCount} chars — over 4000 limit</span>
    {:else if hasSelection}
      <span class="status ok">{charCount} char{charCount === 1 ? '' : 's'} selected</span>
    {:else}
      <span class="status muted">Highlight text in the body to polish</span>
    {/if}
  </header>

  <div class="controls">
    <label class="tone">
      Tone
      <select bind:value={tone} disabled={busy}>
        <option value="professional">Professional</option>
        <option value="concise">Concise</option>
        <option value="friendly">Friendly</option>
      </select>
    </label>
    <button
      type="button"
      class="go"
      onclick={polish}
      disabled={!hasSelection || busy || tooLong}
      title={hasSelection
        ? `Rewrite the ${charCount}-char selection`
        : 'Select text in the body first'}
    >
      {busy ? '…' : 'Polish selection'}
    </button>
  </div>

  {#if err}
    <div class="err-row">⚠ {err}</div>
  {:else if lastInfo}
    <div class="info-row">{lastInfo}</div>
  {/if}
</section>

<style>
  .rewrite {
    border: 1px solid var(--border);
    border-radius: 0.6rem;
    padding: 0.5rem 0.7rem;
    background: color-mix(in oklab, currentColor 3%, transparent);
    font-size: 0.78rem;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 0.4rem;
  }
  .title {
    font-weight: 600;
    opacity: 0.75;
  }
  .status {
    opacity: 0.6;
    font-size: 0.72rem;
  }
  .status.ok {
    color: #10b981;
    opacity: 1;
  }
  .status.err {
    color: #ef4444;
    opacity: 1;
  }
  .status.muted {
    opacity: 0.5;
  }

  .controls {
    display: flex;
    align-items: center;
    gap: 0.55rem;
    flex-wrap: wrap;
  }
  .tone {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    font-size: 0.74rem;
    opacity: 0.8;
  }
  .tone select {
    padding: 0.2rem 0.4rem;
    border: 1px solid var(--border);
    border-radius: 0.3rem;
    background: var(--surface);
    color: inherit;
    font: inherit;
    font-size: 0.74rem;
  }
  button.go {
    padding: 0.25rem 0.7rem;
    border-radius: 0.35rem;
    border: 1px solid color-mix(in oklab, var(--accent) 35%, var(--border));
    background: color-mix(in oklab, var(--accent) 12%, var(--surface));
    color: inherit;
    font: inherit;
    font-size: 0.76rem;
    font-weight: 600;
    cursor: pointer;
  }
  button.go:hover:not(:disabled) {
    background: color-mix(in oklab, var(--accent) 22%, var(--surface));
  }
  button.go:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  .err-row {
    margin-top: 0.4rem;
    color: #ef4444;
    font-size: 0.74rem;
  }
  .info-row {
    margin-top: 0.4rem;
    opacity: 0.55;
    font-size: 0.7rem;
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  }
</style>
