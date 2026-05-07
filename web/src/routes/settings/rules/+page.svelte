<script lang="ts">
  import { onMount } from 'svelte';
  import { api, type Rule, type NewRule, type Account } from '$lib/api';

  let rules = $state<Rule[]>([]);
  let accounts = $state<Account[]>([]);
  let loading = $state(true);
  let showForm = $state(false);
  let applying = $state(false);
  let applyResult = $state<{ checked: number; acted: number } | null>(null);

  // New rule form
  let name = $state('');
  let accountId = $state<number | null>(null);
  let condField = $state('from');
  let condOp = $state('contains');
  let condValue = $state('');
  let actionType = $state('move_to');
  let actionValue = $state('');
  let priority = $state(0);
  let formErr = $state<string | null>(null);

  const FIELDS = [
    { id: 'from', label: 'From' },
    { id: 'to', label: 'To' },
    { id: 'cc', label: 'Cc' },
    { id: 'subject', label: 'Subject' },
    { id: 'any', label: 'Any field' }
  ];

  const OPS = [
    { id: 'contains', label: 'contains' },
    { id: 'not_contains', label: 'does not contain' },
    { id: 'equals', label: 'equals' },
    { id: 'starts_with', label: 'starts with' },
    { id: 'ends_with', label: 'ends with' }
  ];

  const ACTIONS = [
    { id: 'move_to', label: 'Move to folder', needsValue: true, placeholder: 'e.g. Notifications, [Gmail]/Spam' },
    { id: 'label', label: 'Add label', needsValue: true, placeholder: 'e.g. Work/Projects' },
    { id: 'mark_read', label: 'Mark as read', needsValue: false, placeholder: '' },
    { id: 'spam', label: 'Mark as spam', needsValue: false, placeholder: '' },
    { id: 'trash', label: 'Move to trash', needsValue: false, placeholder: '' }
  ];

  onMount(async () => {
    try {
      [rules, accounts] = await Promise.all([api.listRules(), api.listAccounts()]);
    } catch {} finally {
      loading = false;
    }
  });

  async function create() {
    formErr = null;
    if (!name.trim()) { formErr = 'Give the rule a name'; return; }
    if (!condValue.trim()) { formErr = 'Condition value is required'; return; }
    const action = ACTIONS.find(a => a.id === actionType);
    if (action?.needsValue && !actionValue.trim()) { formErr = 'Action value is required'; return; }

    try {
      const r = await api.createRule({
        account_id: accountId,
        name: name.trim(),
        condition_field: condField,
        condition_op: condOp,
        condition_value: condValue.trim(),
        action_type: actionType,
        action_value: action?.needsValue ? actionValue.trim() : '',
        priority
      });
      rules = [...rules, r];
      resetForm();
    } catch (e) {
      formErr = e instanceof Error ? e.message : String(e);
    }
  }

  async function remove(id: number) {
    if (!confirm('Delete this rule?')) return;
    await api.deleteRule(id);
    rules = rules.filter(r => r.id !== id);
  }

  async function toggle(id: number, enabled: boolean) {
    const updated = await api.toggleRule(id, enabled);
    rules = rules.map(r => r.id === id ? updated : r);
  }

  function resetForm() {
    showForm = false;
    name = '';
    accountId = null;
    condField = 'from';
    condOp = 'contains';
    condValue = '';
    actionType = 'move_to';
    actionValue = '';
    priority = 0;
    formErr = null;
  }

  function opLabel(op: string) { return OPS.find(o => o.id === op)?.label ?? op; }
  function fieldLabel(f: string) { return FIELDS.find(x => x.id === f)?.label ?? f; }
  function actionLabel(a: string) { return ACTIONS.find(x => x.id === a)?.label ?? a; }
  function acctLabel(id: number | null) {
    if (id === null) return 'All accounts';
    return accounts.find(a => a.id === id)?.email ?? `#${id}`;
  }
</script>

