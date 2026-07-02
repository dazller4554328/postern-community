/** Trusted senders slice — allowlist that survives "Not spam"
 *  actions. Sync auto-rescues any spam-folder mail whose sender is
 *  on this list. */

import { request } from './_client';
import type { TrustedSender } from '../api';

export const trustedSendersApi = {
  listTrustedSenders: () => request<TrustedSender[]>('/api/trusted-senders'),
  addTrustedSender: (account_id: number, email: string) =>
    request<{ account_id: number; email: string; inserted: boolean }>(
      '/api/trusted-senders',
      { method: 'POST', body: JSON.stringify({ account_id, email }) }
    ),
  deleteTrustedSender: (id: number) =>
    request<{ id: number; removed: boolean }>(
      `/api/trusted-senders/${id}`,
      { method: 'DELETE' }
    )
};
