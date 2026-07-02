/** Calendar slice — CalDAV accounts + events + local-only events. */

import { request } from './_client';
import type {
  CalAccount,
  CalCalendar,
  CalEvent,
  CalSyncReport,
  EventOccurrence,
  NewCalAccount,
  NewLocalEvent,
  PatchLocalEvent
} from '../api';

export const calendarApi = {
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
  calCreateEvent: (body: NewLocalEvent) =>
    request<CalEvent>('/api/cal/events', {
      method: 'POST',
      body: JSON.stringify(body)
    }),
  calUpdateEvent: (id: number, body: PatchLocalEvent) =>
    request<CalEvent>(`/api/cal/events/${id}`, {
      method: 'PATCH',
      body: JSON.stringify(body)
    }),
  calDeleteEvent: (id: number) =>
    request<{ deleted: number; removed: boolean }>(
      `/api/cal/events/${id}`,
      { method: 'DELETE' }
    )
};
