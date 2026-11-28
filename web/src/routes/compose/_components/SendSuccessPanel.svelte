<script lang="ts">
  import { goto } from '$app/navigation';
  import type { SendForensics } from '$lib/api';

  interface SendResult {
    message_id: string;
    appended: boolean;
    encrypted: boolean;
    forensics: SendForensics;
    /// Frozen at dispatch so the success card can show "you sent this
    /// to …" even after the undo window closes — handy when a user is
    /// looking back at a stack of recent sends and needs to remember
    /// exactly who got what.
    recap: {
      to: string;
      cc: string;
      bcc: string;
      subject: string;
    };
  }

  interface Props {
    sent: SendResult;
  }

  let { sent }: Props = $props();
</script>

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

<style>
  .success {
    padding: 1rem 1.25rem;
    border: 1px solid color-mix(in oklab, forestgreen 40%, transparent);
    background: color-mix(in oklab, forestgreen 8%, transparent);
    border-radius: 1rem;
  }
  .success code {
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 0.78rem;
    background: color-mix(in oklab, currentColor 5%, transparent);
    padding: 0 0.35em;
    border-radius: 0.2em;
  }

  /* Recipient / subject recap — shown after dispatch so the user can
     verify who the message actually went to. Full recipient strings
     are rendered verbatim; truncating here would defeat the point. */
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

  /* `.actions` had no compose-level styling — the buttons render with
     browser defaults, matching the original. Keep it that way. */
  .actions {
    display: flex;
    gap: 0.5rem;
    margin-top: 0.85rem;
    flex-wrap: wrap;
  }
</style>
