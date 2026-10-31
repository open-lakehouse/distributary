//! The Postgres store against a live database.
//!
//! Runs only when `SHARING_SERVER_TEST_DATABASE_URL` holds the connection URL
//! of an empty, disposable Postgres database (it is migrated and written to) —
//! for example a throwaway `postgres:17` container — and is skipped otherwise:
//!
//! ```sh
//! SHARING_SERVER_TEST_DATABASE_URL=<connection url> \
//!   cargo test -p opensharing-server --test postgres
//! ```
#![cfg(feature = "postgres")]

use opensharing_server::store::postgres::{PostgresStore, connect, ensure_schema_current, migrate};

#[path = "store/conformance.rs"]
mod conformance;

#[tokio::test]
async fn postgres_store_round_trips() {
    let Ok(url) = std::env::var("SHARING_SERVER_TEST_DATABASE_URL") else {
        eprintln!("SHARING_SERVER_TEST_DATABASE_URL not set; skipping");
        return;
    };
    let pool = connect(&url, 2).await.unwrap();
    assert!(
        ensure_schema_current(&pool).await.is_err(),
        "pending migrations must be reported"
    );
    migrate(&pool).await.unwrap();
    ensure_schema_current(&pool).await.unwrap();
    conformance::round_trips(&PostgresStore::new(pool)).await;
}
