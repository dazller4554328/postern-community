# Postern document-viewer sandbox

A minimal sibling container that converts untrusted Office /
OpenDocument attachments to PDF. Runs LibreOffice headless inside a
maximally-locked-down Docker container so that even if a parser bug
lets an attacker get code execution, they land somewhere useless.

## Threat model

This sandbox exists for one specific worry: **a `.docx` / `.xlsx` /
`.odt` file from an unknown sender triggers a parsing bug in
LibreOffice**. Modern mail clients that offer inline preview of Office
docs either (a) render them in a browser process that has access to
your other tabs (Gmail, Outlook Web), or (b) use a native integration
that runs in the same security domain as your mail client (Thunderbird
with LibreOffice plugin, Apple Mail).

Postern's default browser sandbox handles PDFs fine (PDF.js + strict
CSP). But for formats that need native parsing, we need somewhere
*outside* the main Postern container to do the parsing. That's this.

## What's locked down

The sandbox runs with:

| Setting | Value | What it buys |
|---|---|---|
| `network_mode: none` | No interfaces at all, not even loopback | Nothing the exploited process does can reach the network |
| `read_only: true` | Root filesystem immutable | No persistence on the image layer |
| `tmpfs: /tmp` | Working dir is RAM-only | Converted files vanish on restart |
| `cap_drop: ALL` | No Linux capabilities | Can't bind to low ports, can't ptrace, can't do almost anything interesting |
| `security_opt: no-new-privileges` | setuid / fscaps can't elevate | Even a setuid bug in LibreOffice goes nowhere |
| `user: 65534:65534` | Runs as `nobody` | Unprivileged namespace-user |
| `pids_limit: 128` | Fork-bomb cap | A malicious doc can't exhaust host PIDs |
| `mem_limit: 1g` | Memory cap | OOM-loops bounded |

The only channel in or out is a Unix socket on a shared tmpfs volume
(`viewer-socket`). The main Postern container writes a conversion
request to that socket; the sandbox reads the file bytes, runs
`soffice --headless --safe-mode --convert-to pdf`, and writes the
PDF back on the same socket.

## How to enable

Disabled by default because the image is ~700MB (LibreOffice). Turn it
on when you actually need to preview Office docs:

```bash
# From the repo root:
docker compose -f deploy/community/docker-compose.yml --profile viewer up -d

# Or with the pro compose:
docker compose -f deploy/docker/docker-compose.yml --profile viewer up -d
```

The `profiles: [viewer]` tag means compose skips this service unless
you pass `--profile viewer`. When it's not running Postern's Preview
button for Office docs is hidden (via `/api/viewer-sandbox/status`),
so the UI doesn't point at a broken path.

Once up, the Preview button appears next to `.doc / .docx / .xls /
.xlsx / .ppt / .pptx / .odt / .ods / .odp / .rtf / .csv` attachments.
Click it: Postern asks the sandbox to convert, the sandbox replies
with PDF bytes, Postern streams them to the browser's PDF.js viewer.

## Turning it off

```bash
docker compose --profile viewer down
docker image rm postern-viewer:latest
```

## Stronger isolation: gVisor

The container sandbox above defends against *unprivileged* escape
attempts: even with code exec inside LibreOffice, the attacker can't
reach the host network, can't write outside tmpfs, can't gain caps.
What it doesn't defend against is a *kernel* bug — if a Linux syscall
has an unpatched vulnerability, an exploit chain could escape the
container namespace.

If you're paranoid enough to care about that, run this sandbox on top
of [gVisor](https://gvisor.dev), which re-implements the Linux syscall
API in user-space Go. An exploit would have to bug both LibreOffice
*and* gVisor's syscall emulation — significantly harder than "bug
Linux."

### Install gVisor on the Docker host

On the host where your Postern containers run (not inside any
container), as root:

```bash
# Debian / Ubuntu
curl -fsSL https://gvisor.dev/archive.key | gpg --dearmor -o /usr/share/keyrings/gvisor-archive-keyring.gpg
echo "deb [arch=$(dpkg --print-architecture) signed-by=/usr/share/keyrings/gvisor-archive-keyring.gpg] https://storage.googleapis.com/gvisor/releases release main" > /etc/apt/sources.list.d/gvisor.list
apt-get update && apt-get install -y runsc
runsc install
systemctl restart docker
```

On Fedora / CentOS / Rocky:

```bash
dnf install -y https://storage.googleapis.com/gvisor/releases/release/latest/x86_64/runsc-<version>.rpm
runsc install
systemctl restart docker
```

See gvisor.dev for the current package URLs.

### Point the viewer container at runsc

Edit the viewer service block in your compose file:

```yaml
  postern-viewer:
    runtime: runsc    # ← add this line
    build: ...
    # ...everything else stays the same
```

`docker compose --profile viewer up -d --force-recreate` and the
sandbox now boots under gVisor instead of runc. Verify with:

```bash
docker inspect postern-community-postern-viewer-1 \
  --format '{{.HostConfig.Runtime}}'
# → runsc
```

Expect a 2–3× slowdown on document conversion (gVisor intercepts every
syscall, which isn't free). In absolute terms: a 1-page `.docx` that
took ~1.5 seconds takes ~3–4 seconds. Acceptable for an interactive
preview; you feel it but it's not annoying.

### Qubes users

On Qubes you already have true disposable VMs at the OS level. This
Docker sandbox still works (the usual Postern-in-a-qube setup), but
for absolute safety you can also send attachments through Qubes'
native dispVM path:

```bash
# After clicking Download in Postern:
qvm-open-in-dvm ~/Downloads/the-file.docx
```

That opens the file in a brand-new ephemeral VM which dies when you
close it. Hardware-enforced isolation, not syscall filtering. The
Postern sandbox and the Qubes dispVM are complementary — use both
for layered defense.

## Troubleshooting

**"Preview not available, use Download" on an Office doc**
The sandbox isn't running. Run
`docker compose --profile viewer ps` — if you don't see
`postern-viewer` listed as `Up`, start it with `--profile viewer up -d`.

**Conversion takes forever then fails**
Sandbox default timeout is 90 seconds. A genuinely enormous doc
(hundreds of MB, thousands of pages) can blow through that. Either
raise `CONVERT_TIMEOUT` in `server.py` and rebuild the image, or
download + open locally.

**Container starts then immediately dies**
Check `docker compose --profile viewer logs postern-viewer`. Most
common cause: the tmpfs `/tmp` mount is too small for LibreOffice's
profile dir — bump `size=512m` to `size=1g` in the compose file.
