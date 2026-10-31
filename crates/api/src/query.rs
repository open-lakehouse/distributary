//! The hand-written NDJSON table query surface.
//!
//! `version`, `metadata` and `query` answer with newline-delimited JSON and
//! result-derived headers, which the generated JSON-only handler traits do not
//! model. [`SharingQueryHandler`] is their handler trait; the [`router`](crate::router)
//! binds it next to the generated discovery routes.

use bytes::Bytes;

use opensharing_common::models::opensharing::v1::*;

use crate::codegen::sharing::SharingHandler;
use crate::codegen::sharing_skill::SharingSkillHandler;
use crate::codegen::sharing_volume::SharingVolumeHandler;
use crate::error::Result;

/// A table version.
pub type Version = u64;

/// A newline-delimited JSON table response and the table version it describes
/// (sent as the `Delta-Table-Version` response header).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableResponse {
    pub version: Version,
    pub body: Bytes,
}

/// The hand-written table-query surface: `version` / `metadata` / `query` return
/// newline-delimited JSON (`application/x-ndjson`) with result-derived headers, a
/// streaming contract the generated JSON handlers do not model. It also carries
/// the temporary table credentials endpoint and the more recent protocol
/// additions: change data feed and asynchronous queries.
#[async_trait::async_trait]
pub trait SharingQueryHandler<Cx = crate::DefaultContext>: Send + Sync + 'static {
    /// The current Delta version of a shared table.
    async fn get_table_version(
        &self,
        request: GetTableVersionRequest,
        context: Cx,
    ) -> Result<GetTableVersionResponse>;

    /// The NDJSON protocol + metadata lines for a shared table.
    async fn get_table_metadata(
        &self,
        request: GetTableMetadataRequest,
        context: Cx,
    ) -> Result<TableResponse>;

    /// The NDJSON protocol + metadata + file lines answering a table query.
    async fn query_table(&self, request: QueryTableRequest, context: Cx) -> Result<TableResponse>;

    /// Read-only temporary credentials for a shared table's storage location (the
    /// OpenSharing `temporary-table-credentials` endpoint), so a recipient can
    /// read the table directly instead of through presigned URLs.
    async fn generate_temporary_table_credentials(
        &self,
        share: String,
        schema: String,
        name: String,
        context: Cx,
    ) -> Result<SharingTemporaryCredentials>;

    /// Change data feed for a shared table (`GET …/changes`). A host that does not
    /// serve it returns [`Error::NotImplemented`](crate::Error::NotImplemented) (501).
    async fn get_table_changes(&self, request: QueryTableRequest, context: Cx) -> Result<Bytes>;

    /// Poll an asynchronous query by id (`POST /queries/{id}`). A host that does not
    /// serve it returns [`Error::NotImplemented`](crate::Error::NotImplemented) (501).
    async fn poll_query(&self, query_id: String, context: Cx) -> Result<Bytes>;
}

/// A convenience alias for the full set of handler traits a sharing router needs.
///
/// A host that implements all four serves the whole [`sharing_router`](crate::sharing_router).
/// Named so a host (or the router) can bound on one trait instead of four.
pub trait SharingApiHandler<Cx = crate::DefaultContext>:
    SharingHandler<Cx> + SharingVolumeHandler<Cx> + SharingSkillHandler<Cx> + SharingQueryHandler<Cx>
{
}

impl<T, Cx> SharingApiHandler<Cx> for T where
    T: SharingHandler<Cx>
        + SharingVolumeHandler<Cx>
        + SharingSkillHandler<Cx>
        + SharingQueryHandler<Cx>
{
}
