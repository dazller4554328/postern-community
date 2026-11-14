#!/usr/bin/env bash
#
# Apply every migration in order against an empty SQLite DB, then dump
# `.schema` into ../migrations/SCHEMA.generated.sql. Lets a reader see
# the canonical shape of every table without walking 35 ALTER scripts.
#
# Idempotent: rebuilds the file on every run.
#
# Limitations: sqlite3 CLI on most distros doesn't ship with FTS5, so
# the FTS migration's CREATE VIRTUAL TABLE fails — we capture stderr
# and continue. The FTS table is documented manually in
# STORAGE_INVARIANTS.md.

set -euo pipefail

SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
ROOT="${SCRIPT_DIR}/.."
MIGRATIONS_DIR="${ROOT}/migrations"
OUTPUT="${MIGRATIONS_DIR}/SCHEMA.generated.sql"

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

WORK_DB="${TMP}/build.db"

for f in "${MIGRATIONS_DIR}"/*.sql; do
  # Skip the .generated file if anyone re-ran this without cleaning.
  case "$(basename "$f")" in
    SCHEMA.generated.sql) continue ;;
  esac
  sqlite3 "${WORK_DB}" ".read $f" 2>/dev/null || true
done

{
  echo "-- Postern canonical schema — GENERATED FILE, DO NOT EDIT."
  echo "-- Regenerate via: server/scripts/generate-schema.sh"
  echo "-- Sourced from server/migrations/000{1..N}_*.sql in order."
  echo "-- Last regenerated: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo ""
  echo "-- NOTE: FTS5 virtual tables are skipped (CLI sqlite3 lacks the"
  echo "-- module). See STORAGE_INVARIANTS.md for the messages_fts shape."
  echo ""
  sqlite3 "${WORK_DB}" ".schema" | grep -v "^CREATE TABLE sqlite_"
} > "${OUTPUT}"

echo "wrote ${OUTPUT}"
