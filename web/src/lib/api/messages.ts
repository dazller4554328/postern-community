/** Messages slice — listing, mark/move/spam/trash actions, bulk
 *  operations, and the URL-builder helpers (getRawUrl,
 *  attachmentUrl). The detail-fetch (getMessage) and per-message
 *  plain-text endpoint live here too. */

import { request } from './_client';
import type {
  Forensics,
  MessageDetail,
  MessageListItem
} from '../api';

export const messagesApi = {
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
  getMessagePlain: (id: number) =>
    request<{ text: string }>(`/api/messages/${id}/plain`),
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
  bulkAction: (
    ids: number[],
    action: 'trash' | 'archive' | 'read' | 'unread' | 'spam' | 'notspam'
  ) =>
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

  // ── URL helpers (return strings; not request<T> calls) ──
  getForensics: (id: number) =>
    request<Forensics>(`/api/messages/${id}/forensics`),
  getRawUrl: (id: number) => `/api/messages/${id}/raw`,
  /** `mode=inline` asks the server for a Content-Disposition: inline
   *  response if the attachment's MIME type is on the browser's
   *  safe-to-render whitelist — otherwise the server silently falls
   *  back to attachment/download. */
  attachmentUrl: (id: number, index: number, mode: 'inline' | 'download' = 'download') =>
    `/api/messages/${id}/attachment/${index}?mode=${mode}`
};
