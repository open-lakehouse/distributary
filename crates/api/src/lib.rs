#![cfg_attr(docsrs, feature(doc_cfg))]
//! The **OpenSharing** / **Delta Sharing** REST surface, and nothing more.
//!
//! This crate is the protocol as the spec defines it: one handler trait per
//! service, generic over a request context `Cx`, and the axum routers that bind
//! them. It holds no sharing semantics of its own. A host implements the traits
//! and mounts [`sharing_router`]; that is the most barebones server there is.
//!
//! - The generated discovery and asset traits: [`SharingHandler`] (shares,
//!   schemas, tables), [`SharingVolumeHandler`] and [`SharingSkillHandler`].
//! - [`SharingQueryHandler`], hand-written: the NDJSON `version` / `metadata` /
//!   `query` path, temporary table credentials, change data feed and
//!   asynchronous queries.
//! - [`router`]: the Delta Sharing and OpenSharing mounts.
//! - [`auth`]: bearer-token authentication in front of the routes.
//! - [`error`]: the `{ "errorCode", "message" }` error contract.
//!
//! `opensharing-engine` implements every trait over pluggable ports (share
//! catalog, storage access, table formats) and is the usual way to serve this
//! surface. The wire types come from [`opensharing_common`], shared with the
//! client. The crate depends on no catalog and no table format.

#[cfg(feature = "axum")]
pub mod auth;
pub mod codegen;
pub mod error;
pub mod query;
#[cfg(feature = "axum")]
pub mod router;

/// The default context type for the generated sharing handler traits.
///
/// The traits are generic over a context `Cx`; this crate-neutral alias is the
/// default so the portable crate does not name any server's context type. A
/// server plugs in its own request-context type when it implements the port.
pub type DefaultContext = ();

pub use error::{Error, Result};
pub use query::{SharingApiHandler, SharingQueryHandler, TableResponse, Version};
#[cfg(feature = "axum")]
pub use router::{get_router, opensharing_router, sharing_router};

/// The generated handler traits (`SharingHandler`, `SharingVolumeHandler`,
/// `SharingSkillHandler`), re-exported at the crate root.
pub use codegen::sharing::SharingHandler;
pub use codegen::sharing_skill::SharingSkillHandler;
pub use codegen::sharing_volume::SharingVolumeHandler;

/// The crate's error type, re-exported under a descriptive alias.
pub type SharingApiError = Error;
/// The crate's result type, re-exported under a descriptive alias.
pub type SharingApiResult<T> = Result<T>;
