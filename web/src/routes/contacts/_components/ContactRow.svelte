<script lang="ts">
  import type { Contact } from '$lib/api';
  import { formatDate } from '$lib/format';
  import SenderAvatar from '$lib/components/SenderAvatar.svelte';

  interface Props {
    contact: Contact;
    photoVersion: number | undefined;
    onToggleFavorite: () => void;
    onUploadPhoto: (file: File) => void;
    onClearPhoto: () => void;
    onCompose: () => void;
    onEdit: () => void;
    onDelete: () => void;
  }
  let {
    contact: c,
    photoVersion,
    onToggleFavorite,
    onUploadPhoto,
    onClearPhoto,
    onCompose,
    onEdit,
    onDelete,
  }: Props = $props();
</script>

<li class="contact-row">
  <button
    type="button"
    class="fav-btn"
    class:on={c.is_favorite}
    onclick={onToggleFavorite}
    aria-label={c.is_favorite ? 'Remove favourite' : 'Mark favourite'}
    title={c.is_favorite ? 'Remove favourite' : 'Mark favourite'}
  >
    {#if c.is_favorite}
      <svg viewBox="0 0 20 20" width="18" height="18" fill="currentColor">
        <path d="M10 2.5l2.4 4.86 5.36.78-3.88 3.78.92 5.34L10 14.74 5.2 17.26l.92-5.34L2.24 8.14l5.36-.78z" />
      </svg>
    {:else}
      <svg viewBox="0 0 20 20" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round">
        <path d="M10 2.5l2.4 4.86 5.36.78-3.88 3.78.92 5.34L10 14.74 5.2 17.26l.92-5.34L2.24 8.14l5.36-.78z" />
      </svg>
    {/if}
  </button>

  <SenderAvatar email={c.address} size={36} version={photoVersion} />

  <div class="contact-body">
    <div class="contact-line">
      <strong>{c.display_name || c.address}</strong>
      {#if c.display_name}
        <span class="muted-addr">{c.address}</span>
      {/if}
    </div>
    <div class="contact-meta">
      {#if c.message_count > 0}
        {c.message_count.toLocaleString()}
        {c.message_count === 1 ? 'message' : 'messages'}
        · last on {formatDate(c.last_seen_utc)}
      {:else}
        Manual entry — no messages yet
      {/if}
      {#if c.notes}<span class="has-notes" title={c.notes}>· note</span>{/if}
    </div>
  </div>

  <div class="row-actions">
    <label class="btn ghost small" title="Upload a photo for this contact (image/*, ≤2 MB)">
      Photo
      <input
        type="file"
        accept="image/*"
        hidden
        onchange={(e) => {
          const target = e.currentTarget as HTMLInputElement;
          const f = target.files?.[0];
          if (f) {
            onUploadPhoto(f);
            target.value = '';
          }
        }}
      />
    </label>
    <button class="btn ghost small" type="button" onclick={onCompose}>Email</button>
    <button class="btn ghost small" type="button" onclick={onEdit}>Edit</button>
    <button class="btn ghost small danger" type="button" onclick={onClearPhoto}>Clear photo</button>
    <button class="btn ghost small danger" type="button" onclick={onDelete}>Delete</button>
  </div>
</li>

<style>
  .contact-row {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    padding: 0.7rem 0.9rem;
    border-bottom: 1px solid var(--border);
  }
  .contact-row:last-child {
    border-bottom: 0;
  }
  .contact-row:hover {
    background: color-mix(in oklab, currentColor 4%, transparent);
  }
  .fav-btn {
    background: transparent;
    border: 0;
    cursor: pointer;
    color: color-mix(in oklab, currentColor 35%, transparent);
    padding: 0.25rem;
    border-radius: 999px;
    display: inline-grid;
    place-items: center;
  }
  .fav-btn.on {
    color: gold;
  }
  .fav-btn:hover {
    background: color-mix(in oklab, currentColor 6%, transparent);
  }
  .contact-body {
    flex: 1 1 auto;
    min-width: 0;
  }
  .contact-line {
    display: flex;
    align-items: baseline;
    gap: 0.6rem;
    flex-wrap: wrap;
    line-height: 1.3;
  }
  .contact-line strong {
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .muted-addr {
    color: color-mix(in oklab, currentColor 55%, transparent);
    font-size: 0.84rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .contact-meta {
    margin-top: 0.18rem;
    color: color-mix(in oklab, currentColor 55%, transparent);
    font-size: 0.78rem;
  }
  .has-notes {
    color: color-mix(in oklab, var(--accent) 75%, currentColor);
    margin-left: 0.25rem;
  }
  .row-actions {
    display: inline-flex;
    gap: 0.35rem;
    flex-shrink: 0;
  }
  .btn {
    font: inherit;
    font-size: 0.85rem;
    padding: 0.5rem 1.1rem;
    border-radius: 999px;
    cursor: pointer;
    border: 1px solid color-mix(in oklab, currentColor 18%, transparent);
    background: transparent;
    color: inherit;
  }
  .btn.ghost:hover:not(:disabled) {
    background: color-mix(in oklab, currentColor 6%, transparent);
  }
  .row-actions .btn.small {
    padding: 0.32rem 0.65rem;
    font-size: 0.78rem;
    border-radius: 0.4rem;
  }
  .row-actions .btn.danger {
    color: color-mix(in oklab, crimson 75%, currentColor);
    border-color: color-mix(in oklab, crimson 35%, var(--border));
  }

  @media (max-width: 600px) {
    .row-actions {
      flex-wrap: wrap;
    }
  }
</style>
