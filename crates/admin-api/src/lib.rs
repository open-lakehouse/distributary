#![cfg_attr(docsrs, feature(doc_cfg))]
//! The OpenSharing provider admin API, server side.
//!
//! The control plane of a sharing server: shares and their permissions,
//! recipients and providers, in the Databricks Delta Sharing REST shape
//! (`/api/2.1/unity-catalog/{shares,recipients,providers}`). The models and
//! their request extractors come from Unity Catalog (`olai-uc-common`, only the
//! three admin resources), so the API stays wire-identical to Unity Catalog's.
//! The models are re-exported as [`shares`], [`recipients`] and [`providers`].
//!
//! The handler traits and the route functions are this crate's. They follow
//! Unity Catalog's generated ones, but a failed call answers with the error
//! type of the host ([`ShareHandler::Error`]): a catalog that mounts the API
//! keeps the error codes, messages and body of its other routes. [`Error`], from
//! `olai-uc-handlers`, is the error for a host without its own.
//!
//! The surface is built like the recipient-facing protocol in `opensharing-api`,
//! but shares nothing with it. Each service has one handler trait, generic over
//! the request context `Cx`, plus one route function per RPC. [`router`]
//! composes the route functions. A host implements the traits it serves and
//! mounts the routers under [`ADMIN_PREFIX`]. Its own `Cx` extractor carries
//! the authenticated caller.
//!
//! Admin callers are not recipients. Recipients authenticate with the bearer
//! tokens a provider hands out, and that lives in `opensharing-api`. Admin
//! callers authenticate however the host operates: provider tokens
//! ([`auth::require_provider`]), OIDC, a session, or a header set by a gateway.
//! Since the routers only require `Cx: FromRequestParts`, any of these slots
//! in as a layer plus an extractor.
//!
//! # Layout
//! - [`shares`], [`recipients`], [`providers`]: the models.
//! - [`handler`]: the handler traits, with the error type of the host.
//! - [`server`] (feature `axum`): the route functions.
//! - [`router`] (feature `axum`): the per-service routers and [`admin_router`].
//! - [`auth`] (feature `axum`): the [`ProviderAuthenticator`](auth::ProviderAuthenticator)
//!   port and its bearer-token layer.
//!
//! # Feature flags
//!
//! `axum` (default) compiles the route functions, routers, auth layer, the
//! models' request extractors and the error `IntoResponse`. Without it only the
//! models, the handler traits and the error type remain.

#[cfg(feature = "axum")]
pub mod auth;
pub mod handler;
#[cfg(feature = "axum")]
pub mod router;
#[cfg(feature = "axum")]
pub mod server;

pub use handler::{ProviderHandler, RecipientHandler, ShareHandler};
#[cfg(feature = "axum")]
pub use router::{ADMIN_PREFIX, admin_router, providers_router, recipients_router, shares_router};
pub use unitycatalog_common::models::{providers, recipients, shares};
pub use unitycatalog_handlers::{Error, RequestContext, Result};
