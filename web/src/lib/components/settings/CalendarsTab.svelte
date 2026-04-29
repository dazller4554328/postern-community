<script lang="ts">
  import { onMount } from 'svelte';
  import { api, type CalAccount, type NewCalAccount } from '$lib/api';
  import { formatDate } from '$lib/format';

  let calAccounts = $state<CalAccount[]>([]);
  let calAccountsLoading = $state(false);
  let calSyncBusy = $state<Record<number, boolean>>({});
  let calAddError = $state<string | null>(null);
  let calAddBusy = $state(false);
  let calForm = $state<NewCalAccount>({
    label: '',
    server_url: '',
    username: '',
    app_password: ''
  });

  async function loadCalAccounts() {
    calAccountsLoading = true;
    try {
      calAccounts = await api.calListAccounts();
    } catch (e) {
      console.error('cal accounts load failed', e);
    } finally {
      calAccountsLoading = false;
    }
  }

  async function calSyncAccount(id: number) {
    calSyncBusy = { ...calSyncBusy, [id]: true };
    try {
      await api.calSyncAccount(id);
      await loadCalAccounts();
    } catch (e) {
      alert(e instanceof Error ? e.message : String(e));
    } finally {
      calSyncBusy = { ...calSyncBusy, [id]: false };
    }
  }

  async function calDeleteAccount(id: number) {
    if (!confirm('Remove this calendar account and delete all synced events?')) return;
    try {
      await api.calDeleteAccount(id);
      await loadCalAccounts();
    } catch (e) {
      alert(e instanceof Error ? e.message : String(e));
    }
  }

  async function addCalAccount() {
    calAddBusy = true;
    calAddError = null;
    try {
      await api.calCreateAccount({ ...calForm });
      calForm = { label: '', server_url: '', username: '', app_password: '' };
      await loadCalAccounts();
    } catch (e) {
      calAddError = e instanceof Error ? e.message : String(e);
    } finally {
      calAddBusy = false;
    }
  }

  // Component is mounted only when the Calendars tab opens (the
  // parent gates with `{#if tab === 'calendars'}`), so onMount is
  // the right time to fetch.
  onMount(() => {
    loadCalAccounts();
  });
</script>

<section class="panel">
  <div class="section-head">
    <h2>Calendars</h2>
    <p>
      CalDAV servers to sync events from. Works with Nextcloud,
      iCloud, Fastmail, Radicale and Baïkal — anything that speaks
      Basic-auth CalDAV. Same app-password flow as mailboxes.
    </p>
  </div>

  {#if calAccountsLoading}
    <p class="muted">Loading…</p>
  {:else if calAccounts.length === 0}
    <p class="muted">No calendar accounts yet.</p>
  {:else}
    <ul class="account-list">
      {#each calAccounts as a (a.id)}
        <li class="cal-acct">
          <div class="cal-acct-head">
            <strong>{a.label}</strong>
            <span class="muted">
              {a.username} · <code>{a.server_url}</code>
            </span>
          </div>
          <div class="cal-acct-meta">
            {#if a.last_sync_error}
              <span class="err-pill">last sync: {a.last_sync_error}</span>
            {:else if a.last_sync_at}
              <span class="muted">last synced {formatDate(a.last_sync_at)}</span>
            {:else}
              <span class="muted">awaiting first sync</span>
            {/if}
          </div>
          <div class="cal-acct-actions">
            <button
              class="btn"
              type="button"
              disabled={calSyncBusy[a.id]}
              onclick={() => calSyncAccount(a.id)}
            >{calSyncBusy[a.id] ? 'Syncing…' : 'Sync now'}</button>
            <button
              class="btn danger"
              type="button"
              onclick={() => calDeleteAccount(a.id)}
            >Remove</button>
          </div>
        </li>
      {/each}
    </ul>
  {/if}

  <div class="cal-add">
    <h3>Add CalDAV account</h3>
    <p class="muted">
      Paste the server URL exactly as your provider gives it. We auto-
      discover the principal and calendar-home-set on first sync.
    </p>
    <label>
      <span>Label</span>
      <input type="text" bind:value={calForm.label} placeholder="Personal" />
    </label>
    <label>
      <span>Server URL</span>
      <input
        type="url"
        bind:value={calForm.server_url}
        placeholder="https://caldav.fastmail.com/ or https://cloud.example.com/remote.php/dav/"
      />
    </label>
    <label>
      <span>Username</span>
      <input type="text" bind:value={calForm.username} autocomplete="off" />
    </label>
    <label>
      <span>App password</span>
      <input type="password" bind:value={calForm.app_password} autocomplete="new-password" />
    </label>
    {#if calAddError}
      <p class="err">⚠ {calAddError}</p>
    {/if}
    <button
      class="btn primary"
      type="button"
      disabled={calAddBusy || !calForm.label || !calForm.server_url || !calForm.username || !calForm.app_password}
      onclick={addCalAccount}
    >{calAddBusy ? 'Adding…' : 'Add & sync'}</button>
  </div>
</section>
