/** Updates + license slice. License CRUD drives Settings → Updates;
 *  the update endpoints drive the in-app installer flow. */

import { request } from './_client';
import type {
  LicenseActivateResult,
  LicenseInfo,
  LicenseReleaseResult,
  LicenseVerifyResult,
  UpdateCheckResult,
  UpdateStatusResult
} from '../api';

export const updatesApi = {
  licenseGet: () => request<LicenseInfo>('/api/license'),
  licenseSet: (license_key: string | null) =>
    request<LicenseInfo>('/api/license', {
      method: 'POST',
      body: JSON.stringify({ license_key })
    }),
  licenseVerify: () =>
    request<LicenseVerifyResult>('/api/license/verify', { method: 'POST' }),
  licenseActivate: (confirm_transfer = false) =>
    request<LicenseActivateResult>('/api/license/activate', {
      method: 'POST',
      body: JSON.stringify({ confirm_transfer })
    }),
  licenseRelease: () =>
    request<LicenseReleaseResult>('/api/license/release', { method: 'POST' }),
  updatesVersion: () => request<{ commit: string }>('/api/updates/version'),
  updatesCheck: () =>
    request<UpdateCheckResult>('/api/updates/check', { method: 'POST' }),
  updatesApply: () =>
    request<{ queued: boolean; message: string }>('/api/updates/apply', {
      method: 'POST'
    }),
  updatesStatus: () => request<UpdateStatusResult>('/api/updates/status')
};
