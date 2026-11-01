//! The SQLite store, on a fresh database file.

use opensharing_sqlite::{SqliteStore, connect, ensure_schema_current, migrate};

use opensharing_provider::testing::round_trips;

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
    round_trips(&SqliteStore::new(pool)).await;
}

#[tokio::test]
async fn sqlite_state_survives_reconnecting() {
    use opensharing_provider::store::{ShareReader, ShareStore};

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

/// Upgrading to digested activation codes keeps activated tokens and voids the
/// links of pending ones, whose codes were stored in clear.
#[tokio::test]
async fn sqlite_upgrade_voids_clear_activation_codes() {
    use opensharing_provider::store::{RecipientStore, TokenReader};
    use opensharing_sqlite::MIGRATOR;
    use sqlx::migrate::Migrator;

    let dir = tempfile::tempdir().unwrap();
    let url = format!("sqlite://{}", dir.path().join("sharing.db").display());
    let pool = connect(&url, 1).await.unwrap();
    let initial = Migrator {
        migrations: MIGRATOR
            .iter()
            .filter(|m| m.version == 1)
            .cloned()
            .collect(),
        ..Migrator::DEFAULT
    };
    initial.run(&pool).await.unwrap();
    sqlx::query("INSERT INTO recipients (id, name, created_at) VALUES ('r1', 'acme', 1)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO recipient_tokens \
         (id, recipient_id, token_hash, sealed_token, activation_code, created_at) VALUES \
         ('active', 'r1', x'01', NULL, NULL, 1), ('pending', 'r1', x'02', x'09', 'code', 2)",
    )
    .execute(&pool)
    .await
    .unwrap();

    migrate(&pool).await.unwrap();
    ensure_schema_current(&pool).await.unwrap();
    let store = SqliteStore::new(pool);
    let (active, recipient) = store.token_by_hash(&[1]).await.unwrap();
    assert_eq!(
        (active.id.as_str(), recipient.name.as_str()),
        ("active", "acme")
    );
    let (pending, _) = store.token_by_hash(&[2]).await.unwrap();
    assert_eq!(
        (pending.sealed_token, pending.activation_code_hash),
        (None, None)
    );
    assert!(store.sealed_tokens().await.unwrap().is_empty());
    // The rebuilt table still cascades from its recipient.
    store.delete_recipient("acme").await.unwrap();
    assert!(store.token_by_hash(&[1]).await.is_err());
}
