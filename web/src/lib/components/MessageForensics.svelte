<script lang="ts">
  import type { Forensics } from '$lib/api';
  import { flagEmoji, humanBytes } from '$lib/format';

  interface Props {
    forensics: Forensics;
  }

  let { forensics }: Props = $props();

  function verdictClass(v: string) {
    if (v === 'pass') return 'ok';
    if (v === 'fail' || v === 'permerror') return 'bad';
    if (v === 'softfail' || v === 'temperror' || v === 'neutral') return 'warn';
    return 'unknown';
  }
</script>

<div class="forensics">
  <div class="summary">
    <dl>
      <dt>Size</dt><dd>{humanBytes(forensics.size_bytes)}</dd>
      <dt>Attachments</dt><dd>{forensics.attachments.length}</dd>
      {#if forensics.spam_score !== null}
        <dt>Spam score</dt><dd>{forensics.spam_score}</dd>
      {/if}
    </dl>
    <dl class="auth">
      <dt>SPF</dt><dd class={verdictClass(forensics.auth.spf)}>{forensics.auth.spf}</dd>
      <dt>DKIM</dt><dd class={verdictClass(forensics.auth.dkim)}>{forensics.auth.dkim}</dd>
      <dt>DMARC</dt><dd class={verdictClass(forensics.auth.dmarc)}>{forensics.auth.dmarc}</dd>
    </dl>
    <dl class="sec">
      <dt>PGP encrypted</dt><dd class={forensics.is_pgp_encrypted ? 'ok' : 'unknown'}>{forensics.is_pgp_encrypted ? 'yes' : 'no'}</dd>
      <dt>PGP signed</dt><dd class={forensics.is_pgp_signed ? 'ok' : 'unknown'}>{forensics.is_pgp_signed ? 'yes' : 'no'}</dd>
      <dt>S/MIME encrypted</dt><dd class={forensics.is_smime_encrypted ? 'ok' : 'unknown'}>{forensics.is_smime_encrypted ? 'yes' : 'no'}</dd>
      <dt>S/MIME signed</dt><dd class={forensics.is_smime_signed ? 'ok' : 'unknown'}>{forensics.is_smime_signed ? 'yes' : 'no'}</dd>
    </dl>
  </div>

  {#if forensics.received_chain.length}
    <h3>Delivery path</h3>
    <ol class="chain">
      {#each forensics.received_chain.slice().reverse() as hop, i (i)}
        <li>
          <span class="hop-label">
            hop {i + 1}
            {#if hop.country_code}
              <span class="hop-flag" title={hop.country_name ?? hop.country_code}>{flagEmoji(hop.country_code)}</span>
            {/if}
          </span>
          <div class="hop-body">
            {#if hop.from}<div><strong>from</strong> <code>{hop.from}</code></div>{/if}
            {#if hop.by}<div><strong>by</strong> <code>{hop.by}</code></div>{/if}
            {#if hop.with}<div><strong>with</strong> <code>{hop.with}</code></div>{/if}
            {#if hop.ip}
              <div>
                <strong>ip</strong> <code>{hop.ip}</code>
                {#if hop.country_name}
                  <span class="hop-country">{flagEmoji(hop.country_code)} {hop.country_name}</span>
                {/if}
              </div>
            {/if}
          </div>
        </li>
      {/each}
    </ol>
  {/if}

  {#if forensics.attachments.length}
    <h3>Attachments</h3>
    <ul class="atts">
      {#each forensics.attachments as a, i (i)}
        <li>
          <span class="ft">{a.content_type}</span>
          <span class="fn">{a.filename ?? '(unnamed)'}</span>
          <span class="fs">{humanBytes(a.size_bytes)}</span>
        </li>
      {/each}
    </ul>
  {/if}

  <h3>Headers</h3>
  <table class="headers">
    <tbody>
      {#each forensics.headers as h, i (i)}
        <tr>
          <th>{h.name}</th>
          <td>{h.value}</td>
        </tr>
      {/each}
    </tbody>
  </table>
</div>

<style>
  .forensics {
    flex: 1;
    overflow: auto;
  }
  .forensics h3 {
    font-size: 0.72rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    opacity: 0.55;
    font-weight: 600;
    margin: 1.5rem 0 0.6rem;
  }
  .summary {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(12rem, 1fr));
    gap: 0.75rem;
  }
  .summary dl {
    padding: 0.75rem 1rem;
    margin: 0;
    border: 1px solid var(--border);
    border-radius: 0.4rem;
    background: var(--surface);
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 0.35rem 1rem;
    font-size: 0.83rem;
  }
  .summary dt { opacity: 0.6; }
  .summary dd {
    margin: 0;
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 0.8rem;
  }
  .summary dd.ok { color: #12a150; font-weight: 600; }
  .summary dd.bad { color: #dc2626; font-weight: 600; }
  .summary dd.warn { color: #d97706; font-weight: 600; }
  .summary dd.unknown { opacity: 0.5; }

  ol.chain {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }
  ol.chain li {
    display: grid;
    grid-template-columns: 5rem 1fr;
    gap: 0.75rem;
    padding: 0.6rem 0.9rem;
    border: 1px solid var(--border);
    border-radius: 0.35rem;
    background: var(--surface);
    font-size: 0.83rem;
  }
  .hop-label {
    opacity: 0.55;
    font-size: 0.72rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
  }
  .hop-flag {
    font-size: 1.1rem;
    line-height: 1;
    opacity: 1;
  }
  .hop-body code {
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 0.85em;
    background: color-mix(in oklab, currentColor 5%, transparent);
    padding: 0 0.3em;
    border-radius: 0.2em;
  }
  .hop-country {
    margin-left: 0.6rem;
    opacity: 0.75;
    font-size: 0.85em;
  }

  ul.atts {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  ul.atts li {
    display: grid;
    grid-template-columns: 10rem 1fr 5rem;
    gap: 0.75rem;
    padding: 0.5rem 0.9rem;
    border: 1px solid var(--border);
    border-radius: 0.3rem;
    margin-bottom: 0.25rem;
    font-size: 0.82rem;
  }
  .ft {
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 0.78rem;
    opacity: 0.75;
  }
  .fn { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .fs { text-align: right; opacity: 0.6; font-size: 0.78rem; }

  table.headers {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.8rem;
    border: 1px solid var(--border);
    border-radius: 0.35rem;
    overflow: hidden;
  }
  table.headers tr {
    border-bottom: 1px solid var(--border);
  }
  table.headers tr:last-child { border-bottom: 0; }
  table.headers th {
    text-align: left;
    font-weight: 500;
    vertical-align: top;
    padding: 0.35rem 0.75rem;
    width: 12rem;
    background: color-mix(in oklab, currentColor 3%, transparent);
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 0.76rem;
  }
  table.headers td {
    padding: 0.35rem 0.75rem;
    word-break: break-all;
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 0.76rem;
    line-height: 1.5;
  }

  @media (max-width: 900px) {
    .summary {
      grid-template-columns: 1fr;
    }
    ol.chain li {
      grid-template-columns: 1fr;
      gap: 0.35rem;
    }
    ul.atts li {
      grid-template-columns: 1fr auto;
    }
    .fn {
      white-space: normal;
      overflow-wrap: anywhere;
    }
    table.headers,
    table.headers tbody,
    table.headers tr,
    table.headers th,
    table.headers td {
      display: block;
    }
    table.headers {
      border-radius: 0.85rem;
      overflow: hidden;
    }
    table.headers tr {
      padding: 0.55rem 0.7rem;
    }
    table.headers th,
    table.headers td {
      width: auto;
      padding: 0;
    }
    table.headers th {
      margin-bottom: 0.25rem;
      background: transparent;
      opacity: 0.62;
    }
    table.headers td {
      overflow-wrap: anywhere;
    }
  }
</style>
