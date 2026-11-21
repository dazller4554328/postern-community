<script lang="ts">
  import VpnBadge from '$lib/components/VpnBadge.svelte';
  import { lockVault } from '$lib/vault';
  import type { FoldersResponse } from '$lib/api';

  interface Props {
    folders: FoldersResponse | null;
    syncTarget: 'all' | number;
    syncing: boolean;
    isMobile: boolean;
    hasSelection: boolean;
    mobileToolbarCollapsed: boolean;
    onForceSync: () => void;
    onToggleMobileToolbar: () => void;
    onBeginMobileToolbarGesture: (e: PointerEvent) => void;
    onEndMobileToolbarGesture: (e: PointerEvent) => void;
    onCancelMobileToolbarGesture: () => void;
  }
  let {
    folders,
    syncTarget = $bindable(),
    syncing,
    isMobile,
    hasSelection,
    mobileToolbarCollapsed,
    onForceSync,
    onToggleMobileToolbar,
    onBeginMobileToolbarGesture,
    onEndMobileToolbarGesture,
    onCancelMobileToolbarGesture,
  }: Props = $props();
</script>

<div
  class="top-bar-wrap"
  class:hidden-mobile={isMobile && hasSelection}
  class:collapsed-mobile={isMobile && mobileToolbarCollapsed}
>
  <div class="top-bar" role="toolbar" aria-label="Main actions">
    <a class="tb-btn" href="/compose" title="Compose" aria-label="Compose">
      <svg viewBox="0 0 20 20" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
        <path d="M14.5 2.5 17 5l-9 9-3 1 1-3Z"/>
        <path d="M12.5 4.5 15.5 7.5"/>
      </svg>
      <span class="tb-label">Compose</span>
    </a>

    <div class="tb-sep" aria-hidden="true"></div>

    <div class="tb-group">
      <button
        type="button"
        class="tb-btn"
        disabled={syncing}
        onclick={onForceSync}
        title="Force send & receive for the selected account"
        aria-label="Send and receive"
      >
        <svg viewBox="0 0 20 20" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" class:spinning={syncing}>
          <path d="M3 10a7 7 0 0 1 12-5l2-2v5h-5"/>
          <path d="M17 10a7 7 0 0 1-12 5l-2 2v-5h5"/>
        </svg>
        <span class="tb-label">{syncing ? 'Syncing…' : 'Send & receive'}</span>
      </button>
      <select
        class="tb-select"
        bind:value={syncTarget}
        disabled={syncing}
        title="Which account to sync"
        aria-label="Account to sync"
      >
        <option value="all">All accounts</option>
        {#if folders}
          {#each folders.accounts as a (a.account_id)}
            <option value={a.account_id}>{a.email}</option>
          {/each}
        {/if}
      </select>
    </div>

    <div class="tb-spacer"></div>

    <div class="tb-vpn">
      <VpnBadge />
    </div>
    <button
      type="button"
      class="tb-btn tb-lock"
      onclick={() => lockVault()}
      title="Lock the mailbox — the vault will require your master password to unlock again"
      aria-label="Lock"
    >
      <svg viewBox="0 0 20 20" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
        <rect x="4.5" y="9" width="11" height="8" rx="1.2"/>
        <path d="M7 9V6.5a3 3 0 0 1 6 0V9"/>
      </svg>
      <span class="tb-label">Lock</span>
    </button>
  </div>
  {#if isMobile && !hasSelection}
    <button
      type="button"
      class="top-bar-handle"
      aria-expanded={!mobileToolbarCollapsed}
      aria-label={mobileToolbarCollapsed ? 'Show inbox actions' : 'Hide inbox actions'}
      title={mobileToolbarCollapsed ? 'Show inbox actions' : 'Hide inbox actions'}
      onclick={onToggleMobileToolbar}
      onpointerdown={onBeginMobileToolbarGesture}
      onpointerup={onEndMobileToolbarGesture}
      onpointercancel={onCancelMobileToolbarGesture}
    >
      <span class="handle-grip" aria-hidden="true"></span>
      <span class="handle-label">{mobileToolbarCollapsed ? 'Show actions' : 'Hide actions'}</span>
    </button>
  {/if}
</div>
