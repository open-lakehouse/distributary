//! The SQLite store, on a fresh database file.
#![cfg(feature = "sqlite")]

use opensharing_server::store::sqlite::{SqliteStore, connect, ensure_schema_current, migrate};

#[path = "store/conformance.rs"]
mod conformance;

#[tokio::test]
async fn sqlite_store_round_trips() {
    let dir = tempfile::tempdir().unwrap();
    let url = format!("sqlite://{}", dir.path().join("sharing.db").display());
    let pool = connect(&url, 2).await.unwrap();
    assert!(
        ensure_schema_current(&pool).await.is_err(),
        "pending migrations must be reported"
    );
    migrate(&pool).await.unwrap();
    ensure_schema_current(&pool).await.unwrap();
    conformance::round_trips(&SqliteStore::new(pool)).await;
}

#[tokio::test]
async fn sqlite_state_survives_reconnecting() {
    use opensharing_server::store::{ShareReader, ShareStore};

    let dir = tempfile::tempdir().unwrap();
    let url = format!("sqlite://{}", dir.path().join("sharing.db").display());
    let pool = connect(&url, 1).await.unwrap();
    migrate(&pool).await.unwrap();
    SqliteStore::new(pool.clone())
        .create_share("kept", None)
        .await
        .unwrap();
    pool.close().await;

    let pool = connect(&url, 1).await.unwrap();
    ensure_schema_current(&pool).await.unwrap();
    assert_eq!(
        SqliteStore::new(pool).get_share("kept").await.unwrap().name,
        "kept"
    );
}
