<script lang="ts">
  import './styles.css';
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { page } from '$app/stores';
  import { api, type Account, type VpnStatus } from '$lib/api';
  import AboutTab from '$lib/components/settings/AboutTab.svelte';
  import AutomationTab from '$lib/components/settings/AutomationTab.svelte';
  import UpdatesTab from '$lib/components/settings/UpdatesTab.svelte';
  import PrivacyTab from '$lib/components/settings/PrivacyTab.svelte';
  import CalendarsTab from '$lib/components/settings/CalendarsTab.svelte';
  import SecurityTab from '$lib/components/settings/SecurityTab.svelte';
  import ImportTab from '$lib/components/settings/ImportTab.svelte';
  import DisplayTab from '$lib/components/settings/DisplayTab.svelte';
  import ArchiveTab from '$lib/components/settings/ArchiveTab.svelte';
  import MailboxesTab from '$lib/components/settings/MailboxesTab.svelte';
  import AiTab from '$lib/components/settings/AiTab.svelte';
  import { tier } from '$lib/tier';
  import { get } from 'svelte/store';

  // ---- Tabs ----
  type TabId =
    | 'mailboxes'
    | 'calendars'
    | 'archive'
    | 'backups'
    | 'import'
    | 'display'
    | 'security'
    | 'privacy'
    | 'automation'
    | 'ai'
    | 'updates'
    | 'about';
  // AI tab only appears when this build supports it. Filtered at
  // render-time so a build without AI doesn't ship the dead label.
  const ALL_TABS: { id: TabId; label: string; gate?: 'ai' }[] = [
    { id: 'mailboxes', label: 'Mailboxes' },
    { id: 'calendars', label: 'Calendars' },
    { id: 'archive', label: 'Archive' },
    { id: 'backups', label: 'Backups' },
    { id: 'import', label: 'Import' },
    { id: 'display', label: 'Display' },
    { id: 'security', label: 'Security' },
    { id: 'privacy', label: 'Privacy' },
    { id: 'automation', label: 'Automation' },
    { id: 'ai', label: 'AI', gate: 'ai' },
    { id: 'updates', label: 'Updates' },
    { id: 'about', label: 'About' }
  ];
  const TABS = $derived(
    ALL_TABS.filter((t) => {
      if (t.gate === 'ai') return $tier.features.ai;
      return true;
    })
  );
  let tab = $state<TabId>('mailboxes');

  function selectTab(next: TabId) {
    // Backups lives on its own page (`/settings/backups`) so the
    // tab acts as a router shortcut rather than an in-page panel
    // toggle. Everything else stays in-page via `tab` state.
    if (next === 'backups') {
      goto('/settings/backups');
      return;
    }
    if (tab === next) return;
    tab = next;
    const url = new URL(window.location.href);
    url.searchParams.set('tab', next);
    goto(url.pathname + url.search, { replaceState: true, noScroll: true });
    try { localStorage.setItem('postern.settings.tab', next); } catch {}
  }

  // ---- Server-backed state ----
  let accounts = $state<Account[]>([]);
  let vpn = $state<VpnStatus | null>(null);
  let loadingAccounts = $state(true);

  onMount(async () => {
    // Restore last tab first so initial render matches expectations.
    try {
      const urlTab = $page.url.searchParams.get('tab') as TabId | null;
      const stored = localStorage.getItem('postern.settings.tab') as TabId | null;
      const picked = urlTab ?? stored;
      if (picked && TABS.some((t) => t.id === picked)) tab = picked;
    } catch {}

    try {
      // VPN status is pro-only. On community builds the route doesn't
      // exist — fetching it would 404 and poison the whole Promise.all.
      // The tier store has already loaded by the time the settings page
      // mounts; read it directly so we only send the requests this
      // build actually supports.
      const snapshot = get(tier);
      const supportsVpn = snapshot.features.vpn;

      const [accts, vpnStatus] = await Promise.all([
        api.listAccounts(),
        supportsVpn ? api.vpnStatus() : Promise.resolve(null)
      ]);
      accounts = accts;
      vpn = vpnStatus;
    } catch (e) {
      console.error(e);
    } finally {
      loadingAccounts = false;
    }
  });
</script>

<article class="settings-shell">
  <div class="page-top">
    <a class="back" href="/inbox">← Inbox</a>
  </div>

  <header class="hero">
    <div class="hero-copy">
      <span class="eyebrow">Control Surface</span>
      <h1>Settings</h1>
      <p>Pick a tab. Changes to mailbox and archive settings don't apply until you click Save.</p>
    </div>
    <div class="hero-badges">
      <span class="hero-chip">Vault sealed at rest</span>
      <span class="hero-chip">Local key custody</span>
      <span class="hero-chip">No remote fonts</span>
    </div>
  </header>

  <nav class="tab-bar" role="tablist" aria-label="Settings">
    {#each TABS as t (t.id)}
      <button
        role="tab"
        class="tab"
        class:active={tab === t.id}
        aria-selected={tab === t.id}
        onclick={() => selectTab(t.id)}
      >
        {t.label}
      </button>
    {/each}
  </nav>

  <!-- ========= MAILBOXES ========= -->
  {#if tab === 'mailboxes'}<MailboxesTab {accounts} {loadingAccounts} onAccountsChanged={async () => { accounts = await api.listAccounts(); }} />{/if}

  {#if tab === 'calendars'}<CalendarsTab />{/if}

  <!-- ========= ARCHIVE ========= -->
  {#if tab === 'archive'}<ArchiveTab {accounts} {loadingAccounts} onAccountsChanged={async () => { accounts = await api.listAccounts(); }} />{/if}

  {#if tab === 'import'}<ImportTab {accounts} />{/if}

  {#if tab === 'display'}<DisplayTab />{/if}

  <!-- ========= SECURITY ========= -->
  {#if tab === 'security'}<SecurityTab />{/if}

  {#if tab === 'privacy'}<PrivacyTab {vpn} />{/if}
  {#if tab === 'automation'}<AutomationTab />{/if}
  {#if tab === 'ai' && $tier.features.ai}<AiTab />{/if}
  {#if tab === 'updates'}<UpdatesTab />{/if}
  {#if tab === 'about'}<AboutTab />{/if}
</article>

