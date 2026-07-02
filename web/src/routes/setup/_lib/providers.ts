import type { AccountKind } from '$lib/api';

export type Provider =
  | 'gmail'
  | 'outlook'
  | 'icloud'
  | 'fastmail'
  | 'proton'
  | 'custom';

export interface ProviderSpec {
  id: Provider;
  label: string;
  blurb: string;
  kind: AccountKind;
  imapHost: string;
  imapPort: number;
  smtpHost: string;
  smtpPort: number;
  appPasswordUrl?: string;
  appPasswordNote: string;
  disabled?: boolean;
  disabledNote?: string;
  emoji: string;
}

export const PROVIDERS: ProviderSpec[] = [
  {
    id: 'gmail',
    label: 'Gmail',
    blurb: 'gmail.com / googlemail.com',
    kind: 'gmail',
    imapHost: 'imap.gmail.com',
    imapPort: 993,
    smtpHost: 'smtp.gmail.com',
    smtpPort: 465,
    appPasswordUrl: 'https://myaccount.google.com/apppasswords',
    appPasswordNote:
      'Generate an App Password at myaccount.google.com/apppasswords. Requires 2-Step Verification enabled.',
    emoji: '📮',
  },
  {
    id: 'outlook',
    label: 'Outlook / Microsoft',
    blurb: 'outlook.com, hotmail.com, live.com',
    kind: 'imap',
    imapHost: 'outlook.office365.com',
    imapPort: 993,
    smtpHost: 'smtp.office365.com',
    smtpPort: 587,
    appPasswordNote:
      "Microsoft removed Basic-Auth / App-Password support for outlook.com, hotmail.com, and live.com. IMAP and SMTP now require OAuth2, which Postern doesn't speak yet. Workarounds: forward Outlook mail to a provider that still supports app passwords (Gmail, Fastmail, self-hosted), or run DavMail locally as an OAuth2 → Basic-Auth bridge and add that as a Custom IMAP account.",
    disabled: true,
    disabledNote: 'Outlook.com no longer supports IMAP app passwords. See the provider blurb for workarounds.',
    emoji: '🪟',
  },
  {
    id: 'icloud',
    label: 'iCloud Mail',
    blurb: 'icloud.com, me.com, mac.com',
    kind: 'imap',
    imapHost: 'imap.mail.me.com',
    imapPort: 993,
    smtpHost: 'smtp.mail.me.com',
    smtpPort: 587,
    appPasswordUrl: 'https://appleid.apple.com',
    appPasswordNote:
      'Apple ID → Sign-In and Security → App-Specific Passwords. Requires 2FA enabled.',
    emoji: '☁️',
  },
  {
    id: 'fastmail',
    label: 'FastMail',
    blurb: 'fastmail.com',
    kind: 'imap',
    imapHost: 'imap.fastmail.com',
    imapPort: 993,
    smtpHost: 'smtp.fastmail.com',
    smtpPort: 465,
    appPasswordUrl: 'https://app.fastmail.com/settings/security',
    appPasswordNote:
      'FastMail → Settings → Privacy & Security → Connected apps & API tokens → Manage app passwords → New app password. Set access to "Mail (IMAP, POP, SMTP)", generate the password, then use it here.',
    emoji: '💨',
  },
  {
    id: 'proton',
    label: 'ProtonMail',
    blurb: 'proton.me, protonmail.com',
    kind: 'imap',
    imapHost: '127.0.0.1',
    imapPort: 1143,
    smtpHost: '127.0.0.1',
    smtpPort: 1025,
    appPasswordNote:
      'Proton Mail uses a proprietary protocol — Postern can only connect via Proton Bridge. Free accounts cannot use Bridge.',
    disabled: true,
    disabledNote: 'Requires Proton Bridge (paid Proton only). Integration coming later.',
    emoji: '🛡️',
  },
  {
    id: 'custom',
    label: 'Custom IMAP / SMTP',
    blurb: 'Any standards-compliant provider',
    kind: 'imap',
    imapHost: '',
    imapPort: 993,
    smtpHost: '',
    smtpPort: 587,
    appPasswordNote:
      'Enter the IMAP and SMTP settings your provider gave you. Most use port 993 for IMAP (TLS) and 587 or 465 for SMTP.',
    emoji: '⚙️',
  },
];
