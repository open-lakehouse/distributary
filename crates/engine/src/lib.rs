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
//! - [`SharingPolicy`]: whether the caller may perform an action;
//! - [`TableFormat`]: serving one table format. Delta (via `delta_kernel`, no
//!   query engine) is built in behind the `delta` feature.
//!
//! With the `vending` feature (on by default) it also implements
//! [`StorageAccess`] over source catalogs ([`source`]): delegated to a catalog
//! that vends credentials, or minted locally from credentials the host
//! resolves ([`vending`]).
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
//! - [`policy`]: the [`SharingPolicy`] port and [`AllowAll`](policy::AllowAll).
//! - [`format`](mod@format): the [`TableFormat`] port, the format registry, the Delta format
//!   and predicate-hint translation.
//! - [`service`]: [`SharingService`], the composition root, and the handler
//!   implementations.
//! - `source` (feature `vending`): the `CatalogSource` port shared assets are
//!   resolved through, a static source and the source registry.
//! - `vending` (feature `vending`): `SourceStorageAccess`.

pub mod access;
pub mod catalog;
pub mod format;
mod handler;
#[cfg(feature = "delta")]
mod kernel;
pub mod policy;
pub mod service;
#[cfg(feature = "vending")]
pub mod source;
#[cfg(feature = "testing")]
pub mod testing;
#[cfg(feature = "vending")]
pub mod vending;

pub use access::StorageAccess;
pub use catalog::ShareCatalog;
pub use format::TableFormat;
pub use opensharing_api::{Error, Result};
pub use policy::SharingPolicy;
pub use service::SharingService;
