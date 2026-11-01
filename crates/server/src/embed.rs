//! Assemble the server as a socket-less [`Router`], for any host: the
//! `sharing-server` binary, an embedding application, or a serverless function.

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::routing::get;
use opensharing_api::auth::require_bearer;
use opensharing_api::sharing_router;
use opensharing_engine::SharingService;
use opensharing_engine::source::SourceRegistry;
use opensharing_engine::vending::NoCredentials;
use opensharing_provider::recipients::{
    Recipient, StoreAuthenticator, activation_router, rewrap_tokens,
};
use opensharing_provider::secrets::Secrets;
use opensharing_provider::{AccessSettings, sharing_service};

use crate::Result;
use crate::config::{Config, DatabaseConfig, EncryptionConfig, StorageCredentials};
use crate::store::SharingStore;
use crate::store::memory::InMemoryStore;
#[cfg(feature = "unity")]
use crate::unity::storage_access::{
    InMemoryStorageAccess, StorageAccessResolver, StorageAccessStore,
};

/// Everything the routes need, besides the store.
#[derive(Clone)]
pub struct Settings {
    /// The externally reachable base URL, without a trailing slash.
    pub public_url: String,
    /// Accepted provider (admin) tokens. With none, [`start`] does not mount
    /// the admin API.
    pub provider_tokens: Vec<String>,
    /// The longest a presigned URL stays valid.
    pub presign_ttl: Duration,
    /// The S3 region used for presigning.
    pub aws_region: Option<String>,
}

/// The stores a server runs on, over one database.
#[derive(Clone)]
pub struct Stores {
    /// Shares, recipients, tokens and grants.
    pub sharing: Arc<dyn SharingStore>,
    /// The credentials and external locations local-mode sources mint from
    /// (feature `unity`); `None` when local access serves `file://` only.
    #[cfg(feature = "unity")]
    pub storage_access: Option<Arc<dyn StorageAccessStore>>,
}

impl Stores {
    /// Over `sharing`, with no stored storage credentials.
    pub fn new(sharing: Arc<dyn SharingStore>) -> Self {
        Self {
            sharing,
            #[cfg(feature = "unity")]
            storage_access: None,
        }
    }

    /// With `storage_access` as the credentials local access mints from.
    #[cfg(feature = "unity")]
    pub fn with_storage_access(mut self, storage_access: Arc<dyn StorageAccessStore>) -> Self {
        self.storage_access = Some(storage_access);
        self
    }

    /// Everything in memory (storage credentials too, with `unity`).
    pub fn memory() -> Self {
        let stores = Self::new(Arc::new(InMemoryStore::new()));
        #[cfg(feature = "unity")]
        let stores = stores.with_storage_access(Arc::new(InMemoryStorageAccess::new()));
        stores
    }
}

/// An assembled server.
pub struct Service {
    pub router: Router,
    pub stores: Stores,
    pub secrets: Secrets,
}

/// The recipient-facing router over existing stores, secrets and sources:
///
/// - the sharing protocols under `/api/v1/delta-sharing` and
///   `/api/1.0/opensharing`, behind recipient tokens;
/// - activation links under `/api/1.0/opensharing/activations/{code}`;
/// - `GET /health`.
///
/// Local-mode sources mint from the stored credentials (`stores.storage_access`),
/// or serve `file://` only without them.
///
/// The provider side is separate ([`admin_router`], feature `admin`). A host
/// with its own admin surface manages shares, recipients and grants through the
/// [`SharingStore`] directly (see
/// [`create_recipient`](opensharing_provider::recipients::create_recipient)),
/// or mounts the generated admin routers over its own context (see
/// [`opensharing_provider::admin`]).
pub fn router(
    stores: &Stores,
    secrets: Secrets,
    sources: SourceRegistry,
    settings: Settings,
) -> Router {
    let store = stores.sharing.clone();
    let access = AccessSettings {
        presign_ttl: settings.presign_ttl,
        aws_region: settings.aws_region,
    };
    #[cfg(feature = "unity")]
    let service = match &stores.storage_access {
        Some(storage) => sharing_service::<Recipient, _>(
            store.clone(),
            sources,
            StorageAccessResolver(storage.clone()),
            access,
        ),
        None => sharing_service::<Recipient, _>(store.clone(), sources, NoCredentials, access),
    };
    #[cfg(not(feature = "unity"))]
    let service = sharing_service::<Recipient, _>(store.clone(), sources, NoCredentials, access);
    let sharing = require_bearer(
        sharing_router::<SharingService<Recipient>, Recipient>(service),
        Arc::new(StoreAuthenticator::new(store.clone())),
    );
    Router::new()
        .merge(sharing)
        .merge(activation_router(store, secrets, settings.public_url))
        .route("/health", get(|| async { "ok" }))
}

