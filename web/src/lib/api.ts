// Thin fetch wrapper for the Postern API. Everything is relative — in
// production the SvelteKit build is served from the same origin as the
// Rust server, so /api/* just works. In dev, vite.config.ts proxies it.

export type AccountKind = 'gmail' | 'imap';

export type ArchiveStrategy = 'single' | 'yearly' | 'monthly';

export interface Account {
  id: number;
  kind: AccountKind;
  email: string;
  display_name: string | null;
  imap_host: string;
  imap_port: number;
  smtp_host: string | null;
  smtp_port: number | null;
  vpn_required: boolean;
  delete_after_sync: boolean;
  created_at: number;
  /// User override for the archive folder base. `null` means `Archive`
  /// for both account kinds.
  archive_folder: string | null;
  /// How the archive folder is subdivided. 'single' keeps everything flat,
  /// 'yearly' buckets by year (Archive/2026), 'monthly' by year/month.
  archive_strategy: ArchiveStrategy;
  /// When false, the Archive button is hidden for this mailbox and
  /// auto-archive skips it entirely.
  archive_enabled: boolean;
  auto_archive_enabled: boolean;
  auto_archive_age_days: number;
  auto_archive_read_only: boolean;
  /// Server-side retention — when on, messages older than
  /// `retention_days` are deleted from the provider (Gmail → Trash,
  /// plain IMAP → EXPUNGE) after each sync. Postern's local copy is
  /// preserved. Starred messages are always skipped.
  retention_enabled: boolean;
  retention_days: number;
  /// Gmail-only. Nuclear-option toggle: together with
  /// `delete_after_sync`, the scheduler runs a post-sync pass over the
  /// five Gmail categories via X-GM-RAW, downloads any new message,
  /// and MOVEs every matched UID to [Gmail]/Trash so the provider
  /// copy leaves every label. Ignored for non-Gmail accounts.
  purge_gmail_categories: boolean;
  /// Paired with `purge_gmail_categories`. When on, the purge also
  /// permanently deletes everything currently in [Gmail]/Trash — no
  /// 30-day wait, quota freed immediately. Wipes the entire Trash
  /// mailbox including anything trashed via Gmail's web UI.
  skip_gmail_trash: boolean;
  /// RoboHash seed override. `null` falls back to the account's email.
  avatar_seed: string | null;
  /// Which RoboHash collection to render: set1..set5.
  avatar_set: RobohashSet;
  /// Per-account HTML signature. Rendered verbatim inside compose.
  signature_html: string | null;
  /// Plain-text signature used for plain-body sends.
  signature_plain: string | null;
  /// Master switch for inbound. When false, scheduler skips this
  /// account — no IMAP pulls, retention, or auto-archive.
  sync_enabled: boolean;
  /// Master switch for outbound. When false, SMTP refuses before
  /// touching the network.
  send_enabled: boolean;
  /// Participation in the cross-account Unified views (Inbox/Sent/
  /// Drafts/Spam/Trash at the top of the sidebar, plus "All mail").
  /// When false, this mailbox still syncs and renders per-account
  /// but is excluded from those aggregate surfaces.
  include_in_unified: boolean;
}

export type RobohashSet = 'set1' | 'set2' | 'set3' | 'set4' | 'set5';

export interface AutoArchivePreview {
  eligible_count: number;
  age_days: number;
  read_only: boolean;
  archive_base: string;
}

export interface RetentionPreview {
  eligible_count: number;
  days: number;
}

export interface ImportReport {
  scanned: number;
  imported: number;
  skipped: number;
  errors: number;
}

export interface NewAccount {
  kind: AccountKind;
  email: string;
  display_name?: string;
  imap_host: string;
  imap_port: number;
  smtp_host?: string;
  smtp_port?: number;
  app_password: string;
  vpn_required?: boolean;
  delete_after_sync?: boolean;
}

export interface MessageListItem {
  id: number;
  account_id: number;
  account_email: string;
  message_id: string;
  thread_id: string | null;
  subject: string | null;
  from_addr: string | null;
  to_addrs: string | null;
  cc_addrs: string | null;
  date_utc: number;
  snippet: string | null;
  has_attachments: boolean;
  is_read: boolean;
  is_starred: boolean;
  is_encrypted: boolean;
  /** `Disposition-Notification-To` from the incoming message — present
   *  when the sender requested a read receipt. Drives the manual
   *  "Send receipt" banner. */
  receipt_to: string | null;
}

export interface MessageDetail extends MessageListItem {
  labels: string[];
}

export interface SearchHit extends MessageListItem {
  match_snippet: string;
}

/** One row in the address book. Auto-populated from sync + send,
 *  manually edited via the Contacts page. */
export interface Contact {
  id: number;
  address: string;
  display_name: string | null;
  first_seen_utc: number;
  last_seen_utc: number;
  message_count: number;
  is_favorite: boolean;
  notes: string | null;
  created_at: number;
  updated_at: number;
}

export interface SyncReport {
  account_id: number;
  folders: { folder: string; new: number; scanned: number }[];
  started_at: number;
  finished_at: number;
  error: string | null;
}

export interface PurgeReport {
  account_id: number;
  mode: 'precheck' | 'execute';
  trigger: 'policy_change' | 'manual';
  state: 'running' | 'success' | 'failed';
  started_at: number;
  finished_at: number | null;
  scanned: number;
  verified_safe: number;
  skipped_no_local_copy: number;
  moved_or_deleted: number;
  expunged_from_trash: number;
  errors: string[];
}

export interface FolderEntry {
  name: string;
  display: string;
  kind: 'system' | 'gmail_category' | 'user';
  total: number;
  unread: number;
  /// Sum of message blob sizes in this folder, in bytes. Shown in
  /// the sidebar tooltip.
  size_bytes: number;
  weight: number;
}

export interface AccountFolders {
  account_id: number;
  email: string;
  kind: AccountKind;
  avatar_seed: string | null;
  avatar_set: RobohashSet;
  system: FolderEntry[];
  categories: FolderEntry[];
  user: FolderEntry[];
  categories_missing: string[];
  include_in_unified: boolean;
}

export interface FoldersResponse {
  accounts: AccountFolders[];
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(path, {
    ...init,
    headers: {
      'content-type': 'application/json',
      ...(init?.headers ?? {})
    }
  });
  if (!res.ok) {
    let msg = `${res.status} ${res.statusText}`;
    try {
      const body = await res.json();
      if (body?.error) msg = body.error;
    } catch {
      /* empty */
    }
    throw new Error(msg);
  }
  return res.json() as Promise<T>;
}

/**
 * POST a raw binary upload with a custom content-type (no JSON body).
 * Shares the error-envelope decode with {@link request} so import
 * endpoints surface the same friendly 4xx messages.
 */
async function uploadAttachment<T>(path: string, body: Blob, contentType: string): Promise<T> {
  const res = await fetch(path, {
    method: 'POST',
    headers: { 'content-type': contentType },
    body
  });
  if (!res.ok) {
    let msg = `${res.status} ${res.statusText}`;
    try {
      const parsed = await res.json();
      if (parsed?.error) msg = parsed.error;
    } catch {
      /* not JSON — fall back to the status line */
    }
    throw new Error(msg);
  }
  return (await res.json()) as T;
}

