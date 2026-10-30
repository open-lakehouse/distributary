//! Unity Catalog integration (feature `unity`).
//!
//! The rest of the crate depends on no catalog's models. This module is where
//! Unity Catalog comes in:
//!
//! - [`source`]: a UC REST server as a [`CatalogSource`](crate::source::CatalogSource),
//!   with delegated (catalog-vended) storage access;
//! - [`admin`]: the UC-shaped `credentials` and `external-locations` routes of
//!   the provider admin API (the share and recipient routes are in
//!   [`crate::admin`] and need no feature).
//!
//! A host with its own catalog builds without this feature and implements those
//! parts itself.

pub mod admin;
pub mod source;
