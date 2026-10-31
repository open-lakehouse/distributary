#![cfg_attr(docsrs, feature(doc_cfg))]
//! An **OpenSharing** / **Delta Sharing** implementation over pluggable ports.
//!
//! `opensharing-api` defines the protocol surface: handler traits and routers.
//! This crate owns the sharing *semantics* behind it: share / schema / asset
//! discovery, the table query path (version / metadata / query) and the
//! storage-backed asset surface (volumes, agent skills). It expresses them over
//! narrow ports a host plugs implementations into:
//!
//! - [`ShareCatalog`]: which shares and objects a recipient sees, and where a
//!   shared object lives (backed by any source catalog);
//! - [`StorageAccess`]: the server's own reads of a shared asset, presigned file
//!   URLs and temporary credentials for recipients;
//! - [`TableFormat`]: serving one table format. Delta (via `delta_kernel`, no
//!   query engine) is built in behind the `delta` feature.
//!
//! [`SharingService`] composes them and implements every `opensharing-api`
//! handler trait, so it mounts straight into
//! [`opensharing_api::sharing_router`]. See `ARCHITECTURE.md` for the overall
//! design.
//!
//! The crate depends on no catalog. An existing catalog becomes a sharing server
//! by implementing the ports over its own discovery, authorization and
//! credential machinery; `opensharing-server` is one such host.
//!
//! # Layout
//! - [`catalog`]: the [`ShareCatalog`] port and the share / asset types.
//! - [`access`]: the [`StorageAccess`] port and [`FileSigner`](access::FileSigner).
//! - [`format`](mod@format): the [`TableFormat`] port, the format registry, the Delta format
//!   and predicate-hint translation.
//! - [`service`]: [`SharingService`], the composition root, and the handler
//!   implementations.

pub mod access;
pub mod catalog;
pub mod format;
mod handler;
#[cfg(feature = "delta")]
mod kernel;
pub mod service;
#[cfg(feature = "testing")]
pub mod testing;

pub use access::StorageAccess;
pub use catalog::ShareCatalog;
pub use format::TableFormat;
pub use opensharing_api::{Error, Result};
pub use service::SharingService;
