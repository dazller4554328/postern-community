<script lang="ts">
  import type { AccountFolders, FolderEntry } from '$lib/api';
  import FolderIcon from '../FolderIcon.svelte';
  import FolderTreeNode from './FolderTreeNode.svelte';
  import type { TreeCtx, TreeNode } from './folderTreeTypes';

  interface Props {
    acct: AccountFolders;
    collapsed: boolean;
    visSystem: FolderEntry[];
    visCategories: FolderEntry[];
    userTree: TreeNode[];
    hasUserRows: boolean;
    acctUnread: number;
    accountColor: string;
    lastSyncLabel: string | null;
    ctx: TreeCtx;
    onToggleCollapse: () => void;
  }
  let {
    acct,
    collapsed,
    visSystem,
    visCategories,
    userTree,
    hasUserRows,
    acctUnread,
    accountColor,
    lastSyncLabel,
    ctx,
    onToggleCollapse,
  }: Props = $props();

  let creatingHere = $derived(
    ctx.creatingIn?.accountId === acct.account_id && ctx.creatingIn?.parent === ''
  );
</script>

<section class="account" class:collapsed>
  <button
    type="button"
    class="acct-header"
    onclick={onToggleCollapse}
    title={collapsed ? 'Expand folders' : 'Collapse folders'}
    aria-expanded={!collapsed}
  >
    <span class="acct-avatar-wrap">
      <span
        class="acct-color"
        class:collapsed
        style:background-color={accountColor}
        aria-hidden="true"
        title={acct.email}
      ></span>
    </span>
    <span class="acct-meta">
      <span class="email" title={acct.email}>{acct.email}</span>
      {#if !collapsed && lastSyncLabel}
        <span class="last-sync">{lastSyncLabel}</span>
      {/if}
    </span>
    {#if collapsed && acctUnread > 0}
      <span class="unread-count acct-badge">{acctUnread}</span>
    {/if}
    <span class="acct-chevron" class:open={!collapsed} aria-hidden="true">
      <svg viewBox="0 0 12 12" width="10" height="10" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
        <path d="m4 2 4 4-4 4"/>
      </svg>
    </span>
  </button>

  {#if !collapsed}
    <div class="account-body">
      {#each visSystem as f, i (f.name)}
        {@const isLastSystem = i === visSystem.length - 1 && visCategories.length === 0 && !hasUserRows}
        {@const canUnread = ctx.folderShowsUnread(f.name)}
        {@const canTotal = ctx.folderShowsTotal(f.name)}
        <div class="tree-node">
          <div class="tree-row-wrap">
            <span class="conn elbow" class:end={isLastSystem} aria-hidden="true"></span>
            <span class="tree-chev-spacer" aria-hidden="true"></span>
            <button
              type="button"
              class="row tree-row branch-row"
              class:active={ctx.isActive(acct.account_id, f.name)}
              class:unread={canUnread && f.unread > 0}
              onclick={() => ctx.navigateFolder(acct.account_id, f.name)}
              title={ctx.folderTooltip(f.display, f.total, f.unread, f.size_bytes)}
            >
              <FolderIcon name={f.name} kind="system" />
              <span class="label">{f.display}</span>
              {#if canUnread && f.unread > 0}
                <span class="unread-count">
                  {f.unread}{#if canTotal && f.total > f.unread}<span class="total-tail">/{f.total}</span>{/if}
                </span>
              {:else if canTotal && f.total > 0}
                <span class="total-count">{f.total}</span>
              {/if}
            </button>
          </div>
        </div>
      {/each}

      {#if visCategories.length > 0}
        {#each visCategories as f, i (f.name)}
          {@const isLastCategory = i === visCategories.length - 1 && !hasUserRows}
          {@const canUnread = ctx.folderShowsUnread(f.name)}
          {@const canTotal = ctx.folderShowsTotal(f.name)}
          <div class="tree-node">
            <div class="tree-row-wrap">
              <span class="conn elbow" class:end={isLastCategory} aria-hidden="true"></span>
              <span class="tree-chev-spacer" aria-hidden="true"></span>
              <button
                type="button"
                class="row tree-row branch-row category-row"
                class:active={ctx.isActive(acct.account_id, f.name)}
                class:unread={canUnread && f.unread > 0}
                onclick={() => ctx.navigateFolder(acct.account_id, f.name)}
                title={ctx.folderTooltip(f.display, f.total, f.unread, f.size_bytes)}
              >
                <FolderIcon name={f.name} kind="gmail_category" />
                <span class="label">{f.display}</span>
                {#if canUnread && f.unread > 0}
                  <span class="unread-count">
                    {f.unread}{#if canTotal && f.total > f.unread}<span class="total-tail">/{f.total}</span>{/if}
                  </span>
                {:else if canTotal && f.total > 0}
                  <span class="total-count">{f.total}</span>
                {/if}
              </button>
            </div>
          </div>
        {/each}
      {/if}

      <div class="tree-node">
        <div class="tree-row-wrap">
          <span class="conn elbow" class:end={!hasUserRows} aria-hidden="true"></span>
          <span class="tree-chev-spacer" aria-hidden="true"></span>
          <button
            type="button"
            class="row tree-row branch-row tree-action-row"
            onclick={() => ctx.beginCreate(acct.account_id, '')}
            title="Create a new top-level folder"
            aria-label="New folder"
          >
            <span class="new-folder-icon" aria-hidden="true">+</span>
            <span class="label">New folder</span>
          </button>
        </div>
      </div>
      {#if creatingHere}
        <div class="tree-node">
          <div class="tree-row-wrap">
            <span class="conn elbow end" aria-hidden="true"></span>
            <span class="tree-chev-spacer" aria-hidden="true"></span>
            <form
              class="inline-form"
              onsubmit={(e) => { e.preventDefault(); ctx.submitCreate(); }}
            >
              <input
                type="text"
                value={ctx.createValue}
                oninput={(e) => (ctx.createValue = (e.currentTarget as HTMLInputElement).value)}
                placeholder="new folder name…"
                onblur={() => ctx.cancelCreate()}
                onkeydown={(e) => { if (e.key === 'Escape') ctx.cancelCreate(); }}
                autofocus
              />
            </form>
          </div>
        </div>
      {/if}
      {#each userTree as node, i (node.fullPath)}
        <FolderTreeNode
          accountId={acct.account_id}
          {node}
          trail={[]}
          isLast={i === userTree.length - 1}
          {ctx}
        />
      {/each}
    </div>
  {/if}
</section>
