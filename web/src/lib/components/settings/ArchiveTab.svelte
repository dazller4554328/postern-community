<script lang="ts">
  import { api, type Account, type ArchiveStrategy } from '$lib/api';
  import InfoBubble from '$lib/components/InfoBubble.svelte';

  let {
    accounts,
    loadingAccounts,
    onAccountsChanged
  }: {
    accounts: Account[];
    loadingAccounts: boolean;
    onAccountsChanged: () => Promise<void>;
  } = $props();

  // Per-account draft state. Lives here, not in parent — these forms
  // are tab-local and discarded if the user switches tabs.
  type ArchiveDraft = {
    archive_enabled: boolean;
    archive_folder: string;
    archive_strategy: ArchiveStrategy;
    auto_archive_enabled: boolean;
    auto_archive_age_days: number;
    auto_archive_read_only: boolean;
    retention_enabled: boolean;
    retention_days: number;
  };
  let archiveDrafts = $state<Record<number, ArchiveDraft>>({});
  let saving = $state<Record<number, boolean>>({});
  let expandedArchive = $state<number | null>(null);
  let autoPreviews = $state<Record<number, { count: number; loading: boolean } | null>>({});
  let retentionPreviews = $state<Record<number, { count: number; loading: boolean } | null>>({});

  const DEFAULT_ARCHIVE_BASE = 'Archive';

  const AGE_OPTIONS: { days: number; label: string }[] = [
    { days: 7, label: '7 days' },
    { days: 30, label: '30 days' },
    { days: 90, label: '3 months' },
    { days: 180, label: '6 months' },
    { days: 365, label: '1 year' },
    { days: 730, label: '2 years' },
    { days: 1825, label: '5 years' }
  ];
  const ARCHIVE_STRATEGIES: { id: ArchiveStrategy; label: string; hint: string }[] = [
    { id: 'single', label: 'Single folder', hint: 'Everything in one flat archive.' },
    { id: 'yearly', label: 'Yearly', hint: 'Archive/2026 — bucketed by year.' },
    { id: 'monthly', label: 'Monthly', hint: 'Archive/2026/03 — bucketed by year/month.' }
  ];

  // Auto-fetch preview when an archive panel opens and the account has
  // auto-archive enabled. Saves the user a click.
  $effect(() => {
    const id = expandedArchive;
    if (id == null) return;
    const acct = accounts.find((x) => x.id === id);
    if (!acct) return;
    if (acct.archive_enabled && acct.auto_archive_enabled && !autoPreviews[id]) {
      refreshAutoPreview(id);
    }
    if (acct.retention_enabled && !retentionPreviews[id]) {
      refreshRetentionPreview(id);
    }
  });

  async function refreshAutoPreview(id: number) {
    autoPreviews = { ...autoPreviews, [id]: { count: autoPreviews[id]?.count ?? 0, loading: true } };
    try {
      const p = await api.autoArchivePreview(id);
      autoPreviews = { ...autoPreviews, [id]: { count: p.eligible_count, loading: false } };
    } catch {
      autoPreviews = { ...autoPreviews, [id]: { count: 0, loading: false } };
    }
  }

  async function refreshRetentionPreview(id: number) {
    retentionPreviews = {
      ...retentionPreviews,
      [id]: { count: retentionPreviews[id]?.count ?? 0, loading: true }
    };
    try {
      const p = await api.retentionPreview(id);
      retentionPreviews = { ...retentionPreviews, [id]: { count: p.eligible_count, loading: false } };
    } catch {
      retentionPreviews = { ...retentionPreviews, [id]: { count: 0, loading: false } };
    }
  }

  function archivePreview(base: string, strategy: ArchiveStrategy, when: Date): string {
    const root = base.trim().replace(/\/+$/, '') || DEFAULT_ARCHIVE_BASE;
    if (strategy === 'single') return root;
    const y = when.getFullYear();
    if (strategy === 'yearly') return `${root}/${y}`;
    const m = String(when.getMonth() + 1).padStart(2, '0');
    return `${root}/${y}/${m}`;
  }

  function archiveDraft(a: Account): ArchiveDraft {
    return archiveDrafts[a.id] ?? {
      archive_enabled: a.archive_enabled,
      archive_folder: a.archive_folder ?? '',
      archive_strategy: a.archive_strategy,
      auto_archive_enabled: a.auto_archive_enabled,
      auto_archive_age_days: a.auto_archive_age_days,
      auto_archive_read_only: a.auto_archive_read_only,
      retention_enabled: a.retention_enabled,
      retention_days: a.retention_days
    };
  }

  function archiveDirty(a: Account): boolean {
    const d = archiveDrafts[a.id];
    if (!d) return false;
    return (
      d.archive_enabled !== a.archive_enabled ||
      d.archive_folder.trim() !== (a.archive_folder ?? '') ||
      d.archive_strategy !== a.archive_strategy ||
      d.auto_archive_enabled !== a.auto_archive_enabled ||
      d.auto_archive_age_days !== a.auto_archive_age_days ||
      d.auto_archive_read_only !== a.auto_archive_read_only ||
      d.retention_enabled !== a.retention_enabled ||
      d.retention_days !== a.retention_days
    );
  }

  function updateArchiveDraft(id: number, patch: Partial<ArchiveDraft>) {
    const base = archiveDrafts[id] ?? archiveDraft(accounts.find((x) => x.id === id)!);
    archiveDrafts = { ...archiveDrafts, [id]: { ...base, ...patch } };
  }

  function discardArchive(id: number) {
    const next = { ...archiveDrafts };
    delete next[id];
    archiveDrafts = next;
  }

  async function saveArchive(a: Account) {
    const d = archiveDrafts[a.id];
    if (!d) return;
    saving = { ...saving, [a.id]: true };
    try {
      if (d.archive_enabled !== a.archive_enabled) {
        await api.setArchiveEnabled(a.id, d.archive_enabled);
      }
      const draftFolder = d.archive_folder.trim();
      if (draftFolder !== (a.archive_folder ?? '')) {
        await api.setArchiveFolder(a.id, draftFolder || null);
      }
      if (d.archive_strategy !== a.archive_strategy) {
        await api.setArchiveStrategy(a.id, d.archive_strategy);
      }
      if (
        d.auto_archive_enabled !== a.auto_archive_enabled ||
        d.auto_archive_age_days !== a.auto_archive_age_days ||
        d.auto_archive_read_only !== a.auto_archive_read_only
      ) {
        await api.setAutoArchive(
          a.id,
          d.auto_archive_enabled,
          d.auto_archive_age_days,
          d.auto_archive_read_only
        );
      }
      if (
        d.retention_enabled !== a.retention_enabled ||
        d.retention_days !== a.retention_days
      ) {
        await api.setRetention(a.id, d.retention_enabled, d.retention_days);
      }
      await onAccountsChanged();
      discardArchive(a.id);
      // Refresh the preview so it reflects saved settings.
      await refreshAutoPreview(a.id);
      await refreshRetentionPreview(a.id);
    } catch (e) {
      alert(e instanceof Error ? e.message : String(e));
    } finally {
      saving = { ...saving, [a.id]: false };
    }
  }
