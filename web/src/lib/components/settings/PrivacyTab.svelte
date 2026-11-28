<script lang="ts">
  import InfoBubble from '$lib/components/InfoBubble.svelte';
  import { tier } from '$lib/tier';
  import type { VpnStatus } from '$lib/api';
  import { remoteAvatars, setRemoteAvatars, ensureRemoteAvatarSetting } from '$lib/avatars';

  let { vpn }: { vpn: VpnStatus | null } = $props();

  ensureRemoteAvatarSetting();

  async function onToggleRemoteAvatars(e: Event) {
    await setRemoteAvatars((e.currentTarget as HTMLInputElement).checked);
  }
</script>

<section class="panel">
  <div class="section-head">
    <h2>Encryption &amp; privacy</h2>
    <p>Key management and egress routing.</p>
  </div>
  <div class="row">
    <div class="label">
      <strong class="inline">
        PGP Keys
        <InfoBubble text="Generate or import your keypair, harvest keys received via Autocrypt, and look up recipients via WKD." />
      </strong>
    </div>
    <a class="btn" href="/settings/pgp">Manage →</a>
  </div>
  <div class="row">
    <div class="label">
      <strong class="inline">
        Remote sender avatars
        <InfoBubble text="When on, Postern fetches a real avatar for each sender (Libravatar/Gravatar, then the sender's domain icon, then DuckDuckGo). Each lookup tells those services that this mailbox is viewing that sender. Off by default; locally-stored contact photos are always shown either way. Fetches ride the VPN tunnel when one is active." />
      </strong>
      <span class="field-sub">
        {$remoteAvatars
          ? 'On — sender avatars are fetched from third-party services.'
          : 'Off — initials only; no third-party lookup leaves your server.'}
      </span>
    </div>
    <label class="switch" title={$remoteAvatars ? 'Turn off' : 'Turn on'}>
      <input type="checkbox" checked={$remoteAvatars} onchange={onToggleRemoteAvatars} />
      <span class="track"></span>
    </label>
  </div>
  {#if $tier.features.vpn}
  <div class="row">
    <div class="label">
      <strong class="inline">
        Outbound VPN
        <InfoBubble text="Route IMAP and SMTP traffic through a VPN tunnel (WireGuard or NordLynx). Kill-switch drops the connection if the tunnel falls." />
      </strong>
      <span class="field-sub">
        {#if vpn?.enabled && vpn.interface_up}
          Active — {vpn.region_label ?? 'tunnel up'}
          {#if vpn.killswitch_enabled}· kill-switch on{/if}
        {:else if vpn?.enabled}
          Configured but tunnel is down
        {:else}
          Not configured
        {/if}
      </span>
    </div>
    <a class="btn" href="/settings/vpn">Configure →</a>
  </div>
  {/if}
</section>
