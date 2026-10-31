# opensharing-sqlite

The SQLite implementation of the [`opensharing-provider`](../provider) store
traits (`ShareStore`, `RecipientStore`, `CredentialStore`), over sqlx, with
embedded migrations (`migrations/`). It is meant for single-node deployments
whose state should survive a restart without a database server.

As with Postgres, migrations run only through `migrate`, and
`ensure_schema_current` reports a schema that does not match the binary.
`tests/store.rs` runs the provider's conformance suite on a temporary file.
