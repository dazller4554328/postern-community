---
title: Community Edition install
---

# Community Edition

Postern Community is the open-source build of Postern, published
under Apache 2.0 at
[github.com/dazller4554328/postern-community][repo].

## Scope

Community is **localhost-only by design**. It binds to `127.0.0.1`,
ships no remote-access path, and caps the mailbox count at 3. The
intent is "run it on the machine you'll use it from" — a Linux
desktop, a Mac with Docker, a Qubes qube. If you want any of:

- Mobile / multi-device access via Tailscale
- More than 3 mailboxes
- VPN kill-switch (NordVPN / Mullvad / WireGuard)
- AI features (Datas RAG, polish, transcribe)
- Trusted-device session tracking

…that's [Postern Pro](index.md). Community and Pro share
the same SQLCipher schema, so a Pro license activates in place — no
data migration.

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/dazller4554328/postern-community/main/install.sh | bash
```

Or from a clone:

```bash
git clone https://github.com/dazller4554328/postern-community.git
cd postern-community
./install.sh
```

That's it — `http://127.0.0.1:8080` is your inbox. First-boot prompts
you for a master password.

### Flags

- `--dir <path>` — install location. Default: `~/postern-community`.
- `--update` — `git pull` + rebuild + restart. Same effect as
  pulling new commits and `docker compose up -d --build`.
- `--uninstall` — stop the stack and remove the data volume. Asks
  for confirmation.
- `--no-build` — skip the image build (useful with a pre-built
  image).
- `--service` — install a `postern-community.service` systemd unit
  so the stack comes back automatically on reboot.

## Updates

The Community build does **not** auto-install updates. The flow is:

1. The UI banner notices a new release on the GitHub Releases feed
   (anonymous poll — no license, no telemetry).
2. You run `./install.sh --update`.
3. The new container starts. Existing data carries over (the data
   volume is preserved across rebuilds).

That `--update` step is intentionally manual. The Community image
holds no privileged credentials and has no host-side updater — the
container can't restart its own host stack. Pro's update flow is
different (signed tarballs, host-side updater); this is the OSS
build's safer floor.

## Where the data lives

- **SQLCipher database**: in the `postern-community-data` Docker
  volume.
- **Encryption key**: derived at runtime from your master password
  via Argon2id. Never written to disk.
- **Mail bodies + attachments**: stored as encrypted blobs alongside
  the DB.

Backing up the data volume is enough to migrate to a new machine.
There's no cloud sync; if you lose the volume + your master
password, the mail is unrecoverable. Set up the in-app backup to
S3 / B2 / SFTP / Google Drive (Settings → Backups) early.

[repo]: https://github.com/dazller4554328/postern-community
