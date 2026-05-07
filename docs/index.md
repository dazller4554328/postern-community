---
title: Postern
hide:
  - navigation
  - toc
---

# Postern

## Why Postern

Postern is a Proton-style email client for people who don't want to
hand their keys to a provider. Your PGP private keys live on your
own machine — never on someone else's server — and
[Autocrypt](https://autocrypt.org) handles key exchange between you
and the people you write to, quietly and automatically.

It's a **client**, not a mail server. Postern keeps using whichever
IMAP provider you already have (Gmail, Fastmail, iCloud, anything),
and adds a private layer on top: encrypted local storage, your own
keys, and a UI you actually own.

If you want, Postern can pull **every** message off that provider
and store the only copy in your own SQLCipher-encrypted vault —
leaving nothing behind on Gmail, Fastmail, or wherever you came
from. After that, the provider becomes a relay: ferrying new mail
in and out while the archive lives on your hardware, not theirs.

Combine that with PGP and anything you send to another PGP user
(another Postern, Thunderbird, ProtonMail, anyone Autocrypt-aware)
is encrypted end-to-end. Your provider sees ciphertext and routing
headers — they can't read the body, and neither can anyone watching
the wire between them.

Reach it from anywhere over the free [Tailscale](https://tailscale.com)
mesh — no open ports, no public DNS, no monthly subscription, and
none of the "trust us" you get with a hosted service.

## What you get

<div class="grid cards" markdown>

-   :material-key-variant:{ .lg .middle } __Your keys, never theirs__

    ---

    PGP private keys live on your box. Autocrypt does the handshake
    with the people you email, so encryption "just works" between
    Postern users and any Autocrypt-compatible client.

-   :material-server-network:{ .lg .middle } __Bring any provider__

    ---

    Gmail, Fastmail, iCloud, your own Postfix — anything that speaks
    IMAP and SMTP. Postern doesn't replace your mail flow, it
    replaces the client reading it.

-   :material-shield-lock:{ .lg .middle } __Private by default__

    ---

    SQLCipher-encrypted vault on disk. Remote access over Tailscale's
    WireGuard mesh — no inbound ports, no exposed DNS. Free for
    personal use.

-   :material-currency-usd-off:{ .lg .middle } __No subscription__

    ---

    One-off Pro license or the Apache-2.0 Community build. No
    per-mailbox fees, no recurring billing, no "Plus tier required
    for X." Your hardware, your bill.

</div>

## Built-in AI, on your machine

![Postern with Datas open](assets/screenshots/postern_landing_datas_dark.png){ loading=lazy }

Postern Pro ships with **Datas** — an AI assistant that runs RAG
over your local mailbox. Ask "when did Joe last pay via PayPal?"
or "summarise my conversation with Sarah about Q3 budget" and it
answers with citations to actual messages.

Datas runs on whichever LLM you point it at. Default is
[Ollama](https://ollama.com) on your own machine for fully local,
nothing-leaves-the-box inference — but it also speaks the OpenAI
API and any compatible endpoint (Anthropic, OpenRouter, your own
proxy) if you'd rather trade some privacy for speed or model size.
Compose gets the same toolbox: voice dictation for hands-free
writing, Harper-powered grammar and spell check, and a "polish
this paragraph" button that rewrites just the selected text. None
of it is required — turn pieces on as you need them.

## On any device

<div class="md-landing-mobile" markdown>

<div markdown>
Postern is responsive top to bottom. Same inbox, same keys, same
encrypted vault — whether you're on a desktop, laptop, or phone
connected over Tailscale. Sender chips, folder counts, and Datas
all carry across.
</div>

<img
  src="assets/screenshots/postern_landing_dark_mobile.png"
  alt="Postern on mobile"
  loading="lazy"
/>

</div>

## Two editions

|                                       | **Postern Pro**                                | **Postern Community** |
| ------------------------------------- | ---------------------------------------------- | --------------------- |
| License                               | Paid (one-off, lifetime install, 3 yrs updates) | Apache 2.0            |
| Mailbox cap                           | Unlimited                                      | 3                     |
| Tailscale + remote access             | Yes                                            | No (localhost-only)   |
| Trusted-device sessions               | Yes                                            | No                    |
| AI features (Datas)                   | Yes                                            | No                    |
| VPN kill-switch (NordVPN / Mullvad)   | Yes                                            | No                    |
| Updates                               | Signed releases, in-app installer               | `git pull && ./install.sh --update` |

[Buy a Pro license](https://billing.postern.email){ .md-button .md-button--primary }
[Community source](https://github.com/dazller4554328/postern-community){ .md-button }

## Start here

- [Install Postern](install/index.md) — Tailscale-native path,
  ~10–15 minutes from a clean Ubuntu box.
- [Home server install](install/home-server.md) — same path on a
  NUC, mini-PC, or Pi 5 you already own.
- [Community edition](install/community.md) — OSS build,
  localhost-only.
- [Reference](reference/index.md) — storage invariants, migration
  paths, the bits worth knowing.