/// The provider admin routes of `service`, unmounted: shares, permissions and
/// recipients, plus (feature `unity`) the credentials and external locations of
/// `stores`, when it keeps them. [`mount`](opensharing_provider::admin::mount)
/// them behind an authenticator.
#[cfg(feature = "admin")]
pub fn admin_routes(
    service: opensharing_provider::admin::ProviderService<opensharing_admin_api::auth::Provider>,
    stores: &Stores,
) -> Router {
    let routes = opensharing_provider::admin::routes(service.clone());
    #[cfg(feature = "unity")]
    let routes = match &stores.storage_access {
        Some(storage) => routes.merge(crate::unity::admin::routes(
            crate::unity::admin::StorageAdmin::new(service, storage.clone()),
        )),
        None => routes,
    };
    #[cfg(not(feature = "unity"))]
    let _ = stores;
    routes
}

/// The provider admin API under `/api/2.1/unity-catalog` (see
/// [`admin_routes`]), behind the provider tokens. Merge it into [`router`].
#[cfg(feature = "admin")]
pub fn admin_router(stores: &Stores, secrets: Secrets, settings: &Settings) -> Router {
    use opensharing_provider::admin::{self, ProviderService, ProviderTokens};

    admin::mount(
        admin_routes(
            ProviderService::new(stores.sharing.clone(), secrets, settings.public_url.clone()),
            stores,
        ),
        Arc::new(ProviderTokens::new(&settings.provider_tokens)),
    )
}

impl Config {
    /// The secrets encryptor. The in-memory store falls back to an insecure
    /// development key; anything persistent requires `encryption`.
    pub fn secrets(&self) -> std::result::Result<Secrets, String> {
        let encryption = match (&self.encryption, &self.database) {
            (Some(encryption), _) => encryption.clone(),
            (None, DatabaseConfig::Memory) => {
                tracing::warn!("no `encryption` configured; using an insecure development key");
                EncryptionConfig::insecure_dev()
            }
            (None, _) => {
                return Err("`encryption` must be configured for a persistent store".to_string());
            }
        };
        Ok(Secrets::new(encryption.build()?))
    }

    pub fn settings(&self) -> std::result::Result<Settings, String> {
        Ok(Settings {
            public_url: self.public_url(),
            provider_tokens: self.provider_tokens()?,
            presign_ttl: Duration::from_secs(self.presign_ttl_secs),
            aws_region: self.aws_region.clone(),
        })
    }