export const api = {
  listAccounts: () => request<Account[]>('/api/accounts'),
  createAccount: (a: NewAccount) =>
    request<Account>('/api/accounts', { method: 'POST', body: JSON.stringify(a) }),
  deleteAccount: (id: number) =>
    request<{ deleted: number }>(`/api/accounts/${id}`, { method: 'DELETE' }),
  updateAccountCredentials: (id: number, appPassword: string) =>
    request<{ id: number; ok: boolean }>(`/api/accounts/${id}/credentials`, {
      method: 'POST',
      body: JSON.stringify({ app_password: appPassword })
    }),
  setSyncEnabled: (id: number, enabled: boolean) =>
    request<{ id: number; sync_enabled: boolean }>(
      `/api/accounts/${id}/sync-enabled`,
      {
        method: 'POST',
        body: JSON.stringify({ enabled })
      }
    ),
  setSendEnabled: (id: number, enabled: boolean) =>
    request<{ id: number; send_enabled: boolean }>(
      `/api/accounts/${id}/send-enabled`,
      {
        method: 'POST',
        body: JSON.stringify({ enabled })
      }
    ),
  setIncludeInUnified: (id: number, enabled: boolean) =>
    request<{ id: number; include_in_unified: boolean }>(
      `/api/accounts/${id}/include-in-unified`,
      {
        method: 'POST',
        body: JSON.stringify({ enabled })
      }
    ),
  importMbox: (file: Blob, accountId?: number) =>
    uploadAttachment<ImportReport>(
      accountId != null ? `/api/import/mbox?account_id=${accountId}` : '/api/import/mbox',
      file,
      'application/octet-stream'
    ),
  importArchiveZip: (file: Blob, accountId?: number) =>
    uploadAttachment<ImportReport>(
      accountId != null ? `/api/import/archive-zip?account_id=${accountId}` : '/api/import/archive-zip',
      file,
      'application/zip'
    ),
  /// Path-based Maildir import. Walks `<POSTERN_IMPORT_DIR>/<path>`
  /// (server-side, sandboxed to the configured import root) and
  /// upserts every RFC822 file it finds. Dedups by Message-ID — safe
  /// to re-run.
  importMaildirPath: (path: string, accountId?: number) =>
    request<ImportReport>('/api/import/maildir', {
      method: 'POST',
      body: JSON.stringify({
        path,
        ...(accountId != null ? { account_id: accountId } : {})
      })
    }),
  getSyncInterval: () => request<{ interval_secs: number }>('/api/settings/sync-interval'),
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
      {
        method: 'POST',
        body: JSON.stringify({ archive_folder: folder })
      }
    ),
  setArchiveStrategy: (id: number, strategy: ArchiveStrategy) =>
    request<{ id: number; strategy: ArchiveStrategy; preview: string }>(
      `/api/accounts/${id}/archive-strategy`,
      {
        method: 'POST',
        body: JSON.stringify({ strategy })
      }
    ),
  setArchiveEnabled: (id: number, enabled: boolean) =>
    request<{ id: number; archive_enabled: boolean }>(
      `/api/accounts/${id}/archive-enabled`,
      {
        method: 'POST',
        body: JSON.stringify({ enabled })
      }
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
      {
        method: 'POST',
        body: JSON.stringify({ enabled, days })
      }
    ),
  retentionPreview: (id: number) =>
    request<RetentionPreview>(`/api/accounts/${id}/retention/preview`),
  setPurgeGmailCategories: (id: number, enabled: boolean) =>
    request<{ id: number; purge_gmail_categories: boolean }>(
      `/api/accounts/${id}/purge-gmail-categories`,
      {
        method: 'POST',
        body: JSON.stringify({ enabled })
      }
    ),
  setSkipGmailTrash: (id: number, enabled: boolean) =>
    request<{ id: number; skip_gmail_trash: boolean }>(
      `/api/accounts/${id}/skip-gmail-trash`,
      {
        method: 'POST',
        body: JSON.stringify({ enabled })
      }
    ),
  rescanGmailLabels: (id: number) =>
    request<{ scanned: number; updated: number }>(
      `/api/accounts/${id}/rescan-gmail-labels`,
      { method: 'POST' }
    ),
  setAvatar: (id: number, seed: string | null, set: RobohashSet) =>
    request<{ id: number; avatar_seed: string | null; avatar_set: RobohashSet }>(
      `/api/accounts/${id}/avatar`,
      {
        method: 'POST',
        body: JSON.stringify({ seed, set })
      }
    ),

  listTrustedDevices: () => request<TrustedDevice[]>('/api/security/devices'),
  revokeTrustedDevice: (id: number) =>
    request<{ id: number; removed: boolean; self: boolean }>(
      `/api/security/devices/${id}`,
      { method: 'DELETE' }
    ),
  revokeAllTrustedDevices: () =>
    request<{ revoked: number }>('/api/security/devices/revoke-all', { method: 'POST' }),
  triggerSync: (id: number) =>
    request<{ triggered: number }>(`/api/accounts/${id}/sync`, { method: 'POST' }),
  syncStatus: (id: number) => request<SyncReport | null>(`/api/accounts/${id}/sync/status`),

  folders: () => request<FoldersResponse>('/api/folders'),

  listMessages: (
    params: {
      account_id?: number;
      label?: string;
      labels?: string[];
      limit?: number;
      offset?: number;
      sort?: string;
    } = {}
  ) => {
    const q = new URLSearchParams();
    if (params.account_id != null) q.set('account_id', String(params.account_id));
    if (params.labels && params.labels.length) q.set('labels', params.labels.join(','));
    else if (params.label) q.set('label', params.label);
    if (params.limit != null) q.set('limit', String(params.limit));
    if (params.offset != null) q.set('offset', String(params.offset));
    if (params.sort) q.set('sort', params.sort);
    const qs = q.toString();
    return request<MessageListItem[]>(`/api/messages${qs ? `?${qs}` : ''}`);
  },
  getMessage: (id: number) => request<MessageDetail>(`/api/messages/${id}`),
  autocomplete: (q: string) =>
    request<string[]>(`/api/contacts/autocomplete?q=${encodeURIComponent(q)}`),

  // Contacts CRUD — drives the /contacts page and (later) any
  // import/export tooling. The autocomplete endpoint above also
  // backs onto the same table, so anything done here is immediately
  // visible in the compose recipient field.
  listContacts: (params: { q?: string; limit?: number; offset?: number } = {}) => {
    const qs = new URLSearchParams();
    if (params.q) qs.set('q', params.q);
    if (params.limit != null) qs.set('limit', String(params.limit));
    if (params.offset != null) qs.set('offset', String(params.offset));
    const tail = qs.toString();
    return request<{ contacts: Contact[]; total: number }>(
      `/api/contacts${tail ? `?${tail}` : ''}`
    );
  },
  getContact: (id: number) => request<Contact>(`/api/contacts/${id}`),
  createContact: (body: {
    address: string;
    display_name?: string;
    notes?: string;
    is_favorite?: boolean;
  }) =>
    request<Contact>('/api/contacts', {
      method: 'POST',
      body: JSON.stringify(body)
    }),
  updateContact: (
    id: number,
    body: {
      // Use null to explicitly clear; omit the property to leave alone.
      display_name?: string | null;
      notes?: string | null;
      is_favorite?: boolean;
    }
  ) =>
    request<Contact>(`/api/contacts/${id}`, {
      method: 'PATCH',
      body: JSON.stringify(body)
    }),
  deleteContact: (id: number) =>
    request<unknown>(`/api/contacts/${id}`, { method: 'DELETE' }),
  getMessagePlain: (id: number) => request<{ text: string }>(`/api/messages/${id}/plain`),
  setMessageRead: (id: number, isRead: boolean) =>
    request<{ id: number; is_read: boolean }>(`/api/messages/${id}/read`, {
      method: 'POST',
      body: JSON.stringify({ is_read: isRead })
    }),
  markSpam: (id: number) =>
    request<{ id: number }>(`/api/messages/${id}/spam`, { method: 'POST' }),
  markNotSpam: (id: number) =>
    request<{ id: number }>(`/api/messages/${id}/not-spam`, { method: 'POST' }),
  markTrash: (id: number) =>
    request<{ id: number }>(`/api/messages/${id}/trash`, { method: 'POST' }),
  markFolderRead: (accountId: number, folder: string) =>
    request<{ action: 'mark_read'; folder: string; updated: number }>(
      `/api/messages/folder-action`,
      {
        method: 'POST',
        body: JSON.stringify({ account_id: accountId, folder, action: 'mark_read' })
      }
    ),
  emptyFolder: (accountId: number, folder: string) =>
    request<{ action: 'empty'; folder: string; deleted: number }>(
      `/api/messages/folder-action`,
      {
        method: 'POST',
        body: JSON.stringify({ account_id: accountId, folder, action: 'empty' })
      }
    ),
  bulkAction: (ids: number[], action: 'trash' | 'archive' | 'read' | 'unread' | 'spam' | 'notspam') =>
    request<{ action: string; ok: number[]; failed: number[] }>(
      '/api/messages/bulk',
      { method: 'POST', body: JSON.stringify({ ids, action }) }
    ),
  bulkMoveTo: (ids: number[], folder: string) =>
    request<{ action: string; folder: string; ok: number[]; failed: number[] }>(
      '/api/messages/bulk/move-to',
      { method: 'POST', body: JSON.stringify({ ids, folder }) }
    ),
  archiveMessage: (id: number) =>
    request<{ id: number; moved_to: string; labels: string[] }>(
      `/api/messages/${id}/archive`,
      { method: 'POST' }
    ),
  moveMessage: (id: number, folder: string) =>
    request<{ id: number; target_folder: string; labels: string[] }>(
      `/api/messages/${id}/move`,
      {
        method: 'POST',
        body: JSON.stringify({ folder })
      }
    ),

  createFolder: (accountId: number, name: string) =>
    request<{ account_id: number; name: string }>(
      `/api/accounts/${accountId}/folders`,
      {
        method: 'POST',
        body: JSON.stringify({ name })
      }
    ),
  renameFolder: (accountId: number, from: string, to: string) =>
    request<{ account_id: number; from: string; to: string; labels_renamed: number }>(
      `/api/accounts/${accountId}/folders/rename`,
      {
        method: 'POST',
        body: JSON.stringify({ from, to })
      }
    ),
  deleteFolder: (accountId: number, name: string, force = false) => {
    const q = new URLSearchParams({ name });
    if (force) q.set('force', 'true');
    return request<{ account_id: number; name: string; labels_removed: number }>(
      `/api/accounts/${accountId}/folders?${q.toString()}`,
      { method: 'DELETE' }
    );
  },

  listRules: () => request<Rule[]>('/api/rules'),
  createRule: (r: NewRule) =>
    request<Rule>('/api/rules', { method: 'POST', body: JSON.stringify(r) }),
  deleteRule: (id: number) =>
    request<{ deleted: number }>(`/api/rules/${id}`, { method: 'DELETE' }),
  applyRules: () =>
    request<{ checked: number; acted: number }>('/api/rules/apply', { method: 'POST' }),
  toggleRule: (id: number, enabled: boolean) =>
    request<Rule>(`/api/rules/${id}/toggle`, {
      method: 'POST',
      body: JSON.stringify({ enabled })
    }),

  listBackups: () => request<BackupReport[]>('/api/backups'),
  createBackup: () =>
    request<BackupJob>('/api/backups/create', { method: 'POST' }),
  getBackupStatus: () =>
    request<BackupJob | null>('/api/backups/status'),
  deleteBackup: (filename: string) =>
    request<{ deleted: string }>(`/api/backups/${encodeURIComponent(filename)}`, { method: 'DELETE' }),
  /// Returns the URL the browser should navigate to in order to
  /// download the backup. Plain href; the server sets
  /// Content-Disposition: attachment so it Saves rather than displays.
  backupDownloadUrl: (filename: string) =>
    `/api/backups/${encodeURIComponent(filename)}/download`,

  // Off-site backup destinations.
  listBackupDestinations: () =>
    request<BackupDestination[]>('/api/backups/destinations'),
  createBackupDestination: (body: NewSftpDestination) =>
    request<BackupDestination>('/api/backups/destinations', {
      method: 'POST',
      body: JSON.stringify(body)
    }),
  updateBackupDestination: (id: number, patch: { label?: string; enabled?: boolean }) =>
    request<BackupDestination>(`/api/backups/destinations/${id}`, {
      method: 'PATCH',
      body: JSON.stringify(patch)
    }),
  deleteBackupDestination: (id: number) =>
    request<{ deleted: number }>(`/api/backups/destinations/${id}`, { method: 'DELETE' }),
  /// Response shape varies by destination kind:
  ///   sftp   → { ok, fingerprint, first_use }
  ///   gdrive → { ok, account_email, folder_name }
  /// All optional so callers can branch on what's present.
  testBackupDestination: (id: number) =>
    request<{
      ok: boolean;
      fingerprint?: string;
      first_use?: boolean;
      account_email?: string;
      folder_name?: string;
    }>(`/api/backups/destinations/${id}/test`, { method: 'POST' }),
  forgetBackupDestinationFingerprint: (id: number) =>
    request<BackupDestination>(`/api/backups/destinations/${id}/forget-fingerprint`, {
      method: 'POST'
    }),
  backupIntegrations: () =>
    request<Integrations>('/api/backups/integrations'),
  getBackupSchedule: () => request<BackupSchedule>('/api/backups/schedule'),
  setBackupSchedule: (body: Omit<BackupSchedule, 'last_run_at'>) =>
    request<BackupSchedule>('/api/backups/schedule', {
      method: 'POST',
      body: JSON.stringify(body)
    }),
  /// URL the browser navigates to to start the Google Drive OAuth
  /// flow. Server stashes the label, redirects to Google's consent
  /// screen, then redirects back to /settings/backups.
  gdriveOauthStartUrl: (label: string) =>
    `/api/backups/destinations/oauth/google/start?label=${encodeURIComponent(label)}`,
  pushBackupDestination: (id: number, filename?: string) =>
    request<{ ok: boolean; remote_path: string; bytes_uploaded: number }>(
      `/api/backups/destinations/${id}/push`,
      {
        method: 'POST',
        body: JSON.stringify(filename ? { filename } : {})
      }
    ),

  // Restore flow — three-step + cancel. Either start with an upload
  // (off-site backup) or with restoreFromExisting (an on-server file
  // already in the backups list).
  restoreFromExistingBackup: (filename: string) =>
    request<{ staging_id: string }>('/api/backups/restore/from-existing', {
      method: 'POST',
      body: JSON.stringify({ filename })
    }),
  uploadRestoreBackup: async (file: File): Promise<RestoreUploadResult> => {
    const fd = new FormData();
    fd.append('backup', file, file.name);
    const r = await fetch('/api/backups/restore/upload', {
      method: 'POST',
      body: fd,
      credentials: 'same-origin'
    });
    if (!r.ok) {
      let msg = 'upload failed';
      try {
        const body = (await r.json()) as { error?: string };
        if (body.error) msg = body.error;
      } catch {
        // Fall through with default message.
      }
      throw new Error(msg);
    }
    return (await r.json()) as RestoreUploadResult;
  },
  validateRestoreBackup: (stagingId: string, password: string) =>
    request<RestoreValidation>('/api/backups/restore/validate', {
      method: 'POST',
      body: JSON.stringify({ staging_id: stagingId, password })
    }),
  applyRestoreBackup: (stagingId: string) =>
    request<{ scheduled: boolean; restart_in_secs: number }>(
      '/api/backups/restore/apply',
      {
        method: 'POST',
        body: JSON.stringify({ staging_id: stagingId })
      }
    ),
  cancelRestoreBackup: (stagingId: string) =>
    request<{ cancelled: string }>(
      `/api/backups/restore/staging/${encodeURIComponent(stagingId)}`,
      { method: 'DELETE' }
    ),

  auditLog: (params: { limit?: number; offset?: number; category?: AuditCategory } = {}) => {
    const q = new URLSearchParams();
    if (params.limit != null) q.set('limit', String(params.limit));
    if (params.offset != null) q.set('offset', String(params.offset));
    if (params.category) q.set('category', params.category);
    const qs = q.toString();
    return request<AuditEntry[]>(`/api/audit${qs ? `?${qs}` : ''}`);
  },

  sendMessage: (req: SendRequest) =>
    request<SendEnqueueResponse>('/api/send', {
      method: 'POST',
      body: JSON.stringify(req)
    }),

  /// Manually send a read-receipt MDN for a message that has a
  /// non-null `receipt_to`. Postern never auto-sends — the user
  /// must click "Send receipt" on the banner. Idempotency-light:
  /// the server only refuses if the address is missing.
  sendReadReceipt: (id: number) =>
    request<{ id: number; sent_to: string }>(
      `/api/messages/${id}/send-receipt`,
      { method: 'POST' }
    ),

  outboxList: () => request<OutboxListItem[]>('/api/outbox'),
  outboxRecentFailures: () => request<OutboxListItem[]>('/api/outbox/recent-failures'),
  outboxGet: (id: number) => request<OutboxEntry>(`/api/outbox/${id}`),
  outboxCancel: (id: number) =>
    request<{ cancelled: number }>(`/api/outbox/${id}`, { method: 'DELETE' }),
  outboxReschedule: (id: number, scheduled_at: number) =>
    request<{ id: number; scheduled_at: number }>(
      `/api/outbox/${id}/reschedule`,
      { method: 'POST', body: JSON.stringify({ scheduled_at }) }
    ),

  setSignature: (account_id: number, html: string | null, plain: string | null) =>
    request<{ id: number; signature_html: string | null; signature_plain: string | null }>(
      `/api/accounts/${account_id}/signature`,
      { method: 'POST', body: JSON.stringify({ html, plain }) }
    ),

  // ---- Calendar (CalDAV) ----
  calListAccounts: () => request<CalAccount[]>('/api/cal/accounts'),
  calCreateAccount: (body: NewCalAccount) =>
    request<CalAccount>('/api/cal/accounts', {
      method: 'POST',
      body: JSON.stringify(body)
    }),
  calDeleteAccount: (id: number) =>
    request<{ deleted: number }>(`/api/cal/accounts/${id}`, { method: 'DELETE' }),
  calSyncAccount: (id: number) =>
    request<CalSyncReport>(`/api/cal/accounts/${id}/sync`, { method: 'POST' }),
  calListCalendars: (account_id?: number) => {
    const qs = account_id == null ? '' : `?account_id=${account_id}`;
    return request<CalCalendar[]>(`/api/cal/calendars${qs}`);
  },
  calListEventsInRange: (from: number, to: number) =>
    request<EventOccurrence[]>(`/api/cal/events?from=${from}&to=${to}`),
  calGetEvent: (id: number) => request<CalEvent>(`/api/cal/events/${id}`),

  // ---- Reminders (local, never leaves the device) ----
  remindersList: (includeDone = false) =>
    request<Reminder[]>(
      `/api/reminders${includeDone ? '?include_done=true' : ''}`
    ),
  remindersInRange: (from: number, to: number) =>
    request<Reminder[]>(`/api/reminders/range?from=${from}&to=${to}`),
  remindersDue: () => request<Reminder[]>('/api/reminders/due'),
  remindersCreate: (body: NewReminder) =>
    request<Reminder>('/api/reminders', {
      method: 'POST',
      body: JSON.stringify(body)
    }),
  remindersUpdate: (id: number, patch: UpdateReminder) =>
    request<Reminder>(`/api/reminders/${id}`, {
      method: 'PATCH',
      body: JSON.stringify(patch)
    }),
  remindersDelete: (id: number) =>
    request<{ deleted: number }>(`/api/reminders/${id}`, {
      method: 'DELETE'
    }),
  remindersMarkDone: (id: number) =>
    request<Reminder>(`/api/reminders/${id}/done`, { method: 'POST' }),
  remindersSnooze: (id: number, until: SnoozeUntil) =>
    request<Reminder>(`/api/reminders/${id}/snooze`, {
      method: 'POST',
      body: JSON.stringify({ until })
    }),

  // ---- Updates + license ----
  licenseGet: () => request<LicenseInfo>('/api/license'),
  licenseSet: (license_key: string | null) =>
    request<LicenseInfo>('/api/license', {
      method: 'POST',
      body: JSON.stringify({ license_key })
    }),
  licenseVerify: () =>
    request<LicenseVerifyResult>('/api/license/verify', { method: 'POST' }),
  updatesVersion: () => request<{ commit: string }>('/api/updates/version'),
  updatesCheck: () =>
    request<UpdateCheckResult>('/api/updates/check', { method: 'POST' }),
  updatesApply: () =>
    request<{ queued: boolean; message: string }>('/api/updates/apply', {
      method: 'POST'
    }),
  updatesStatus: () => request<UpdateStatusResult>('/api/updates/status'),

  // Build-tier surface. Cached on first call — compile-time constants
  // server-side, no need to re-fetch during a session.
  tier: () => request<TierInfo>('/api/tier'),

  // Datas — RAG over the local message corpus. Named after Mr. Memory
  // / Datas the music-hall mnemonist from Hitchcock's "The 39 Steps":
  // the man who knew everything. The internal route paths
  // (/api/ai/ask, /ai/ask/stream) keep the original names so older
  // bundles in flight don't break.
  aiStatus: () => request<AiStatus>('/api/ai/status'),
  aiCoverage: () => request<AiCoverage>('/api/ai/coverage'),
  aiAsk: (body: AiAskRequest) =>
    request<AiAskResponse>('/api/ai/ask', {
      method: 'POST',
      body: JSON.stringify(body)
    }),
  /// One-shot rewrite of a single block of user-authored text. No
  /// retrieval, no email context — sends only the supplied draft to
  /// the configured chat model. Used by the compose pane's "Polish"
  /// button so token spend stays bounded by what the user typed.
  aiRewrite: (body: AiRewriteRequest) =>
    request<AiRewriteResponse>('/api/ai/rewrite', {
      method: 'POST',
      body: JSON.stringify(body)
    }),
  /// Snapshot what the configured chat provider has installed /
  /// exposed (Ollama tags, OpenAI /v1/models, Anthropic /v1/models).
  /// Used by Settings → AI to populate the Polish-model dropdown so
  /// the user picks from real options instead of typing freehand.
  aiListModels: () => request<AiModelsResponse>('/api/ai/models'),
  /// Voice dictation: upload an audio blob (recorded in the browser
  /// via MediaRecorder), get a transcript back. Server forwards to
  /// the configured chat provider's transcribe endpoint — only
  /// OpenAI (Whisper) is supported today; other providers return
  /// 400. Audio routes through Postern's outbound (your VPN if
  /// configured), never browser-direct to a third party.
  aiTranscribe: async (
    blob: Blob
  ): Promise<{
    text: string;
    provider: string;
    elapsed_ms: number;
    audio_bytes: number;
  }> => {
    const form = new FormData();
    form.append('file', blob, 'audio.webm');
    const r = await fetch('/api/ai/transcribe', {
      method: 'POST',
      body: form,
      credentials: 'include'
    });
    if (!r.ok) {
      const text = await r.text().catch(() => '');
      throw new Error(text || `transcribe failed: ${r.status}`);
    }
    return r.json();
  },
  aiHistory: (params: { limit?: number; offset?: number } = {}) => {
    const q = new URLSearchParams();
    if (params.limit != null) q.set('limit', String(params.limit));
    if (params.offset != null) q.set('offset', String(params.offset));
    const qs = q.toString();
    return request<AiHistoryEntry[]>(`/api/ai/history${qs ? `?${qs}` : ''}`);
  },
  aiClearHistory: () =>
    request<{ deleted: number }>('/api/ai/history', { method: 'DELETE' }),
  /// Persisted Settings → AI config. The api key never travels in
  /// the response — only `api_key_set` indicates whether one is on
  /// file. Vault must be unlocked to save (POST), but GET is open
  /// so the panel can render even before unlock.
  aiGetSettings: () => request<AiSettingsDto>('/api/ai/settings'),
  aiUpdateSettings: (body: AiSettingsUpdate) =>
    request<AiSettingsDto>('/api/ai/settings', {
      method: 'POST',
      body: JSON.stringify(body)
    }),
  aiTestSettings: (body: AiSettingsTest) =>
    request<AiSettingsTestResult>('/api/ai/test', {
      method: 'POST',
      body: JSON.stringify(body)
    }),
  /// Quick on/off switch — does not touch provider/model/key.
  /// Off releases both providers atomically so no outbound API
  /// calls happen until it's flipped back on.
  aiSetEnabled: (enabled: boolean) =>
    request<AiSettingsDto>('/api/ai/enabled', {
      method: 'POST',
      body: JSON.stringify({ enabled })
    }),
  /// Per-call activity log (chat + embed). Capped at ~1,000 rows
  /// server-side via auto-trim. Filter chips drive the kind +
  /// provider params. Newest first.
  aiActivity: (params: {
    kind?: string;
    provider?: string;
    errors_only?: boolean;
    limit?: number;
    offset?: number;
  } = {}) => {
    const q = new URLSearchParams();
    if (params.kind) q.set('kind', params.kind);
    if (params.provider) q.set('provider', params.provider);
    if (params.errors_only) q.set('errors_only', 'true');
    if (params.limit != null) q.set('limit', String(params.limit));
    if (params.offset != null) q.set('offset', String(params.offset));
    const qs = q.toString();
    return request<AiActivityRow[]>(`/api/ai/activity${qs ? `?${qs}` : ''}`);
  },
  aiActivityDetail: (id: number) =>
    request<AiActivityDetail>(`/api/ai/activity/${id}`),
  aiActivitySummary: (window: 'hour' | 'day' | 'month' = 'day') =>
    request<AiActivitySummary>(`/api/ai/activity/summary?window=${window}`),
  aiClearActivity: () =>
    request<{ deleted: number }>('/api/ai/activity', { method: 'DELETE' }),

  /// The seven Commandments — non-negotiable rules baked into
  /// the prompt. Plus the user's editable additional rules and
  /// a live preview of the assembled system prompt.
  aiCommandments: () =>
    request<AiCommandmentsResponse>('/api/ai/commandments'),
  /// Wipe every row from `ai_embeddings`. The indexer rebuilds
  /// from scratch on its next tick. Useful after the embedding-
  /// input format changes — keeps retrieval quality consistent
  /// across the whole inbox instead of split between old + new
  /// formats.
  aiClearEmbeddings: () =>
    request<{ deleted: number }>('/api/ai/embeddings', { method: 'DELETE' }),

  /// Lockdown mode — install-wide kill-switch. When on, the server
  /// 403s every mutating endpoint (send, reply, archive, trash,
  /// move, spam, MDN, outbox reschedule), forces remote=0 on
  /// message-body rendering, and the outbox worker holds pending
  /// rows instead of dispatching. Datas Q&A still works.
  lockdownStatus: () =>
    request<{ enabled: boolean }>('/api/lockdown'),
  lockdownSet: (enabled: boolean) =>
    request<{ enabled: boolean }>('/api/lockdown', {
      method: 'POST',
      body: JSON.stringify({ enabled })
    }),

  vaultStatus: () => request<{ state: VaultState; trusted_device: boolean }>('/api/vault/status'),
  vaultInit: (password: string) =>
    request<{ state: VaultState }>('/api/vault/init', {
      method: 'POST',
      body: JSON.stringify({ password })
    }),
  vaultUnlock: (
    password: string,
    rememberDevice = false,
    opts: { totpCode?: string; recoveryCode?: string } = {}
  ) =>
    request<{ state: VaultState }>('/api/vault/unlock', {
      method: 'POST',
      body: JSON.stringify({
        password,
        remember_device: rememberDevice,
        totp_code: opts.totpCode || undefined,
        recovery_code: opts.recoveryCode || undefined
      })
    }),
  vaultLock: () =>
    request<{ state: VaultState }>('/api/vault/lock', { method: 'POST' }),
  vaultChangePassword: (oldPassword: string, newPassword: string) =>
    request<{ state: VaultState }>('/api/vault/change-password', {
      method: 'POST',
      body: JSON.stringify({ old_password: oldPassword, new_password: newPassword })
    }),

  /// Two-factor (TOTP) at vault unlock. Status is reachable while
  /// vault is locked — the unlock screen needs to know whether to
  /// show a 2FA field.
  authTotpStatus: () =>
    request<{ enabled: boolean; pending: boolean; recovery_codes_remaining: number }>(
      '/api/auth/totp/status'
    ),
  authTotpInit: () =>
    request<{ secret: string; otpauth_url: string; qr_png_data_url: string }>(
      '/api/auth/totp/init',
      {
        method: 'POST'
      }
    ),
  authTotpConfirm: (code: string) =>
    request<{ enabled: boolean; recovery_codes: string[] }>('/api/auth/totp/confirm', {
      method: 'POST',
      body: JSON.stringify({ code })
    }),
  authTotpDisable: (opts: { code?: string; recoveryCode?: string }) =>
    request<{ enabled: boolean; pending: boolean; recovery_codes_remaining: number }>(
      '/api/auth/totp/disable',
      {
        method: 'POST',
        body: JSON.stringify({
          code: opts.code || undefined,
          recovery_code: opts.recoveryCode || undefined
        })
      }
    ),
  getForensics: (id: number) => request<Forensics>(`/api/messages/${id}/forensics`),
  getRawUrl: (id: number) => `/api/messages/${id}/raw`,
  // `mode=inline` asks the server for a Content-Disposition: inline
  // response if the attachment's MIME type is on the browser's
  // safe-to-render whitelist — otherwise the server silently falls
  // back to attachment/download.
  attachmentUrl: (id: number, index: number, mode: 'inline' | 'download' = 'download') =>
    `/api/messages/${id}/attachment/${index}?mode=${mode}`,
  viewerSandboxStatus: () =>
    request<{ viewer_available: boolean }>('/api/viewer-sandbox/status'),

  search: (params: { q: string; account_id?: number; limit?: number; offset?: number; sort?: string }) => {
    const q = new URLSearchParams({ q: params.q });
    if (params.account_id != null) q.set('account_id', String(params.account_id));
    if (params.limit != null) q.set('limit', String(params.limit));
    if (params.offset != null) q.set('offset', String(params.offset));
    if (params.sort) q.set('sort', params.sort);
    return request<SearchHit[]>(`/api/search?${q.toString()}`);
  },

  vpnStatus: () => request<VpnStatus>('/api/vpn'),
  vpnInstall: (
    body:
      | { provider: 'manual_wireguard'; wg_config: string; region_label?: string; killswitch?: boolean }
      | { provider: 'proton_wireguard'; wg_config: string; killswitch?: boolean }
      | { provider: 'nordlynx'; token: string; country_id?: number; killswitch?: boolean }
  ) => request<VpnStatus>('/api/vpn/install', { method: 'POST', body: JSON.stringify(body) }),
  vpnUninstall: () => request<VpnStatus>('/api/vpn', { method: 'DELETE' }),
  vpnHealthcheck: () => request<VpnStatus>('/api/vpn/healthcheck', { method: 'POST' }),
  vpnRefresh: () => request<VpnStatus>('/api/vpn/refresh', { method: 'POST' }),
  vpnKillswitch: (enabled: boolean) =>
    request<VpnStatus>('/api/vpn/killswitch', { method: 'POST', body: JSON.stringify({ enabled }) }),
  vpnNordCountries: () => request<NordCountry[]>('/api/vpn/nord/countries'),

  pgpKeys: () => request<PgpKey[]>('/api/pgp/keys'),
  pgpGenerate: (user_id: string) =>
    request<PgpKey>('/api/pgp/keys/generate', { method: 'POST', body: JSON.stringify({ user_id }) }),
  pgpImport: (armored: string) =>
    request<PgpKey>('/api/pgp/keys', { method: 'POST', body: JSON.stringify({ armored }) }),
  pgpDelete: (id: number) => request<{ deleted: number }>(`/api/pgp/keys/${id}`, { method: 'DELETE' }),
  pgpExport: (id: number) => request<{ armored: string }>(`/api/pgp/keys/${id}/export`),
  pgpCanEncrypt: (emails: string[]) =>
    request<{ can_encrypt: boolean; missing: string[]; imported: string[] }>(
      `/api/pgp/can-encrypt?emails=${encodeURIComponent(emails.join(','))}`
    ),
  pgpDiscover: (email: string) =>
    request<PgpDiscovery>(`/api/pgp/discover?email=${encodeURIComponent(email)}`),
  pgpPublish: (id: number) =>
    request<PgpPublishResult>(`/api/pgp/keys/${id}/publish`, { method: 'POST' }),
  pgpKeyserverScan: () =>
    request<PgpKeyserverStatus[]>('/api/pgp/keyserver-scan'),
  // Returns the raw .asc file as a Blob (so the browser can trigger
  // a download). `includeSecret` controls whether private key blocks
  // are bundled — UI should confirm before passing true.
  pgpExportAll: async (includeSecret: boolean): Promise<Blob> => {
    const url = `/api/pgp/keys/export-all${includeSecret ? '?include_secret=true' : ''}`;
    const res = await fetch(url, { credentials: 'include' });
    if (!res.ok) {
      const body = await res.text().catch(() => '');
      throw new Error(body || `Export failed: ${res.status}`);
    }
    return res.blob();
  },

  listThreads: (
    params: {
      account_id?: number;
      label?: string;
      labels?: string[];
      limit?: number;
      offset?: number;
    } = {}
  ) => {
    const q = new URLSearchParams();
    if (params.account_id != null) q.set('account_id', String(params.account_id));
    if (params.labels && params.labels.length) q.set('labels', params.labels.join(','));
    else if (params.label) q.set('label', params.label);
    if (params.limit != null) q.set('limit', String(params.limit));
    if (params.offset != null) q.set('offset', String(params.offset));
    const qs = q.toString();
    return request<ThreadSummary[]>(`/api/threads${qs ? `?${qs}` : ''}`);
  },
  threadMessages: (threadId: string) =>
    request<MessageListItem[]>(`/api/threads/${encodeURIComponent(threadId)}/messages`)
};

