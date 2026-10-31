//! The [`SharingPolicy`] port: who may do what on the recipient surface.
//!
//! The service asks the policy before every recipient-facing operation, with a
//! [`SharingAction`] scoped to the share. The policy is separate from the
//! [`ShareCatalog`](crate::ShareCatalog), which only says what a share contains
//! and where its objects live. A host can then pair its own catalog with
//! another crate's grant model, or the other way round.
//!
//! A [`ShareCatalog::list_shares`](crate::ShareCatalog::list_shares) still
//! returns only the shares the caller sees, because a listing is paged where it
//! is filtered. The policy decides whether the caller may list at all.

use async_trait::async_trait;

use crate::Result;

/// The action the service authorizes before an operation. Scoped to the share;
/// the concrete object is checked when it is resolved.
#[derive(Debug, Clone)]
pub enum SharingAction<'a> {
    /// List shares accessible to the caller.
    ListShares,
    /// Read a specific share (discovery, listing, querying its objects).
    ReadShare { share: &'a str },
}

/// Decides whether the caller in `Cx` may perform a [`SharingAction`].
///
/// Refuse a share the caller may not read with
/// [`Error::NotFound`](crate::Error::NotFound), so the response does not reveal
/// whether it exists.
#[async_trait]
pub trait SharingPolicy<Cx = opensharing_api::DefaultContext>: Send + Sync + 'static {
    /// `Ok(())` to allow; an error to refuse.
    async fn authorize(&self, action: SharingAction<'_>, cx: &Cx) -> Result<()>;
}

/// Allows every action: for tests, demos, and hosts whose authentication alone
/// decides access.
#[derive(Debug, Clone, Copy, Default)]
pub struct AllowAll;

#[async_trait]
impl<Cx: Send + Sync + 'static> SharingPolicy<Cx> for AllowAll {
    async fn authorize(&self, _action: SharingAction<'_>, _cx: &Cx) -> Result<()> {
        Ok(())
    }
}
