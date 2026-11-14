<script lang="ts">
  import type { MessageListItem } from '$lib/api';
  import { formatSender, formatRelative } from '$lib/format';

  type DisplayItem = MessageListItem & { match_snippet?: string };

  export interface HoverState {
    item: DisplayItem;
    rect: DOMRect;
    cursorX: number;
  }

  const CARD_W = 360;
  const CARD_H = 200;
  const MARGIN = 12;

  let { hovered }: { hovered: HoverState } = $props();

  let r = $derived(hovered.rect);
  let m = $derived(hovered.item);
  let rawLeft = $derived(hovered.cursorX - CARD_W / 2);
  let clampedLeft = $derived(
    Math.max(MARGIN, Math.min(window.innerWidth - CARD_W - MARGIN, rawLeft))
  );
  let belowRoom = $derived(window.innerHeight - r.bottom);
  let aboveRoom = $derived(r.top);
  let placeAbove = $derived(belowRoom < CARD_H + MARGIN && aboveRoom > belowRoom);
  let cardTop = $derived(placeAbove ? Math.max(MARGIN, r.top - CARD_H - 8) : r.bottom + 8);
</script>

<div
  class="hover-card"
  style="left: {clampedLeft}px; top: {cardTop}px;"
  role="tooltip"
>
  <div class="hc-head">
    <span class="hc-from">{formatSender(m.from_addr)}</span>
    <span class="hc-date" title={new Date(m.date_utc * 1000).toLocaleString()}>
      {formatRelative(m.date_utc)}
    </span>
  </div>
  <div class="hc-subject">{m.subject || '(no subject)'}</div>
  {#if m.snippet}
    <div class="hc-snippet">{m.snippet}</div>
  {:else}
    <div class="hc-snippet empty">No preview text — open the message to view.</div>
  {/if}
  <div class="hc-footer">
    {#if m.has_attachments}<span class="hc-attach">📎 attachment</span>{/if}
    <span class="hc-acct">{m.account_email}</span>
  </div>
</div>
