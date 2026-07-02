---
title: WHMCS PGP Mailer
---

# Postern PGP Mailer for WHMCS

**Postern PGP Mailer** is a WHMCS add-on that automatically PGP-encrypts
your outbound WHMCS email — invoices, password resets, support replies,
even custom messages — to any recipient whose public key is known or
discoverable. Everyone else keeps receiving normal mail. It uses the same
OpenPGP approach as the Postern email client, but runs as a standalone
WHMCS module: you don't need to run Postern to use it.

This guide covers installing it, activating your license, and turning on
encryption.

!!! info "Where to buy"
    Postern PGP Mailer is a one-off **£50** purchase for a single WHMCS
    installation. Get it from
    [postern.email/whmcs-pgp](https://postern.email/whmcs-pgp); your
    license key arrives by email after checkout.

---

## How it works

It ships as **two pieces** with a clean split:

| Component | Where | Role |
|---|---|---|
| **Mail Provider** | `modules/mail/PosternPgp/` | The engine. Selected under **System Settings → Mail**, it sits in the transport path for *all* outbound mail, encrypts when it can, and sends over the SMTP you configure here. |
| **Add-on** | `modules/addons/posternpgp/` | Key management — the recipient key list, opportunistic-discovery toggle, and the license. No SMTP here. |

```text
WHMCS  →  Mail Provider "Postern PGP"  →  single keyed recipient?
                                          ├─ yes → PGP/MIME encrypt → SMTP
                                          └─ no  → send plaintext   → SMTP
```

Encryption is **best-effort**: if a message can't be encrypted it falls
back to a normal send, so mail never fails to go out.

---

## Requirements

- **WHMCS 8.6+** (the Mail Provider API).
- **PHP 8.1–8.4.**
- **A current ionCube Loader — v15 or newer.** The module ships
  ionCube-encoded, so the loader must be enabled. It's free and **already
  present on most WHMCS hosts** (WHMCS itself uses it); if it's missing or
  out of date, install the latest from
  [get-loader.ioncube.com](https://get-loader.ioncube.com) and restart PHP.

    !!! warning "An out-of-date loader is the most common install problem"
        An *old* loader produces *"cannot be decoded by this version of the
        ionCube Loader"* — that's a loader problem on the server, not a
        problem with the files. Updating it is free and is usually a host
        setting or a quick support request. Note that a newer PHP needs a
        newer loader regardless (PHP 8.3 needs v13+, PHP 8.4 needs v14+), so
        keeping the loader current is the safe default.

- The **PHP `gnupg` extension**:

    ```bash
    sudo apt install php-gnupg     # then restart PHP-FPM / Apache
    ```

    Without it the module still loads, but every message sends in
    plaintext until gnupg is available.

!!! tip "Check before you install"
    The download includes **`requirements-check.php`** — a small,
    *unencoded* script that runs even when the loader is missing or too old.
    Upload it to your WHMCS root, open it in a browser, and it reports your
    PHP version, ionCube Loader version, and `gnupg` status with a clear
    pass/fail for each. Delete it once you're done.

---

## 1. Install the module

1. Unzip the download and copy **both** directories into your WHMCS
   install, preserving the paths:

    ```text
    modules/addons/posternpgp/
    modules/mail/PosternPgp/
    ```

    === "SSH / FTP (uploads folders directly)"

        Copy the two directories straight into your WHMCS root so they land
        at the paths above.

    === "cPanel / CWP File Manager (can't upload folders)"

        Most file managers won't upload a folder — only files and zips. The
        download includes a per-folder archive for exactly this:

        1. Upload **`posternpgp.zip`** into `modules/addons/`, then
           **Extract** it there.
        2. Upload **`PosternPgp.zip`** into `modules/mail/`, then
           **Extract** it there.
        3. Delete the leftover `.zip` files once extracted.

2. Make the add-on's storage directory writable by the web user:

    ```bash
    chown -R www-data:www-data modules/addons/posternpgp/storage
    ```

3. In WHMCS, go to **System Settings → Addon Modules**, find
   **Postern PGP Mailer**, and click **Activate**.

---

## 2. Activate your license

1. On the **Postern PGP Mailer** addon, click **Configure** (top-right).
2. Paste the **license key** from your order email into the
   **License key** field and **Save Changes**.
3. Open the addon's admin page (**Addons → Postern PGP Mailer**). It
   should show **License active for this installation**.

!!! note "One license, one installation"
    Your key locks to this WHMCS **domain** on first activation.
    Encryption stays disabled until the license reads *active* — the
    Mail Provider simply sends plaintext in the meantime.

!!! tip "Moved servers?"
    If you migrate to a new domain or server, open the addon's admin
    page and click **Move license to this server** to re-bind your key.

!!! tip "Coming from the free trial?"
    If you trialed Postern PGP Mailer and then bought the full license,
    you'll receive a **new license key**. You don't reinstall or
    reconfigure anything: just open **Addons → Postern PGP Mailer →
    Configure**, replace the trial key with your new one, and save. Your
    SMTP settings and stored recipient keys are preserved, and encryption
    resumes immediately.

---

## 3. Select the mail provider

1. Go to **System Settings → Mail**.
2. Set the **Mail Provider** to **Postern PGP (encrypting mailer)**.
3. Enter your **SMTP** details (host, port, security, username,
   password). These live here, in the provider — the add-on holds no
   SMTP credentials.
4. Send the test message to confirm the connection.

!!! tip "Use an app password, not your normal login password"
    Most providers with two-factor authentication — **Gmail, Yahoo,
    Outlook / Microsoft 365, iCloud** — reject your account login
    password over SMTP. Generate an **app password** in that account's
    security settings and use it as the SMTP password here. For Gmail,
    see [Gmail app password](gmail-app-password.md); the others follow
    the same idea ("app password" / "app-specific password" in their
    account security pages).

From now on, every outbound WHMCS email runs through Postern PGP.

---

## 4. Recipient keys

A message is encrypted only when there's a public key for the recipient.
There are two ways keys get there:

- **Opportunistic discovery (recommended).** On the addon admin page,
  enable **Opportunistic discovery**. When an outbound email has no
  stored key for the recipient, the module looks them up at send time
  via **keys.openpgp.org** and **WKD**, encrypts if a key is published,
  and caches the result. Misses are remembered for a week so non-PGP
  recipients don't trigger a lookup on every send.

- **Manual keys.** Add a recipient's armored public key by hand under
  **Add / update a recipient key**. Manual keys always take precedence
  over discovery.

!!! tip "Invite clients to enable PGP"
    Turn on the **plaintext “enable PGP” footer** to append a short note
    to *unencrypted* mail, inviting recipients to publish a key.
    Encrypted mail never includes it.

---

## Receiving encrypted replies (publish your system key)

Everything above is *outbound* — you encrypting to clients. The natural
next question is whether they can reply **encrypted back to you**.

**In most cases they already can, with nothing extra to set up.** Every
message Postern sends carries your public key with it (in the Autocrypt
header), so the recipient's client can pick your key straight out of your
email. **Thunderbird, Enigmail, gpg and most OpenPGP clients** do exactly
that — the user accepts your key once (usually a one-click prompt) and
their replies to you are encrypted from then on.

The exception is **ProtonMail**. Proton ignores the key attached to a
message and only encrypts to an external contact whose key it can
**discover on its own** — via a keyserver or, best, **WKD on your domain**.
So Proton shows "this contact uses PGP" but won't auto-encrypt its replies
until you publish your key one of the two ways below.

| Recipient's client | Replies encrypt without publishing? |
|---|---|
| Thunderbird / Enigmail / most OpenPGP clients | ✅ Yes — key travels in your message |
| ProtonMail | ❌ No — needs a keyserver or WKD (below) |

Publishing the public key for your WHMCS system address
(`billing@example.com`) closes that gap and makes it automatic for
*everyone*, Proton included. The addon's admin page shows whether the
address has a discoverable key, with a **Check now** button to confirm
once it's live. First you create the keypair, then you publish its public
half — two ways, both below.

### First, create the keypair

The add-on doesn't generate keys — it only discovers and stores keys that
are already published. You make the keypair **wherever you'll read this
mailbox's replies**, because the matching *secret* key is what decrypts
them. Only the **public** key is ever published.

=== "In Postern (easiest)"

    If you read `billing@example.com` in the Postern email client:

    1. Add `billing@example.com` as a mailbox in Postern.
    2. Go to **Settings → PGP** and click **Generate** (or **Import** an
       existing key).
    3. Click **Publish** to push the public key out, then jump to **Check
       now** on the addon page. Postern's *Publish* handles the keyserver
       step for you; for WKD, follow the steps further down.

=== "With gpg (standalone, no Postern)"

    On any machine with `gpg` — this is the route if you run the addon
    without the Postern client. For platform install steps (Windows /
    macOS / Linux) and a fuller walkthrough, see
    [Create a PGP key with GnuPG](create-pgp-key.md).

    ```bash
    # generate a modern Ed25519 sign+encrypt keypair, 2-year expiry
    # (you'll be asked to set a passphrase — keep it safe)
    gpg --quick-generate-key "Billing <billing@example.com>" \
        future-default default 2y

    # the PUBLIC key — this is what you publish
    gpg --armor --export billing@example.com > billing-public.asc

    # the SECRET key — back it up somewhere private; it decrypts your
    # replies. NEVER publish or upload this one.
    gpg --armor --export-secret-keys billing@example.com > billing-secret.asc
    ```

    Keep `billing-secret.asc` in whatever mail setup you use to *read* that
    inbox. Publish only `billing-public.asc`, using one of the methods
    below.

!!! note "The secret key never goes on the WHMCS server"
    The add-on only *encrypts to your recipients* — it never signs with
    your system key, so it needs nothing secret. Your system keypair exists
    purely so people can encrypt replies *to* you; you decrypt those
    wherever you read the mailbox.

### Quick: a public keyserver

Upload your system address's public key to
[keys.openpgp.org](https://keys.openpgp.org) and confirm the verification
email it sends. Most clients check it and it needs no server access — but
some clients (Proton included) treat a key served from **your own domain**
as more authoritative, which is what WKD below gives you.

### Recommended: WKD on your domain

**WKD (Web Key Directory)** serves your key from your email domain's own
web server, so any client — Proton, Thunderbird, gpg — discovers it
automatically and trusts it as authoritative. This is the most reliable
way to make Proton auto-encrypt replies to you.

You set this up **once**, on the web server for your email domain — the
domain *after* the `@` in your system address. You'll need shell access to
that server and `gpg` with your system key available.

1. **Find your key's WKD hash.** It's derived from the localpart (the
   part before the `@`) and needs no keyring:

    ```bash
    gpg-wks-client --print-wkd-hash billing@example.com
    ```

    It prints `<hash> billing@example.com` — that `<hash>` is the filename
    WKD expects.

    !!! note
        `gpg --list-keys --with-wkd-hash` also shows it, but **only after
        the key is imported into that machine's local keyring** — on a
        fresh box it just errors with *"No public key"*. Use
        `--print-wkd-hash` above, which doesn't need the key.

2. **Place the key in the web root** as a **binary** file (not
   ASCII-armored) at the WKD path. If you exported your system key's
   *public* key from Postern as an `.asc` file, just dearmor it — no
   keyring needed:

    ```bash
    ROOT=/var/www/example.com      # your domain's document root
    HASH=<hash>                    # from step 1
    mkdir -p "$ROOT/.well-known/openpgpkey/hu"
    gpg --dearmor < billing-public.asc \
        > "$ROOT/.well-known/openpgpkey/hu/$HASH"
    : > "$ROOT/.well-known/openpgpkey/policy"   # required, may be empty
    chmod -R a+rX "$ROOT/.well-known"
    ```

    (If the key is already in this server's gpg keyring, use
    `gpg --no-armor --export billing@example.com` in place of the
    `gpg --dearmor …` line.)

3. **Serve the files as `application/octet-stream` over HTTPS**, and allow
   cross-origin reads so web-based clients can fetch them:

    === "Apache"

        `.htaccess` in `.well-known/openpgpkey/`:

        ```apache
        <Files "*">
          ForceType application/octet-stream
          Header set Access-Control-Allow-Origin "*"
        </Files>
        ```

    === "Nginx"

        ```nginx
        location /.well-known/openpgpkey/ {
            default_type application/octet-stream;
            add_header Access-Control-Allow-Origin "*";
        }
        ```

4. **Verify with a real client.** If gpg discovers and imports the key,
   Proton will too:

    ```bash
    gpg --locate-external-keys --auto-key-locate clear,wkd billing@example.com
    ```

    `gpg-wks-client --check billing@example.com` is another quick check.

Once it resolves, click **Check now** on the addon admin page. From then
on, Proton and other PGP clients auto-encrypt their replies to your system
address — with no per-contact setup on the sender's side.

!!! tip "If the direct path doesn't resolve: the advanced method"
    Some hosts or CDNs serve `.well-known` paths inconsistently. WKD also
    defines an *advanced* layout on an `openpgpkey.` subdomain:
    `https://openpgpkey.example.com/.well-known/openpgpkey/example.com/hu/<hash>`.
    Point `openpgpkey.example.com` at the same server and place the key
    under `…/openpgpkey/example.com/hu/<hash>` (note the extra `example.com`
    segment). Clients try this layout before the direct one.

!!! warning "WKD only works on a domain you control"
    WKD must be served from your *email domain's* web server, so it works
    for addresses on your own domain (`billing@example.com`) but **not**
    for `@gmail.com`, `@outlook.com`, or any domain you don't run — you
    can't add `.well-known/openpgpkey/` to their servers. If your system
    address is on one of those, use the keyserver method above, or move
    your WHMCS system address to an address on your own domain.

---

## Troubleshooting

??? failure "\"the ionCube Loader needs to be installed\""

    The module is ionCube-encoded, so the loader must be enabled — and
    **for the PHP version WHMCS runs on**, not just the default one. Get
    the free loader from
    [get-loader.ioncube.com](https://get-loader.ioncube.com) and restart
    PHP. Most WHMCS hosts already have it. Run **`requirements-check.php`**
    (included in the download) to confirm.

??? failure "\"cannot be decoded by this version of the ionCube Loader\""

    Your server has an ionCube Loader, but it's **older than the one the
    module is encoded for**. This is a server-side loader problem, not a
    problem with the files — and re-downloading won't change it.

    Update the loader to the current version (free) from
    [get-loader.ioncube.com](https://get-loader.ioncube.com) and restart PHP.
    On cPanel/CWP this is usually a one-line host setting or a quick support
    request. To see your current version, run **`requirements-check.php`**
    from the download, or check **WHMCS → Utilities → System → PHP Info**
    and search for *ionCube*.

    !!! note "Why we don't ship for older loaders"
        Encoding for older loaders would weaken the licensing protection
        (public decoders exist for old loader formats), and it wouldn't help
        anyway — a newer PHP version requires a newer loader regardless. The
        fix is always to update the loader.

??? failure "\"gnupg extension not loaded\" (mail sends as plaintext)"

    Install the PHP `gnupg` extension and restart PHP. On servers with
    **more than one PHP version** (common with Apache + PHP-FPM),
    `apt install php-gnupg` installs it for the *default/newest* version —
    which is often **not** the one WHMCS uses. Install it for WHMCS's
    exact version and restart **that** FPM pool:

    1. Find WHMCS's PHP version — from your site's Apache vhost (the
       `php8.x-fpm.sock` it proxies PHP to) or **WHMCS → Utilities →
       System → PHP Info** (the version at the top).
    2. Install the matching package and restart that pool — example for
       **PHP 8.2**:

        ```bash
        sudo apt install php8.2-gnupg
        sudo systemctl restart php8.2-fpm
        ```

    3. Reload the addon's admin page — the crypto backend should now read
       as ready.

??? failure "\"License Server Down\""

    The license server couldn't be reached. Check that outbound HTTPS is
    allowed from your WHMCS host, then reload the admin page.

??? failure "\"Wrong product\""

    The key you entered is for a different Postern product. Use the key
    from your **Postern PGP Mailer** order.

??? failure "\"In use on another domain\""

    The license is bound to a different installation. Click **Move
    license to this server** on the admin page to re-bind it here.

??? failure "Mail sends but isn't encrypted"

    Either there's no key for that recipient (enable **opportunistic
    discovery**), the recipient hasn't published a PGP key, or `gnupg`
    isn't loaded. Messages with multiple recipients are always sent
    plaintext by design.

---

Still stuck? Reply to your order email and we'll help you get set up.
