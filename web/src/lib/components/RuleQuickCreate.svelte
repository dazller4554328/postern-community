<script lang="ts">
  import { api, type MessageDetail } from '$lib/api';
  import { formatSender } from '$lib/format';

  interface Props {
    message: MessageDetail;
    onClose: () => void;
  }

  let { message, onClose }: Props = $props();

  let ruleField = $state<'from' | 'subject' | 'to'>('from');
  let ruleOp = $state('contains');
  let ruleValue = $state(message.from_addr ?? '');
  let ruleAction = $state('move_to');
  let ruleFolder = $state('');
  let ruleErr = $state<string | null>(null);
  let ruleSaving = $state(false);

  /** Re-prime the form against a different field on the source message
   *  (Sender / Subject / To). Called when the user clicks the chips. */
  function pickField(field: 'from' | 'subject' | 'to') {
    ruleField = field;
    ruleOp = 'contains';
    ruleValue = field === 'from'
      ? (message.from_addr ?? '')
      : field === 'subject'
        ? (message.subject ?? '')
        : (message.to_addrs ?? '');
    ruleAction = 'move_to';
    ruleFolder = '';
    ruleErr = null;
  }

  async function save() {
    if (!ruleValue.trim()) { ruleErr = 'Value is required'; return; }
    if (ruleAction === 'move_to' && !ruleFolder.trim()) {
      ruleErr = 'Folder name is required';
      return;
    }
    ruleSaving = true;
    ruleErr = null;
    try {
      const fieldLabels: Record<string, string> = {
        from: 'From',
        subject: 'Subject',
        to: 'To',
      };
      await api.createRule({
        account_id: message.account_id,
        name: `${fieldLabels[ruleField]}: ${ruleValue.slice(0, 40)}`,
        condition_field: ruleField,
        condition_op: ruleOp,
        condition_value: ruleValue.trim(),
        action_type: ruleAction,
        action_value: ruleAction === 'move_to' ? ruleFolder.trim() : '',
      });
      onClose();
    } catch (e) {
      ruleErr = e instanceof Error ? e.message : String(e);
    } finally {
      ruleSaving = false;
    }
  }
</script>

<div class="rule-card">
  <div class="rule-quick">
    <span class="rule-label">Quick rule from:</span>
    <button class="rule-chip" class:active={ruleField === 'from'} onclick={() => pickField('from')}>
      Sender: {formatSender(message.from_addr)}
    </button>
    <button class="rule-chip" class:active={ruleField === 'subject'} onclick={() => pickField('subject')}>
      Subject: {(message.subject ?? '').slice(0, 40)}
    </button>
  </div>
  <div class="rule-form-inline">
    <div class="rule-row">
      <span class="rule-label">When</span>
      <select bind:value={ruleField}>
        <option value="from">From</option>
        <option value="to">To</option>
        <option value="subject">Subject</option>
      </select>
      <select bind:value={ruleOp}>
        <option value="contains">contains</option>
        <option value="not_contains">does not contain</option>
        <option value="equals">equals</option>
        <option value="starts_with">starts with</option>
        <option value="ends_with">ends with</option>
      </select>
      <input bind:value={ruleValue} placeholder="value" />
    </div>
    <div class="rule-row">
      <span class="rule-label">Then</span>
      <select bind:value={ruleAction}>
        <option value="move_to">Move to folder</option>
        <option value="mark_read">Mark as read</option>
        <option value="spam">Mark as spam</option>
        <option value="trash">Move to trash</option>
      </select>
      {#if ruleAction === 'move_to'}
        <input bind:value={ruleFolder} placeholder="Folder name (new folders created automatically)" />
      {/if}
    </div>
    {#if ruleErr}
      <div class="rule-err">⚠ {ruleErr}</div>
    {/if}
    <div class="rule-btns">
      <button class="ghost" onclick={onClose}>Cancel</button>
      <button class="primary" onclick={save} disabled={ruleSaving}>
        {ruleSaving ? 'Saving…' : 'Create rule'}
      </button>
    </div>
  </div>
</div>

<style>
  .rule-card {
    padding: 0.85rem 1rem;
    margin-bottom: 0.75rem;
    border: 1px solid var(--border);
    border-radius: 0.45rem;
    background: var(--surface);
    display: flex;
    flex-direction: column;
    gap: 0.65rem;
    animation: rule-in 150ms ease-out;
  }
  @keyframes rule-in {
    from { opacity: 0; transform: translateY(-4px); }
    to { opacity: 1; transform: translateY(0); }
  }
  .rule-quick {
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem;
    align-items: center;
  }
  .rule-label {
    font-size: 0.75rem;
    opacity: 0.6;
    font-weight: 500;
    min-width: 3rem;
  }
  .rule-chip {
    font: inherit;
    font-size: 0.78rem;
    padding: 0.3rem 0.65rem;
    border: 1px solid color-mix(in oklab, currentColor 15%, transparent);
    background: transparent;
    color: inherit;
    border-radius: 999px;
    cursor: pointer;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 22rem;
  }
  .rule-chip:hover { background: color-mix(in oklab, currentColor 6%, transparent); }
  .rule-chip.active {
    background: color-mix(in oklab, dodgerblue 15%, transparent);
    border-color: color-mix(in oklab, dodgerblue 40%, transparent);
  }
  .rule-form-inline {
    display: flex;
    flex-direction: column;
    gap: 0.55rem;
  }
  .rule-row {
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem;
    align-items: center;
  }
  .rule-row select, .rule-row input {
    font: inherit;
    font-size: 0.82rem;
    padding: 0.35rem 0.55rem;
    border: 1px solid color-mix(in oklab, currentColor 18%, transparent);
    background: color-mix(in oklab, currentColor 3%, transparent);
    color: inherit;
    border-radius: 0.3rem;
  }
  .rule-row select { flex: 0 0 auto; }
  .rule-row input { flex: 1; min-width: 10rem; }
  .rule-err {
    font-size: 0.8rem;
    padding: 0.4rem 0.65rem;
    background: color-mix(in oklab, crimson 10%, transparent);
    border-left: 2px solid crimson;
    border-radius: 0 0.25rem 0.25rem 0;
  }
  .rule-btns {
    display: flex;
    gap: 0.4rem;
    justify-content: flex-end;
  }
  .rule-btns button {
    font: inherit;
    font-size: 0.82rem;
    padding: 0.4rem 0.85rem;
    border: 1px solid var(--border);
    background: transparent;
    color: inherit;
    border-radius: 0.3rem;
    cursor: pointer;
  }
  .rule-btns button.primary {
    background: dodgerblue;
    border-color: dodgerblue;
    color: white;
    font-weight: 500;
  }
  .rule-btns button.primary:hover:not(:disabled) {
    background: color-mix(in oklab, dodgerblue 85%, black);
  }
  .rule-btns button.ghost:hover {
    background: color-mix(in oklab, currentColor 6%, transparent);
  }
  .rule-btns button:disabled { opacity: 0.55; cursor: progress; }

  @media (max-width: 900px) {
    .rule-card {
      padding: 0.8rem 0.85rem;
      border-radius: 0.85rem;
    }
    .rule-chip {
      max-width: 100%;
    }
    .rule-row {
      align-items: stretch;
    }
    .rule-label {
      min-width: 0;
      width: 100%;
    }
    .rule-row select,
    .rule-row input {
      width: 100%;
      min-width: 0;
      box-sizing: border-box;
    }
    .rule-btns {
      justify-content: stretch;
    }
    .rule-btns button {
      flex: 1 1 0;
    }
  }
</style>
