<script lang="ts">
  import './rules.css';
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

