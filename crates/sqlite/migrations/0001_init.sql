-- The OpenSharing server's state (SQLite). The same tables as the Postgres
-- schema: secrets are stored sealed (envelope encryption), recipient tokens
-- only as sha256 digests. Foreign keys are enforced by the connection
-- (`PRAGMA foreign_keys = ON`, set by `store::sqlite::connect`).

CREATE TABLE shares (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL UNIQUE,
    comment     TEXT,
    -- The share's UC `DataObject`s as a JSON array, in the order they were added.
    objects     TEXT NOT NULL DEFAULT '[]',
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
);

CREATE TABLE recipients (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL UNIQUE,
    comment     TEXT,
    created_at  INTEGER NOT NULL
);

CREATE TABLE recipient_tokens (
    id                    TEXT PRIMARY KEY,
    recipient_id          TEXT NOT NULL REFERENCES recipients (id) ON DELETE CASCADE,
    token_hash            BLOB NOT NULL UNIQUE,
    -- Kept only until the recipient activates the token.
    sealed_token          BLOB,
    -- The sha256 digest of the activation code, which is derived from the
    -- token; the link is rebuilt from the sealed token while that is kept.
    activation_code_hash  BLOB UNIQUE,
    expires_at            INTEGER,
    created_at            INTEGER NOT NULL
);

CREATE INDEX recipient_tokens_recipient ON recipient_tokens (recipient_id);

CREATE TABLE grants (
    share_id      TEXT NOT NULL REFERENCES shares (id) ON DELETE CASCADE,
    recipient_id  TEXT NOT NULL REFERENCES recipients (id) ON DELETE CASCADE,
    PRIMARY KEY (share_id, recipient_id)
);

CREATE INDEX grants_recipient ON grants (recipient_id);

-- Storage credentials and external locations are not here: they live in the
-- storage-access tables (`olai-uc-storage-access`, migrations 02xx), which the
-- server merges into this ledger with its `unity` feature.
