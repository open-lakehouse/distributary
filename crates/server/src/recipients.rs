//! Recipient tokens: issuing, authenticating and activating them.
//!
//! A recipient is handed an *activation link* once (at creation or rotation).
//! Opening it returns the Delta Sharing profile — endpoint plus bearer token —
//! and consumes the link: the clear token, kept sealed only for that moment, is
//! dropped, and from then on the server holds just its sha256 digest.

use std::sync::Arc;

use async_trait::async_trait;
use axum::extract::{FromRequestParts, Path, State};
use axum::http::request::Parts;
use axum::routing::get;
use axum::{Json, Router};
use opensharing_api::auth::{BearerAuthenticator, authenticated};
use serde::Serialize;

use crate::secrets::Secrets;
use crate::store::{RecipientRecord, RecipientStore, TokenReader, TokenRecord, now_millis};
use crate::tokens::random_secret;
pub use crate::tokens::token_hash;
use crate::{Error, Result};
use opensharing_api::{Error as ApiError, Result as ApiResult};

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
            Err(Error::NotFound) => return Err(ApiError::Unauthenticated),
            Err(e) => return Err(e.into()),
        };
        if token.expires_at.is_some_and(|at| at <= now_millis()) {
            return Err(ApiError::Unauthenticated);
        }
        Ok(Recipient {
            id: recipient.id,
            name: recipient.name,
        })
    }
}

/// The AAD a token awaiting activation is sealed under.
fn token_aad(token_id: &str) -> String {
    format!("recipient-token:{token_id}")
}

/// Issue a new token for `recipient`. The returned record carries the activation
/// code; [`activation_url`] turns it into the link handed to the recipient.
pub async fn issue_token(
    store: &dyn RecipientStore,
    secrets: &Secrets,
    recipient: &RecipientRecord,
    expires_at: Option<i64>,
) -> Result<TokenRecord> {
    let id = uuid::Uuid::new_v4().to_string();
    let token = random_secret();
    let activation_code = random_secret();
    let record = TokenRecord {
        id: id.clone(),
        recipient_id: recipient.id.clone(),
        token_hash: token_hash(&token),
        sealed_token: Some(secrets.seal(&token_aad(&id), token.as_bytes()).await?),
        activation_code: Some(activation_code),
        expires_at,
        created_at: now_millis(),
    };
    store.insert_token(record.clone()).await?;
    Ok(record)
}

/// The activation link for a token, until it is used.
pub fn activation_url(public_url: &str, token: &TokenRecord) -> Option<String> {
    token
        .activation_code
        .as_ref()
        .map(|code| format!("{public_url}{ACTIVATION_PATH}/{code}"))
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
pub async fn activate(
    store: &dyn RecipientStore,
    secrets: &Secrets,
    public_url: &str,
    code: &str,
) -> Result<SharingProfile> {
    let token = store.token_by_activation_code(code).await?;
    let sealed = token.sealed_token.as_deref().ok_or(Error::NotFound)?;
    let (clear, _) = secrets.open(&token_aad(&token.id), sealed).await?;
    store.mark_activated(&token.id).await?;
    Ok(SharingProfile {
        share_credentials_version: 1,
        endpoint: format!(
            "{public_url}{}",
            opensharing_api::router::DELTA_SHARING_PREFIX
        ),
        bearer_token: String::from_utf8(clear)
            .map_err(|_| Error::generic("stored token is not valid UTF-8"))?,
        expiration_time: token
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
    let mut rewrapped = 0;
    for token in store.sealed_tokens().await? {
        let Some(sealed) = &token.sealed_token else {
            continue;
        };
        if let Some(sealed) = secrets.rewrap(sealed).await? {
            store.update_sealed_token(&token.id, sealed).await?;
            rewrapped += 1;
        }
    }
    Ok(rewrapped)
}
