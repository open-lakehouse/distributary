# opensharing-sqlite

The SQLite database of the OpenSharing server: the
[`opensharing-provider`](../provider) store (`SqliteStore`, the
`SqliteSharingStore` of mangrove's `olai-uc-sharing`), its migrations, and the
connection and migration lifecycle around them, over sqlx. It is meant for
single-node deployments whose state should survive a restart without a
database server.

As with Postgres, migrations run only through `migrate`, and
`ensure_schema_current` reports a schema that does not match the binary.
`tests/store.rs` runs the provider's conformance suite on a temporary file.

The sharing tables' migrations (`MIGRATOR`) are numbered 03xx. `migrator_with`
merges another component's into one migrator for the same database: the server
adds the storage-access tables (`olai-uc-storage-access`, 02xx) that way.
