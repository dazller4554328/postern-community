---
title: Create a PGP key with GnuPG
---

# Create a PGP key with GnuPG (Windows, macOS, Linux)

This guide creates a PGP keypair using **GnuPG** — the standard, free
OpenPGP toolkit — and publishes the public half so people can send you
encrypted mail and reply to you encrypted.

Use this when you need a key **outside** the Postern app: e.g. you run the
[WHMCS PGP Mailer](whmcs-pgp.md) standalone, or you simply prefer GnuPG /
Kleopatra. If you read your mail in Postern, the built-in
[Set up PGP keys](pgp-keys.md) flow is easier and needs none of this.

!!! info "What you'll end up with"
    - `publickey.asc` — your **public** key. Safe to share and publish.
    - `privatekey.asc` — your **secret** key. Decrypts mail sent to you.
      **Never share or upload this.** Back it up somewhere private.

---

## 1. Install GnuPG

=== "Windows"

    Install **[Gpg4win](https://www.gpg4win.org/)**. It bundles the `gpg`
    command line *and* **Kleopatra**, a graphical key manager. Accept the
    defaults during setup.

    Then open **PowerShell** (or **Command Prompt**) and confirm it works:

    ```powershell
    gpg --version
    ```

=== "macOS"

    Install **[GPG Suite](https://gpgtools.org/)** (includes the GPG
    Keychain GUI), or via [Homebrew](https://brew.sh/):

    ```bash
    brew install gnupg
    gpg --version
    ```

=== "Linux"

    GnuPG ships with most distributions. If it's missing:

    ```bash
    sudo apt install gnupg     # Debian / Ubuntu
    sudo dnf install gnupg2    # Fedora / RHEL
    gpg --version
    ```

---

## 2. Check the address isn't already published

Before making a key, confirm one isn't already live for that address.
Search for it at **[keys.openpgp.org](https://keys.openpgp.org/)**:

```text
billing@example.com
```

You should see **"No key found for email address"**. Good — that's the
expected starting point. (If a key *does* show up, you already have one
published; only make a new one if you've lost access to it.)

---

## 3. Generate the keypair

=== "Command line (any OS)"

    Replace the name and address with your own. This makes a modern
    Ed25519 sign+encrypt key that expires in two years:

    ```bash
    gpg --quick-generate-key "Billing <billing@example.com>" \
        future-default default 2y
    ```

    A window (or prompt) appears asking you to **set a passphrase** —
    **enter a strong one** and save it in your password manager. It
    encrypts your secret key, so a stolen key file is useless without it.
    Don't leave it blank.

    !!! tip "Want to choose the options yourself?"
        Run `gpg --full-generate-key` instead for the interactive version
        (key type, size, expiry, name, email — one prompt at a time).

=== "Windows — Kleopatra (GUI)"

    1. Open **Kleopatra** → **File → New Key Pair** → **Create a personal
       OpenPGP key pair**.
    2. Enter your **name** and the **email address** (e.g.
       `billing@example.com`), then **Next** → **Create**.
    3. When prompted, **set a passphrase** and keep it safe.

    Kleopatra also handles export and upload in later steps via right-click
    on the key — the command-line equivalents are shown below.

---

## 4. Confirm the key exists

```bash
gpg --list-keys
```

You should see your new key listed with its name and email.

---

## 5. Export your keys

**Public key** — this is the one you publish and share:

```bash
gpg --armor --export billing@example.com > publickey.asc
```

**Secret key** — your private backup. Store it somewhere safe and private
(password manager, encrypted drive). **Never upload or send this:**

```bash
gpg --armor --export-secret-keys billing@example.com > privatekey.asc
```

!!! danger "Keep the secret key secret"
    Anyone with `privatekey.asc` **and** your passphrase can read every
    encrypted message sent to you. Treat it like a master password — back
    it up, but never publish it, email it, or commit it anywhere.

---

## 6. Publish your public key

So others can find your key automatically, upload the **public** key to a
keyserver:

1. Go to **[keys.openpgp.org/upload](https://keys.openpgp.org/upload)**.
2. Upload your **`publickey.asc`** file.
3. Click **Send verification email**.
4. Open the email and **click the verification link** — this is what makes
   your address searchable on the keyserver.

After verifying, search [keys.openpgp.org](https://keys.openpgp.org/) for
your address again — it should now return your key.

!!! tip "For ProtonMail and zero-touch discovery: also set up WKD"
    A keyserver covers most clients, but **ProtonMail** only auto-encrypts
    to addresses it can discover via **WKD** (a key served from your own
    domain). If you control the domain in your address, follow the WKD
    steps in the
    [WHMCS PGP Mailer guide](whmcs-pgp.md#recommended-wkd-on-your-domain) —
    they work for any address, not just WHMCS.

---

## Using the key

- **WHMCS PGP Mailer:** publishing is all you need — the add-on discovers
  the published key automatically. See
  [Receiving encrypted replies](whmcs-pgp.md#receiving-encrypted-replies-publish-your-system-key).
- **Reading encrypted replies:** import your **secret** key into the mail
  client you read that inbox in (Thunderbird, Kleopatra, Apple Mail with
  GPG Suite, etc.):

    ```bash
    gpg --import privatekey.asc
    ```

- **Moving to another computer:** copy `privatekey.asc` over securely and
  `gpg --import` it there.

---

Lost the passphrase or the secret key? You can't recover encrypted mail
sent to that key — generate a new keypair and publish it, and ask senders
to use the new one.
