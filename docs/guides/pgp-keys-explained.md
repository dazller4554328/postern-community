---
title: PGP keys in depth (GnuPG)
---

# PGP keys in depth: generate your own with GnuPG

This is the **long-form** companion to
[Create a PGP key with GnuPG](create-pgp-key.md). The quick guide gives
you a one-liner; this one walks the **interactive** GnuPG flow prompt by
prompt and explains what each choice means — key type, length, expiry,
passphrase, revocation — so you understand the keypair you're creating
rather than just copying commands.

If you just want a working key fast, use the
[quick guide](create-pgp-key.md). If you read your mail in Postern, the
built-in [Set up PGP keys](pgp-keys.md) flow is easier still.

---

## What PGP actually is

**PGP** (Pretty Good Privacy) combines two kinds of cryptography:

- **Public-key crypto** for the keys you share. You get a **key pair**:
    - a **public key** — give this to anyone; they use it to *encrypt*
      messages to you and to *verify* your signatures.
    - a **private (secret) key** — keep this to yourself; it *decrypts*
      messages sent to you and *signs* your outgoing mail.
- **Symmetric crypto** under the hood for the bulk message encryption,
  which is faster.

The golden rule: **the public key is meant to be published; the private
key must never leave your control.**

---

## 1. Install GnuPG

GnuPG (`gpg`) is the standard OpenPGP toolkit. Install steps per platform —
Windows (Gpg4win), macOS (GPG Suite / Homebrew), Linux — are in the
[quick guide](create-pgp-key.md#1-install-gnupg). Confirm it's ready:

```bash
gpg --version
```

---

## 2. Generate your key pair (interactive)

Start the guided generator:

```bash
gpg --full-generate-key
```

GnuPG asks a series of questions. Here's what each one means:

**Key type.**

```text
Please select what kind of key you want:
   (1) RSA and RSA (default)
   (2) DSA and Elgamal
   (9) ECC (sign and encrypt) *default*
```

`RSA and RSA` is the safe, universally compatible default — pick it if
unsure. **ECC (Curve 25519)** is the modern alternative: smaller, faster
keys at equivalent strength, and what the quick guide's `future-default`
produces. Both are fine; RSA has the widest client support.

**Key length** (RSA only).

```text
What keysize do you want? (3072)
```

`2048` bits is perfectly adequate for everyday use. `4096` is stronger and
the common choice for a long-lived identity key, at the cost of slightly
slower operations. (ECC keys skip this question — the curve sets the
strength.)

**Expiry.**

```text
Please specify how long the key should be valid.
```

**Set an expiration** — `2y` (two years) is sensible. An expiry means a
lost or compromised key can't be abused forever, and you can always extend
it later. A non-expiring key lives forever, including after you've lost
control of it.

**Identity.** Enter your **real name** and the **email address** the key is
for (e.g. `billing@example.com`). The comment field can be left blank.

**Passphrase.** Finally, set a **strong, unique passphrase**. This encrypts
your private key on disk, so a stolen key file is useless without it. Store
it in your password manager — and don't leave it blank.

!!! tip "Generating on a server? Feed it entropy"
    Key generation needs randomness. On a headless box, moving data /
    running `ls -R /` in another terminal, or installing `rng-tools`, speeds
    it up if it stalls on "we need to generate a lot of random bytes".

---

## 3. View and list your keys

Confirm the public key exists:

```bash
gpg --list-keys
```

And that the matching secret key is present:

```bash
gpg --list-secret-keys
```

Each entry shows the key's **fingerprint** — the unique identifier you can
read aloud or compare to verify you have the right key.

---

## 4. Export your public key

This is the half you publish and hand out. ASCII-armored (`--armor`) makes
it copy-paste-friendly text:

```bash
gpg --armor --export youremail@example.com > publickey.asc
```

---

## 5. Export your private key (backup only)

Export the secret key **purely as a backup**, and guard it:

```bash
gpg --armor --export-secret-keys youremail@example.com > privatekey.asc
```

!!! danger "This file is the keys to the kingdom"
    Anyone with `privatekey.asc` **and** your passphrase can read every
    message encrypted to you and sign as you. Never email it, never upload
    it, never commit it. Keep it in encrypted storage (password manager,
    encrypted disk, offline USB).

---

## 6. Key management and security

A keypair isn't fire-and-forget. Three things worth doing:

**Create a revocation certificate now.** This lets you publicly mark the
key as dead if it's ever lost or compromised — generate it *before* you
need it, because it requires your passphrase:

```bash
gpg --output revoke.asc --gen-revoke youremail@example.com
```

Store `revoke.asc` somewhere safe and separate from the key. If disaster
strikes, import and publish it to tell the world to stop using the key.

**Publish the public key.** Upload `publickey.asc` to a keyserver so people
can find it — see
[Publish your public key](create-pgp-key.md#6-publish-your-public-key) for
the keys.openpgp.org upload-and-verify steps, and the
[WKD section](whmcs-pgp.md#recommended-wkd-on-your-domain) if you want
ProtonMail and other clients to discover it automatically.

**Maintain it.** Back the secret key up in more than one safe place, never
share it, and **renew before expiry** (`gpg --edit-key <email>` → `expire`)
rather than letting it lapse and having to start over.

---

## Where to go next

- **Use it with WHMCS:** [WHMCS PGP Mailer](whmcs-pgp.md) — publish the key
  and the add-on discovers it automatically.
- **Read encrypted replies:** import the secret key into your mail client
  (`gpg --import privatekey.asc`, or via Kleopatra / GPG Keychain).
- **Just want the short version:**
  [Create a PGP key with GnuPG](create-pgp-key.md).
