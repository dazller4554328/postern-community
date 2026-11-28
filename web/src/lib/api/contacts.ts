/**
 * Contacts CRUD + autocomplete slice.
 *
 * Demonstrates the per-domain split pattern requested in the
 * refactor plan — sibling api/<domain>.ts files export per-slice
 * objects that the top-level api.ts spreads into the unified
 * `api` const. New code can also import a slice directly:
 *
 *   import { contactsApi } from '$lib/api/contacts';
 *
 * Existing code keeps using `import { api } from '$lib/api'`.
 */

import { request } from './_client';
import type { Contact } from '../api';

export const contactsApi = {
  autocomplete: (q: string) =>
    request<string[]>(`/api/contacts/autocomplete?q=${encodeURIComponent(q)}`),

  // Contacts CRUD — drives the /contacts page and (later) any
  // import/export tooling. The autocomplete endpoint above also
  // backs onto the same table, so anything done here is immediately
  // visible in the compose recipient field.
  listContacts: (params: { q?: string; limit?: number; offset?: number } = {}) => {
    const qs = new URLSearchParams();
    if (params.q) qs.set('q', params.q);
    if (params.limit != null) qs.set('limit', String(params.limit));
    if (params.offset != null) qs.set('offset', String(params.offset));
    const tail = qs.toString();
    return request<{ contacts: Contact[]; total: number }>(
      `/api/contacts${tail ? `?${tail}` : ''}`
    );
  },
  getContact: (id: number) => request<Contact>(`/api/contacts/${id}`),
  createContact: (body: {
    address: string;
    display_name?: string;
    notes?: string;
    is_favorite?: boolean;
  }) =>
    request<Contact>('/api/contacts', {
      method: 'POST',
      body: JSON.stringify(body)
    }),
  updateContact: (
    id: number,
    body: {
      // Use null to explicitly clear; omit the property to leave alone.
      display_name?: string | null;
      notes?: string | null;
      is_favorite?: boolean;
    }
  ) =>
    request<Contact>(`/api/contacts/${id}`, {
      method: 'PATCH',
      body: JSON.stringify(body)
    }),
  deleteContact: (id: number) =>
    request<unknown>(`/api/contacts/${id}`, { method: 'DELETE' }),
  /** Upload a photo for a contact. The file's MIME type is sent as
   *  Content-Type; backend validates it's image/* and clamps size.
   *  204 on success. */
  setContactPhoto: async (id: number, file: File) => {
    const r = await fetch(`/api/contacts/${id}/photo`, {
      method: 'PUT',
      headers: { 'Content-Type': file.type || 'application/octet-stream' },
      body: file
    });
    if (!r.ok) {
      const text = await r.text().catch(() => '');
      throw new Error(text || `setContactPhoto: HTTP ${r.status}`);
    }
  },
  clearContactPhoto: (id: number) =>
    request<unknown>(`/api/contacts/${id}/photo`, { method: 'DELETE' })
};