</script>

<section class="panel">
  <div class="section-head">
    <h2>Archive</h2>
    <p>Where the Archive button drops mail, per mailbox. Postern creates missing folders on your IMAP server automatically.</p>
  </div>

  {#if loadingAccounts}
    <p class="muted">Loading…</p>
  {:else if accounts.length === 0}
    <p class="muted">Add a mailbox first to configure its archive.</p>
  {:else}
    <ul class="account-list">
      {#each accounts as a (a.id)}
        {@const open = expandedArchive === a.id}
        {@const d = archiveDraft(a)}
        {@const dirty = archiveDirty(a)}
        {@const basePreview = d.archive_folder.trim() || DEFAULT_ARCHIVE_BASE}
        {@const preview = archivePreview(basePreview, d.archive_strategy, new Date())}
        <li class:open>
          <button
            type="button"
            class="account-row"
            class:dirty
            onclick={() => (expandedArchive = open ? null : a.id)}
            aria-expanded={open}
          >
            <div class="account-id">
              <strong>{a.email}</strong>
              <span class="muted">
                {#if a.archive_enabled}
                  archiving to <code>{archivePreview(a.archive_folder ?? DEFAULT_ARCHIVE_BASE, a.archive_strategy, new Date())}</code>
                {:else}
                  archive disabled
                {/if}
              </span>
            </div>
            <div class="account-meta">
              {#if dirty}
                <span class="dirty-chip">Unsaved</span>
              {/if}
              <span class="chev" class:rot={open} aria-hidden="true">
                <svg viewBox="0 0 12 12" width="12" height="12" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
                  <path d="m3 4.5 3 3 3-3"/>
                </svg>
              </span>
            </div>
          </button>

          {#if open}
            <div class="account-panel">
              <div class="field">
                <div class="field-label">
                  <label class="toggle-row" for="enable-{a.id}">
                    <input
                      id="enable-{a.id}"
                      type="checkbox"
                      checked={d.archive_enabled}
                      onchange={(e) => updateArchiveDraft(a.id, {
                        archive_enabled: (e.currentTarget as HTMLInputElement).checked
                      })}
                    />
                    <span>Enable archive for this mailbox</span>
                    <InfoBubble text="When disabled, the Archive button is hidden for this mailbox. Useful for accounts where you prefer just Inbox / Trash without a dedicated archive." />
                  </label>
                </div>
              </div>

              <fieldset class="nested" disabled={!d.archive_enabled}>
                <div class="field">
                  <div class="field-label">
                    <label for="base-{a.id}">
                      Base folder
                      <InfoBubble text="The folder (or Gmail label) that archive messages land in. Leave blank to use 'Archive'. Postern creates it on your IMAP server on first use." />
                    </label>
                  </div>
                  <input
                    id="base-{a.id}"
                    type="text"
                    class="std-input mono"
                    placeholder={DEFAULT_ARCHIVE_BASE}
                    value={d.archive_folder}
                    oninput={(e) => updateArchiveDraft(a.id, {
                      archive_folder: (e.currentTarget as HTMLInputElement).value
                    })}
                  />
                </div>

                <div class="field">
                  <div class="field-label">
                    <label for="strategy-{a.id}">
                      Organise by
                      <InfoBubble text="How the archive is subdivided. 'Yearly' and 'Monthly' create dated subfolders based on each message's date — so archiving a 2024 email lands in Archive/2024, not today's folder. Matches Thunderbird." />
                    </label>
                  </div>
                  <select
                    id="strategy-{a.id}"
                    class="std-select"
                    value={d.archive_strategy}
                    onchange={(e) => updateArchiveDraft(a.id, {
                      archive_strategy: (e.currentTarget as HTMLSelectElement).value as ArchiveStrategy
                    })}
                  >
                    {#each ARCHIVE_STRATEGIES as s (s.id)}
                      <option value={s.id}>{s.label} — {s.hint}</option>
                    {/each}
                  </select>
                </div>

                <div class="preview-line">
                  A message archived <em>today</em> would land in <code>{preview}</code>
                </div>

                <div class="auto-block">
                  <div class="auto-head">
                    <strong>Auto-archive</strong>
                    <InfoBubble text="Once enabled, each sync cycle scans your Inbox and moves messages older than the threshold into Archive. Caps at 200 per cycle to avoid hammering your IMAP server, so initial catch-up runs may take several cycles." />
                  </div>
                  <label class="toggle-row compact" for="auto-en-{a.id}">
                    <input
                      id="auto-en-{a.id}"
                      type="checkbox"
                      checked={d.auto_archive_enabled}
                      onchange={(e) => updateArchiveDraft(a.id, {
                        auto_archive_enabled: (e.currentTarget as HTMLInputElement).checked
                      })}
                    />
                    <span>Automatically archive old messages from Inbox</span>
                  </label>

                  <div class="auto-inner" class:dim={!d.auto_archive_enabled}>
                    <div class="field">
                      <div class="field-label">
                        <label for="auto-age-{a.id}">
                          Archive messages older than
                          <InfoBubble text="Age is measured from the message's received date. Counters reset once a message is out of the Inbox." />
                        </label>
                      </div>
                      <select
                        id="auto-age-{a.id}"
                        class="std-select"
                        disabled={!d.auto_archive_enabled}
                        value={d.auto_archive_age_days}
                        onchange={(e) => updateArchiveDraft(a.id, {
                          auto_archive_age_days: Number((e.currentTarget as HTMLSelectElement).value)
                        })}
                      >
                        {#each AGE_OPTIONS as opt (opt.days)}
                          <option value={opt.days}>{opt.label}</option>
                        {/each}
                      </select>
                    </div>

                    <label class="toggle-row compact" for="auto-read-{a.id}">
                      <input
                        id="auto-read-{a.id}"
                        type="checkbox"
                        disabled={!d.auto_archive_enabled}
                        checked={d.auto_archive_read_only}
                        onchange={(e) => updateArchiveDraft(a.id, {
                          auto_archive_read_only: (e.currentTarget as HTMLInputElement).checked
                        })}
                      />
                      <span>Only archive messages I've read</span>
                      <InfoBubble text="When on, unread messages in the Inbox are left alone even if they're older than the threshold. Recommended — otherwise a rarely-opened inbox just wipes itself into archive." />
                    </label>

                    <div class="preview-line">
                      {#if autoPreviews[a.id]?.loading}
                        Counting eligible messages…
                      {:else}
                        Right now, <strong>{autoPreviews[a.id]?.count ?? 0}</strong>
                        {(autoPreviews[a.id]?.count ?? 0) === 1 ? 'message is' : 'messages are'}
                        eligible for auto-archive.
                        <button type="button" class="link-btn" onclick={() => refreshAutoPreview(a.id)}>Recount</button>
                      {/if}
                    </div>
                  </div>
                </div>

                <div class="auto-block retention">
                  <div class="auto-head">
                    <strong>Server-side retention</strong>
                    <InfoBubble text="Deletes old Inbox messages from the mail provider (Gmail → Trash, plain IMAP → EXPUNGE) after each sync. Postern keeps its local copy, so nothing disappears from here — this is purely to free up provider-side quota. Starred messages are always skipped." />
                  </div>
                  <label class="toggle-row compact" for="ret-en-{a.id}">
                    <input
                      id="ret-en-{a.id}"
                      type="checkbox"
                      checked={d.retention_enabled}
                      onchange={(e) => updateArchiveDraft(a.id, {
                        retention_enabled: (e.currentTarget as HTMLInputElement).checked
                      })}
                    />
                    <span>Delete old Inbox messages from the mail provider</span>
                  </label>

                  <div class="auto-inner" class:dim={!d.retention_enabled}>
                    <div class="field">
                      <div class="field-label">
                        <label for="ret-age-{a.id}">
                          Delete after
                          <InfoBubble text="Age is measured from the message's received date. Starred messages are never deleted even if older than this threshold." />
                        </label>
                      </div>
                      <select
                        id="ret-age-{a.id}"
                        class="std-select"
                        disabled={!d.retention_enabled}
                        value={d.retention_days}
                        onchange={(e) => updateArchiveDraft(a.id, {
                          retention_days: Number((e.currentTarget as HTMLSelectElement).value)
                        })}
                      >
                        {#each AGE_OPTIONS as opt (opt.days)}
                          <option value={opt.days}>{opt.label}</option>
                        {/each}
                      </select>
                    </div>

                    <div class="preview-line">
                      {#if retentionPreviews[a.id]?.loading}
                        Counting eligible messages…
                      {:else}
                        Right now, <strong>{retentionPreviews[a.id]?.count ?? 0}</strong>
                        {(retentionPreviews[a.id]?.count ?? 0) === 1 ? 'message is' : 'messages are'}
                        eligible for server-side deletion.
                        <button type="button" class="link-btn" onclick={() => refreshRetentionPreview(a.id)}>Recount</button>
                      {/if}
                    </div>
                  </div>
                </div>
              </fieldset>

              <div class="actions">
                <span></span>
                <div class="save-bar">
                  {#if dirty}
                    <button class="btn ghost" type="button" onclick={() => discardArchive(a.id)}>Discard</button>
                  {/if}
                  <button
                    class="btn primary"
                    type="button"
                    disabled={!dirty || saving[a.id]}
                    onclick={() => saveArchive(a)}
                  >
                    {saving[a.id] ? 'Saving…' : 'Save'}
                  </button>
                </div>
              </div>
            </div>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
</section>
