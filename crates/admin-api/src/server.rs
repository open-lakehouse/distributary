//! The route functions of the admin API: one per RPC. Each extracts the
//! request context and the request, calls the handler, and answers with the
//! model as JSON or with the handler's error.

use axum::Json;
use axum::extract::{FromRequestParts, State};
use axum::response::IntoResponse;

use crate::handler::{ProviderHandler, RecipientHandler, ShareHandler};
use crate::providers::v1::*;
use crate::recipients::v1::*;
use crate::shares::v1::*;

pub async fn list_shares<T, Cx>(
    State(handler): State<T>,
    context: Cx,
    request: ListSharesRequest,
) -> Result<Json<ListSharesResponse>, T::Error>
where
    T: ShareHandler<Cx> + Clone + Send + Sync + 'static,
    T::Error: IntoResponse,
    Cx: FromRequestParts<T> + Send,
{
    handler.list_shares(request, context).await.map(Json)
}

pub async fn create_share<T, Cx>(
    State(handler): State<T>,
    context: Cx,
    request: CreateShareRequest,
) -> Result<Json<Share>, T::Error>
where
    T: ShareHandler<Cx> + Clone + Send + Sync + 'static,
    T::Error: IntoResponse,
    Cx: FromRequestParts<T> + Send,
{
    handler.create_share(request, context).await.map(Json)
}

pub async fn get_share<T, Cx>(
    State(handler): State<T>,
    context: Cx,
    request: GetShareRequest,
) -> Result<Json<Share>, T::Error>
where
    T: ShareHandler<Cx> + Clone + Send + Sync + 'static,
    T::Error: IntoResponse,
    Cx: FromRequestParts<T> + Send,
{
    handler.get_share(request, context).await.map(Json)
}

pub async fn update_share<T, Cx>(
    State(handler): State<T>,
    context: Cx,
    request: UpdateShareRequest,
) -> Result<Json<Share>, T::Error>
where
    T: ShareHandler<Cx> + Clone + Send + Sync + 'static,
    T::Error: IntoResponse,
    Cx: FromRequestParts<T> + Send,
{
    handler.update_share(request, context).await.map(Json)
}

pub async fn delete_share<T, Cx>(
    State(handler): State<T>,
    context: Cx,
    request: DeleteShareRequest,
) -> Result<(), T::Error>
where
    T: ShareHandler<Cx> + Clone + Send + Sync + 'static,
    T::Error: IntoResponse,
    Cx: FromRequestParts<T> + Send,
{
    handler.delete_share(request, context).await
}

pub async fn get_permissions<T, Cx>(
    State(handler): State<T>,
    context: Cx,
    request: GetPermissionsRequest,
) -> Result<Json<GetPermissionsResponse>, T::Error>
where
    T: ShareHandler<Cx> + Clone + Send + Sync + 'static,
    T::Error: IntoResponse,
    Cx: FromRequestParts<T> + Send,
{
    handler.get_permissions(request, context).await.map(Json)
}

pub async fn update_permissions<T, Cx>(
    State(handler): State<T>,
    context: Cx,
    request: UpdatePermissionsRequest,
) -> Result<Json<UpdatePermissionsResponse>, T::Error>
where
    T: ShareHandler<Cx> + Clone + Send + Sync + 'static,
    T::Error: IntoResponse,
    Cx: FromRequestParts<T> + Send,
{
    handler.update_permissions(request, context).await.map(Json)
}

pub async fn list_recipients<T, Cx>(
    State(handler): State<T>,
    context: Cx,
    request: ListRecipientsRequest,
) -> Result<Json<ListRecipientsResponse>, T::Error>
where
    T: RecipientHandler<Cx> + Clone + Send + Sync + 'static,
    T::Error: IntoResponse,
    Cx: FromRequestParts<T> + Send,
{
    handler.list_recipients(request, context).await.map(Json)
}

