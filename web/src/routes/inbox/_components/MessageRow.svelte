<script lang="ts">
  import { formatDate, formatRelative, formatSender } from '$lib/format';
  import SenderAvatar from '$lib/components/SenderAvatar.svelte';
  import type { MessageListItem } from '$lib/api';
  import type { RowStyle } from '$lib/prefs';

  /** Search hits append a `match_snippet` highlight to the standard
   *  list item — the parent's `DisplayItem` alias. Keeping it local
   *  avoids leaking the parent type. */
  export type MessageRowItem = MessageListItem & { match_snippet?: string };

  interface Props {
    message: MessageRowItem;
    selected: boolean;
    rowStyle: RowStyle;
    accountColor: string;
    checked: boolean;
    onSelect: () => void;
    onToggleCheck: (e: MouseEvent | KeyboardEvent) => void;
    onFilterBySender: () => void;
    onColResize: (e: PointerEvent) => void;
    onMouseEnter: (e: MouseEvent) => void;
    onMouseMove: (e: MouseEvent) => void;
    onMouseLeave: () => void;
  }
  let {
    message: m,
    selected,
    rowStyle,
    accountColor,
    checked,
    onSelect,
    onToggleCheck,
    onFilterBySender,
    onColResize,
    onMouseEnter,
    onMouseMove,
    onMouseLeave,
  }: Props = $props();
</script>

<li class:checked>
  <button
    class="row"
    class:unread={!m.is_read}
    class:selected
    onclick={onSelect}
    onmouseenter={onMouseEnter}
    onmousemove={onMouseMove}
    onmouseleave={onMouseLeave}
  >
    <span
      class="envelope"
      class:unread={!m.is_read}
      class:encrypted={m.is_encrypted}
      style:--pill-color={accountColor}
      title={`${m.is_read ? 'Read' : 'Unread'}${m.is_encrypted ? ' · PGP encrypted' : ''}`}
      aria-label={`${m.is_read ? 'Read' : 'Unread'}${m.is_encrypted ? ' PGP encrypted' : ''}`}
    >
      {#if m.is_encrypted}
        {#if m.is_read}
          <!-- Open padlock — read PGP mail. Inherits the muted .envelope
               colour so it greys out alongside non-PGP rows. -->
          <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round" stroke-linecap="round" aria-hidden="true">
            <rect x="5" y="11" width="14" height="9" rx="1.5"/>
            <path d="M8 11V7.5a4 4 0 0 1 7.5-2"/>
          </svg>
        {:else}
          <!-- Closed padlock — unread PGP mail. Inherits the mailbox
               accent colour via --pill-color on .envelope.unread. -->
          <svg viewBox="0 0 24 24" width="18" height="18" fill="currentColor" aria-hidden="true">
            <rect x="5" y="11" width="14" height="9" rx="1.5"/>
            <path d="M8 11V7.5a4 4 0 0 1 8 0V11h-1.6V7.5a2.4 2.4 0 0 0-4.8 0V11Z"/>
          </svg>
        {/if}
      {:else if m.is_read}
        <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round" stroke-linecap="round" aria-hidden="true">
          <path d="M3 9.5 12 15l9-5.5"/>
          <path d="M3 9.5v10h18v-10"/>
          <path d="M3 9.5 12 4l9 5.5"/>
        </svg>
      {:else}
        <svg viewBox="0 0 24 24" width="18" height="18" fill="currentColor" aria-hidden="true">
          <path d="M2.5 6.5A1.5 1.5 0 0 1 4 5h16a1.5 1.5 0 0 1 1.5 1.5v11A1.5 1.5 0 0 1 20 19H4a1.5 1.5 0 0 1-1.5-1.5v-11Zm1.6.2 7.9 5.5 7.9-5.5-.5-.2H4.5l-.4.2Z"/>
        </svg>
      {/if}
    </span>
    <SenderAvatar email={m.from_addr} size={26} />
    <span class="star" class:starred={m.is_starred}>{m.is_starred ? '★' : '☆'}</span>
    <span class="from" title={m.from_addr ?? ''}>
      <span
        role="button"
        tabindex="-1"
        class="from-click"
        title={m.from_addr ? `Show all from ${m.from_addr}` : ''}
        onclick={(e) => {
          e.stopPropagation();
          onFilterBySender();
        }}
        onkeydown={(e) => {
          if (e.key === 'Enter' || e.key === ' ') {
            e.preventDefault();
            e.stopPropagation();
            onFilterBySender();
          }
        }}
      >{formatSender(m.from_addr)}</span>
    </span>
    <span
      class="col-resizer"
      role="separator"
      aria-orientation="vertical"
      aria-label="Resize sender column"
      onpointerdown={(e) => {
        e.stopPropagation();
        onColResize(e);
      }}
    ></span>
    <span class="subject">
      <span class="subject-text">{m.subject || '(no subject)'}</span>
      {#if rowStyle === 'detailed'}
        {#if m.match_snippet}
          <span class="snippet">— {m.match_snippet?.replace(/<\/?mark>/g, '') ?? ''}</span>
        {:else if m.snippet}
          <span class="snippet">— {m.snippet}</span>
        {/if}
      {/if}
    </span>
    <span class="meta-col">
      {#if m.has_attachments}<span class="attach" title="has attachments">📎</span>{/if}
      {#if rowStyle === 'detailed'}
        <span class="acct">{m.account_email.split('@')[0]}</span>
        <time>{formatDate(m.date_utc)}</time>
      {:else}
        <time class="relative" title={new Date(m.date_utc * 1000).toLocaleString()}>
          {formatRelative(m.date_utc)}
        </time>
      {/if}
    </span>
  </button>
  <span class="row-check">
    <input
      type="checkbox"
      aria-label="Select message"
      {checked}
      onclick={(e) => {
        e.stopPropagation();
        onToggleCheck(e);
      }}
    />
  </span>
</li>
