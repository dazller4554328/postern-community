<script lang="ts">
  import { onMount } from 'svelte';
  import { api, type VpnStatus } from '$lib/api';
  import { tier } from '$lib/tier';
  import { get } from 'svelte/store';

  let status = $state<VpnStatus | null>(null);
  let interval: ReturnType<typeof setInterval> | null = null;

  const POLL_MS = 20_000;

  async function refresh() {
    try {
      status = await api.vpnHealthcheck();
    } catch {
      /* silent — badge falls back to "off" */
    }
  }

  onMount(() => {
    // Skip entirely on community builds — the VPN endpoint doesn't
    // exist there, polling it would 404 every 20s for nothing. The
    // template below also renders null when the feature's off, so
    // nothing reaches the DOM either.
    if (!get(tier).features.vpn) return;
    refresh();
    interval = setInterval(refresh, POLL_MS);
    return () => {
      if (interval) clearInterval(interval);
    };
  });

  // Three-state light:
  //   connected → up, tunnel working → green
  //   off       → not enabled → grey
  //   problem   → enabled but interface down → red
  let vpnState = $derived.by((): 'off' | 'connected' | 'problem' => {
    if (!status || !status.enabled) return 'off';
    if (!status.interface_up) return 'problem';
    return 'connected';
  });

  let providerName = $derived.by(() => {
    switch (status?.provider) {
      case 'nordlynx': return 'NordVPN';
      case 'proton_wireguard': return 'ProtonVPN';
      case 'manual_wireguard': return 'WireGuard';
      default: return null;
    }
  });

  // Country flag via regional-indicator code points. Nord publishes
  // hostnames with their own abbreviations ("uk", "us", "jp"), not
  // strict ISO 3166-1 alpha-2 — map the known exceptions so the flag
  // matches what users expect.
  const NORD_TO_ISO: Record<string, string> = {
    uk: 'gb'
  };
  function flagEmoji(code: string | null | undefined): string {
    if (!code) return '';
    const iso = (NORD_TO_ISO[code.toLowerCase()] ?? code).toLowerCase();
    if (iso.length !== 2) return '';
    const A = 0x1f1e6;
    const a = 'a'.charCodeAt(0);
    const cp1 = A + (iso.charCodeAt(0) - a);
    const cp2 = A + (iso.charCodeAt(1) - a);
    return String.fromCodePoint(cp1) + String.fromCodePoint(cp2);
  }

  let flag = $derived(flagEmoji(status?.server_country_code));
  let serverLabel = $derived.by(() => {
    // Prefer "UK #2347" when we have both; fall back to just one,
    // or the region label for providers without structured data.
    const cc = status?.server_country_code?.toUpperCase();
    const num = status?.server_number;
    if (cc && num) return `${cc} #${num}`;
    if (cc) return cc;
    if (num) return `#${num}`;
    // ManualWireGuard users: fall back to whatever region they tagged.
    return status?.region_label ?? '';
  });

  let tooltip = $derived.by(() => {
    if (!status?.enabled) return 'Outbound VPN is disabled. Click to configure.';
    const parts: string[] = [];
    if (providerName) parts.push(providerName);
    if (status.server_city) parts.push(status.server_city);
    if (status.server_load !== null) parts.push(`load ${status.server_load}%`);
    if (status.exit_ip) parts.push(`exit ${status.exit_ip}`);
    if (status.last_error) parts.push(`error: ${status.last_error}`);
    return parts.join(' · ') || 'VPN status';
  });
</script>

