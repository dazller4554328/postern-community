/** PGP slice — keys, can-encrypt probe, discovery, keyserver, export. */

import { request } from './_client';
import type {
  PgpDiscovery,
  PgpKey,
  PgpKeyserverStatus,
  PgpPublishResult
} from '../api';

export const pgpApi = {
  pgpKeys: () => request<PgpKey[]>('/api/pgp/keys'),
  pgpGenerate: (user_id: string) =>
    request<PgpKey>('/api/pgp/keys/generate', {
      method: 'POST',
      body: JSON.stringify({ user_id })
    }),
  pgpImport: (armored: string, passphrase?: string) =>
    request<PgpKey>('/api/pgp/keys', {
      method: 'POST',
      body: JSON.stringify(passphrase ? { armored, passphrase } : { armored })
    }),
  pgpDelete: (id: number) =>
    request<{ deleted: number }>(`/api/pgp/keys/${id}`, { method: 'DELETE' }),
  pgpExport: (id: number) =>
    request<{ armored: string }>(`/api/pgp/keys/${id}/export`),
  /**
   * Check whether recipients can be encrypted to. `discover=false`
   * (default) consults only the local keyring — no network — and is
   * what the compose form calls on every keystroke. Pass `discover=true`
   * only on explicit intent (user enabled encryption / hit Send), since
   * WKD + keyserver lookups reveal the recipient address to a third
   * party.
   */
  pgpCanEncrypt: (emails: string[], discover = false) =>
    request<{ can_encrypt: boolean; missing: string[]; imported: string[] }>(
      `/api/pgp/can-encrypt?emails=${encodeURIComponent(emails.join(','))}${
        discover ? '&discover=1' : ''
      }`
    ),
  pgpDiscover: (email: string) =>
    request<PgpDiscovery>(`/api/pgp/discover?email=${encodeURIComponent(email)}`),
  pgpPublish: (id: number) =>
    request<PgpPublishResult>(`/api/pgp/keys/${id}/publish`, { method: 'POST' }),
  pgpKeyserverScan: () => request<PgpKeyserverStatus[]>('/api/pgp/keyserver-scan'),
  /** Returns the raw .asc file as a Blob (so the browser can trigger
   *  a download). `includeSecret` controls whether private key blocks
   *  are bundled — UI should confirm before passing true. */
  pgpExportAll: async (includeSecret: boolean): Promise<Blob> => {
    const url = `/api/pgp/keys/export-all${includeSecret ? '?include_secret=true' : ''}`;
    const res = await fetch(url, { credentials: 'include' });
    if (!res.ok) {
      const body = await res.text().catch(() => '');
      throw new Error(body || `Export failed: ${res.status}`);
    }
    return res.blob();
  }
};
