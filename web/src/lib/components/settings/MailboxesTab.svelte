<script lang="ts">
  import { onMount } from 'svelte';
  import { api, type Account, type PurgeReport } from '$lib/api';
  import { colorForAccount } from '$lib/accountColor';
  import MailboxIdentitySection from './_components/MailboxIdentitySection.svelte';
  import { formatDate } from '$lib/format';
  import InfoBubble from '$lib/components/InfoBubble.svelte';
  import { tier, ensureTierLoaded } from '$lib/tier';

  let {
    accounts,
    loadingAccounts,
    onAccountsChanged
  }: {
    accounts: Account[];
    loadingAccounts: boolean;
    onAccountsChanged: () => Promise<void>;
  } = $props();

  // Mailbox cap from the build tier (Community = 3, Pro = unlimited).
  // Used to gate the "Add mailbox" button so users hit a clear
  // limit message instead of a backend BadRequest on submit.
  ensureTierLoaded();
  const maxMailboxes = $derived($tier.max_mailboxes);
  const atMailboxLimit = $derived(
    maxMailboxes != null && accounts.length >= maxMailboxes
  );

  // Sync-interval row at the top of the tab. Two-state pattern:
  // `syncInterval` is the last value persisted server-side; the
  // dropdown binds to `syncIntervalDraft`. Save lights up when they
  // diverge; `syncIntervalSaved` flashes briefly after a successful
  // save.
  let syncInterval = $state(120);
  let syncIntervalDraft = $state(120);
  let syncIntervalSaving = $state(false);
  let syncIntervalSaved = $state(false);

  // Per-account credential update — collected inline in the mailbox
  // panel, submitted through the /credentials probe so bad passwords
  // never overwrite good ones.
  let credInputs = $state<Record<number, string>>({});
  let credBusy = $state<Record<number, boolean>>({});
  let credError = $state<Record<number, string | null>>({});
  let credOpen = $state<Record<number, boolean>>({});
  let enableBusy = $state<Record<number, boolean>>({});

  // Per-account Gmail label rescan — walks All Mail and paints
  // hidden labels onto local messages without re-downloading bodies.
  let rescanBusy = $state<Record<number, boolean>>({});

  // Per-account connection (IMAP/SMTP host:port) editing. Like the
  // password flow, the server re-tests the IMAP login before saving so
  // a typo'd host can't silently break sync.
  type ServerDraft = {
    imap_host: string;
    imap_port: number;
    smtp_host: string;
    smtp_port: number | null;
  };
  let serverDrafts = $state<Record<number, ServerDraft>>({});
  let serverBusy = $state<Record<number, boolean>>({});
  let serverError = $state<Record<number, string | null>>({});
  let serverSaved = $state<Record<number, boolean>>({});

  function serverDraft(a: Account): ServerDraft {
    return serverDrafts[a.id] ?? {
      imap_host: a.imap_host,
      imap_port: a.imap_port,
      smtp_host: a.smtp_host ?? '',
      smtp_port: a.smtp_port ?? null
    };
  }

  function updateServerDraft(id: number, patch: Partial<ServerDraft>) {
    const base = serverDrafts[id] ?? serverDraft(accounts.find((x) => x.id === id)!);
    serverDrafts = { ...serverDrafts, [id]: { ...base, ...patch } };
  }

  function serverDirty(a: Account): boolean {
    const d = serverDrafts[a.id];
    if (!d) return false;
    return (
      d.imap_host !== a.imap_host ||
      d.imap_port !== a.imap_port ||
      d.smtp_host !== (a.smtp_host ?? '') ||
      (d.smtp_port ?? null) !== (a.smtp_port ?? null)
    );
  }

  function resetServerDraft(id: number) {
    const next = { ...serverDrafts };
    delete next[id];
    serverDrafts = next;
    serverError = { ...serverError, [id]: null };
  }

  async function saveServer(a: Account) {
    const d = serverDrafts[a.id] ?? serverDraft(a);
    if (!d.imap_host.trim()) {
      serverError = { ...serverError, [a.id]: 'IMAP server is required' };
      return;
    }
    serverBusy = { ...serverBusy, [a.id]: true };
    serverError = { ...serverError, [a.id]: null };
    try {
      await api.updateAccountServer(a.id, {
        imap_host: d.imap_host.trim(),
        imap_port: Number(d.imap_port),
        smtp_host: d.smtp_host.trim() || null,
        smtp_port: d.smtp_port ? Number(d.smtp_port) : null
      });
      await onAccountsChanged();
      resetServerDraft(a.id);
      serverSaved = { ...serverSaved, [a.id]: true };
      setTimeout(() => (serverSaved = { ...serverSaved, [a.id]: false }), 2000);
    } catch (e) {
      serverError = { ...serverError, [a.id]: e instanceof Error ? e.message : String(e) };
    } finally {
      serverBusy = { ...serverBusy, [a.id]: false };
    }
  }

  // Mailbox draft — unsaved edits to per-account preferences. The
  // panel edits a draft; Save reconciles it into the server state.
  type MailboxDraft = {
    delete_after_sync: boolean;
    purge_gmail_categories: boolean;
    skip_gmail_trash: boolean;
    /** `''` = use the deterministic default; otherwise `#rrggbb`. */
    color: string;
    /** Sender "From" name; `''` = fall back to the bare address. */
    display_name: string;
    signature_plain: string;
    signature_html: string;
  };
  let mailboxDrafts = $state<Record<number, MailboxDraft>>({});
  let saving = $state<Record<number, boolean>>({});

  let expandedMailbox = $state<number | null>(null);

  // Server-purge backfill UI state. `purgeReports[id]` is the latest
  // report from the backend; `purgePollers[id]` holds the interval
  // handle so we can stop polling once a job reaches success/failed.
  let purgeReports = $state<Record<number, PurgeReport | null>>({});
  let purgePollers: Record<number, ReturnType<typeof setInterval>> = {};

  function startPurgePoll(accountId: number) {
    if (purgePollers[accountId]) return;
    const tick = async () => {
      try {
        const { report } = await api.getPurgeStatus(accountId);
        purgeReports = { ...purgeReports, [accountId]: report };
        if (report && report.state !== 'running') {
          stopPurgePoll(accountId);
        }
      } catch {
        // If the request errors transiently, keep polling — the next
        // tick will retry. If it errors persistently, the user can
        // close the panel and move on.
      }
    };
    void tick();
    purgePollers[accountId] = setInterval(tick, 2500);
  }

  function stopPurgePoll(accountId: number) {
    const handle = purgePollers[accountId];
    if (handle) {
      clearInterval(handle);
      delete purgePollers[accountId];
    }
  }

  async function runSafetyScan(account: Account) {
    try {
      await api.startServerPurge(account.id, true);
      startPurgePoll(account.id);
    } catch (e) {
      alert(e instanceof Error ? e.message : String(e));
    }
  }

  async function runServerPurge(account: Account) {
    const ok = confirm(
      `Move every server copy of mail Postern has already downloaded for ${account.email} ` +
        `to ${account.kind === 'gmail' ? 'Gmail Trash' : 'the deleted-items folder'}? ` +
        `Postern keeps its local copy — only the provider-side message is removed.`
    );
    if (!ok) return;
    try {
      await api.startServerPurge(account.id, false);
      startPurgePoll(account.id);
    } catch (e) {
      alert(e instanceof Error ? e.message : String(e));
    }
  }

  async function submitNewPassword(a: Account) {
    const pw = (credInputs[a.id] ?? '').trim();
    if (!pw) {
      credError = { ...credError, [a.id]: 'Enter a new app password' };
      return;
    }
    credBusy = { ...credBusy, [a.id]: true };
    credError = { ...credError, [a.id]: null };
    try {
      await api.updateAccountCredentials(a.id, pw);
      credInputs = { ...credInputs, [a.id]: '' };
      credOpen = { ...credOpen, [a.id]: false };
    } catch (e) {
      credError = {
        ...credError,
        [a.id]: e instanceof Error ? e.message : String(e)
      };
    } finally {
      credBusy = { ...credBusy, [a.id]: false };
    }
  }

  async function toggleSyncEnabled(a: Account, enabled: boolean) {
    enableBusy = { ...enableBusy, [a.id]: true };
    try {
      await api.setSyncEnabled(a.id, enabled);
      await onAccountsChanged();
    } catch (e) {
      alert(e instanceof Error ? e.message : String(e));
    } finally {
      enableBusy = { ...enableBusy, [a.id]: false };
    }
  }

  async function toggleSendEnabled(a: Account, enabled: boolean) {
    enableBusy = { ...enableBusy, [a.id]: true };
    try {
      await api.setSendEnabled(a.id, enabled);
      await onAccountsChanged();
    } catch (e) {
      alert(e instanceof Error ? e.message : String(e));
    } finally {
      enableBusy = { ...enableBusy, [a.id]: false };
    }
  }

  async function toggleIncludeInUnified(a: Account, enabled: boolean) {
    enableBusy = { ...enableBusy, [a.id]: true };
    try {
      await api.setIncludeInUnified(a.id, enabled);
      await onAccountsChanged();
    } catch (e) {
      alert(e instanceof Error ? e.message : String(e));
    } finally {
      enableBusy = { ...enableBusy, [a.id]: false };
    }
  }

  async function rescanLabels(a: Account) {
    rescanBusy = { ...rescanBusy, [a.id]: true };
    try {
      const r = await api.rescanGmailLabels(a.id);
      alert(`Scanned ${r.scanned} messages on Gmail, updated labels on ${r.updated} local messages.`);
    } catch (e) {
      alert(e instanceof Error ? e.message : String(e));
    } finally {
      rescanBusy = { ...rescanBusy, [a.id]: false };
    }
  }

  function mailboxDraft(a: Account): MailboxDraft {
    return mailboxDrafts[a.id] ?? {
      delete_after_sync: a.delete_after_sync,
      purge_gmail_categories: a.purge_gmail_categories,
      skip_gmail_trash: a.skip_gmail_trash,
      color: a.color ?? '',
      display_name: a.display_name ?? '',
      signature_plain: a.signature_plain ?? '',
      signature_html: a.signature_html ?? ''
    };
  }

  function mailboxDirty(a: Account): boolean {
    const d = mailboxDrafts[a.id];
    if (!d) return false;
    return (
      d.delete_after_sync !== a.delete_after_sync ||
      d.purge_gmail_categories !== a.purge_gmail_categories ||
      d.skip_gmail_trash !== a.skip_gmail_trash ||
      d.color !== (a.color ?? '') ||
      d.display_name !== (a.display_name ?? '') ||
      d.signature_plain !== (a.signature_plain ?? '') ||
      d.signature_html !== (a.signature_html ?? '')
    );
  }

  function updateMailboxDraft(id: number, patch: Partial<MailboxDraft>) {
    const base = mailboxDrafts[id] ?? mailboxDraft(accounts.find((x) => x.id === id)!);
    mailboxDrafts = { ...mailboxDrafts, [id]: { ...base, ...patch } };
  }

  function discardMailbox(id: number) {
    const next = { ...mailboxDrafts };
    delete next[id];
    mailboxDrafts = next;
  }

  async function saveSyncInterval() {
    if (syncIntervalSaving || syncIntervalDraft === syncInterval) return;
    syncIntervalSaving = true;
    try {
      const r = await api.setSyncInterval(syncIntervalDraft);
      syncInterval = r.interval_secs;
      syncIntervalDraft = r.interval_secs;
      syncIntervalSaved = true;
      // Clear the "Saved" indicator after 2s — long enough to register,
      // short enough not to linger if the user changes the value again.
      setTimeout(() => (syncIntervalSaved = false), 2000);
    } catch (e) {
      alert(e instanceof Error ? e.message : String(e));
    } finally {
      syncIntervalSaving = false;
    }
  }

  async function saveMailbox(a: Account) {
    const d = mailboxDrafts[a.id];
    if (!d) return;
    saving = { ...saving, [a.id]: true };
    try {
      if (d.delete_after_sync !== a.delete_after_sync) {
        await api.setDeletePolicy(a.id, d.delete_after_sync);
      }
      if (d.purge_gmail_categories !== a.purge_gmail_categories) {
        await api.setPurgeGmailCategories(a.id, d.purge_gmail_categories);
      }
      if (d.skip_gmail_trash !== a.skip_gmail_trash) {
        await api.setSkipGmailTrash(a.id, d.skip_gmail_trash);
      }
      if (d.color !== (a.color ?? '')) {
        // Empty string = "fall back to the per-id default"; pass null
        // to the server so the row clears, not a literal empty string.
        await api.setAccountColor(a.id, d.color || null);
      }
      if (d.display_name !== (a.display_name ?? '')) {
        await api.setDisplayName(a.id, d.display_name.trim() || null);
      }
      if (
        d.signature_plain !== (a.signature_plain ?? '') ||
        d.signature_html !== (a.signature_html ?? '')
      ) {
        await api.setSignature(
          a.id,
          d.signature_html.trim() || null,
          d.signature_plain.trim() || null
        );
      }
      await onAccountsChanged();
      discardMailbox(a.id);
    } catch (e) {
      alert(e instanceof Error ? e.message : String(e));
    } finally {
      saving = { ...saving, [a.id]: false };
    }
  }

  async function deleteAccount(id: number) {
    if (!confirm('Remove this mailbox from Postern? Messages stay on your mail provider.')) return;
    await api.deleteAccount(id);
    await onAccountsChanged();
  }

  onMount(async () => {
    try {
      const r = await api.getSyncInterval();
      syncInterval = r.interval_secs;
      syncIntervalDraft = r.interval_secs;
    } catch (e) {
      console.error('sync-interval load failed', e);
    }
  });
