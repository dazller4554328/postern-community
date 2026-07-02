/** Reminders slice — local, never leaves the device. */

import { request } from './_client';
import type { NewReminder, Reminder, SnoozeUntil, UpdateReminder } from '../api';

export const remindersApi = {
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
    request<{ deleted: number }>(`/api/reminders/${id}`, { method: 'DELETE' }),
  remindersMarkDone: (id: number) =>
    request<Reminder>(`/api/reminders/${id}/done`, { method: 'POST' }),
  remindersSnooze: (id: number, until: SnoozeUntil) =>
    request<Reminder>(`/api/reminders/${id}/snooze`, {
      method: 'POST',
      body: JSON.stringify({ until })
    })
};
