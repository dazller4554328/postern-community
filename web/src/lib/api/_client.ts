/**
 * Shared fetch helpers used by every api/* domain slice.
 *
 * Both helpers decode the server's `{ error: "…" }` envelope on
 * non-2xx so callers see the friendly message rather than just
 * "500 Internal Server Error". Keep this contract in sync with the
 * Rust `error::Error` axum responder.
 */

/**
 * Thrown when the server tells us the current session is gone —
 * either the vault is locked, the session cookie is missing /
 * invalid, or the per-device idle timer fired. The vault-gate
 * subscriber flips the UI back to the unlock screen when this
 * surfaces, so callers usually don't need to do anything beyond
 * letting the error propagate (or swallowing it if they're a
 * background poll that doesn't want to spam the user).
 */
export class AuthExpiredError extends Error {
  constructor(public status: number, message: string) {
    super(message);
    this.name = 'AuthExpiredError';
  }
}

// Callback registered by `vault.ts` at module load. Kept as a private
// var (instead of importing the store directly) so this file has no
// circular dependency on the vault module — `vault.ts` already
// imports `./api` which transitively imports this file.
let authExpiredHandler: (() => void) | null = null;
export function setAuthExpiredHandler(fn: () => void): void {
  authExpiredHandler = fn;
}

function isAuthExpiredStatus(status: number): boolean {
  // 401: vault-lock guard, session guard, IP-change auto-lock — all
  //      the server's "you don't have a live session" responses.
  // 423: Locked. The handlers' own `require_unlocked()` check fires
  //      this code path; rarer than 401 but the same user intent
  //      (re-authenticate before continuing).
  return status === 401 || status === 423;
}

export async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(path, {
    ...init,
    headers: {
      'content-type': 'application/json',
      ...(init?.headers ?? {})
    }
  });
  if (!res.ok) {
    let msg = `${res.status} ${res.statusText}`;
    try {
      const body = await res.json();
      if (body?.error) msg = body.error;
    } catch {
      /* empty */
    }
    if (isAuthExpiredStatus(res.status)) {
      // Fire the gate flip before the throw so the UI re-renders
      // even if the caller silently swallows the error (background
      // polls and the like).
      authExpiredHandler?.();
      throw new AuthExpiredError(res.status, msg);
    }
    throw new Error(msg);
  }
  // 204 No Content + empty 200 bodies (some endpoints return just a
  // status code) would blow up `res.json()` with "Unexpected end of
  // JSON input". Callers that pass `<unknown>` or `<void>` don't
  // need a real value — return undefined cast as T.
  if (res.status === 204 || res.headers.get('content-length') === '0') {
    return undefined as T;
  }
  return res.json() as Promise<T>;
}

/**
 * POST a raw binary upload with a custom content-type (no JSON body).
 * Shares the error-envelope decode with {@link request} so import
 * endpoints surface the same friendly 4xx messages.
 */
export async function uploadAttachment<T>(
  path: string,
  body: Blob,
  contentType: string
): Promise<T> {
  const res = await fetch(path, {
    method: 'POST',
    headers: { 'content-type': contentType },
    body
  });
  if (!res.ok) {
    let msg = `${res.status} ${res.statusText}`;
    try {
      const parsed = await res.json();
      if (parsed?.error) msg = parsed.error;
    } catch {
      /* not JSON — fall back to the status line */
    }
    if (isAuthExpiredStatus(res.status)) {
      authExpiredHandler?.();
      throw new AuthExpiredError(res.status, msg);
    }
    throw new Error(msg);
  }
  return (await res.json()) as T;
}
