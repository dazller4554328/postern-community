/** Vault slice — init / unlock / lock / change-password. The vault
 *  controls SQLCipher access; locked = encrypted DB closed. */

import { request } from './_client';
import type { VaultState } from '../api';

export const vaultApi = {
  vaultStatus: () =>
    request<{ state: VaultState; trusted_device: boolean }>('/api/vault/status'),
  vaultInit: (password: string) =>
    request<{ state: VaultState }>('/api/vault/init', {
      method: 'POST',
      body: JSON.stringify({ password })
    }),
  vaultUnlock: (
    password: string,
    rememberDevice = false,
    opts: { totpCode?: string; recoveryCode?: string } = {}
  ) =>
    request<{ state: VaultState }>('/api/vault/unlock', {
      method: 'POST',
      body: JSON.stringify({
        password,
        remember_device: rememberDevice,
        totp_code: opts.totpCode || undefined,
        recovery_code: opts.recoveryCode || undefined
      })
    }),
  vaultLock: () =>
    request<{ state: VaultState }>('/api/vault/lock', { method: 'POST' }),
  vaultChangePassword: (oldPassword: string, newPassword: string) =>
    request<{ state: VaultState }>('/api/vault/change-password', {
      method: 'POST',
      body: JSON.stringify({ old_password: oldPassword, new_password: newPassword })
    })
};
