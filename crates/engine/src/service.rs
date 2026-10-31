//! [`SharingService`]: the composition root of the sharing surface.
//!
//! A service is assembled from one implementation of each port — a
//! [`ShareCatalog`], a [`StorageAccess`], a [`SharingPolicy`] and a
//! [`FormatRegistry`] — and
//! implements every `opensharing-api` handler trait on top of them. Hosts build
//! one, hand it to a router, and never re-implement any protocol semantics.

use std::sync::Arc;

use crate::access::StorageAccess;
use crate::catalog::{Page, ShareCatalog};
use crate::format::FormatRegistry;
use crate::policy::SharingPolicy;
use crate::{Error, Result};

/// The sharing service. Cheap to clone (all ports are behind [`Arc`]s).
pub struct SharingService<Cx = opensharing_api::DefaultContext> {
    pub(crate) catalog: Arc<dyn ShareCatalog<Cx>>,
    pub(crate) access: Arc<dyn StorageAccess<Cx>>,
    pub(crate) policy: Arc<dyn SharingPolicy<Cx>>,
    pub(crate) formats: Arc<FormatRegistry>,
}

impl<Cx> Clone for SharingService<Cx> {
    fn clone(&self) -> Self {
        Self {
            catalog: self.catalog.clone(),
            access: self.access.clone(),
            policy: self.policy.clone(),
            formats: self.formats.clone(),
        }
    }
}

impl<Cx: Send + Sync + 'static> SharingService<Cx> {
    /// A service over `catalog` and `access`, authorized by `policy`, serving
    /// the [built-in](FormatRegistry::builtin) table formats.
    ///
    /// There is no default policy, so a service never allows everything by
    /// accident. Pass [`AllowAll`](crate::policy::AllowAll) to do so on purpose.
    pub fn new(
        catalog: Arc<dyn ShareCatalog<Cx>>,
        access: Arc<dyn StorageAccess<Cx>>,
        policy: Arc<dyn SharingPolicy<Cx>>,
    ) -> Self {
        Self {
            catalog,
            access,
            policy,
            formats: Arc::new(FormatRegistry::builtin()),
        }
    }

    /// Replace the table formats this service serves.
    pub fn with_formats(mut self, formats: FormatRegistry) -> Self {
        self.formats = Arc::new(formats);
        self
    }

    /// The share catalog.
    pub fn catalog(&self) -> &Arc<dyn ShareCatalog<Cx>> {
        &self.catalog
    }

    /// The storage access.
    pub fn access(&self) -> &Arc<dyn StorageAccess<Cx>> {
        &self.access
    }

    /// The sharing policy.
    pub fn policy(&self) -> &Arc<dyn SharingPolicy<Cx>> {
        &self.policy
    }
}

/// Page over an in-memory, stably-ordered list.
///
/// Page tokens are opaque to clients; here they encode the offset of the next
/// item. Without `max_results` the remainder is returned in one page.
pub fn paginate<T>(
    items: Vec<T>,
    max_results: Option<i32>,
    page_token: Option<&str>,
) -> Result<Page<T>> {
    let offset = match page_token.filter(|t| !t.is_empty()) {
        None => 0,
        Some(token) => token
            .parse::<usize>()
            .map_err(|_| Error::invalid_argument("invalid page token"))?,
    };
    let limit = match max_results {
        None => usize::MAX,
        Some(n) if n > 0 => n as usize,
        Some(_) => return Err(Error::invalid_argument("maxResults must be positive")),
    };
    let total = items.len();
    let items: Vec<T> = items.into_iter().skip(offset).take(limit).collect();
    let next = offset.saturating_add(items.len());
    Ok(Page {
        next_page_token: (next < total).then(|| next.to_string()),
        items,
    })
}

#[cfg(test)]
mod tests {
    use super::paginate;

    #[test]
    fn paginates_with_offset_tokens() {
        let first = paginate((0..5).collect(), Some(2), None).unwrap();
        assert_eq!(first.items, vec![0, 1]);
        assert_eq!(first.next_page_token.as_deref(), Some("2"));

        let last = paginate((0..5).collect(), Some(3), Some("2")).unwrap();
        assert_eq!(last.items, vec![2, 3, 4]);
        assert_eq!(last.next_page_token, None);

        let all = paginate((0..5).collect::<Vec<_>>(), None, None).unwrap();
        assert_eq!(all.items.len(), 5);
        assert_eq!(all.next_page_token, None);
    }

    #[test]
    fn rejects_bad_tokens_and_sizes() {
        assert!(paginate(vec![1], None, Some("x")).is_err());
        assert!(paginate(vec![1], Some(0), None).is_err());
    }
}
