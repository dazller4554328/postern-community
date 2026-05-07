---
title: Reference
---

# Reference

Material that's useful when you're poking around inside Postern —
schema invariants, migration paths, anything with enough depth that
it doesn't belong in the install flow.

- **[Storage invariants](storage-invariants.md)** — the rules the
  storage layer assumes. Useful when manually inspecting the
  SQLCipher DB or writing a tool that touches it.
- **[Migrating from Mailpile](migration-from-mailpile.md)** — bulk-
  importing a Mailpile mail tree into a fresh Postern install. Path-
  based importer, no Mailpile runtime needed.
