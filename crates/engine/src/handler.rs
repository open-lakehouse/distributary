//! The sharing handler surface, implemented by [`SharingService`].
//!
//! The generated discovery / asset handler traits (`SharingHandler`,
//! `SharingVolumeHandler`, `SharingSkillHandler`) and the hand-written
//! [`SharingQueryHandler`] — whose newline-delimited JSON responses the generated,
//! JSON-only handlers do not model — are all implemented here in terms of the
//! service's ports. This is the only place protocol semantics live.

use bytes::Bytes;
use chrono::DateTime;

use opensharing_common::models::opensharing::v1::{
    Schema, Share as SharingShare, SharingSkill, SharingVolume, Table, *,
};

use crate::catalog::{AssetKind, AssetRef, PageRequest, SharedAsset, SharedObject};
use crate::format::{QuerySpec, TableResponse, VersionSpec};
use crate::policy::SharingAction;
use crate::service::{SharingService, paginate};
use crate::{Error, Result};
use opensharing_api::SharingQueryHandler;
use opensharing_api::codegen::sharing::SharingHandler;
use opensharing_api::codegen::sharing_skill::SharingSkillHandler;
use opensharing_api::codegen::sharing_volume::SharingVolumeHandler;

impl<Cx: Clone + Send + Sync + 'static> SharingService<Cx> {
    /// Authorize reading `share`.
    async fn authorize_share(&self, share: &str, cx: &Cx) -> Result<()> {
        self.policy
            .authorize(SharingAction::ReadShare { share }, cx)
            .await
    }

    /// The share's objects of `kind`, optionally restricted to one schema.
    async fn objects(
        &self,
        share: &str,
        kind: AssetKind,
        schema: Option<&str>,
        cx: &Cx,
    ) -> Result<Vec<SharedObject>> {
        Ok(self
            .catalog
            .list_objects(share, cx)
            .await?
            .into_iter()
            .filter(|o| o.kind == kind && schema.is_none_or(|s| o.schema == s))
            .collect())
    }

    /// Authorize and resolve a shared table.
    async fn table(&self, share: &str, schema: &str, name: &str, cx: &Cx) -> Result<SharedAsset> {
        self.authorize_share(share, cx).await?;
        self.catalog
            .resolve(&AssetRef::table(share, schema, name), cx)
            .await
    }

    /// Authorize and resolve a shared storage-backed asset.
    async fn storage_asset(
        &self,
        share: &str,
        schema: &str,
        name: &str,
        kind: AssetKind,
        cx: &Cx,
    ) -> Result<SharedAsset> {
        self.authorize_share(share, cx).await?;
        self.catalog
            .resolve(&AssetRef::new(share, schema, name, kind), cx)
            .await
    }
}

/// Reject reading history the provider does not share.
fn check_history(asset: &SharedAsset, at: VersionSpec) -> Result<()> {
    if at.is_time_travel() && !asset.history_shared {
        return Err(Error::invalid_argument(
            "the provider does not share this table's history; only the latest version can be read",
        ));
    }
    Ok(())
}

/// Parse an ISO-8601 / RFC-3339 timestamp into milliseconds since the epoch.
fn parse_timestamp(field: &str, value: &str) -> Result<i64> {
    DateTime::parse_from_rfc3339(value)
        .map(|ts| ts.timestamp_millis())
        .map_err(|e| Error::invalid_argument(format!("{field}: invalid timestamp {value:?}: {e}")))
}

// ---------------------------------------------------------------------------
// Discovery
// ---------------------------------------------------------------------------

