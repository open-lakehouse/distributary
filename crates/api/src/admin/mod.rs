//! The provider admin API surface (feature `admin`): shares, share permissions,
//! recipients and providers, in the Databricks Delta Sharing REST shape.
//!
//! Like the protocol, the surface is generated from the admin protos
//! (`proto/opensharing/{shares,recipients,providers}`, models in
//! [`opensharing_admin`]): one handler trait per service, generic over the
//! request context `Cx`, and one route function per RPC. [`router`] composes
//! them. A host implements the traits it serves and mounts the routers under
//! [`ADMIN_PREFIX`], with its own `Cx` extractor carrying
//! the authenticated principal (e.g. [`Provider`](crate::auth::Provider) behind
//! [`require_provider`](crate::auth::require_provider), or a catalog's own
//! request context).
//!
//! `opensharing-server` implements [`ShareHandler`] and [`RecipientHandler`]
//! over its share store. A catalog with its own store can implement them
//! itself instead.

mod codegen;
pub mod router;

pub use codegen::providers::ProviderHandler;
pub use codegen::recipients::RecipientHandler;
pub use codegen::shares::ShareHandler;
pub use router::{ADMIN_PREFIX, admin_router, providers_router, recipients_router, shares_router};
