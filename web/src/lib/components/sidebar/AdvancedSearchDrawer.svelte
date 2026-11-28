<script lang="ts">
  import type { FoldersResponse } from '$lib/api';

  let {
    searchText,
    folders,
    onSearch,
    onClose
  }: {
    searchText: string;
    folders: FoldersResponse | null;
    onSearch: (q: string) => void;
    onClose: () => void;
  } = $props();

  let advFrom = $state('');
  let advTo = $state('');
  let advSubject = $state('');
  let advBody = $state('');
  let advAfter = $state('');
  let advBefore = $state('');
  let advLabel = $state('');
  let advAccountEmail = $state('');
  let advHasAttachment = $state(false);
  let advIsUnread = $state(false);
  let advIsStarred = $state(false);
  let advIsEncrypted = $state(false);

  function quoteIfSpaces(v: string): string {
    return /\s/.test(v) ? `"${v.replace(/"/g, '')}"` : v;
  }

  function compileQuery(): string {
    const parts: string[] = [];
    const existing = searchText.trim();
    if (existing) parts.push(existing);
    if (advFrom.trim()) parts.push(`from:${quoteIfSpaces(advFrom.trim())}`);
    if (advTo.trim()) parts.push(`to:${quoteIfSpaces(advTo.trim())}`);
    if (advSubject.trim()) parts.push(`subject:${quoteIfSpaces(advSubject.trim())}`);
    if (advBody.trim()) parts.push(`body:${quoteIfSpaces(advBody.trim())}`);
    if (advAfter) parts.push(`after:${advAfter}`);
    if (advBefore) parts.push(`before:${advBefore}`);
    if (advLabel.trim()) parts.push(`label:${quoteIfSpaces(advLabel.trim())}`);
    if (advAccountEmail) parts.push(`account:${advAccountEmail}`);
    if (advHasAttachment) parts.push('has:attachment');
    if (advIsUnread) parts.push('is:unread');
    if (advIsStarred) parts.push('is:starred');
    if (advIsEncrypted) parts.push('is:encrypted');
    return parts.join(' ');
  }

  function runSearch() {
    onSearch(compileQuery());
    onClose();
  }

  function reset() {
    advFrom = '';
    advTo = '';
    advSubject = '';
    advBody = '';
    advAfter = '';
    advBefore = '';
    advLabel = '';
    advAccountEmail = '';
    advHasAttachment = false;
    advIsUnread = false;
    advIsStarred = false;
    advIsEncrypted = false;
  }
</script>

