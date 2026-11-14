<script lang="ts">
  import type { SendAttachment } from '$lib/api';

  interface Props {
    /// Two-way bound — the parent reads attachments to pass into the
    /// SendRequest, and the component mutates it on add/remove.
    attachments: SendAttachment[];
  }

  let { attachments = $bindable() }: Props = $props();

  function bufToBase64(buf: ArrayBuffer): string {
    const bytes = new Uint8Array(buf);
    let bin = '';
    for (const b of bytes) bin += String.fromCharCode(b);
    return btoa(bin);
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

  function removeAttachment(i: number) {
    attachments = attachments.filter((_, idx) => idx !== i);
  }

  /// data_base64 is ceil(N/4) × 3 raw bytes per encoded length; we use
  /// the exact "×3/4" since the pad chars are negligible at KB scale.
  function totalSizeKb(): number {
    return Math.round(
      attachments.reduce((a, x) => a + (x.data_base64.length * 3) / 4, 0) / 1024
    );
  }
</script>

<!-- Renders only the chip-list and add-button; the parent provides
     the `<div class="row"><label>Attachments</label>...</div>` shell
     so the compose-shell's row layout/label styling applies. -->
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

<style>
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
</style>
