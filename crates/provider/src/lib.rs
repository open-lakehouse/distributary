//! The OpenSharing **provider** domain, as a library.
//!
//! A provider maps shares onto assets in source catalogs, issues recipients
//! their bearer tokens and grants them shares. This crate holds that state and
//! its logic, and plugs it into the protocol on both sides:
//!
//! - [`store`]: shares, recipients, tokens, grants and sealed storage
//!   credentials, behind targeted per-resource traits, with an in-memory
//!   implementation (the SQL ones live in their own crates);
//! - [`secrets`]: envelope encryption with lazy key rotation;
//! - [`recipients`]: recipient tokens with activation links;
//! - `admin` (feature `admin`): the provider admin API service, implementing
//!   the handler traits `opensharing-admin-api` generates;
//! - [`credentials`]: the stored credentials local-mode storage access mints
//!   from.
//!
//! # Plugging into the protocol
//!
//! The provider owns the recipients, so it supplies what the recipient surface
//! needs from them, generic over the host's request context:
//!
//! - [`Recipient`](recipients::Recipient), the context the standalone server
//!   uses, put there by [`StoreAuthenticator`](recipients::StoreAuthenticator)
//!   (a `BearerAuthenticator` over the token store);
//! - [`RecipientIdentity`], which a host implements for its own context;
//! - [`StoreCatalog`], the `ShareCatalog` over the share mapping and the
//!   source catalogs;
//! - [`GrantPolicy`], the `SharingPolicy` over the grants;
//! - [`StoreCredentialResolver`], the `CredentialResolver` local-mode vending
//!   mints from.
//!
//! [`sharing_service`] wires all of them into an `opensharing-engine`
//! `SharingService`.
//!
//! See `ARCHITECTURE.md` for the design.

use std::sync::Arc;
use std::time::Duration;

use opensharing_engine::SharingService;
use opensharing_engine::source::SourceRegistry;
use opensharing_engine::vending::SourceStorageAccess;

#[cfg(feature = "admin")]
pub mod admin;
pub mod catalog;
pub mod credentials;
pub mod error;
pub mod recipients;
pub mod secrets;
pub mod store;
#[cfg(feature = "testing")]
pub mod testing;
mod tokens;

pub use error::{Error, Result};

use crate::catalog::{GrantPolicy, StoreCatalog};
use crate::credentials::StoreCredentialResolver;
use crate::secrets::Secrets;
use crate::store::SharingStore;

/// Who the caller in a request context is, as far as the provider's grants go.
///
/// The provider grants shares to recipients by id. A host that serves the
/// protocol with its own context implements this to reuse [`StoreCatalog`] and
/// [`GrantPolicy`].
pub trait RecipientIdentity: Send + Sync + 'static {
    /// The id of the recipient the caller is, or `None` for a caller who is no
    /// recipient (who then sees no shares).
    fn recipient_id(&self) -> Option<&str>;
}

/// How [`sharing_service`] reaches storage.
#[derive(Debug, Clone)]
pub struct AccessSettings {
    /// The longest a presigned URL stays valid.
    pub presign_ttl: Duration,
    /// The S3 region used for presigning.
    pub aws_region: Option<String>,
}

/// The recipient surface over the provider's state: shares resolved through
/// `sources`, authorized by the grants, with storage access delegated to the
/// sources or minted from the stored credentials.
pub fn sharing_service<Cx: RecipientIdentity + Clone>(
    store: Arc<dyn SharingStore>,
    secrets: Secrets,
    sources: SourceRegistry,
    access: AccessSettings,
) -> SharingService<Cx> {
    let catalog = Arc::new(StoreCatalog::new(store.clone(), sources.clone()));
    let policy = Arc::new(GrantPolicy::new(store.clone()));
    let access = Arc::new(SourceStorageAccess::new(
        sources,
        StoreCredentialResolver::new(store, secrets),
        access.presign_ttl,
        access.aws_region,
    ));
    SharingService::new(catalog, access, policy)
}
