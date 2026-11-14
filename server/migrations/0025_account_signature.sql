-- Per-account signature. signature_html is the rich form used when
-- compose is HTML; signature_plain mirrors it for plain-text sends.
-- Both NULL means "no signature". Auto-insertion into replies is a
-- client-side preference (postern.prefs.signatureOnReplies) rather
-- than a server column — the content is universal, the policy isn't.
ALTER TABLE accounts ADD COLUMN signature_html TEXT;
ALTER TABLE accounts ADD COLUMN signature_plain TEXT;
