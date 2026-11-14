-- Sprint 5 vault: holds the Argon2id salt + verifier that lets the
-- server recognise "this password unlocks the installation". Single
-- row by design — enforced via CHECK (id = 1).
CREATE TABLE kek_config (
  id         INTEGER PRIMARY KEY CHECK (id = 1),
  salt       BLOB NOT NULL,
  verifier   BLOB NOT NULL,  -- AEAD-encrypted known plaintext; decrypt succeeds iff password is right
  created_at INTEGER NOT NULL,
  params     TEXT NOT NULL   -- JSON: {"m_cost":19456,"t_cost":2,"p_cost":1,"algo":"argon2id"}
);
