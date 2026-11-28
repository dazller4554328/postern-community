#!/usr/bin/env bash
#
# Fetch a country-flavoured MMDB for the forensics delivery-path view.
#
# Source: DB-IP country-lite, public free download under CC-BY 4.0.
# (Same MMDB format as MaxMind GeoLite2-Country — the maxminddb crate
# reads either.) No account or licence key required.
#
# Output: <data_dir>/GeoLite2-Country.mmdb (the canonical name the
# server looks for; falls back to dbip-country-lite.mmdb if you'd
# rather keep the upstream filename).
#
# Usage:
#   scripts/fetch-geoip.sh                     # writes ./data/GeoLite2-Country.mmdb
#   scripts/fetch-geoip.sh /var/lib/postern/data
#
set -euo pipefail

DEST_DIR="${1:-./data}"
DEST_FILE="${DEST_DIR}/GeoLite2-Country.mmdb"

mkdir -p "$DEST_DIR"

# DB-IP republishes the lite DB on the 1st of each month. If "this
# month" hasn't been published yet (early in the month, mirror lag),
# fall back to the previous month.
month_url() {
  local ym="$1"
  echo "https://download.db-ip.com/free/dbip-country-lite-${ym}.mmdb.gz"
}

YM_NOW=$(date -u +%Y-%m)
YM_PREV=$(date -u -d "1 month ago" +%Y-%m 2>/dev/null || date -u -v-1m +%Y-%m)

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

for ym in "$YM_NOW" "$YM_PREV"; do
  url=$(month_url "$ym")
  echo "fetching $url"
  if curl -fSL --retry 3 --retry-delay 2 -o "$TMP/db.mmdb.gz" "$url"; then
    gunzip -f "$TMP/db.mmdb.gz"
    mv "$TMP/db.mmdb" "$DEST_FILE"
    echo "installed: $DEST_FILE ($(stat -c%s "$DEST_FILE" 2>/dev/null || stat -f%z "$DEST_FILE") bytes)"
    exit 0
  fi
done

echo "ERROR: could not fetch DB-IP country-lite for $YM_NOW or $YM_PREV" >&2
exit 1
