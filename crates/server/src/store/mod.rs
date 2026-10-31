//! The provider's stores, as the server selects them.
//!
//! The traits, the records and the in-memory store come from
//! [`opensharing_provider::store`], re-exported here, plus:
//!
//! - `postgres` (feature `postgres`): `opensharing-postgres`;
//! - `sqlite` (feature `sqlite`): `opensharing-sqlite`, for single-node
//!   deployments.

pub use opensharing_provider::store::*;

#[cfg(feature = "postgres")]
pub use opensharing_postgres as postgres;
#[cfg(feature = "sqlite")]
pub use opensharing_sqlite as sqlite;
