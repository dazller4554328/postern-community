#!/usr/bin/env python3
"""Tiny HTTP-over-UDS conversion server for the Postern viewer sandbox.

Listens on /run/viewer/viewer.sock (the shared tmpfs volume between
this container and the main Postern container) and responds to a
single endpoint:

    POST /convert
      X-Filename: <original filename>
      Content-Type: application/octet-stream
      Body: the raw file bytes

    → 200 application/pdf  (bytes)
    → 5xx text/plain       (short error message)

Everything else 404s. No GETs, no listings, no introspection endpoints
— the less surface this speaks, the better.

Why HTTP over UDS instead of a bespoke binary protocol: the main
Postern container already has an HTTP client in the codebase; extending
it to speak a new protocol just to save a few bytes per request isn't
worth the maintenance cost. HTTP on a Unix socket keeps the attack
surface on the viewer side to stdlib code only.
"""
from __future__ import annotations

import http.server
import os
import shutil
import socketserver
import stat
import subprocess
import sys
import tempfile
from pathlib import Path

SOCKET_PATH = "/run/viewer/viewer.sock"
MAX_BODY = 200 * 1024 * 1024  # 200 MiB per file — enormous for email.
CONVERT_TIMEOUT = 90  # seconds; LibreOffice cold start + a big doc.

# Extensions we accept. Anything else 415s before we even try.
ALLOWED_EXTS = {
    ".doc", ".docx", ".rtf", ".odt",
    ".xls", ".xlsx", ".ods", ".csv",
    ".ppt", ".pptx", ".odp",
    ".txt",
}


class ConvertHandler(http.server.BaseHTTPRequestHandler):
    # Suppress the default stderr access log — the main Postern
    # container's logs are the source of truth for conversion events.
    def log_message(self, *_args, **_kwargs):
        return

    def do_POST(self):
        if self.path != "/convert":
            self._error(404, b"not found")
            return

        length = int(self.headers.get("Content-Length", 0))
        if length <= 0:
            self._error(400, b"empty body")
            return
        if length > MAX_BODY:
            self._error(413, b"file too large")
            return

        # Defensive: strip control chars from filename so a malicious
        # header can't inject newlines into our filesystem paths.
        raw_name = self.headers.get("X-Filename", "upload.docx")
        filename = "".join(
            c for c in raw_name if c.isprintable() and c not in "/\\\0"
        )[:200] or "upload.docx"

        ext = Path(filename).suffix.lower()
        if ext not in ALLOWED_EXTS:
            self._error(415, f"unsupported extension: {ext}".encode())
            return

        body = self.rfile.read(length)

        with tempfile.TemporaryDirectory(prefix="conv-") as td:
            infile = Path(td) / f"in{ext}"
            infile.write_bytes(body)
            pdf = _convert_to_pdf(infile)
            if pdf is None:
                self._error(500, b"conversion failed")
                return

            self.send_response(200)
            self.send_header("Content-Type", "application/pdf")
            self.send_header("Content-Length", str(len(pdf)))
            self.end_headers()
            self.wfile.write(pdf)

    def _error(self, status: int, msg: bytes):
        self.send_response(status)
        self.send_header("Content-Type", "text/plain; charset=utf-8")
        self.send_header("Content-Length", str(len(msg)))
        self.end_headers()
        self.wfile.write(msg)


def _convert_to_pdf(infile: Path) -> bytes | None:
    """Run soffice --headless --convert-to pdf. Returns PDF bytes or
    None on failure. The LibreOffice profile directory lives under
    $HOME (tmpfs), so a crashed conversion leaves no disk residue."""
    outdir = infile.parent
    try:
        proc = subprocess.run(
            [
                "soffice",
                "--headless",
                "--safe-mode",
                "--norestore",
                "--nologo",
                "--convert-to", "pdf",
                "--outdir", str(outdir),
                str(infile),
            ],
            capture_output=True,
            timeout=CONVERT_TIMEOUT,
        )
    except subprocess.TimeoutExpired:
        print(f"convert timeout: {infile.name}", file=sys.stderr, flush=True)
        return None
    if proc.returncode != 0:
        print(
            f"convert failed rc={proc.returncode}: "
            f"{proc.stderr.decode('utf-8', 'replace')[:300]}",
            file=sys.stderr,
            flush=True,
        )
        return None

    pdf_path = outdir / (infile.stem + ".pdf")
    if not pdf_path.exists():
        return None
    try:
        return pdf_path.read_bytes()
    except OSError:
        return None


class UDSHTTPServer(socketserver.UnixStreamServer):
    """socketserver wants a tuple for TCPServer but a string path for
    UnixStreamServer. We subclass just to set allow_reuse_address-
    adjacent cleanup on the socket file."""
    daemon_threads = True

    def server_bind(self):
        # Clean up a stale socket left by an ungraceful shutdown so we
        # don't fail bind() with EADDRINUSE on the next start.
        try:
            os.unlink(self.server_address)
        except FileNotFoundError:
            pass
        super().server_bind()
        # 0666 so the Postern container (running as a different uid)
        # can read/write. The socket lives on an internal tmpfs volume
        # that only the two Postern containers mount — nobody else on
        # the host can reach it.
        os.chmod(
            self.server_address,
            stat.S_IRUSR | stat.S_IWUSR | stat.S_IRGRP | stat.S_IWGRP
            | stat.S_IROTH | stat.S_IWOTH,
        )


def main():
    sock_dir = os.path.dirname(SOCKET_PATH)
    os.makedirs(sock_dir, exist_ok=True)

    # LibreOffice wants a writable HOME. It lives on tmpfs so it's
    # wiped on restart.
    home = os.environ.get("HOME", "/tmp/lo-home")
    os.makedirs(home, exist_ok=True)

    # Pre-warm LibreOffice's user profile directory so the first
    # real conversion isn't a cold-start with its 4-second startup.
    # Conveniently also validates that soffice can run at all before
    # we start accepting requests.
    try:
        subprocess.run(
            ["soffice", "--headless", "--terminate_after_init"],
            capture_output=True,
            timeout=30,
            check=False,
        )
    except Exception as e:  # noqa: BLE001 — this is a warmup, not critical
        print(f"warmup skipped: {e}", file=sys.stderr, flush=True)

    server = UDSHTTPServer(SOCKET_PATH, ConvertHandler)
    print(f"postern-viewer listening on {SOCKET_PATH}", flush=True)
    try:
        server.serve_forever()
    finally:
        try:
            os.unlink(SOCKET_PATH)
        except OSError:
            pass


if __name__ == "__main__":
    main()