export interface ThreadSummary {
  thread_id: string;
  subject: string | null;
  participants: string[];
  message_count: number;
  unread_count: number;
  has_attachments: boolean;
  latest_date_utc: number;
  latest_snippet: string | null;
  latest_from: string | null;
  account_emails: string[];
}

export interface PgpKey {
  id: number;
  fingerprint: string;
  user_id: string;
  primary_email: string | null;
  is_secret: boolean;
  created_at: number;
  expires_at: number | null;
  source: 'generated' | 'imported' | 'autocrypt' | 'wkd' | 'keyserver';
  last_used_at: number | null;
}

export interface PgpDiscovery {
  source: 'wkd' | 'keyserver' | 'not_found';
  armored_public_key: string | null;
  url_tried: string[];
}

export interface PgpPublishResult {
  key_fpr: string;
  verification_sent: string[];
  already_published: string[];
  key_url: string;
}

export interface PgpKeyserverStatus {
  email: string;
  presence: 'published' | 'notfound' | 'unknown';
}

export interface NordCountry {
  id: number;
  name: string;
  code: string;
}

export type AuthVerdict =
  | 'pass' | 'fail' | 'softfail' | 'neutral' | 'temperror' | 'permerror' | 'none' | 'unknown';

export interface Forensics {
  headers: { name: string; value: string }[];
  received_chain: { from: string | null; by: string | null; with: string | null; raw: string }[];
  auth: { spf: AuthVerdict; dkim: AuthVerdict; dmarc: AuthVerdict; raw: string[] };
  is_pgp_encrypted: boolean;
  is_pgp_signed: boolean;
  is_smime_signed: boolean;
  is_smime_encrypted: boolean;
  spam_score: number | null;
  spam_status: string | null;
  size_bytes: number;
  attachments: { filename: string | null; content_type: string; size_bytes: number }[];
  mime_tree: { content_type: string; size_bytes: number; is_attachment: boolean; filename: string | null }[];
}

