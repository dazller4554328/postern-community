/** Secure notes slice — markdown body, vault-encrypted at rest. */

import { request } from './_client';
import type { NewNote, Note, NoteRevision, UpdateNote } from '../api';

export const notesApi = {
  notesList: () => request<Note[]>('/api/notes'),
  notesGet: (id: number) => request<Note>(`/api/notes/${id}`),
  notesCreate: (body: NewNote) =>
    request<Note>('/api/notes', { method: 'POST', body: JSON.stringify(body) }),
  notesUpdate: (id: number, patch: UpdateNote) =>
    request<Note>(`/api/notes/${id}`, {
      method: 'PATCH',
      body: JSON.stringify(patch)
    }),
  notesDelete: (id: number) =>
    request<{ deleted: number }>(`/api/notes/${id}`, { method: 'DELETE' }),
  notesListRevisions: (id: number) =>
    request<NoteRevision[]>(`/api/notes/${id}/revisions`),
  notesGetRevision: (id: number, revId: number) =>
    request<NoteRevision>(`/api/notes/${id}/revisions/${revId}`),
  notesRestoreRevision: (id: number, revId: number) =>
    request<Note>(`/api/notes/${id}/revisions/${revId}/restore`, {
      method: 'POST'
    })
};
