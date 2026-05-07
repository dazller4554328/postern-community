<script lang="ts">
  import { onMount } from 'svelte';
  import { api, type Account, type Forensics, type MessageDetail, type NewRule } from '$lib/api';
  import { lockdown } from '$lib/lockdown';
  import FolderPicker from './FolderPicker.svelte';
  import { formatDate, formatSender } from '$lib/format';
  import { prefs, type DefaultView } from '$lib/prefs';

  type ViewMode = 'html' | 'plain' | 'source' | 'forensics';

  interface Props {
    messageId: number;
    variant?: 'preview' | 'full';
  }
  let { messageId, variant = 'full' }: Props = $props();

  interface TrackerBlocked {
    host: string;
    service: string;
  }

  interface BodyInfo {
    format: 'html' | 'plain';
    remote_hosts: string[];
    has_remote_content: boolean;
    trackers_blocked: TrackerBlocked[];
  }

  let message = $state<MessageDetail | null>(null);
  let accountsById = $state<Record<number, Account>>({});
  let pickerOpen = $state(false);
  let archiveEnabled = $derived(
    message ? accountsById[message.account_id]?.archive_enabled ?? true : true
  );
  let bodyInfo = $state<BodyInfo | null>(null);
  let forensics = $state<Forensics | null>(null);
  let plainText = $state<string | null>(null);
  let rawText = $state<string | null>(null);
  let mode = $state<ViewMode>('html');
  let iframeHeight = $state(400);
  let allowRemote = $state(false);
  let err = $state<string | null>(null);
  let iframe = $state<HTMLIFrameElement | undefined>(undefined);
  let defaultView: DefaultView = $state('html');
  let isMobile = $state(false);
  let mobileToolsOpen = $state(false);

  $effect(() => {
    const unsub = prefs.subscribe((p) => {
      defaultView = p.defaultView;
    });
    return unsub;
  });

  let src = $derived(
    message ? `/api/messages/${message.id}/body.html${allowRemote ? '?remote=1' : ''}` : ''
  );

  async function load() {
    message = null;
    bodyInfo = null;
    forensics = null;
    plainText = null;
    rawText = null;
    err = null;
    allowRemote = false;
    iframeHeight = 400;
    // Default to html until the message loads; switchMode below will honor
    // the user's saved pref (plain/source/forensics) and trigger the right
    // fetch — previously the mode was set here but the fetch never fired
    // because switchMode wasn't called, leaving a permanent "Loading…".
    mode = 'html';
    try {
      message = await api.getMessage(messageId);
      const res = await fetch(`/api/messages/${messageId}/body`);
      if (res.ok) {
        const info = (await res.json()) as BodyInfo & { html: string };
        bodyInfo = {
          format: info.format,
          remote_hosts: info.remote_hosts,
          has_remote_content: info.has_remote_content,
          trackers_blocked: info.trackers_blocked ?? []
        };
      }
      // Apply the user's default view now that message is loaded. switchMode
      // kicks off the plain/source/forensics fetch as needed.
      await switchMode(defaultView as ViewMode);
      // Kick off forensics in background so badges (encrypted/signed) show fast.
      api.getForensics(messageId).then((f) => (forensics = f)).catch(() => {});
    } catch (e) {
      err = e instanceof Error ? e.message : String(e);
    }
  }

  async function switchMode(next: ViewMode) {
    mode = next;
    if (next === 'plain' && plainText === null && message) {
      try {
        const r = await api.getMessagePlain(message.id);
        plainText = r.text || '(no plain-text body)';
      } catch (e) {
        plainText = `Error: ${e instanceof Error ? e.message : String(e)}`;
      }
    } else if (next === 'source' && rawText === null && message) {
      try {
        const r = await fetch(api.getRawUrl(message.id));
        rawText = await r.text();
      } catch (e) {
        rawText = `Error: ${e instanceof Error ? e.message : String(e)}`;
      }
    } else if (next === 'forensics' && forensics === null && message) {
      try {
        forensics = await api.getForensics(message.id);
      } catch (e) {
        err = e instanceof Error ? e.message : String(e);
      }
    }
  }

  $effect(() => {
    messageId;
    load();
  });

  // Turn the tracker list into "Mailchimp, SendGrid (+2 more)" — caps
  // the rendered services so long lists stay readable. Deduplicated
  // so a sender using three Mailchimp pixels still reads as one
  // service.
  function trackerServicesSummary(trackers: TrackerBlocked[]): string {
    const unique = Array.from(new Set(trackers.map((t) => t.service)));
    if (unique.length === 0) return '';
    if (unique.length <= 2) return unique.join(', ');
    return `${unique.slice(0, 2).join(', ')} (+${unique.length - 2} more)`;
  }

  /// Drive the Reply-all visibility test: only show the button when
  /// the message has Cc'd recipients OR more than one To address.
  function splitToAddrs(raw: string): string[] {
    return raw
      .split(/[,;]/)
      .map((s) => s.trim())
      .filter(Boolean);
  }

  let receiptSendState = $state<'idle' | 'sending' | 'sent' | 'error'>('idle');
  let receiptSendError = $state<string | null>(null);
  async function handleSendReceipt() {
    if (!message || receiptSendState === 'sending') return;
    receiptSendState = 'sending';
    receiptSendError = null;
    try {
      await api.sendReadReceipt(message.id);
      receiptSendState = 'sent';
    } catch (e) {
      receiptSendState = 'error';
      receiptSendError = e instanceof Error ? e.message : String(e);
    }
  }

  onMount(() => {
    const mq = window.matchMedia('(max-width: 900px)');
    const syncMobile = () => {
      isMobile = mq.matches;
      if (!mq.matches) mobileToolsOpen = false;
    };
    syncMobile();
    mq.addEventListener('change', syncMobile);
    void (async () => {
      try {
        const list = await api.listAccounts();
        accountsById = Object.fromEntries(list.map((a) => [a.id, a]));
      } catch {
        // Non-fatal — the Archive button just defaults to visible.
      }
    })();
    return () => {
      mq.removeEventListener('change', syncMobile);
    };
  });

  function onIframeLoad() {
    if (!iframe) return;
    try {
      const doc = iframe.contentDocument;
      if (doc) iframeHeight = Math.max(240, doc.documentElement.scrollHeight + 16);
    } catch {}
  }

  function verdictClass(v: string) {
    return v === 'pass' ? 'ok' : v === 'fail' ? 'bad' : v === 'unknown' || v === 'none' ? 'unknown' : 'warn';
  }

  // ── Inline rule creation ──────────────────────────────────────────
  let showRuleForm = $state(false);
  let ruleField = $state<'from' | 'subject' | 'to'>('from');
  let ruleOp = $state('contains');
  let ruleValue = $state('');
  let ruleAction = $state('move_to');
  let ruleFolder = $state('');
  let ruleErr = $state<string | null>(null);
  let ruleSaving = $state(false);

  function openRuleForm(field: 'from' | 'subject' | 'to') {
    if (!message) return;
    ruleField = field;
    ruleOp = field === 'from' ? 'contains' : 'contains';
    ruleValue = field === 'from'
      ? (message.from_addr ?? '')
      : field === 'subject'
        ? (message.subject ?? '')
        : (message.to_addrs ?? '');
    ruleAction = 'move_to';
    ruleFolder = '';
    ruleErr = null;
    showRuleForm = true;
  }

  async function saveRule() {
    if (!message) return;
    if (!ruleValue.trim()) { ruleErr = 'Value is required'; return; }
    if (ruleAction === 'move_to' && !ruleFolder.trim()) { ruleErr = 'Folder name is required'; return; }
    ruleSaving = true;
    ruleErr = null;
    try {
      const fieldLabels: Record<string, string> = { from: 'From', subject: 'Subject', to: 'To' };
      await api.createRule({
        account_id: message.account_id,
        name: `${fieldLabels[ruleField]}: ${ruleValue.slice(0, 40)}`,
        condition_field: ruleField,
        condition_op: ruleOp,
        condition_value: ruleValue.trim(),
        action_type: ruleAction,
        action_value: ruleAction === 'move_to' ? ruleFolder.trim() : '',
      });
      showRuleForm = false;
    } catch (e) {
      ruleErr = e instanceof Error ? e.message : String(e);
    } finally {
      ruleSaving = false;
    }
  }

  function humanBytes(n: number): string {
    if (n < 1024) return `${n} B`;
    if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
    return `${(n / (1024 * 1024)).toFixed(2)} MB`;
  }

  // Kept in sync with the server-side whitelist in body.rs. If the
  // type isn't listed the Preview button is hidden and only Download
  // is offered — matches the Mailpile model we adopted.
  const INLINE_TYPES = new Set([
    'image/png',
    'image/jpeg',
    'image/gif',
    'image/webp',
    'image/tiff',
    // SVG deliberately omitted — opening in a new tab executes any
    // inline scripts. Matches server-side is_inline_whitelisted.
    'audio/mp3',
    'audio/mpeg',
    'audio/ogg',
    'audio/x-wav',
    'audio/wav',
    'video/mpeg',
    'video/ogg',
    'video/mp4',
    'video/webm',
    'application/pdf',
    'text/plain'
  ]);
  // Types that aren't natively previewable but CAN be rendered via
  // the viewer sandbox (LibreOffice → PDF → PDF.js). Gated on the
  // sandbox actually being running — see sandboxAvailable below.
  const CONVERT_TYPES = new Set([
    'application/msword',
    'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
    'application/vnd.ms-excel',
    'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
    'application/vnd.ms-powerpoint',
    'application/vnd.openxmlformats-officedocument.presentationml.presentation',
    'application/vnd.oasis.opendocument.text',
    'application/vnd.oasis.opendocument.spreadsheet',
    'application/vnd.oasis.opendocument.presentation',
    'application/rtf',
    'text/rtf',
    'text/csv'
  ]);
  function isInlinePreviewable(ct: string): boolean {
    return INLINE_TYPES.has(ct.toLowerCase().trim());
  }
  function isConvertible(ct: string): boolean {
    return CONVERT_TYPES.has(ct.toLowerCase().trim());
  }

  // Polled once per message view so the Preview button for Office
  // docs only appears when the sandbox is actually running. Cached
  // across the component lifecycle — unlikely to change mid-session.
  let sandboxAvailable = $state<boolean | null>(null);
  $effect(() => {
    if (sandboxAvailable !== null) return;
    // Skip the probe entirely if no attachment on this message
    // actually needs conversion — saves a round-trip on 95% of
    // messages.
    const any = forensics?.attachments?.some((a) => isConvertible(a.content_type));
    if (!any) return;
    void api.viewerSandboxStatus()
      .then((r) => { sandboxAvailable = r.viewer_available; })
      .catch(() => { sandboxAvailable = false; });
  });