export type VaultState = 'uninitialized' | 'locked' | 'unlocked';

export interface Rule {
  id: number;
  account_id: number | null;
  name: string;
  enabled: boolean;
  priority: number;
  condition_field: string;
  condition_op: string;
  condition_value: string;
  action_type: string;
  action_value: string;
  created_at: number;
  updated_at: number;
}

export interface BackupReport {
  filename: string;
  path: string;
  size_bytes: number;
  db_bytes: number;
  blob_count: number;
  created_at: number;
}

export interface BackupJob {
  state: 'running' | 'success' | 'failed';
  started_at: number;
  finished_at: number | null;
  report: BackupReport | null;
  error: string | null;
}

export interface SftpPublicConfig {
  host: string;
  port: number;
  username: string;
  remote_dir: string;
}

export interface GDrivePublicConfig {
  folder_id: string;
  folder_name: string;
  account_email: string;
}

/// Tagged by `kind`; the frontend reads the kind first and then picks
/// which shape `public_config` is.
export interface BackupDestination {
  id: number;
  kind: 'sftp' | 'gdrive';
  label: string;
  enabled: boolean;
  public_config: SftpPublicConfig | GDrivePublicConfig;
  /// SHA-256 hostkey fingerprint pinned via TOFU on first connect.
  /// `null` until the next successful test/push captures one.
  /// SFTP-only — always null for gdrive.
  server_fingerprint: string | null;
  last_push_at: number | null;
  last_push_filename: string | null;
  last_push_status: 'ok' | 'error' | null;
  last_push_error: string | null;
  created_at: number;
}

