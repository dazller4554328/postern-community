<script lang="ts">
  import { onMount } from 'svelte';
  import { api, type Account, type Forensics, type MessageDetail } from '$lib/api';
  import FolderPicker from './FolderPicker.svelte';
  import MessageForensics from './MessageForensics.svelte';
  import ReceiptBanner from './ReceiptBanner.svelte';
  import RuleQuickCreate from './RuleQuickCreate.svelte';
  import AttachmentStrip from './message/AttachmentStrip.svelte';
  import MessageActionsToolbar from './message/MessageActionsToolbar.svelte';
  import MessageBodyBanners from './message/MessageBodyBanners.svelte';
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
  // Monotonic load token. Rapid j/k navigation fires overlapping load()s;
  // a slow response for an earlier message must not overwrite the state of
  // the one now selected. Each load claims a token and bails after every
  // await if a newer load has superseded it.
  let loadSeq = 0;

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
    const seq = ++loadSeq;
    const id = messageId;
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
      const loaded = await api.getMessage(id);
      if (seq !== loadSeq) return;
      message = loaded;
      const res = await fetch(`/api/messages/${id}/body`);
      if (seq !== loadSeq) return;
      if (res.ok) {
        const info = (await res.json()) as BodyInfo & { html: string };
        if (seq !== loadSeq) return;
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
      api
        .getForensics(id)
        .then((f) => {
          if (seq === loadSeq) forensics = f;
        })
        .catch(() => {});
    } catch (e) {
      if (seq !== loadSeq) return;
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

  // Tracker-summary formatter lives in message/MessageBodyBanners.svelte.

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
      bodyObserver?.disconnect();
    };
  });

  // Keeps the iframe sized to its document so it grows to fit the whole
  // email instead of trapping the body in a fixed-height box. Re-measures
  // as proxied images load and reflow. Only works in the full view, whose
  // iframe carries `allow-same-origin` so contentDocument is reachable; the
  // sandboxed preview throws on access (opaque origin) and keeps its own
  // internal scroll.
  let bodyObserver: ResizeObserver | null = null;
  function onIframeLoad() {
    if (!iframe) return;
    bodyObserver?.disconnect();
    bodyObserver = null;
    try {
      const doc = iframe.contentDocument;
      if (!doc) return;
      const measure = () => {
        iframeHeight = Math.max(240, doc.documentElement.scrollHeight + 16);
      };
      measure();
      if (typeof ResizeObserver !== 'undefined') {
        bodyObserver = new ResizeObserver(measure);
        bodyObserver.observe(doc.documentElement);
      }
    } catch {
      // Sandboxed preview iframe — opaque origin blocks contentDocument.
      // Leave iframeHeight at its default and let the iframe scroll itself.
    }
  }

  // ── Inline rule creation ──────────────────────────────────────────
  let showRuleForm = $state(false);

  // Sandbox probe — only the parent decides whether to probe based on
  // attachment types. Convertible-type detection is duplicated in
  // AttachmentStrip; keeping it small here avoids a shared util just
  // for the probe gate.
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
    'text/csv',
  ]);
  let sandboxAvailable = $state<boolean | null>(null);
  $effect(() => {
    if (sandboxAvailable !== null) return;
    const any = forensics?.attachments?.some((a) =>
      CONVERT_TYPES.has(a.content_type.toLowerCase().trim())
    );
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
      <ReceiptBanner messageId={message.id} receiptTo={message.receipt_to} />
    {/if}

    {#if isMobile}
      <div class="mobile-tools-toggle">
        <button type="button" class="mobile-tools-btn" onclick={() => (mobileToolsOpen = !mobileToolsOpen)}>
          <span>{mobileToolsOpen ? 'Hide actions' : 'Actions & view'}</span>
          <span aria-hidden="true">{mobileToolsOpen ? '−' : '+'}</span>
        </button>
      </div>
    {/if}

    <MessageActionsToolbar
      {message}
      {archiveEnabled}
      {mode}
      {isMobile}
      {mobileToolsOpen}
      {variant}
      onRefresh={load}
      onPickerOpen={() => (pickerOpen = true)}
      onToggleRuleForm={() => (showRuleForm = !showRuleForm)}
      onSwitchMode={switchMode}
    />

    {#if showRuleForm && message}
      <RuleQuickCreate {message} onClose={() => (showRuleForm = false)} />
    {/if}

    {#if forensics?.attachments?.length}
      <AttachmentStrip
        {messageId}
        attachments={forensics.attachments}
        {sandboxAvailable}
      />
    {/if}

    {#if mode === 'html'}
      <MessageBodyBanners
        trackers={bodyInfo?.trackers_blocked ?? []}
        hasRemoteContent={bodyInfo?.has_remote_content ?? false}
        remoteHosts={bodyInfo?.remote_hosts ?? []}
        bind:allowRemote
      />
      <div class="body-wrap">
        <!--
          Full view gets `allow-same-origin` ONLY — never `allow-scripts` —
          so the parent can measure content height and let the page scroll
          the whole email. With no scripts permitted, same-origin grants the
          email no runnable code, and the strict CSP on the body document
          still blocks remote content; this is not a leak vector. The preview
          stays fully locked (`sandbox=""`) and scrolls internally.
        -->
        <iframe
          bind:this={iframe}
          title="Message body"
          {src}
          sandbox={variant === 'full' ? 'allow-same-origin' : ''}
          loading="eager"
          onload={onIframeLoad}
          style={variant === 'full' ? `height: ${iframeHeight}px` : 'height: 100%'}
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
        <MessageForensics {forensics} />
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
  /* No width cap on the full view — the dedicated /message/[id] page (and
     the mobile full view) should use the whole width rather than render as
     a narrow centered column. The split-pane preview keeps its own sizing. */
  section.full { max-width: none; padding: 2rem; }

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

  /* .toolbar-row / .actions-toolbar / .action* / .view-toolbar live
     in message/MessageActionsToolbar.svelte. */
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
  /* .banner styles live in message/MessageBodyBanners.svelte. */

  /* ── Attachment strip — above the body so it's always visible ── */
  /* .att-* styles live in message/AttachmentStrip.svelte. */
  .body-wrap {
    flex: 1;
    border: 1px solid var(--border);
    border-radius: 1rem;
    background: var(--surface);
    overflow: hidden;
    min-height: 300px;
    box-shadow: 0 12px 28px rgba(0, 0, 0, 0.05);
  }
  /* Full view (dedicated /message page + mobile): the iframe is measured to
     its content height, so the wrapper hugs it and the whole page scrolls —
     no fixed-height box, header + actions scroll away for max reading space.
     The split-pane preview keeps `flex: 1` above and scrolls internally. */
  section.full .body-wrap {
    flex: 0 0 auto;
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

  }

</style>
