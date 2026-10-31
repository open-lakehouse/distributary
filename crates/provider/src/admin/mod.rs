//! The provider admin API (feature `admin`), in the Databricks Delta Sharing
//! REST shape (`/api/2.1/unity-catalog/…`), with the models from
//! `opensharing-admin`:
//!
//! - `shares`: create / list / get / update (add, remove or update data objects)
//!   / delete, and `shares/{name}/permissions` (grant `SELECT` to recipients);
//! - `recipients`: create (issuing a token + activation link) / list / get /
//!   update / delete, and `recipients/{name}/rotate-token`.
//!
//! The surface itself — the [`ShareHandler`] and [`RecipientHandler`] traits and
//! their routers — is generated in `opensharing-admin-api`, generic over the
//! request context `Cx`. [`ProviderService`] implements both over the
//! [`SharingStore`] and [`Secrets`]: token issuing, activation links and sealing
//! need more than a plain resource store. Every handler first asks the
//! service's [`AdminPolicy`] whether the caller in `Cx` may perform the
//! [`AdminAction`], the same way [`SharingPolicy`] gates the protocol.
//!
//! The standalone server authenticates providers by bearer token
//! ([`ProviderTokens`] behind [`require_provider`]), with [`Provider`] as the
//! context and [`AllowAll`] as the policy, since the token check already
//! decides. A catalog that mounts these routes into its own server uses its own
//! context and plugs its permission model in as the [`AdminPolicy`].
//!
//! `opensharing-server`'s `unity` feature merges the UC-shaped `credentials`
//! and `external-locations` routes into [`routes`] before [`mount`]ing them.
//!
//! The store keeps shares as catalog-independent
//! [`ShareObject`](crate::store::ShareObject)s; [`shares`] converts them to and
//! from `DataObject`s at the API boundary.
//!
//! [`SharingPolicy`]: opensharing_engine::SharingPolicy
//! [`ShareHandler`]: opensharing_admin_api::ShareHandler
//! [`RecipientHandler`]: opensharing_admin_api::RecipientHandler
//! [`require_provider`]: opensharing_admin_api::auth::require_provider

use std::sync::Arc;

use async_trait::async_trait;
use axum::Router;
use opensharing_admin_api::auth::{Provider, ProviderAuthenticator, require_provider};
use opensharing_admin_api::{ADMIN_PREFIX, admin_router};

use crate::secrets::Secrets;
use crate::store::SharingStore;
use crate::tokens::token_hash;
use crate::{Error, Result};
use opensharing_admin_api::{Error as AdminError, Result as AdminResult};

pub mod recipients;
pub mod shares;

pub use recipients::listed_token;

/// An operation on the admin API, as passed to [`AdminPolicy::authorize`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdminAction<'a> {
    ListShares,
    CreateShare {
        share: &'a str,
    },
    ReadShare {
        share: &'a str,
    },
    UpdateShare {
        share: &'a str,
    },
    DeleteShare {
        share: &'a str,
    },
    /// Read who a share is granted to.
    ReadPermissions {
        share: &'a str,
    },
    /// Grant a share to, or revoke it from, recipients.
    UpdatePermissions {
        share: &'a str,
    },
    ListRecipients,
    CreateRecipient {
        recipient: &'a str,
    },
    ReadRecipient {
        recipient: &'a str,
    },
    UpdateRecipient {
        recipient: &'a str,
    },
    DeleteRecipient {
        recipient: &'a str,
    },
    RotateRecipientToken {
        recipient: &'a str,
    },
    /// Manage stored storage credentials and external locations (feature `unity`).
    ManageStorage,
}

/// Decides whether the caller in `Cx` may perform an [`AdminAction`].
#[async_trait]
pub trait AdminPolicy<Cx>: Send + Sync + 'static {
    /// `Ok(())` to allow; [`Error::NotAllowed`] (or any error) to refuse.
    async fn authorize(&self, action: AdminAction<'_>, context: &Cx) -> Result<()>;
}

/// Allows every action: for when authentication alone decides, as with
/// provider tokens.
#[derive(Debug, Clone, Copy, Default)]
pub struct AllowAll;

#[async_trait]
impl<Cx: Send + Sync + 'static> AdminPolicy<Cx> for AllowAll {
    async fn authorize(&self, _action: AdminAction<'_>, _context: &Cx) -> Result<()> {
        Ok(())
    }
}

