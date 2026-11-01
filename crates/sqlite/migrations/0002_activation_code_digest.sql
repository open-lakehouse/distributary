-- Activation codes are stored only as sha256 digests, like the tokens: the
-- code is now derived from the token, so the link is rebuilt from the sealed
-- token while that is kept.
--
-- Codes issued before this were random and stored in clear, so they cannot
-- be carried over: their links stop working, and recipients that had not
-- activated yet need their token rotated.
--
-- SQLite cannot drop a UNIQUE column, so the table is rebuilt. Nothing
-- references `recipient_tokens`, so dropping it is safe with foreign keys on.

CREATE TABLE recipient_tokens_next (
    id                    TEXT PRIMARY KEY,
    recipient_id          TEXT NOT NULL REFERENCES recipients (id) ON DELETE CASCADE,
    token_hash            BLOB NOT NULL UNIQUE,
    -- Kept only until the recipient activates the token.
    sealed_token          BLOB,
    activation_code_hash  BLOB UNIQUE,
    expires_at            INTEGER,
    created_at            INTEGER NOT NULL
);

INSERT INTO recipient_tokens_next (id, recipient_id, token_hash, sealed_token, expires_at, created_at)
SELECT id, recipient_id, token_hash, CASE WHEN activation_code IS NULL THEN sealed_token END,
       expires_at, created_at
FROM recipient_tokens;

DROP TABLE recipient_tokens;

ALTER TABLE recipient_tokens_next RENAME TO recipient_tokens;

CREATE INDEX recipient_tokens_recipient ON recipient_tokens (recipient_id);
