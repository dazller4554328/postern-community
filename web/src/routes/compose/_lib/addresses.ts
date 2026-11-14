/// Lower-case the local part for dedup/comparison only — display
/// strings keep their original casing.
export function normalizeAddr(raw: string): string {
  const m = raw.match(/<([^>]+)>/);
  return (m ? m[1] : raw).trim().toLowerCase();
}

/// Drop empties, drop self, drop anything already in `existing`,
/// preserving order.
export function dedupeAddrs(
  list: string[],
  self: string,
  existing: string[] = []
): string[] {
  const seen = new Set<string>();
  if (self) seen.add(self.toLowerCase());
  for (const e of existing) seen.add(normalizeAddr(e));
  const out: string[] = [];
  for (const raw of list) {
    const trimmed = raw.trim();
    if (!trimmed) continue;
    const key = normalizeAddr(trimmed);
    if (!key || seen.has(key)) continue;
    seen.add(key);
    out.push(trimmed);
  }
  return out;
}

export function splitAddrs(s: string): string[] {
  return s
    .split(/[,\n;]/)
    .map((a) => a.trim())
    .filter(Boolean);
}
