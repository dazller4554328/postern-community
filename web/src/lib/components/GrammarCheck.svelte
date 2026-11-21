<script lang="ts">
  import { onDestroy } from 'svelte';
  import { prefs } from '$lib/prefs';

  interface Props {
    text: string;
    onApplyAll: (newText: string) => void;
  }

  let { text, onApplyAll }: Props = $props();

  function disableFromHere() {
    // Settings → Display owns this preference. Flipping it here
    // hides the panel immediately because compose's #if guard reads
    // the same store. Users get a one-click out from the surface
    // they're actually looking at.
    prefs.update((p) => ({ ...p, composeGrammarCheck: false }));
  }

  // Keep the linter surface typed loosely — harper.js exposes complex
  // wasm-bindgen objects (Lint, Suggestion, Span) whose concrete shapes
  // change across versions. We only ever call documented methods on
  // them, so `unknown` + narrow casts is safer than pulling the
  // library's `.d.ts` across this component boundary.
  type HarperLint = {
    message: () => string;
    lint_kind_pretty: () => string;
    span: () => { start: number; end: number };
    suggestions: () => HarperSuggestion[];
  };
  type HarperSuggestion = { get_replacement_text: () => string };
  type HarperLinter = {
    setup: () => Promise<void>;
    lint: (text: string) => Promise<HarperLint[]>;
    applySuggestion: (text: string, lint: HarperLint, s: HarperSuggestion) => Promise<string>;
  };

  interface Issue {
    start: number;
    end: number;
    problem: string;
    message: string;
    kind: string;
    suggestions: string[];
    raw: HarperLint;
  }

  let linter: HarperLinter | null = null;
  let loadError = $state<string | null>(null);
  let initializing = $state(false);
  let initialized = $state(false);
  let running = $state(false);
  let issues = $state<Issue[]>([]);
  let lintTimer: ReturnType<typeof setTimeout> | null = null;

  // Debounce at 500ms — long enough to not run on every keystroke,
  // short enough that suggestions feel live while you pause.
  const LINT_DEBOUNCE_MS = 500;

  async function ensureLinter(): Promise<HarperLinter | null> {
    if (linter) return linter;
    if (loadError) return null;
    initializing = true;
    try {
      const [{ WorkerLinter }, { binary }] = await Promise.all([
        import('harper.js'),
        import('harper.js/binary')
      ]);
      const l = new WorkerLinter({ binary }) as unknown as HarperLinter;
      await l.setup();
      linter = l;
      initialized = true;
      return l;
    } catch (e) {
      loadError = e instanceof Error ? e.message : String(e);
      return null;
    } finally {
      initializing = false;
    }
  }

  async function runLint(t: string) {
    const l = await ensureLinter();
    if (!l) return;
    running = true;
    try {
      const lints = await l.lint(t);
      const next: Issue[] = [];
      for (const lint of lints) {
        const span = lint.span();
        const sugs = lint.suggestions();
        next.push({
          start: span.start,
          end: span.end,
          problem: t.slice(span.start, span.end),
          message: lint.message(),
          kind: lint.lint_kind_pretty(),
          suggestions: sugs.map((s) => s.get_replacement_text()),
          raw: lint
        });
      }
      issues = next;
    } catch (e) {
      loadError = e instanceof Error ? e.message : String(e);
    } finally {
      running = false;
    }
  }

  $effect(() => {
    const t = text;
    if (lintTimer) clearTimeout(lintTimer);
    if (!t || !t.trim()) {
      issues = [];
      return;
    }
    lintTimer = setTimeout(() => runLint(t), LINT_DEBOUNCE_MS);
    return () => {
      if (lintTimer) clearTimeout(lintTimer);
    };
  });

  onDestroy(() => {
    if (lintTimer) clearTimeout(lintTimer);
  });

  async function apply(issue: Issue, idx: number) {
    const l = linter;
    if (!l) return;
    const sugs = issue.raw.suggestions();
    const target = sugs[idx];
    if (!target) return;
    // Let Harper compute the post-replace text — it handles unicode
    // spans correctly, and it's robust to minor text drift that
    // happened between lint-time and apply-time.
    const newText = await l.applySuggestion(text, issue.raw, target);
    onApplyAll(newText);
  }

  let suggestionCount = $derived(issues.length);
  let hasIssues = $derived(issues.length > 0);
