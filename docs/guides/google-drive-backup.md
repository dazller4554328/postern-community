---
title: Google Drive backups
---

# Back up to Google Drive

Postern can push your encrypted backup tarball to **your own** Google
Drive, and restore directly from it. Because Postern is self-hosted and
privacy-first, it does **not** ship a shared Google app — you create
your own OAuth client in Google Cloud Console (5 minutes, free) and drop
three values into your `.env`. Your backups go to your Drive, through
your credentials; nothing routes via a Postern-operated server.

!!! info "What Google ever sees"
    The tarball is already encrypted on disk (SQLCipher + ChaCha20)
    before it leaves the machine. Google stores an opaque blob it can't
    read. The OAuth scope is `drive.file`, which only lets Postern see
    files **it** created — never the rest of your Drive.

## 1. Create a Google Cloud project + enable the Drive API

1. Go to the [Google Cloud Console](https://console.cloud.google.com).
2. Top bar → project picker → **New Project** (name it e.g.
   "Postern Backups"). Select it once created.
3. **APIs & Services → Library** → search **Google Drive API** →
   **Enable**.

## 2. Configure the OAuth consent screen

**APIs & Services → OAuth consent screen.**

1. User type: **External** → **Create**.
2. Fill in app name (e.g. "Postern"), your support email, developer
   email. Logo/links optional. **Save and continue.**
3. **Scopes** → *Add or remove scopes* → add
   `.../auth/drive.file` (the "See, edit, create, and delete only the
   specific Google Drive files you use with this app" scope). Save.
4. **Test users** → add the Google account you'll back up to.

!!! danger "Publish the app, or backups die after 7 days"
    While the consent screen is in **Testing** status, Google
    **expires refresh tokens after 7 days** — your backups will push
    fine for a week, then start failing with
    `invalid_grant`. To avoid this, on the OAuth consent screen click
    **Publish app** → move it to **In production**. For the
    `drive.file` scope this needs **no Google verification review**.
    Do this now and you'll never hit the 7-day cliff.

## 3. Create the OAuth client ID

**APIs & Services → Credentials → Create credentials → OAuth client
ID.**

1. Application type: **Web application**.
2. Name: anything (e.g. "Postern server").
3. **Authorized redirect URIs → Add URI.** This must be your Postern
   URL plus `/api/backups/oauth/google/callback`. For a Tailscale
   install:

   ```
   https://postern.<your-tailnet>.ts.net/api/backups/oauth/google/callback
   ```

   It must match what you put in `.env` (next step) **byte-for-byte** —
   same scheme, host, path, **no trailing slash**.
4. **Create.** Copy the **Client ID** and **Client secret** — you need
   them next.

## 4. Add three values to `.env`

Edit `/opt/postern/.env` on the server:

```bash
sudo nano /opt/postern/.env
```

Add these three lines. **Do not wrap the values in quotes** — the
compose `env_file` keeps quotes as part of the value, which would break
the redirect match:

```dotenv
POSTERN_GDRIVE_CLIENT_ID=1234567890-abc123.apps.googleusercontent.com
POSTERN_GDRIVE_CLIENT_SECRET=GOCSPX-your-secret-here
POSTERN_GDRIVE_REDIRECT_URI=https://postern.<your-tailnet>.ts.net/api/backups/oauth/google/callback
```

All three must be present and non-empty — if any is missing, Postern
hides the **Connect Google Drive** button entirely.

## 5. Recreate the container so it reads the new `.env`

`env_file` values are only read when the container is **created**, not
on a live restart. Recreate it:

```bash
cd /opt/postern
sudo docker compose -f deploy/docker/docker-compose.yml --profile tailscale up -d
# if it reports "up to date", force it:
sudo docker compose -f deploy/docker/docker-compose.yml --profile tailscale up -d --force-recreate postern
```

(Drop `--profile tailscale` if you didn't install the Tailscale
sidecar.)

## 6. Connect Postern to Drive

1. Open Postern → **Settings → Backups**.
2. Under off-site destinations, click **Connect Google Drive**, give it
   a label, and continue.
3. You're redirected to Google's consent screen → choose your account →
   **Allow**. Google redirects back to Postern and the destination
   shows as connected.
4. Click **Push** (or wait for the schedule) to send a backup. A
   `Postern Backups` folder appears in your Drive.

## 7. Restoring from Drive

On a fresh install you can restore **without** downloading the tarball
to your browser — the server pulls it from Drive directly:

1. **Settings → Backups → restore section → Browse "your Drive
   destination".** It lists the backups in your Drive folder with size
   and date.
2. Click **Restore** on the one you want. The server downloads it
   (multi-GB backups take a few minutes — leave the tab open).
3. Enter the **master password the backup was made with**, review the
   summary, and **Apply & restart**.

After the restart you log in with that backup's master password.

## Troubleshooting

??? failure "`redirect_uri_mismatch` on the consent screen"

    The redirect URI in Google Cloud Console doesn't match
    `POSTERN_GDRIVE_REDIRECT_URI` exactly. Check scheme (`https`),
    host, the `/api/backups/oauth/google/callback` path, and that
    neither side has a trailing slash. Fix whichever is wrong and
    retry — no container restart needed if you only changed the Console
    side.

??? failure "Backups stopped after about a week (`invalid_grant`)"

    Your OAuth consent screen is still in **Testing** mode, where
    Google expires refresh tokens after 7 days. Go to **OAuth consent
    screen → Publish app** (In production), then in Postern remove and
    re-add the Drive destination to mint a fresh token.

??? failure "`invalid_client` when connecting"

    The client ID/secret in `.env` don't match the OAuth client in
    Google Cloud Console (often a rotated secret). Re-copy both from
    **Credentials**, update `.env`, and recreate the container
    (Step 5). Re-adding the destination alone won't fix this.

??? failure "No 'Connect Google Drive' button"

    One of the three `POSTERN_GDRIVE_*` vars is missing/empty, or the
    container wasn't recreated after editing `.env`. Confirm all three
    are set (`sudo grep GDRIVE /opt/postern/.env`) and re-run Step 5.
