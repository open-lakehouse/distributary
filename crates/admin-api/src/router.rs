//! Axum routers for the provider admin API.
//!
//! Each service has its own router, so a host mounts only what it implements:
//! [`shares_router`] (shares and their permissions), [`recipients_router`]
//! (recipients and token rotation) and [`providers_router`]. [`admin_router`]
//! merges the shares and recipients routers, which is what a sharing server
//! serves. Paths are relative; a host nests them under [`ADMIN_PREFIX`].
//!
//! The routes bind to the generated route functions, so the request context
//! `Cx` is extracted per request and handed to the handler, exactly as on the
//! recipient-facing protocol in `opensharing-api`.

use axum::routing::{Router, get, post};

use crate::codegen::providers::{self, ProviderHandler};
use crate::codegen::recipients::{self, RecipientHandler};
use crate::codegen::shares::{self, ShareHandler};

/// Where the admin API is conventionally mounted, matching the Databricks /
/// Unity Catalog REST API.
pub const ADMIN_PREFIX: &str = "/api/2.1/unity-catalog";

/// `shares`, `shares/{name}` and `shares/{name}/permissions`.
pub fn shares_router<T, Cx>(state: T) -> Router
where
    T: ShareHandler<Cx> + Clone + Send + Sync + 'static,
    Cx: axum::extract::FromRequestParts<T> + Send + 'static,
{
    Router::new()
        .route(
            "/shares",
            get(shares::server::list_shares::<T, Cx>).post(shares::server::create_share::<T, Cx>),
        )
        .route(
            "/shares/{name}",
            get(shares::server::get_share::<T, Cx>)
                .patch(shares::server::update_share::<T, Cx>)
                .delete(shares::server::delete_share::<T, Cx>),
        )
        .route(
            "/shares/{name}/permissions",
            get(shares::server::get_permissions::<T, Cx>)
                .patch(shares::server::update_permissions::<T, Cx>),
        )
        .with_state(state)
}

/// `recipients`, `recipients/{name}` and `recipients/{name}/rotate-token`.
pub fn recipients_router<T, Cx>(state: T) -> Router
where
    T: RecipientHandler<Cx> + Clone + Send + Sync + 'static,
    Cx: axum::extract::FromRequestParts<T> + Send + 'static,
{
    Router::new()
        .route(
            "/recipients",
            get(recipients::server::list_recipients::<T, Cx>)
                .post(recipients::server::create_recipient::<T, Cx>),
        )
        .route(
            "/recipients/{name}",
            get(recipients::server::get_recipient::<T, Cx>)
                .patch(recipients::server::update_recipient::<T, Cx>)
                .delete(recipients::server::delete_recipient::<T, Cx>),
        )
        .route(
            "/recipients/{name}/rotate-token",
            post(recipients::server::rotate_recipient_token::<T, Cx>),
        )
        .with_state(state)
}

/// `providers` and `providers/{name}`.
pub fn providers_router<T, Cx>(state: T) -> Router
where
    T: ProviderHandler<Cx> + Clone + Send + Sync + 'static,
    Cx: axum::extract::FromRequestParts<T> + Send + 'static,
{
    Router::new()
        .route(
            "/providers",
            get(providers::server::list_providers::<T, Cx>)
                .post(providers::server::create_provider::<T, Cx>),
        )
        .route(
            "/providers/{name}",
            get(providers::server::get_provider::<T, Cx>)
                .patch(providers::server::update_provider::<T, Cx>)
                .delete(providers::server::delete_provider::<T, Cx>),
        )
        .with_state(state)
}

/// The provider side of a sharing server: [`shares_router`] merged with
/// [`recipients_router`].
pub fn admin_router<T, Cx>(state: T) -> Router
where
    T: ShareHandler<Cx> + RecipientHandler<Cx> + Clone + Send + Sync + 'static,
    Cx: axum::extract::FromRequestParts<T> + Send + 'static,
{
    shares_router::<T, Cx>(state.clone()).merge(recipients_router::<T, Cx>(state))
}
