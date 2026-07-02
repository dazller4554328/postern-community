<script lang="ts">
  import './compose.css';
  import { onMount, onDestroy } from 'svelte';
  import { get } from 'svelte/store';
  import { goto } from '$app/navigation';
  import { page } from '$app/stores';
  import {
    api,
    type Account,
    type SendAttachment,
    type SendForensics,
    type OutboxEntry
  } from '$lib/api';
  import { prefs } from '$lib/prefs';
  import AttachmentList from './_components/AttachmentList.svelte';
  import BodyEditor from './_components/BodyEditor.svelte';
  import ComposeHero from './_components/ComposeHero.svelte';
  import PendingSendCard from './_components/PendingSendCard.svelte';
  import PgpToggleRow from './_components/PgpToggleRow.svelte';
  import RecipientAutocomplete from './_components/RecipientAutocomplete.svelte';
  import ScheduleSendButton from './_components/ScheduleSendButton.svelte';
  import SendSuccessPanel from './_components/SendSuccessPanel.svelte';
  import { splitAddrs } from './_lib/addresses';
  import { computePrefill, insertSignature, type PrefillMode } from './_lib/prefill';
  import { resolveScheduledAt as resolveScheduledAtLib, type SendChoice } from './_lib/scheduling';

  let accounts = $state<Account[]>([]);
  let loadingAccounts = $state(true);

  let fromAccountId = $state<number | null>(null);
  let to = $state('');
  let cc = $state('');
  let bcc = $state('');
  let showCcBcc = $state(false);
  let subject = $state('');
  let body = $state('');

  // Body textarea + selection state + dictation/rewrite helpers live
  // in _components/BodyEditor.svelte. The parent just binds `body`.
  let attachments = $state<SendAttachment[]>([]);
  let pgpEncrypt = $state(false);
  let pgpSign = $state(false);
  let attachKey = $state(false);
  let pgpAutoDetected = $state(false);
  let pgpMissing = $state<string[]>([]);
  let pgpJustDiscovered = $state<string[]>([]);
  let pgpChecking = $state(false);

  // Auto-detect PGP: when To/Cc/Bcc change, check whether all recipients
  // already have public keys in the LOCAL keyring (discover=false — no
  // network). If yes, auto-enable encryption. We deliberately do NOT
  // reach out to WKD/keyservers on keystroke: that would disclose the
  // address being typed (or mistyped, or later deleted) to a third
  // party before the user has committed to encrypting. Discovery runs
  // only on explicit intent — the user turns encryption on, or sends —
  // via `runPgpCheck(true)` (see onEnableEncrypt + submit).
  let pgpCheckTimer: ReturnType<typeof setTimeout> | null = null;
  async function runPgpCheck(discover: boolean) {
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
      const result = await api.pgpCanEncrypt(allAddrs, discover);
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
  }
  function checkPgpCapability() {
    if (pgpCheckTimer) clearTimeout(pgpCheckTimer);
    pgpCheckTimer = setTimeout(() => runPgpCheck(false), 500);
  }
  // User explicitly enabled encryption — now we're allowed to hit
  // WKD/keyservers to pull any missing recipient keys.
  async function onEnableEncrypt() {
    if (pgpCheckTimer) clearTimeout(pgpCheckTimer);
    await runPgpCheck(true);
  }
  let inReplyTo = $state<string | null>(null);
  let references = $state<string | null>(null);
  let requestReceipt = $state(false);

  // Auto-complete state. The dropdown is rendered at the top of <body>
  // with `position: fixed`, escaping the form's stacking context entirely
  // — three rounds of z-index tweaks couldn't reliably keep it above the
  // Add-Cc/Bcc row, so we sidestep stacking math by painting in the
  // viewport's top layer and computing the rect from the focused input.
  let suggestions = $state<string[]>([]);
  let acFocused = $state<'to' | 'cc' | 'bcc' | null>(null);
  let acTimer: ReturnType<typeof setTimeout> | null = null;
  let toEl = $state<HTMLInputElement | null>(null);
  let ccEl = $state<HTMLInputElement | null>(null);
  let bccEl = $state<HTMLInputElement | null>(null);
  let acRect = $state<{ top: number; left: number; width: number } | null>(null);

  function recomputeAcRect() {
    const el =
      acFocused === 'to' ? toEl : acFocused === 'cc' ? ccEl : acFocused === 'bcc' ? bccEl : null;
    if (!el) {
      acRect = null;
      return;
    }
    const r = el.getBoundingClientRect();
    acRect = { top: r.bottom + 2, left: r.left, width: r.width };
  }

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
        recomputeAcRect();
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

  let sendChoice = $state<SendChoice>('now');
  let customScheduledAt = $state<string>(''); // datetime-local input

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

  // Prefill + signature logic lives in ./_lib/prefill.ts. The async
  // helpers return a partial draft; we apply the fields here so the
  // reactive state lights up at one assignment site.
  async function prefillFromMessage(id: number, mode: PrefillMode) {
    const r = await computePrefill(id, mode, accounts);
    fromAccountId = r.fromAccountId;
    if (r.to !== undefined) to = r.to;
    if (r.cc !== undefined) cc = r.cc;
    if (r.showCcBcc) showCcBcc = true;
    subject = r.subject;
    inReplyTo = r.inReplyTo;
    references = r.references;
    body = r.body;
  }

  function maybeInsertSignature() {
    if (fromAccountId === null) return;
    const acct = accounts.find((a) => a.id === fromAccountId);
    body = insertSignature(
      body,
      acct?.signature_plain,
      composeMode as 'new' | PrefillMode,
      get(prefs).signatureOnReplies
    );
  }

  // Address + scheduling helpers live in ./_lib/.

  function resolveScheduledAt(): number | null {
    return resolveScheduledAtLib(sendChoice, get(prefs).sendUndoSecs, customScheduledAt);
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
    // Explicit send with encryption on: this is the moment we're allowed
    // to discover any still-missing recipient keys via WKD/keyserver
    // (never on keystroke). No-op when every key is already local.
    if (pgpEncrypt) {
      await runPgpCheck(true);
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

</script>

<article class="compose-shell" class:reply-mode={composeMode === 'reply' || composeMode === 'reply_all'} class:forward-mode={composeMode === 'forward'}>
  <div class="page-top">
    <a class="back" href="/inbox">← Inbox</a>
  </div>

  <ComposeHero composeMode={composeMode as 'new' | 'reply' | 'reply_all' | 'forward'} />

  {#if pending}
    <PendingSendCard {pending} {sendChoice} {undoBusy} onUndo={undoSend} />
  {:else if sent}
    <SendSuccessPanel {sent} />
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
        <input
          id="to"
          type="text"
          bind:this={toEl}
          bind:value={to}
          placeholder="alice@example.com, bob@example.com"
          autocomplete="off"
          oninput={() => { onAddrInput('to'); checkPgpCapability(); }}
          onfocus={() => { acFocused = 'to'; recomputeAcRect(); }}
          onblur={() => setTimeout(() => { if (acFocused === 'to') suggestions = []; checkPgpCapability(); }, 200)}
        />
      </div>

      {#if !showCcBcc}
        <div class="row">
          <span></span>
          <button type="button" class="linklike" onclick={() => (showCcBcc = true)}>Add Cc / Bcc</button>
        </div>
      {:else}
        <div class="row">
          <label for="cc">Cc</label>
          <input
            id="cc"
            type="text"
            bind:this={ccEl}
            bind:value={cc}
            autocomplete="off"
            oninput={() => { onAddrInput('cc'); checkPgpCapability(); }}
            onfocus={() => { acFocused = 'cc'; recomputeAcRect(); }}
            onblur={() => setTimeout(() => { if (acFocused === 'cc') suggestions = []; checkPgpCapability(); }, 200)}
          />
        </div>
        <div class="row">
          <label for="bcc">Bcc</label>
          <input
            id="bcc"
            type="text"
            bind:this={bccEl}
            bind:value={bcc}
            autocomplete="off"
            oninput={() => { onAddrInput('bcc'); checkPgpCapability(); }}
            onfocus={() => { acFocused = 'bcc'; recomputeAcRect(); }}
            onblur={() => setTimeout(() => { if (acFocused === 'bcc') suggestions = []; checkPgpCapability(); }, 200)}
          />
        </div>
      {/if}

      <div class="row">
        <label for="subject">Subject</label>
        <input id="subject" type="text" bind:value={subject} autocomplete="off" />
      </div>

      <BodyEditor bind:body />

      <div class="row">
        <label for="attachments-input">Attachments</label>
        <AttachmentList bind:attachments />
      </div>

      {#if err}
        <div class="err">⚠ {err}</div>
      {/if}

      <div class="compose-bar">
        <PgpToggleRow
          bind:pgpEncrypt
          bind:pgpSign
          bind:attachKey
          bind:requestReceipt
          {pgpChecking}
          {pgpAutoDetected}
          {pgpJustDiscovered}
          {pgpMissing}
          {onEnableEncrypt}
        />

        <div class="bar-actions">
          <button type="button" class="ghost" onclick={() => history.back()}>Cancel</button>
          <ScheduleSendButton bind:sendChoice bind:customScheduledAt {sending} />
        </div>
      </div>
    </form>
  {/if}
</article>

<RecipientAutocomplete
  suggestions={acFocused !== null ? suggestions : []}
  rect={acRect}
  onPick={pickSuggestion}
/>

<svelte:window onscroll={recomputeAcRect} onresize={recomputeAcRect} />

