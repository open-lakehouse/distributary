//! Axum routers for the provider admin API.
//!
//! Each service has its own router, so a host mounts only what it implements:
//! [`shares_router`] (shares and their permissions), [`recipients_router`]
//! (recipients and token rotation) and [`providers_router`]. [`admin_router`]
//! merges the shares and recipients routers, which is what a sharing server
//! serves. Paths are relative; a host nests them under [`ADMIN_PREFIX`].
//!
//! The routes bind to the route functions of [`crate::server`], so the request
//! context `Cx` is extracted per request and handed to the handler, exactly as
//! on the recipient-facing protocol in `opensharing-api`. A failed call
//! answers with the handler's own error type.

use axum::routing::{Router, get, post};

use axum::response::IntoResponse;

use crate::handler::{ProviderHandler, RecipientHandler, ShareHandler};
use crate::server;

/// Where the admin API is conventionally mounted, matching the Databricks /
/// Unity Catalog REST API.
pub const ADMIN_PREFIX: &str = "/api/2.1/unity-catalog";

/// `shares`, `shares/{name}` and `shares/{name}/permissions`.
pub fn shares_router<T, Cx>(state: T) -> Router
where
    T: ShareHandler<Cx> + Clone + Send + Sync + 'static,
    T::Error: IntoResponse,
    Cx: axum::extract::FromRequestParts<T> + Send + 'static,
{
    Router::new()
        .route(
            "/shares",
            get(server::list_shares::<T, Cx>).post(server::create_share::<T, Cx>),
        )
        .route(
            "/shares/{name}",
            get(server::get_share::<T, Cx>)
                .patch(server::update_share::<T, Cx>)
                .delete(server::delete_share::<T, Cx>),
        )
        .route(
            "/shares/{name}/permissions",
            get(server::get_permissions::<T, Cx>).patch(server::update_permissions::<T, Cx>),
        )
        .with_state(state)
}

/// `recipients`, `recipients/{name}` and `recipients/{name}/rotate-token`.
pub fn recipients_router<T, Cx>(state: T) -> Router
where
    T: RecipientHandler<Cx> + Clone + Send + Sync + 'static,
    T::Error: IntoResponse,
    Cx: axum::extract::FromRequestParts<T> + Send + 'static,
{
    Router::new()
        .route(
            "/recipients",
            get(server::list_recipients::<T, Cx>).post(server::create_recipient::<T, Cx>),
        )
        .route(
            "/recipients/{name}",
            get(server::get_recipient::<T, Cx>)
                .patch(server::update_recipient::<T, Cx>)
                .delete(server::delete_recipient::<T, Cx>),
        )
        .route(
            "/recipients/{name}/rotate-token",
            post(server::rotate_recipient_token::<T, Cx>),
        )
        .with_state(state)
}

/// `providers` and `providers/{name}`.
pub fn providers_router<T, Cx>(state: T) -> Router
where
    T: ProviderHandler<Cx> + Clone + Send + Sync + 'static,
    T::Error: IntoResponse,
    Cx: axum::extract::FromRequestParts<T> + Send + 'static,
{
    Router::new()
        .route(
            "/providers",
            get(server::list_providers::<T, Cx>).post(server::create_provider::<T, Cx>),
        )
        .route(
            "/providers/{name}",
            get(server::get_provider::<T, Cx>)
                .patch(server::update_provider::<T, Cx>)
                .delete(server::delete_provider::<T, Cx>),
        )
        .with_state(state)
}

/// The provider side of a sharing server: [`shares_router`] merged with
/// [`recipients_router`].
pub fn admin_router<T, Cx>(state: T) -> Router
where
    T: ShareHandler<Cx> + RecipientHandler<Cx> + Clone + Send + Sync + 'static,
    <T as ShareHandler<Cx>>::Error: IntoResponse,
    <T as RecipientHandler<Cx>>::Error: IntoResponse,
    Cx: axum::extract::FromRequestParts<T> + Send + 'static,
{
    shares_router::<T, Cx>(state.clone()).merge(recipients_router::<T, Cx>(state))
}
