export type RepeatChoice = 'none' | 'daily' | 'weekly' | 'monthly' | 'yearly';

/** `Date` → `YYYY-MM-DDTHH:mm` in local time. The browser's
 *  `datetime-local` input wants exactly this — it intentionally
 *  doesn't accept timezones because its purpose is "the time the
 *  user typed in their local clock." Flip back to UTC in
 *  `localDatetimeToUnix` on the way out. */
export function unixToLocalDatetime(unix: number): string {
  const d = new Date(unix * 1000);
  const pad = (n: number) => n.toString().padStart(2, '0');
  return (
    `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}` +
    `T${pad(d.getHours())}:${pad(d.getMinutes())}`
  );
}

export function localDatetimeToUnix(s: string): number {
  if (!s) return Math.floor(Date.now() / 1000);
  return Math.floor(new Date(s).getTime() / 1000);
}

export function rruleFromRepeat(r: RepeatChoice): string | null {
  switch (r) {
    case 'daily':
      return 'FREQ=DAILY';
    case 'weekly':
      return 'FREQ=WEEKLY';
    case 'monthly':
      return 'FREQ=MONTHLY';
    case 'yearly':
      return 'FREQ=YEARLY';
    default:
      return null;
  }
}

export function repeatFromRrule(rr: string | null | undefined): RepeatChoice {
  if (!rr) return 'none';
  if (/FREQ=DAILY/i.test(rr)) return 'daily';
  if (/FREQ=WEEKLY/i.test(rr)) return 'weekly';
  if (/FREQ=MONTHLY/i.test(rr)) return 'monthly';
  if (/FREQ=YEARLY/i.test(rr)) return 'yearly';
  return 'none';
}
