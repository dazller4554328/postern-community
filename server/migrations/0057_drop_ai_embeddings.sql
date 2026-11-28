-- Embeddings feature removed 2026-05-08. Datas now uses FTS5
-- keyword retrieval (the same engine the inbox search bar uses)
-- instead of vector similarity. The indexer background loop, the
-- ai_embeddings table, and every related Rust surface are gone.
-- This migration reclaims the space on existing installs.
--
-- Schema-wise this means a brand-new install runs 0050 (creates
-- ai_embeddings) → 0057 (drops it) back to back and lands in the
-- same state. We could skip applying 0050 entirely with a
-- conditional, but treating migrations as a sealed log of past
-- shape keeps the system simpler — every install converges to
-- the same end state regardless of when it joined.

DROP INDEX IF EXISTS idx_ai_embeddings_message;
DROP INDEX IF EXISTS idx_ai_embeddings_model;
DROP TABLE IF EXISTS ai_embeddings;
