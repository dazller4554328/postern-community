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
  /// Per-account display colour as a `#rrggbb` hex string. Drives
  /// the unread-indicator pill in the inbox row. `null` = client
  /// computes a deterministic default from `id`.
  color: string | null;
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
  /// Parent Message-ID parsed from the blob's In-Reply-To header.
  /// Used by compose-reply to build a standards-compliant References
  /// chain so receiving clients (Gmail, Outlook) still thread the
  /// conversation correctly.
  in_reply_to: string | null;
  /// Full References header from the blob, oldest-first per RFC 5322.
  references: string[];
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
  /** Per-account display colour. `null` = client picks a default. */
  color: string | null;
  system: FolderEntry[];
  categories: FolderEntry[];
  user: FolderEntry[];
  categories_missing: string[];
  include_in_unified: boolean;
}

export interface FoldersResponse {
  accounts: AccountFolders[];
}

import { request, uploadAttachment } from './api/_client';
import { accountsApi } from './api/accounts';
import { aiApi } from './api/ai';
import { auditApi } from './api/audit';
import { backupApi } from './api/backup';
import { calendarApi } from './api/calendar';
import { contactsApi } from './api/contacts';
import { foldersApi } from './api/folders';
import { importApi } from './api/import';
import { messagesApi } from './api/messages';
import { notesApi } from './api/notes';
import { outboxApi } from './api/outbox';
import { pgpApi } from './api/pgp';
import { remindersApi } from './api/reminders';
import { rulesApi } from './api/rules';
import { searchApi } from './api/search';
import { sendApi } from './api/send';
import { totpApi } from './api/totp';
import { trustedDevicesApi } from './api/trustedDevices';
import { trustedSendersApi } from './api/trustedSenders';
import { updatesApi } from './api/updates';
import { vaultApi } from './api/vault';
import { vpnApi } from './api/vpn';

export const api = {
  ...accountsApi,
  ...aiApi,
  ...auditApi,
  ...backupApi,
  ...calendarApi,
  ...contactsApi,
  ...foldersApi,
  ...importApi,
  ...messagesApi,
  ...notesApi,
  ...outboxApi,
  ...pgpApi,
  ...remindersApi,
  ...rulesApi,
  ...searchApi,
  ...sendApi,
  ...totpApi,
  ...trustedDevicesApi,
  ...trustedSendersApi,
  ...updatesApi,
  ...vaultApi,
  ...vpnApi,
  // Build-tier surface. Cached on first call — compile-time constants
  // server-side, no need to re-fetch during a session.
  tier: () => request<TierInfo>('/api/tier'),

  viewerSandboxStatus: () =>
    request<{ viewer_available: boolean }>('/api/viewer-sandbox/status'),

};

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
  received_chain: {
    from: string | null;
    by: string | null;
    with: string | null;
    ip: string | null;
    country_code: string | null;
    country_name: string | null;
    raw: string;
  }[];
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

export interface TrustedSender {
  id: number;
  account_id: number;
  email_lower: string;
  created_at: number;
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

/** A backup tarball already present in a Google Drive destination's
 *  folder — listed so the user can restore one without downloading it
 *  to their browser first. */
export interface CloudBackup {
  file_id: string;
  name: string;
  modified_time: string;
  size: number | null;
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
  /** `'caldav'` for accounts synced from a remote server, `'local'`
   *  for purely on-device calendars. The server bootstraps a single
   *  `'local'` account on first calendar load so the UI always has
   *  somewhere to write new events. */
  kind: 'caldav' | 'local';
  label: string;
  server_url: string | null;
  username: string | null;
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

export interface NewLocalEvent {
  /** Optional — server falls back to the default local calendar. */
  calendar_id?: number;
  summary?: string | null;
  description?: string | null;
  location?: string | null;
  dtstart_utc: number;
  dtend_utc?: number | null;
  all_day?: boolean;
  rrule?: string | null;
}

export type PatchLocalEvent = Partial<NewLocalEvent>;

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

export interface Note {
  id: number;
  title: string;
  body: string;
  pinned: boolean;
  created_at: number;
  updated_at: number;
}

export interface NewNote {
  title?: string;
  body?: string;
  pinned?: boolean;
}

/** Patch body for PATCH /api/notes/:id. All fields optional. */
export interface UpdateNote {
  title?: string;
  body?: string;
  pinned?: boolean;
}

/** One snapshot of a note's prior title+body, captured by the auto-save
 *  layer when the content actually changed and the previous snapshot
 *  was at least 30 s old. Returned newest-first. */
export interface NoteRevision {
  id: number;
  note_id: number;
  title: string;
  body: string;
  created_at: number;
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

export interface LicenseActivateResult {
  ok: boolean;
  /** `activated`, `needs_transfer_confirm`, `unknown`, `malformed`,
   *  `inactive`, `terminated` … */
  status: string;
  tier: string | null;
  message: string | null;
  /** Present when status === `needs_transfer_confirm` — masked
   *  fingerprint of the previously-bound install. */
  bound_install_masked: string | null;
  /** ISO-8601 week of the previous install's last check, e.g.
   *  `"2026-W18"`. Coarse on purpose. */
  last_seen_week: string | null;
  update_window_until: number | null;
}

export interface LicenseReleaseResult {
  ok: boolean;
  status: string;
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
  /** Lifetime install / time-bounded updates: unix timestamp at
   *  which the buyer's update entitlement lapses. `null` = no expiry. */
  update_window_until: number | null;
  tier: string | null;
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
  provider: string | null;
  privacy_posture: PrivacyPosture | null;
  chat_model: string;
}

/** Provider kinds the user can pick in Settings → AI. */
export type AiProviderKind =
  | 'ollama'
  | 'anthropic'
  | 'openai'
  | 'openai_compat';

/** Persisted AI configuration as returned by `GET /api/ai/settings`. */
export interface AiSettingsDto {
  enabled: boolean;
  provider_kind: AiProviderKind;
  chat_model: string;
  base_url: string | null;
  /** True when a key is on file. The plaintext is never returned. */
  api_key_set: boolean;
  cloud_consent: boolean;
  /** "Always on" — AI providers rebuild automatically after the
   *  vault unlocks post-restart / post-update. Defaults true. */
  auto_start: boolean;
  /** Optional model override used by the compose-pane "Polish"
   *  rewrite. Empty / null = inherit chat_model. */
  polish_chat_model: string | null;
  updated_at: number;
}

/** Body for `POST /api/ai/settings`. `api_key`:
 *  omitted/null = leave existing, "" = clear, non-empty = rotate. */
export interface AiSettingsUpdate {
  enabled: boolean;
  provider_kind: AiProviderKind;
  chat_model?: string;
  base_url?: string | null;
  api_key?: string | null;
  cloud_consent: boolean;
  auto_start?: boolean;
  /** Polish-feature chat-model override. Same three-state
   *  semantics: omitted = leave; "" = clear; non-empty = replace. */
  polish_chat_model?: string | null;
}

/** Result of `GET /api/ai/models` — what the active chat
 *  provider has installed/available. Drives the Polish-model
 *  dropdown in Settings → AI. */
export interface AiModelsResponse {
  provider: string;
  models: string[];
  error: string | null;
}

export interface AiSettingsTest {
  provider_kind: AiProviderKind;
  chat_model?: string;
  base_url?: string | null;
  api_key?: string | null;
}

export interface AiSettingsTestResult {
  ok: boolean;
  provider: string;
  privacy_posture: string;
  message: string | null;
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
