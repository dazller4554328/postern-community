import { marked } from 'marked';
import DOMPurify from 'dompurify';
import type { Note } from '$lib/api';

/// Render markdown synchronously with gfm + breaks pinned for the
/// most "expected" rendering on short notes. The output is injected via
/// {@html}, so it MUST be sanitized: marked passes raw HTML in the source
/// straight through, which would otherwise be stored XSS.
export function previewHtml(src: string): string {
  try {
    const raw = marked.parse(src || '', {
      async: false,
      gfm: true,
      breaks: true,
    }) as string;
    return DOMPurify.sanitize(raw);
  } catch {
    return '';
  }
}

/// First non-empty line, stripped of the most obvious markdown noise.
export function summarize(n: Note): string {
  const body = (n.body || '').trim();
  if (!body) return '';
  const firstLine = body.split('\n').find((l) => l.trim().length > 0) ?? '';
  return firstLine
    .replace(/^#{1,6}\s+/, '')
    .replace(/[*_`>~-]+/g, '')
    .slice(0, 140);
}

/// Title with fallbacks: explicit title → summary → 'Untitled'.
export function displayTitle(n: Note): string {
  const t = (n.title || '').trim();
  if (t) return t;
  return summarize(n) || 'Untitled';
}
