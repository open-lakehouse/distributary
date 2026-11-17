//! The handler traits of the admin API: one per service, generic over the
//! request context `Cx` and with the error type of the host.
//!
//! The methods and the models are those of Unity Catalog's generated traits
//! (`olai-uc-handlers`). The difference is [`Error`](ShareHandler::Error):
//! a host answers with its own error type, so its codes, messages and body
//! are the ones of its other routes. A host without one uses
//! [`crate::Error`].

use async_trait::async_trait;

use crate::RequestContext;
use crate::providers::v1::*;
use crate::recipients::v1::*;
use crate::shares::v1::*;

/// Shares and the permissions on them.
#[async_trait]
pub trait ShareHandler<Cx = RequestContext>: Send + Sync + 'static {
    /// What a failed call answers with.
    type Error: Send + 'static;

    /// List shares.
    async fn list_shares(
        &self,
        request: ListSharesRequest,
        context: Cx,
    ) -> Result<ListSharesResponse, Self::Error>;
    /// Create a new share.
    async fn create_share(
        &self,
        request: CreateShareRequest,
        context: Cx,
    ) -> Result<Share, Self::Error>;
    /// Get a share by name.
    async fn get_share(&self, request: GetShareRequest, context: Cx) -> Result<Share, Self::Error>;
    /// Update a share.
    async fn update_share(
        &self,
        request: UpdateShareRequest,
        context: Cx,
    ) -> Result<Share, Self::Error>;
    /// Deletes a share.
    async fn delete_share(
        &self,
        request: DeleteShareRequest,
        context: Cx,
    ) -> Result<(), Self::Error>;
    /// Gets the permissions for a data share from the metastore.
    async fn get_permissions(
        &self,
        request: GetPermissionsRequest,
        context: Cx,
    ) -> Result<GetPermissionsResponse, Self::Error>;
    /// Updates the permissions for a data share in the metastore.
    async fn update_permissions(
        &self,
        request: UpdatePermissionsRequest,
        context: Cx,
    ) -> Result<UpdatePermissionsResponse, Self::Error>;
}

/// Recipients and their tokens.
#[async_trait]
pub trait RecipientHandler<Cx = RequestContext>: Send + Sync + 'static {
    /// What a failed call answers with.
    type Error: Send + 'static;

    /// List recipients.
    async fn list_recipients(
        &self,
        request: ListRecipientsRequest,
        context: Cx,
    ) -> Result<ListRecipientsResponse, Self::Error>;
    /// Create a new recipient.
    async fn create_recipient(
        &self,
        request: CreateRecipientRequest,
        context: Cx,
    ) -> Result<Recipient, Self::Error>;
    /// Get a recipient by name.
    async fn get_recipient(
        &self,
        request: GetRecipientRequest,
        context: Cx,
    ) -> Result<Recipient, Self::Error>;
    /// Update a recipient.
    async fn update_recipient(
        &self,
        request: UpdateRecipientRequest,
        context: Cx,
    ) -> Result<Recipient, Self::Error>;
    /// Delete a recipient.
    async fn delete_recipient(
        &self,
        request: DeleteRecipientRequest,
        context: Cx,
    ) -> Result<(), Self::Error>;
    /// Rotate a recipient's token: issue a new token (and activation link) and
    /// retire the existing one.
    async fn rotate_recipient_token(
        &self,
        request: RotateRecipientTokenRequest,
        context: Cx,
    ) -> Result<Recipient, Self::Error>;
}

/// Providers: the organizations that share data with this metastore.
#[async_trait]
pub trait ProviderHandler<Cx = RequestContext>: Send + Sync + 'static {
    /// What a failed call answers with.
    type Error: Send + 'static;

    /// List providers.
    async fn list_providers(
        &self,
        request: ListProvidersRequest,
        context: Cx,
    ) -> Result<ListProvidersResponse, Self::Error>;
    /// Create a new provider.
    async fn create_provider(
        &self,
        request: CreateProviderRequest,
        context: Cx,
    ) -> Result<Provider, Self::Error>;
    /// Get a provider by name.
    async fn get_provider(
        &self,
        request: GetProviderRequest,
        context: Cx,
    ) -> Result<Provider, Self::Error>;
    /// Update a provider.
    async fn update_provider(
        &self,
        request: UpdateProviderRequest,
        context: Cx,
    ) -> Result<Provider, Self::Error>;
    /// Delete a provider.
    async fn delete_provider(
        &self,
        request: DeleteProviderRequest,
        context: Cx,
    ) -> Result<(), Self::Error>;
}
