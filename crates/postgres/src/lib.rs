//! The Postgres database of the sharing server: the
//! [`SharingStore`](opensharing_provider::store::SharingStore) of
//! `olai-uc-sharing` ([`PostgresStore`]), its migrations, and the connection
//! and migration lifecycle.
//!
//! Migrations run only through [`migrate`] (the `migrate` subcommand), never on
//! startup; [`ensure_schema_current`] makes `serve` refuse to start on a schema
//! that is behind, ahead of, or diverged from this binary. Both hold a session
//! advisory lock, so on a transaction-pooled endpoint (PgBouncer, Neon's pooled
//! URL) point them at the direct, unpooled connection string.
//!
//! The migrator records applied migrations in `_sqlx_migrations`, so give the
//! sharing server its own database (or Neon branch) rather than sharing one with
//! another sqlx-migrated service.

pub use sqlx::migrate::{Migration, Migrator};
use sqlx::postgres::{PgPool, PgPoolOptions};

use crate::sql::{APPLIED_MIGRATIONS, AppliedMigration, check_applied, merged};
use opensharing_provider::{Error, Result};

/// The Postgres [`SharingStore`](opensharing_provider::store::SharingStore), over
/// the `uc_shares`, `uc_recipients`, `uc_recipient_tokens` and
/// `uc_share_grants` tables.
pub use unitycatalog_sharing::PgSharingStore as PostgresStore;
/// The migrations of the sharing tables, versioned 03xx (`olai-uc-sharing`).
///
/// Another component keeping tables in the same database merges its own into
/// one migrator with [`migrator_with`]; the 02xx range belongs to the
/// storage-access tables of `olai-uc-storage-access`.
pub use unitycatalog_sharing::postgres::MIGRATOR;

mod sql;

/// [`MIGRATOR`] merged with `extra`, so the database keeps one ordered
/// `_sqlx_migrations` ledger. Pass the result to [`migrate`] and
/// [`ensure_schema_current`].
pub fn migrator_with(extra: impl IntoIterator<Item = Migration>) -> Migrator {
    merged(&MIGRATOR, extra)
}

/// Connect a pool to `url`.
pub async fn connect(url: &str, max_connections: u32) -> Result<PgPool> {
    PgPoolOptions::new()
        .max_connections(max_connections)
        .connect(url)
        .await
        .map_err(|e| Error::generic(format!("connecting to postgres: {e}")))
}

/// Apply all of `migrator`'s pending migrations.
pub async fn migrate(migrator: &Migrator, pool: &PgPool) -> Result<()> {
    migrator
        .run(pool)
        .await
        .map_err(|e| Error::generic(format!("running migrations: {e}")))
}

/// Fail unless the database schema is exactly `migrator`'s.
pub async fn ensure_schema_current(migrator: &Migrator, pool: &PgPool) -> Result<()> {
    let applied: Vec<AppliedMigration> =
        match sqlx::query_as(APPLIED_MIGRATIONS).fetch_all(pool).await {
            Ok(rows) => rows,
            // No migrations table yet: nothing is applied.
            Err(sqlx::Error::Database(e)) if e.code().as_deref() == Some("42P01") => Vec::new(),
            Err(e) => return Err(Error::generic(format!("reading applied migrations: {e}"))),
        };
    check_applied(migrator, &applied)
}
