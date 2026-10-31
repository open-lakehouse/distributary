#![cfg_attr(docsrs, feature(doc_cfg))]
//! The OpenSharing provider admin API, server side.
//!
//! The control plane of a sharing server: shares and their permissions,
//! recipients and providers, in the Databricks Delta Sharing REST shape
//! (`/api/2.1/unity-catalog/{shares,recipients,providers}`). The models come
//! from [`opensharing_admin`].
//!
//! The surface is built like the recipient-facing protocol in `opensharing-api`,
//! but shares nothing with it. Each service gets one handler trait, generated
//! from the admin protos and generic over the request context `Cx`, plus one
//! route function per RPC. [`router`] composes the route functions. A host
//! implements the traits it serves and mounts the routers under
//! [`ADMIN_PREFIX`]. Its own `Cx` extractor carries the authenticated caller.
//!
//! Admin callers are not recipients. Recipients authenticate with the bearer
//! tokens a provider hands out, and that lives in `opensharing-api`. Admin
//! callers authenticate however the host operates: provider tokens
//! ([`auth::require_provider`]), OIDC, a session, or a header set by a gateway.
//! Since the routers only require `Cx: FromRequestParts`, any of these slots
//! in as a layer plus an extractor.
//!
//! # Layout
//! - [`router`] (feature `axum`): the per-service routers and [`admin_router`].
//! - [`auth`] (feature `axum`): the [`ProviderAuthenticator`](auth::ProviderAuthenticator)
//!   port and its bearer-token layer.
//! - [`error`]: the admin error contract.
//!
//! # Feature flags
//!
//! `axum` (default) compiles the route functions, routers, auth layer and the
//! error `IntoResponse`. Without it only the handler traits remain.

#[cfg(feature = "axum")]
pub mod auth;
mod codegen;
pub mod error;
#[cfg(feature = "axum")]
pub mod router;

/// The default context type of the generated handler traits.
///
/// The traits are generic over a context `Cx`. This crate-neutral alias is the
/// default, so the crate never names a host's context type.
pub type DefaultContext = ();

pub use codegen::providers::ProviderHandler;
pub use codegen::recipients::RecipientHandler;
pub use codegen::shares::ShareHandler;
pub use error::{Error, Result};
#[cfg(feature = "axum")]
pub use router::{ADMIN_PREFIX, admin_router, providers_router, recipients_router, shares_router};
