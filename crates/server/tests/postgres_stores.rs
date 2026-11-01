//! The server's Postgres stores against a live database: the store's and the
//! storage-access tables migrated as one ledger, and both stores built over the
//! one pool.
//!
//! Runs only when `SHARING_SERVER_TEST_DATABASE_URL` holds the connection URL
//! of a disposable Postgres server, and is skipped otherwise. It (re)creates a
//! database of its own there, `sharing_server_stores`, so it can run alongside
//! the store crate's test of the same URL.
#![cfg(all(feature = "postgres", feature = "unity"))]

use opensharing_provider::secrets::Secrets;
use opensharing_server::config::{Config, ConfigValue, DatabaseConfig, EncryptionConfig};
use opensharing_server::store::{postgres, postgres_migrator};
use unitycatalog_common::models::credentials::v1::{AzureStorageKey, Credential, Purpose};
use unitycatalog_common::models::external_locations::v1::ExternalLocation;

#[tokio::test]
async fn postgres_stores_share_one_migrated_database() {
    let Ok(server) = std::env::var("SHARING_SERVER_TEST_DATABASE_URL") else {
        eprintln!("SHARING_SERVER_TEST_DATABASE_URL not set; skipping");
        return;
    };
    let admin = postgres::connect(&server, 1).await.unwrap();
    for statement in [
        "DROP DATABASE IF EXISTS sharing_server_stores",
        "CREATE DATABASE sharing_server_stores",
    ] {
        sqlx::query(statement).execute(&admin).await.unwrap();
    }
    admin.close().await;
    let mut url = url::Url::parse(&server).unwrap();
    url.set_path("/sharing_server_stores");
    let url = url.to_string();
    let config = Config {
        database: DatabaseConfig::Postgres {
            url: ConfigValue::Value(url.clone()),
            migration_url: None,
            max_connections: 2,
        },
        ..Config::default()
    };
    let secrets = Secrets::new(EncryptionConfig::insecure_dev().build().unwrap());
    assert!(
        config.stores(&secrets).await.is_err(),
        "an unmigrated database must be refused"
    );

    let pool = postgres::connect(&url, 1).await.unwrap();
    postgres::migrate(&postgres_migrator(), &pool)
        .await
        .unwrap();
    // The store's own migrator alone does not know the storage-access tables.
    assert!(
        postgres::ensure_schema_current(&postgres::MIGRATOR, &pool)
            .await
            .is_err()
    );
    pool.close().await;

    let stores = config.stores(&secrets).await.unwrap();
    let storage = stores.storage_access.unwrap();
    storage
        .create_credential(Credential {
            name: "Storage".into(),
            purpose: Purpose::STORAGE.into(),
            azure_storage_key: AzureStorageKey {
                account_name: "acct".into(),
                account_key: "a2V5".into(),
                ..Default::default()
            }
            .into(),
            ..Default::default()
        })
        .await
        .unwrap();
    storage
        .create_external_location(ExternalLocation {
            name: "data".into(),
            url: "abfss://data@acct.dfs.core.windows.net/".into(),
            // Names are case-insensitive.
            credential_name: "storage".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    let governed = storage
        .governing_location(&"abfss://data@acct.dfs.core.windows.net/x".parse().unwrap())
        .await
        .unwrap();
    assert_eq!(governed.location.name, "data");
}