#[async_trait::async_trait]
impl<Cx: Clone + Send + Sync + 'static> SharingHandler<Cx> for SharingService<Cx> {
    async fn list_shares(
        &self,
        request: ListSharesRequest,
        context: Cx,
    ) -> Result<ListSharesResponse> {
        self.policy
            .authorize(SharingAction::ListShares, &context)
            .await?;
        let page = self
            .catalog
            .list_shares(
                PageRequest {
                    max_results: request
                        .max_results
                        .map(|n| usize::try_from(n).unwrap_or_default()),
                    page_token: request.page_token,
                },
                &context,
            )
            .await?;
        Ok(ListSharesResponse {
            items: page.items.into_iter().map(SharingShare::from).collect(),
            next_page_token: page.next_page_token,
            ..Default::default()
        })
    }

    async fn get_share(&self, request: GetShareRequest, context: Cx) -> Result<SharingShare> {
        self.authorize_share(&request.name, &context).await?;
        Ok(self
            .catalog
            .get_share(&request.name, &context)
            .await?
            .into())
    }

    async fn list_schemas(
        &self,
        request: ListSchemasRequest,
        context: Cx,
    ) -> Result<ListSchemasResponse> {
        self.authorize_share(&request.share, &context).await?;
        // Schemas are implied by the share's objects — of every kind, so a schema
        // holding only volumes or skills is listed too.
        let mut seen = std::collections::HashSet::new();
        let schemas: Vec<_> = self
            .catalog
            .list_objects(&request.share, &context)
            .await?
            .into_iter()
            .filter(|o| seen.insert(o.schema.clone()))
            .map(|o| Schema {
                name: o.schema,
                share: request.share.clone(),
                ..Default::default()
            })
            .collect();
        let page = paginate(schemas, request.max_results, request.page_token.as_deref())?;
        Ok(ListSchemasResponse {
            items: page.items,
            next_page_token: page.next_page_token,
            ..Default::default()
        })
    }

    async fn list_tables(
        &self,
        request: ListTablesRequest,
        context: Cx,
    ) -> Result<ListTablesResponse> {
        self.authorize_share(&request.share, &context).await?;
        let share = self.catalog.get_share(&request.share, &context).await?;
        let tables = self
            .objects(
                &request.share,
                AssetKind::Table,
                Some(&request.name),
                &context,
            )
            .await?
            .into_iter()
            .map(|o| table(o, &share.name, share.id.as_deref()))
            .collect();
        let page = paginate(tables, request.max_results, request.page_token.as_deref())?;
        Ok(ListTablesResponse {
            items: page.items,
            next_page_token: page.next_page_token,
            ..Default::default()
        })
    }

    async fn list_all_tables(
        &self,
        request: ListAllTablesRequest,
        context: Cx,
    ) -> Result<ListAllTablesResponse> {
        self.authorize_share(&request.name, &context).await?;
        let share = self.catalog.get_share(&request.name, &context).await?;
        let tables = self
            .objects(&request.name, AssetKind::Table, None, &context)
            .await?
            .into_iter()
            .map(|o| table(o, &share.name, share.id.as_deref()))
            .collect();
        let page = paginate(tables, request.max_results, request.page_token.as_deref())?;
        Ok(ListAllTablesResponse {
            items: page.items,
            next_page_token: page.next_page_token,
            ..Default::default()
        })
    }
}