    /// Connect the configured stores. For the SQL stores this checks the
    /// schema is current (for Postgres over the migration URL) but never
    /// migrates. Stored storage credentials are sealed with `secrets`.
    pub async fn stores(&self, secrets: &Secrets) -> std::result::Result<Stores, String> {
        // Only the SQL storage-access stores seal.
        #[cfg(not(all(feature = "unity", any(feature = "postgres", feature = "sqlite"))))]
        let _ = secrets;
        let local = self.storage_credentials == StorageCredentials::Local;
        let stores = match &self.database {
            DatabaseConfig::Memory => {
                let stores = Stores::memory();
                #[cfg(feature = "unity")]
                let stores = Stores {
                    storage_access: stores.storage_access.filter(|_| local),
                    ..stores
                };
                stores
            }
            #[cfg(feature = "postgres")]
            DatabaseConfig::Postgres {
                url,
                migration_url,
                max_connections,
            } => {
                use crate::store::{postgres, postgres_migrator};
                let url = url.value().ok_or("database url is not set")?;
                let check_url = migration_url
                    .as_ref()
                    .and_then(|u| u.value())
                    .unwrap_or_else(|| url.clone());
                let check = postgres::connect(&check_url, 1)
                    .await
                    .map_err(|e| e.to_string())?;
                postgres::ensure_schema_current(&postgres_migrator(), &check)
                    .await
                    .map_err(|e| e.to_string())?;
                check.close().await;
                let pool = postgres::connect(&url, *max_connections)
                    .await
                    .map_err(|e| e.to_string())?;
                let stores = Stores::new(Arc::new(postgres::PostgresStore::new(pool.clone())));
                #[cfg(feature = "unity")]
                let stores = if local {
                    stores.with_storage_access(Arc::new(
                        unitycatalog_storage_access::PgStorageAccess::new(
                            pool,
                            secrets.encryptor().clone(),
                        ),
                    ))
                } else {
                    stores
                };
                stores
            }
            #[cfg(not(feature = "postgres"))]
            DatabaseConfig::Postgres { .. } => {
                return Err("this build has no Postgres support (feature `postgres`)".to_string());
            }
            #[cfg(feature = "sqlite")]
            DatabaseConfig::Sqlite {
                url,
                max_connections,
            } => {
                use crate::store::{sqlite, sqlite_migrator};
                let url = url.value().ok_or("database url is not set")?;
                let pool = sqlite::connect(&url, *max_connections)
                    .await
                    .map_err(|e| e.to_string())?;
                sqlite::ensure_schema_current(&sqlite_migrator(), &pool)
                    .await
                    .map_err(|e| e.to_string())?;
                let stores = Stores::new(Arc::new(sqlite::SqliteStore::new(pool.clone())));
                #[cfg(feature = "unity")]
                let stores = if local {
                    stores.with_storage_access(Arc::new(
                        unitycatalog_storage_access::SqliteStorageAccess::new(
                            pool,
                            secrets.encryptor().clone(),
                        ),
                    ))
                } else {
                    stores
                };
                stores
            }
            #[cfg(not(feature = "sqlite"))]
            DatabaseConfig::Sqlite { .. } => {
                return Err("this build has no SQLite support (feature `sqlite`)".to_string());
            }
        };
        #[cfg(not(feature = "unity"))]
        let _ = local;
        Ok(stores)
    }
}

/// Build the whole service from configuration.
pub async fn start(config: &Config) -> std::result::Result<Service, String> {
    let secrets = config.secrets()?;
    let stores = config.stores(&secrets).await?;
    let settings = config.settings()?;
    let router = router(
        &stores,
        secrets.clone(),
        config.sources()?,
        settings.clone(),
    );
    #[cfg(feature = "admin")]
    let router = if settings.provider_tokens.is_empty() {
        tracing::info!("no `provider_tokens` configured; the admin API is not mounted");
        router
    } else {
        router.merge(admin_router(&stores, secrets.clone(), &settings))
    };
    Ok(Service {
        router,
        stores,
        secrets,
    })
}

/// Re-wrap every stored secret under the active key. Returns how many were
/// re-wrapped (storage credentials, tokens awaiting activation).
pub async fn rotate_keys(stores: &Stores, secrets: &Secrets) -> Result<(usize, usize)> {
    #[cfg(feature = "unity")]
    let credentials = match &stores.storage_access {
        Some(storage) => storage
            .rewrap_secrets()
            .await
            .map_err(|e| crate::Error::generic(format!("re-wrapping storage credentials: {e}")))?,
        None => 0,
    };
    #[cfg(not(feature = "unity"))]
    let credentials = 0;
    let tokens = rewrap_tokens(stores.sharing.as_ref(), secrets).await?;
    Ok((credentials, tokens))
}
