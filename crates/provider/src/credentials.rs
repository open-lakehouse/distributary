//! [`StoreCredentialResolver`]: the credentials local-mode storage access mints
//! from, read from the store's external locations and sealed credentials.
//!
//! It is the [`CredentialResolver`] the provider hands
//! [`SourceStorageAccess`](opensharing_engine::vending::SourceStorageAccess).

use std::sync::Arc;

use async_trait::async_trait;
use olai_credential_vending::{CredentialResolver, RegisteredCredential};
use olai_storage_credentials::location::longest_match;
use olai_storage_credentials::{StorageLocation, StorageScope};

use crate::secrets::Secrets;
use crate::store::CredentialStore;
use crate::{Error, Result};

/// Resolves a location's credential from the store's external locations and
/// sealed credentials, re-wrapping credentials sealed under a retired key.
pub struct StoreCredentialResolver {
    store: Arc<dyn CredentialStore>,
    secrets: Secrets,
}

impl StoreCredentialResolver {
    pub fn new(store: Arc<dyn CredentialStore>, secrets: Secrets) -> Self {
        Self { store, secrets }
    }

    /// Open the stored credential `name`, migrating it to the active key.
    pub(crate) async fn open(&self, name: &str) -> Result<RegisteredCredential> {
        open_credential(self.store.as_ref(), &self.secrets, name).await
    }
}

/// Open the stored credential `name`, writing it back re-wrapped if it was sealed
/// under a retired key.
///
/// Credentials sealed as a full Unity Catalog `Credential` document (as the admin
/// API stored them before) open too: the metadata keys are ignored.
pub async fn open_credential(
    store: &dyn CredentialStore,
    secrets: &Secrets,
    name: &str,
) -> Result<RegisteredCredential> {
    let sealed = store.get_credential(name).await?;
    let (plaintext, rewrapped) = secrets.open(&credential_aad(name), &sealed.sealed).await?;
    if let Some(sealed) = rewrapped {
        store
            .put_credential(crate::store::SealedCredential {
                name: name.to_string(),
                sealed,
            })
            .await?;
    }
    Ok(serde_json::from_slice(&plaintext)?)
}

/// The AAD a stored credential is sealed under.
pub fn credential_aad(name: &str) -> String {
    format!("credential:{name}")
}

#[async_trait]
impl CredentialResolver for StoreCredentialResolver {
    async fn resolve(
        &self,
        location: &StorageLocation,
    ) -> olai_credential_vending::Result<RegisteredCredential> {
        let to_vending = |e: Error| olai_credential_vending::Error::mint(e.to_string());
        let locations = self
            .store
            .list_external_locations()
            .await
            .map_err(to_vending)?;
        // Registrations whose URL no longer parses govern nothing.
        let scopes: Vec<_> = locations
            .iter()
            .filter_map(|l| Some((l.url.parse::<StorageScope>().ok()?, l)))
            .collect();
        let governing =
            longest_match(scopes.iter().map(|(s, l)| (s, *l)), location).ok_or_else(|| {
                olai_credential_vending::Error::NotFound {
                    location: location.to_string(),
                }
            })?;
        // A credential minted here covers everything below `location`, so refuse a
        // location containing one another credential governs.
        if let Some((nested, _)) = scopes.iter().find(|(scope, other)| {
            other.credential_name != governing.credential_name
                && scope.location().starts_with(location)
                && scope.location() != location
        }) {
            return Err(olai_credential_vending::Error::Overlapping {
                location: location.to_string(),
                nested: nested.to_string(),
            });
        }
        self.open(&governing.credential_name)
            .await
            .map_err(to_vending)
    }
}
