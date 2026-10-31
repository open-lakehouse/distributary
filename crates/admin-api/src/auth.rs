//! Provider authentication for the admin API: the [`ProviderAuthenticator`]
//! port and [`require_provider`], the bearer-token layer that runs it.
//!
//! Provider tokens are one way to authenticate admin callers, and the one the
//! standalone sharing server uses. They are unrelated to the recipient tokens
//! of the protocol (`opensharing-api`'s `auth`). A host with its own
//! authentication (OIDC, sessions, a gateway) skips this module. It puts its
//! own layer in front of the routers and uses its own `Cx` extractor.

use std::sync::Arc;

use async_trait::async_trait;
use axum::extract::{FromRequestParts, Request, State};
use axum::http::header::AUTHORIZATION;
use axum::http::request::Parts;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::error::{Error, Result};

/// An authenticated provider: the operator of a sharing server, allowed on the
/// admin API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Provider {
    /// The provider's name.
    pub name: String,
}

/// Resolves a bearer token to the [`Provider`] it belongs to.
#[async_trait]
pub trait ProviderAuthenticator: Send + Sync + 'static {
    /// The provider `token` belongs to. An unknown token →
    /// [`Error::Unauthenticated`].
    async fn authenticate(&self, token: &str) -> Result<Provider>;
}

/// Require a valid provider bearer token on every route of `router`. The
/// [`Provider`] is stored in the request extensions, where the [`Provider`]
/// extractor picks it up as the handlers' `Cx`.
pub fn require_provider(
    router: axum::Router,
    authenticator: Arc<dyn ProviderAuthenticator>,
) -> axum::Router {
    router.layer(axum::middleware::from_fn_with_state(
        authenticator,
        authenticate,
    ))
}

async fn authenticate(
    State(authenticator): State<Arc<dyn ProviderAuthenticator>>,
    mut request: Request,
    next: Next,
) -> Response {
    let Some(token) = request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(bearer_token)
    else {
        return Error::Unauthenticated.into_response();
    };
    match authenticator.authenticate(token).await {
        Ok(provider) => {
            request.extensions_mut().insert(provider);
            next.run(request).await
        }
        Err(err) => err.into_response(),
    }
}

/// The token of an `Authorization: Bearer <token>` header value.
///
/// Kept apart from the recipient-side parser in `opensharing-api`, so the two
/// authentication paths share no code.
fn bearer_token(value: &str) -> Option<&str> {
    let (scheme, token) = value.split_once(' ')?;
    let token = token.trim();
    (scheme.eq_ignore_ascii_case("bearer") && !token.is_empty()).then_some(token)
}

impl<S: Send + Sync> FromRequestParts<S> for Provider {
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self> {
        parts
            .extensions
            .get::<Provider>()
            .cloned()
            .ok_or(Error::Unauthenticated)
    }
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
