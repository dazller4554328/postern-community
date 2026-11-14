/** Outbox slice — drives the Outbox page + the compose-pane
 *  send-with-undo flow. */

import { request } from './_client';
import type { OutboxEntry, OutboxListItem } from '../api';

export const outboxApi = {
  outboxList: () => request<OutboxListItem[]>('/api/outbox'),
  outboxRecentFailures: () => request<OutboxListItem[]>('/api/outbox/recent-failures'),
  outboxClearFailures: () =>
    request<{ removed: number }>('/api/outbox/recent-failures', { method: 'DELETE' }),
  outboxGet: (id: number) => request<OutboxEntry>(`/api/outbox/${id}`),
  outboxCancel: (id: number) =>
    request<{ cancelled: number }>(`/api/outbox/${id}`, { method: 'DELETE' }),
  outboxReschedule: (id: number, scheduled_at: number) =>
    request<{ id: number; scheduled_at: number }>(
      `/api/outbox/${id}/reschedule`,
      { method: 'POST', body: JSON.stringify({ scheduled_at }) }
    )
};