</script>

<section class="grammar">
  <header>
    <span class="title">Grammar &amp; spelling</span>
    <span class="header-end">
      {#if initializing}
        <span class="status">Loading dictionary…</span>
      {:else if loadError}
        <span class="status err" title={loadError}>Unavailable</span>
      {:else if running}
        <span class="status">Checking…</span>
      {:else if initialized}
        <span class="status" class:ok={!hasIssues}>
          {hasIssues ? `${suggestionCount} suggestion${suggestionCount === 1 ? '' : 's'}` : 'Looks good'}
        </span>
      {/if}
      <button
        type="button"
        class="off"
        onclick={disableFromHere}
        title="Hide grammar &amp; spelling suggestions (toggle back in Settings → Display)"
        aria-label="Hide grammar suggestions"
      >×</button>
    </span>
  </header>

  {#if !loadError && hasIssues}
    <ul>
      {#each issues as issue, i (i)}
        <li>
          <div class="row-head">
            <span class="kind">{issue.kind}</span>
            <span class="msg">{issue.message}</span>
          </div>
          <div class="row-body">
            {#if issue.problem}
              <span class="problem">{issue.problem}</span>
            {/if}
            {#if issue.suggestions.length > 0}
              <span class="arrow">→</span>
              {#each issue.suggestions.slice(0, 3) as s, si (si)}
                <button type="button" class="fix" onclick={() => apply(issue, si)}>
                  {s === '' ? '(delete)' : s}
                </button>
              {/each}
            {/if}
          </div>
        </li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  .grammar {
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
    gap: 0.5rem;
    margin-bottom: 0.25rem;
  }
  .header-end {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
  }
  .off {
    background: transparent;
    border: 0;
    color: inherit;
    opacity: 0.45;
    font-size: 0.95rem;
    line-height: 1;
    padding: 0.05rem 0.35rem;
    border-radius: 0.25rem;
    cursor: pointer;
  }
  .off:hover {
    opacity: 1;
    background: color-mix(in oklab, currentColor 8%, transparent);
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
  ul {
    list-style: none;
    padding: 0;
    margin: 0.3rem 0 0;
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
  }
  li {
    padding: 0.4rem 0.5rem;
    border-radius: 0.4rem;
    background: color-mix(in oklab, currentColor 4%, transparent);
  }
  .row-head {
    display: flex;
    gap: 0.5rem;
    align-items: baseline;
    flex-wrap: wrap;
  }
  .kind {
    font-size: 0.66rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    opacity: 0.55;
    font-weight: 600;
  }
  .msg {
    flex: 1;
    min-width: 0;
  }
  .row-body {
    display: flex;
    flex-wrap: wrap;
    gap: 0.35rem;
    align-items: center;
    margin-top: 0.25rem;
  }
  .problem {
    text-decoration: line-through wavy #ef4444;
    opacity: 0.7;
    padding: 0.05rem 0.25rem;
    border-radius: 0.25rem;
    background: color-mix(in oklab, #ef4444 10%, transparent);
  }
  .arrow {
    opacity: 0.4;
  }
  button.fix {
    padding: 0.15rem 0.5rem;
    border-radius: 0.3rem;
    border: 1px solid var(--border);
    background: var(--surface);
    color: inherit;
    font: inherit;
    font-size: 0.74rem;
    cursor: pointer;
  }
  button.fix:hover {
    background: color-mix(in oklab, #10b981 15%, var(--surface));
    border-color: color-mix(in oklab, #10b981 35%, var(--border));
  }
</style>
