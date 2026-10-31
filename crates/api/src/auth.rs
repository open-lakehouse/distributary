//! Bearer-token authentication in front of the sharing routes.
//!
//! Delta Sharing / OpenSharing recipients authenticate with the bearer token
//! from their sharing profile. That mechanism is the spec's; who the token
//! belongs to is the host's. [`require_bearer`] wraps a router so every request
//! must carry `Authorization: Bearer <token>`. The host's
//! [`BearerAuthenticator`] resolves the token to the request context `Cx` of the
//! sharing handlers, which goes into the request extensions. Requests without a
//! valid token get `401`.
//!
//! The handlers extract `Cx` per request, so `Cx` implements
//! [`FromRequestParts`]. [`authenticated`] is the whole of that impl:
//!
//! ```
//! use axum::extract::FromRequestParts;
//! use axum::http::request::Parts;
//! use opensharing_api::auth::authenticated;
//!
//! #[derive(Clone)]
//! struct Caller {
//!     id: String,
//! }
//!
//! impl<S: Send + Sync> FromRequestParts<S> for Caller {
//!     type Rejection = opensharing_api::Error;
//!
//!     async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Self::Rejection> {
//!         authenticated(parts)
//!     }
//! }
//! ```
//!
//! Recipient tokens are what a provider hands to third parties. The provider
//! admin API authenticates its callers separately (`opensharing-admin-api`).

use std::sync::Arc;

use async_trait::async_trait;
use axum::extract::{Request, State};
use axum::http::header::AUTHORIZATION;
use axum::http::request::Parts;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

#[cfg(doc)]
use axum::extract::FromRequestParts;

use crate::error::{Error, Result};

/// Resolves a bearer token to the request context `Cx` of the caller it
/// belongs to.
#[async_trait]
pub trait BearerAuthenticator<Cx>: Send + Sync + 'static {
    /// The context of the caller `token` was issued to. An unknown, expired or
    /// revoked token → [`Error::Unauthenticated`].
    async fn authenticate(&self, token: &str) -> Result<Cx>;
}

/// Require a valid bearer token on every route of `router`, and put the
/// caller's context in the request extensions.
pub fn require_bearer<Cx>(
    router: axum::Router,
    authenticator: Arc<dyn BearerAuthenticator<Cx>>,
) -> axum::Router
where
    Cx: Clone + Send + Sync + 'static,
{
    router.layer(axum::middleware::from_fn_with_state(
        authenticator,
        authenticate::<Cx>,
    ))
}

/// The context [`require_bearer`] stored for this request. Missing →
/// [`Error::Unauthenticated`].
pub fn authenticated<Cx: Clone + Send + Sync + 'static>(parts: &Parts) -> Result<Cx> {
    parts
        .extensions
        .get::<Cx>()
        .cloned()
        .ok_or(Error::Unauthenticated)
}

async fn authenticate<Cx: Clone + Send + Sync + 'static>(
    State(authenticator): State<Arc<dyn BearerAuthenticator<Cx>>>,
    mut request: Request,
    next: Next,
) -> Response {
    let Some(token) = request_token(&request) else {
        return Error::Unauthenticated.into_response();
    };
    match authenticator.authenticate(token).await {
        Ok(cx) => {
            request.extensions_mut().insert(cx);
            next.run(request).await
        }
        Err(err) => err.into_response(),
    }
}

/// The bearer token of a request, if it carries one.
fn request_token(request: &Request) -> Option<&str> {
    request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(bearer_token)
}

/// The token of an `Authorization: Bearer <token>` header value.
fn bearer_token(value: &str) -> Option<&str> {
    let (scheme, token) = value.split_once(' ')?;
    let token = token.trim();
    (scheme.eq_ignore_ascii_case("bearer") && !token.is_empty()).then_some(token)
}

#[cfg(test)]
mod tests {
    use super::bearer_token;

    #[test]
    fn parses_bearer_tokens() {
        assert_eq!(bearer_token("Bearer abc"), Some("abc"));
        assert_eq!(bearer_token("bearer  abc "), Some("abc"));
        assert_eq!(bearer_token("Basic abc"), None);
        assert_eq!(bearer_token("Bearer "), None);
        assert_eq!(bearer_token("Bearer"), None);
    }
}
