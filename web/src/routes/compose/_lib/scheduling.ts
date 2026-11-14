export type SendChoice = 'now' | 'in5m' | 'in30m' | 'in1h' | 'tomorrow9' | 'custom';

/// Compute the scheduled_at Unix timestamp for the picker selection.
/// Returns null when the user picked 'custom' with a malformed or
/// past datetime.
export function resolveScheduledAt(
  choice: SendChoice,
  sendUndoSecs: number,
  customScheduledAt: string
): number | null {
  const now = Math.floor(Date.now() / 1000);
  switch (choice) {
    case 'now': {
      const delay = Math.max(0, Math.min(60, sendUndoSecs));
      return now + delay;
    }
    case 'in5m':
      return now + 5 * 60;
    case 'in30m':
      return now + 30 * 60;
    case 'in1h':
      return now + 60 * 60;
    case 'tomorrow9': {
      const d = new Date();
      d.setDate(d.getDate() + 1);
      d.setHours(9, 0, 0, 0);
      return Math.floor(d.getTime() / 1000);
    }
    case 'custom': {
      if (!customScheduledAt) return null;
      const ts = Math.floor(new Date(customScheduledAt).getTime() / 1000);
      if (!Number.isFinite(ts) || ts <= now) return null;
      return ts;
    }
  }
}
