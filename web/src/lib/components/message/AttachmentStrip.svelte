<script lang="ts">
  import { api, type Forensics } from '$lib/api';
  import { humanBytes } from '$lib/format';

  type Attachment = NonNullable<Forensics['attachments']>[number];

  interface Props {
    messageId: number;
    attachments: Attachment[];
    sandboxAvailable: boolean | null;
  }
  let { messageId, attachments, sandboxAvailable }: Props = $props();

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
    'text/plain',
  ]);
  // Types that aren't natively previewable but CAN be rendered via
  // the viewer sandbox (LibreOffice → PDF → PDF.js). Gated on the
  // sandbox actually being running.
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
  function isInlinePreviewable(ct: string): boolean {
    return INLINE_TYPES.has(ct.toLowerCase().trim());
  }
  function isConvertible(ct: string): boolean {
    return CONVERT_TYPES.has(ct.toLowerCase().trim());
  }
</script>

{#if attachments.length > 0}
  <div class="att-strip">
    <span class="att-label">
      📎 {attachments.length} attachment{attachments.length === 1 ? '' : 's'}
    </span>
    <ul class="att-list">
      {#each attachments as a, i (i)}
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

<style>
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
</style>

