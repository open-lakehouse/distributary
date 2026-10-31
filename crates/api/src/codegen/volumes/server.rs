// @generated — do not edit by hand.
#![allow(unused_mut, clippy::too_many_arguments)]
use super::handler::VolumeHandler;
use crate::Result;
use axum::extract::State;
use opensharing_common::models::opensharing::v1::*;
pub async fn list_volumes<T, Cx>(
    State(handler): State<T>,
    context: Cx,
    request: ListVolumesRequest,
) -> Result<::axum::Json<ListVolumesResponse>>
where
    T: VolumeHandler<Cx> + Clone + Send + Sync + 'static,
    Cx: axum::extract::FromRequestParts<T> + Send,
{
    let result = handler.list_volumes(request, context).await?;
    Ok(axum::Json(result))
}
pub async fn list_all_volumes<T, Cx>(
    State(handler): State<T>,
    context: Cx,
    request: ListAllVolumesRequest,
) -> Result<::axum::Json<ListAllVolumesResponse>>
where
    T: VolumeHandler<Cx> + Clone + Send + Sync + 'static,
    Cx: axum::extract::FromRequestParts<T> + Send,
{
    let result = handler.list_all_volumes(request, context).await?;
    Ok(axum::Json(result))
}
pub async fn get_volume<T, Cx>(
    State(handler): State<T>,
    context: Cx,
    request: GetVolumeRequest,
) -> Result<::axum::Json<Volume>>
where
    T: VolumeHandler<Cx> + Clone + Send + Sync + 'static,
    Cx: axum::extract::FromRequestParts<T> + Send,
{
    let result = handler.get_volume(request, context).await?;
    Ok(axum::Json(result))
}
pub async fn generate_temporary_volume_credentials<T, Cx>(
    State(handler): State<T>,
    context: Cx,
    request: GenerateTemporaryVolumeCredentialsRequest,
) -> Result<::axum::Json<TemporaryCredentials>>
where
    T: VolumeHandler<Cx> + Clone + Send + Sync + 'static,
    Cx: axum::extract::FromRequestParts<T> + Send,
{
    let result = handler
        .generate_temporary_volume_credentials(request, context)
        .await?;
    Ok(axum::Json(result))
}
