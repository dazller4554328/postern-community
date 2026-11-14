export type AuditSeverity = 'normal' | 'warn' | 'elevated' | 'success';

const ICONS: Record<string, string> = {
  // Security
  vault_init: '🔐',
  vault_unlock: '🔓',
  vault_unlock_failed: '🚫',
  vault_lock: '🔒',
  ip_change_lock: '🌐',
  password_changed: '🔑',
  account_added: '📬',
  account_deleted: '🗑',
  rule_created: '📋',
  sync_interval_changed: '⏱',
  sync_policy_changed: '🔄',
  // Activity
  sync_started: '⟳',
  sync_completed: '✓',
  sync_error: '⚠',
  folder_sync_error: '⚠',
  smtp_send: '📤',
  smtp_error: '✗',
  imap_error: '✗',
};

export function icon(t: string): string {
  return ICONS[t] ?? '📝';
}

export function label(t: string): string {
  return t.replace(/_/g, ' ').replace(/\b\w/g, (c) => c.toUpperCase());
}

export function severity(t: string): AuditSeverity {
  if (t.includes('failed') || t.includes('error') || t.includes('ip_change')) return 'warn';
  if (t.includes('password') || t.includes('deleted')) return 'elevated';
  if (t === 'sync_completed' || t === 'smtp_send') return 'success';
  return 'normal';
}

export function severityLabel(t: string): string {
  const s = severity(t);
  if (s === 'warn') return 'Alert';
  if (s === 'elevated') return 'Sensitive';
  if (s === 'success') return 'OK';
  return 'Normal';
}

export interface HeroCopy {
  readonly eyebrow: string;
  readonly title: string;
  readonly body: string;
  readonly chips: readonly string[];
  readonly emptyHint: string;
  readonly tableSub: string;
}

export const HERO: { readonly security: HeroCopy; readonly activity: HeroCopy } = {
  security: {
    eyebrow: 'Event Ledger',
    title: 'Security Audit Log',
    body:
      'Review the local security trail for vault access, identity changes, mailbox mutations, and network anomalies.',
    chips: ['Local-only event history', 'Vault lifecycle tracking', 'IP anomaly visibility'],
    emptyHint:
      'No events logged yet. Entries will appear after vault or account security operations.',
    tableSub:
      'Newest entries first. Elevated rows highlight failed access and suspicious network changes.',
  },
  activity: {
    eyebrow: 'Server Activity',
    title: 'Sync & Send Activity',
    body:
      'See what the server is doing under the hood — mail sync cycles, messages fetched, send outcomes, and errors.',
    chips: ['Sync cycle timing', 'Send / receive outcomes', 'Live error surfacing'],
    emptyHint:
      'No activity recorded yet. Events will appear once the scheduler runs or you send a message.',
    tableSub: 'Newest first. Red rows are errors; green rows are successful sync/send operations.',
  },
} as const;
