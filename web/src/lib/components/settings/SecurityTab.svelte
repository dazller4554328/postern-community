<script lang="ts">
  import { onMount } from 'svelte';
  import { api, type TrustedDevice } from '$lib/api';
  import { prefs } from '$lib/prefs';
  import { lockVault, clearLocalTrustedDevice } from '$lib/vault';
  import { formatDate } from '$lib/format';
  import InfoBubble from '$lib/components/InfoBubble.svelte';
  import TotpSection from './_components/TotpSection.svelte';
  import { tier } from '$lib/tier';

  let devices = $state<TrustedDevice[]>([]);
  let devicesLoaded = $state(false);
  let devicesLoading = $state(false);
  // Pagination — list grows unbounded on long-running deployments
  // (each phone-WiFi-cellular-flip can mint a new persistent token
  // if the user has remember-me ticked). Slice client-side because
  // listTrustedDevices returns the whole table; that's still cheap
  // (a row is ~200 bytes) but rendering hundreds of <li>s tanks the
  // panel.
  const DEVICES_PAGE_SIZE = 8;
  let devicesPage = $state(0);
  let totalDevicePages = $derived(
    Math.max(1, Math.ceil(devices.length / DEVICES_PAGE_SIZE))
  );
  let pagedDevices = $derived(
    devices.slice(
      devicesPage * DEVICES_PAGE_SIZE,
      (devicesPage + 1) * DEVICES_PAGE_SIZE
    )
  );
  function devicesPrev() {
    if (devicesPage > 0) devicesPage -= 1;
  }
  function devicesNext() {
    if (devicesPage < totalDevicePages - 1) devicesPage += 1;
  }
  // Reset to page 0 whenever the list shrinks past the current
  // window — e.g. after Revoke removes the last item on page 3.
  $effect(() => {
    if (devicesPage >= totalDevicePages) {
      devicesPage = Math.max(0, totalDevicePages - 1);
    }
  });

  // TOTP enrollment + disable flows live in ./_components/TotpSection.svelte.

  // Read eventTicker reactively from the prefs store; write back via
  // prefs.update so other tabs see the change too.
  let eventTicker = $state(false);
  $effect(() => {
    const unsub = prefs.subscribe((p) => {
      eventTicker = p.eventTicker;
    });
    return unsub;
  });
  function setEventTicker(v: boolean) {
    prefs.update((p) => ({ ...p, eventTicker: v }));
  }

  async function loadDevices() {
    devicesLoading = true;
    try {
      devices = await api.listTrustedDevices();
      devicesLoaded = true;
    } catch (e) {
      console.error('trusted devices load failed', e);
    } finally {
      devicesLoading = false;
    }
  }

  async function revokeDevice(id: number) {
    if (!confirm('Revoke this device? It will need the master password on its next request.')) return;
    try {
      const res = await api.revokeTrustedDevice(id);
      if (res.self) {
        // We just pulled our own cookie out from under us. A full reload
        // is the cleanest way to land back on the unlock screen. Also
        // drop the client-side enrol flag so the "Remember this device"
        // checkbox comes back on the next render.
        clearLocalTrustedDevice();
        window.location.reload();
        return;
      }
      await loadDevices();
    } catch (e) {
      alert(e instanceof Error ? e.message : String(e));
    }
  }

  async function revokeAllDevices() {
    if (!confirm('Revoke every trusted device? You and every other phone/browser will need the master password again.')) return;
    try {
      await api.revokeAllTrustedDevices();
      // This browser's own enrolment just got wiped server-side —
      // clear the local mirror too so the checkbox reappears.
      clearLocalTrustedDevice();
      window.location.reload();
    } catch (e) {
      alert(e instanceof Error ? e.message : String(e));
    }
  }

  function deviceShortUA(ua: string | null): string {
    if (!ua) return 'Unknown browser';
    // Heuristic: pluck browser + platform from a UA string.
    const isMobile = /Mobi|Android|iPhone|iPad/i.test(ua);
    let browser = 'Browser';
    if (/Firefox/.test(ua)) browser = 'Firefox';
    else if (/Edg/.test(ua)) browser = 'Edge';
    else if (/Brave/.test(ua)) browser = 'Brave';
    else if (/Chrome/.test(ua)) browser = 'Chrome';
    else if (/Safari/.test(ua)) browser = 'Safari';
    let os = '';
    if (/Windows/.test(ua)) os = 'Windows';
    else if (/Mac OS X/.test(ua) || /Macintosh/.test(ua)) os = 'macOS';
    else if (/iPhone|iPad|iOS/i.test(ua)) os = 'iOS';
    else if (/Android/.test(ua)) os = 'Android';
    else if (/Linux/.test(ua)) os = 'Linux';
    const suffix = isMobile && os ? `${os} (mobile)` : os;
    return suffix ? `${browser} — ${suffix}` : browser;
  }

  // TotpSection loads its own status on mount.
  onMount(() => {
    loadDevices();
  });
</script>