<div class="search-advanced" role="region" aria-label="Advanced search">
  <div class="sa-row">
    <label class="sa-field">
      <span>From</span>
      <input type="text" placeholder="alice@corp.com" bind:value={advFrom} />
    </label>
    <label class="sa-field">
      <span>To / Cc</span>
      <input type="text" placeholder="bob" bind:value={advTo} />
    </label>
  </div>
  <div class="sa-row">
    <label class="sa-field">
      <span>Subject</span>
      <input type="text" placeholder="invoice" bind:value={advSubject} />
    </label>
    <label class="sa-field">
      <span>Body contains</span>
      <input type="text" placeholder="quarterly" bind:value={advBody} />
    </label>
  </div>
  <div class="sa-row">
    <label class="sa-field">
      <span>After</span>
      <input type="date" bind:value={advAfter} />
    </label>
    <label class="sa-field">
      <span>Before</span>
      <input type="date" bind:value={advBefore} />
    </label>
  </div>
  <div class="sa-row">
    <label class="sa-field">
      <span>Label / folder</span>
      <input type="text" placeholder="Work/Projects" bind:value={advLabel} />
    </label>
    <label class="sa-field">
      <span>Account email</span>
      <select bind:value={advAccountEmail}>
        <option value="">Any mailbox</option>
        {#if folders}
          {#each folders.accounts as a (a.account_id)}
            <option value={a.email}>{a.email}</option>
          {/each}
        {/if}
      </select>
    </label>
  </div>
  <div class="sa-checks">
    <label><input type="checkbox" bind:checked={advHasAttachment} /> Has attachment</label>
    <label><input type="checkbox" bind:checked={advIsUnread} /> Unread</label>
    <label><input type="checkbox" bind:checked={advIsStarred} /> Starred</label>
    <label><input type="checkbox" bind:checked={advIsEncrypted} /> Encrypted (PGP)</label>
  </div>
  <div class="sa-actions">
    <button type="button" class="sa-btn primary" onclick={runSearch}>Search</button>
    <button type="button" class="sa-btn" onclick={reset}>Clear fields</button>
    <details class="sa-help">
      <summary>Operator reference</summary>
      <ul>
        <li><code>from:</code>, <code>to:</code>, <code>cc:</code>, <code>subject:</code>, <code>body:</code> — scope to a field</li>
        <li><code>has:attachment</code> — only messages with files attached</li>
        <li><code>is:unread</code> · <code>is:read</code> · <code>is:starred</code> · <code>is:encrypted</code></li>
        <li><code>label:Work/Projects</code> — only in this label</li>
        <li><code>before:2025-01-01</code> · <code>after:2025-06-15</code> — date range</li>
        <li><code>older_than:30d</code> · <code>newer_than:7d</code> — relative (s/m/h/d/w/y)</li>
        <li><code>account:you@gmail.com</code> — scope to one mailbox</li>
        <li><code>-word</code> — exclude (same as <code>NOT word</code>)</li>
        <li><code>"exact phrase"</code> — phrase match</li>
      </ul>
    </details>
  </div>
</div>

<style>
  .search-advanced {
    margin: 0 1rem 0.7rem;
    padding: 0.7rem 0.8rem 0.8rem;
    border: 1px solid color-mix(in oklab, currentColor 10%, transparent);
    background: color-mix(in oklab, var(--surface-2) 58%, transparent);
    border-radius: 0.8rem;
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
    font-size: 0.78rem;
  }
  .sa-row {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0.5rem;
  }
  .sa-field {
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
    min-width: 0;
  }
  .sa-field > span {
    font-size: 0.66rem;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--muted);
  }
  .sa-field input,
  .sa-field select {
    font: inherit;
    font-size: 0.78rem;
    padding: 0.38rem 0.55rem;
    border: 1px solid color-mix(in oklab, currentColor 12%, transparent);
    background: var(--surface, #fff);
    color: inherit;
    border-radius: 0.45rem;
    min-width: 0;
  }
  .sa-field input:focus,
  .sa-field select:focus {
    outline: none;
    border-color: color-mix(in oklab, var(--accent) 36%, transparent);
    box-shadow: 0 0 0 3px color-mix(in oklab, var(--accent) 12%, transparent);
  }
  .sa-checks {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0.3rem 0.55rem;
    padding-top: 0.1rem;
  }
  .sa-checks label {
    display: inline-flex;
    align-items: center;
    gap: 0.45rem;
    font-size: 0.77rem;
    cursor: pointer;
  }
  .sa-actions {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    flex-wrap: wrap;
    margin-top: 0.1rem;
  }
  .sa-btn {
    font: inherit;
    font-size: 0.77rem;
    font-weight: 600;
    padding: 0.42rem 0.85rem;
    border: 1px solid color-mix(in oklab, currentColor 12%, transparent);
    border-radius: 999px;
    background: var(--surface);
    color: inherit;
    cursor: pointer;
    transition: background 120ms, border-color 120ms;
  }
  .sa-btn:hover {
    background: color-mix(in oklab, currentColor 5%, var(--surface));
  }
  .sa-btn.primary {
    background: color-mix(in oklab, var(--accent) 22%, var(--surface));
    border-color: color-mix(in oklab, var(--accent) 46%, transparent);
  }
  .sa-btn.primary:hover {
    background: color-mix(in oklab, var(--accent) 32%, var(--surface));
  }
  .sa-help {
    margin-left: auto;
    font-size: 0.72rem;
    color: var(--muted);
  }
  .sa-help summary {
    cursor: pointer;
    user-select: none;
  }
  .sa-help summary:hover {
    color: inherit;
  }
  .sa-help ul {
    margin: 0.4rem 0 0;
    padding-left: 1rem;
    line-height: 1.55;
  }
  .sa-help code {
    font-size: 0.7rem;
    padding: 0.04rem 0.25rem;
    background: color-mix(in oklab, currentColor 8%, transparent);
    border-radius: 0.2rem;
  }
</style>
