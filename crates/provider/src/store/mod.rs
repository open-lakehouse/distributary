//! The store ports: the sharing server's own state.
//!
//! The store holds what the provider configures: shares (each a list of
//! [`ShareObject`]s naming assets in a source catalog), recipients, their bearer
//! tokens and the share grants. The ports, records and stores come from
//! `olai-uc-sharing` (`unitycatalog_sharing`), re-exported here. Secrets are
//! stored only sealed (see [`crate::secrets`]) and tokens only as digests. The
//! storage credentials local access mints from are not kept here:
//! [`sharing_service`](crate::sharing_service) takes them as a
//! `CredentialResolver`.
//!
//! The state is split by resource into targeted traits:
//!
//! - [`ShareStore`] holds shares and grants. Its read-only view,
//!   [`ShareReader`], is all the protocol needs.
//! - [`RecipientStore`] holds recipients and tokens. Its read-only view,
//!   [`TokenReader`], is all recipient authentication needs.
//! - [`SharingStore`] bundles both.
//!
//! A catalog that keeps shares and recipients in its own store can implement
//! just [`ShareReader`] and [`TokenReader`] and reuse the server's
//! [`StoreCatalog`](crate::catalog::StoreCatalog) and
//! [`StoreAuthenticator`](crate::recipients::StoreAuthenticator).
//!
//! The stores report errors as [`StoreError`] (`unitycatalog_sharing::Error`),
//! which converts into this crate's [`Error`](crate::Error). A host store
//! returns it too.
//!
//! Three implementations:
//!
//! - [`memory::InMemoryStore`], for tests and ephemeral demos;
//! - `PgSharingStore` (`opensharing-postgres`);
//! - `SqliteSharingStore` (`opensharing-sqlite`), for single-node deployments.

pub use unitycatalog_sharing::{Error as StoreError, Result as StoreResult};
pub use unitycatalog_sharing::{
    RecipientRecord, RecipientStore, RecipientUpdate, ShareObject, ShareObjectKind, ShareReader,
    ShareRecord, ShareStore, SharingStore, TokenReader, TokenRecord, now_millis,
};

/// The in-memory store.
pub mod memory {
    /// An in-memory [`SharingStore`](super::SharingStore). Nothing survives a
    /// restart.
    pub use unitycatalog_sharing::InMemorySharingStore as InMemoryStore;
}
