<script lang="ts">
  import { ACCOUNT_COLOR_PALETTE } from '$lib/accountColor';
  import InfoBubble from '$lib/components/InfoBubble.svelte';

  interface Props {
    id: number;
    color: string;
    defaultColor: string;
    displayName: string;
    email: string;
    signaturePlain: string;
    onUpdate: (patch: {
      color?: string;
      display_name?: string;
      signature_plain?: string;
    }) => void;
  }
  let { id, color, defaultColor, displayName, email, signaturePlain, onUpdate }: Props =
    $props();
</script>

<section class="setting-group">
  <h4 class="group-title">Identity</h4>

  <div class="field">
    <div class="field-label">
      <label for="display-name-{id}">
        Display name
        <InfoBubble text="The name recipients see in the 'From' line of mail you send from this mailbox. Leave blank to send as just your email address." />
      </label>
    </div>
    <input
      id="display-name-{id}"
      type="text"
      class="std-input"
      value={displayName}
      placeholder={email}
      oninput={(e) => onUpdate({ display_name: (e.currentTarget as HTMLInputElement).value })}
    />
  </div>

  <div class="field">
    <div class="field-label">
      <label for="color-{id}">
        Mailbox colour
        <InfoBubble text="Drives the unread pill next to each message in the inbox so you can see at a glance which mailbox a message landed in. Click a swatch, or pick any colour. The pill fades to grey once a message is read." />
      </label>
    </div>
    <div class="color-row">
      <span
        class="color-preview"
        style:background-color={color || defaultColor}
        aria-hidden="true"
      ></span>
      <div class="color-swatches">
        {#each ACCOUNT_COLOR_PALETTE as swatch (swatch)}
          <button
            type="button"
            class="swatch"
            class:active={color.toLowerCase() === swatch}
            style:background-color={swatch}
            aria-label={`Use ${swatch}`}
            onclick={() => onUpdate({ color: swatch })}
          ></button>
        {/each}
      </div>
      <input
        id="color-{id}"
        type="color"
        class="color-input"
        value={color || defaultColor}
        oninput={(e) => onUpdate({ color: (e.currentTarget as HTMLInputElement).value })}
        title="Pick any colour"
      />
      {#if color}
        <button
          type="button"
          class="btn small"
          onclick={() => onUpdate({ color: '' })}
          title="Revert to the auto-assigned default for this mailbox"
        >Default</button>
      {/if}
    </div>
  </div>

  <div class="field">
    <div class="field-label">
      <label for="signature-{id}">Signature</label>
      <InfoBubble text="Auto-appended to outgoing mail from this account. Stored as plain text — the standard '-- ' delimiter is inserted automatically so recipients' mail clients can hide it on reply. Auto-insert on replies and forwards is controlled per-user in Display → Notifications; it's off by default." />
    </div>
    <textarea
      id="signature-{id}"
      class="signature-editor"
      rows="4"
      placeholder="Jane Doe&#10;Postern Systems · privacy you own"
      value={signaturePlain}
      oninput={(e) => onUpdate({ signature_plain: (e.currentTarget as HTMLTextAreaElement).value })}
    ></textarea>
  </div>
</section>
