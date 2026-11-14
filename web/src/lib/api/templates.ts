/** Email templates slice — reusable subject + body snippets for the
 *  compose form. Managed from Settings → Templates. */

import { request } from './_client';
import type { EmailTemplate, NewTemplate, UpdateTemplate } from '../api';

export const templatesApi = {
  templatesList: () => request<EmailTemplate[]>('/api/templates'),
  templatesCreate: (body: NewTemplate) =>
    request<EmailTemplate>('/api/templates', {
      method: 'POST',
      body: JSON.stringify(body)
    }),
  templatesUpdate: (id: number, patch: UpdateTemplate) =>
    request<EmailTemplate>(`/api/templates/${id}`, {
      method: 'PATCH',
      body: JSON.stringify(patch)
    }),
  templatesDelete: (id: number) =>
    request<{ deleted: number }>(`/api/templates/${id}`, { method: 'DELETE' })
};
