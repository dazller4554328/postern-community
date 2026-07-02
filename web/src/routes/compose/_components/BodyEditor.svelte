<script lang="ts">
  import { prefs } from '$lib/prefs';
  import GrammarCheck from '$lib/components/GrammarCheck.svelte';
  import RewriteSelection from '$lib/components/RewriteSelection.svelte';
  import VoiceDictate from '$lib/components/VoiceDictate.svelte';

  interface Props {
    body: string;
  }
  let { body = $bindable() }: Props = $props();

  let bodyEl: HTMLTextAreaElement | null = $state(null);
  let bodySelStart = $state(0);
  let bodySelEnd = $state(0);

  function captureBodySelection() {
    if (!bodyEl) return;
    bodySelStart = bodyEl.selectionStart;
    bodySelEnd = bodyEl.selectionEnd;
  }

  // Splice a dictated chunk at the current cursor position (or append
  // if the textarea was never focused). Smart-space + sentence-case so
  // Web-Speech-API lowercase finalisations look right.
  function applyDictation(chunk: string) {
    const trimmed = chunk.trim();
    if (!trimmed) return;
    const start = bodySelStart;
    const end = bodySelEnd;
    const before = body.slice(0, start);
    const after = body.slice(end);
    const needsLeadingSpace = before.length > 0 && !/[\s\n]$/.test(before);
    const startsSentence = before.length === 0 || /[.!?]\s*$/.test(before);
    let insert = trimmed;
    if (startsSentence && insert.length > 0) {
      insert = insert.charAt(0).toUpperCase() + insert.slice(1);
    }
    if (needsLeadingSpace) insert = ' ' + insert;
    if (bodyEl) {
      bodyEl.focus();
      bodyEl.setSelectionRange(start, end);
      let inserted = false;
      try {
        inserted = document.execCommand('insertText', false, insert);
      } catch {
        inserted = false;
      }
      if (!inserted) {
        body = before + insert + after;
      }
      queueMicrotask(() => {
        if (!bodyEl) return;
        const cursor = start + insert.length;
        bodyEl.setSelectionRange(cursor, cursor);
        captureBodySelection();
      });
    } else {
      body = before + insert + after;
    }
  }

  // Splice a rewrite into `body`, then re-select the replacement so the
  // user immediately sees what changed. execCommand keeps native undo
  // working when the textarea is focused; falls back to a plain assign.
  function applyRewrite(start: number, end: number, replacement: string) {
    if (!bodyEl) {
      body = body.slice(0, start) + replacement + body.slice(end);
      return;
    }
    bodyEl.focus();
    bodyEl.setSelectionRange(start, end);
    let inserted = false;
    try {
      inserted = document.execCommand('insertText', false, replacement);
    } catch {
      inserted = false;
    }
    if (!inserted) {
      body = body.slice(0, start) + replacement + body.slice(end);
    }
    queueMicrotask(() => {
      if (!bodyEl) return;
      bodyEl.setSelectionRange(start, start + replacement.length);
      captureBodySelection();
    });
  }

  // Notes companion — opens /notes in a chrome-stripped popup. Mobile
  // ignores the features string and opens a new tab, which still
  // preserves the draft.
  function openNotes() {
    const url = '/notes?popup=1';
    const features = 'width=480,height=720,resizable=yes,scrollbars=yes,noopener=no';
    window.open(url, 'postern-notes', features);
  }
</script>

<div class="row body-row">
  <label for="body">Body</label>
  <div class="body-stack">
    <textarea
      id="body"
      bind:this={bodyEl}
      bind:value={body}
      placeholder="Write your message…"
      spellcheck="true"
      onselect={captureBodySelection}
      onkeyup={captureBodySelection}
      onmouseup={captureBodySelection}
      onfocus={captureBodySelection}
    ></textarea>
    <div class="body-toolbar">
      <VoiceDictate onAppend={applyDictation} />
      <button
        type="button"
        class="notes-btn"
        onclick={openNotes}
        title="Open Notes alongside this draft (popup window on desktop, new tab on mobile — your draft stays put)"
      >
        <svg viewBox="0 0 20 20" width="14" height="14" aria-hidden="true">
          <path d="M5 3h7l4 4v10H5z" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round"/>
          <path d="M12 3v4h4" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round"/>
          <path d="M8 11h6M8 14h4" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"/>
        </svg>
        Notes
      </button>
      <span class="body-hint">Dictate, then run Polish below to clean it up.</span>
    </div>
  </div>
</div>

<div class="row grammar-row">
  <span class="grammar-label"></span>
  <div class="assist-stack">
    {#if $prefs.composeGrammarCheck}
      <GrammarCheck text={body} onApplyAll={(t) => (body = t)} />
    {/if}
    <RewriteSelection
      text={body}
      selectionStart={bodySelStart}
      selectionEnd={bodySelEnd}
      onReplace={applyRewrite}
    />
  </div>
</div>

<style>
  .body-stack {
    display: flex;
    flex-direction: column;
    gap: 0.45rem;
    min-width: 0;
  }
  .body-toolbar {
    display: flex;
    align-items: center;
    gap: 0.7rem;
    flex-wrap: wrap;
  }
  .body-hint {
    color: var(--muted, color-mix(in oklab, currentColor 55%, transparent));
    font-size: 0.74rem;
  }
  .notes-btn {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    padding: 0.32rem 0.6rem;
    background: color-mix(in oklab, var(--accent) 10%, transparent);
    color: var(--accent);
    border: 1px solid color-mix(in oklab, var(--accent) 22%, transparent);
    border-radius: 0.55rem;
    font: inherit;
    font-size: 0.78rem;
    font-weight: 600;
    cursor: pointer;
    transition: background 140ms ease;
  }
  .notes-btn:hover {
    background: color-mix(in oklab, var(--accent) 18%, transparent);
  }
  .notes-btn svg { display: block; }
  .assist-stack {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    min-width: 0;
  }
</style>
