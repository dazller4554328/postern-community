<script lang="ts">
  import Icon from './Icon.svelte';

  interface Props {
    code: string;
    lang?: string;
  }
  let { code, lang = 'bash' }: Props = $props();

  let copied = $state(false);
  let timer: ReturnType<typeof setTimeout> | undefined;

  async function copy() {
    try {
      await navigator.clipboard.writeText(code);
      copied = true;
      clearTimeout(timer);
      timer = setTimeout(() => (copied = false), 1800);
    } catch {
      /* clipboard blocked — selection still works */
    }
  }
</script>

<div class="code">
  <span class="dot">$</span>
  <pre><code>{code}</code></pre>
  <button type="button" onclick={copy} aria-label="Copy command" class:copied>
    <Icon name={copied ? 'check' : 'import'} size={16} />
    {copied ? 'Copied' : 'Copy'}
  </button>
  <span class="visually-hidden">{lang}</span>
</div>

<style>
  .code {
    display: flex;
    align-items: center;
    gap: 0.8rem;
    padding: 0.9rem 0.9rem 0.9rem 1.1rem;
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    background: var(--bg);
    font-family: var(--font-mono);
    overflow: hidden;
  }
  .dot {
    color: var(--accent);
    user-select: none;
  }
  pre {
    margin: 0;
    flex: 1;
    overflow-x: auto;
  }
  code {
    font-size: var(--text-sm);
    color: var(--text);
    white-space: pre;
  }
  button {
    flex: none;
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    font-family: var(--font-display);
    font-size: var(--text-xs);
    font-weight: 600;
    padding: 0.5rem 0.8rem;
    border-radius: var(--r-sm);
    border: 1px solid var(--border);
    background: var(--surface);
    color: var(--text-muted);
    cursor: pointer;
    transition:
      color var(--dur-fast) var(--ease),
      border-color var(--dur-fast) var(--ease);
  }
  button:hover {
    color: var(--text);
    border-color: var(--border-strong);
  }
  button.copied {
    color: var(--accent);
    border-color: var(--accent);
  }
</style>
