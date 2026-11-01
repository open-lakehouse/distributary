//! The Unity Catalog storage routes of the provider admin API: `credentials`
//! and `external-locations`, over the [`StorageAccessStore`] local access mints
//! from. Credential secrets are sealed and never returned.
//!
//! The routes and request types are the ones `olai-uc-handlers` generates for a
//! Unity Catalog server; [`StorageAdmin`] implements their handler traits. The
//! store owns the rules of the pair (a location names an existing credential,
//! locations never overlap, a credential in use cannot be deleted). Each
//! handler first checks the same
//! [`AdminPolicy`](opensharing_provider::admin::AdminPolicy) as the share and
//! recipient routes, with [`AdminAction::ManageStorage`].
//!
//! [`routes`] is merged into the [admin routes](crate::embed::admin_routes),
//! behind the same provider-token check.

use std::sync::Arc;

use async_trait::async_trait;
use axum::Router;
use axum::extract::FromRequestParts;
use axum::routing::get;
use unitycatalog_common::models::credentials::v1::{
    CreateCredentialRequest, Credential, DeleteCredentialRequest, GetCredentialRequest,
    ListCredentialsRequest, ListCredentialsResponse, Purpose, UpdateCredentialRequest,
};
use unitycatalog_common::models::external_locations::v1::{
    CreateExternalLocationRequest, DeleteExternalLocationRequest, ExternalLocation,
    GetExternalLocationRequest, ListExternalLocationsRequest, ListExternalLocationsResponse,
    UpdateExternalLocationRequest,
};
use unitycatalog_handlers::credentials::CredentialHandler;
use unitycatalog_handlers::external_locations::ExternalLocationHandler;
use unitycatalog_handlers::{Error, Result};
use unitycatalog_storage_access::{
    CredentialSecrets, CredentialUpdate, ExternalLocationUpdate, Page, StorageAccessStore,
};

use opensharing_provider::admin::{AdminAction, ProviderService, identifier};

/// The credential and external-location handlers: the admin policy of
/// `service`, over `storage`. Cheap to clone.
pub struct StorageAdmin<Cx> {
    service: ProviderService<Cx>,
    storage: Arc<dyn StorageAccessStore>,
}

impl<Cx> Clone for StorageAdmin<Cx> {
    fn clone(&self) -> Self {
        Self {
            service: self.service.clone(),
            storage: self.storage.clone(),
        }
    }
}

impl<Cx: Send + Sync + 'static> StorageAdmin<Cx> {
    pub fn new(service: ProviderService<Cx>, storage: Arc<dyn StorageAccessStore>) -> Self {
        Self { service, storage }
    }

    async fn authorize(&self, context: &Cx) -> Result<()> {
        self.service
            .authorize(AdminAction::ManageStorage, context)
            .await
            .map_err(provider_error)
    }
}

/// The admin policy's and name checks' errors, as the UC handlers report them.
fn provider_error(e: opensharing_provider::Error) -> Error {
    use opensharing_provider::Error as P;
    match e {
        P::NotFound => Error::NotFound,
        P::NotAllowed => Error::NotAllowed,
        P::Unauthenticated => Error::Unauthenticated,
        P::InvalidArgument(message) => Error::InvalidArgument(message),
        P::NotImplemented(what) => Error::NotImplemented(what),
        other => Error::generic(other),
    }
}

fn page(max_results: Option<i32>, token: Option<String>) -> Result<Page> {
    let max_results = match max_results {
        Some(max) if max < 0 => {
            return Err(Error::invalid_argument("max_results must not be negative"));
        }
        Some(max) if max > 0 => Some(max as usize),
        _ => None,
    };
    Ok(Page::new(max_results, token))
}

/// The credential and external-location routes over `admin`, to merge into
/// [`admin_routes`](crate::embed::admin_routes).
pub fn routes<Cx>(admin: StorageAdmin<Cx>) -> Router
where
    Cx: FromRequestParts<StorageAdmin<Cx>> + Send + Sync + 'static,
{
    use unitycatalog_handlers::credentials::server as credentials;
    use unitycatalog_handlers::external_locations::server as locations;

    type S<Cx> = StorageAdmin<Cx>;
    Router::new()
        .route(
            "/credentials",
            get(credentials::list_credentials::<S<Cx>, Cx>)
                .post(credentials::create_credential::<S<Cx>, Cx>),
        )
        .route(
            "/credentials/{name}",
            get(credentials::get_credential::<S<Cx>, Cx>)
                .patch(credentials::update_credential::<S<Cx>, Cx>)
                .delete(credentials::delete_credential::<S<Cx>, Cx>),
        )
        .route(
            "/external-locations",
            get(locations::list_external_locations::<S<Cx>, Cx>)
                .post(locations::create_external_location::<S<Cx>, Cx>),
        )
        .route(
            "/external-locations/{name}",
            get(locations::get_external_location::<S<Cx>, Cx>)
                .patch(locations::update_external_location::<S<Cx>, Cx>)
                .delete(locations::delete_external_location::<S<Cx>, Cx>),
        )
        .with_state(admin)
}

