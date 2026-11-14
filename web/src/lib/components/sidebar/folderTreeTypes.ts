import type { FolderEntry } from '$lib/api';

export interface TreeNode {
  segment: string;
  fullPath: string;
  entry: FolderEntry | null;
  children: TreeNode[];
}

/** Bundle of handlers + reactive state that the recursive folder
 *  tree needs. The parent owns the $state cells and exposes them
 *  via getters so reactive reads propagate through the wrapper. */
export interface TreeCtx {
  readonly collapsedTreeNodes: Record<string, boolean>;
  readonly menuOpen: string | null;
  readonly renameTarget: { accountId: number; path: string } | null;
  renameValue: string;
  readonly creatingIn: { accountId: number; parent: string } | null;
  createValue: string;
  cancelRename(): void;
  cancelCreate(): void;
  toggleTreeNode(accountId: number, fullPath: string): void;
  openMenu(accountId: number, path: string, e: Event): void;
  closeMenus(): void;
  beginRename(accountId: number, path: string, current: string): void;
  beginCreate(accountId: number, parentPath: string): void;
  submitRename(): void | Promise<void>;
  submitCreate(): void | Promise<void>;
  deleteFolder(accountId: number, path: string): void | Promise<void>;
  isActive(accountId: number | null, folder: string | null): boolean;
  navigateFolder(accountId: number | null, folder: string | null): void;
  treeKey(accountId: number, fullPath: string): string;
  folderShowsUnread(name: string): boolean;
  folderShowsTotal(name: string): boolean;
  folderTooltip(display: string, total: number, unread: number, sizeBytes: number): string;
  subtreeUnread(n: TreeNode): number;
}