export interface Integrations {
  google_drive: {
    configured: boolean;
  };
}

export interface BackupSchedule {
  enabled: boolean;
  frequency: 'daily' | 'weekly';
  hour: number;
  minute: number;
  day_of_week: number; // 0=Sunday … 6=Saturday
  retention_count: number;
  last_run_at: number | null;
}

export interface NewSftpDestination {
  label: string;
  kind: 'sftp';
  sftp: {
    host: string;
    port: number;
    username: string;
    remote_dir: string;
    auth: 'password' | 'key';
    password?: string;
    key_pem?: string;
    passphrase?: string;
  };
}

export interface RestoreUploadResult {
  staging_id: string;
  size_bytes: number;
}

export interface RestoreValidation {
  staging_id: string;
  backup_filename: string;
  size_bytes: number;
  accounts: number;
  messages: number;
  blobs: number;
  created_at: number;
}

export type AuditCategory = 'security' | 'activity';

export interface TrustedDevice {
  id: number;
  user_agent: string | null;
  first_seen_ip: string | null;
  last_seen_ip: string | null;
  last_seen_at: number | null;
  created_at: number;
  expires_at: number;
}

// ---- Calendar (CalDAV) ----

export interface CalAccount {
  id: number;
  label: string;
  server_url: string;
  username: string;
  principal_url: string | null;
  calendar_home_url: string | null;
  last_sync_at: number | null;
  last_sync_error: string | null;
  created_at: number;
}

