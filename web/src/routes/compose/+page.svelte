<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { get } from 'svelte/store';
  import { goto } from '$app/navigation';
  import { page } from '$app/stores';
  import {
    api,
    type Account,
    type MessageDetail,
    type SendAttachment,
    type SendForensics,
    type OutboxEntry
  } from '$lib/api';
  import { prefs } from '$lib/prefs';
  import { formatSender } from '$lib/format';
  import GrammarCheck from '$lib/components/GrammarCheck.svelte';
  import RewriteSelection from '$lib/components/RewriteSelection.svelte';
  import VoiceDictate from '$lib/components/VoiceDictate.svelte';

  let accounts = $state<Account[]>([]);
  let loadingAccounts = $state(true);

  let fromAccountId = $state<number | null>(null);
  let to = $state('');
  let cc = $state('');
  let bcc = $state('');
  let showCcBcc = $state(false);
  let subject = $state('');
  let body = $state('');

  // Body textarea ref + last-known selection range — driven by
  // `select`/`keyup`/`click` handlers on the textarea so the AI
  // polish widget always knows what to operate on. Updated again
  // by the document's selectionchange so cursor-only moves
  // (e.g. arrow keys) reset the highlight to a zero-length range.
  let bodyEl = $state<HTMLTextAreaElement | null>(null);
  let bodySelStart = $state(0);
  let bodySelEnd = $state(0);

  function captureBodySelection() {
    if (!bodyEl) return;
    bodySelStart = bodyEl.selectionStart;
    bodySelEnd = bodyEl.selectionEnd;
  }

  // Splice a rewritten replacement into `body`, then re-select the
  // replacement range so the user immediately sees what changed and
  // can Ctrl/Cmd-Z to undo if it's not what they wanted. Native
  // textarea undo is preserved by going through document.execCommand
  // when the textarea is focused — falls back to a plain assignment
  // when execCommand isn't available (older browsers, sandboxes).
  // Splice a dictated chunk at the current cursor position (or
  // append if the textarea was never focused). Smart-space: if the
  // existing text doesn't already end with whitespace and we're
  // mid-stream, add a single space before the chunk so words don't
  // run together. Capitalise the first letter when we're inserting
  // at the start of a sentence — the Web Speech API gives us
  // lowercase finalised chunks otherwise.
  function applyDictation(chunk: string) {
    const trimmed = chunk.trim();
    if (!trimmed) return;
    // Use the captured cursor directly. When the textarea was
    // never focused (fresh compose with a pre-filled signature),
    // both values are 0 and dictation lands at the start — i.e.
    // ABOVE the signature, which is what the user wants. The
    // earlier `|| body.length` fallback was wrong: it conflated
    // "cursor at 0" with "no cursor captured" and pushed the
    // dictated text after the signature.
    const start = bodySelStart;
    const end = bodySelEnd;
    const before = body.slice(0, start);
    const after = body.slice(end);
    const needsLeadingSpace =
      before.length > 0 && !/[\s\n]$/.test(before);
    const startsSentence =
      before.length === 0 || /[.!?]\s*$/.test(before);
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

  function applyRewrite(start: number, end: number, replacement: string) {
    if (!bodyEl) {
      body = body.slice(0, start) + replacement + body.slice(end);
      return;
    }
    bodyEl.focus();
    bodyEl.setSelectionRange(start, end);
    let inserted = false;
    try {
      // execCommand('insertText') is deprecated but still the only
      // path that participates in the textarea's native undo stack.
      // When it's available we use it; otherwise fall back.
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
  let attachments = $state<SendAttachment[]>([]);
  let pgpEncrypt = $state(false);
  let pgpSign = $state(false);
  let attachKey = $state(false);
  let pgpAutoDetected = $state(false);
  let pgpMissing = $state<string[]>([]);
  let pgpJustDiscovered = $state<string[]>([]);
  let pgpChecking = $state(false);

  // Auto-detect PGP: when To/Cc/Bcc change, check if all recipients
  // have public keys. If yes, auto-enable encryption. Addresses that
  // aren't in the local keyring trigger a WKD lookup server-side, so
  // Proton / Riseup / any WKD-publishing provider "just works" the
  // first time you type the address.
  let pgpCheckTimer: ReturnType<typeof setTimeout> | null = null;
  function checkPgpCapability() {
    if (pgpCheckTimer) clearTimeout(pgpCheckTimer);
    pgpCheckTimer = setTimeout(async () => {
      const allAddrs = [
        ...splitAddrs(to),
        ...splitAddrs(cc),
        ...splitAddrs(bcc)
      ].filter(Boolean);
      if (allAddrs.length === 0) {
        pgpAutoDetected = false;
        pgpMissing = [];
        pgpJustDiscovered = [];
        return;
      }
      pgpChecking = true;
      try {
        const result = await api.pgpCanEncrypt(allAddrs);
        pgpAutoDetected = result.can_encrypt;
        pgpMissing = result.missing;
        pgpJustDiscovered = result.imported ?? [];
        if (result.can_encrypt) {
          pgpEncrypt = true;
        }
      } catch {
        pgpAutoDetected = false;
      } finally {
        pgpChecking = false;
      }
    }, 500);
  }
  let inReplyTo = $state<string | null>(null);
  let references = $state<string | null>(null);
  let requestReceipt = $state(false);

  // Auto-complete state
  let suggestions = $state<string[]>([]);
  let acFocused = $state<'to' | 'cc' | 'bcc' | null>(null);
  let acTimer: ReturnType<typeof setTimeout> | null = null;

  async function onAddrInput(field: 'to' | 'cc' | 'bcc') {
    acFocused = field;
    if (acTimer) clearTimeout(acTimer);
    const raw = field === 'to' ? to : field === 'cc' ? cc : bcc;
    const last = raw.split(/[,;]/).pop()?.trim() ?? '';
    if (last.length < 2) {
      suggestions = [];
      return;
    }
    acTimer = setTimeout(async () => {
      try {
        suggestions = await api.autocomplete(last);
      } catch {
        suggestions = [];
      }
    }, 200);
  }

  function pickSuggestion(addr: string) {
    const setter = (old: string, addr: string) => {
      const parts = old.split(/[,;]/);
      parts.pop();
      parts.push(addr);
      return parts.join(', ').replace(/^,\s*/, '') + ', ';
    };
    if (acFocused === 'to') to = setter(to, addr);
    else if (acFocused === 'cc') cc = setter(cc, addr);
    else if (acFocused === 'bcc') bcc = setter(bcc, addr);
    suggestions = [];
    checkPgpCapability();
  }

  let sending = $state(false);
  let sent = $state<null | {
    message_id: string;
    appended: boolean;
    encrypted: boolean;
    forensics: SendForensics;
    // Frozen at dispatch so the success card can show "you sent this
    // to …" even after the undo window closes — handy when a user is
    // looking back at a stack of recent sends and needs to remember
    // exactly who got what.
    recap: {
      to: string;
      cc: string;
      bcc: string;
      subject: string;
    };
  }>(null);
  let err = $state<string | null>(null);

  // --- Outbox-driven send flow ---------------------------------------
  // Every send enqueues on the server and then either settles (worker
  // dispatches within seconds) or sits until a future scheduled_at. The
  // UI stays on this page showing a "Sent — Undo" countdown for the
  // pref-configured window, then polls the outbox for the final state.

  type SendChoice = 'now' | 'in5m' | 'in30m' | 'in1h' | 'tomorrow9' | 'custom';
  let sendChoice = $state<SendChoice>('now');
  let customScheduledAt = $state<string>(''); // datetime-local input
  let showScheduleMenu = $state(false);

  interface PendingState {
    outboxId: number;
    scheduledAt: number;
    /** Seconds remaining until the worker is free to pick this up. */
    countdown: number;
    /** Frozen draft values so Undo restores them verbatim. */
    draft: {
      fromAccountId: number | null;
      to: string;
      cc: string;
      bcc: string;
      showCcBcc: boolean;
      subject: string;
      body: string;
      attachments: SendAttachment[];
      pgpEncrypt: boolean;
      pgpSign: boolean;
      attachKey: boolean;
      requestReceipt: boolean;
      inReplyTo: string | null;
      references: string | null;
    };
  }
  let pending = $state<PendingState | null>(null);
  let pendingTimer: ReturnType<typeof setInterval> | null = null;
  let pollTimer: ReturnType<typeof setInterval> | null = null;
  let undoBusy = $state(false);

  function clearTimers() {
    if (pendingTimer) {
      clearInterval(pendingTimer);
      pendingTimer = null;
    }
    if (pollTimer) {
      clearInterval(pollTimer);
      pollTimer = null;
    }
  }

  onDestroy(() => {
    clearTimers();
  });

  // Query-string driven: ?reply=<msgId>, ?reply_all=<msgId>,
  // ?forward=<msgId>, ?account=<id>, ?to=<addr>
  let replyId = $derived(Number($page.url.searchParams.get('reply')) || null);
  let replyAllId = $derived(Number($page.url.searchParams.get('reply_all')) || null);
  let forwardId = $derived(Number($page.url.searchParams.get('forward')) || null);
  let composeMode = $derived(
    replyAllId ? 'reply_all' : replyId ? 'reply' : forwardId ? 'forward' : 'new'
  );

  onMount(async () => {
    try {
      accounts = await api.listAccounts();
      const hint = Number($page.url.searchParams.get('account')) || null;
      fromAccountId = hint ?? (accounts[0]?.id ?? null);

      const toHint = $page.url.searchParams.get('to');
      if (toHint) to = toHint;

      if (replyAllId) await prefillFromMessage(replyAllId, 'reply_all');
      else if (replyId) await prefillFromMessage(replyId, 'reply');
      else if (forwardId) await prefillFromMessage(forwardId, 'forward');
      maybeInsertSignature();
      checkPgpCapability();
    } catch (e) {
      err = e instanceof Error ? e.message : String(e);
    } finally {
      loadingAccounts = false;
    }
  });

  // Inject the selected account's signature into the draft. For new
  // messages we append with a standard "-- " delimiter; for replies
  // and forwards we only insert when the user has opted in, and we put
  // the signature above the quote block so it reads correctly.
  //
  // Idempotent: if a `-- ` marker is already present we leave the body
  // alone so re-firing this (e.g. after an account switch) doesn't
  // stack signatures.
  function maybeInsertSignature() {
    if (fromAccountId === null) return;
    const acct = accounts.find((a) => a.id === fromAccountId);
    const sig = acct?.signature_plain?.trim();
    if (!sig) return;
    if (body.includes('\n-- \n') || body.startsWith('-- \n')) return;
    const block = `-- \n${sig}\n`;
    if (composeMode === 'new') {
      body = body ? `${body}\n\n${block}` : `\n\n${block}`;
      return;
    }
    if (!get(prefs).signatureOnReplies) return;
    // Reply/forward prefill is `\n\n<header>\n> quoted…` — the leading
    // blank is the user's typing area. Insert the sig between that
    // area and the header.
    if (body.startsWith('\n\n')) {
      body = `\n\n${block}\n` + body.slice(2);
    } else {
      body = `\n\n${block}\n` + body;
    }
  }

  async function prefillFromMessage(id: number, mode: 'reply' | 'reply_all' | 'forward') {
    const m: MessageDetail = await api.getMessage(id);
    // Default sending from the account that received this message —
    // looked up first so reply-all knows which address to drop from
    // recipients (you don't want yourself on the To/Cc line).
    fromAccountId = m.account_id;
    const selfEmail = accounts.find((a) => a.id === m.account_id)?.email ?? '';

    if (mode === 'reply' && m.from_addr) {
      to = m.from_addr;
    } else if (mode === 'reply_all') {
      // To = sender + the original To recipients, minus self.
      const senderList = m.from_addr ? [m.from_addr] : [];
      const toList = splitAddrs(m.to_addrs ?? '');
      const ccList = splitAddrs(m.cc_addrs ?? '');
      const dedupedTo = dedupeAddrs([...senderList, ...toList], selfEmail);
      const dedupedCc = dedupeAddrs(ccList, selfEmail, dedupedTo);
      to = dedupedTo.join(', ');
      cc = dedupedCc.join(', ');
      if (dedupedCc.length > 0) showCcBcc = true;
    }
    const prefix = mode === 'forward' ? 'Fwd: ' : 'Re: ';
    const subj = m.subject || '';
    subject = subj.toLowerCase().startsWith(prefix.toLowerCase()) ? subj : prefix + subj;
    // Thread headers so the reply stitches into the conversation.
    if (mode === 'reply' || mode === 'reply_all') {
      inReplyTo = m.message_id;
      references = m.message_id;
    }
    // Quote body — lazy plain-text fetch so we don't wait on the HTML path.
    try {
      const r = await api.getMessagePlain(m.id);
      const quoted = r.text
        .split('\n')
        .map((line) => `> ${line}`)
        .join('\n');
      const header = mode === 'forward'
        ? `\n--- Forwarded message ---\nFrom: ${m.from_addr ?? ''}\nDate: ${new Date(m.date_utc * 1000).toLocaleString()}\nSubject: ${m.subject ?? ''}\nTo: ${m.to_addrs ?? ''}\n\n`
        : `On ${new Date(m.date_utc * 1000).toLocaleString()}, ${formatSender(m.from_addr)} wrote:\n`;
      body = `\n\n${header}${quoted}`;
    } catch {
      /* body-fetch optional */
    }
  }

  /// Lower-case the local part for dedup/comparison only — display
  /// strings keep their original casing.
  function normalizeAddr(raw: string): string {
    const m = raw.match(/<([^>]+)>/);
    const addr = (m ? m[1] : raw).trim().toLowerCase();
    return addr;
  }

  /// Drop empties, drop self, drop anything already in `existing`,
  /// preserving order.
  function dedupeAddrs(
    list: string[],
    self: string,
    existing: string[] = []
  ): string[] {
    const seen = new Set<string>();
    if (self) seen.add(self.toLowerCase());
    for (const e of existing) seen.add(normalizeAddr(e));
    const out: string[] = [];
    for (const raw of list) {
      const trimmed = raw.trim();
      if (!trimmed) continue;
      const key = normalizeAddr(trimmed);
      if (!key || seen.has(key)) continue;
      seen.add(key);
      out.push(trimmed);
    }
    return out;
  }

  function splitAddrs(s: string): string[] {
    return s
      .split(/[,\n;]/)
      .map((a) => a.trim())
      .filter(Boolean);
  }

  async function onFiles(e: Event) {
    const input = e.currentTarget as HTMLInputElement;
    if (!input.files) return;
    const next: SendAttachment[] = [];
    for (const f of Array.from(input.files)) {
      const buf = await f.arrayBuffer();
      next.push({
        filename: f.name,
        content_type: f.type || 'application/octet-stream',
        data_base64: bufToBase64(buf)
      });
    }
    attachments = [...attachments, ...next];
    input.value = '';
  }

  function bufToBase64(buf: ArrayBuffer): string {
    const bytes = new Uint8Array(buf);
    let bin = '';
    for (const b of bytes) bin += String.fromCharCode(b);
    return btoa(bin);
  }

  function removeAttachment(i: number) {
    attachments = attachments.filter((_, idx) => idx !== i);
  }

  // Compute the scheduled_at timestamp (Unix seconds) based on the
  // picker + user preferences. Returns null when the user picked a
  // malformed custom time.
  function resolveScheduledAt(): number | null {
    const now = Math.floor(Date.now() / 1000);
    switch (sendChoice) {
      case 'now': {
        const delay = Math.max(0, Math.min(60, get(prefs).sendUndoSecs));
        return now + delay;
      }
      case 'in5m':
        return now + 5 * 60;
      case 'in30m':
        return now + 30 * 60;
      case 'in1h':
        return now + 60 * 60;
      case 'tomorrow9': {
        const d = new Date();
        d.setDate(d.getDate() + 1);
        d.setHours(9, 0, 0, 0);
        return Math.floor(d.getTime() / 1000);
      }
      case 'custom': {
        if (!customScheduledAt) return null;
        const ts = Math.floor(new Date(customScheduledAt).getTime() / 1000);
        if (!Number.isFinite(ts) || ts <= now) return null;
        return ts;
      }
    }
  }

  async function submit(e: Event) {
    e.preventDefault();
    if (fromAccountId === null) {
      err = 'Choose a From account';
      return;
    }
    const toAddrs = splitAddrs(to);
    if (toAddrs.length === 0) {
      err = 'At least one recipient (To) is required';
      return;
    }
    const scheduledAt = resolveScheduledAt();
    if (scheduledAt === null) {
      err = 'Pick a valid future time for the scheduled send';
      return;
    }
    sending = true;
    err = null;
    try {
      const resp = await api.sendMessage({
        account_id: fromAccountId,
        to: toAddrs,
        cc: splitAddrs(cc),
        bcc: splitAddrs(bcc),
        subject,
        body,
        attachments,
        in_reply_to: inReplyTo ?? undefined,
        references: references ?? undefined,
        pgp_encrypt: pgpEncrypt,
        request_receipt: requestReceipt,
        scheduled_at: scheduledAt
      });
      // Freeze a draft snapshot so Undo restores the compose form
      // verbatim — vital after an undo window because the user may
      // have already clicked away visually.
      pending = {
        outboxId: resp.outbox_id,
        scheduledAt: resp.scheduled_at,
        countdown: Math.max(0, resp.scheduled_at - Math.floor(Date.now() / 1000)),
        draft: {
          fromAccountId,
          to,
          cc,
          bcc,
          showCcBcc,
          subject,
          body,
          attachments: [...attachments],
          pgpEncrypt,
          pgpSign,
          attachKey,
          requestReceipt,
          inReplyTo,
          references
        }
      };
      clearTimers();
      pendingTimer = setInterval(tickPending, 1000);
      // Poll outbox state after the undo window + a little slack so we
      // catch the sent/failed transition as quickly as the worker tick.
      pollTimer = setInterval(pollPending, 3000);
    } catch (e) {
      err = e instanceof Error ? e.message : String(e);
    } finally {
      sending = false;
    }
  }

  function tickPending() {
    if (!pending) return;
    const now = Math.floor(Date.now() / 1000);
    pending = { ...pending, countdown: Math.max(0, pending.scheduledAt - now) };
  }

  async function pollPending() {
    if (!pending) return;
    try {
      const entry: OutboxEntry = await api.outboxGet(pending.outboxId);
      if (entry.status === 'sent') {
        clearTimers();
        let forensics: SendForensics | null = null;
        try {
          if (entry.forensics_json) forensics = JSON.parse(entry.forensics_json);
        } catch {
          /* ignore — we'll show sent_folder=null as a fallback */
        }
        sent = {
          message_id: entry.sent_message_id ?? '(pending)',
          appended: forensics?.sent_folder != null && !forensics.sent_folder.includes('APPEND failed'),
          encrypted: false, // the outbox stores forensics; "encrypted" is in the payload, redundant here
          forensics: forensics ?? {
            sent_at_utc: entry.updated_at,
            smtp_host: '',
            smtp_port: 0,
            recipient_count: 0,
            raw_size_bytes: 0,
            bind_iface: null,
            vpn_enabled: false,
            vpn_interface_up: false,
            vpn_exit_ip: null,
            vpn_provider: null,
            vpn_region_label: null,
            vpn_server_country_code: null,
            vpn_server_city: null,
            vpn_server_number: null,
            killswitch_enabled: false,
            autocrypt_attached: false,
            sent_folder: null
          },
          recap: {
            to: pending.draft.to,
            cc: pending.draft.cc,
            bcc: pending.draft.bcc,
            subject: pending.draft.subject
          }
        };
        pending = null;
      } else if (entry.status === 'failed') {
        clearTimers();
        err = entry.last_error ?? 'Send failed.';
        // Restore the draft so the user can fix and retry.
        restoreDraft();
        pending = null;
      } else if (entry.status === 'cancelled') {
        clearTimers();
        restoreDraft();
        pending = null;
      }
      // status still 'pending' or 'sending': keep polling
    } catch {
      // Network blip — keep polling.
    }
  }

  function restoreDraft() {
    if (!pending) return;
    const d = pending.draft;
    fromAccountId = d.fromAccountId;
    to = d.to;
    cc = d.cc;
    bcc = d.bcc;
    showCcBcc = d.showCcBcc;
    subject = d.subject;
    body = d.body;
    attachments = d.attachments;
    pgpEncrypt = d.pgpEncrypt;
    pgpSign = d.pgpSign;
    attachKey = d.attachKey;
    requestReceipt = d.requestReceipt;
    inReplyTo = d.inReplyTo;
    references = d.references;
  }

  async function undoSend() {
    if (!pending) return;
    undoBusy = true;
    try {
      await api.outboxCancel(pending.outboxId);
      clearTimers();
      restoreDraft();
      pending = null;
    } catch (e) {
      // 409 = worker already dispatched. Keep polling — status will
      // move to sent/failed on its own.
      const msg = e instanceof Error ? e.message : String(e);
      err = `Undo too late: ${msg}`;
    } finally {
      undoBusy = false;
    }
  }

  function totalSizeKb(): number {
    return Math.round(
      attachments.reduce((a, x) => a + (x.data_base64.length * 3) / 4, 0) / 1024
    );
  }
</script>

<article class="compose-shell" class:reply-mode={composeMode === 'reply' || composeMode === 'reply_all'} class:forward-mode={composeMode === 'forward'}>
  <div class="page-top">
    <a class="back" href="/inbox">← Inbox</a>
  </div>

  <header class="hero" class:compact-hero={composeMode !== 'new'}>
    <div class="hero-copy">
      <span class="eyebrow">Outbound Channel</span>
      <h1>{composeMode === 'reply_all' ? 'Reply all' : composeMode === 'reply' ? 'Reply' : composeMode === 'forward' ? 'Forward' : 'New message'}</h1>
      <p>
        Draft inside a cleaner operational console with room for encryption controls,
        attachments, and long-form mail without the cramped old layout.
      </p>
    </div>
    <div class="hero-side">
      <span class="hero-chip">PGP-ready</span>
      <span class="hero-chip">Local autocomplete</span>
      <span class="hero-chip">Attachment staging</span>
    </div>
  </header>

  {#if pending}
    <div class="pending">
      {#if pending.countdown > 0 && sendChoice === 'now'}
        <div class="pending-head">
          <strong>Sent.</strong>
          <span class="pending-countdown">Undo in <strong>{pending.countdown}s</strong></span>
        </div>
        <p class="pending-hint">
          The message is held in the outbox. Click Undo to bring it
          back and edit, or ignore this banner to let it dispatch.
        </p>
      {:else if pending.countdown > 0}
        <div class="pending-head">
          <strong>Scheduled.</strong>
          <span class="pending-countdown">
            Dispatches {new Date(pending.scheduledAt * 1000).toLocaleString()}
          </span>
        </div>
        <p class="pending-hint">
          Sitting in the outbox until dispatch time. Manage or cancel
          from the <a href="/outbox">outbox page</a>.
        </p>
      {:else}
        <div class="pending-head">
          <strong>Dispatching…</strong>
          <span class="pending-countdown">Waiting on the worker</span>
        </div>
        <p class="pending-hint">
          The undo window has passed. Forensics appear below once SMTP
          confirms.
        </p>
      {/if}

      <!-- Recap so mistakes (wrong recipient, wrong subject) are
           visible while the undo window is still open. Full
           unsummarized recipient list — truncation would defeat the
           whole "catch the mistake" point. -->
      <dl class="recap">
        <dt>To</dt>
        <dd>{pending.draft.to || '(none)'}</dd>
        {#if pending.draft.cc.trim()}
          <dt>Cc</dt>
          <dd>{pending.draft.cc}</dd>
        {/if}
        {#if pending.draft.bcc.trim()}
          <dt>Bcc</dt>
          <dd>{pending.draft.bcc}</dd>
        {/if}
        <dt>Subject</dt>
        <dd class="recap-subject">{pending.draft.subject || '(no subject)'}</dd>
      </dl>

      <div class="pending-actions">
        <button
          class="ghost"
          onclick={undoSend}
          disabled={undoBusy || pending.countdown === 0}
        >{undoBusy ? 'Undoing…' : 'Undo'}</button>
        <button class="ghost" onclick={() => goto('/outbox')}>View outbox</button>
      </div>
    </div>
  {:else if sent}
    <div class="success">
      <p><strong>Sent.</strong> Message-ID: <code>{sent.message_id}</code></p>
      {#if sent.appended}<p>Filed in the Sent folder.</p>{/if}

      <dl class="recap">
        <dt>To</dt>
        <dd>{sent.recap.to || '(none)'}</dd>
        {#if sent.recap.cc.trim()}
          <dt>Cc</dt>
          <dd>{sent.recap.cc}</dd>
        {/if}
        {#if sent.recap.bcc.trim()}
          <dt>Bcc</dt>
          <dd>{sent.recap.bcc}</dd>
        {/if}
        <dt>Subject</dt>
        <dd class="recap-subject">{sent.recap.subject || '(no subject)'}</dd>
      </dl>

      <details class="forensics" open>
        <summary>Forensics</summary>
        <dl>
          <dt>Dispatched</dt>
          <dd>{new Date(sent.forensics.sent_at_utc * 1000).toISOString()}</dd>

          <dt>SMTP host</dt>
          <dd><code>{sent.forensics.smtp_host}:{sent.forensics.smtp_port}</code></dd>

          <dt>Recipients</dt>
          <dd>{sent.forensics.recipient_count}</dd>

          <dt>Payload size</dt>
          <dd>{sent.forensics.raw_size_bytes.toLocaleString()} bytes</dd>

          <dt>Encryption</dt>
          <dd>
            {#if sent.encrypted}PGP (to all recipients)
            {:else if sent.forensics.autocrypt_attached}Plain — Autocrypt header attached
            {:else}Plain
            {/if}
          </dd>

          <dt>Routed through</dt>
          <dd>
            {#if sent.forensics.vpn_interface_up && sent.forensics.bind_iface}
              VPN tunnel (<code>{sent.forensics.bind_iface}</code>)
            {:else if sent.forensics.vpn_enabled}
              VPN enabled but tunnel down — <strong>direct</strong>
            {:else}
              Direct / host network
            {/if}
          </dd>

          {#if sent.forensics.vpn_interface_up}
            <dt>VPN provider</dt>
            <dd>
              {sent.forensics.vpn_provider ?? 'unknown'}
              {#if sent.forensics.vpn_region_label} · {sent.forensics.vpn_region_label}{/if}
            </dd>

            <dt>Exit</dt>
            <dd>
              {#if sent.forensics.vpn_exit_ip}<code>{sent.forensics.vpn_exit_ip}</code>{:else}<em>unknown</em>{/if}
              {#if sent.forensics.vpn_server_country_code} · {sent.forensics.vpn_server_country_code.toUpperCase()}{/if}
              {#if sent.forensics.vpn_server_city} / {sent.forensics.vpn_server_city}{/if}
              {#if sent.forensics.vpn_server_number} · #{sent.forensics.vpn_server_number}{/if}
            </dd>
          {/if}

          <dt>Kill-switch</dt>
          <dd>{sent.forensics.killswitch_enabled ? 'on — non-wg0 egress blocked' : 'off'}</dd>

          <dt>Sent copy</dt>
          <dd>
            {#if sent.appended}
              filed in <code>{sent.forensics.sent_folder ?? 'Sent'}</code>
            {:else if sent.forensics.sent_folder?.includes('auto-filed')}
              auto-filed by the provider (<code>{sent.forensics.sent_folder.replace(' (auto-filed by Gmail)', '')}</code>)
            {:else if sent.forensics.sent_folder}
              <em>attempted <code>{sent.forensics.sent_folder}</code>, APPEND failed</em>
            {:else}
              <em>no copy filed</em>
            {/if}
          </dd>
        </dl>
      </details>

      <div class="actions">
        <button onclick={() => goto('/inbox')}>Back to inbox</button>
        <button onclick={() => location.reload()}>Write another</button>
      </div>
    </div>
  {:else}
    <form onsubmit={submit} class="compose">
      <div class="row">
        <label for="from">From</label>
        <select id="from" bind:value={fromAccountId} disabled={loadingAccounts}>
          {#each accounts as a (a.id)}
            <option value={a.id}>
              {a.display_name ? `${a.display_name} <${a.email}>` : a.email}
            </option>
          {/each}
        </select>
      </div>

      <div class="row to-row">
        <label for="to">To</label>
        <div class="ac-wrap">
          <input
            id="to"
            type="text"
            bind:value={to}
            placeholder="alice@example.com, bob@example.com"
            autocomplete="off"
            oninput={() => { onAddrInput('to'); checkPgpCapability(); }}
            onfocus={() => (acFocused = 'to')}
            onblur={() => setTimeout(() => { if (acFocused === 'to') suggestions = []; checkPgpCapability(); }, 200)}
          />
          {#if suggestions.length > 0 && acFocused === 'to'}
            <ul class="ac-list">
              {#each suggestions as s (s)}
                <li><button type="button" onmousedown={(e) => { e.preventDefault(); pickSuggestion(s); }}>{s}</button></li>
              {/each}
            </ul>
          {/if}
        </div>
      </div>

      {#if !showCcBcc}
        <div class="row">
          <span></span>
          <button type="button" class="linklike" onclick={() => (showCcBcc = true)}>Add Cc / Bcc</button>
        </div>
      {:else}
        <div class="row">
          <label for="cc">Cc</label>
          <input id="cc" type="text" bind:value={cc} autocomplete="off" />
        </div>
        <div class="row">
          <label for="bcc">Bcc</label>
          <input id="bcc" type="text" bind:value={bcc} autocomplete="off" />
        </div>
      {/if}

      <div class="row">
        <label for="subject">Subject</label>
        <input id="subject" type="text" bind:value={subject} autocomplete="off" />
      </div>

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

      <div class="row">
        <label for="attachments-input">Attachments</label>
        <div class="atts">
          {#each attachments as a, i (i)}
            <span class="att">
              📎 {a.filename}
              <button type="button" class="x" onclick={() => removeAttachment(i)} aria-label="Remove">×</button>
            </span>
          {/each}
          <label class="add-att">
            + Add
            <input id="attachments-input" type="file" multiple onchange={onFiles} />
          </label>
          {#if attachments.length > 0}
            <span class="att-size">{totalSizeKb()} KB total</span>
          {/if}
        </div>
      </div>

      {#if err}
        <div class="err">⚠ {err}</div>
      {/if}

      <div class="compose-bar">
        <div class="pgp-toggles">
          <button
            type="button"
            class="pgp-icon"
            class:active={pgpEncrypt}
            onclick={() => (pgpEncrypt = !pgpEncrypt)}
            title={pgpEncrypt
              ? 'PGP encrypt ON — click to disable'
              : pgpAutoDetected
                ? 'All recipients have keys — PGP encryption available'
                : pgpMissing.length > 0
                  ? `Missing keys for: ${pgpMissing.join(', ')}`
                  : 'PGP encrypt OFF — click to enable (needs recipient public keys)'}
          >
            <svg viewBox="0 0 20 20" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
              <rect x="4.5" y="9" width="11" height="8" rx="1.2"/>
              <path d="M7 9V6.5a3 3 0 0 1 6 0V9"/>
            </svg>
          </button>
          {#if pgpChecking}
            <span class="pgp-status checking" title="Looking up recipient keys…">…</span>
          {:else if pgpJustDiscovered.length > 0 && pgpAutoDetected}
            <span
              class="pgp-status discovered"
              title="Auto-discovered via WKD: {pgpJustDiscovered.join(', ')}"
            >+wkd</span>
          {:else if pgpAutoDetected && pgpEncrypt}
            <span class="pgp-status ok" title="All recipients have public keys">auto</span>
          {:else if pgpMissing.length > 0}
            <span class="pgp-status warn" title="No PGP key found for: {pgpMissing.join(', ')}">!</span>
          {/if}
          <button
            type="button"
            class="pgp-icon"
            class:active={pgpSign}
            onclick={() => (pgpSign = !pgpSign)}
            title={pgpSign ? 'PGP sign ON — click to disable' : 'PGP sign OFF — click to enable (proves sender identity)'}
          >
            <svg viewBox="0 0 20 20" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
              <path d="M10 18s-6-3.5-6-8.5V4l6-2 6 2v5.5c0 5-6 8.5-6 8.5z"/>
              <path d="M7.5 10l2 2 3.5-4"/>
            </svg>
          </button>
          <button
            type="button"
            class="pgp-icon"
            class:active={attachKey}
            onclick={() => (attachKey = !attachKey)}
            title={attachKey ? 'Attach public key ON — recipient can encrypt replies to you' : 'Attach public key OFF — click to attach your public key'}
          >
            <svg viewBox="0 0 20 20" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
              <circle cx="8" cy="8" r="4"/>
              <path d="M11 11 17 17"/>
              <path d="M15 13v4h-4"/>
            </svg>
          </button>
          <button
            type="button"
            class="pgp-icon"
            class:active={requestReceipt}
            onclick={() => (requestReceipt = !requestReceipt)}
            title={requestReceipt
              ? 'Request read receipt ON — recipient is asked to confirm the message was opened'
              : 'Request read receipt OFF — click to ask recipient to confirm read'}
          >
            <svg viewBox="0 0 20 20" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
              <path d="M3.5 6 10 11l6.5-5"/>
              <rect x="3.5" y="5" width="13" height="10" rx="1.2"/>
              <path d="M6.5 16.5 9 14"/>
              <path d="M11 14l2.5 2.5"/>
            </svg>
          </button>
        </div>

        <div class="bar-actions">
          <button type="button" class="ghost" onclick={() => history.back()}>Cancel</button>
          <div class="schedule-group">
            <button type="submit" class="primary" disabled={sending}>
              <svg viewBox="0 0 20 20" width="16" height="16" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
                <path d="m2 10 16-7.5L13 14l-3-3.5L2 10Z"/>
                <path d="M10 10.5 18 2.5"/>
              </svg>
              {sending
                ? 'Queueing…'
                : sendChoice === 'now'
                  ? ($prefs.sendUndoSecs > 0 ? `Send (undo ${$prefs.sendUndoSecs}s)` : 'Send')
                  : sendChoice === 'in5m'
                    ? 'Send in 5 min'
                    : sendChoice === 'in30m'
                      ? 'Send in 30 min'
                      : sendChoice === 'in1h'
                        ? 'Send in 1 h'
                        : sendChoice === 'tomorrow9'
                          ? 'Send tomorrow 9 AM'
                          : 'Send at custom time'}
            </button>
            <button
              type="button"
              class="schedule-toggle"
              title="Schedule send"
              aria-label="Schedule options"
              onclick={() => (showScheduleMenu = !showScheduleMenu)}
            >▾</button>
            {#if showScheduleMenu}
              <div class="schedule-menu" role="menu">
                <button type="button" role="menuitemradio" aria-checked={sendChoice === 'now'} onclick={() => { sendChoice = 'now'; showScheduleMenu = false; }}>
                  Send now
                  {#if $prefs.sendUndoSecs > 0}<span class="hint">{$prefs.sendUndoSecs}s undo window</span>{/if}
                </button>
                <button type="button" role="menuitemradio" aria-checked={sendChoice === 'in5m'} onclick={() => { sendChoice = 'in5m'; showScheduleMenu = false; }}>In 5 minutes</button>
                <button type="button" role="menuitemradio" aria-checked={sendChoice === 'in30m'} onclick={() => { sendChoice = 'in30m'; showScheduleMenu = false; }}>In 30 minutes</button>
                <button type="button" role="menuitemradio" aria-checked={sendChoice === 'in1h'} onclick={() => { sendChoice = 'in1h'; showScheduleMenu = false; }}>In 1 hour</button>
                <button type="button" role="menuitemradio" aria-checked={sendChoice === 'tomorrow9'} onclick={() => { sendChoice = 'tomorrow9'; showScheduleMenu = false; }}>Tomorrow at 9 AM</button>
                <label class="schedule-custom">
                  <span>Custom:</span>
                  <input
                    type="datetime-local"
                    bind:value={customScheduledAt}
                    onchange={() => (sendChoice = 'custom')}
                  />
                </label>
              </div>
            {/if}
          </div>
        </div>
      </div>
    </form>
  {/if}
</article>

<style>
  article.compose-shell {
    max-width: 82rem;
    width: 100%;
    margin: 0 auto;
    padding: 1.25rem 1.75rem 2rem;
    box-sizing: border-box;
    min-height: 100vh;
    display: flex;
    flex-direction: column;
  }
  .page-top {
    margin-bottom: 0.55rem;
  }
  .hero {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 1rem;
    padding: 1.3rem 1.4rem;
    margin-bottom: 1rem;
    border: 1px solid var(--border);
    border-radius: 1.3rem;
    background:
      radial-gradient(circle at top right, color-mix(in oklab, var(--accent) 12%, transparent), transparent 35%),
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
  header h1 { flex-shrink: 0; }
  .hero h1 {
    margin: 0 0 0.45rem;
    font-weight: 650;
    font-size: 2rem;
    letter-spacing: -0.03em;
  }
  .hero p {
    margin: 0;
    max-width: 46rem;
    color: var(--muted);
    line-height: 1.55;
  }
  .hero-side {
    display: flex;
    flex-wrap: wrap;
    gap: 0.45rem;
    justify-content: flex-end;
    align-content: start;
  }
  .hero-chip {
    display: inline-flex;
    align-items: center;
    padding: 0.42rem 0.72rem;
    border-radius: 999px;
    background: color-mix(in oklab, var(--surface-2) 82%, transparent);
    border: 1px solid color-mix(in oklab, currentColor 8%, transparent);
    font-size: 0.72rem;
    font-weight: 600;
  }
  form.compose {
    flex: 1;
    min-height: 0;
    padding: 1rem 1.1rem 1.2rem;
    border: 1px solid var(--border);
    border-radius: 1.3rem;
    background: color-mix(in oklab, var(--surface) 94%, transparent);
    box-shadow: 0 16px 36px rgba(0, 0, 0, 0.05);
  }
  /* Body row stretches to fill vertical space — body textarea grows with
     the window instead of stopping at a fixed 18 rows. */
  .body-row {
    flex: 1;
    min-height: 0;
  }
  .body-row textarea {
    min-height: 60vh;
    height: 100%;
    resize: vertical;
  }
  .back {
    display: inline-block;
    color: inherit;
    text-decoration: none;
    opacity: 0.65;
    font-size: 0.78rem;
  }
  .back:hover { opacity: 1; }

  form.compose {
    display: flex;
    flex-direction: column;
    gap: 0.7rem;
  }
  .row {
    display: grid;
    grid-template-columns: 5rem 1fr;
    gap: 1rem;
    align-items: start;
    padding: 0.9rem 1rem;
    border: 1px solid color-mix(in oklab, currentColor 8%, transparent);
    border-radius: 1rem;
    background: color-mix(in oklab, var(--surface) 98%, transparent);
  }
  .row label {
    font-size: 0.78rem;
    opacity: 0.7;
    padding-top: 0.65rem;
    font-weight: 600;
  }
  .row input,
  .row select,
  .row textarea {
    font: inherit;
    font-size: 0.88rem;
    padding: 0.7rem 0.8rem;
    border: 1px solid color-mix(in oklab, currentColor 18%, transparent);
    border-radius: 0.85rem;
    background: color-mix(in oklab, var(--surface-2) 72%, transparent);
    color: inherit;
    box-sizing: border-box;
    width: 100%;
  }
  .row textarea {
    resize: vertical;
    font-family: inherit;
    line-height: 1.55;
  }
  .row input:focus, .row select:focus, .row textarea:focus {
    outline: none;
    border-color: color-mix(in oklab, var(--accent) 32%, transparent);
    box-shadow: 0 0 0 3px color-mix(in oklab, var(--accent) 12%, transparent);
  }

  /* GrammarCheck + RewriteSelection live in the same row, stacked. */
  .assist-stack {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    min-width: 0;
  }
  /* Body textarea + dictation toolbar share the column under the
     "Body" label so the mic button is right next to where the
     transcribed text will land. */
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

  /* Auto-complete dropdown */
  .ac-wrap {
    position: relative;
    /* Establish a stacking context AT the wrap level, with a high
       z-index so the dropdown — which positions absolutely against
       this wrap — paints above sibling .row elements that follow it
       in the form (Cc/Bcc, Subject, etc.). Without this, the
       dropdown rendered behind those rows because each .row paints
       its own opaque background and the default z-index ordering
       puts later siblings on top of earlier ones' overflow. */
    z-index: 50;
  }
  /* The To row hosts the dropdown — make sure it wins over the
     subsequent row(s) too. Same trick: relative + a z-index that
     clearly beats anything below it in the form. Class-based
     instead of :has(.ac-wrap) because Safari < 15.4 and older
     mobile browsers don't support :has() — the user reported
     the dropdown still rendered behind cc/bcc on phone + spare
     laptop because of that gap. Explicit class works everywhere. */
  .row.to-row {
    position: relative;
    z-index: 50;
  }
  .ac-list {
    position: absolute;
    top: 100%;
    left: 0;
    right: 0;
    z-index: 100;
    list-style: none;
    margin: 2px 0 0;
    padding: 0.25rem 0;
    background: var(--surface, #fff);
    border: 1px solid var(--border);
    border-radius: 0.9rem;
    box-shadow: 0 6px 18px rgba(0, 0, 0, 0.14);
    max-height: 200px;
    overflow-y: auto;
  }
  .ac-list li button {
    display: block;
    width: 100%;
    text-align: left;
    padding: 0.4rem 0.65rem;
    background: transparent;
    border: 0;
    color: inherit;
    font: inherit;
    font-size: 0.85rem;
    cursor: pointer;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .ac-list li button:hover {
    background: color-mix(in oklab, currentColor 8%, transparent);
  }

  .linklike {
    background: transparent;
    border: 0;
    color: inherit;
    cursor: pointer;
    font-size: 0.78rem;
    opacity: 0.72;
    text-decoration: underline;
    padding: 0.55rem 0;
    text-align: left;
  }
  .linklike:hover { opacity: 1; }

  .atts {
    display: flex;
    flex-wrap: wrap;
    gap: 0.45rem;
    align-items: center;
  }
  .att {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    padding: 0.38rem 0.62rem;
    background: color-mix(in oklab, currentColor 7%, transparent);
    border-radius: 999px;
    font-size: 0.78rem;
  }
  .att .x {
    border: 0;
    background: transparent;
    color: inherit;
    opacity: 0.55;
    cursor: pointer;
    padding: 0 0.2rem;
  }
  .att .x:hover { opacity: 1; }
  .add-att {
    display: inline-flex;
    align-items: center;
    padding: 0.42rem 0.68rem;
    border: 1px dashed color-mix(in oklab, currentColor 25%, transparent);
    border-radius: 999px;
    font-size: 0.78rem;
    cursor: pointer;
    opacity: 0.75;
  }
  .add-att:hover { opacity: 1; background: color-mix(in oklab, currentColor 5%, transparent); }
  .add-att input[type='file'] { display: none; }
  .att-size {
    opacity: 0.5;
    font-size: 0.75rem;
  }

  .err {
    margin-top: 0.5rem;
    padding: 0.55rem 0.75rem;
    background: color-mix(in oklab, #c83333 14%, transparent);
    border-left: 2px solid #c83333;
    border-radius: 0 0.3rem 0.3rem 0;
    font-size: 0.83rem;
  }

  /* Mailpile-style bottom bar: PGP icons on the left, Send on the right. */
  .compose-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    padding: 1rem 0 0;
    margin-top: 0.25rem;
    border-top: 1px solid var(--border);
  }
  .pgp-toggles {
    display: inline-flex;
    gap: 0.35rem;
  }
  .pgp-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 36px;
    height: 36px;
    border-radius: 0.85rem;
    border: 1px solid color-mix(in oklab, currentColor 14%, transparent);
    background: transparent;
    color: inherit;
    opacity: 0.45;
    cursor: pointer;
    transition: opacity 120ms, background 120ms, border-color 120ms;
  }
  .pgp-icon:hover {
    opacity: 0.8;
    background: color-mix(in oklab, currentColor 6%, transparent);
  }
  .pgp-icon.active {
    opacity: 1;
    background: color-mix(in oklab, forestgreen 18%, transparent);
    border-color: color-mix(in oklab, forestgreen 50%, transparent);
    color: forestgreen;
  }
  .pgp-status {
    font-size: 0.65rem;
    font-weight: 700;
    padding: 0.1rem 0.35rem;
    border-radius: 0.2rem;
    line-height: 1;
    align-self: center;
  }
  .pgp-status.ok {
    color: forestgreen;
    background: color-mix(in oklab, forestgreen 12%, transparent);
  }
  .pgp-status.warn {
    color: color-mix(in oklab, orange 80%, currentColor);
    background: color-mix(in oklab, orange 12%, transparent);
  }
  .pgp-status.checking {
    color: var(--accent);
    background: color-mix(in oklab, var(--accent) 10%, transparent);
    letter-spacing: 0.08em;
  }
  .pgp-status.discovered {
    color: var(--accent);
    background: color-mix(in oklab, var(--accent) 14%, transparent);
    border: 1px solid color-mix(in oklab, var(--accent) 40%, transparent);
  }
  .bar-actions {
    display: flex;
    gap: 0.5rem;
    align-items: center;
  }
  .bar-actions button {
    font: inherit;
    font-size: 0.85rem;
    padding: 0.5rem 1.15rem;
    border-radius: 999px;
    cursor: pointer;
    border: 1px solid color-mix(in oklab, currentColor 18%, transparent);
    background: transparent;
    color: inherit;
  }
  .bar-actions button.primary {
    display: inline-flex;
    align-items: center;
    gap: 0.5rem;
    background: dodgerblue;
    border-color: dodgerblue;
    color: white;
    font-weight: 500;
  }
  .bar-actions button.primary:hover:not(:disabled) {
    background: color-mix(in oklab, dodgerblue 85%, black);
  }
  .bar-actions button:disabled { opacity: 0.55; cursor: progress; }
  .bar-actions button.ghost:hover {
    background: color-mix(in oklab, currentColor 6%, transparent);
  }

  .success {
    padding: 1rem 1.25rem;
    border: 1px solid color-mix(in oklab, forestgreen 40%, transparent);
    background: color-mix(in oklab, forestgreen 8%, transparent);
    border-radius: 1rem;
  }

  .pending {
    padding: 1rem 1.25rem;
    border: 1px solid color-mix(in oklab, var(--accent) 40%, transparent);
    background: color-mix(in oklab, var(--accent) 7%, var(--surface));
    border-radius: 1rem;
    display: flex;
    flex-direction: column;
    gap: 0.55rem;
  }
  .pending-head {
    display: flex;
    align-items: baseline;
    gap: 0.9rem;
    flex-wrap: wrap;
  }
  .pending-head strong {
    font-size: 1.05rem;
  }
  .pending-countdown {
    font-variant-numeric: tabular-nums;
    color: var(--muted);
  }
  .pending-countdown strong {
    color: var(--fg);
  }
  .pending-hint {
    margin: 0;
    font-size: 0.88rem;
    color: var(--muted);
  }
  .pending-hint a {
    color: inherit;
    text-decoration: underline;
  }
  .pending-actions {
    display: flex;
    gap: 0.5rem;
    flex-wrap: wrap;
  }

  /* Recipient / subject recap — shown in both the pending (undo
     window) and sent states so the user can verify who the message
     actually went to. Full recipient strings are rendered verbatim;
     truncating here would defeat the point. */
  .recap {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr);
    gap: 0.25rem 0.85rem;
    margin: 0.1rem 0 0;
    padding: 0.7rem 0.85rem;
    background: color-mix(in oklab, currentColor 4%, transparent);
    border: 1px solid color-mix(in oklab, currentColor 10%, transparent);
    border-radius: 0.55rem;
    font-size: 0.88rem;
  }
  .recap dt {
    color: var(--muted);
    font-weight: 500;
    font-size: 0.78rem;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    padding-top: 0.12rem;
  }
  .recap dd {
    margin: 0;
    min-width: 0;
    overflow-wrap: anywhere;
    word-break: break-word;
    color: var(--fg);
  }
  .recap-subject {
    font-weight: 600;
  }

  .schedule-group {
    position: relative;
    display: inline-flex;
    align-items: stretch;
  }
  .schedule-group .primary {
    border-top-right-radius: 0;
    border-bottom-right-radius: 0;
  }
  .schedule-toggle {
    padding: 0 0.55rem;
    border: 1px solid var(--border);
    border-left: 0;
    border-top-right-radius: 0.55rem;
    border-bottom-right-radius: 0.55rem;
    background: color-mix(in oklab, var(--accent) 18%, var(--surface));
    color: var(--fg);
    cursor: pointer;
    font-size: 0.8rem;
  }
  .schedule-toggle:hover {
    background: color-mix(in oklab, var(--accent) 28%, var(--surface));
  }
  .schedule-menu {
    position: absolute;
    right: 0;
    bottom: calc(100% + 0.4rem);
    min-width: 15rem;
    border: 1px solid var(--border);
    background: var(--surface);
    border-radius: 0.7rem;
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.2);
    display: flex;
    flex-direction: column;
    padding: 0.3rem;
    gap: 0.1rem;
    z-index: 50;
  }
  .schedule-menu button {
    text-align: left;
    background: transparent;
    border: 0;
    padding: 0.5rem 0.7rem;
    border-radius: 0.45rem;
    cursor: pointer;
    color: var(--fg);
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 0.6rem;
    font-size: 0.88rem;
  }
  .schedule-menu button:hover {
    background: color-mix(in oklab, var(--accent) 12%, transparent);
  }
  .schedule-menu button[aria-checked='true'] {
    background: color-mix(in oklab, var(--accent) 18%, transparent);
    font-weight: 600;
  }
  .schedule-menu .hint {
    font-size: 0.72rem;
    color: var(--muted);
    font-weight: 400;
  }
  .schedule-custom {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.45rem 0.7rem 0.2rem;
    border-top: 1px solid var(--border);
    margin-top: 0.15rem;
  }
  .schedule-custom {
    font-size: 0.82rem;
  }
  .schedule-custom span {
    font-size: 0.8rem;
    color: var(--muted);
  }
  .schedule-custom input {
    flex: 1;
    font-size: 0.82rem;
  }
  .success code {
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 0.78rem;
    background: color-mix(in oklab, currentColor 5%, transparent);
    padding: 0 0.35em;
    border-radius: 0.2em;
  }
  .forensics {
    margin: 0.75rem 0 0.5rem;
    padding: 0.6rem 0.85rem;
    background: color-mix(in oklab, var(--surface-2) 70%, transparent);
    border: 1px solid var(--border);
    border-radius: 0.75rem;
    font-size: 0.84rem;
  }
  .forensics summary {
    cursor: pointer;
    font-weight: 600;
    padding: 0.15rem 0;
    user-select: none;
  }
  .forensics dl {
    margin: 0.6rem 0 0;
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 0.35rem 0.9rem;
    font-variant-numeric: tabular-nums;
  }
  .forensics dt {
    color: var(--muted);
    font-weight: 500;
    white-space: nowrap;
  }
  .forensics dd {
    margin: 0;
    word-break: break-word;
  }
  .forensics dd em {
    opacity: 0.7;
    font-style: italic;
  }

  @media (max-width: 900px) {
    article.compose-shell {
      padding: 0.65rem;
    }
    .page-top {
      margin-bottom: 0.4rem;
    }
    .back {
      font-size: 0.74rem;
    }
    .hero {
      gap: 0.75rem;
      padding: 0.95rem 1rem;
      margin-bottom: 0.8rem;
      border-radius: 1rem;
      grid-template-columns: 1fr;
    }
    .hero.compact-hero {
      padding: 0.65rem 0.8rem;
      margin-bottom: 0.6rem;
      gap: 0.35rem;
      border-radius: 0.9rem;
    }
    .eyebrow {
      margin-bottom: 0.35rem;
      font-size: 0.62rem;
    }
    .hero.compact-hero .eyebrow {
      margin-bottom: 0.2rem;
      font-size: 0.58rem;
    }
    .hero h1 {
      font-size: 1.3rem;
      margin-bottom: 0.25rem;
    }
    .hero.compact-hero h1 {
      font-size: 1rem;
      margin-bottom: 0;
    }
    .hero p {
      font-size: 0.82rem;
      line-height: 1.45;
    }
    .hero.compact-hero p {
      display: none;
    }
    .hero-side {
      justify-content: flex-start;
      gap: 0.3rem;
    }
    .hero.compact-hero .hero-side {
      display: none;
    }
    .hero-chip {
      padding: 0.28rem 0.5rem;
      font-size: 0.66rem;
    }
    form.compose {
      padding: 0.72rem 0.72rem 0.9rem;
      border-radius: 1rem;
      gap: 0.5rem;
    }
    .row {
      grid-template-columns: 1fr;
      gap: 0.38rem;
      padding: 0.62rem 0.68rem;
      border-radius: 0.85rem;
    }
    .row label {
      padding-top: 0;
      font-size: 0.69rem;
    }
    .row input,
    .row select,
    .row textarea {
      font-size: 0.82rem;
      padding: 0.56rem 0.68rem;
      border-radius: 0.75rem;
    }
    .body-row textarea {
      min-height: 46vh;
    }
    .linklike,
    .att,
    .add-att,
    .att-size,
    .err {
      font-size: 0.74rem;
    }
    .compose-bar {
      flex-direction: column;
      align-items: stretch;
      gap: 0.6rem;
      padding-top: 0.7rem;
    }
    .pgp-toggles {
      justify-content: flex-start;
    }
    .pgp-icon {
      width: 34px;
      height: 34px;
      border-radius: 0.75rem;
    }
    .bar-actions {
      justify-content: stretch;
    }
    .bar-actions button {
      flex: 1 1 0;
      justify-content: center;
      min-height: 38px;
      padding: 0.44rem 0.8rem;
      font-size: 0.78rem;
    }
  }
</style>
