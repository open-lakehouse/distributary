-- Activation codes are stored only as sha256 digests, like the tokens: the
-- code is now derived from the token, so the link is rebuilt from the sealed
-- token while that is kept.
--
-- Codes issued before this were random and stored in clear, so they cannot
-- be carried over: their links stop working, and recipients that had not
-- activated yet need their token rotated.

UPDATE recipient_tokens SET sealed_token = NULL WHERE activation_code IS NOT NULL;

ALTER TABLE recipient_tokens DROP COLUMN activation_code;

ALTER TABLE recipient_tokens ADD COLUMN activation_code_hash BYTEA UNIQUE;
