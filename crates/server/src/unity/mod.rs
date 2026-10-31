//! Unity Catalog integration (feature `unity`).
//!
//! The rest of the crate depends on no catalog's models. This module is where
//! Unity Catalog comes in:
//!
//! - [`source`]: a UC REST server as a [`CatalogSource`](opensharing_engine::source::CatalogSource),
//!   with delegated (catalog-vended) storage access;
//! - `admin` (with the `admin` feature): the UC-shaped `credentials` and
//!   `external-locations` routes of the provider admin API (the share and
//!   recipient routes are in `opensharing_provider::admin`).
//!
//! A host with its own catalog builds without this feature and implements those
//! parts itself.

#[cfg(feature = "admin")]
pub mod admin;
pub mod source;
