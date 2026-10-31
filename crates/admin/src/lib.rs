//! OpenSharing provider admin API models.
//!
//! The control plane of a sharing server: [shares](models::shares) and their
//! permissions, [recipients](models::recipients), and
//! [providers](models::providers). The types are generated from
//! `proto/unitycatalog/{shares,recipients,providers}` and serialize to the same
//! JSON as the Databricks Delta Sharing REST API
//! (`/api/2.1/unity-catalog/{shares,recipients,providers}`).
//!
//! The recipient-facing protocol models live in `opensharing-common`. The
//! two are separate crates so a protocol-only client or server never compiles
//! the admin surface.
//!
//! # Feature flags
//!
//! `axum` compiles the generated request extractors (`FromRequest` /
//! `FromRequestParts`) for a server built on these types.

pub mod models;
