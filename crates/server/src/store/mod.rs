//! The SQL implementations of the provider's store traits.
//!
//! The traits, the records and the in-memory store come from
//! [`opensharing_provider::store`], re-exported here. This module adds:
//!
//! - `postgres::PostgresStore` (feature `postgres`);
//! - `sqlite::SqliteStore` (feature `sqlite`), for single-node deployments.
//!
//! The SQL stores embed their own migrations (`migrations/{postgres,sqlite}`).

pub use opensharing_provider::store::*;

#[cfg(feature = "postgres")]
pub mod postgres;
#[cfg(any(feature = "postgres", feature = "sqlite"))]
mod sql;
#[cfg(feature = "sqlite")]
pub mod sqlite;
