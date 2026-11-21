# Migrating from Mailpile to Postern

Mailpile's mail store is a Maildir-flavoured tree of RFC822 files —
under the WERVD wrapper, the per-message files are plain SMTP mail.
Postern's path-based importer reads them directly without needing
Mailpile to be running.

## Prerequisites

- Postern is installed and an account is configured (the import will
  tag every message with that account).
- You have shell access to the VPS that holds the Mailpile data.
- The Postern container can read the Mailpile mail directory (next
  section).

## 1. Locate your Mailpile mail directory

The default path is

```
~/.local/share/Mailpile/<profile>/mail/
```

For most installs the profile is `default`:

```bash
ls ~/.local/share/Mailpile/default/mail/
# 02da1  0aa8e  107e9  ...   (one bucket per Mailpile mailbox)
```

Each bucket contains the standard Maildir layout (`cur/`, `new/`,
`tmp/`) plus a `wervd.ver` marker file. Postern's importer skips the
marker and walks the `cur/` and `new/` subdirectories for actual
message files.

## 2. Point Postern's import bind-mount at it

Postern's container mounts a single sandboxed import root at
`/var/lib/postern/import` (read-only). The host path is configured
via the `POSTERN_IMPORT_DIR` environment variable in the operator's
`.env` file.

```bash
sudo nano /opt/postern/.env
```

Add or update:

```dotenv
POSTERN_IMPORT_DIR=/home/<user>/.local/share/Mailpile/default/mail
```

The path should be readable by the user that owns the Postern
container (typically `ubuntu` on Postern's reference VPS deploy).

### .env discoverability fix (one-time)

`docker compose` looks for the project `.env` in the **directory of
the compose file** (`/opt/postern/deploy/docker/.env`), not the
operator-managed `.env` at `/opt/postern/.env`. Without a link
between them, the `${POSTERN_IMPORT_DIR:-/dev/null}` substitution
silently falls back to `/dev/null` — the bind-mount looks empty
inside the container and the import endpoint reports 0 files
scanned.

The Postern auto-updater (`postern-updater.sh`) creates this
symlink on every release, so installs that ride updates pick it up
automatically. For an install that hasn't taken an update since
this fix shipped, run:

```bash
sudo ln -sfn /opt/postern/.env /opt/postern/deploy/docker/.env
```

## 3. Recreate the container

The bind-mount source is fixed at container-create time — `restart`
is not enough; you need `--force-recreate`:

```bash
cd /opt/postern
sudo docker compose -f deploy/docker/docker-compose.yml up -d --force-recreate postern
```

Wait ~10 seconds for the container to come back healthy:

```bash
sudo docker inspect -f '{{.State.Health.Status}}' postern-postern-1
# healthy
```

Verify the bind-mount is now pointed at the right place:

```bash
sudo docker exec postern-postern-1 ls /var/lib/postern/import | head
# 02da1
# 0aa8e
# ...
```

If you still see `crw-rw-rw- /dev/null`, the symlink step above
didn't take. Re-check `/opt/postern/deploy/docker/.env` exists and
points at `/opt/postern/.env`.

## 4. Run the import

In Postern's web UI:

1. **Settings → Import**
2. **Target mailbox**: pick the account that should own these
   messages, OR leave on **Auto-detect from headers** to scan each
   message's `Delivered-To` / `To` / `Cc` against your configured
   accounts (recommended when the Mailpile store is mixed across
   multiple addresses).
3. **Or import from server path**: leave the path field blank to
   import the entire bind-mount root, or specify a sub-path like
   `02da1/cur` to import a single Mailpile bucket.
4. Click **Import path**.

The walker recurses through every directory under the configured
root. On a typical Mailpile store with thousands of messages, expect
1–3 minutes; the page stays open and the result lands in `Scanned /
Imported / Skipped / Errors` counters.

## How dedup works

Every imported message goes through `Db::upsert_message`, which keys
on `(account_id, message_id)`. If a message with the same
`Message-ID` is already in Postern's local store for the same
account, the upsert returns `false` and the importer increments
`skipped` instead of `imported`. **Re-running the import is safe** —
it will not create duplicates.

A few corner cases:

- **No `Message-ID` header**: the parser fabricates a stable ID from
  the body bytes, so a re-import of the same file still dedups.
- **Same RFC822 body, different `Message-ID`**: counted as two
  distinct messages. Mailpile-side bouncers / forwarding can produce
  this; nothing Postern can do without semantic guesswork.
- **Auto-detect can't match an account**: skipped (counted in
  `skipped`). To rescue these, run a second import with **Target
  mailbox** explicitly set.

## What gets imported

Every plain RFC822 file Postern can find. By default the importer
walks all subdirectories under the bind-mount root, including
Mailpile's `new/` and `cur/` Maildir subdirectories.

What's **not** imported, by design:

- Mailpile's `wervd.ver` marker files (skipped).
- Hidden files (`.dotfiles`).
- The Mailpile config (`mailpile.cfg`), search index
  (`mailpile.idx`), or autocrypt store — Postern has its own.

## After the import

- New messages appear under their detected account's `INBOX` label.
  Move into folders / labels via Postern's normal archive flow if
  you want them organised differently.
- The Mailpile data is not touched — the bind-mount is read-only.
  You can leave Mailpile installed indefinitely; Postern just reads
  the same files.
- If you want to free up the disk, take a Postern backup (Settings →
  Backups → Create backup now), verify it restores, then archive or
  delete the Mailpile tree.

## Troubleshooting

| Symptom | Likely cause | Fix |
|---|---|---|
| `Scanned: 0` | Bind-mount points at `/dev/null` | Re-do step 2 + recreate container; check `docker inspect` mount source |
| `Scanned: N, Imported: 0, Skipped: N` | Auto-detect found no account; OR everything is already in Postern | Pick an explicit target account; or treat as a no-op (already imported) |
| `Errors: N` (high) | Permission denied somewhere in the tree | Check the path is readable by the container user (`docker exec ... ls /var/lib/postern/import`) |
| Container stuck unhealthy after recreate | Compose env-file resolution issue | Verify `/opt/postern/deploy/docker/.env` symlink exists and points at `/opt/postern/.env` |