<section class="panel">
  <div class="section-head">
    <h2>Security</h2>
    <p>Master password and the local security audit trail.</p>
  </div>

  <div class="field">
    <div class="field-label">
      <label>
        Change master password
        <InfoBubble text="Re-wraps every stored app password and PGP private key under a new key, and re-encrypts the database. You'll need the current password and be logged out at the end." />
      </label>
    </div>
    <form class="change-pw" onsubmit={async (e) => {
      e.preventDefault();
      const form = e.currentTarget as HTMLFormElement;
      const data = new FormData(form);
      const old_pw = data.get('old_pw') as string;
      const new_pw = data.get('new_pw') as string;
      const confirm_pw = data.get('confirm_pw') as string;
      if (new_pw.length < 8) { alert('New password must be at least 8 characters.'); return; }
      if (new_pw !== confirm_pw) { alert('New passwords do not match.'); return; }
      try {
        await api.vaultChangePassword(old_pw, new_pw);
        alert('Master password changed. You will be locked out — please unlock with the new password.');
        await lockVault();
      } catch (err) {
        alert(err instanceof Error ? err.message : String(err));
      }
    }}>
      <input type="password" name="old_pw" placeholder="Current password" required />
      <input type="password" name="new_pw" placeholder="New password (min 8 chars)" required />
      <input type="password" name="confirm_pw" placeholder="Confirm new password" required />
      <button type="submit" class="btn danger">Change password</button>
    </form>
  </div>

  <!-- ─────────── Two-factor authentication ─────────── -->
  <TotpSection />

  <div class="row">
    <div class="label">
      <strong class="inline">
        Security audit log
        <InfoBubble text="Local ledger of vault access, IP shifts, account changes, and policy adjustments. View the Activity tab from here for sync/send events instead." />
      </strong>
    </div>
    <a class="btn" href="/settings/audit">Inspect →</a>
  </div>

  <div class="row">
    <div class="label">
      <strong class="inline">
        Event ticker
        <InfoBubble text="Scroll every new audit + activity event once along the bottom of the screen — a live-feed style monitor for sync cycles, send outcomes, folder changes, and security events. Only events that happen after you enable this appear; historical entries never replay." />
      </strong>
      <span class="field-sub">
        {eventTicker ? 'On — new events scroll along the bottom.' : 'Off.'}
      </span>
    </div>
    <label class="switch" title={eventTicker ? 'Turn off' : 'Turn on'}>
      <input type="checkbox" checked={eventTicker} onchange={(e) => setEventTicker((e.currentTarget as HTMLInputElement).checked)} />
      <span class="track"></span>
    </label>
  </div>

  <!-- Trusted devices — browsers that skip the IP-change auto-lock.
       Pro-only: the concept is meaningless on a localhost build. -->
  {#if $tier.features.trusted_devices}
  <div class="field">
    <div class="field-label">
      <label>
        Trusted devices
        <InfoBubble text="Browsers that ticked 'Remember this device' at unlock. Each token is random, server-verified, and revocable — the actual value only lives in the cookie. IP-change auto-lock is skipped while a valid token is present. Tokens expire after 30 days regardless." />
      </label>
    </div>

    {#if devicesLoading && !devicesLoaded}
      <p class="muted">Loading devices…</p>
    {:else if devices.length === 0}
      <p class="muted">No remembered devices yet. Tick "Remember this device" next time you unlock on a browser you trust.</p>
    {:else}
      <ul class="device-list">
        {#each pagedDevices as d (d.id)}
          <li>
            <div class="device-copy">
              <strong>{deviceShortUA(d.user_agent)}</strong>
              <span class="device-meta">
                {#if d.last_seen_at}
                  Last active {formatDate(d.last_seen_at)}
                  {#if d.last_seen_ip}· {d.last_seen_ip}{/if}
                {:else}
                  Enrolled {formatDate(d.created_at)}
                {/if}
                · expires {formatDate(d.expires_at)}
              </span>
            </div>
            <button class="btn danger" onclick={() => revokeDevice(d.id)}>Revoke</button>
          </li>
        {/each}
      </ul>
      {#if totalDevicePages > 1}
        <div class="pager" aria-label="Device pages">
          <button
            type="button"
            class="btn ghost"
            disabled={devicesPage === 0}
            onclick={devicesPrev}
          >Previous</button>
          <span class="pager-status">
            Page {devicesPage + 1} of {totalDevicePages}
            <span class="pager-detail">— {devices.length} devices total</span>
          </span>
          <button
            type="button"
            class="btn ghost"
            disabled={devicesPage >= totalDevicePages - 1}
            onclick={devicesNext}
          >Next</button>
        </div>
      {/if}
      <div class="device-actions">
        <button class="btn ghost" type="button" onclick={loadDevices}>Refresh</button>
        <button class="btn danger" type="button" onclick={revokeAllDevices}>
          Revoke all devices
        </button>
      </div>
    {/if}
  </div>
  {/if}
</section>

<style>
  /* TOTP enrollment + status — scoped to this component so the
     existing global .btn / .field / .muted styles still apply. */
  /* TOTP-specific styles live in _components/TotpSection.svelte. */
  /* Existing global .btn doesn't define a 'small' modifier — add a
     local one so the manual-code copy button doesn't dwarf the
     paragraph next to it. */
  :global(.btn.small) {
    padding: 0.35rem 0.7rem;
    font-size: 0.78rem;
  }

  /* Trusted-devices pager. Shares the visual rhythm of the AI
     activity / history pagers so the panels feel of-a-set. */
  .pager {
    display: flex;
    align-items: center;
    gap: 0.7rem;
    margin: 0.6rem 0 0.4rem;
    padding: 0.55rem 0.75rem;
    background: color-mix(in oklab, currentColor 4%, transparent);
    border-radius: 0.5rem;
    font-size: 0.82rem;
  }
  .pager-status {
    flex: 1 1 auto;
    text-align: center;
    color: color-mix(in oklab, currentColor 70%, transparent);
  }
  .pager-detail {
    opacity: 0.7;
    font-size: 0.78rem;
    margin-left: 0.3rem;
  }
</style>
