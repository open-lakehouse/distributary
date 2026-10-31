//! The batteries-included OpenSharing / Delta Sharing server.
//!
//! The pieces come from the library crates; this crate assembles them into a
//! deployable server:
//!
//! - `opensharing-api`: the protocol surface (handler traits, routers, bearer
//!   auth);
//! - `opensharing-engine`: the protocol implementation over ports, the Delta
//!   format, catalog sources and credential vending;
//! - `opensharing-provider`: shares, recipients, tokens, grants, sealed
//!   secrets and the admin API service, and the catalog, policy and context
//!   they plug into the protocol.
//!
//! This crate adds:
//!
//! - [`store`]: the Postgres (feature `postgres`) and SQLite (feature
//!   `sqlite`) implementations of the provider's store traits;
//! - [`config`]: layered configuration (file, environment, `DATABASE_URL`);
//! - [`embed`]: the socket-less [`Router`](axum::Router) any host can serve;
//! - `unity` (feature `unity`): the Unity Catalog source and the UC-shaped
//!   storage credential / external location admin routes;
//! - the `sharing-server` binary (feature `bin`).
//!
//! # Without Unity Catalog
//!
//! With `default-features = false` the crate depends on no catalog's models. A
//! host with its own catalog that wants less than the whole server composes the
//! library crates directly instead; see `ARCHITECTURE.md`.

pub mod config;
pub mod embed;
pub mod store;
#[cfg(feature = "unity")]
pub mod unity;

pub use config::Config;
pub use opensharing_provider::{Error, Result};
