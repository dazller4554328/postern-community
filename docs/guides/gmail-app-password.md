---
title: Create a Gmail app password
---

# Create a Gmail app password

Gmail won't let third-party email clients (Postern, Thunderbird,
Apple Mail, anything that speaks IMAP) sign in with your normal
account password. Since 2022, Google requires either OAuth — which
isn't available to small projects without a Google verification
review — or a **16-character app password** generated specifically
for that one client.

This guide walks through generating that app password and pasting
it into Postern. Three minutes, no command line.

---

## What you need first

- A Gmail account.
- **2-Step Verification turned on.** Google hides the app-password
  page entirely if 2FA isn't enabled. If you haven't done that yet,
  open [myaccount.google.com/signinoptions/two-step-verification][2sv]
  and finish that flow first — it takes another two minutes.

---

## 1. Open Google Account settings

Sign in to Gmail in a browser, then go to
**[myaccount.google.com](https://myaccount.google.com)**.

This is the central settings page for your Google identity, not
Gmail's own settings panel. App passwords live here.

---

## 2. Search for "App passwords"

Use the search bar at the top of the Google Account page and type
**App passwords**.

The search result is the fastest way in — Google reshuffles their
menus often, so a deep-link to the panel sometimes 404s, but the
search box always finds it.

Click the **App passwords** result.

---

## 3. Re-verify your password

Google will ask you to re-enter your Google account password before
showing the app-password page. This is normal — it's a sensitive
screen.

If the page says **"This setting is not available for your account"**:

- 2-Step Verification isn't enabled. Turn it on at
  [myaccount.google.com/signinoptions/two-step-verification][2sv]
  and come back.
- Or you're on a Google Workspace account where the admin has
  disabled less secure access. You'll need to ask the admin to
  enable app passwords, or use OAuth instead.

---

## 4. Name the app password

You'll see a single text field labelled **App name**.

Type something memorable — `Postern` is fine. The name is purely a
label so you can find this entry later in the list of app passwords
and revoke it if you stop using Postern. Google does not care what
you type.

Click **Create**.

---

## 5. Copy the 16-character password

Google now shows a yellow box containing a password like:

```
abcd efgh ijkl mnop
```

Copy it now. **Google only shows this once** — close the window and
you'll have to delete this entry and create a new one.

The spaces are decorative. Postern accepts the password with or
without spaces — Google strips them on the server side.

---

## 6. Paste it into Postern

In Postern's mailbox setup wizard:

| Field | Value |
|---|---|
| **Provider** | Gmail (the IMAP/SMTP host + port are auto-filled) |
| **Email** | `you@gmail.com` (your real Gmail address) |
| **Password** | Paste the 16-character app password — **not** your normal Gmail password |

Click **Save**. Postern connects to Gmail's IMAP server, your inbox
syncs, and you're done.

---

## Revoking an app password later

If you uninstall Postern, change devices, or just want to clean up:

1. Back to [myaccount.google.com](https://myaccount.google.com) →
   search **App passwords**.
2. You'll see a list of every app password you've ever created,
   labelled with whatever you typed in step 4.
3. Click the trash icon next to **Postern**.

The next time Postern tries to sync, IMAP will fail with an
authentication error — exactly what you want.

---

## Why this is needed

Google deprecated "less secure app access" in May 2022. Since then,
the only ways for an IMAP/SMTP client to authenticate against a
Gmail account are:

- **OAuth 2.0** with a Google-verified app. The verification process
  involves a security audit, takes weeks, and is geared toward
  public-facing SaaS — not self-hosted clients.
- **App passwords**, scoped to a single client and revocable
  individually.

App passwords are the path the Postern team recommends for
self-hosted use: you own the credential, you can revoke it
independently of your main account password, and there's nothing to
verify with Google.

---

## Other providers

The other big webmail providers handle this differently:

- **Fastmail / iCloud** — same pattern, different settings page.
  Fastmail calls them "app passwords" too; iCloud calls them
  "app-specific passwords."
- **Outlook.com / Office 365** — supports OAuth (Postern's
  Microsoft OAuth support is on the roadmap). App passwords are
  also available but Microsoft is phasing them out for accounts
  with modern security defaults.
- **ProtonMail** — needs the
  [ProtonMail Bridge](https://proton.me/mail/bridge) running on your
  Postern host. The Bridge exposes a local IMAP/SMTP listener that
  your Gmail-style password works against.

[2sv]: https://myaccount.google.com/signinoptions/two-step-verification
