import type { AccountFolders, FolderEntry } from '$lib/api';
import { countsTowardAggregateUnread } from '$lib/folderSemantics';
import type { TreeNode } from '../folderTreeTypes';

/// "12.4 MB", "3 KB", "521 B" — same scale step used elsewhere.
export function humanBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / (1024 * 1024)).toFixed(1)} MB`;
  return `${(n / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

/// title= text for a folder row: shows what the badge can't fit.
export function folderTooltip(
  display: string,
  total: number,
  unread: number,
  sizeBytes: number
): string {
  const parts = [display];
  if (total > 0) {
    parts.push(unread > 0 ? `${unread} unread of ${total}` : `${total} message${total === 1 ? '' : 's'}`);
  }
  if (sizeBytes > 0) parts.push(humanBytes(sizeBytes));
  return parts.join(' · ');
}

export function treeKey(accountId: number, fullPath: string): string {
  return `${accountId}:${fullPath}`;
}

/// Aggregate unread across an account, filtered to folders where the
/// count is semantically meaningful (excludes Sent/Drafts/Trash/etc).
export function accountUnread(acct: AccountFolders): number {
  const all = [...acct.system, ...acct.categories, ...acct.user];
  return all
    .filter((f) => countsTowardAggregateUnread(f.name))
    .reduce((s, f) => s + f.unread, 0);
}

/// Aggregate unread across a subtree — shown on a collapsed parent
/// so the user knows there's something inside worth expanding.
export function subtreeUnread(n: TreeNode): number {
  let u = 0;
  if (n.entry && countsTowardAggregateUnread(n.entry.name)) {
    u += n.entry.unread;
  }
  for (const c of n.children) u += subtreeUnread(c);
  return u;
}

/// Build a tree from "Work/Projects/Alpha"-style folder names. A node
/// with no `entry` is a grouping-only parent (children have mail, the
/// parent itself doesn't exist as an IMAP folder). Sorts siblings
/// alphabetically; depth comes from the walk, not from a level field.
export function buildTree(folders: FolderEntry[], hideEmpty: boolean): TreeNode[] {
  const filtered = hideEmpty
    ? folders.filter((f) => (f.total ?? 0) > 0 || (f.unread ?? 0) > 0)
    : folders;
  const root: TreeNode = { segment: '', fullPath: '', entry: null, children: [] };
  const sorted = [...filtered].sort((a, b) => a.name.localeCompare(b.name));
  for (const f of sorted) {
    const parts = f.name.split('/');
    let cur = root;
    let acc = '';
    for (let i = 0; i < parts.length; i++) {
      const seg = parts[i];
      acc = acc ? `${acc}/${seg}` : seg;
      let child = cur.children.find((c) => c.segment === seg);
      if (!child) {
        child = { segment: seg, fullPath: acc, entry: null, children: [] };
        cur.children.push(child);
      }
      if (i === parts.length - 1) child.entry = f;
      cur = child;
    }
  }
  return root.children;
}

/// System folders + Gmail categories with zero messages. System-row
/// filtering is a subset: never hide INBOX / Sent / Drafts / Trash /
/// Spam even when empty — they're reference points users expect to
/// see. Categories (Updates, Promotions, Social…) and generic empties
/// do get hidden.
const NEVER_HIDE_SYSTEM = new Set([
  'INBOX',
  'Sent',
  'Drafts',
  'Trash',
  'Spam',
  'Junk',
  '[Gmail]/Sent Mail',
  '[Gmail]/Drafts',
  '[Gmail]/Trash',
  '[Gmail]/Spam',
]);

export function visibleSystem(list: FolderEntry[], hideEmpty: boolean): FolderEntry[] {
  if (!hideEmpty) return list;
  return list.filter(
    (f) => NEVER_HIDE_SYSTEM.has(f.name) || (f.total ?? 0) > 0 || (f.unread ?? 0) > 0
  );
}

export function visibleCategories(list: FolderEntry[], hideEmpty: boolean): FolderEntry[] {
  if (!hideEmpty) return list;
  return list.filter((f) => (f.total ?? 0) > 0 || (f.unread ?? 0) > 0);
}
