//! Assemble the server as a socket-less [`Router`], for any host: the
//! `sharing-server` binary, an embedding application, or a serverless function.

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::routing::get;
use opensharing_api::auth::require_bearer;
use opensharing_api::sharing_router;
use opensharing_engine::SharingService;

use crate::Result;
use opensharing_engine::vending::SourceStorageAccess;

use crate::access::{StoreCredentialResolver, open_credential};
use crate::catalog::{GrantPolicy, StoreCatalog};
use crate::config::{Config, DatabaseConfig, EncryptionConfig};
use crate::recipients::{Recipient, StoreAuthenticator, activation_router, rewrap_tokens};
use crate::secrets::Secrets;
use crate::store::SharingStore;
use crate::store::memory::InMemoryStore;
use opensharing_engine::source::SourceRegistry;

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

/// An assembled server.
pub struct Service {
    pub router: Router,
    pub store: Arc<dyn SharingStore>,
    pub secrets: Secrets,
}

/// The recipient-facing router over an existing store, secrets and sources:
///
/// - the sharing protocols under `/api/v1/delta-sharing` and
///   `/api/1.0/opensharing`, behind recipient tokens;
/// - activation links under `/api/1.0/opensharing/activations/{code}`;
/// - `GET /health`.
///
/// The provider side is separate ([`admin_router`], feature `admin`). A host
/// with its own admin surface manages shares, recipients and grants through the
/// [`SharingStore`] directly (see [`issue_token`](crate::recipients::issue_token)),
/// or mounts the generated admin routers over its own context (see
/// [`crate::admin`]).
pub fn router(
    store: Arc<dyn SharingStore>,
    secrets: Secrets,
    sources: SourceRegistry,
    settings: Settings,
) -> Router {
    let catalog = Arc::new(StoreCatalog::new(store.clone(), sources.clone()));
    let access = Arc::new(SourceStorageAccess::new(
        sources,
        StoreCredentialResolver::new(store.clone(), secrets.clone()),
        settings.presign_ttl,
        settings.aws_region,
    ));
    let policy = Arc::new(GrantPolicy::new(store.clone()));
    let service = SharingService::<Recipient>::new(catalog, access, policy);
    let sharing = require_bearer(
        sharing_router::<SharingService<Recipient>, Recipient>(service),
        Arc::new(StoreAuthenticator::new(store.clone())),
    );
    Router::new()
        .merge(sharing)
        .merge(activation_router(store, secrets, settings.public_url))
        .route("/health", get(|| async { "ok" }))
}

/// The provider admin API under `/api/2.1/unity-catalog` (shares, permissions,
/// recipients; with the `unity` feature also credentials and external
/// locations), behind the provider tokens. Merge it into [`router`].
#[cfg(feature = "admin")]
pub fn admin_router(store: Arc<dyn SharingStore>, secrets: Secrets, settings: &Settings) -> Router {
    use crate::admin::{self, ProviderService, ProviderTokens};

    admin::router(
        ProviderService::new(store, secrets, settings.public_url.clone()),
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

    /// Connect the configured store. For the SQL stores this checks the schema
    /// is current (for Postgres over the migration URL) but never migrates.
    pub async fn store(&self) -> std::result::Result<Arc<dyn SharingStore>, String> {
        match &self.database {
            DatabaseConfig::Memory => Ok(Arc::new(InMemoryStore::new())),
            #[cfg(feature = "postgres")]
            DatabaseConfig::Postgres {
                url,
                migration_url,
                max_connections,
            } => {
                use crate::store::postgres;
                let url = url.value().ok_or("database url is not set")?;
                let check_url = migration_url
                    .as_ref()
                    .and_then(|u| u.value())
                    .unwrap_or_else(|| url.clone());
                let check = postgres::connect(&check_url, 1)
                    .await
                    .map_err(|e| e.to_string())?;
                postgres::ensure_schema_current(&check)
                    .await
                    .map_err(|e| e.to_string())?;
                check.close().await;
                let pool = postgres::connect(&url, *max_connections)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(Arc::new(postgres::PostgresStore::new(pool)))
            }
            #[cfg(not(feature = "postgres"))]
            DatabaseConfig::Postgres { .. } => {
                Err("this build has no Postgres support (feature `postgres`)".to_string())
            }
            #[cfg(feature = "sqlite")]
            DatabaseConfig::Sqlite {
                url,
                max_connections,
            } => {
                use crate::store::sqlite;
                let url = url.value().ok_or("database url is not set")?;
                let pool = sqlite::connect(&url, *max_connections)
                    .await
                    .map_err(|e| e.to_string())?;
                sqlite::ensure_schema_current(&pool)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(Arc::new(sqlite::SqliteStore::new(pool)))
            }
            #[cfg(not(feature = "sqlite"))]
            DatabaseConfig::Sqlite { .. } => {
                Err("this build has no SQLite support (feature `sqlite`)".to_string())
            }
        }
    }
}

/// Build the whole service from configuration.
pub async fn start(config: &Config) -> std::result::Result<Service, String> {
    let store = config.store().await?;
    let secrets = config.secrets()?;
    let settings = config.settings()?;
    let router = router(
        store.clone(),
        secrets.clone(),
        config.sources()?,
        settings.clone(),
    );
    #[cfg(feature = "admin")]
    let router = if settings.provider_tokens.is_empty() {
        tracing::info!("no `provider_tokens` configured; the admin API is not mounted");
        router
    } else {
        router.merge(admin_router(store.clone(), secrets.clone(), &settings))
    };
    Ok(Service {
        router,
        store,
        secrets,
    })
}

/// Re-wrap every stored secret under the active key. Returns how many were
/// re-wrapped (credentials, tokens awaiting activation).
pub async fn rotate_keys(store: &dyn SharingStore, secrets: &Secrets) -> Result<(usize, usize)> {
    let mut credentials = 0;
    for credential in store.list_credentials().await? {
        if secrets.rewrap(&credential.sealed).await?.is_some() {
            // Opening writes the re-wrapped value back.
            open_credential(store, secrets, &credential.name).await?;
            credentials += 1;
        }
    }
    let tokens = rewrap_tokens(store, secrets).await?;
    Ok((credentials, tokens))
}