#[async_trait]
impl<Cx: Send + Sync + 'static> CredentialHandler<Cx> for StorageAdmin<Cx> {
    async fn list_credentials(
        &self,
        request: ListCredentialsRequest,
        context: Cx,
    ) -> Result<ListCredentialsResponse> {
        self.authorize(&context).await?;
        let purpose = request
            .purpose
            .and_then(|p| p.as_known())
            .filter(|p| *p != Purpose::PURPOSE_UNSPECIFIED);
        let (credentials, next_page_token) = self
            .storage
            .list_credentials(purpose, page(request.max_results, request.page_token)?)
            .await?;
        Ok(ListCredentialsResponse {
            credentials,
            next_page_token,
            ..Default::default()
        })
    }

    async fn create_credential(
        &self,
        request: CreateCredentialRequest,
        context: Cx,
    ) -> Result<Credential> {
        self.authorize(&context).await?;
        identifier(&request.name).map_err(provider_error)?;
        let credential = Credential {
            name: request.name,
            comment: request.comment,
            purpose: request.purpose,
            read_only: request.read_only.unwrap_or(false),
            azure_managed_identity: request.azure_managed_identity,
            azure_service_principal: request.azure_service_principal,
            azure_storage_key: request.azure_storage_key,
            aws_iam_role: request.aws_iam_role,
            databricks_gcp_service_account: request.databricks_gcp_service_account,
            ..Default::default()
        };
        if CredentialSecrets::take(&mut credential.clone()).is_empty() {
            return Err(Error::invalid_argument(
                "a credential needs one of aws_iam_role, azure_service_principal, \
                 azure_managed_identity, azure_storage_key or databricks_gcp_service_account",
            ));
        }
        Ok(self.storage.create_credential(credential).await?)
    }

    async fn get_credential(
        &self,
        request: GetCredentialRequest,
        context: Cx,
    ) -> Result<Credential> {
        self.authorize(&context).await?;
        Ok(self.storage.get_credential(&request.name).await?)
    }

    /// `purpose` is fixed; the secrets are replaced only when the request sets
    /// one.
    async fn update_credential(
        &self,
        request: UpdateCredentialRequest,
        context: Cx,
    ) -> Result<Credential> {
        self.authorize(&context).await?;
        if let Some(name) = &request.new_name {
            identifier(name).map_err(provider_error)?;
        }
        let secrets = CredentialSecrets::take(&mut Credential {
            azure_managed_identity: request.azure_managed_identity,
            azure_service_principal: request.azure_service_principal,
            azure_storage_key: request.azure_storage_key,
            aws_iam_role: request.aws_iam_role,
            databricks_gcp_service_account: request.databricks_gcp_service_account,
            ..Default::default()
        });
        let update = CredentialUpdate {
            new_name: request.new_name,
            comment: request.comment,
            read_only: request.read_only,
            owner: request.owner,
            secrets: (!secrets.is_empty()).then_some(secrets),
        };
        Ok(self
            .storage
            .update_credential(&request.name, update)
            .await?)
    }

    /// Refused while an external location uses the credential.
    async fn delete_credential(&self, request: DeleteCredentialRequest, context: Cx) -> Result<()> {
        self.authorize(&context).await?;
        Ok(self.storage.delete_credential(&request.name).await?)
    }
}

#[async_trait]
impl<Cx: Send + Sync + 'static> ExternalLocationHandler<Cx> for StorageAdmin<Cx> {
    async fn list_external_locations(
        &self,
        request: ListExternalLocationsRequest,
        context: Cx,
    ) -> Result<ListExternalLocationsResponse> {
        self.authorize(&context).await?;
        let (external_locations, next_page_token) = self
            .storage
            .list_external_locations(page(request.max_results, request.page_token)?)
            .await?;
        Ok(ListExternalLocationsResponse {
            external_locations,
            next_page_token,
            ..Default::default()
        })
    }

    /// Refused when the URL overlaps a registered location, in either
    /// direction, so a storage path has at most one governing credential.
    async fn create_external_location(
        &self,
        request: CreateExternalLocationRequest,
        context: Cx,
    ) -> Result<ExternalLocation> {
        self.authorize(&context).await?;
        identifier(&request.name).map_err(provider_error)?;
        let location = ExternalLocation {
            name: request.name,
            url: request.url,
            credential_name: request.credential_name,
            read_only: request.read_only.unwrap_or(false),
            comment: request.comment,
            ..Default::default()
        };
        Ok(self.storage.create_external_location(location).await?)
    }

    async fn get_external_location(
        &self,
        request: GetExternalLocationRequest,
        context: Cx,
    ) -> Result<ExternalLocation> {
        self.authorize(&context).await?;
        Ok(self.storage.get_external_location(&request.name).await?)
    }

    async fn update_external_location(
        &self,
        request: UpdateExternalLocationRequest,
        context: Cx,
    ) -> Result<ExternalLocation> {
        self.authorize(&context).await?;
        if let Some(name) = &request.new_name {
            identifier(name).map_err(provider_error)?;
        }
        let update = ExternalLocationUpdate {
            new_name: request.new_name,
            url: request.url,
            credential_name: request.credential_name,
            read_only: request.read_only,
            comment: request.comment,
            owner: request.owner,
        };
        Ok(self
            .storage
            .update_external_location(&request.name, update)
            .await?)
    }

    async fn delete_external_location(
        &self,
        request: DeleteExternalLocationRequest,
        context: Cx,
    ) -> Result<()> {
        self.authorize(&context).await?;
        Ok(self.storage.delete_external_location(&request.name).await?)
    }
}
