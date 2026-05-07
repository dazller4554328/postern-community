export function formatDate(unix: number): string {
  if (!unix) return '—';
  const d = new Date(unix * 1000);
  const now = new Date();
  const diffMs = now.getTime() - d.getTime();
  const diffDays = diffMs / (1000 * 60 * 60 * 24);
  if (diffDays < 1 && d.getDate() === now.getDate()) {
    return d.toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit' });
  }
  if (diffDays < 365) {
    return d.toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
  }
  return d.toLocaleDateString(undefined, { year: 'numeric', month: 'short', day: 'numeric' });
}

// "5m ago", "2h ago", "Yesterday", "3d ago", then absolute date after
// a week. Used by the compact message list row so every message ends
// with a terse relative timestamp instead of an absolute date.
export function formatRelative(unix: number): string {
  if (!unix) return '—';
  const now = Date.now();
  const then = unix * 1000;
  const diff = now - then;
  if (diff < 45_000) return 'just now';
  const mins = Math.round(diff / 60_000);
  if (mins < 60) return `${mins}m ago`;
  const hours = Math.round(diff / 3_600_000);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.round(diff / 86_400_000);
  if (days === 1) return 'Yesterday';
  if (days < 7) return `${days}d ago`;
  // Past a week, an absolute date reads better than "37d ago".
  return new Date(then).toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
}

export function formatSender(from: string | null): string {
  if (!from) return '(unknown sender)';
  const m = from.match(/^\s*"?([^"<]+?)"?\s*<.+>\s*$/);
  return (m ? m[1] : from).trim();
}
