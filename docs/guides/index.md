---
title: Guides
---

# Guides

Short, task-focused walkthroughs for the bits that aren't really
about Postern itself but always show up the first time you set it
up — provider quirks, app passwords, Tailscale toggles, and the
like.

## Provider setup

- **[Create a Gmail app password](gmail-app-password.md)** — Google
  blocks your normal password for IMAP. You need a 16-character
  app password instead. Three minutes, with screenshots.

## Encryption

- **[Set up PGP keys](pgp-keys.md)** — generate or import a keypair in
  Settings → PGP, publish it so people can write to you, and let
  Postern auto-discover everyone else's keys. No GnuPG required.
- **[Create a PGP key with GnuPG](create-pgp-key.md)** — the
  cross-platform (Windows / macOS / Linux) route for a key *outside*
  Postern: generate, export, and publish in a few commands.
- **[PGP keys in depth](pgp-keys-explained.md)** — the long-form
  walkthrough: every `gpg --full-generate-key` prompt explained, plus
  key length, expiry, revocation certificates, and backups.
- **[WHMCS PGP Mailer](whmcs-pgp.md)** — install the add-on that
  PGP-encrypts your outbound WHMCS email, and publish your system key
  so clients can reply encrypted.

## Backups

- **[Back up to Google Drive](google-drive-backup.md)** — create your
  own Google OAuth client, add three values to `.env`, connect, and
  restore directly from Drive. Includes the "publish the app so
  backups don't die after 7 days" gotcha.

More providers coming as users hit them. If you're stuck on one
that isn't here, open an issue at
[github.com/dazller4554328/postern-community](https://github.com/dazller4554328/postern-community)
and we'll write the guide.
