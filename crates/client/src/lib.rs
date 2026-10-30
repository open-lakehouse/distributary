//! OpenSharing / Delta Sharing protocol client.
//!
//! This crate provides a [`DeltaSharingClient`](client::DeltaSharingClient) for
//! consuming the Delta Sharing REST API — discovering shares, schemas, and
//! tables, and reading table metadata and data. The request/response [`models`]
//! it sends live in `opensharing-common` (shared with the server-side
//! `opensharing-api` crate) and are re-exported here.

pub use opensharing_common::models;

pub use crate::error::{Error, Result};

pub mod client;
mod codegen;
pub mod error;
mod utils;
