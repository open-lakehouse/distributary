//! The SQLite database of the sharing server: the
//! [`SharingStore`](opensharing_provider::store::SharingStore) of
//! `olai-uc-sharing` ([`SqliteStore`]), its migrations, and the connection and
//! migration lifecycle.
//!
//! For single-node deployments, demos and tests that want state to survive a
//! restart without running Postgres. As with Postgres, migrations run only
//! through [`migrate`] (the `migrate` subcommand), and
//! [`ensure_schema_current`] makes `serve` refuse to start on a schema that
//! does not match this binary.
//!
//! [`connect`] creates the database file if it is missing, enables foreign
//! keys (the schema cascades deletes through them) and uses WAL journaling. An
//! in-memory URL (`sqlite::memory:`) gives each pooled connection its own
//! database, so use a file.

use std::str::FromStr;
use std::time::Duration;

pub use sqlx::migrate::{Migration, Migrator};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions};

use crate::sql::{APPLIED_MIGRATIONS, AppliedMigration, check_applied, merged};
use opensharing_provider::{Error, Result};

/// The SQLite [`SharingStore`](opensharing_provider::store::SharingStore), over
/// the `uc_shares`, `uc_recipients`, `uc_recipient_tokens` and
/// `uc_share_grants` tables.
pub use unitycatalog_sharing::SqliteSharingStore as SqliteStore;
/// The migrations of the sharing tables, versioned 03xx (`olai-uc-sharing`).
///
/// Another component keeping tables in the same database merges its own into
/// one migrator with [`migrator_with`]; the 02xx range belongs to the
/// storage-access tables of `olai-uc-storage-access`.
pub use unitycatalog_sharing::sqlite::MIGRATOR;

mod sql;

/// [`MIGRATOR`] merged with `extra`, so the database keeps one ordered
/// `_sqlx_migrations` ledger. Pass the result to [`migrate`] and
/// [`ensure_schema_current`].
pub fn migrator_with(extra: impl IntoIterator<Item = Migration>) -> Migrator {
    merged(&MIGRATOR, extra)
}

/// Connect a pool to `url` (`sqlite://path/to/sharing.db`), creating the
/// database file if needed.
pub async fn connect(url: &str, max_connections: u32) -> Result<SqlitePool> {
    let options = SqliteConnectOptions::from_str(url)
        .map_err(|e| Error::invalid_argument(format!("invalid sqlite url: {e}")))?
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Wal)
        .busy_timeout(Duration::from_secs(5));
    SqlitePoolOptions::new()
        .max_connections(max_connections)
        .connect_with(options)
        .await
        .map_err(|e| Error::generic(format!("connecting to sqlite: {e}")))
}

/// Apply all of `migrator`'s pending migrations.
pub async fn migrate(migrator: &Migrator, pool: &SqlitePool) -> Result<()> {
    migrator
        .run(pool)
        .await
        .map_err(|e| Error::generic(format!("running migrations: {e}")))
}

/// Fail unless the database schema is exactly `migrator`'s.
pub async fn ensure_schema_current(migrator: &Migrator, pool: &SqlitePool) -> Result<()> {
    let applied: Vec<AppliedMigration> =
        match sqlx::query_as(APPLIED_MIGRATIONS).fetch_all(pool).await {
            Ok(rows) => rows,
            // No migrations table yet: nothing is applied.
            Err(sqlx::Error::Database(e)) if e.message().contains("no such table") => Vec::new(),
            Err(e) => return Err(Error::generic(format!("reading applied migrations: {e}"))),
        };
    check_applied(migrator, &applied)
}
