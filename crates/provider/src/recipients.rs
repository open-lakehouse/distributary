//! Recipient tokens: issuing, authenticating and activating them.
//!
//! A recipient is handed an *activation link* once (at creation or rotation).
//! Opening it returns the Delta Sharing profile — endpoint plus bearer token —
//! and consumes the link: the clear token, kept sealed only for that moment, is
//! dropped, and from then on the server holds just its sha256 digest. The
//! activation code is derived from the token and stored only as a digest too.
//!
//! These functions hold the rules (generating and sealing tokens, expiry, the
//! rotation grace period); each store write they make is one atomic
//! [`RecipientStore`] operation.

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
use crate::store::{RecipientRecord, RecipientStore, TokenReader, TokenRecord, now_millis};
pub use crate::tokens::token_hash;
use crate::tokens::{activation_code, random_secret};
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
            Err(Error::NotFound) => return Err(ApiError::Unauthenticated),
            Err(e) => return Err(e.into()),
        };
        if !token.is_live(now_millis()) {
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

/// A new token for the recipient `recipient_id`, sealed for activation, not
/// yet stored. Ids are time-ordered, so tokens created within the same
/// millisecond still sort by creation.
pub async fn new_token(
    secrets: &Secrets,
    recipient_id: &str,
    expires_at: Option<i64>,
) -> Result<TokenRecord> {
    let id = uuid::Uuid::now_v7().to_string();
    let token = random_secret();
    Ok(TokenRecord {
        id: id.clone(),
        recipient_id: recipient_id.to_string(),
        token_hash: token_hash(&token),
        sealed_token: Some(secrets.seal(&token_aad(&id), token.as_bytes()).await?),
        activation_code_hash: Some(token_hash(&activation_code(&token))),
        expires_at,
        created_at: now_millis(),
    })
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
    let recipient = RecipientRecord {
        id: uuid::Uuid::now_v7().to_string(),
        name: name.to_string(),
        comment,
        created_at: now_millis(),
    };
    let token = new_token(secrets, &recipient.id, expires_at).await?;
    store.create_recipient(recipient.clone(), token).await?;
    Ok(recipient)
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
    let grace = match existing_expire_in_seconds {
        Some(seconds) if seconds < 0 => {
            return Err(Error::invalid_argument(
                "existing_token_expire_in_seconds must not be negative",
            ));
        }
        Some(seconds) if seconds > 0 => Some(seconds.saturating_mul(1000)),
        _ => None,
    };
    let expires_at = store
        .tokens_for_recipient(&recipient.id)
        .await?
        .first()
        .and_then(|t| t.expires_at);
    let token = new_token(secrets, &recipient.id, expires_at).await?;
    let existing_expire_at = grace.map(|grace| token.created_at.saturating_add(grace));
    store.rotate_token(token, existing_expire_at).await
}

/// The activation link for a token, until it is used.
pub async fn activation_url(
    secrets: &Secrets,
    public_url: &str,
    token: &TokenRecord,
) -> Result<Option<String>> {
    let Some(sealed) = &token.sealed_token else {
        return Ok(None);
    };
    let clear = open_token(secrets, token, sealed).await?;
    Ok(Some(format!(
        "{public_url}{ACTIVATION_PATH}/{}",
        activation_code(&clear)
    )))
}

async fn open_token(secrets: &Secrets, token: &TokenRecord, sealed: &[u8]) -> Result<String> {
    let (clear, _) = secrets.open(&token_aad(&token.id), sealed).await?;
    String::from_utf8(clear).map_err(|_| Error::generic("stored token is not valid UTF-8"))
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
    let token = store
        .redeem_activation(&token_hash(code), now_millis())
        .await?;
    let sealed = token.sealed_token.as_deref().ok_or(Error::NotFound)?;
    Ok(SharingProfile {
        share_credentials_version: 1,
        endpoint: format!(
            "{public_url}{}",
            opensharing_api::router::DELTA_SHARING_PREFIX
        ),
        bearer_token: open_token(secrets, &token, sealed).await?,
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
        let Some(current) = &token.sealed_token else {
            continue;
        };
        if let Some(sealed) = secrets.rewrap(current).await?
            && store.reseal_token(&token.id, current, sealed).await?
        {
            rewrapped += 1;
        }
    }
    Ok(rewrapped)
}