</script>

<section class:preview={variant === 'preview'} class:full={variant === 'full'}>
  {#if err}
    <p class="err">Error: {err}</p>
  {:else if !message}
    <p class="placeholder">Loading…</p>
  {:else}
    <header>
      <div class="subject-row">
        <h1>{message.subject || '(no subject)'}</h1>
        <div class="header-badges">
          {#if forensics?.is_pgp_encrypted || forensics?.is_smime_encrypted}
            <span class="badge enc" title={forensics.is_pgp_encrypted ? 'PGP encrypted' : 'S/MIME encrypted'}>🔒</span>
          {/if}
          {#if forensics?.is_pgp_signed || forensics?.is_smime_signed}
            <span class="badge sig" title={forensics.is_pgp_signed ? 'PGP signed' : 'S/MIME signed'}>🖋</span>
          {/if}
          {#if variant === 'preview'}
            <a class="open-full" href="/message/{message.id}" title="Open in full view">↗</a>
          {/if}
        </div>
      </div>
      <div class="meta">
        <div class="from">
          <strong>{formatSender(message.from_addr)}</strong>
          {#if message.from_addr}<span class="addr">&lt;{message.from_addr}&gt;</span>{/if}
        </div>
        {#if message.to_addrs}<div class="line">to {message.to_addrs}</div>{/if}
        <div class="line date">{formatDate(message.date_utc)}</div>
      </div>
      {#if message.labels.length}
        <div class="labels">
          {#each message.labels as l}<span class="label">{l}</span>{/each}
        </div>
      {/if}
    </header>

    {#if message.receipt_to}
      <div class="receipt-banner" role="note" aria-live="polite">
        <div class="receipt-text">
          <strong>Read receipt requested.</strong>
          <span>The sender asked to be notified when you open this message ({message.receipt_to}). Postern never sends one automatically.</span>
        </div>
        <div class="receipt-actions">
          {#if receiptSendState === 'idle'}
            <button type="button" class="receipt-send" onclick={handleSendReceipt}>Send receipt</button>
            <button type="button" class="receipt-ignore" onclick={() => (receiptSendState = 'sent')}>Ignore</button>
          {:else if receiptSendState === 'sending'}
            <span class="receipt-status">Sending…</span>
          {:else if receiptSendState === 'sent'}
            <span class="receipt-status done">Done — banner stays for this session.</span>
          {:else}
            <span class="receipt-status err">Failed: {receiptSendError ?? 'unknown error'}</span>
            <button type="button" class="receipt-send" onclick={handleSendReceipt}>Retry</button>
          {/if}
        </div>
      </div>
    {/if}

    {#if isMobile}
      <div class="mobile-tools-toggle">
        <button type="button" class="mobile-tools-btn" onclick={() => (mobileToolsOpen = !mobileToolsOpen)}>
          <span>{mobileToolsOpen ? 'Hide actions' : 'Actions & view'}</span>
          <span aria-hidden="true">{mobileToolsOpen ? '−' : '+'}</span>
        </button>
      </div>
    {/if}

    <!-- Action + view-mode toolbar. Reply/Forward on the left; mode tabs on the right. -->
    <div class="toolbar-row" class:mobile-drawer={isMobile} class:open={!isMobile || mobileToolsOpen}>
      <div class="actions-toolbar" class:locked={$lockdown.enabled}>
        {#if $lockdown.enabled}
          <span class="lockdown-notice" title="Lockdown is on. Reply / Forward / Archive / Trash / Move / Spam are disabled.">
            <svg viewBox="0 0 16 16" width="12" height="12" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
              <rect x="3" y="7.5" width="10" height="6.5" rx="1.2"/>
              <path d="M5 7.5V5a3 3 0 0 1 6 0v2.5"/>
            </svg>
            Lockdown
          </span>
        {/if}
        <a class="action" href="/compose?reply={message.id}" title="Reply">
          <svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
            <path d="M7 3 2.5 7.5 7 12"/>
            <path d="M2.5 7.5h7a4 4 0 0 1 4 4v1"/>
          </svg>
          Reply
        </a>
        {#if message.cc_addrs || (message.to_addrs && splitToAddrs(message.to_addrs).length > 1)}
          <a
            class="action"
            href="/compose?reply_all={message.id}"
            title="Reply to sender + everyone on the To/Cc lines"
          >
            <svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
              <path d="M5 3 0.5 7.5 5 12"/>
              <path d="M9 3 4.5 7.5 9 12"/>
              <path d="M4.5 7.5h6a4 4 0 0 1 4 4v1"/>
            </svg>
            Reply all
          </a>
        {/if}
        <a class="action" href="/compose?forward={message.id}" title="Forward">
          <svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
            <path d="M9 3 13.5 7.5 9 12"/>
            <path d="M13.5 7.5h-7a4 4 0 0 0-4 4v1"/>
          </svg>
          Forward
        </a>
        <span class="action-sep" aria-hidden="true"></span>
        {#if message.labels.some((l) => l.includes('Spam') || l === 'Junk')}
          <button class="action" onclick={async () => { await api.markNotSpam(message!.id); load(); }} title="Not spam — move to Inbox">
            <svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
              <path d="M8 1.5 14.5 13H1.5L8 1.5Z"/>
              <path d="M8 6.5v3"/>
            </svg>
            Not spam
          </button>
        {:else}
          <button class="action" onclick={async () => { await api.markSpam(message!.id); load(); }} title="Mark as spam">
            <svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
              <path d="M8 1.5 14.5 13H1.5L8 1.5Z"/>
              <path d="M8 6.5v3"/><path d="M8 11.2v.1"/>
            </svg>
            Spam
          </button>
        {/if}
        {#if archiveEnabled}
          <button class="action" onclick={async () => { await api.archiveMessage(message!.id); load(); }} title="Archive — move to the account's configured archive folder">
            <svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
              <rect x="1.75" y="3" width="12.5" height="3" rx="0.6"/>
              <path d="M3 6v6.5a1 1 0 0 0 1 1h8a1 1 0 0 0 1-1V6"/>
              <path d="M6.5 9h3"/>
            </svg>
            Archive
          </button>
        {/if}
        <button class="action" onclick={() => (pickerOpen = true)} title="Move to a specific folder…">
          <svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
            <path d="M2.5 5.5h11v7.5a.5.5 0 0 1-.5.5H3a.5.5 0 0 1-.5-.5z"/>
            <path d="M2.5 5.5V4a.5.5 0 0 1 .5-.5h4l1.5 2h5a.5.5 0 0 1 .5.5v1"/>
          </svg>
          Move…
        </button>
        <button class="action" onclick={async () => { await api.markTrash(message!.id); load(); }} title="Move to Trash">
          <svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
            <path d="M2.5 4.5h11"/>
            <path d="M6 4.5V3a1 1 0 0 1 1-1h2a1 1 0 0 1 1 1v1.5"/>
            <path d="M4 4.5v8.5a1 1 0 0 0 1 1h6a1 1 0 0 0 1-1V4.5"/>
          </svg>
          Trash
        </button>
        <span class="action-sep" aria-hidden="true"></span>
        <div class="rule-dropdown">
          <button class="action" onclick={() => { showRuleForm = !showRuleForm; }} title="Create a rule from this message">
            <svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
              <path d="M2 4h12M2 8h8M2 12h6"/>
              <path d="M13 9l2 2-2 2"/>
            </svg>
            Rule
          </button>
        </div>
      </div>
      <div class="view-toolbar" role="tablist" aria-label="View mode">
        <button role="tab" class:active={mode === 'html'} onclick={() => switchMode('html')} title="Rendered HTML in a sandbox">HTML</button>
        <button role="tab" class:active={mode === 'plain'} onclick={() => switchMode('plain')} title="Plain text only — Mailpile-style secure default">Plain</button>
        <button role="tab" class:active={mode === 'source'} onclick={() => switchMode('source')} title="Raw RFC822 source">Source</button>
        <button role="tab" class:active={mode === 'forensics'} onclick={() => switchMode('forensics')} title="Headers + authentication + MIME tree">Forensics</button>
      </div>
    </div>

    {#if showRuleForm && message}
      <div class="rule-card">
        <div class="rule-quick">
          <span class="rule-label">Quick rule from:</span>
          <button class="rule-chip" class:active={ruleField === 'from'} onclick={() => openRuleForm('from')}>
            Sender: {formatSender(message.from_addr)}
          </button>
          <button class="rule-chip" class:active={ruleField === 'subject'} onclick={() => openRuleForm('subject')}>
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
            <button class="ghost" onclick={() => (showRuleForm = false)}>Cancel</button>
            <button class="primary" onclick={saveRule} disabled={ruleSaving}>
              {ruleSaving ? 'Saving…' : 'Create rule'}
            </button>
          </div>
        </div>
      </div>
    {/if}

    {#if forensics?.attachments?.length}
      <div class="att-strip">
        <span class="att-label">
          📎 {forensics.attachments.length} attachment{forensics.attachments.length === 1 ? '' : 's'}
        </span>
        <ul class="att-list">
          {#each forensics.attachments as a, i (i)}
            {@const canPreview = isInlinePreviewable(a.content_type)}
            {@const canConvert = isConvertible(a.content_type) && sandboxAvailable === true}
            <li class="att-item">
              <span class="att-name">
                <span class="att-fname">{a.filename ?? '(unnamed)'}</span>
                <span class="att-meta">{a.content_type} · {humanBytes(a.size_bytes)}</span>
              </span>
              <span class="att-actions">
                {#if canPreview || canConvert}
                  {@const isPdf = a.content_type.toLowerCase().trim() === 'application/pdf'}
                  {#if isPdf}
                    <a
                      class="att-btn"
                      href={`/viewer/pdf?msg=${messageId}&idx=${i}${a.filename ? `&name=${encodeURIComponent(a.filename)}` : ''}`}
                      target="_blank"
                      rel="noopener"
                      title="Open in Postern's sandboxed PDF viewer — no network, no scripts"
                    >Preview</a>
                  {:else if canConvert}
                    <a
                      class="att-btn"
                      href={`/viewer/pdf?msg=${messageId}&idx=${i}&render=1${a.filename ? `&name=${encodeURIComponent(a.filename)}` : ''}`}
                      target="_blank"
                      rel="noopener"
                      title="Converts to PDF in an isolated sandbox container (no network), then renders in the PDF.js viewer"
                    >Preview</a>
                  {:else}
                    <a
                      class="att-btn"
                      href={api.attachmentUrl(messageId, i, 'inline')}
                      target="_blank"
                      rel="noopener"
                      title="Open in a new tab"
                    >Preview</a>
                  {/if}
                {/if}
                <a
                  class="att-btn primary"
                  href={api.attachmentUrl(messageId, i, 'download')}
                  download={a.filename ?? `attachment-${i}`}
                  title="Save to disk"
                >Download</a>
                {#if !canPreview}
                  <span
                    class="att-warn"
                    title="This file type can run code on your device. Only open attachments from senders you trust."
                  >⚠</span>
                {/if}
              </span>
            </li>
          {/each}
        </ul>
      </div>
    {/if}

    {#if mode === 'html'}
      {#if bodyInfo?.trackers_blocked && bodyInfo.trackers_blocked.length > 0}
        <div class="banner tracker" title={bodyInfo.trackers_blocked.map((t) => `${t.service} — ${t.host}`).join('\n')}>
          <span>
            🛡 Blocked <strong>{bodyInfo.trackers_blocked.length}</strong>
            tracker{bodyInfo.trackers_blocked.length === 1 ? '' : 's'}
            {#if trackerServicesSummary(bodyInfo.trackers_blocked)}
              — <em>{trackerServicesSummary(bodyInfo.trackers_blocked)}</em>
            {/if}
          </span>
        </div>
      {/if}
      {#if bodyInfo?.has_remote_content}
        <div class="banner" class:active={allowRemote}>
          {#if !allowRemote}
            <span>🚫 Blocks remote content from
              <strong>{bodyInfo.remote_hosts.slice(0, 3).join(', ')}{bodyInfo.remote_hosts.length > 3 ? '…' : ''}</strong>
            </span>
            <button onclick={() => (allowRemote = true)}>Show</button>
          {:else}
            <span>👁 Loaded via proxy</span>
            <button onclick={() => (allowRemote = false)}>Block</button>
          {/if}
        </div>
      {/if}
      <div class="body-wrap">
        <iframe
          bind:this={iframe}
          title="Message body"
          {src}
          sandbox=""
          loading="eager"
          onload={onIframeLoad}
          style="height: {iframeHeight}px"
        ></iframe>
      </div>
    {:else if mode === 'plain'}
      <div class="text-wrap">
        {#if plainText === null}
          <p class="placeholder">Loading plain text…</p>
        {:else}
          <pre>{plainText}</pre>
        {/if}
      </div>
    {:else if mode === 'source'}
      <div class="text-wrap source">
        {#if rawText === null}
          <p class="placeholder">Loading source…</p>
        {:else}
          <pre>{rawText}</pre>
        {/if}
      </div>
    {:else if mode === 'forensics'}
      {#if !forensics}
        <p class="placeholder">Analyzing…</p>
      {:else}
        <div class="forensics">
          <div class="summary">
            <dl>
              <dt>Size</dt><dd>{humanBytes(forensics.size_bytes)}</dd>
              <dt>Attachments</dt><dd>{forensics.attachments.length}</dd>
              {#if forensics.spam_score !== null}
                <dt>Spam score</dt><dd>{forensics.spam_score}</dd>
              {/if}
            </dl>
            <dl class="auth">
              <dt>SPF</dt><dd class={verdictClass(forensics.auth.spf)}>{forensics.auth.spf}</dd>
              <dt>DKIM</dt><dd class={verdictClass(forensics.auth.dkim)}>{forensics.auth.dkim}</dd>
              <dt>DMARC</dt><dd class={verdictClass(forensics.auth.dmarc)}>{forensics.auth.dmarc}</dd>
            </dl>
            <dl class="sec">
              <dt>PGP encrypted</dt><dd class={forensics.is_pgp_encrypted ? 'ok' : 'unknown'}>{forensics.is_pgp_encrypted ? 'yes' : 'no'}</dd>
              <dt>PGP signed</dt><dd class={forensics.is_pgp_signed ? 'ok' : 'unknown'}>{forensics.is_pgp_signed ? 'yes' : 'no'}</dd>
              <dt>S/MIME encrypted</dt><dd class={forensics.is_smime_encrypted ? 'ok' : 'unknown'}>{forensics.is_smime_encrypted ? 'yes' : 'no'}</dd>
              <dt>S/MIME signed</dt><dd class={forensics.is_smime_signed ? 'ok' : 'unknown'}>{forensics.is_smime_signed ? 'yes' : 'no'}</dd>
            </dl>
          </div>

          {#if forensics.received_chain.length}
            <h3>Delivery path</h3>
            <ol class="chain">
              {#each forensics.received_chain.slice().reverse() as hop, i (i)}
                <li>
                  <span class="hop-label">hop {forensics.received_chain.length - i}</span>
                  <div class="hop-body">
                    {#if hop.from}<div><strong>from</strong> <code>{hop.from}</code></div>{/if}
                    {#if hop.by}<div><strong>by</strong> <code>{hop.by}</code></div>{/if}
                    {#if hop.with}<div><strong>with</strong> <code>{hop.with}</code></div>{/if}
                  </div>
                </li>
              {/each}
            </ol>
          {/if}

          {#if forensics.attachments.length}
            <h3>Attachments</h3>
            <ul class="atts">
              {#each forensics.attachments as a, i (i)}
                <li>
                  <span class="ft">{a.content_type}</span>
                  <span class="fn">{a.filename ?? '(unnamed)'}</span>
                  <span class="fs">{humanBytes(a.size_bytes)}</span>
                </li>
              {/each}
            </ul>
          {/if}

          <h3>Headers</h3>
          <table class="headers">
            <tbody>
              {#each forensics.headers as h, i (i)}
                <tr>
                  <th>{h.name}</th>
                  <td>{h.value}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
    {/if}
  {/if}
</section>

{#if pickerOpen && message}
  <FolderPicker
    accountId={message.account_id}
    exclude={message.labels}
    onPick={async (folder) => {
      pickerOpen = false;
      if (!message) return;
      try {
        await api.moveMessage(message.id, folder);
        load();
      } catch (e) {
        err = e instanceof Error ? e.message : String(e);
      }
    }}
    onClose={() => (pickerOpen = false)}
  />
{/if}

<style>
  section {
    width: 100%;
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: auto;
  }
  section.preview { padding: 1rem 1.1rem 1.35rem; }
  section.full { max-width: 54rem; padding: 2rem; }

  header {
    border-bottom: 1px solid var(--border);
    padding: 0 0 1rem;
    margin-bottom: 1rem;
  }
  .subject-row {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 1rem;
  }
  h1 {
    margin: 0 0 0.5rem;
    font-weight: 650;
    letter-spacing: -0.02em;
    font-size: 1.42rem;
    line-height: 1.3;
  }
  section.preview h1 { font-size: 1.1rem; }

  .header-badges {
    display: flex;
    gap: 0.35rem;
    align-items: center;
    flex-shrink: 0;
  }
  .badge {
    font-size: 0.95rem;
    padding: 0.28rem 0.5rem;
    border-radius: 999px;
    background: color-mix(in oklab, currentColor 8%, transparent);
    cursor: default;
  }
  .badge.enc {
    background: color-mix(in oklab, forestgreen 22%, transparent);
  }
  .badge.sig {
    background: color-mix(in oklab, dodgerblue 22%, transparent);
  }
  .open-full {
    color: inherit;
    text-decoration: none;
    font-size: 1.1rem;
    opacity: 0.55;
    padding: 0.25rem 0.5rem;
    border-radius: 0.3rem;
  }
  .open-full:hover {
    opacity: 1;
    background: color-mix(in oklab, currentColor 8%, transparent);
  }

  .meta {
    font-size: 0.83rem;
    opacity: 0.78;
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
  }
  .addr { opacity: 0.55; }
  .date { opacity: 0.55; font-size: 0.78rem; }
  .labels {
    margin-top: 0.8rem;
    display: flex;
    flex-wrap: wrap;
    gap: 0.35rem;
  }
  .label {
    font-size: 0.68rem;
    padding: 0.2rem 0.48rem;
    border: 1px solid var(--border);
    border-radius: 999px;
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    opacity: 0.7;
  }

  .toolbar-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    margin-bottom: 1rem;
    flex-wrap: wrap;
  }
  .mobile-tools-toggle {
    display: none;
  }
  .mobile-tools-btn {
    width: 100%;
    display: inline-flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    padding: 0.7rem 0.9rem;
    border-radius: 0.95rem;
    border: 1px solid color-mix(in oklab, currentColor 10%, transparent);
    background: color-mix(in oklab, var(--surface-2) 72%, transparent);
    color: inherit;
    font: inherit;
    font-size: 0.82rem;
    font-weight: 600;
    cursor: pointer;
  }
  .mobile-tools-btn:hover {
    background: color-mix(in oklab, currentColor 8%, transparent);
  }
  .actions-toolbar {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    flex-wrap: wrap;
  }
  /* Lockdown — visually + functionally lock the action buttons.
     pointer-events:none on action items so anchor clicks and
     button activations are silently dropped, paired with reduced
     opacity so the user reads the row as inactive. The server
     would 403 anyway; this is the UX hint that matches it. */
  .actions-toolbar.locked .action {
    pointer-events: none;
    opacity: 0.4;
    filter: saturate(0.4);
  }
  .lockdown-notice {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    padding: 0.32rem 0.7rem;
    border-radius: 999px;
    background: color-mix(in oklab, tomato 14%, transparent);
    color: color-mix(in oklab, tomato 80%, var(--fg));
    border: 1px solid color-mix(in oklab, tomato 30%, transparent);
    font-size: 0.74rem;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    margin-right: 0.2rem;
    /* Stays clickable for the tooltip even though siblings don't. */
    pointer-events: auto;
  }
  .action {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    padding: 0.48rem 0.82rem;
    background: color-mix(in oklab, var(--surface-2) 72%, transparent);
    color: inherit;
    text-decoration: none;
    border: 1px solid color-mix(in oklab, currentColor 8%, transparent);
    border-radius: 999px;
    font-size: 0.8rem;
    font-weight: 600;
  }
  .action:hover {
    background: color-mix(in oklab, currentColor 14%, transparent);
  }
  .action-sep {
    width: 1px;
    height: 16px;
    background: color-mix(in oklab, currentColor 12%, transparent);
    margin: 0 0.15rem;
  }
  .rule-dropdown { position: relative; }

  /* Inline rule card — appears between toolbar and body. */
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

  .view-toolbar {
    display: inline-flex;
    gap: 0.1rem;
    padding: 0.2rem;
    background: color-mix(in oklab, var(--surface-2) 78%, transparent);
    border: 1px solid color-mix(in oklab, currentColor 8%, transparent);
    border-radius: 999px;
  }
  .view-toolbar button {
    font: inherit;
    font-size: 0.76rem;
    font-weight: 500;
    padding: 0.38rem 0.82rem;
    border: 0;
    background: transparent;
    color: inherit;
    border-radius: 999px;
    cursor: pointer;
    opacity: 0.65;
  }
  .view-toolbar button:hover { opacity: 1; }
  .view-toolbar button.active {
    background: var(--surface);
    opacity: 1;
    box-shadow: 0 6px 16px rgba(0, 0, 0, 0.08);
  }

  .banner {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    padding: 0.5rem 0.75rem;
    margin-bottom: 0.75rem;
    border: 1px solid var(--border);
    border-radius: 0.8rem;
    font-size: 0.8rem;
    background: color-mix(in oklab, orange 10%, transparent);
  }
  .banner.active { background: color-mix(in oklab, dodgerblue 10%, transparent); }
  /* Trackers are a separate beast from "allow remote content" — shield
     icon, green tint, and no toggle button (tracker blocking is
     unconditional). */
  .banner.tracker {
    background: color-mix(in oklab, seagreen 11%, transparent);
    border-color: color-mix(in oklab, seagreen 35%, var(--border));
    cursor: help;
  }
  .banner.tracker em {
    font-style: normal;
    opacity: 0.75;
  }
  .banner strong {
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-weight: 500;
  }
  .banner button {
    margin-left: auto;
    font: inherit;
    font-size: 0.75rem;
    padding: 0.22rem 0.6rem;
    border: 1px solid color-mix(in oklab, currentColor 25%, transparent);
    background: transparent;
    color: inherit;
    border-radius: 0.3rem;
    cursor: pointer;
  }

  /* ── Attachment strip — above the body so it's always visible ── */
  .att-strip {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    margin-bottom: 0.85rem;
    padding: 0.65rem 0.9rem 0.75rem;
    border: 1px solid var(--border);
    border-radius: 0.9rem;
    background: color-mix(in oklab, var(--surface-2) 55%, transparent);
  }
  .att-label {
    font-size: 0.76rem;
    font-weight: 650;
    opacity: 0.72;
    letter-spacing: 0.01em;
  }
  ul.att-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
  }
  .att-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.6rem;
    padding: 0.4rem 0.6rem;
    border-radius: 0.55rem;
    background: color-mix(in oklab, var(--surface) 90%, transparent);
    border: 1px solid color-mix(in oklab, currentColor 7%, transparent);
    font-size: 0.82rem;
  }
  .att-name {
    display: flex;
    flex-direction: column;
    min-width: 0;
    gap: 0.1rem;
  }
  .att-fname {
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .att-meta {
    font-size: 0.7rem;
    opacity: 0.55;
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .att-actions {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    flex-shrink: 0;
  }
  .att-btn {
    padding: 0.3rem 0.7rem;
    border-radius: 0.4rem;
    border: 1px solid var(--border);
    background: var(--surface);
    color: inherit;
    text-decoration: none;
    font-size: 0.76rem;
    font-weight: 600;
  }
  .att-btn:hover {
    background: color-mix(in oklab, var(--accent) 14%, var(--surface));
    border-color: color-mix(in oklab, var(--accent) 42%, var(--border));
  }
  .att-btn.primary {
    background: color-mix(in oklab, var(--accent) 16%, var(--surface));
    border-color: color-mix(in oklab, var(--accent) 45%, var(--border));
  }
  /* Yellow warning icon for file types we won't render inline —
     hints that the user's OS will be the one opening it. */
  .att-warn {
    color: #fbbf24;
    font-size: 1rem;
    line-height: 1;
    cursor: help;
  }

  .body-wrap {
    flex: 1;
    border: 1px solid var(--border);
    border-radius: 1rem;
    background: var(--surface);
    overflow: hidden;
    min-height: 300px;
    box-shadow: 0 12px 28px rgba(0, 0, 0, 0.05);
  }
  iframe {
    display: block;
    width: 100%;
    border: 0;
    background: transparent;
  }

  .text-wrap {
    flex: 1;
    border: 1px solid var(--border);
    border-radius: 1rem;
    background: var(--surface);
    overflow: auto;
    padding: 1.15rem 1.25rem;
    box-shadow: 0 12px 28px rgba(0, 0, 0, 0.05);
  }
  .text-wrap pre {
    margin: 0;
    white-space: pre-wrap;
    word-wrap: break-word;
    font-family: inherit;
    font-size: 0.9rem;
    line-height: 1.55;
  }
  .text-wrap.source pre {
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 0.78rem;
    line-height: 1.5;
  }

  .forensics {
    flex: 1;
    overflow: auto;
  }
  .forensics h3 {
    font-size: 0.72rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    opacity: 0.55;
    font-weight: 600;
    margin: 1.5rem 0 0.6rem;
  }
  .summary {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(12rem, 1fr));
    gap: 0.75rem;
  }
  .summary dl {
    padding: 0.75rem 1rem;
    margin: 0;
    border: 1px solid var(--border);
    border-radius: 0.4rem;
    background: var(--surface);
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 0.35rem 1rem;
    font-size: 0.83rem;
  }
  .summary dt { opacity: 0.6; }
  .summary dd {
    margin: 0;
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 0.8rem;
  }
  .summary dd.ok { color: #12a150; font-weight: 600; }
  .summary dd.bad { color: #dc2626; font-weight: 600; }
  .summary dd.warn { color: #d97706; font-weight: 600; }
  .summary dd.unknown { opacity: 0.5; }

  ol.chain {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }
  ol.chain li {
    display: grid;
    grid-template-columns: 5rem 1fr;
    gap: 0.75rem;
    padding: 0.6rem 0.9rem;
    border: 1px solid var(--border);
    border-radius: 0.35rem;
    background: var(--surface);
    font-size: 0.83rem;
  }
  .hop-label {
    opacity: 0.55;
    font-size: 0.72rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }
  .hop-body code {
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 0.85em;
    background: color-mix(in oklab, currentColor 5%, transparent);
    padding: 0 0.3em;
    border-radius: 0.2em;
  }

  ul.atts {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  ul.atts li {
    display: grid;
    grid-template-columns: 10rem 1fr 5rem;
    gap: 0.75rem;
    padding: 0.5rem 0.9rem;
    border: 1px solid var(--border);
    border-radius: 0.3rem;
    margin-bottom: 0.25rem;
    font-size: 0.82rem;
  }
  .ft {
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 0.78rem;
    opacity: 0.75;
  }
  .fn { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .fs { text-align: right; opacity: 0.6; font-size: 0.78rem; }

  table.headers {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.8rem;
    border: 1px solid var(--border);
    border-radius: 0.35rem;
    overflow: hidden;
  }
  table.headers tr {
    border-bottom: 1px solid var(--border);
  }
  table.headers tr:last-child { border-bottom: 0; }
  table.headers th {
    text-align: left;
    font-weight: 500;
    vertical-align: top;
    padding: 0.35rem 0.75rem;
    width: 12rem;
    background: color-mix(in oklab, currentColor 3%, transparent);
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 0.76rem;
  }
  table.headers td {
    padding: 0.35rem 0.75rem;
    word-break: break-all;
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 0.76rem;
    line-height: 1.5;
  }

  .placeholder, .err {
    padding: 2rem;
    opacity: 0.55;
    text-align: center;
  }
  .err { color: #c83333; opacity: 1; }

  @media (max-width: 900px) {
    section.preview,
    section.full {
      padding: 0.7rem 0.85rem 1rem;
    }

    header {
      padding-bottom: 0.7rem;
      margin-bottom: 0.75rem;
    }
    .subject-row {
      flex-direction: column;
      gap: 0.45rem;
    }
    h1,
    section.preview h1 {
      font-size: 0.98rem;
      line-height: 1.32;
    }
    .header-badges {
      width: 100%;
      flex-wrap: wrap;
      gap: 0.25rem;
    }
    .badge {
      font-size: 0.82rem;
      padding: 0.22rem 0.42rem;
    }
    .open-full {
      font-size: 0.95rem;
      padding: 0.18rem 0.4rem;
    }
    .meta {
      gap: 0.12rem;
      font-size: 0.8rem;
    }
    .from strong {
      display: block;
      font-size: 0.84rem;
      line-height: 1.3;
    }
    .line,
    .date,
    .addr {
      font-size: 0.76rem;
    }
    .addr {
      display: block;
      overflow-wrap: anywhere;
    }
    .labels {
      margin-top: 0.55rem;
      gap: 0.28rem;
    }
    .label {
      font-size: 0.62rem;
      padding: 0.16rem 0.42rem;
    }

    .mobile-tools-toggle {
      display: block;
      margin-bottom: 0.7rem;
    }
    .toolbar-row {
      display: none;
      align-items: stretch;
      gap: 0.65rem;
      margin-bottom: 0.75rem;
    }
    .toolbar-row.mobile-drawer.open {
      display: flex;
      padding: 0.8rem 0.85rem;
      border: 1px solid color-mix(in oklab, currentColor 9%, transparent);
      border-radius: 0.95rem;
      background: color-mix(in oklab, var(--surface-2) 48%, var(--surface));
    }
    .actions-toolbar {
      width: 100%;
      flex-wrap: wrap;
      gap: 0.3rem;
    }
    .action {
      flex: 0 1 auto;
      justify-content: center;
      min-height: 36px;
      min-width: 0;
      padding: 0.42rem 0.68rem;
      font-size: 0.74rem;
      border-radius: 999px;
    }
    .action-sep {
      display: none;
    }
    .view-toolbar {
      width: 100%;
      padding: 0.15rem;
      overflow-x: auto;
      scrollbar-width: none;
    }
    .view-toolbar::-webkit-scrollbar {
      display: none;
    }
    .view-toolbar button {
      flex: 0 0 auto;
      white-space: nowrap;
      font-size: 0.72rem;
      padding: 0.34rem 0.7rem;
    }

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
      min-height: 42px;
    }

    .banner {
      align-items: flex-start;
      flex-wrap: wrap;
      gap: 0.5rem;
    }
    .banner button {
      margin-left: 0;
      min-height: 40px;
    }

    .body-wrap {
      min-height: 220px;
      border-radius: 0.85rem;
    }
    .text-wrap {
      padding: 0.95rem 0.9rem;
      border-radius: 0.85rem;
    }
    .text-wrap pre {
      font-size: 0.84rem;
      line-height: 1.5;
    }

    .summary {
      grid-template-columns: 1fr;
    }
    ol.chain li {
      grid-template-columns: 1fr;
      gap: 0.35rem;
    }
    ul.atts li {
      grid-template-columns: 1fr auto;
    }
    .fn {
      white-space: normal;
      overflow-wrap: anywhere;
    }
    table.headers,
    table.headers tbody,
    table.headers tr,
    table.headers th,
    table.headers td {
      display: block;
    }
    table.headers {
      border-radius: 0.85rem;
      overflow: hidden;
    }
    table.headers tr {
      padding: 0.55rem 0.7rem;
    }
    table.headers th,
    table.headers td {
      width: auto;
      padding: 0;
    }
    table.headers th {
      margin-bottom: 0.25rem;
      background: transparent;
      opacity: 0.62;
    }
    table.headers td {
      overflow-wrap: anywhere;
    }
  }

  .receipt-banner {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 0.65rem 1rem;
    margin: 0.6rem 0 0;
    padding: 0.6rem 0.85rem;
    border: 1px solid color-mix(in oklab, dodgerblue 35%, transparent);
    border-radius: 0.6rem;
    background: color-mix(in oklab, dodgerblue 8%, var(--surface));
    font-size: 0.84rem;
  }
  .receipt-text {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
    min-width: 0;
    flex: 1 1 18rem;
  }
  .receipt-text span {
    color: var(--muted);
    font-size: 0.78rem;
  }
  .receipt-actions {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
  }
  .receipt-send,
  .receipt-ignore {
    font: inherit;
    font-size: 0.78rem;
    padding: 0.32rem 0.7rem;
    border-radius: 999px;
    cursor: pointer;
    border: 1px solid color-mix(in oklab, currentColor 22%, transparent);
    background: transparent;
    color: inherit;
  }
  .receipt-send {
    background: dodgerblue;
    border-color: dodgerblue;
    color: white;
    font-weight: 500;
  }
  .receipt-send:hover {
    background: color-mix(in oklab, dodgerblue 85%, black);
  }
  .receipt-ignore:hover {
    background: color-mix(in oklab, currentColor 7%, transparent);
  }
  .receipt-status {
    font-size: 0.78rem;
    color: var(--muted);
  }
  .receipt-status.done {
    color: forestgreen;
  }
  .receipt-status.err {
    color: #c83333;
  }
</style>
