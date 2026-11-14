<script lang="ts">
  import { api, type MessageDetail } from '$lib/api';

  type ViewMode = 'html' | 'plain' | 'source' | 'forensics';

  interface Props {
    message: MessageDetail;
    archiveEnabled: boolean;
    mode: ViewMode;
    isMobile: boolean;
    mobileToolsOpen: boolean;
    /// 'preview' renders the windowed split-pane view, which gets an
    /// "Open" action to pop out to the full-page route. The full view
    /// is already full-page, so it omits the button.
    variant?: 'preview' | 'full';
    onRefresh: () => void | Promise<void>;
    onPickerOpen: () => void;
    onToggleRuleForm: () => void;
    onSwitchMode: (m: ViewMode) => void;
  }
  let {
    message,
    archiveEnabled,
    mode,
    isMobile,
    mobileToolsOpen,
    variant = 'full',
    onRefresh,
    onPickerOpen,
    onToggleRuleForm,
    onSwitchMode,
  }: Props = $props();

  /// Reply-all visibility: show when Cc'd recipients exist OR more
  /// than one To recipient.
  function splitToAddrs(raw: string): string[] {
    return raw.split(/[,;]/).map((s) => s.trim()).filter(Boolean);
  }

  let showReplyAll = $derived(
    !!(message.cc_addrs || (message.to_addrs && splitToAddrs(message.to_addrs).length > 1))
  );
  let isSpam = $derived(
    message.labels.some((l) => l.includes('Spam') || l === 'Junk')
  );
</script>

<div class="toolbar-row" class:mobile-drawer={isMobile} class:open={!isMobile || mobileToolsOpen}>
  <div class="actions-toolbar">
    {#if variant === 'preview' && !isMobile}
      <a class="action open-action" href="/message/{message.id}" title="Open this message in its own full page">
        <svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
          <path d="M9.5 2.5H13.5V6.5"/>
          <path d="M13.5 2.5 8.5 7.5"/>
          <path d="M11 9v3.5a1 1 0 0 1-1 1H3.5a1 1 0 0 1-1-1V6a1 1 0 0 1 1-1H7"/>
        </svg>
        Open
      </a>
      <span class="action-sep" aria-hidden="true"></span>
    {/if}
    <a class="action" href="/compose?reply={message.id}" title="Reply">
      <svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
        <path d="M7 3 2.5 7.5 7 12"/>
        <path d="M2.5 7.5h7a4 4 0 0 1 4 4v1"/>
      </svg>
      Reply
    </a>
    {#if showReplyAll}
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
    {#if isSpam}
      <button class="action" onclick={async () => { await api.markNotSpam(message.id); onRefresh(); }} title="Not spam — move to Inbox">
        <svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
          <path d="M8 1.5 14.5 13H1.5L8 1.5Z"/>
          <path d="M8 6.5v3"/>
        </svg>
        Not spam
      </button>
    {:else}
      <button class="action" onclick={async () => { await api.markSpam(message.id); onRefresh(); }} title="Mark as spam">
        <svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
          <path d="M8 1.5 14.5 13H1.5L8 1.5Z"/>
          <path d="M8 6.5v3"/><path d="M8 11.2v.1"/>
        </svg>
        Spam
      </button>
    {/if}
    {#if archiveEnabled}
      <button class="action" onclick={async () => { await api.archiveMessage(message.id); onRefresh(); }} title="Archive — move to the account's configured archive folder">
        <svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
          <rect x="1.75" y="3" width="12.5" height="3" rx="0.6"/>
          <path d="M3 6v6.5a1 1 0 0 0 1 1h8a1 1 0 0 0 1-1V6"/>
          <path d="M6.5 9h3"/>
        </svg>
        Archive
      </button>
    {/if}
    <button class="action" onclick={onPickerOpen} title="Move to a specific folder…">
      <svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
        <path d="M2.5 5.5h11v7.5a.5.5 0 0 1-.5.5H3a.5.5 0 0 1-.5-.5z"/>
        <path d="M2.5 5.5V4a.5.5 0 0 1 .5-.5h4l1.5 2h5a.5.5 0 0 1 .5.5v1"/>
      </svg>
      Move…
    </button>
    <button class="action" onclick={async () => { await api.markTrash(message.id); onRefresh(); }} title="Move to Trash">
      <svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
        <path d="M2.5 4.5h11"/>
        <path d="M6 4.5V3a1 1 0 0 1 1-1h2a1 1 0 0 1 1 1v1.5"/>
        <path d="M4 4.5v8.5a1 1 0 0 0 1 1h6a1 1 0 0 0 1-1V4.5"/>
      </svg>
      Trash
    </button>
    <span class="action-sep" aria-hidden="true"></span>
    <div class="rule-dropdown">
      <button class="action" onclick={onToggleRuleForm} title="Create a rule from this message">
        <svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
          <path d="M2 4h12M2 8h8M2 12h6"/>
          <path d="M13 9l2 2-2 2"/>
        </svg>
        Rule
      </button>
    </div>
  </div>
  <div class="view-toolbar" role="tablist" aria-label="View mode">
    <button role="tab" class:active={mode === 'html'} onclick={() => onSwitchMode('html')} title="Rendered HTML in a sandbox">HTML</button>
    <button role="tab" class:active={mode === 'plain'} onclick={() => onSwitchMode('plain')} title="Plain text only — Mailpile-style secure default">Plain</button>
    <button role="tab" class:active={mode === 'source'} onclick={() => onSwitchMode('source')} title="Raw RFC822 source">Source</button>
    <button role="tab" class:active={mode === 'forensics'} onclick={() => onSwitchMode('forensics')} title="Headers + authentication + MIME tree">Forensics</button>
  </div>
</div>

<style>
  .toolbar-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    margin-bottom: 1rem;
    flex-wrap: wrap;
  }
  .actions-toolbar {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    flex-wrap: wrap;
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
  .open-action {
    background: color-mix(in oklab, var(--accent, #2a6df4) 14%, transparent);
    border-color: color-mix(in oklab, var(--accent, #2a6df4) 35%, transparent);
  }
  .open-action:hover {
    background: color-mix(in oklab, var(--accent, #2a6df4) 24%, transparent);
  }
  .action-sep {
    width: 1px;
    height: 16px;
    background: color-mix(in oklab, currentColor 12%, transparent);
    margin: 0 0.15rem;
  }
  .rule-dropdown { position: relative; }

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

  @media (max-width: 900px) {
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
  }
</style>
