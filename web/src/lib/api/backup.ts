/** Backup + restore + off-site destinations slice. Big domain —
 *  list/create/delete/download local backups, manage SFTP/GDrive
 *  destinations, schedule, and the three-step restore flow plus
 *  cancel. */

import { request } from './_client';
import type {
  BackupDestination,
  BackupJob,
  BackupReport,
  BackupSchedule,
  CloudBackup,
  Integrations,
  NewSftpDestination,
  RestoreUploadResult,
  RestoreValidation
} from '../api';

export const backupApi = {
  listBackups: () => request<BackupReport[]>('/api/backups'),
  createBackup: () => request<BackupJob>('/api/backups/create', { method: 'POST' }),
  getBackupStatus: () => request<BackupJob | null>('/api/backups/status'),
  deleteBackup: (filename: string) =>
    request<{ deleted: string }>(
      `/api/backups/${encodeURIComponent(filename)}`,
      { method: 'DELETE' }
    ),
  /** Returns the URL the browser should navigate to in order to
   *  download the backup. Plain href; the server sets
   *  Content-Disposition: attachment so it Saves rather than displays. */
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
    request<{ deleted: number }>(`/api/backups/destinations/${id}`, {
      method: 'DELETE'
    }),
  /** Response shape varies by destination kind:
   *    sftp   → { ok, fingerprint, first_use }
   *    gdrive → { ok, account_email, folder_name }
   *  All optional so callers can branch on what's present. */
  testBackupDestination: (id: number) =>
    request<{
      ok: boolean;
      fingerprint?: string;
      first_use?: boolean;
      account_email?: string;
      folder_name?: string;
    }>(`/api/backups/destinations/${id}/test`, { method: 'POST' }),
  forgetBackupDestinationFingerprint: (id: number) =>
    request<BackupDestination>(
      `/api/backups/destinations/${id}/forget-fingerprint`,
      { method: 'POST' }
    ),
  backupIntegrations: () => request<Integrations>('/api/backups/integrations'),
  getBackupSchedule: () => request<BackupSchedule>('/api/backups/schedule'),
  setBackupSchedule: (body: Omit<BackupSchedule, 'last_run_at'>) =>
    request<BackupSchedule>('/api/backups/schedule', {
      method: 'POST',
      body: JSON.stringify(body)
    }),
  /** URL the browser navigates to to start the Google Drive OAuth
   *  flow. Server stashes the label, redirects to Google's consent
   *  screen, then redirects back to /settings/backups. */
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
  /** List the backups already in a Google Drive destination's folder. */
  listCloudBackups: (destinationId: number) =>
    request<CloudBackup[]>(`/api/backups/destinations/${destinationId}/cloud-backups`),
  /** Stage a Drive backup server-side (no browser download); returns a
   *  staging_id the normal validate/apply flow consumes. The request
   *  stays open for the whole download, which can take minutes on a
   *  multi-GB tarball. */
  restoreFromGdrive: (destinationId: number, fileId: string) =>
    request<RestoreUploadResult>('/api/backups/restore/from-gdrive', {
      method: 'POST',
      body: JSON.stringify({ destination_id: destinationId, file_id: fileId })
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
    )
};