<article class="rules-shell">
  <div class="page-top">
    <a class="back" href="/settings">← Settings</a>
  </div>
  <header class="hero">
    <div class="hero-copy">
      <span class="eyebrow">Automation</span>
      <h1>Message Rules</h1>
      <p>Rules run on every new message during IMAP sync. Matching mail is routed, labeled, or suppressed automatically.</p>
    </div>
    <div class="hero-badges">
      <span class="hero-chip">Inbox triage</span>
      <span class="hero-chip">Account-specific or global</span>
      <span class="hero-chip">Runs during sync</span>
    </div>
  </header>

  {#if loading}
    <p class="muted">Loading…</p>
  {:else}
    {#if rules.length === 0 && !showForm}
      <p class="muted">No rules yet. Create one to start filtering incoming mail.</p>
    {/if}

    {#if rules.length > 0}
      <ul class="rules">
        {#each rules as r (r.id)}
          <li class:disabled={!r.enabled}>
            <div class="rule-head">
              <strong>{r.name}</strong>
              <span class="rule-account">{acctLabel(r.account_id)}</span>
            </div>
            <div class="rule-desc">
              If <b>{fieldLabel(r.condition_field)}</b> {opLabel(r.condition_op)} <code>{r.condition_value}</code>
              → <b>{actionLabel(r.action_type)}</b>{r.action_value ? `: ${r.action_value}` : ''}
            </div>
            <div class="rule-actions">
              <button onclick={() => toggle(r.id, !r.enabled)}>
                {r.enabled ? 'Disable' : 'Enable'}
              </button>
              <button class="danger" onclick={() => remove(r.id)}>Delete</button>
            </div>
          </li>
        {/each}
      </ul>
    {/if}

    <div class="rule-toolbar">
      <button class="add-btn" onclick={() => (showForm = true)}>+ New rule</button>
      {#if rules.length > 0}
        <button
          class="apply-btn"
          disabled={applying}
          onclick={async () => {
            applying = true;
            applyResult = null;
            try {
              applyResult = await api.applyRules();
            } catch {} finally { applying = false; }
          }}
        >
          {applying ? 'Applying…' : 'Apply rules to existing messages'}
        </button>
      {/if}
      {#if applyResult}
        <span class="apply-result">
          Checked {applyResult.checked.toLocaleString()} messages, {applyResult.acted.toLocaleString()} actions taken.
        </span>
      {/if}
    </div>

    {#if showForm}
    {:else}
      <form class="rule-form" onsubmit={(e) => { e.preventDefault(); create(); }}>
        <div class="field">
          <label for="rule-name">Rule name</label>
          <input id="rule-name" bind:value={name} placeholder="e.g. Newsletters to Read Later" />
        </div>

        <div class="field">
          <label for="rule-account">Apply to</label>
          <select id="rule-account" bind:value={accountId}>
            <option value={null}>All accounts</option>
            {#each accounts as a (a.id)}
              <option value={a.id}>{a.email}</option>
            {/each}
          </select>
        </div>

        <div class="condition">
          <label for="rule-condition-field">When</label>
          <select id="rule-condition-field" bind:value={condField}>
            {#each FIELDS as f (f.id)}
              <option value={f.id}>{f.label}</option>
            {/each}
          </select>
          <select aria-label="Condition operator" bind:value={condOp}>
            {#each OPS as o (o.id)}
              <option value={o.id}>{o.label}</option>
            {/each}
          </select>
          <input aria-label="Condition value" bind:value={condValue} placeholder="value to match" />
        </div>

        <div class="action-row">
          <label for="rule-action-type">Then</label>
          <select id="rule-action-type" bind:value={actionType}>
            {#each ACTIONS as a (a.id)}
              <option value={a.id}>{a.label}</option>
            {/each}
          </select>
          {#if ACTIONS.find(a => a.id === actionType)?.needsValue}
            <input
              aria-label="Action value"
              bind:value={actionValue}
              placeholder={ACTIONS.find(a => a.id === actionType)?.placeholder ?? ''}
            />
          {/if}
        </div>

        {#if formErr}
          <div class="err">⚠ {formErr}</div>
        {/if}

        <div class="form-actions">
          <button type="button" class="ghost" onclick={resetForm}>Cancel</button>
          <button type="submit" class="primary">Create rule</button>
        </div>
      </form>
    {/if}
  {/if}
</article>

<style>
  article.rules-shell {
    width: 100%;
    max-width: clamp(60rem, 94vw, 110rem);
    margin: 0 auto;
    padding: 1.25rem 2rem 2.75rem;
    box-sizing: border-box;
  }
  .page-top { margin-bottom: 0.9rem; }
  .back { display: inline-block; color: inherit; opacity: 0.62; text-decoration: none; font-size: 0.85rem; }
  .back:hover { opacity: 1; }
  .hero {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 1rem;
    padding: 1.4rem 1.5rem;
    margin-bottom: 1rem;
    border: 1px solid var(--border);
    border-radius: 1.35rem;
    background:
      radial-gradient(circle at top right, color-mix(in oklab, var(--accent) 12%, transparent), transparent 32%),
      linear-gradient(180deg, color-mix(in oklab, var(--surface) 88%, white 12%), var(--surface));
    box-shadow: 0 18px 42px rgba(0, 0, 0, 0.08);
  }
  .eyebrow {
    display: inline-block;
    margin-bottom: 0.55rem;
    font-size: 0.7rem;
    text-transform: uppercase;
    letter-spacing: 0.12em;
    color: var(--muted);
    font-weight: 700;
  }
  .hero h1 { font-size: 2rem; font-weight: 650; margin: 0 0 0.4rem; letter-spacing: -0.03em; }
  .hero p { font-size: 0.9rem; color: var(--muted); margin: 0; line-height: 1.55; max-width: 44rem; }
  .hero-badges { display: flex; flex-wrap: wrap; gap: 0.45rem; align-content: start; justify-content: flex-end; }
  .hero-chip {
    display: inline-flex; align-items: center; padding: 0.42rem 0.72rem;
    border-radius: 999px; background: color-mix(in oklab, var(--surface-2) 82%, transparent);
    border: 1px solid color-mix(in oklab, currentColor 8%, transparent); font-size: 0.72rem; font-weight: 600;
  }
  .muted { opacity: 0.55; font-size: 0.88rem; }

  ul.rules { list-style: none; margin: 0 0 1.5rem; padding: 0; display: flex; flex-direction: column; gap: 0.65rem; }
  ul.rules li {
    padding: 1rem 1rem;
    border: 1px solid var(--border);
    border-radius: 1rem;
    background: color-mix(in oklab, var(--surface) 96%, transparent);
  }
  ul.rules li.disabled { opacity: 0.5; }
  .rule-head { display: flex; justify-content: space-between; align-items: center; margin-bottom: 0.35rem; }
  .rule-head strong { font-size: 0.92rem; }
  .rule-account { font-size: 0.72rem; opacity: 0.55; font-family: ui-monospace, monospace; }
  .rule-desc { font-size: 0.82rem; opacity: 0.8; margin-bottom: 0.5rem; }
  .rule-desc code { background: color-mix(in oklab, currentColor 8%, transparent); padding: 0.1em 0.35em; border-radius: 0.2em; font-size: 0.88em; }
  .rule-actions { display: flex; gap: 0.4rem; }
  .rule-actions button {
    font: inherit; font-size: 0.75rem; padding: 0.25rem 0.6rem;
    border: 1px solid var(--border); background: transparent; color: inherit;
    border-radius: 999px; cursor: pointer;
  }
  .rule-actions button:hover { background: color-mix(in oklab, currentColor 6%, transparent); }
  .rule-actions button.danger { color: color-mix(in oklab, crimson 80%, currentColor); border-color: color-mix(in oklab, crimson 30%, transparent); }
  .rule-actions button.danger:hover { background: color-mix(in oklab, crimson 10%, transparent); }

  .rule-toolbar {
    display: flex;
    flex-wrap: wrap;
    gap: 0.65rem;
    align-items: center;
    margin-bottom: 1rem;
  }
  .apply-btn {
    font: inherit; font-size: 0.82rem; padding: 0.45rem 0.85rem;
    border: 1px solid var(--border); background: transparent; color: inherit;
    border-radius: 0.35rem; cursor: pointer; opacity: 0.8;
  }
  .apply-btn:hover:not(:disabled) { background: color-mix(in oklab, currentColor 6%, transparent); opacity: 1; }
  .apply-btn:disabled { opacity: 0.5; cursor: progress; }
  .apply-result {
    font-size: 0.8rem; opacity: 0.7;
  }
  .add-btn {
    font: inherit; font-size: 0.88rem; padding: 0.55rem 1rem;
    border: 1px dashed var(--border); background: transparent; color: inherit;
    border-radius: 999px; cursor: pointer; opacity: 0.75;
  }
  .add-btn:hover { opacity: 1; background: color-mix(in oklab, currentColor 4%, transparent); }

  .rule-form {
    border: 1px solid var(--border); border-radius: 1rem;
    padding: 1.25rem; background: color-mix(in oklab, var(--surface) 96%, transparent);
    display: flex; flex-direction: column; gap: 0.75rem;
  }
  .field, .condition, .action-row {
    display: flex; flex-wrap: wrap; gap: 0.5rem; align-items: center;
  }
  .field label, .condition label, .action-row label {
    font-size: 0.78rem; opacity: 0.6; min-width: 5rem;
  }
  .rule-form input, .rule-form select {
    font: inherit; font-size: 0.85rem; padding: 0.4rem 0.6rem;
    border: 1px solid color-mix(in oklab, currentColor 18%, transparent);
    background: color-mix(in oklab, var(--surface-2) 72%, transparent);
    color: inherit; border-radius: 0.8rem; flex: 1; min-width: 8rem;
  }
  .rule-form select { flex: 0 0 auto; min-width: auto; }
  .err { padding: 0.5rem 0.75rem; background: color-mix(in oklab, crimson 12%, transparent); border-left: 2px solid crimson; border-radius: 0 0.3rem 0.3rem 0; font-size: 0.82rem; }

  .form-actions { display: flex; gap: 0.5rem; justify-content: flex-end; margin-top: 0.5rem; }
  .form-actions button {
    font: inherit; font-size: 0.85rem; padding: 0.45rem 1rem;
    border: 1px solid var(--border); background: transparent; color: inherit;
    border-radius: 999px; cursor: pointer;
  }
  .form-actions button.primary { background: var(--accent); border-color: var(--accent); color: white; font-weight: 600; }
  .form-actions button.primary:hover { filter: brightness(0.97); }
  .form-actions button.ghost:hover { background: color-mix(in oklab, currentColor 6%, transparent); }

  @media (max-width: 820px) {
    article.rules-shell { padding: 1rem; }
    .hero { grid-template-columns: 1fr; }
    .hero-badges { justify-content: flex-start; }
    .rule-head { flex-direction: column; align-items: flex-start; gap: 0.25rem; }
  }
</style>