fn table(object: SharedObject, share: &str, share_id: Option<&str>) -> Table {
    Table {
        name: object.name,
        schema: object.schema,
        share: share.to_string(),
        id: object.id,
        share_id: share_id.map(ToString::to_string),
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// Assets: volumes
// ---------------------------------------------------------------------------

#[async_trait::async_trait]
impl<Cx: Clone + Send + Sync + 'static> SharingVolumeHandler<Cx> for SharingService<Cx> {
    async fn list_volumes(
        &self,
        request: ListVolumesRequest,
        context: Cx,
    ) -> Result<ListVolumesResponse> {
        self.authorize_share(&request.share, &context).await?;
        let share = self.catalog.get_share(&request.share, &context).await?;
        let volumes = self
            .objects(
                &request.share,
                AssetKind::Volume,
                Some(&request.schema),
                &context,
            )
            .await?
            .into_iter()
            .map(|o| volume(o, &share.name, share.id.as_deref()))
            .collect();
        let page = paginate(volumes, request.max_results, request.page_token.as_deref())?;
        Ok(ListVolumesResponse {
            items: page.items,
            next_page_token: page.next_page_token,
            ..Default::default()
        })
    }

    async fn list_all_volumes(
        &self,
        request: ListAllVolumesRequest,
        context: Cx,
    ) -> Result<ListAllVolumesResponse> {
        self.authorize_share(&request.share, &context).await?;
        let share = self.catalog.get_share(&request.share, &context).await?;
        let volumes = self
            .objects(&request.share, AssetKind::Volume, None, &context)
            .await?
            .into_iter()
            .map(|o| volume(o, &share.name, share.id.as_deref()))
            .collect();
        let page = paginate(volumes, request.max_results, request.page_token.as_deref())?;
        Ok(ListAllVolumesResponse {
            items: page.items,
            next_page_token: page.next_page_token,
            ..Default::default()
        })
    }

    async fn get_volume(&self, request: GetVolumeRequest, context: Cx) -> Result<SharingVolume> {
        let asset = self
            .storage_asset(
                &request.share,
                &request.schema,
                &request.name,
                AssetKind::Volume,
                &context,
            )
            .await?;
        Ok(SharingVolume {
            name: request.name,
            schema: request.schema,
            share: request.share,
            id: asset.id,
            storage_location: Some(asset.location.raw),
            ..Default::default()
        })
    }

    async fn generate_temporary_volume_credentials(
        &self,
        request: GenerateTemporaryVolumeCredentialsRequest,
        context: Cx,
    ) -> Result<SharingTemporaryCredentials> {
        let asset = self
            .storage_asset(
                &request.share,
                &request.schema,
                &request.name,
                AssetKind::Volume,
                &context,
            )
            .await?;
        self.access.vend_read_credential(&asset, &context).await
    }
}

fn volume(object: SharedObject, share: &str, share_id: Option<&str>) -> SharingVolume {
    SharingVolume {
        name: object.name,
        schema: object.schema,
        share: share.to_string(),
        id: object.id,
        share_id: share_id.map(ToString::to_string),
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// Assets: agent skills
// ---------------------------------------------------------------------------

#[async_trait::async_trait]
impl<Cx: Clone + Send + Sync + 'static> SharingSkillHandler<Cx> for SharingService<Cx> {
    async fn list_skills(
        &self,
        request: ListSkillsRequest,
        context: Cx,
    ) -> Result<ListSkillsResponse> {
        self.authorize_share(&request.share, &context).await?;
        let share = self.catalog.get_share(&request.share, &context).await?;
        let skills = self
            .objects(
                &request.share,
                AssetKind::AgentSkill,
                Some(&request.schema),
                &context,
            )
            .await?
            .into_iter()
            .map(|o| skill(o, &share.name, share.id.as_deref()))
            .collect();
        let page = paginate(skills, request.max_results, request.page_token.as_deref())?;
        Ok(ListSkillsResponse {
            items: page.items,
            next_page_token: page.next_page_token,
            ..Default::default()
        })
    }

    async fn list_all_skills(
        &self,
        request: ListAllSkillsRequest,
        context: Cx,
    ) -> Result<ListAllSkillsResponse> {
        self.authorize_share(&request.share, &context).await?;
        let share = self.catalog.get_share(&request.share, &context).await?;
        let skills = self
            .objects(&request.share, AssetKind::AgentSkill, None, &context)
            .await?
            .into_iter()
            .map(|o| skill(o, &share.name, share.id.as_deref()))
            .collect();
        let page = paginate(skills, request.max_results, request.page_token.as_deref())?;
        Ok(ListAllSkillsResponse {
            items: page.items,
            next_page_token: page.next_page_token,
            ..Default::default()
        })
    }

    async fn get_skill(&self, request: GetSkillRequest, context: Cx) -> Result<SharingSkill> {
        let asset = self
            .storage_asset(
                &request.share,
                &request.schema,
                &request.name,
                AssetKind::AgentSkill,
                &context,
            )
            .await?;
        Ok(SharingSkill {
            name: request.name,
            schema: request.schema,
            share: request.share,
            id: asset.id,
            storage_location: Some(asset.location.raw),
            ..Default::default()
        })
    }

    async fn generate_temporary_skill_credentials(
        &self,
        request: GenerateTemporarySkillCredentialsRequest,
        context: Cx,
    ) -> Result<SharingTemporaryCredentials> {
        let asset = self
            .storage_asset(
                &request.share,
                &request.schema,
                &request.name,
                AssetKind::AgentSkill,
                &context,
            )
            .await?;
        self.access.vend_read_credential(&asset, &context).await
    }
}

fn skill(object: SharedObject, share: &str, share_id: Option<&str>) -> SharingSkill {
    SharingSkill {
        name: object.name,
        schema: object.schema,
        share: share.to_string(),
        id: object.id,
        share_id: share_id.map(ToString::to_string),
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// The hand-written NDJSON query path (version / metadata / query), plus the
// not-yet-implemented protocol additions (CDF, async queries).
// ---------------------------------------------------------------------------

#[async_trait::async_trait]
impl<Cx: Clone + Send + Sync + 'static> SharingQueryHandler<Cx> for SharingService<Cx> {
    async fn get_table_version(
        &self,
        request: GetTableVersionRequest,
        context: Cx,
    ) -> Result<GetTableVersionResponse> {
        let asset = self
            .table(&request.share, &request.schema, &request.name, &context)
            .await?;
        let starting_timestamp = request
            .starting_timestamp
            .as_deref()
            .map(|ts| parse_timestamp("startingTimestamp", ts))
            .transpose()?;
        if starting_timestamp.is_some() && !asset.history_shared {
            return Err(Error::invalid_argument(
                "the provider does not share this table's history",
            ));
        }
        let storage = self.access.table_storage(&asset, &context).await?;
        let version = self
            .formats
            .get(asset.table_format()?)?
            .version(&asset, &storage, starting_timestamp)
            .await?;
        Ok(GetTableVersionResponse {
            version: version as i64,
            ..Default::default()
        })
    }

    async fn get_table_metadata(
        &self,
        request: GetTableMetadataRequest,
        context: Cx,
    ) -> Result<TableResponse> {
        let asset = self
            .table(&request.share, &request.schema, &request.name, &context)
            .await?;
        let storage = self.access.table_storage(&asset, &context).await?;
        self.formats
            .get(asset.table_format()?)?
            .metadata(&asset, &storage, VersionSpec::Latest)
            .await
    }

    async fn query_table(&self, request: QueryTableRequest, context: Cx) -> Result<TableResponse> {
        let asset = self
            .table(&request.share, &request.schema, &request.name, &context)
            .await?;
        let at = match (request.version, request.timestamp.as_deref()) {
            (Some(_), Some(_)) => {
                return Err(Error::invalid_argument(
                    "version and timestamp are mutually exclusive",
                ));
            }
            (Some(version), None) => VersionSpec::Version(
                u64::try_from(version)
                    .map_err(|_| Error::invalid_argument("version must not be negative"))?,
            ),
            (None, Some(ts)) => VersionSpec::Timestamp(parse_timestamp("timestamp", ts)?),
            (None, None) => VersionSpec::Latest,
        };
        check_history(&asset, at)?;
        let query = QuerySpec {
            at,
            predicate: request.json_predicate_hints.as_option().cloned(),
            limit_hint: request
                .limit_hint
                .and_then(|limit| u64::try_from(limit).ok()),
        };
        let storage = self.access.table_storage(&asset, &context).await?;
        self.formats
            .get(asset.table_format()?)?
            .query(&asset, &storage, query)
            .await
    }

    async fn generate_temporary_table_credentials(
        &self,
        share: String,
        schema: String,
        name: String,
        context: Cx,
    ) -> Result<SharingTemporaryCredentials> {
        let asset = self.table(&share, &schema, &name, &context).await?;
        self.access.vend_read_credential(&asset, &context).await
    }

    async fn get_table_changes(&self, _request: QueryTableRequest, _context: Cx) -> Result<Bytes> {
        Err(Error::not_implemented(
            "Delta Sharing change data feed (/changes) is not implemented",
        ))
    }

    async fn poll_query(&self, _query_id: String, _context: Cx) -> Result<Bytes> {
        Err(Error::not_implemented(
            "Delta Sharing asynchronous queries (POST /queries/{id}) are not implemented",
        ))
    }
}
