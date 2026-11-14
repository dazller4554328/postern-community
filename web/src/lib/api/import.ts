/** Import slice — mbox upload, archive zip, server-side maildir
 *  walk. All three deduplicate by Message-ID, so re-running is
 *  safe; a partial re-run picks up where the last left off. */

import { request, uploadAttachment } from './_client';
import type { ImportReport } from '../api';

export const importApi = {
  importMbox: (file: Blob, accountId?: number) =>
    uploadAttachment<ImportReport>(
      accountId != null ? `/api/import/mbox?account_id=${accountId}` : '/api/import/mbox',
      file,
      'application/octet-stream'
    ),
  importArchiveZip: (file: Blob, accountId?: number) =>
    uploadAttachment<ImportReport>(
      accountId != null
        ? `/api/import/archive-zip?account_id=${accountId}`
        : '/api/import/archive-zip',
      file,
      'application/zip'
    ),
  /** Path-based Maildir import. Walks `<POSTERN_IMPORT_DIR>/<path>`
   *  (server-side, sandboxed to the configured import root) and
   *  upserts every RFC822 file it finds. Dedups by Message-ID — safe
   *  to re-run. */
  importMaildirPath: (path: string, accountId?: number) =>
    request<ImportReport>('/api/import/maildir', {
      method: 'POST',
      body: JSON.stringify({
        path,
        ...(accountId != null ? { account_id: accountId } : {})
      })
    })
};