export interface NewCalAccount {
  label: string;
  server_url: string;
  username: string;
  app_password: string;
}

export interface CalCalendar {
  id: number;
  account_id: number;
  dav_url: string;
  name: string;
  ctag: string | null;
  color: string | null;
  read_only: boolean;
  hidden: boolean;
  created_at: number;
}

export interface CalEvent {
  id: number;
  calendar_id: number;
  dav_href: string;
  dav_etag: string | null;
  uid: string;
  summary: string | null;
  description: string | null;
  location: string | null;
  dtstart_utc: number;
  dtend_utc: number | null;
  all_day: boolean;
  rrule: string | null;
  raw_ics: string;
  created_at: number;
  updated_at: number;
}

/** One concrete occurrence from /api/cal/events?from=…&to=…. */
export interface EventOccurrence {
  id: number;
  calendar_id: number;
  uid: string;
  summary: string | null;
  description: string | null;
  location: string | null;
  dtstart_utc: number;
  dtend_utc: number | null;
  all_day: boolean;
  is_recurring: boolean;
  occurrence_index: number;
}

export interface CalSyncReport {
  account_id: number;
  calendars_total: number;
  calendars_changed: number;
  events_upserted: number;
  events_pruned: number;
  started_at: number;
  finished_at: number;
  error: string | null;
}

