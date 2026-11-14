---
title: Set up PGP keys
---

# Set up PGP keys

Postern has a full OpenPGP keyring built in, at **Settings → PGP**
(`/settings/pgp`). You don't need GnuPG, a browser extension, or any
external tool — Postern generates keys, stores the secret half in its
encrypted vault, discovers other people's public keys automatically,
and encrypts or signs mail at compose time.

This guide walks through the whole panel: making your own key,
publishing it so people can write to you, importing a key you already
have, and finding keys for the people you mail.

---

## The mental model

There are two kinds of key in the keyring, and the page splits them
into two lists:

- **My keys** — keypairs you hold the *secret* half of. These are
  your identities: the secret key decrypts mail sent to you and signs
  mail you send. Marked with a red **SECRET** pill.
- **Contacts** — *public-only* keys for other people. You encrypt
  *to* these. Postern harvests them automatically (see
  [Finding other people's keys](#4-find-someone-elses-public-key)),
  so this list fills itself in over time.

Secret material never leaves this Postern node unless you explicitly
export or back it up.

---

## 1. Generate your keypair

If you don't already have a PGP key, make one here. Scroll to
**Generate your keypair**.

| Field | Value |
|---|---|
| **User ID** | Your name and email in the standard format: `Your Name <you@example.com>` |

Use the **same email address as the mailbox** you want to send
encrypted mail from — that's what lets recipients (and Postern's own
auto-encryption) match the key to the address.

Click **Generate**. Postern creates an **Ed25519 + ECDH** keypair
(modern elliptic-curve, the same kind Proton and current GnuPG
default to) with **no passphrase** — the secret key is protected by
Postern's vault and your login, not a separate passphrase you'd have
to type on every send.

The new key appears at the top of **My keys** with a **generated
here** badge.

!!! tip "During first-time setup"
    You can also create or import a key in step 3 of the mailbox setup
    wizard. Picking **Skip for now** there is fine — you can always
    come back to Settings → PGP later, and Postern still
    auto-discovers and auto-encrypts to other people regardless.

---

## 2. Publish your key so people can write to you

A keypair on its own only lets *you* read mail others encrypt to you
— but they need your *public* key first. The easiest way to hand it
out is to publish it to the public keyserver.

On your secret key's row, click **Publish**. Postern uploads the
public half to **keys.openpgp.org** (the privacy-respecting "Hagrid"
keyserver) and starts the verification flow:

1. The keyserver emails a verification link to your address.
2. Open that mail and click the link from `keys@keys.openpgp.org`.
3. Your key becomes publicly retrievable — clients like Proton, K-9
   Mail, Thunderbird, and GnuPG can now find it by your email address.

Until you click that link, the key is uploaded but not discoverable.

### Check who can find you

Click **Scan keyserver** in the toolbar above the lists. Postern
checks every mailbox you've configured against keys.openpgp.org and
shows, per address:

- **✓ published** — clients can find your key.
- **✗ no public key found** — click **Publish** on your key.
- **? couldn't reach the keyserver** — transient network issue, try
  again.

---

## 3. Import a key you already have

Already have a PGP key from GnuPG, Proton, or another client? Scroll
to **Import existing key** and paste the **armored** key block into
the box — the text that starts with one of:

```
-----BEGIN PGP PUBLIC KEY BLOCK-----
```
```
-----BEGIN PGP PRIVATE KEY BLOCK-----
```

Then click **Import**.

- Paste a **public** key → it's added to your **Contacts** list.
- Paste a **private** key → Postern extracts the public half *and*
  stores the secret half in the vault, so it shows up under **My
  keys** as a full identity you can decrypt and sign with.

Exporting from GnuPG, for reference:

```bash
# public key
gpg --armor --export you@example.com

# private key (handle with care)
gpg --armor --export-secret-keys you@example.com
```

---

## 4. Find someone else's public key

To encrypt mail *to* someone, you need their public key. Most of the
time Postern finds it for you automatically — but you can also look it
up on demand.

Scroll to **Find someone's public key**, enter the recipient's email
address (e.g. `alice@example.com`), and click search. Postern checks,
in order:

1. **WKD** (Web Key Directory) — the key published on the recipient's
   *own domain*. The most trustworthy source.
2. **keys.openpgp.org** — the public keyserver.

If a key is found, you'll see where it came from; click **Add to my
keyring** to save it to **Contacts**. If nothing is found, the
recipient needs to publish a key, or send you a signed message first
(see below).

### Keys Postern collects on its own

You usually won't need the manual lookup, because Postern harvests
contact keys passively. Each key's badge shows where it came from:

| Badge | Meaning |
|---|---|
| **via WKD** | Looked up on the sender/recipient's domain |
| **via Autocrypt** | Pulled from the `Autocrypt` header of a message they sent you |
| **keyserver** | Fetched from keys.openpgp.org |
| **imported** | You pasted it in manually |
| **generated here** | Your own key, made in Postern |

The Contacts list grows quickly once Autocrypt and WKD have been
running for a while — use the **filter** box to search by name,
email, or fingerprint, and the list paginates automatically.

---

## 5. Encrypt and sign when composing

With keys in place, the compose window gains two toggles:

- **Encrypt** — turns on PGP encryption for the message. Postern
  shows a status next to it:
    - **auto** — every recipient has a public key, encryption is ready.
    - **!** — at least one recipient has no key found; hover to see
      which addresses are missing. You can't encrypt until every
      recipient has a key.
    - **…** — Postern is looking up recipient keys right now.
- **Sign** — attaches your cryptographic signature so recipients can
  verify the message really came from you (and wasn't tampered with).

When you turn **Encrypt** on, Postern runs a live WKD/keyserver lookup
for any recipient it doesn't already have a key for, so the status
reflects the real, current state.

---

## Managing keys

Each key row has actions:

- **Export** — download a single key as an `.asc` file (named by its
  fingerprint).
- **Publish** *(your keys only)* — upload to keys.openpgp.org (see
  [step 2](#2-publish-your-key-so-people-can-write-to-you)).
- **Delete** — remove the key. Deleting a **SECRET** key means you can
  no longer decrypt mail encrypted to it — **export or back it up
  first**.

### Back up the whole keyring

The toolbar above the lists has two backup buttons:

- **Download public keys** — bundles every public key into one `.asc`.
  Safe to store anywhere.
- **Download backup (incl. private)** — bundles *everything*,
  including your `PRIVATE KEY BLOCK` sections. Anyone who gets this
  file can read your mail and impersonate you — store it on an
  encrypted disk, in a password manager, or on an offline USB. Postern
  asks you to confirm before producing it.

!!! warning "Your secret keys live in the vault"
    Postern keeps secret keys inside its encrypted vault on the
    server. A **private** backup is the only copy that exists outside
    that vault — if you ever rebuild the server without restoring the
    vault, that backup file is how you get your identity back. Keep
    one.

---

## Why no passphrase?

Traditional PGP protects the secret key with a passphrase you type on
every decrypt. Postern instead protects secret keys with its
**at-rest vault encryption** (tied to your server's key and your
login). For a single-user, self-hosted client that's the better
trade-off: no passphrase to forget or re-type per message, and the
secret key is still encrypted on disk. If you want a passphrase-locked
key for use elsewhere, generate it in GnuPG and
[import it](#3-import-a-key-you-already-have).
