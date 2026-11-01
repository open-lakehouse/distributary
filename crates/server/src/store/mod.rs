//! The provider's stores, as the server selects them.
//!
//! The traits, the records and the in-memory store come from
//! [`opensharing_provider::store`], re-exported here, plus:
//!
//! - `postgres` (feature `postgres`): `opensharing-postgres`;
//! - `sqlite` (feature `sqlite`): `opensharing-sqlite`, for single-node
//!   deployments.
//!
//! With the `unity` feature the storage-access tables
//! (`olai-uc-storage-access`) live in the same database: [`postgres_migrator`]
//! and [`sqlite_migrator`] merge their migrations into the store's, so the
//! database keeps one ledger, and `migrate` and the startup schema check cover
//! both.

pub use opensharing_provider::store::*;

#[cfg(feature = "postgres")]
pub use opensharing_postgres as postgres;
#[cfg(feature = "sqlite")]
pub use opensharing_sqlite as sqlite;

/// The Postgres store's migrations, with (feature `unity`) the storage-access
/// tables'.
#[cfg(feature = "postgres")]
pub fn postgres_migrator() -> postgres::Migrator {
    #[cfg(feature = "unity")]
    let extra: Vec<_> = unitycatalog_storage_access::postgres::MIGRATOR
        .iter()
        .cloned()
        .collect();
    #[cfg(not(feature = "unity"))]
    let extra = Vec::new();
    postgres::migrator_with(extra)
}

/// The SQLite store's migrations, with (feature `unity`) the storage-access
/// tables'.
#[cfg(feature = "sqlite")]
pub fn sqlite_migrator() -> sqlite::Migrator {
    #[cfg(feature = "unity")]
    let extra: Vec<_> = unitycatalog_storage_access::sqlite::MIGRATOR
        .iter()
        .cloned()
        .collect();
    #[cfg(not(feature = "unity"))]
    let extra = Vec::new();
    sqlite::migrator_with(extra)
}
