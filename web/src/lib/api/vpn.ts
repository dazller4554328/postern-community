/** VPN slice — pro-only on the backend; routes 404 on free builds. */

import { request } from './_client';
import type { NordCountry, VpnStatus } from '../api';

export const vpnApi = {
  vpnStatus: () => request<VpnStatus>('/api/vpn'),
  vpnInstall: (
    body:
      | { provider: 'manual_wireguard'; wg_config: string; region_label?: string; killswitch?: boolean }
      | { provider: 'proton_wireguard'; wg_config: string; killswitch?: boolean }
      | { provider: 'nordlynx'; token: string; country_id?: number; killswitch?: boolean }
  ) =>
    request<VpnStatus>('/api/vpn/install', {
      method: 'POST',
      body: JSON.stringify(body)
    }),
  vpnUninstall: () => request<VpnStatus>('/api/vpn', { method: 'DELETE' }),
  vpnHealthcheck: () => request<VpnStatus>('/api/vpn/healthcheck', { method: 'POST' }),
  vpnRefresh: () => request<VpnStatus>('/api/vpn/refresh', { method: 'POST' }),
  vpnKillswitch: (enabled: boolean) =>
    request<VpnStatus>('/api/vpn/killswitch', {
      method: 'POST',
      body: JSON.stringify({ enabled })
    }),
  vpnNordCountries: () => request<NordCountry[]>('/api/vpn/nord/countries')
};