/// The admin API over the store. Implements the generated
/// [`ShareHandler`](opensharing_admin_api::ShareHandler) and
/// [`RecipientHandler`](opensharing_admin_api::RecipientHandler). Cheap to clone.
pub struct ProviderService<Cx = Provider> {
    pub(crate) store: Arc<dyn SharingStore>,
    pub(crate) secrets: Secrets,
    pub(crate) public_url: String,
    policy: Arc<dyn AdminPolicy<Cx>>,
}

impl<Cx> Clone for ProviderService<Cx> {
    fn clone(&self) -> Self {
        Self {
            store: self.store.clone(),
            secrets: self.secrets.clone(),
            public_url: self.public_url.clone(),
            policy: self.policy.clone(),
        }
    }
}

impl<Cx: Send + Sync + 'static> ProviderService<Cx> {
    /// A service allowing every authenticated caller ([`AllowAll`]).
    /// `public_url` is the externally reachable base URL activation links point
    /// at, without a trailing slash.
    pub fn new(
        store: Arc<dyn SharingStore>,
        secrets: Secrets,
        public_url: impl Into<String>,
    ) -> Self {
        Self {
            store,
            secrets,
            public_url: public_url.into(),
            policy: Arc::new(AllowAll),
        }
    }

    /// Replace the policy that authorizes each action.
    pub fn with_policy(mut self, policy: Arc<dyn AdminPolicy<Cx>>) -> Self {
        self.policy = policy;
        self
    }

    /// Ask the policy whether the caller in `context` may perform `action`.
    pub async fn authorize(&self, action: AdminAction<'_>, context: &Cx) -> Result<()> {
        self.policy.authorize(action, context).await
    }

    /// The store the service manages.
    pub fn store(&self) -> &Arc<dyn SharingStore> {
        &self.store
    }

    /// The secrets the service seals with.
    pub fn secrets(&self) -> &Secrets {
        &self.secrets
    }
}

/// Accepts a fixed set of provider tokens, compared by digest.
pub struct ProviderTokens {
    digests: Vec<Vec<u8>>,
}

impl ProviderTokens {
    pub fn new<T: AsRef<str>>(tokens: impl IntoIterator<Item = T>) -> Self {
        Self {
            digests: tokens.into_iter().map(|t| token_hash(t.as_ref())).collect(),
        }
    }
}

#[async_trait]
impl ProviderAuthenticator for ProviderTokens {
    async fn authenticate(&self, token: &str) -> AdminResult<Provider> {
        if self.digests.contains(&token_hash(token)) {
            Ok(Provider {
                name: "provider".to_string(),
            })
        } else {
            Err(AdminError::Unauthenticated)
        }
    }
}

/// The share and recipient admin routes, unmounted and unauthenticated. Merge
/// further admin routes into them, then [`mount`] the result.
pub fn routes(service: ProviderService<Provider>) -> Router {
    admin_router::<ProviderService<Provider>, Provider>(service)
}

/// Mount admin `routes` under [`ADMIN_PREFIX`], behind `authenticator`.
pub fn mount(routes: Router, authenticator: Arc<dyn ProviderAuthenticator>) -> Router {
    Router::new().nest(ADMIN_PREFIX, require_provider(routes, authenticator))
}

/// The share and recipient admin API under [`ADMIN_PREFIX`], behind
/// `authenticator`. The activation links of the tokens it issues are served by
/// the protocol router ([`activation_router`](crate::recipients::activation_router)).
pub fn router(
    service: ProviderService<Provider>,
    authenticator: Arc<dyn ProviderAuthenticator>,
) -> Router {
    mount(routes(service), authenticator)
}

/// Check `name` is a plain identifier (non-empty, no `.` or `/`).
pub fn identifier(name: &str) -> Result<()> {
    if name.is_empty() || name.contains(['.', '/']) {
        return Err(Error::invalid_argument(format!(
            "'{name}' is not a valid name (non-empty, without '.' or '/')"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn provider_tokens_accept_only_configured_tokens() {
        let tokens = ProviderTokens::new(["one", "two"]);
        assert!(tokens.authenticate("one").await.is_ok());
        assert!(tokens.authenticate("two").await.is_ok());
        assert!(matches!(
            tokens.authenticate("three").await,
            Err(AdminError::Unauthenticated)
        ));
        assert!(matches!(
            ProviderTokens::new(Vec::<String>::new())
                .authenticate("one")
                .await,
            Err(AdminError::Unauthenticated)
        ));
    }
}
