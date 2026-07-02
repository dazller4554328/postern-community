/** Central place for links and copy reused across pages. */

export const URLS = {
  github: 'https://github.com/dazller4554328/postern-community',
  docs: 'https://docs.postern.email/',
  docsInstall: 'https://docs.postern.email/install/',
  docsHomeServer: 'https://docs.postern.email/install/home-server/',
  docsHardening: 'https://docs.postern.email/install/hardening/',
  docsCommunity: 'https://docs.postern.email/install/community/',
  billing: 'https://billing.postern.email/',
  // Direct WHMCS store/order pages for each product.
  storePro: 'https://billing.postern.email/index.php?rp=/store/postern',
  storePgp: 'https://billing.postern.email/index.php?rp=/store/postern-pgp-mailerlicense',
  storePgpTrial:
    'https://billing.postern.email/index.php?rp=/store/postern-pgp-mailer-trial-license',
  tailscale: 'https://tailscale.com',
  autocrypt: 'https://autocrypt.org'
} as const;

export const NAV = [
  { label: 'Features', href: '/features' },
  { label: 'Pricing', href: '/pricing' },
  { label: 'WHMCS PGP', href: '/whmcs-pgp' },
  { label: 'Download', href: '/download' },
  { label: 'Docs', href: URLS.docs, external: true }
] as const;

/** Crypto / trust chips shown under the hero. */
export const TRUST_CHIPS = [
  'Argon2id',
  'ChaCha20-Poly1305',
  'SQLCipher vault',
  'OpenPGP',
  'Autocrypt',
  'No telemetry'
] as const;

export interface Feature {
  id: string;
  icon: string; // key into Icon component
  title: string;
  blurb: string;
}

export const PILLARS: Feature[] = [
  {
    id: 'keys',
    icon: 'key',
    title: 'Your keys, never theirs',
    blurb:
      'PGP private keys live on your machine and never touch a provider. Autocrypt does the handshake, so encryption just works between you and anyone Autocrypt-aware.'
  },
  {
    id: 'provider',
    icon: 'server',
    title: 'Bring any provider',
    blurb:
      'Gmail, Fastmail, iCloud, your own Postfix — anything that speaks IMAP and SMTP. Postern replaces the client reading your mail, not your mail flow.'
  },
  {
    id: 'private',
    icon: 'shield',
    title: 'Private by default',
    blurb:
      'A SQLCipher-encrypted vault on hardware you control. No tracking pixels, no third-party fetches, no telemetry phoning home.'
  }
];

export interface BentoItem {
  id: string;
  title: string;
  blurb: string;
  img?: string;
  alt?: string;
  span?: 'wide' | 'tall' | 'normal';
}

export const BENTO: BentoItem[] = [
  {
    id: 'vault',
    title: 'Encrypted local vault',
    blurb:
      'Pull every message off your provider and keep the only copy in your own SQLCipher vault. The provider becomes a relay — the archive lives on your box.',
    img: '/img/app/main/main_inbox.png',
    alt: 'Postern unified inbox in the dark theme',
    span: 'wide'
  },
  {
    id: 'pgp',
    title: 'PGP + Autocrypt',
    blurb: 'End-to-end encryption that negotiates itself. Send to any Autocrypt client and the body stays sealed.',
    span: 'normal'
  },
  {
    id: 'ai',
    title: 'Local-first AI',
    blurb:
      'Opt-in and kept deliberately simple for privacy: voice dictation and one-tap polish, running on your own Ollama box. Off by default.',
    img: '/img/app/mobile/mobile_mail_send_features.png',
    alt: 'Postern compose screen with voice dictation and polish',
    span: 'tall'
  },
  {
    id: 'mobile',
    title: 'Built for the phone browser',
    blurb: 'A responsive, touch-first web UI — no app to install. Open Postern in your phone’s browser over your private mesh and triage on the go.',
    img: '/img/app/mobile/mobile_inbox.png',
    alt: 'Postern mobile inbox',
    span: 'tall'
  },
  {
    id: 'themes',
    title: 'Themes with character',
    blurb: 'Calm light, deep dark, and a full cyber theme. Display settings that actually change the feel of the app.',
    span: 'normal'
  },
  {
    id: 'import',
    title: 'Bring your history',
    blurb: 'Migrate from Mailpile or bulk-import from Gmail. Your old mail comes with you into the vault.',
    span: 'normal'
  }
];

export interface PlanFeature {
  label: string;
  community: boolean | string;
  pro: boolean | string;
}

export const PLAN_FEATURES: PlanFeature[] = [
  { label: 'Mailboxes', community: 'Up to 3', pro: 'Unlimited' },
  { label: 'Self-hosted, your hardware', community: true, pro: true },
  { label: 'SQLCipher encrypted vault', community: true, pro: true },
  { label: 'PGP + Autocrypt', community: true, pro: true },
  { label: 'Bring any IMAP/SMTP provider', community: true, pro: true },
  { label: 'Light / dark / cyber themes', community: true, pro: true },
  { label: 'Mailpile & Gmail import', community: true, pro: true },
  { label: 'Tailscale mesh access', community: false, pro: true },
  { label: 'Per-device sessions & audit log', community: false, pro: true },
  { label: 'Local-first AI (dictation & polish)', community: false, pro: true },
  { label: 'Calendars & contacts', community: false, pro: true },
  { label: 'One-click in-app updates', community: false, pro: true },
  { label: 'Automatic VPN egress routing', community: false, pro: true }
];

export const YEAR = 2026;
