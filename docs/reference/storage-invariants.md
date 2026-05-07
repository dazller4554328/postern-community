# Storage invariants

The schema lives in `server/migrations/`; the canonical shape of each
table is regenerated into `SCHEMA.generated.sql` (run
`server/scripts/generate-schema.sh`). This document covers things that
*aren't* in the SQL — the semantic invariants a reader needs to know
before they touch sync, label, or backup code.

## Labels and the local view

**Local labels reflect user organisation in Postern. They are not a
mirror of server folder state.**

A message Postern has downloaded keeps the local label(s) it was
filed under in Postern's UI, regardless of where the byte-for-byte
copy happens to sit on the provider. Server-side bookkeeping
(quota-driven moves, provider Trash auto-purge, etc.) does *not*
propagate to local labels.

**The single mirror rule** — applied by `sync_folder` in
`server/src/sync/imap.rs`:

| Server folder being synced | Message state | Local labels added |
|---|---|---|
| `INBOX` | new to Postern | `["INBOX"]` |
| `INBOX` | already known | (none — preserve user's existing organisation) |
| `[Gmail]/Sent Mail`, `[Gmail]/Drafts`, custom label | new to Postern | the source folder name |
| `[Gmail]/Sent Mail`, custom label | already known | (none) |
| `[Gmail]/Trash` | new to Postern | `["[Gmail]/Trash"]` (genuine "user trashed in Gmail web before Postern saw it") |
| `[Gmail]/Trash` | already known | **(none — never re-tag)** |
| Any folder | already known + delete-after-sync flips false→true | (none added; backfill purge takes over) |

**Why the Trash rule matters**: with `delete_after_sync = true`, the
streaming sync MOVEs each downloaded message to `[Gmail]/Trash` for
quota relief. The next sync cycle over Trash *will* see those
messages — but they're already in Postern under the user's original
labels (`INBOX` etc.). Adding `[Gmail]/Trash` to them would
duplicate-display in the user's Inbox view AND Trash view. Don't.

**What MAY add labels to a known message**:
- The streaming sync path above, for a *new-to-this-folder* sighting
  not previously seen elsewhere.
- The Gmail label rescan (`POST /api/accounts/:id/rescan-gmail-labels`
  in `server/src/sync/gmail_rescan.rs`) — explicit user-triggered
  X-GM-LABELS reconciliation.
- Direct user actions: per-message Move (`/messages/:id/move`),
  bulk Move (`/messages/bulk/move-to`), archive, spam — these go
  through `Db::relabel_message` which *replaces* the label set
  atomically.

**What MUST NOT happen**:
- Server-side label removal (e.g. Gmail's MOVE-to-Trash strips other
  labels server-side) → matching local strip. This was tried in
  migration 0035; it permanently lost the "this was originally in
  INBOX" information for messages already in Trash, with no recovery
  path. Reverted in commit 55e286d. The right model is canonical-
  local; server-side strips are invisible to Postern's UI.

## Atomicity

`Db::relabel_message` (`server/src/storage/messages.rs`) wraps its
DELETE + INSERT cycle in a transaction. Without that, a pool eviction
between the DELETE and the first INSERT leaves the message with zero
labels — invisible in every folder view. Callers (move handler,
spam handler, archive handler) reach this on the optimistic path
*before* the IMAP MOVE confirms, so the crash window was a real
label-loss risk. Don't unwrap the transaction.

## Vault and the encryption layers

| Layer | Encryption | Key source |
|---|---|---|
| `postern.db` | SQLCipher (AES-256-CBC + HMAC) | Argon2id(master password, salt) → HKDF "db" |
| `blobs/<hash>` | ChaCha20-Poly1305 | Argon2id(master password, salt) → HKDF "blob" |
| `secrets.ciphertext` (table) | ChaCha20-Poly1305 (KEK) | Argon2id(master password, salt) → HKDF "kek" |
| `vault.json` | wraps the verifier blob | salt + KEK |

Three subkeys from one Argon2 derivation. The vault sidecar exists
*outside* SQLCipher because the DB key needs to come from somewhere
before the DB is queryable.

A backup tarball (`server/src/backup.rs`) carries `postern.db`,
`vault.json`, and the entire `blobs/` directory. The user's master
password is required to read any of it — the SQLCipher DB and the
blobs are both encrypted at rest. **The blobs are NOT plaintext**;
the doc-comment in `backup.rs` to the contrary was stale and was
corrected in commit 902605d.

## Foreign keys and cascading deletes

Most data tables `REFERENCES accounts(id) ON DELETE CASCADE`.
Removing an account removes all its messages, labels,
message_labels, sync_state, rules, etc. — but NOT `secrets`
(the credential blob is keyed by `credential_ref` and cleaned up
explicitly by `Db::delete_account` rather than via FK cascade,
because `secrets` is shared with non-account-scoped consumers
like backup destinations).

## Schedule + scheduler tables

`backup_schedule` is a single-row table (`CHECK (id = 1)`). The
scheduler tick task in `server/src/backup_scheduler.rs` reads it
once a minute and applies the `should_fire_now` predicate (pure
function in `server/src/storage/backup_schedule.rs`). The
60-minute debounce on `last_run_at` survives clock skew.

## Auth boundary

Every HTTP handler that touches `accounts`, `messages`, `secrets`,
or any vault-encrypted column starts with `s.vault.require_unlocked()?;`.
There is currently no middleware enforcing this — a new endpoint
that forgets the call is silently un-gated. Audit visually when
adding routes; future work (Phase 4 / AppState refactor) may codify
this via a typed wrapper.

## Migration policy

- Migrations are applied in lexical order (`0001_*.sql` before
  `0002_*.sql`).
- Migrations are recorded in `schema_migrations` by name; a migration
  must be *additive* or risk failing on existing installs.
- **Do not write migrations that destroy user data** unless every
  reader of the data signals consent. Migration 0035 deleted INBOX
  labels from messages that had `[Gmail]/Trash` and had no recovery
  path — committed before the streaming sync's mirror logic was
  reverted, so the data loss was permanent. Future migrations that
  affect labels, blobs, or messages should be reviewed against this
  document's invariants first.
