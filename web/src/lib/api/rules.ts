/**
 * Rules slice — drives the Settings → Rules page and the
 * RuleQuickCreate component.
 */

import { request } from './_client';
import type { NewRule, Rule } from '../api';

export const rulesApi = {
  listRules: () => request<Rule[]>('/api/rules'),
  createRule: (r: NewRule) =>
    request<Rule>('/api/rules', { method: 'POST', body: JSON.stringify(r) }),
  deleteRule: (id: number) =>
    request<{ deleted: number }>(`/api/rules/${id}`, { method: 'DELETE' }),
  applyRules: () =>
    request<{ checked: number; acted: number }>('/api/rules/apply', { method: 'POST' }),
  toggleRule: (id: number, enabled: boolean) =>
    request<Rule>(`/api/rules/${id}/toggle`, {
      method: 'POST',
      body: JSON.stringify({ enabled })
    })
};
