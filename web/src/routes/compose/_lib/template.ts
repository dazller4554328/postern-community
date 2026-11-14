/** Template insertion for the compose body.
 *
 *  The body textarea may already hold an account signature (`-- \n…`)
 *  and, on replies/forwards, the quoted original. A template replaces
 *  only the "your text goes here" region above the first of those
 *  blocks; everything from that block down is preserved verbatim. */

/** Line that starts the preserved region: signature delimiter, the
 *  forward header, or the "On <date>, <sender> wrote:" reply header
 *  (both headers are produced by _lib/prefill). */
const PRESERVED_START = /^(-- \n|--- Forwarded message ---\n|On [^\n]+ wrote:\n)/m;

/** Split the body into [editable head, preserved tail]. The tail
 *  starts at the signature or quote header when one exists, else is
 *  empty. */
export function splitEditable(body: string): [string, string] {
  const m = PRESERVED_START.exec(body);
  if (!m) return [body, ''];
  return [body.slice(0, m.index), body.slice(m.index)];
}

/** True when the user has typed something above the signature/quote
 *  that a template would overwrite. Callers use this to confirm first. */
export function hasDraftText(body: string): boolean {
  return splitEditable(body)[0].trim().length > 0;
}

/** Return a new body with `templateBody` in place of whatever sat
 *  above the signature/quote. Signature + quoted reply stay intact. */
export function applyTemplate(body: string, templateBody: string): string {
  const [, tail] = splitEditable(body);
  const head = templateBody.replace(/\s+$/, '');
  if (!tail) return head;
  return `${head}\n\n${tail}`;
}