export interface AuditEntry {
  id: number;
  ts_utc: number;
  event_type: string;
  detail: string | null;
  ip: string | null;
  category: AuditCategory;
}

export interface NewRule {
  account_id?: number | null;
  name: string;
  condition_field: string;
  condition_op: string;
  condition_value: string;
  action_type: string;
  action_value: string;
  priority?: number;
}

export interface SendAttachment {
  filename: string;
  content_type: string;
  data_base64: string;
}

export interface SendRequest {
  account_id: number;
  to: string[];
  cc?: string[];
  bcc?: string[];
  subject: string;
  body: string;
  body_html?: string;
  attachments?: SendAttachment[];
  in_reply_to?: string;
  references?: string;
  pgp_encrypt?: boolean;
  /** Inject a `Disposition-Notification-To` header so receiving
   *  clients are asked to confirm the message was opened. The
   *  receipt comes back as a normal email — Postern doesn't auto-
   *  process MDNs into a separate inbox. */
  request_receipt?: boolean;
  /** Unix epoch seconds. Omit (or use a past timestamp) to dispatch ASAP. */
  scheduled_at?: number;
}

/** Response from POST /api/send — the request is enqueued, not dispatched. */
export interface SendEnqueueResponse {
  outbox_id: number;
  scheduled_at: number;
  /** True when the worker will pick this row up within ~2s. */
  immediate: boolean;
}

export interface OutboxListItem {
  id: number;
  account_id: number;
  scheduled_at: number;
  status: 'pending' | 'sending' | 'sent' | 'failed' | 'cancelled';
  attempts: number;
  last_error: string | null;
  summary_to: string;
  summary_subject: string;
  sent_message_id: string | null;
  created_at: number;
  updated_at: number;
}

/** Full outbox entry — payload_json + forensics_json attached. */
export interface OutboxEntry extends OutboxListItem {
  payload_json: string;
  forensics_json: string | null;
}

export interface SendReport {
  ok: boolean;
  message_id: string;
  encrypted: boolean;
  appended_to_sent: boolean;
  details: string | null;
  forensics: SendForensics;
}

export interface SendForensics {
  sent_at_utc: number;
  smtp_host: string;
  smtp_port: number;
  recipient_count: number;
  raw_size_bytes: number;
  bind_iface: string | null;
  vpn_enabled: boolean;
  vpn_interface_up: boolean;
  vpn_exit_ip: string | null;
  vpn_provider: string | null;
  vpn_region_label: string | null;
  vpn_server_country_code: string | null;
  vpn_server_city: string | null;
  vpn_server_number: number | null;
  killswitch_enabled: boolean;
  autocrypt_attached: boolean;
  sent_folder: string | null;
}

export interface VpnStatus {
  enabled: boolean;
  provider: string | null;
  region_label: string | null;
  interface_up: boolean;
  exit_ip: string | null;
  last_check_utc: number | null;
  last_error: string | null;
  killswitch_enabled: boolean;
  can_refresh: boolean;
  country_id: number | null;
  server_load: number | null;
  server_country_code: string | null;
  server_number: number | null;
  server_city: string | null;
}

export type ReminderRepeat = 'none' | 'daily' | 'weekly' | 'monthly';

export interface Reminder {
  id: number;
  title: string;
  notes: string | null;
  due_at_utc: number;
  repeat: ReminderRepeat;
  done: boolean;
  notified: boolean;
  snoozed_until_utc: number | null;
  created_at: number;
  updated_at: number;
}

export interface NewReminder {
  title: string;
  notes?: string | null;
  due_at_utc: number;
  repeat?: ReminderRepeat;
}

/** Patch body for PATCH /api/reminders/:id. All fields optional; `notes: null` clears the note. */
export interface UpdateReminder {
  title?: string;
  notes?: string | null;
  due_at_utc?: number;
  repeat?: ReminderRepeat;
}

/** Either a preset ("5m"/"1h"/"tomorrow") or an explicit unix-seconds target. */
export type SnoozeUntil = '5m' | '1h' | 'tomorrow' | number;

export type LicenseStatus =
  | 'unknown'
  | 'active'
  | 'malformed'
  | 'expired'
  | 'revoked'
  | 'not_found'
  | 'missing'
  | 'error';

export interface LicenseInfo {
  install_id: string;
  license_key_masked: string | null;
  license_status: LicenseStatus;
  license_tier: string | null;
  license_verified_at_utc: number | null;
}

export interface LicenseVerifyResult {
  valid: boolean;
  status: LicenseStatus;
  tier: string | null;
  message: string | null;
}

export interface UpdateCheckResult {
  current_commit: string;
  latest_commit: string | null;
  update_available: boolean;
  release_date: string | null;
  release_notes: string | null;
  filename: string | null;
  sha256: string | null;
  size_bytes: number | null;
  license_status: LicenseStatus;
  message: string | null;
}

export type UpdateState = 'idle' | 'running' | 'success' | 'failed';

export interface UpdateStatusResult {
  state: UpdateState;
  message: string | null;
  finished_at: number | null;
  trigger_pending: boolean;
}

export type BuildTier = 'pro' | 'community';

export interface TierFeatures {
  vpn: boolean;
  trusted_devices: boolean;
  licensed_updates: boolean;
  gmail_categories_purge: boolean;
  server_retention: boolean;
  auto_archive: boolean;
  mail_import: boolean;
  ai: boolean;
}

export interface TierInfo {
  tier: BuildTier;
  max_mailboxes: number | null;
  max_send_delay_secs: number | null;
  features: TierFeatures;
}

// ---- AI ------------------------------------------------------------

export type PrivacyPosture =
  | 'local_only'
  | 'user_controlled_remote'
  | 'third_party_cloud';

export interface AiStatus {
  enabled: boolean;
  /** Chat-side provider id + posture when enabled. */
  provider: string | null;
  privacy_posture: PrivacyPosture | null;
  /** Embed-side provider id + posture when enabled. Independent
   *  of chat — the recommended pairing keeps embed=Ollama (local)
   *  even when chat is hosted. */
  embed_provider: string | null;
  embed_privacy_posture: PrivacyPosture | null;
  embed_model: string;
  chat_model: string;
}

/** Provider kinds the user can pick in Settings → AI. */
export type AiProviderKind =
  | 'ollama'
  | 'anthropic'
  | 'openai'
  | 'openai_compat';

/** Provider kinds available for the embed slot. Anthropic is
 *  excluded — they don't offer an embeddings API. */