</script>

<section class="panel">
  <div class="section-head">
    <div class="section-head-row">
      <h2>Mailbox accounts</h2>
      {#if !loadingAccounts && accounts.length > 0}
        {#if atMailboxLimit}
          <span class="status-chip paused" title="Postern Community is limited to {maxMailboxes} mailboxes. Upgrade to Postern to add more.">
            {accounts.length}/{maxMailboxes} mailboxes
          </span>
        {:else}
          <a class="btn primary add-mailbox" href="/setup">Add mailbox</a>
        {/if}
      {/if}
    </div>
    <p>IMAP credentials, sync cadence, and on-download cleanup. Click a mailbox to open its settings.</p>
  </div>

  <div class="row sync-interval-row">
    <div class="label">
      <strong class="inline">
        Check for new mail every
        <InfoBubble text="How often Postern polls your mail providers via IMAP. Lower = faster delivery, higher = less bandwidth and fewer connections." />
      </strong>
    </div>
    <div class="sync-interval-controls">
      <select
        class="std-select"
        bind:value={syncIntervalDraft}
        disabled={syncIntervalSaving}
      >
        <option value={30}>30 seconds</option>
        <option value={60}>1 minute</option>
        <option value={120}>2 minutes</option>
        <option value={300}>5 minutes</option>
        <option value={600}>10 minutes</option>
        <option value={900}>15 minutes</option>
        <option value={1800}>30 minutes</option>
        <option value={3600}>1 hour</option>
      </select>
      <button
        type="button"
        class="btn primary"
        disabled={syncIntervalDraft === syncInterval || syncIntervalSaving}
        onclick={saveSyncInterval}
      >
        {syncIntervalSaving ? 'Saving…' : 'Save'}
      </button>
      {#if syncIntervalSaved}
        <span class="saved-flash" aria-live="polite">Saved ✓</span>
      {/if}
    </div>
  </div>

  {#if loadingAccounts}
    <p class="muted">Loading…</p>
  {:else if accounts.length === 0}
    <p class="muted">No mailboxes yet. <a href="/setup">Add one</a>.</p>
  {:else}
    <ul class="account-list">
      {#each accounts as a (a.id)}
        {@const open = expandedMailbox === a.id}
        {@const dirty = mailboxDirty(a)}
        {@const d = mailboxDraft(a)}
        {@const sd = serverDraft(a)}
        {@const sDirty = serverDirty(a)}
        <li class:open>
          <button
            type="button"
            class="account-row"
            class:dirty
            onclick={() => (expandedMailbox = open ? null : a.id)}
            aria-expanded={open}
          >
            <span
              class="account-dot"
              style:background-color={d.color || colorForAccount(a)}
              aria-hidden="true"
            ></span>
            <div class="account-id">
              <strong>{a.email}</strong>
              <span class="muted">{a.imap_host}:{a.imap_port}</span>
            </div>
            <div class="account-meta">
              {#if !a.sync_enabled && !a.send_enabled}
                <span class="status-chip paused">Paused</span>
              {:else if !a.sync_enabled}
                <span class="status-chip paused">Receive paused</span>
              {:else if !a.send_enabled}
                <span class="status-chip paused">Send paused</span>
              {:else}
                <span class="status-chip ok">
                  <span class="status-dot" aria-hidden="true"></span>
                  Active
                </span>
              {/if}
              <span class="provider-chip">
                {a.kind === 'gmail' ? 'Gmail' : 'IMAP'}
              </span>
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
            <div class="account-panel mailbox-panel">
              <!-- ─────────── 1 · Connection + Identity (two-up) ─────────── -->
              <div class="panel-top">
                <section class="setting-group">
                  <h4 class="group-title">Connection</h4>

                  <div class="field">
                    <div class="field-label">
                      <label for="imap-host-{a.id}">
                        IMAP server
                        <InfoBubble text="The incoming-mail server and port for this mailbox. Edit it if your provider's hostname changed or you moved hosts. Postern re-tests the login with your saved password before saving, and re-derives the account type (Gmail vs IMAP) from the host." />
                      </label>
                    </div>
                    <div class="server-grid">
                      <input
                        id="imap-host-{a.id}"
                        class="std-input"
                        autocomplete="off" spellcheck="false"
                        placeholder="mail.example.com"
                        value={sd.imap_host}
                        oninput={(e) => updateServerDraft(a.id, { imap_host: (e.currentTarget as HTMLInputElement).value })}
                        disabled={serverBusy[a.id]}
                      />
                      <input
                        class="std-input port"
                        type="number" min="1" max="65535"
                        placeholder="993"
                        value={sd.imap_port}
                        oninput={(e) => updateServerDraft(a.id, { imap_port: Number((e.currentTarget as HTMLInputElement).value) })}
                        disabled={serverBusy[a.id]}
                        aria-label="IMAP port"
                      />
                    </div>
                  </div>

                  <div class="field">
                    <div class="field-label">
                      <label for="smtp-host-{a.id}">
                        SMTP server
                        <InfoBubble text="The outgoing-mail server and port. Leave blank if this mailbox is receive-only. Not re-tested on save — a send only happens when you actually send mail." />
                      </label>
                    </div>
                    <div class="server-grid">
                      <input
                        id="smtp-host-{a.id}"
                        class="std-input"
                        autocomplete="off" spellcheck="false"
                        placeholder="mail.example.com"
                        value={sd.smtp_host}
                        oninput={(e) => updateServerDraft(a.id, { smtp_host: (e.currentTarget as HTMLInputElement).value })}
                        disabled={serverBusy[a.id]}
                      />
                      <input
                        class="std-input port"
                        type="number" min="1" max="65535"
                        placeholder="465"
                        value={sd.smtp_port ?? ''}
                        oninput={(e) => {
                          const v = (e.currentTarget as HTMLInputElement).value;
                          updateServerDraft(a.id, { smtp_port: v ? Number(v) : null });
                        }}
                        disabled={serverBusy[a.id]}
                        aria-label="SMTP port"
                      />
                    </div>
                  </div>

                  {#if serverError[a.id]}
                    <p class="field-help" style="color: var(--c-danger, #b91c1c);">{serverError[a.id]}</p>
                  {:else}
                    <p class="field-help">Tested against the IMAP server with your saved password before it's saved.</p>
                  {/if}

                  <div class="server-actions">
                    <button
                      class="btn primary"
                      type="button"
                      disabled={!sDirty || serverBusy[a.id]}
                      onclick={() => saveServer(a)}
                    >
                      {serverBusy[a.id] ? 'Testing…' : 'Test & save'}
                    </button>
                    {#if sDirty}
                      <button class="btn ghost" type="button" disabled={serverBusy[a.id]} onclick={() => resetServerDraft(a.id)}>Reset</button>
                    {/if}
                    {#if serverSaved[a.id]}
                      <span class="saved-flash" aria-live="polite">Saved ✓</span>
                    {/if}
                  </div>
                </section>

                <!-- Identity -->
                <MailboxIdentitySection
                  id={a.id}
                  color={d.color}
                  defaultColor={colorForAccount(a)}
                  displayName={d.display_name}
                  email={a.email}
                  signaturePlain={d.signature_plain}
                  onUpdate={(patch) => updateMailboxDraft(a.id, patch)}
                />
              </div>

              <div class="mailbox-settings-grid">
              <!-- ─────────── 2 · Sync & delivery ─────────── -->
              <section class="setting-group">
                <h4 class="group-title">Sync &amp; delivery</h4>

                <div class="field">
                  <div class="field-label">
                    <label>Mailbox status</label>
                    <InfoBubble text="Pause inbound or outbound independently. Pausing sync stops Postern from pulling mail, running retention, or auto-archiving for this account — the row stays and unpausing picks up where it left off. Pausing send blocks new outgoing mail before SMTP is even contacted." />
                  </div>
                  <label class="toggle-row compact" for="sync-enabled-{a.id}">
                    <input
                      id="sync-enabled-{a.id}"
                      type="checkbox"
                      checked={a.sync_enabled}
                      disabled={enableBusy[a.id]}
                      onchange={(e) => toggleSyncEnabled(a, (e.currentTarget as HTMLInputElement).checked)}
                    />
                    <span>Receive mail (IMAP sync)</span>
                  </label>
                  <label class="toggle-row compact" for="send-enabled-{a.id}" style="margin-top: 0.35rem;">
                    <input
                      id="send-enabled-{a.id}"
                      type="checkbox"
                      checked={a.send_enabled}
                      disabled={enableBusy[a.id]}
                      onchange={(e) => toggleSendEnabled(a, (e.currentTarget as HTMLInputElement).checked)}
                    />
                    <span>Send mail (SMTP)</span>
                  </label>
                  <label class="toggle-row compact" for="include-unified-{a.id}" style="margin-top: 0.35rem;">
                    <input
                      id="include-unified-{a.id}"
                      type="checkbox"
                      checked={a.include_in_unified}
                      disabled={enableBusy[a.id]}
                      onchange={(e) => toggleIncludeInUnified(a, (e.currentTarget as HTMLInputElement).checked)}
                    />
                    <span>Include in unified views
                      <InfoBubble text="When off, this mailbox still syncs and is visible per-account in the sidebar, but its messages don't show up in the cross-account Inbox / Sent / Drafts / Spam / Trash counts at the top, or in All mail. Useful for a low-priority or throwaway account you want to keep configured but out of sight of your main inbox." />
                    </span>
                  </label>
                  {#if !a.sync_enabled || !a.send_enabled}
                    <p class="field-help">
                      {#if !a.sync_enabled && !a.send_enabled}
                        <strong>Paused.</strong> This mailbox won't sync or send until you re-enable it.
                      {:else if !a.sync_enabled}
                        <strong>Inbound paused.</strong> Compose + send still works; new mail won't arrive.
                      {:else}
                        <strong>Outbound paused.</strong> Mail syncs in; send attempts will fail.
                      {/if}
                    </p>
                  {/if}
                </div>

                <div class="field">
                  <div class="field-label">
                    <label for="retention-{a.id}">
                      On-download deletion
                      <InfoBubble
                        text="What happens to messages on your provider's server (Gmail, Fastmail, etc.) at the moment Postern downloads them. Choose 'Delete from provider' to use your server as a pass-through. Messages you archive or star are unaffected — only the provider-side copy is touched. For a 'keep N days as a server backup, then delete' policy instead, see Settings → Archive → 'Delete provider-side after N days' (these are two separate policies; only one needs to be on at a time)."
                      />
                    </label>
                  </div>
                  <select
                    id="retention-{a.id}"
                    class="std-select"
                    value={d.delete_after_sync ? 'delete' : 'keep'}
                    onchange={(e) => updateMailboxDraft(a.id, {
                      delete_after_sync: (e.currentTarget as HTMLSelectElement).value === 'delete'
                    })}
                  >
                    <option value="keep">Keep messages on provider (recommended)</option>
                    <option value="delete">Delete from provider after Postern downloads them</option>
                  </select>
                  <p class="field-help">
                    {#if d.delete_after_sync}
                      <strong>Heads up:</strong> once Postern downloads a message, it will be removed from {a.imap_host}. Postern keeps its own copy in the local encrypted database — this isn't a delete of your mail, just of the remote copy.
                    {:else}
                      Messages stay on {a.imap_host} after download — your provider keeps them as a backup.
                    {/if}
                  </p>
                </div>

                {#if d.delete_after_sync}
                  <div class="field nested">
                    <div class="field-label">
                      <span>
                        Server backfill purge
                        <InfoBubble
                          text="The streaming sync only deletes messages on the server in the same batch it downloads them. Mail synced before you switched to 'Delete from provider' isn't covered — Postern walks every UID currently on the server, verifies it has a local copy by Message-ID, and deletes the matched ones. The toggle above auto-triggers a purge when you change it; this row lets you run a dry-run first or kick off another pass later."
                        />
                      </span>
                    </div>

                    {#if a.kind === 'gmail'}
                      <label class="toggle-row compact" for="skip-trash-{a.id}">
                        <input
                          id="skip-trash-{a.id}"
                          type="checkbox"
                          checked={d.skip_gmail_trash}
                          onchange={(e) => updateMailboxDraft(a.id, {
                            skip_gmail_trash: (e.currentTarget as HTMLInputElement).checked
                          })}
                        />
                        <span>
                          Skip Gmail's 30-day trash &mdash; permanently delete now
                          <InfoBubble
                            text="After the purge moves messages to [Gmail]/Trash, also empties Gmail's Trash permanently so quota frees immediately instead of waiting 30 days. Wipes the entire Trash, including anything you've trashed manually via Gmail's web UI — opt in only if that's what you want. Postern's local copies of the purged messages are untouched either way; they stay searchable here forever."
                          />
                        </span>
                      </label>
                    {/if}

                    <div class="purge-actions">
                      <button
                        type="button"
                        class="btn"
                        onclick={() => runSafetyScan(a)}
                        disabled={purgeReports[a.id]?.state === 'running'}
                      >
                        Run safety scan
                      </button>
                      <button
                        type="button"
                        class="btn primary"
                        onclick={() => runServerPurge(a)}
                        disabled={purgeReports[a.id]?.state === 'running'}
                      >
                        Run purge now
                      </button>
                    </div>

                    {#if purgeReports[a.id]}
                      {@const r = purgeReports[a.id]!}
                      <div class="purge-status" data-state={r.state}>
                        {#if r.state === 'running'}
                          <strong>Running ({r.mode})…</strong>
                          scanned {r.scanned}, verified {r.verified_safe}, skipped {r.skipped_no_local_copy}, deleted {r.moved_or_deleted}
                        {:else if r.state === 'success'}
                          <strong>{r.mode === 'precheck' ? 'Safety scan complete.' : 'Purge complete.'}</strong>
                          scanned {r.scanned}, verified {r.verified_safe}, skipped {r.skipped_no_local_copy}, deleted {r.moved_or_deleted}{#if r.expunged_from_trash}, trash emptied of {r.expunged_from_trash}{/if}
                        {:else}
                          <strong>Purge failed:</strong>
                          {r.errors[0] ?? 'unknown error'}
                        {/if}
                      </div>
                    {/if}
                  </div>

                  {#if a.kind === 'gmail'}
                    <div class="field nested">
                      <div class="field-label">
                        <label for="purge-cats-{a.id}">
                          Also purge Gmail categories
                          <InfoBubble
                            text="Gmail hides its five category tabs (Updates, Promotions, Social, Forums, Purchases) from IMAP's folder list, so 'Delete from provider' can't reach them — the copies live on in All Mail forever and keep eating quota. With this on, every sync cycle downloads any message in those categories and moves it to [Gmail]/Trash (which strips every label). Gmail's normal 30-day trash lifecycle finishes the job."
                          />
                        </label>
                      </div>
                      <label class="toggle-row compact" for="purge-cats-{a.id}">
                        <input
                          id="purge-cats-{a.id}"
                          type="checkbox"
                          checked={d.purge_gmail_categories}
                          onchange={(e) => updateMailboxDraft(a.id, {
                            purge_gmail_categories: (e.currentTarget as HTMLInputElement).checked
                          })}
                        />
                        <span>Updates, Promotions, Social, Forums, Purchases</span>
                      </label>
                    </div>
                  {/if}
                {/if}
              </section>

              <div class="mailbox-side-stack">
              <!-- ─────────── 3 · Maintenance ─────────── -->
              <section class="setting-group">
                <h4 class="group-title">Maintenance</h4>

                {#if a.kind === 'gmail'}
                  <div class="field">
                    <div class="field-label">
                      <label>Rescan Gmail labels</label>
                      <InfoBubble text="Walks [Gmail]/All Mail and reads X-GM-LABELS for every message, then paints any missing labels (categories like Updates, user labels you haven't exposed via IMAP) onto messages Postern already has locally. No bodies are re-downloaded — cheap backfill, safe to run any time. Useful when you've renamed labels in Gmail or when labels don't match what you see in the Gmail web UI." />
                    </div>
                    <div class="static-value">
                      <button
                        class="btn"
                        type="button"
                        disabled={rescanBusy[a.id]}
                        onclick={() => rescanLabels(a)}
                      >
                        {rescanBusy[a.id] ? 'Rescanning…' : 'Rescan labels now'}
                      </button>
                    </div>
                  </div>
                {/if}

                <div class="field">
                  <div class="field-label">
                    <label>App password</label>
                    <InfoBubble text="Rotate the stored app password without removing the mailbox. The new password is tested against the server before being saved — if IMAP login fails, the old one stays in place." />
                  </div>
                  {#if !credOpen[a.id]}
                    <div class="static-value">
                      <button
                        class="btn"
                        type="button"
                        onclick={() => (credOpen = { ...credOpen, [a.id]: true })}
                      >Change app password…</button>
                    </div>
                  {:else}
                    <div class="seed-row">
                      <input
                        type="password"
                        class="std-input"
                        placeholder="new 16-char app password"
                        autocomplete="new-password"
                        value={credInputs[a.id] ?? ''}
                        oninput={(e) => (credInputs = { ...credInputs, [a.id]: (e.currentTarget as HTMLInputElement).value })}
                        disabled={credBusy[a.id]}
                      />
                      <button
                        class="btn primary"
                        type="button"
                        disabled={credBusy[a.id] || !(credInputs[a.id] ?? '').trim()}
                        onclick={() => submitNewPassword(a)}
                      >{credBusy[a.id] ? 'Verifying…' : 'Save'}</button>
                      <button
                        class="btn ghost"
                        type="button"
                        disabled={credBusy[a.id]}
                        onclick={() => {
                          credOpen = { ...credOpen, [a.id]: false };
                          credError = { ...credError, [a.id]: null };
                          credInputs = { ...credInputs, [a.id]: '' };
                        }}
                      >Cancel</button>
                    </div>
                    {#if credError[a.id]}
                      <p class="field-help" style="color: var(--c-danger, #b91c1c);">
                        {credError[a.id]}
                      </p>
                    {:else}
                      <p class="field-help">
                        The new password is tested against {a.imap_host} before it replaces the stored one.
                      </p>
                    {/if}
                  {/if}
                </div>
              </section>

              <!-- ─────────── 4 · About (read-only meta) ─────────── -->
              <section class="setting-group about-group">
                <dl class="about-grid">
                  <dt>
                    Account kind
                    <InfoBubble
                      text="Derived from the IMAP host every time an account is read — Gmail iff host is imap.gmail.com or imap.googlemail.com, plain IMAP otherwise. Gates SMTP auto-file behaviour, X-GM-LABELS / X-GM-RAW usage, and [Gmail]/Trash semantics. Not user-settable: if this looks wrong, fix the IMAP host rather than the kind."
                    />
                  </dt>
                  <dd>
                    {a.kind === 'gmail' ? 'Gmail / Google Workspace' : 'IMAP'}
                    <span class="muted">(<code>{a.imap_host}</code>)</span>
                  </dd>
                  <dt>Added</dt>
                  <dd>{formatDate(a.created_at)}</dd>
                </dl>
              </section>
              </div>
              </div>

              <!-- ─────────── Action bar ─────────── -->
              <div class="actions">
                <button class="btn danger" onclick={() => deleteAccount(a.id)}>Remove mailbox</button>
                <div class="save-bar">
                  {#if dirty}
                    <button class="btn ghost" type="button" onclick={() => discardMailbox(a.id)}>Discard</button>
                  {/if}
                  <button
                    class="btn primary"
                    type="button"
                    disabled={!dirty || saving[a.id]}
                    onclick={() => saveMailbox(a)}
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
