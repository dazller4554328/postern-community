/** Send slice — compose pane + read-receipt action + per-account
 *  signatures. */

import { request } from './_client';
import type { SendEnqueueResponse, SendRequest } from '../api';

export const sendApi = {
  sendMessage: (req: SendRequest) =>
    request<SendEnqueueResponse>('/api/send', {
      method: 'POST',
      body: JSON.stringify(req)
    }),

  /** Manually send a read-receipt MDN for a message that has a
   *  non-null `receipt_to`. Postern never auto-sends — the user
   *  must click "Send receipt" on the banner. Idempotency-light:
   *  the server only refuses if the address is missing. */
  sendReadReceipt: (id: number) =>
    request<{ id: number; sent_to: string }>(
      `/api/messages/${id}/send-receipt`,
      { method: 'POST' }
    ),

  setSignature: (account_id: number, html: string | null, plain: string | null) =>
    request<{ id: number; signature_html: string | null; signature_plain: string | null }>(
      `/api/accounts/${account_id}/signature`,
      { method: 'POST', body: JSON.stringify({ html, plain }) }
    )
};