export type AiEmbedProviderKind = 'ollama' | 'openai' | 'openai_compat';

/** Persisted AI configuration as returned by `GET /api/ai/settings`.
 *  Chat and embed providers are now independent — a common pairing
 *  is chat=openai + embed=ollama (best chat quality + every email
 *  body stays local during indexing). */
export interface AiSettingsDto {
  enabled: boolean;
  provider_kind: AiProviderKind;
  chat_model: string;
  embed_model: string;
  base_url: string | null;
  /** True when a chat-side key is on file. The plaintext is never returned. */
  api_key_set: boolean;
  embed_provider_kind: AiEmbedProviderKind;
  embed_base_url: string | null;
  /** True when an embed-side key is on file. Distinct from
   *  api_key_set: when chat and embed providers match, the chat
   *  key is reused and this stays false. */
  embed_api_key_set: boolean;
  cloud_consent: boolean;
  /** "Always on" — AI providers rebuild automatically after the
   *  vault unlocks post-restart / post-update. Defaults true. */
  auto_start: boolean;
  /** User-defined additional rules appended to the prompt after
   *  the seven Commandments. Echoed in full so the form can
   *  pre-populate. */
  user_rules: string | null;
  /** Newline-delimited sender exclusion patterns. `*` = wildcard.
   *  Lines starting with `#` are treated as comments. */
  excluded_senders: string | null;
  /** Newline-delimited label exclusion list (exact match). */
  excluded_labels: string | null;
  /** Optional model override used by the compose-pane "Polish"
   *  rewrite. Empty / null = inherit chat_model. Provider stays
   *  whatever provider_kind is configured to. */
  polish_chat_model: string | null;
  /** Datas response-freedom mode. NULL or unknown rendered as
   *  'balanced' by the UI. */
  freedom_mode: string | null;
  /** Per-request output-token cap for Ask Datas. NULL = use the
   *  in-code default (~2000). */
  chat_max_tokens: number | null;
  updated_at: number;
}

export type FreedomMode = 'strict' | 'balanced' | 'open';

/** One Commandment as returned by GET /api/ai/commandments. */
export interface AiCommandment {
  n: number;
  title: string;
  body: string;
}

export interface AiCommandmentsResponse {
  /** The seven non-negotiable rules. Read-only — these are the
   *  security floor that ships with the build. */
  commandments: AiCommandment[];
  /** User-editable extension. Plain text. */
  user_rules: string | null;
  /** Server-assembled system prompt — exactly what gets sent
   *  to the model. Useful for "what is Datas being told?". */
  rendered_prompt: string;
}

/** Body for `POST /api/ai/settings`. `api_key` / `embed_api_key`:
 *  omitted/null = leave existing, "" = clear, non-empty = rotate. */
export interface AiSettingsUpdate {
  enabled: boolean;
  provider_kind: AiProviderKind;
  chat_model?: string;
  embed_model?: string;
  base_url?: string | null;
  api_key?: string | null;
  embed_provider_kind: AiEmbedProviderKind;
  embed_base_url?: string | null;
  embed_api_key?: string | null;
  cloud_consent: boolean;
  /** Auto-start preference. Defaults true server-side when omitted. */
  auto_start?: boolean;
  /** Additional user rules appended after the Commandments.
   *  Three states:
   *   * omitted/undefined — preserve the existing value
   *   * "" — clear
   *   * any non-empty string — replace */
  user_rules?: string | null;
  /** Same three-state semantics as user_rules. */
  excluded_senders?: string | null;
  excluded_labels?: string | null;
  /** Polish-feature chat-model override. Same three-state
   *  semantics. "" clears it (inherit chat_model again). */
  polish_chat_model?: string | null;
  /** Datas freedom mode. Same three-state semantics. Valid
   *  non-empty values: 'strict' | 'balanced' | 'open'. */
  freedom_mode?: FreedomMode | null;
  /** Output-token cap for Ask Datas. omitted = leave; <=0 =
   *  clear back to default; >0 = stored (server clamps to
   *  256..=16384). */
  chat_max_tokens?: number | null;
}

/** Result of `GET /api/ai/models` — what the active chat
 *  provider has installed/available. Drives the Polish-model
 *  dropdown in Settings → AI. `error` is set when the provider
 *  refused (auth failure, network down) — UI surfaces it as a
 *  hint rather than crashing the form. */
export interface AiModelsResponse {
  provider: string;
  models: string[];
  error: string | null;
}

export interface AiSettingsTest {
  provider_kind: AiProviderKind;
  chat_model?: string;
  embed_model?: string;
  base_url?: string | null;
  api_key?: string | null;
}

export interface AiSettingsTestResult {
  ok: boolean;
  provider: string;
  privacy_posture: string;
  message: string | null;
}

export interface AiCoverage {
  embed_model: string;
  indexed: number;
  total: number;
  chat_history_count: number;
}

export interface AiAskRequest {
  question: string;
  account_id?: number | null;
  chat_model?: string;
  embed_model?: string;
}

export interface AiCitation {
  message_id: number;
  subject: string | null;
  from_addr: string | null;
  date_utc: number;
  score: number;
}

export interface AiAskResponse {
  answer: string;
  citations: AiCitation[];
  privacy_posture: PrivacyPosture;
  elapsed_ms: number;
}

export type AiRewriteTone = 'professional' | 'concise' | 'friendly';

export interface AiRewriteRequest {
  text: string;
  tone?: AiRewriteTone;
}

export interface AiRewriteResponse {
  rewritten: string;
  provider: string;
  chat_model: string;
  privacy_posture: PrivacyPosture;
  elapsed_ms: number;
  prompt_tokens: number;
  completion_tokens: number;
}

/** One row from the AI activity log table (per-call detail).
 *  Listing rows omit the truncated payload samples — fetched
 *  separately via `aiActivityDetail`. */
export interface AiActivityRow {
  id: number;
  ts_utc: number;
  /** 'chat' | 'chat_stream' | 'embed' | 'health' (health is rare —
   *  the decorator skips logging health probes). */
  kind: string;
  /** LlmProvider::id() string — 'openai' / 'ollama' / 'anthropic' /
   *  'openai_compat'. */
  provider: string;
  model: string;
  /** 'ok' | 'error' */
  status: string;
  elapsed_ms: number;
  prompt_tokens: number;
  completion_tokens: number;
  /** Pre-truncation byte size of the request payload. */
  input_bytes: number;
  output_bytes: number;
  error_message: string | null;
}

export interface AiActivityDetail extends AiActivityRow {
  /** First 4 KB of the JSON-serialized request. */
  request_sample: string | null;
  /** First 4 KB of the response (or full body for streaming chats). */
  response_sample: string | null;
}

/** Aggregated counts + cost rates for the Activity summary strip.
 *  Each bucket is one (provider, kind, model) combination over the
 *  selected window. Multiply token columns by the matching
 *  rate-per-1M to get USD estimates. */
export interface AiActivitySummary {
  window: 'hour' | 'day' | 'month';
  since_ts_utc: number;
  buckets: {
    provider: string;
    kind: string;
    model: string;
    calls: number;
    sum_elapsed_ms: number;
    sum_prompt_tokens: number;
    sum_completion_tokens: number;
    errors: number;
  }[];
  /** Cost-per-1M-token rates for each (provider, model) pair.
   *  Hardcoded server-side — bumped per Postern release as
   *  vendor pricing changes. `null` fields mean we don't have
   *  a published rate for that model. */
  rates: {
    provider: string;
    model: string;
    prompt_per_1m_usd: number | null;
    completion_per_1m_usd: number | null;
  }[];
}

export interface AiHistoryEntry {
  id: number;
  created_at: number;
  question: string;
  answer: string;
  provider: string;
  chat_model: string;
  privacy_posture: string;
  cited_message_ids: number[];
}
