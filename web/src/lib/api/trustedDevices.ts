/** Trusted-devices slice — Settings → Security → Trusted devices.
 *  Pro-only on the backend; the routes are still mounted on the
 *  free build but return empty/404. */

import { request } from './_client';
import type { TrustedDevice } from '../api';

export const trustedDevicesApi = {
  listTrustedDevices: () => request<TrustedDevice[]>('/api/security/devices'),
  revokeTrustedDevice: (id: number) =>
    request<{ id: number; removed: boolean; self: boolean }>(
      `/api/security/devices/${id}`,
      { method: 'DELETE' }
    ),
  revokeAllTrustedDevices: () =>
    request<{ revoked: number }>('/api/security/devices/revoke-all', {
      method: 'POST'
    })
};
