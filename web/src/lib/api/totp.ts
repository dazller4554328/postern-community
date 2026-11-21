/** TOTP (two-factor) slice — used at vault unlock. Status reachable
 *  while vault is locked so the unlock screen knows whether to show
 *  a 2FA field. */

import { request } from './_client';

type TotpStatus = {
  enabled: boolean;
  pending: boolean;
  recovery_codes_remaining: number;
};

export const totpApi = {
  authTotpStatus: () => request<TotpStatus>('/api/auth/totp/status'),
  authTotpInit: () =>
    request<{ secret: string; otpauth_url: string; qr_png_data_url: string }>(
      '/api/auth/totp/init',
      { method: 'POST' }
    ),
  authTotpConfirm: (code: string) =>
    request<{ enabled: boolean; recovery_codes: string[] }>(
      '/api/auth/totp/confirm',
      {
        method: 'POST',
        body: JSON.stringify({ code })
      }
    ),
  authTotpDisable: (opts: { code?: string; recoveryCode?: string }) =>
    request<TotpStatus>('/api/auth/totp/disable', {
      method: 'POST',
      body: JSON.stringify({
        code: opts.code || undefined,
        recovery_code: opts.recoveryCode || undefined
      })
    })
};
