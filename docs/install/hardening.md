---
title: Lock the server down
---

# Lock the server down (Tailscale-only, no open ports)

Once Postern is up and reachable over Tailscale (and optionally
Funnel), the goal is simple: **nothing on this box should be reachable
from the public internet except through Tailscale.** This page sets up
SSH keys, disables password login, and closes every inbound port with a
firewall — without locking yourself out.

## The good news: Postern needs zero open inbound ports

- **Postern itself** binds to `127.0.0.1:8080` only — it is never
  published to a public interface. You reach it through the Tailscale
  sidecar, not a host port.
- **Tailscale** is outbound-only. It makes a WireGuard connection *out*
  to the tailnet and traverses NAT on its own; it needs **no inbound
  port opened** on your firewall.
- **Funnel** ingress arrives *through* that same Tailscale tunnel from
  Tailscale's relay infrastructure — it also needs **no inbound port**
  on the host.

So the only inbound service you have to think about is **SSH (port
22)** — and we're going to move that onto Tailscale too and close it
publicly.

!!! danger "Read this before you touch the firewall"
    A firewall mistake locks you out of your own server. Before
    enabling anything below:

    1. Set up SSH keys **and confirm key login works** (Step 1).
    2. Confirm you can reach the box over **Tailscale** (`ssh
       ubuntu@100.x.y.z` using the tailnet IP from `tailscale ip -4`).
    3. Keep your VPS provider's **web console / VNC** tab open — that's
       your recovery path if SSH ever fails. It connects below the
       firewall, so it always works.

    Do the steps **in order**. Don't enable the firewall until
    Tailscale SSH is proven.

## Step 1 — SSH keys (and prove they work)

On **your laptop** (not the server), generate a key if you don't have
one:

```bash
ssh-keygen -t ed25519 -C "you@laptop"
# press enter for the default path (~/.ssh/id_ed25519)
```

Copy the **public** key to the server:

```bash
ssh-copy-id ubuntu@<server-public-ip>
# or, if ssh-copy-id isn't available:
cat ~/.ssh/id_ed25519.pub | ssh ubuntu@<server-public-ip> \
  'mkdir -p ~/.ssh && chmod 700 ~/.ssh && cat >> ~/.ssh/authorized_keys && chmod 600 ~/.ssh/authorized_keys'
```

Now **open a new terminal** and confirm key login works *without a
password prompt*:

```bash
ssh ubuntu@<server-public-ip>
```

If that drops you straight in, keys are working. **Do not proceed
until this succeeds** — the next step disables passwords.

## Step 2 — Disable password & root SSH login

On the **server**, edit the SSH config:

```bash
sudo install -d /etc/ssh/sshd_config.d
sudo tee /etc/ssh/sshd_config.d/10-postern-hardening.conf >/dev/null <<'EOF'
PasswordAuthentication no
KbdInteractiveAuthentication no
PermitRootLogin no
EOF
sudo systemctl restart ssh
```

Keep your current session open and test a fresh login in another
terminal before trusting it. Password and root logins are now refused;
only your key works.

## Step 3 — SSH over Tailscale, then close public port 22

The cleanest end state is SSH that only works *over the tailnet*, so
port 22 can be closed to the world entirely.

**Option A — Tailscale SSH (recommended).** Let Tailscale broker SSH;
no public 22 at all:

```bash
sudo tailscale up --ssh
```

Then from your laptop (also on the tailnet) confirm:

```bash
ssh ubuntu@<tailnet-ip>      # the 100.x.y.z from `tailscale ip -4`
```

**Option B — keep OpenSSH, but only on the Tailscale interface.** Skip
`--ssh` and instead let the firewall (Step 4) allow 22 *only* on
`tailscale0`.

Either way, confirm you can SSH over the **tailnet IP** before closing
the public port.

## Step 4 — Firewall: deny all inbound, allow only Tailscale

Ubuntu/Debian ship `ufw`. This config blocks every inbound connection
from the public internet while leaving tailnet traffic and all outbound
traffic (IMAP/SMTP, updates, backups) working.

```bash
sudo ufw default deny incoming
sudo ufw default allow outgoing

# Allow anything arriving over the Tailscale interface (covers SSH
# over the tailnet, and is harmless for everything else).
sudo ufw allow in on tailscale0

# OPTION B ONLY — if you did NOT enable Tailscale SSH and want OpenSSH
# reachable over the tailnet, the line above already covers it. Do NOT
# add a global `allow 22` — that re-opens it to the world.

sudo ufw enable        # answer 'y' — your current session survives,
                       # SSH is allowed on tailscale0
sudo ufw status verbose
```

!!! warning "Last chance to avoid a lockout"
    `ufw enable` activates the rules immediately. Before running it,
    re-read Step 0's danger box: you must already be able to SSH over
    the **tailnet IP**, and you should have the provider console open.
    If `ufw status` looks wrong, you can always
    `sudo ufw disable` from the provider console.

### Note on Docker and ufw

Docker normally writes its own iptables rules that can bypass `ufw`
for **published** ports. Postern's compose only publishes
`127.0.0.1:8080` (loopback), so there's nothing for Docker to expose
publicly — no special handling needed. Just don't change that bind to
`0.0.0.0` in `docker-compose.yml`.

## Step 5 — Verify from outside

From a machine that is **not** on your tailnet (or your phone on
cellular with Tailscale off), confirm the box is dark:

```bash
# Expect timeouts / refused on every public port:
nc -vz -w 3 <server-public-ip> 22
nc -vz -w 3 <server-public-ip> 8080
```

Both should fail. Then turn Tailscale back on and confirm Postern and
SSH work over the tailnet. If you enabled Funnel, the public
`https://postern.<tailnet>.ts.net` URL still works — that traffic
arrives through Tailscale, not an open host port.

## What you end up with

- SSH: key-only, reachable **only over Tailscale**. Public 22 closed.
- Postern: reachable at `https://postern.<tailnet>.ts.net` over the
  tailnet (and via Funnel if you enabled it). Bound to loopback on the
  host.
- Every other inbound port: closed.
- Outbound (IMAP/SMTP, the update check, off-site backups): unaffected.

## Recovering from a lockout

If you ever can't SSH in:

1. Open the **VPS provider's web console / VNC** (connects below the
   firewall).
2. `sudo ufw disable` to drop the firewall, or
   `sudo tailscale up` to bring the tunnel back, then fix the rule that
   shut you out and re-enable.

This is why the provider console tab stays open during the whole
process.
