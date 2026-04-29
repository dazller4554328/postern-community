# Postern Community

Privacy-first local mail client. Rust backend, Svelte web UI, SQLCipher
at rest. Runs on your own machine — laptop, desktop, home server,
Qubes qube — and binds to `127.0.0.1` only. Not exposed to the
internet. Single user.

This is the free, open-source build of [Postern](https://postern.mail).
For the VPS-hosted build with VPN kill-switch, trusted-device sign-in,
unlimited mailboxes and send-later, see the paid product.

## Quickstart

### One-liner install (recommended)

```bash
curl -fsSL https://raw.githubusercontent.com/dazller4554328/postern-community/main/install.sh | bash
```

Installs Docker if it's missing, clones this repo to
`~/postern-community`, builds the image, and starts the stack.
First build takes 5–15 minutes (Rust release build).

Flags: `--dir <path>`, `--update`, `--uninstall`, `--no-build`.
Tested on Fedora 39+, Debian 12, Ubuntu 22.04+, Rocky/Alma 9, Arch.

**Qubes OS:** use a standalone Qube based on Fedora or Debian
(not an AppVM — you want `/var/lib/docker` to persist). Then run the
one-liner above inside that Qube. No Qubes-specific prep is needed
beyond giving the Qube enough disk (15 GB+ including data volume) and
network via `sys-firewall`.

### Manual Docker

```bash
git clone https://github.com/dazller4554328/postern-community.git
cd postern-community
docker build -t ghcr.io/dazller4554328/postern-community:latest -f deploy/docker/Dockerfile .
docker compose -f deploy/docker/docker-compose.yml up -d
open http://127.0.0.1:8080
```

On first launch you set a **vault password** — it encrypts the local
SQLite database at rest. If you forget it, the mail is unrecoverable;
Postern has no "forgot password" button because the key never leaves
your machine.

### Build from source (no Docker)

```bash
git clone https://github.com/dazller4554328/postern-community.git
cd postern-community
cargo build --release -p postern
(cd web && npm install && npm run build)
POSTERN_STATIC_DIR=$PWD/web/build ./target/release/postern serve
```

## Feature matrix

|                                | Postern Community | Postern (paid) |
|--------------------------------|:-----------------:|:---------------:|
| IMAP + SMTP                    | ✅                | ✅              |
| SQLCipher vault                | ✅                | ✅              |
| Calendar (CalDAV read-only)    | ✅                | ✅              |
| Local reminders                | ✅                | ✅              |
| PGP / WKD                      | ✅                | ✅              |
| Rules engine                   | ✅                | ✅              |
| Outbox + undo-send (≤60 s)     | ✅                | ✅              |
| Privacy: tracker classifier, image proxy | ✅      | ✅              |
| Signatures, themes, search     | ✅                | ✅              |
| Mailboxes                      | **3 (hard cap)**  | unlimited       |
| Send-later (beyond undo)       | ❌                | ✅              |
| VPN kill-switch (WireGuard)    | ❌                | ✅              |
| Trusted-device sign-in         | ❌                | ✅              |
| Gmail-categories purge         | ❌                | ✅              |
| Server-side retention sweep    | ❌                | ✅              |
| Auto-archive                   | ❌                | ✅              |
| Mailpile import                | ❌                | ✅              |
| Cloudflare Tunnel / remote access | ❌             | ✅              |

## Upgrading to Postern (paid)

The SQLite schema is identical. Stop the community container, start
the paid binary pointed at the same `postern-data` volume, and
everything is there — mailboxes, threads, keys, reminders, rules.

## Updates

The community build **does not auto-install anything**. On the update
flow:

- `/api/updates/check` polls
  [GitHub Releases](https://github.com/dazller4554328/postern-community/releases/latest)
  anonymously (no license key) and reports whether a new tag exists.
  Read-only — it does not download or run anything.
- `/api/updates/apply` is a hard `409 Conflict` on the community
  build. There is no host-side updater shipped with this image. The
  server cannot install its own successor.
- When the UI banner shows "update available", you run
  `./install.sh --update` (or `docker compose pull && docker compose up -d`
  if you built from a published image). That's the only path an
  upgrade can land — with you typing the command.

## Security posture

Postern Community is designed for **localhost-only** use. Things that
are intentionally out of scope:

- Remote access. No ingress listener, no auth gate beyond the vault
  password. Don't port-forward 8080.
- Multi-user. One vault password per running instance.
- Protecting against a compromised host. If someone has code
  execution as your user, your mail is already gone.

In scope:

- Encryption at rest (SQLCipher with a KEK derived from your vault
  password).
- Not leaking to third parties. Trackers in email bodies are blocked
  before render; images proxy through Postern itself so the sender
  doesn't learn your IP.
- Not phoning home. The only outbound traffic is: IMAP/SMTP to your
  mail servers, the optional update check (`api.github.com`), and
  anything you explicitly click on.

## Contributing

Bugs + feature requests: [GitHub Issues](https://github.com/dazller4554328/postern-community/issues).

PRs welcome. See `CONTRIBUTING.md` (coming soon).

## Licence

Apache 2.0. See `LICENSE`.

Postern Community shares the core mail engine with the paid Postern
product; both are built from the same source. The paid build adds
VPS-grade features that aren't meaningful on a personal machine.
