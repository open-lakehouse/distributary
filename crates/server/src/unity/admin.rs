//! The Unity Catalog-shaped storage routes of the provider admin API:
//! `credentials` and `external-locations`, the stored storage credentials local
//! access vends from (credentials are sealed and never returned).
//!
//! [`routes`] is merged into the [admin router](crate::admin::router), behind
//! the same provider-token check. Unlike the share and recipient routes these
//! are hand-written (the UC credential handler traits are not part of the UC
//! models crate), but they take the same request context `Cx` and check the
//! same [`AdminPolicy`](crate::admin::AdminPolicy), with
//! [`AdminAction::ManageStorage`].

use axum::Json;
use axum::extract::{FromRequestParts, Path, State};
use axum::http::StatusCode;
use axum::routing::get;
use olai_credential_vending::RegisteredCredential;
use olai_storage_credentials::StorageLocation;
use serde_json::json;
use unitycatalog_common::models::credentials::v1::Credential;
use unitycatalog_common::models::external_locations::v1::CreateExternalLocationRequest;

use crate::access::credential_aad;
use crate::admin::{AdminAction, ProviderService, identifier};
use crate::store::{ExternalLocationRecord, SealedCredential};
use crate::{Error, Result};

/// The credential and external-location routes, to merge into
/// [`admin::router`](crate::admin::router).
pub fn routes<Cx>() -> axum::Router<ProviderService<Cx>>
where
    Cx: FromRequestParts<ProviderService<Cx>> + Send + Sync + 'static,
{
    axum::Router::new()
        .route(
            "/credentials",
            get(list_credentials::<Cx>).post(create_credential::<Cx>),
        )
        .route(
            "/credentials/{name}",
            axum::routing::delete(delete_credential::<Cx>),
        )
        .route(
            "/external-locations",
            get(list_external_locations::<Cx>).post(create_external_location::<Cx>),
        )
        .route(
            "/external-locations/{name}",
            axum::routing::delete(delete_external_location::<Cx>),
        )
}

// -- storage credentials ------------------------------------------------------

async fn create_credential<Cx: Send + Sync + 'static>(
    State(state): State<ProviderService<Cx>>,
    context: Cx,
    Json(credential): Json<Credential>,
) -> Result<Json<serde_json::Value>> {
    state
        .authorize(AdminAction::ManageStorage, &context)
        .await?;
    identifier(&credential.name)?;
    // Only the storage credential itself is kept; the catalog metadata is not.
    let storage = RegisteredCredential::try_from(&credential).map_err(|_| {
        Error::invalid_argument(
            "a credential needs one of aws_iam_role, azure_service_principal, \
             azure_managed_identity, azure_storage_key or databricks_gcp_service_account",
        )
    })?;
    let sealed = state
        .secrets
        .seal(
            &credential_aad(&credential.name),
            &serde_json::to_vec(&storage)?,
        )
        .await?;
    state
        .store
        .put_credential(SealedCredential {
            name: credential.name.clone(),
            sealed,
        })
        .await?;
    Ok(Json(json!({ "name": credential.name })))
}

async fn list_credentials<Cx: Send + Sync + 'static>(
    State(state): State<ProviderService<Cx>>,
    context: Cx,
) -> Result<Json<serde_json::Value>> {
    state
        .authorize(AdminAction::ManageStorage, &context)
        .await?;
    let names: Vec<_> = state
        .store
        .list_credentials()
        .await?
        .into_iter()
        .map(|c| json!({ "name": c.name }))
        .collect();
    Ok(Json(json!({ "credentials": names })))
}

async fn delete_credential<Cx: Send + Sync + 'static>(
    State(state): State<ProviderService<Cx>>,
    context: Cx,
    Path(name): Path<String>,
) -> Result<StatusCode> {
    state
        .authorize(AdminAction::ManageStorage, &context)
        .await?;
    state.store.delete_credential(&name).await?;
    Ok(StatusCode::OK)
}

async fn create_external_location<Cx: Send + Sync + 'static>(
    State(state): State<ProviderService<Cx>>,
    context: Cx,
    Json(request): Json<CreateExternalLocationRequest>,
) -> Result<Json<serde_json::Value>> {
    state
        .authorize(AdminAction::ManageStorage, &context)
        .await?;
    identifier(&request.name)?;
    request
        .url
        .parse::<StorageLocation>()
        .map_err(|e| Error::invalid_argument(format!("invalid url: {e}")))?;
    state.store.get_credential(&request.credential_name).await?;
    let location = ExternalLocationRecord {
        name: request.name,
        url: request.url,
        credential_name: request.credential_name,
    };
    state.store.put_external_location(location.clone()).await?;
    Ok(Json(location_json(&location)))
}

fn location_json(location: &ExternalLocationRecord) -> serde_json::Value {
    json!({
        "name": location.name,
        "url": location.url,
        "credential_name": location.credential_name,
    })
}

async fn list_external_locations<Cx: Send + Sync + 'static>(
    State(state): State<ProviderService<Cx>>,
    context: Cx,
) -> Result<Json<serde_json::Value>> {
    state
        .authorize(AdminAction::ManageStorage, &context)
        .await?;
    let locations: Vec<_> = state
        .store
        .list_external_locations()
        .await?
        .iter()
        .map(location_json)
        .collect();
    Ok(Json(json!({ "external_locations": locations })))
}

async fn delete_external_location<Cx: Send + Sync + 'static>(
    State(state): State<ProviderService<Cx>>,
    context: Cx,
    Path(name): Path<String>,
) -> Result<StatusCode> {
    state
        .authorize(AdminAction::ManageStorage, &context)
        .await?;
    state.store.delete_external_location(&name).await?;
    Ok(StatusCode::OK)
}
