//! Local storage access through `olai-uc-storage-access`: the credentials, and
//! the external locations they govern, that local-mode sources mint from.
//!
//! The store keeps its own tables in the server's database (migrations 02xx,
//! merged into the store crate's migrator by [`crate::store`]) and seals the
//! credential secrets with the server's [`Secrets`](opensharing_provider::secrets::Secrets).
//! [`StorageAccessResolver`] resolves a storage path to the credential of the
//! location governing it, from an index.

use std::sync::Arc;

use async_trait::async_trait;
use olai_credential_vending::{CredentialResolver, RegisteredCredential, Result};
use olai_storage_credentials::StorageLocation;
pub use unitycatalog_storage_access::{InMemoryStorageAccess, StorageAccessStore};

/// The [`CredentialResolver`] over a [`StorageAccessStore`].
#[derive(Clone)]
pub struct StorageAccessResolver(pub Arc<dyn StorageAccessStore>);

#[async_trait]
impl CredentialResolver for StorageAccessResolver {
    async fn resolve(&self, location: &StorageLocation) -> Result<RegisteredCredential> {
        self.0.as_ref().resolve(location).await
    }
}

#[cfg(test)]
mod tests {
    use olai_credential_vending::Error;
    use unitycatalog_common::models::credentials::v1::{AwsIamRoleConfig, Credential, Purpose};
    use unitycatalog_common::models::external_locations::v1::ExternalLocation;

    use super::*;

    #[tokio::test]
    async fn resolves_the_governing_credential() {
        let store = Arc::new(InMemoryStorageAccess::new());
        store
            .create_credential(Credential {
                name: "cred".into(),
                purpose: Purpose::STORAGE.into(),
                aws_iam_role: AwsIamRoleConfig {
                    role_arn: "arn:aws:iam::123456789012:role/sharing".into(),
                    ..Default::default()
                }
                .into(),
                ..Default::default()
            })
            .await
            .unwrap();
        store
            .create_external_location(ExternalLocation {
                name: "sales".into(),
                url: "s3://bucket/sales".into(),
                credential_name: "cred".into(),
                ..Default::default()
            })
            .await
            .unwrap();
        let resolver = StorageAccessResolver(store);

        let nested = "s3://bucket/sales/orders/part-0.parquet".parse().unwrap();
        assert!(matches!(
            resolver.resolve(&nested).await.unwrap(),
            RegisteredCredential::AwsIamRole(_)
        ));
        let elsewhere = "s3://bucket/marketing".parse().unwrap();
        assert!(matches!(
            resolver.resolve(&elsewhere).await,
            Err(Error::NotFound { .. })
        ));
    }
}
