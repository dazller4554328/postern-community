/** Accounts + per-account-settings slice. The largest single
 *  domain — CRUD, sync flags, archive policy, retention, purge,
 *  avatar/color, plus the global `getSyncInterval` / `folders` /
 *  `triggerSync` etc. */

import { request } from './_client';
import type {
  Account,
  ArchiveStrategy,
  AutoArchivePreview,
  FoldersResponse,
  NewAccount,
  PurgeReport,
  RetentionPreview,
  RobohashSet,
  SyncReport
} from '../api';

export const accountsApi = {
  listAccounts: () => request<Account[]>('/api/accounts'),
  createAccount: (a: NewAccount) =>
    request<Account>('/api/accounts', {
      method: 'POST',
      body: JSON.stringify(a)
    }),
  deleteAccount: (id: number) =>
    request<{ deleted: number }>(`/api/accounts/${id}`, { method: 'DELETE' }),
  updateAccountCredentials: (id: number, appPassword: string) =>
    request<{ id: number; ok: boolean }>(`/api/accounts/${id}/credentials`, {
      method: 'POST',
      body: JSON.stringify({ app_password: appPassword })
    }),
  /** Edit IMAP/SMTP connection details. The server re-tests the IMAP
   *  login with the stored password before saving, so a bad host is
   *  rejected rather than silently breaking sync. */
  updateAccountServer: (
    id: number,
    body: { imap_host: string; imap_port: number; smtp_host: string | null; smtp_port: number | null }
  ) =>
    request<{ account: Account }>(`/api/accounts/${id}/server`, {
      method: 'POST',
      body: JSON.stringify(body)
    }),
  setSyncEnabled: (id: number, enabled: boolean) =>
    request<{ id: number; sync_enabled: boolean }>(
      `/api/accounts/${id}/sync-enabled`,
      { method: 'POST', body: JSON.stringify({ enabled }) }
    ),
  setSendEnabled: (id: number, enabled: boolean) =>
    request<{ id: number; send_enabled: boolean }>(
      `/api/accounts/${id}/send-enabled`,
      { method: 'POST', body: JSON.stringify({ enabled }) }
    ),
  setIncludeInUnified: (id: number, enabled: boolean) =>
    request<{ id: number; include_in_unified: boolean }>(
      `/api/accounts/${id}/include-in-unified`,
      { method: 'POST', body: JSON.stringify({ enabled }) }
    ),
  /** Privacy: whether remote sender-avatar lookups are allowed. Off by
   *  default — each lookup leaks "this sender is being viewed" to
   *  Libravatar/Gravatar/DuckDuckGo. */
  getRemoteAvatars: () =>
    request<{ enabled: boolean }>('/api/settings/remote-avatars'),
  setRemoteAvatars: (enabled: boolean) =>
    request<{ enabled: boolean }>('/api/settings/remote-avatars', {
      method: 'POST',
      body: JSON.stringify({ enabled })
    }),
  getSyncInterval: () =>
    request<{ interval_secs: number }>('/api/settings/sync-interval'),
  setSyncInterval: (secs: number) =>
    request<{ interval_secs: number }>('/api/settings/sync-interval', {
      method: 'POST',
      body: JSON.stringify({ interval_secs: secs })
    }),
  setDeletePolicy: (id: number, deleteAfterSync: boolean) =>
    request<{ id: number }>(`/api/accounts/${id}/delete-policy`, {
      method: 'POST',
      body: JSON.stringify({ delete_after_sync: deleteAfterSync })
    }),
  startServerPurge: (id: number, precheckOnly: boolean) =>
    request<{ id: number; started: boolean; mode: 'precheck' | 'execute' }>(
      `/api/accounts/${id}/purge-server-copies`,
      {
        method: 'POST',
        body: JSON.stringify({ precheck_only: precheckOnly })
      }
    ),
  getPurgeStatus: (id: number) =>
    request<{ id: number; report: PurgeReport | null }>(
      `/api/accounts/${id}/purge-status`
    ),
  setArchiveFolder: (id: number, folder: string | null) =>
    request<{ id: number; archive_folder: string | null; effective: string }>(
      `/api/accounts/${id}/archive-folder`,
      { method: 'POST', body: JSON.stringify({ archive_folder: folder }) }
    ),
  setArchiveStrategy: (id: number, strategy: ArchiveStrategy) =>
    request<{ id: number; strategy: ArchiveStrategy; preview: string }>(
      `/api/accounts/${id}/archive-strategy`,
      { method: 'POST', body: JSON.stringify({ strategy }) }
    ),
  setArchiveEnabled: (id: number, enabled: boolean) =>
    request<{ id: number; archive_enabled: boolean }>(
      `/api/accounts/${id}/archive-enabled`,
      { method: 'POST', body: JSON.stringify({ enabled }) }
    ),
  setAutoArchive: (
    id: number,
    enabled: boolean,
    age_days: number,
    read_only: boolean
  ) =>
    request<{
      id: number;
      auto_archive_enabled: boolean;
      auto_archive_age_days: number;
      auto_archive_read_only: boolean;
    }>(`/api/accounts/${id}/auto-archive`, {
      method: 'POST',
      body: JSON.stringify({ enabled, age_days, read_only })
    }),
  autoArchivePreview: (id: number) =>
    request<AutoArchivePreview>(`/api/accounts/${id}/auto-archive/preview`),
  setRetention: (id: number, enabled: boolean, days: number) =>
    request<{ id: number; retention_enabled: boolean; retention_days: number }>(
      `/api/accounts/${id}/retention`,
      { method: 'POST', body: JSON.stringify({ enabled, days }) }
    ),
  retentionPreview: (id: number) =>
    request<RetentionPreview>(`/api/accounts/${id}/retention/preview`),
  setPurgeGmailCategories: (id: number, enabled: boolean) =>
    request<{ id: number; purge_gmail_categories: boolean }>(
      `/api/accounts/${id}/purge-gmail-categories`,
      { method: 'POST', body: JSON.stringify({ enabled }) }
    ),
  setSkipGmailTrash: (id: number, enabled: boolean) =>
    request<{ id: number; skip_gmail_trash: boolean }>(
      `/api/accounts/${id}/skip-gmail-trash`,
      { method: 'POST', body: JSON.stringify({ enabled }) }
    ),
  rescanGmailLabels: (id: number) =>
    request<{ scanned: number; updated: number }>(
      `/api/accounts/${id}/rescan-gmail-labels`,
      { method: 'POST' }
    ),
  setAvatar: (id: number, seed: string | null, set: RobohashSet) =>
    request<{ id: number; avatar_seed: string | null; avatar_set: RobohashSet }>(
      `/api/accounts/${id}/avatar`,
      { method: 'POST', body: JSON.stringify({ seed, set }) }
    ),
  setAccountColor: (id: number, color: string | null) =>
    request<{ id: number; color: string | null }>(
      `/api/accounts/${id}/color`,
      { method: 'POST', body: JSON.stringify({ color }) }
    ),
  setDisplayName: (id: number, displayName: string | null) =>
    request<{ id: number; display_name: string | null }>(
      `/api/accounts/${id}/display-name`,
      { method: 'POST', body: JSON.stringify({ display_name: displayName }) }
    ),
  triggerSync: (id: number) =>
    request<{ triggered: number }>(`/api/accounts/${id}/sync`, { method: 'POST' }),
  syncStatus: (id: number) =>
    request<SyncReport | null>(`/api/accounts/${id}/sync/status`),
  folders: () => request<FoldersResponse>('/api/folders')
};
