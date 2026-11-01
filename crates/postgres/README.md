# opensharing-postgres

The Postgres database of the OpenSharing server: the
[`opensharing-provider`](../provider) store (`PostgresStore`, the
`PgSharingStore` of mangrove's `olai-uc-sharing`), its migrations, and the
connection and migration lifecycle around them, over sqlx.

Migrations run only through `migrate`, never on startup. `ensure_schema_current`
lets a server refuse to start on a schema that is behind, ahead of, or diverged
from its binary. Both hold a session advisory lock, so on a transaction-pooled
endpoint (PgBouncer, Neon's pooled URL) point them at the direct, unpooled
connection string.

`tests/store.rs` runs the provider's conformance suite when
`SHARING_SERVER_TEST_DATABASE_URL` points at a disposable database.

The sharing tables' migrations (`MIGRATOR`) are numbered 03xx. `migrator_with`
merges another component's into one migrator for the same database: the server
adds the storage-access tables (`olai-uc-storage-access`, 02xx) that way.
