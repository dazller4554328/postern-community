# Postern at home, accessed anywhere via Tailscale

This guide walks through running Postern Pro on a computer you own
(home server, mini-PC, NUC, idle workstation, Raspberry Pi 5, …)
and reaching it from your phone over [Tailscale](https://tailscale.com).

The result: nothing about your email lives on a rented server,
there's no public DNS record pointing at your house, and your phone
sees Postern at a hostname only your devices know about.

## What this looks like

```
  ┌──────────────────────────┐         ┌─────────────┐
  │  Home PC                 │         │  Phone      │
  │                          │         │             │
  │  ┌────────────────────┐  │ ◀────▶  │  Tailscale  │
  │  │ Postern container  │  │  WG     │             │
  │  │ + Tailscale sidecar│  │         │  Browser    │
  │  └────────────────────┘  │         │             │
  └──────────────────────────┘         └─────────────┘
        │                                      ▲
        ▼                                      │
   IMAP/SMTP out                          https://postern.<tailnet>.ts.net
   to your provider
```

The Tailscale sidecar joins your tailnet, requests a cert from
Tailscale's MagicDNS HTTPS endpoint, and exposes Postern at
`https://postern.<your-tailnet>.ts.net`. Your phone, also on the
tailnet, browses to that URL like any other website.

## What you give up

- **The PC has to stay on.** If it sleeps, your phone can't reach
  mail. Most home users solve this with an always-on mini-PC; a
  Raspberry Pi 5 with 4–8 GB RAM is plenty.
- **Initial setup is one extra step** vs. the VPS path — you install
  Tailscale on the host and on each phone. It's free for personal
  use and takes about two minutes per device.

## What you keep

- Same Postern UI, same features, same data shape.
- Mail data lives in a SQLCipher-encrypted volume on your machine.
- Outbound IMAP/SMTP behaves identically (you can still pin egress
  through a kill-switched VPN like NordVPN if you want).
- The Pro license is one install — your home PC counts as the install,
  and any phone or laptop you connect via Tailscale is just a browser.

## Prerequisites

- A 64-bit Linux machine (Ubuntu 22.04+, Debian 12, Fedora 39+, Arch
  all work). Mac (Docker Desktop) and Windows (WSL2) work too but are
  not the recommended path.
- 2 GB RAM minimum, 4 GB comfortable. The Rust release build during
  install benefits from 4 GB.
- 10 GB free disk. Postern itself sits well under 5 GB; the rest is
  Docker image cache headroom.
- Internet for the install + ongoing IMAP sync (no inbound port
  forwarding needed).
- A Postern Pro license key (`PSTN-XXXX-XXXX-XXXX-XXXX`).

## Step 1 — Docker

Skip if you already have Docker. The Postern installer can install it
for you, but on a home PC you might prefer to do it yourself:

```bash
# Ubuntu / Debian
curl -fsSL https://get.docker.com | sh
sudo usermod -aG docker "$USER"
# log out and back in so the group takes effect
```

Verify:

```bash
docker run --rm hello-world
```

## Step 2 — Tailscale

Install Tailscale on the host and bring it up:

```bash
curl -fsSL https://tailscale.com/install.sh | sh
sudo tailscale up
# Follow the URL it prints to authenticate via your Tailscale account
```

Confirm it's up:

```bash
tailscale ip -4         # prints your tailnet IPv4 (100.x.y.z)
tailscale status        # lists peer devices
```

Enable [MagicDNS](https://tailscale.com/kb/1081/magicdns) and
[HTTPS certs](https://tailscale.com/kb/1153/enabling-https) in the
Tailscale admin console — both one-click toggles. Without these the
phone-side experience falls back to raw IPs and untrusted certs.

## Step 3 — Postern

Run the license-gated installer:

```bash
curl -fsSL https://updates.postern.email/install.sh \
  | sudo LICENSE=PSTN-XXXX-XXXX-XXXX-XXXX bash
```

The bootstrap validates your license against `updates.postern.email`,
downloads the signed Pro release tarball, verifies the Ed25519
signature, stages the tree into `/opt/postern`, and hands off to the
in-tree installer. That installer prompts for the Tailscale auth key
and brings up the sidecar profile in `docker-compose.yml`. It does:

- builds the Postern Docker image (~3 minutes the first time)
- creates a persistent volume for the encrypted vault
- starts both the Postern container and the Tailscale sidecar
- prints the URL it's now reachable at

You can also do it manually:

```bash
docker compose -f deploy/docker/docker-compose.yml \
  --profile tailscale up -d --build
```

Once the sidecar is registered, the host name is
`postern.<your-tailnet>.ts.net` (visible under the Machines tab in
the Tailscale admin).

## Step 4 — Mobile

On each phone:

1. Install the [Tailscale app](https://tailscale.com/download) and
   sign in with the same account.
2. Open `https://postern.<your-tailnet>.ts.net` in Safari / Chrome.
3. Optionally, "Add to Home Screen" — the app installs as a PWA and
   behaves like a native mail client.
4. Set the master password (first device only) or unlock the vault
   (subsequent devices).

That's it. The phone now talks to your home PC over the WireGuard
tunnel Tailscale built between them. Nothing transits the public
internet between phone and PC — even when your phone is on a
coffee-shop wifi on the other side of the world.

## Day-2 operations

- **Updates:** Settings → Updates → Install. Same flow as the VPS
  build; Postern fetches the latest signed release from the update
  server.
- **Backups:** Settings → Backups. The encrypted DB ships to whichever
  destinations you configure (S3 / B2 / SFTP / Google Drive).
- **Power:** if your PC is a desktop, set it to never sleep on AC
  power. On Linux: `sudo systemctl mask sleep.target suspend.target
  hibernate.target hybrid-sleep.target`.

## Trouble?

- **Phone can't reach the URL** — confirm both devices show up under
  `tailscale status`. Check Tailscale ACLs aren't blocking node-to-
  node traffic.
- **Cert errors** — MagicDNS HTTPS must be enabled in the Tailscale
  admin. Without it the sidecar serves a self-signed cert.
- **Build OOMs on a 2 GB machine** — add 4 GB swap before the install
  or upgrade RAM. The Rust release build is the spike.

For anything else, file an issue on

[github.com/dazller4554328/postern-community](https://github.com/dazller4554328/postern-community).