pub async fn create_recipient<T, Cx>(
    State(handler): State<T>,
    context: Cx,
    request: CreateRecipientRequest,
) -> Result<Json<Recipient>, T::Error>
where
    T: RecipientHandler<Cx> + Clone + Send + Sync + 'static,
    T::Error: IntoResponse,
    Cx: FromRequestParts<T> + Send,
{
    handler.create_recipient(request, context).await.map(Json)
}

pub async fn get_recipient<T, Cx>(
    State(handler): State<T>,
    context: Cx,
    request: GetRecipientRequest,
) -> Result<Json<Recipient>, T::Error>
where
    T: RecipientHandler<Cx> + Clone + Send + Sync + 'static,
    T::Error: IntoResponse,
    Cx: FromRequestParts<T> + Send,
{
    handler.get_recipient(request, context).await.map(Json)
}

pub async fn update_recipient<T, Cx>(
    State(handler): State<T>,
    context: Cx,
    request: UpdateRecipientRequest,
) -> Result<Json<Recipient>, T::Error>
where
    T: RecipientHandler<Cx> + Clone + Send + Sync + 'static,
    T::Error: IntoResponse,
    Cx: FromRequestParts<T> + Send,
{
    handler.update_recipient(request, context).await.map(Json)
}

pub async fn delete_recipient<T, Cx>(
    State(handler): State<T>,
    context: Cx,
    request: DeleteRecipientRequest,
) -> Result<(), T::Error>
where
    T: RecipientHandler<Cx> + Clone + Send + Sync + 'static,
    T::Error: IntoResponse,
    Cx: FromRequestParts<T> + Send,
{
    handler.delete_recipient(request, context).await
}

pub async fn rotate_recipient_token<T, Cx>(
    State(handler): State<T>,
    context: Cx,
    request: RotateRecipientTokenRequest,
) -> Result<Json<Recipient>, T::Error>
where
    T: RecipientHandler<Cx> + Clone + Send + Sync + 'static,
    T::Error: IntoResponse,
    Cx: FromRequestParts<T> + Send,
{
    handler
        .rotate_recipient_token(request, context)
        .await
        .map(Json)
}

pub async fn list_providers<T, Cx>(
    State(handler): State<T>,
    context: Cx,
    request: ListProvidersRequest,
) -> Result<Json<ListProvidersResponse>, T::Error>
where
    T: ProviderHandler<Cx> + Clone + Send + Sync + 'static,
    T::Error: IntoResponse,
    Cx: FromRequestParts<T> + Send,
{
    handler.list_providers(request, context).await.map(Json)
}

pub async fn create_provider<T, Cx>(
    State(handler): State<T>,
    context: Cx,
    request: CreateProviderRequest,
) -> Result<Json<Provider>, T::Error>
where
    T: ProviderHandler<Cx> + Clone + Send + Sync + 'static,
    T::Error: IntoResponse,
    Cx: FromRequestParts<T> + Send,
{
    handler.create_provider(request, context).await.map(Json)
}

pub async fn get_provider<T, Cx>(
    State(handler): State<T>,
    context: Cx,
    request: GetProviderRequest,
) -> Result<Json<Provider>, T::Error>
where
    T: ProviderHandler<Cx> + Clone + Send + Sync + 'static,
    T::Error: IntoResponse,
    Cx: FromRequestParts<T> + Send,
{
    handler.get_provider(request, context).await.map(Json)
}

pub async fn update_provider<T, Cx>(
    State(handler): State<T>,
    context: Cx,
    request: UpdateProviderRequest,
) -> Result<Json<Provider>, T::Error>
where
    T: ProviderHandler<Cx> + Clone + Send + Sync + 'static,
    T::Error: IntoResponse,
    Cx: FromRequestParts<T> + Send,
{
    handler.update_provider(request, context).await.map(Json)
}

pub async fn delete_provider<T, Cx>(
    State(handler): State<T>,
    context: Cx,
    request: DeleteProviderRequest,
) -> Result<(), T::Error>
where
    T: ProviderHandler<Cx> + Clone + Send + Sync + 'static,
    T::Error: IntoResponse,
    Cx: FromRequestParts<T> + Send,
{
    handler.delete_provider(request, context).await
}
