# syntax=docker/dockerfile:1.7
#
# Postern Community image.
#
# Same three-stage shape as the pro Dockerfile (web → rust → runtime),
# minus everything that only makes sense on a VPS:
#
#   - wireguard-tools, iproute2, iptables — no VPN on community builds
#   - NET_ADMIN / NET_RAW cap and /dev/net/tun — not needed
#   - the tailscale sidecar — lives in the pro compose, not here
#
# Runtime image is just Debian slim + tini + ca-certificates.

ARG RUST_VERSION=1.87
ARG NODE_VERSION=22
ARG DEBIAN=bookworm-slim

# ---------- 1. web build ---------------------------------------------------
FROM node:${NODE_VERSION}-${DEBIAN} AS node-builder
WORKDIR /web
COPY web/package.json web/package-lock.json* ./
RUN --mount=type=cache,target=/root/.npm npm install --no-audit --no-fund
COPY web/ ./
RUN npm run build

# ---------- 2. rust build --------------------------------------------------
FROM rust:${RUST_VERSION}-slim-bookworm AS rust-builder
WORKDIR /src
RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      pkg-config libssl-dev ca-certificates \
      perl make \
 && rm -rf /var/lib/apt/lists/*
COPY Cargo.toml rust-toolchain.toml ./
COPY server ./server
# server/src/release_signing.rs uses include_bytes! to embed this
# pubkey at compile time. Path resolves to /src/deploy/...
COPY deploy/release-signing ./deploy/release-signing
ARG GIT_COMMIT=unknown
ENV GIT_COMMIT=${GIT_COMMIT}
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release -p postern \
 && cp target/release/postern /usr/local/bin/postern

# ---------- 3. runtime -----------------------------------------------------
FROM debian:${DEBIAN} AS runtime
RUN set -eux; \
    for i in 1 2 3; do \
      apt-get -o Acquire::Retries=3 update && break || sleep 5; \
    done; \
    apt-get install -y --no-install-recommends ca-certificates tini curl; \
    rm -rf /var/lib/apt/lists/*; \
    mkdir -p /var/lib/postern/data /var/lib/postern/backups

COPY --from=rust-builder /usr/local/bin/postern /usr/local/bin/postern
COPY --from=node-builder /web/build /usr/share/postern/web

# Forensics country lookup — DB-IP country-lite (CC-BY 4.0). Same MMDB
# format as MaxMind GeoLite2; the maxminddb crate reads it directly.
RUN mkdir -p /usr/share/postern/geoip \
 && set -eux; \
    YM_NOW=$(date -u +%Y-%m); \
    YM_PREV=$(date -u -d "1 month ago" +%Y-%m); \
    for ym in "$YM_NOW" "$YM_PREV"; do \
      url="https://download.db-ip.com/free/dbip-country-lite-${ym}.mmdb.gz"; \
      if curl -fSL --retry 3 --retry-delay 2 -o /tmp/geoip.mmdb.gz "$url"; then \
        gunzip -f /tmp/geoip.mmdb.gz; \
        mv /tmp/geoip.mmdb /usr/share/postern/geoip/GeoLite2-Country.mmdb; \
        break; \
      fi; \
    done; \
    test -s /usr/share/postern/geoip/GeoLite2-Country.mmdb

ARG GIT_COMMIT=unknown
ENV POSTERN_BIND=0.0.0.0:8080 \
    POSTERN_DATA_DIR=/var/lib/postern/data \
    POSTERN_STATIC_DIR=/usr/share/postern/web \
    POSTERN_GEOIP_DB=/usr/share/postern/geoip/GeoLite2-Country.mmdb \
    GIT_COMMIT=${GIT_COMMIT} \
    RUST_LOG=info

EXPOSE 8080
ENTRYPOINT ["/usr/bin/tini", "--"]
CMD ["/usr/local/bin/postern", "serve"]
