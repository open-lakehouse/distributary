//! Recipient tokens: issuing, authenticating and activating them.
//!
//! A recipient is handed an *activation link* once (at creation or rotation).
//! Opening it returns the Delta Sharing profile — endpoint plus bearer token —
//! and consumes the link: the clear token, kept sealed only for that moment, is
//! dropped, and from then on the server holds just its sha256 digest. The
//! activation code is derived from the token and stored only as a digest too.
//!
//! The rules (generating and sealing tokens, expiry, the rotation grace
//! period) live in `unitycatalog_sharing::recipients`; these functions apply
//! them with the server's [`Secrets`] and add the links and profiles of the
//! protocol.

use std::sync::Arc;

use async_trait::async_trait;
use axum::extract::{FromRequestParts, Path, State};
use axum::http::request::Parts;
use axum::routing::get;
use axum::{Json, Router};
use opensharing_api::auth::{BearerAuthenticator, authenticated};
use serde::Serialize;

use crate::RecipientIdentity;
use crate::secrets::Secrets;
use crate::store::{RecipientRecord, RecipientStore, TokenReader, TokenRecord};
use crate::{Error, Result};
use opensharing_api::{Error as ApiError, Result as ApiResult};
use unitycatalog_sharing::recipients as rules;
pub use unitycatalog_sharing::tokens::token_hash;

/// Where activation links are served, under the server's public URL.
pub const ACTIVATION_PATH: &str = "/api/1.0/opensharing/activations";

/// An authenticated sharing recipient: the request context of the protocol
/// routes, put there by [`StoreAuthenticator`] behind
/// [`require_bearer`](opensharing_api::auth::require_bearer).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recipient {
    /// The recipient's stable identifier.
    pub id: String,
    /// The recipient's name.
    pub name: String,
}

impl RecipientIdentity for Recipient {
    fn recipient_id(&self) -> Option<&str> {
        Some(&self.id)
    }
}

impl<S: Send + Sync> FromRequestParts<S> for Recipient {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> ApiResult<Self> {
        authenticated(parts)
    }
}

/// Authenticates recipient bearer tokens against the store.
pub struct StoreAuthenticator {
    store: Arc<dyn TokenReader>,
}

impl StoreAuthenticator {
    pub fn new(store: Arc<dyn TokenReader>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl BearerAuthenticator<Recipient> for StoreAuthenticator {
    async fn authenticate(&self, token: &str) -> ApiResult<Recipient> {
        let (token, recipient) = match self.store.token_by_hash(&token_hash(token)).await {
            Ok(found) => found,
            Err(unitycatalog_sharing::Error::NotFound) => return Err(ApiError::Unauthenticated),
            Err(e) => return Err(Error::from(e).into()),
        };
        if !token.is_live(crate::store::now_millis()) {
            return Err(ApiError::Unauthenticated);
        }
        Ok(Recipient {
            id: recipient.id,
            name: recipient.name,
        })
    }
}

/// A new token for the recipient `recipient_id`, sealed for activation, not
/// yet stored.
pub async fn new_token(
    secrets: &Secrets,
    recipient_id: &str,
    expires_at: Option<i64>,
) -> Result<TokenRecord> {
    Ok(rules::new_token(secrets.encryptor(), recipient_id, expires_at).await?)
}

/// Create the recipient `name` with its first token, whose activation link
/// [`activation_url`] gives.
pub async fn create_recipient(
    store: &dyn RecipientStore,
    secrets: &Secrets,
    name: &str,
    comment: Option<String>,
    expires_at: Option<i64>,
) -> Result<RecipientRecord> {
    Ok(rules::create_recipient(store, secrets.encryptor(), name, comment, expires_at).await?)
}

/// Give `recipient` a fresh token, with the expiry of its newest one, and
/// retire the others.
///
/// They stay valid for `existing_expire_in_seconds` more at most (never longer
/// than they would have); `None` or `0` revokes them at once.
pub async fn rotate_token(
    store: &dyn RecipientStore,
    secrets: &Secrets,
    recipient: &RecipientRecord,
    existing_expire_in_seconds: Option<i64>,
) -> Result<()> {
    Ok(rules::rotate_token(
        store,
        secrets.encryptor(),
        recipient,
        existing_expire_in_seconds,
    )
    .await?)
}

/// The activation link for a token, until it is used.
pub async fn activation_url(
    secrets: &Secrets,
    public_url: &str,
    token: &TokenRecord,
) -> Result<Option<String>> {
    Ok(rules::pending_activation(secrets.encryptor(), token)
        .await?
        .map(|code| format!("{public_url}{ACTIVATION_PATH}/{code}")))
}

/// A Delta Sharing profile (the contents of a `.share` file).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SharingProfile {
    pub share_credentials_version: u32,
    pub endpoint: String,
    pub bearer_token: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expiration_time: Option<String>,
}

/// Consume an activation code, returning the recipient's sharing profile.
///
/// A code is good once, and only while its token is live.
pub async fn activate(
    store: &dyn RecipientStore,
    secrets: &Secrets,
    public_url: &str,
    code: &str,
) -> Result<SharingProfile> {
    let activated = rules::redeem(store, secrets.encryptor(), code).await?;
    Ok(SharingProfile {
        share_credentials_version: 1,
        endpoint: format!(
            "{public_url}{}",
            opensharing_api::router::DELTA_SHARING_PREFIX
        ),
        bearer_token: activated.bearer_token,
        expiration_time: activated
            .token
            .expires_at
            .and_then(chrono::DateTime::from_timestamp_millis)
            .map(|at| at.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)),
    })
}

/// What the activation route needs.
#[derive(Clone)]
struct Activation {
    store: Arc<dyn RecipientStore>,
    secrets: Secrets,
    public_url: String,
}

/// `GET /api/1.0/opensharing/activations/{code}`: consume an activation link and
/// return the recipient's sharing profile. Needs no credential; the code is the
/// secret. `public_url` is the base URL the profile's endpoint points at.
pub fn activation_router(
    store: Arc<dyn RecipientStore>,
    secrets: Secrets,
    public_url: impl Into<String>,
) -> Router {
    Router::new()
        .route(&format!("{ACTIVATION_PATH}/{{code}}"), get(activation))
        .with_state(Activation {
            store,
            secrets,
            public_url: public_url.into(),
        })
}

async fn activation(
    State(state): State<Activation>,
    Path(code): Path<String>,
) -> ApiResult<Json<SharingProfile>> {
    Ok(Json(
        activate(
            state.store.as_ref(),
            &state.secrets,
            &state.public_url,
            &code,
        )
        .await?,
    ))
}

/// Re-wrap every token awaiting activation under the active key.
pub async fn rewrap_tokens(store: &dyn RecipientStore, secrets: &Secrets) -> Result<usize> {
    Ok(rules::rewrap_tokens(store, secrets.encryptor()).await?)
}
