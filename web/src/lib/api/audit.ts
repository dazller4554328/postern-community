/** Audit slice — Settings → Audit log. */

import { request } from './_client';
import type { AuditCategory, AuditEntry } from '../api';

export const auditApi = {
  auditLog: (params: { limit?: number; offset?: number; category?: AuditCategory } = {}) => {
    const q = new URLSearchParams();
    if (params.limit != null) q.set('limit', String(params.limit));
    if (params.offset != null) q.set('offset', String(params.offset));
    if (params.category) q.set('category', params.category);
    const qs = q.toString();
    return request<AuditEntry[]>(`/api/audit${qs ? `?${qs}` : ''}`);
  }
};
