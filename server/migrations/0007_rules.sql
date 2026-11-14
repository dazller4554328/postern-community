CREATE TABLE rules (
  id          INTEGER PRIMARY KEY,
  account_id  INTEGER REFERENCES accounts(id) ON DELETE CASCADE,
  name        TEXT NOT NULL,
  enabled     INTEGER NOT NULL DEFAULT 1,
  priority    INTEGER NOT NULL DEFAULT 0,
  condition_field   TEXT NOT NULL,
  condition_op      TEXT NOT NULL,
  condition_value   TEXT NOT NULL,
  action_type       TEXT NOT NULL,
  action_value      TEXT NOT NULL,
  created_at  INTEGER NOT NULL,
  updated_at  INTEGER NOT NULL
);

CREATE INDEX idx_rules_account ON rules(account_id, enabled, priority);
