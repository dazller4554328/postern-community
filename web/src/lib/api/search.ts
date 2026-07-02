/** Search slice — drives the sidebar/inbox keyword search. */

import { request } from './_client';
import type { SearchHit } from '../api';

export const searchApi = {
  search: (params: {
    q: string;
    account_id?: number;
    limit?: number;
    offset?: number;
    sort?: string;
    /** Default false — server hides Trash/Spam from results unless
     *  the caller is explicitly viewing unified Trash or Spam. */
    include_trash_spam?: boolean;
  }) => {
    const q = new URLSearchParams({ q: params.q });
    if (params.account_id != null) q.set('account_id', String(params.account_id));
    if (params.limit != null) q.set('limit', String(params.limit));
    if (params.offset != null) q.set('offset', String(params.offset));
    if (params.sort) q.set('sort', params.sort);
    if (params.include_trash_spam) q.set('include_trash_spam', 'true');
    return request<SearchHit[]>(`/api/search?${q.toString()}`);
  }
};
