/** Folders slice — CRUD on per-account IMAP folders. The folder
 *  *listing* (GET /api/folders) lives in api.ts proper since the
 *  inbox page is its primary consumer and the type stays there. */

import { request } from './_client';

export const foldersApi = {
  createFolder: (accountId: number, name: string) =>
    request<{ account_id: number; name: string }>(
      `/api/accounts/${accountId}/folders`,
      { method: 'POST', body: JSON.stringify({ name }) }
    ),
  renameFolder: (accountId: number, from: string, to: string) =>
    request<{ account_id: number; from: string; to: string; labels_renamed: number }>(
      `/api/accounts/${accountId}/folders/rename`,
      { method: 'POST', body: JSON.stringify({ from, to }) }
    ),
  deleteFolder: (accountId: number, name: string, force = false) => {
    const q = new URLSearchParams({ name });
    if (force) q.set('force', 'true');
    return request<{ account_id: number; name: string; labels_removed: number }>(
      `/api/accounts/${accountId}/folders?${q.toString()}`,
      { method: 'DELETE' }
    );
  }
};
