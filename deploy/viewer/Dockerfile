# Disposable document-conversion sandbox.
#
# This image exists to run ONE thing: LibreOffice headless, converting
# untrusted Office / OpenDocument attachments into PDF. Everything that
# isn't on that critical path is absent by design — no shells a user
# would ever see, no network clients, no editors, just soffice + a tiny
# Python HTTP-over-UDS server.
#
# The container runs with network_mode: none and drops every capability
# (see docker-compose.yml). If LibreOffice has an unpatched parser bug
# and an attacker gets code exec inside this container, they land in a
# no-network no-cap tmpfs-only cgroup — and they still have to escape
# the kernel namespace to touch the host. The container is literally
# restarted between requests by the server wrapper so any in-process
# state from one conversion can't persist into the next.
FROM debian:12-slim

ENV DEBIAN_FRONTEND=noninteractive \
    LANG=C.UTF-8 \
    PYTHONUNBUFFERED=1

# LibreOffice writer + calc + impress cover .doc/.docx, .xls/.xlsx,
# .ppt/.pptx and the equivalent .odt/.ods/.odp. dejavu for basic font
# coverage so documents don't render as empty boxes when the embedded
# font isn't present. --no-install-recommends keeps the image ~700MB
# rather than 1.5GB.
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        libreoffice-core \
        libreoffice-writer \
        libreoffice-calc \
        libreoffice-impress \
        fonts-dejavu \
        fonts-liberation \
        python3 \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/* /tmp/* /var/tmp/* \
              /usr/share/doc /usr/share/man /usr/share/locale

# Nobody user for the process owner. uid 65534 is the standard "nobody"
# on Debian. Both the main Postern container and this one mount the
# socket volume; the socket is chmod 666 so cross-uid access works
# without us having to align uids across images.
RUN useradd --no-create-home --system --uid 65534 --gid 65534 viewer 2>/dev/null || true

COPY --chown=root:root server.py /usr/local/bin/postern-viewer
RUN chmod 0755 /usr/local/bin/postern-viewer

# /tmp is tmpfs (declared in compose). Working dir lives there so
# nothing persists to the image's writable layer.
WORKDIR /tmp

# soffice wants a HOME it can write to; point at a per-user dir under
# tmpfs. Keeps the rest of the filesystem read-only.
ENV HOME=/tmp/lo-home

USER 65534:65534

ENTRYPOINT ["/usr/local/bin/postern-viewer"]
