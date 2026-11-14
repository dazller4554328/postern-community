<script lang="ts">
  import FolderIcon from '../FolderIcon.svelte';
  import Self from './FolderTreeNode.svelte';
  import type { TreeNode, TreeCtx } from './folderTreeTypes';

  interface Props {
    accountId: number;
    node: TreeNode;
    trail: boolean[];
    isLast: boolean;
    ctx: TreeCtx;
  }
  let { accountId, node, trail, isLast, ctx }: Props = $props();

  let hasChildren = $derived(node.children.length > 0);
  let key = $derived(ctx.treeKey(accountId, node.fullPath));
  let nodeCollapsed = $derived(ctx.collapsedTreeNodes[key]);
  let u = $derived(ctx.subtreeUnread(node));
  let menuKey = $derived(`${accountId}:${node.fullPath}`);
  let isRenaming = $derived(
    ctx.renameTarget?.accountId === accountId && ctx.renameTarget.path === node.fullPath
  );
  let menuIsOpen = $derived(ctx.menuOpen === menuKey);
  let creatingHere = $derived(
    ctx.creatingIn?.accountId === accountId && ctx.creatingIn?.parent === node.fullPath
  );
</script>

<div class="tree-node">
  <div class="tree-row-wrap">
    <!-- Ancestor rail columns: one per ancestor depth. `cont=true` means
         that ancestor still has siblings below, so a vertical rail
         continues through this row. `cont=false` = blank column. -->
    {#each trail as cont}
      <span class="conn" class:rail={cont} aria-hidden="true"></span>
    {/each}
    <!-- Elbow into this node: └── if this node is the last child of
         its parent, otherwise ├──. -->
    <span class="conn elbow" class:end={isLast} aria-hidden="true"></span>
    {#if hasChildren}
      <button
        type="button"
        class="tree-chev"
        class:open={!nodeCollapsed}
        onclick={() => ctx.toggleTreeNode(accountId, node.fullPath)}
        title={nodeCollapsed ? 'Expand' : 'Collapse'}
        aria-label={nodeCollapsed ? 'Expand' : 'Collapse'}
      >
        <svg viewBox="0 0 12 12" width="10" height="10" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
          <path d="m4 2 4 4-4 4"/>
        </svg>
      </button>
    {:else}
      <span class="tree-chev-spacer" aria-hidden="true"></span>
    {/if}
    {#if isRenaming}
      <form
        class="inline-form"
        onsubmit={(e) => { e.preventDefault(); ctx.submitRename(); }}
      >
        <input
          type="text"
          value={ctx.renameValue}
          oninput={(e) => (ctx.renameValue = (e.currentTarget as HTMLInputElement).value)}
          onblur={() => ctx.cancelRename()}
          onkeydown={(e) => { if (e.key === 'Escape') ctx.cancelRename(); }}
          autofocus
        />
      </form>
    {:else if node.entry}
      {@const canUnread = ctx.folderShowsUnread(node.entry.name)}
      {@const canTotal = ctx.folderShowsTotal(node.entry.name)}
      <button
        type="button"
        class="row tree-row"
        class:active={ctx.isActive(accountId, node.entry.name)}
        class:unread={canUnread && node.entry.unread > 0}
        onclick={() => ctx.navigateFolder(accountId, node.entry!.name)}
        oncontextmenu={(e) => { e.preventDefault(); ctx.openMenu(accountId, node.fullPath, e); }}
        title={ctx.folderTooltip(node.entry.display, node.entry.total, node.entry.unread, node.entry.size_bytes)}
      >
        <FolderIcon name={node.segment} kind="user" />
        <span class="label">{node.segment}</span>
        {#if canUnread && node.entry.unread > 0}
          <span class="unread-count">
            {node.entry.unread}{#if canTotal && node.entry.total > node.entry.unread}<span class="total-tail">/{node.entry.total}</span>{/if}
          </span>
        {:else if canTotal && node.entry.total > 0}
          <span class="total-count">{node.entry.total}</span>
        {/if}
      </button>
      <button
        type="button"
        class="row-menu"
        onclick={(e) => ctx.openMenu(accountId, node.fullPath, e)}
        aria-label="Folder actions"
        title="Folder actions"
      >
        ⋯
      </button>
    {:else}
      <!-- Grouping node with no folder of its own -->
      <button
        type="button"
        class="row tree-row grouping"
        onclick={() => ctx.toggleTreeNode(accountId, node.fullPath)}
        oncontextmenu={(e) => { e.preventDefault(); ctx.openMenu(accountId, node.fullPath, e); }}
        title="Folder group"
      >
        <FolderIcon name="" kind="user" />
        <span class="label">{node.segment}</span>
        {#if nodeCollapsed && u > 0}
          <span class="unread-count">{u}</span>
        {/if}
      </button>
      <button
        type="button"
        class="row-menu"
        onclick={(e) => ctx.openMenu(accountId, node.fullPath, e)}
        aria-label="Folder actions"
        title="Folder actions"
      >
        ⋯
      </button>
    {/if}
    {#if menuIsOpen}
      <div
        class="ctx-menu"
        role="menu"
        onclick={(e) => e.stopPropagation()}
        onkeydown={(e) => { if (e.key === 'Escape') ctx.closeMenus(); }}
        tabindex="-1"
      >
        <button type="button" onclick={() => ctx.beginCreate(accountId, node.fullPath)}>New subfolder…</button>
        {#if node.entry}
          <button type="button" onclick={() => ctx.beginRename(accountId, node.fullPath, node.segment)}>Rename…</button>
          <button type="button" class="danger" onclick={() => ctx.deleteFolder(accountId, node.fullPath)}>Delete</button>
        {/if}
      </div>
    {/if}
  </div>
  {#if creatingHere}
    <div class="tree-node">
      <div class="tree-row-wrap">
        {#each [...trail, !isLast] as cont}
          <span class="conn" class:rail={cont} aria-hidden="true"></span>
        {/each}
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
  {#if hasChildren && !nodeCollapsed}
    {#each node.children as child, i (child.fullPath)}
      <Self
        {accountId}
        node={child}
        trail={[...trail, !isLast]}
        isLast={i === node.children.length - 1}
        {ctx}
      />
    {/each}
  {/if}
</div>
