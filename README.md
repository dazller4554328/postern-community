# Postern Community

Privacy-first local mail client. Rust backend, Svelte web UI, encrypted
(SQLCipher) at rest. Runs on your own machine — laptop, desktop, home
server, or a Qubes qube — binds to `127.0.0.1` only, and is built for a
single user. Nothing is exposed to the internet.

📖 **Full docs:** [docs.postern.email](https://docs.postern.email)

---

## Which edition do I want?

|                | **Postern Community**            | **Postern Pro**                     |
|----------------|----------------------------------|-------------------------------------|
| Price          | Free, open source (Apache 2.0)   | Paid                                |
| Runs on        | Your own machine, localhost only | Your VPS, remote access             |
| Mailboxes      | Up to 3                          | Unlimited                           |
| Extras         | Core mail, calendar, PGP, rules  | + VPN kill-switch, devices, more    |
| **Install**    | **[↓ this repo](#install-postern-community)** | **[↓ billing.postern.email](#install-postern-pro)** |

Full side-by-side breakdown in the [feature comparison](#feature-comparison)
below. **This repository is the Community edition** — keep reading to install
it.

---

## Install Postern Community

### Quick install (recommended)

```bash
curl -fsSL https://raw.githubusercontent.com/dazller4554328/postern-community/main/install.sh | bash
```

This installs Docker if it's missing, clones the repo to
`~/postern-community`, builds the image, and starts the stack. The first
build takes 5–15 minutes (it's a Rust release build), then Postern is live
at **http://127.0.0.1:8080**.

Tested on Fedora 39+, Debian 12, Ubuntu 22.04+, Rocky/Alma 9, and Arch.
Non-systemd Debian derivatives (MX Linux, Devuan, antiX) work too.

> **First launch:** you set a **vault password** that encrypts the local
> database at rest. The key never leaves your machine, so there is no
> "forgot password" reset — if you lose it, the mail is unrecoverable.

<details>
<summary><b>Install flags</b> (<code>--update</code>, <code>--uninstall</code>, <code>--purge</code>, …)</summary>

```
--dir <path>   install somewhere other than ~/postern-community
--update       pull/rebuild and restart the stack
--uninstall    stop the stack and delete its volumes (keeps checkout + images)
--purge        full wipe: stack, volumes, checkout, images, build cache
               (asks you to type "purge" to confirm)
--no-build     start without rebuilding the image
--service      install the systemd unit (see below)
--no-service   skip the systemd prompt
```
</details>

<details>
<summary><b>Run on boot (systemd service)</b></summary>

The one-liner above does **not** install a systemd unit — piping into `bash`
is non-interactive, so the prompt is skipped. The container already restarts
on crash and after reboot on its own (`restart: unless-stopped`, as long as
Docker starts on boot).

Install the unit only if you want a normal ops surface
(`systemctl status/start/stop postern-community`, journal logs):

```bash
curl -fsSL https://raw.githubusercontent.com/dazller4554328/postern-community/main/install.sh | bash -s -- --service
```

The `-s --` forwards the flag to the script. Running `bash install.sh` from a
cloned checkout instead is interactive and will prompt you.
</details>

<details>
<summary><b>Qubes OS</b></summary>

Use a **standalone Qube** based on Fedora or Debian (not an AppVM — you want
`/var/lib/docker` to persist), then run the one-liner above inside it. No
Qubes-specific prep is needed beyond giving the Qube enough disk (15 GB+
including the data volume) and network via `sys-firewall`.
</details>

<details>
<summary><b>Manual Docker</b> (no install script)</summary>

```bash
git clone https://github.com/dazller4554328/postern-community.git
cd postern-community
docker build -t ghcr.io/dazller4554328/postern-community:latest -f deploy/docker/Dockerfile .
docker compose -f deploy/docker/docker-compose.yml up -d
open http://127.0.0.1:8080
```
</details>

<details>
<summary><b>Build from source</b> (no Docker)</summary>

```bash
git clone https://github.com/dazller4554328/postern-community.git
cd postern-community
cargo build --release -p postern
(cd web && npm install && npm run build)
POSTERN_STATIC_DIR=$PWD/web/build ./target/release/postern serve
```
</details>

Step-by-step walkthrough:
[docs.postern.email/install/community](https://docs.postern.email/install/community/).

---

## Install Postern Pro

Postern Pro is the VPS-hosted build with remote access, a WireGuard VPN
kill-switch, trusted-device sign-in, unlimited mailboxes, send-later, and
more. It isn't installed from this repo — you get it through the paid product:

👉 **[billing.postern.email](https://billing.postern.email)**

Install guide:
[docs.postern.email/install/home-server](https://docs.postern.email/install/home-server/).

**Already running Community?** Upgrading is seamless — the database schema is
identical. Stop the Community container, start the Pro binary pointed at the
same `postern-data` volume, and everything is there: mailboxes, threads, keys,
reminders, and rules.

---

## Feature comparison

|                                                    | Community         | Pro       |
|----------------------------------------------------|:-----------------:|:---------:|
| IMAP + SMTP                                         | ✅                | ✅        |
| SQLCipher vault (encryption at rest)               | ✅                | ✅        |
| Local-first calendar (create / edit / delete)      | ✅                | ✅        |
| Local reminders                                    | ✅                | ✅        |
| Local-first secure notes                           | ✅                | ✅        |
| Contacts + recipient autocomplete                  | ✅                | ✅        |
| Compose: voice dictation, grammar / rewrite assist | ✅                | ✅        |
| TOTP second factor at vault unlock                 | ✅                | ✅        |
| PGP / WKD                                           | ✅                | ✅        |
| Rules engine                                        | ✅                | ✅        |
| Outbox + undo-send (≤60 s)                          | ✅                | ✅        |
| Privacy: tracker classifier, image proxy           | ✅                | ✅        |
| Signatures, themes, search                         | ✅                | ✅        |
| Mailboxes                                           | **3 (hard cap)**  | unlimited |
| Send-later (beyond undo)                            | ❌                | ✅        |
| VPN kill-switch (WireGuard)                         | ❌                | ✅        |
| Trusted-device sign-in                             | ❌                | ✅        |
| Gmail-categories purge                             | ❌                | ✅        |
| Server-side retention sweep                        | ❌                | ✅        |
| Auto-archive                                        | ❌                | ✅        |
| Mailpile import                                     | ❌                | ✅        |
| Tailscale-fronted remote access                    | ❌                | ✅        |

---

## Updates

Community **never auto-installs anything** — an upgrade only happens when you
run the command yourself:

```bash
./install.sh --update
```

(Or `docker compose pull && docker compose up -d` if you run a published
image.) When the UI shows an "update available" banner, that's the signal to
run it.

Under the hood: `/api/updates/check` anonymously polls
[GitHub Releases](https://github.com/dazller4554328/postern-community/releases/latest)
(no license key, read-only — it never downloads or runs anything), and
`/api/updates/apply` always returns `409 Conflict` — the server has no way to
install its own successor.

---

## Security posture

Postern Community is designed for **localhost-only** use.

**In scope**

- **Encryption at rest** — SQLCipher with a key derived from your vault
  password.
- **No leaking to third parties** — trackers in email bodies are blocked
  before render; images proxy through Postern so the sender never learns your
  IP.
- **No phoning home** — the only outbound traffic is IMAP/SMTP to your mail
  servers, the optional update check (`api.github.com`), and anything you
  explicitly click.

**Intentionally out of scope**

- **Remote access** — no ingress listener, no auth gate beyond the vault
  password. Don't port-forward 8080.
- **Multi-user** — one vault password per running instance.
- **A compromised host** — if someone has code execution as your user, your
  mail is already gone.

---

## Documentation

Full docs: [docs.postern.email](https://docs.postern.email)

- [Install — Community Edition](https://docs.postern.email/install/community/)
- [Install — home server with Tailscale](https://docs.postern.email/install/home-server/)
- [Guides](https://docs.postern.email/guides/) — provider setup (Gmail app
  passwords, etc.)
- [Reference](https://docs.postern.email/reference/) — storage invariants,
  migration notes

---

## Contributing

Bugs and feature requests: [GitHub Issues](https://github.com/dazller4554328/postern-community/issues).
PRs welcome — see `CONTRIBUTING.md` (coming soon).

## Licence

Apache 2.0 (see `LICENSE`). Postern Community shares the core mail engine with
the paid Postern product — both are built from the same source. Pro adds
VPS-grade features that aren't meaningful on a personal machine.
</content>
</invoke>