{#if $tier.features.vpn}
<a class="badge {vpnState}" href="/settings/vpn" title={tooltip}>
  {#if providerName === 'NordVPN'}
    <!-- Provider mark: Nord mountain. Always blue — the state signal
         lives in the dot + border, not the logo (so the brand stays
         recognisable at a glance). -->
    <svg class="provider-icon" viewBox="0 0 48 48" width="18" height="18" aria-hidden="true">
      <rect width="48" height="48" rx="10" fill="#4687ff" />
      <path
        fill="#ffffff"
        d="M10 36 L19 13 L24.5 22.5 L30 13 L38 36 L32 36 L28 26 L24 34 L20 26 L16 36 Z"
      />
    </svg>
  {:else if providerName === 'ProtonVPN'}
    <!-- Proton brand purple with the stylised "P" wordmark curve. Like
         the Nord glyph this stays in the brand colour regardless of
         connection state. -->
    <svg class="provider-icon" viewBox="0 0 48 48" width="18" height="18" aria-hidden="true">
      <rect width="48" height="48" rx="10" fill="#6d4aff" />
      <path
        fill="#ffffff"
        d="M16 12h11c5 0 8.5 3.5 8.5 8.2 0 4.8-3.5 8.4-8.5 8.4h-6.3V36H16V12zm10.6 11.8c2.3 0 3.7-1.4 3.7-3.6s-1.4-3.5-3.7-3.5h-5.9v7.1h5.9z"
      />
    </svg>
  {:else if providerName === 'WireGuard'}
    <svg class="provider-icon" viewBox="0 0 24 24" width="16" height="16" aria-hidden="true">
      <circle cx="12" cy="12" r="9" fill="#88171a" />
      <path fill="#ffffff" d="M8 10h2v4H8zm3-2h2v8h-2zm3 3h2v2h-2z" />
    </svg>
  {/if}

  {#if status?.enabled && (flag || serverLabel)}
    <span class="region">
      {#if flag}<span class="flag" aria-hidden="true">{flag}</span>{/if}
      {#if serverLabel}<span class="server">{serverLabel}</span>{/if}
    </span>
  {:else if !status?.enabled}
    <span class="server muted">VPN off</span>
  {:else}
    <span class="server">VPN down</span>
  {/if}

  <span class="dot" aria-label={vpnState}></span>
</a>
{/if}

<style>
  .badge {
    display: inline-flex;
    align-items: center;
    gap: 0.45rem;
    padding: 0.25rem 0.65rem 0.25rem 0.4rem;
    border-radius: 999px;
    font-size: 0.72rem;
    font-weight: 550;
    text-decoration: none;
    color: inherit;
    border: 1px solid var(--border);
    background: color-mix(in oklab, var(--surface-2) 50%, transparent);
    transition: background 120ms, border-color 120ms, box-shadow 120ms;
    white-space: nowrap;
    line-height: 1;
  }
  .badge:hover {
    background: color-mix(in oklab, var(--surface-2) 85%, transparent);
  }
  .provider-icon {
    flex-shrink: 0;
    display: block;
    line-height: 0;
    border-radius: 4px;
  }
  .region {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    min-width: 0;
  }
  .flag {
    font-size: 0.95rem;
    line-height: 1;
    /* Flag emojis have their own colour rendering and don't follow
       currentColor. Nothing to do here beyond sizing. */
  }
  .server {
    font-variant-numeric: tabular-nums;
    letter-spacing: 0.01em;
  }
  .muted {
    opacity: 0.65;
  }
  /* The status dot is the single authoritative signal — green when
     everything's good, red when something's wrong, grey when the
     VPN isn't on. Paired with a soft glow so it registers at a
     glance. */
  .dot {
    width: 9px;
    height: 9px;
    border-radius: 50%;
    flex-shrink: 0;
    background: #6b7280;
    box-shadow: 0 0 0 0 currentColor;
  }
  .connected .dot {
    background: #10b981;
    box-shadow: 0 0 0 3px color-mix(in oklab, #10b981 30%, transparent);
  }
  .connected {
    border-color: color-mix(in oklab, #10b981 40%, var(--border));
  }
  .problem .dot {
    background: #ef4444;
    box-shadow: 0 0 0 3px color-mix(in oklab, #ef4444 30%, transparent);
    animation: pulse 1.5s ease-in-out infinite;
  }
  .problem {
    border-color: color-mix(in oklab, #ef4444 55%, var(--border));
    background: color-mix(in oklab, #ef4444 8%, var(--surface-2));
  }
  .off .dot {
    background: #6b7280;
  }
  .off .server {
    opacity: 0.65;
  }

  @keyframes pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.55; }
  }
</style>
