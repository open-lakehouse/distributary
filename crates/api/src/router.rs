//! Axum routers for the Delta Sharing and OpenSharing REST surfaces.
//!
//! [`get_router`] serves the Delta Sharing tabular surface (shares / schemas /
//! tables / version / metadata / query); [`opensharing_router`] adds the Open
//! Sharing asset routes (volumes, agent skills, temporary table credentials).
//! [`sharing_router`] mounts both under their protocol prefixes
//! ([`DELTA_SHARING_PREFIX`], [`OPENSHARING_PREFIX`]), which is what a host
//! normally serves — wrapped in [`require_bearer`](crate::auth::require_bearer).
//!
//! # Response formats
//!
//! Table responses are served in the protocol's `parquet` response format. A
//! client that sends a `delta-sharing-capabilities` header accepting only other
//! formats (`responseformat=delta`) is rejected with `400`.
//!
//! The discovery + asset routes bind to the generated route functions; the
//! version / metadata / query routes bind to the hand-written functions below,
//! whose newline-delimited-JSON response contract the generated, JSON-only route
//! functions do not model.
//!
//! # Spec-gap routes
//!
//! The change-data-feed (`/changes`) and asynchronous-query
//! (`POST /queries/{id}`) routes exist so the surface matches the evolved Delta
//! Sharing protocol, but their handlers return `501 Not Implemented`. The CDF
//! route reuses [`QueryTableRequest`] — whose proto already carries the CDF
//! `starting_version` / `ending_version` fields — rather than a dedicated
//! request type, since no serving logic consumes it yet.

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use axum::routing::{Router, get, post};

use opensharing_common::models::opensharing::v1::*;
use opensharing_common::models::{DeltaSharingCapabilities, ResponseFormat};

use crate::codegen::shares::{self, ShareHandler};
use crate::codegen::skills::{self, SkillHandler};
use crate::codegen::volumes::{self, VolumeHandler};
use crate::error::{Error, Result};
use crate::query::SharingQueryHandler;

/// Response header advertising the Delta Sharing capabilities this server
/// supports. The query path currently emits responses in `parquet` format.
const DELTA_SHARING_CAPABILITIES: &str = "delta-sharing-capabilities";
const DELTA_SHARING_CAPABILITIES_VALUE: &str = "responseformat=parquet";
const CONTENT_TYPE: &str = "content-type";
/// The table version a `version` / `metadata` / `query` response describes.
const DELTA_TABLE_VERSION: &str = "delta-table-version";

/// Where the Delta Sharing surface is mounted by [`sharing_router`].
pub const DELTA_SHARING_PREFIX: &str = "/api/v1/delta-sharing";
/// Where the OpenSharing surface is mounted by [`sharing_router`].
pub const OPENSHARING_PREFIX: &str = "/api/1.0/opensharing";

/// Reject a client whose `delta-sharing-capabilities` rule out the `parquet`
/// response format, the only one served so far.
fn check_response_format(headers: &HeaderMap) -> Result<()> {
    let Some(header) = headers.get(DELTA_SHARING_CAPABILITIES) else {
        return Ok(());
    };
    let header = header
        .to_str()
        .map_err(|_| Error::invalid_argument("delta-sharing-capabilities is not valid text"))?;
    let capabilities = DeltaSharingCapabilities::parse_header(header)?;
    if !capabilities.accepts(&ResponseFormat::Parquet) {
        return Err(Error::invalid_argument(
            "only responseformat=parquet is served; the delta response format is not supported yet",
        ));
    }
    Ok(())
}

/// The tabular Delta Sharing routes (shares / schemas / tables / version /
/// metadata / query).
///
/// Shared verbatim between the Delta Sharing and OpenSharing mounts. The
/// discovery routes bind to the generated [`ShareHandler`] route functions; the
/// three NDJSON query routes bind to the hand-written functions below.
fn tabular_routes<T, Cx>() -> Router<T>
where
    T: ShareHandler<Cx> + SharingQueryHandler<Cx> + Clone + Send + Sync + 'static,
    Cx: axum::extract::FromRequestParts<T> + Send + 'static,
{
    Router::new()
        .route("/shares", get(shares::server::list_shares::<T, Cx>))
        .route("/shares/{name}", get(shares::server::get_share::<T, Cx>))
        .route(
            "/shares/{share}/schemas",
            get(shares::server::list_schemas::<T, Cx>),
        )
        .route(
            "/shares/{share}/all-tables",
            get(shares::server::list_all_tables::<T, Cx>),
        )
        .route(
            "/shares/{share}/schemas/{name}/tables",
            get(shares::server::list_tables::<T, Cx>),
        )
        // Hand-written NDJSON query path (not part of the generated service).
        .route(
            "/shares/{share}/schemas/{schema}/tables/{name}/version",
            get(get_table_version::<T, Cx>),
        )
        .route(
            "/shares/{share}/schemas/{schema}/tables/{name}/metadata",
            get(get_table_metadata::<T, Cx>),
        )
        .route(
            "/shares/{share}/schemas/{schema}/tables/{name}/query",
            post(get_table_query::<T, Cx>),
        )
        // Recently added protocol endpoints: types + routes exist for spec
        // coverage; the handlers return 501 until the serving path is implemented.
        .route(
            "/shares/{share}/schemas/{schema}/tables/{name}/changes",
            get(get_table_changes::<T, Cx>),
        )
        .route("/queries/{query_id}", post(poll_query::<T, Cx>))
}

/// The OpenSharing-only asset routes (volumes, agent skills) bound to the
/// generated per-asset route functions, plus temporary table credentials.
fn asset_routes<T, Cx>() -> Router<T>
where
    T: VolumeHandler<Cx>
        + SkillHandler<Cx>
        + SharingQueryHandler<Cx>
        + Clone
        + Send
        + Sync
        + 'static,
    Cx: axum::extract::FromRequestParts<T> + Send + 'static,
{
    Router::new()
        .route(
            "/shares/{share}/schemas/{schema}/tables/{name}/temporary-table-credentials",
            post(generate_temporary_table_credentials::<T, Cx>),
        )
        .route(
            "/shares/{share}/all-volumes",
            get(volumes::server::list_all_volumes::<T, Cx>),
        )
        .route(
            "/shares/{share}/schemas/{schema}/volumes",
            get(volumes::server::list_volumes::<T, Cx>),
        )
        .route(
            "/shares/{share}/schemas/{schema}/volumes/{name}",
            get(volumes::server::get_volume::<T, Cx>),
        )
        .route(
            "/shares/{share}/schemas/{schema}/volumes/{name}/temporary-volume-credentials",
            post(volumes::server::generate_temporary_volume_credentials::<T, Cx>),
        )
        .route(
            "/shares/{share}/all-skills",
            get(skills::server::list_all_skills::<T, Cx>),
        )
        .route(
            "/shares/{share}/schemas/{schema}/skills",
            get(skills::server::list_skills::<T, Cx>),
        )
        .route(
            "/shares/{share}/schemas/{schema}/skills/{name}",
            get(skills::server::get_skill::<T, Cx>),
        )
        .route(
            "/shares/{share}/schemas/{schema}/skills/{name}/temporary-skill-credentials",
            post(skills::server::generate_temporary_skill_credentials::<T, Cx>),
        )
}

/// Create a [`Router`] for the **Delta Sharing** REST API — the tabular surface
/// only, preserved for wire-compatibility with existing Delta Sharing clients.
pub fn get_router<T, Cx>(state: T) -> Router
where
    T: ShareHandler<Cx> + SharingQueryHandler<Cx> + Clone + Send + Sync + 'static,
    Cx: axum::extract::FromRequestParts<T> + Send + 'static,
{
    tabular_routes::<T, Cx>().with_state(state)
}

/// Create a [`Router`] for the **OpenSharing** REST API: the tabular surface
/// plus the storage-backed asset routes (volumes, agent skills).
pub fn opensharing_router<T, Cx>(state: T) -> Router
where
    T: ShareHandler<Cx>
        + SharingQueryHandler<Cx>
        + VolumeHandler<Cx>
        + SkillHandler<Cx>
        + Clone
        + Send
        + Sync
        + 'static,
    Cx: axum::extract::FromRequestParts<T> + Send + 'static,
{
    tabular_routes::<T, Cx>()
        .merge(asset_routes::<T, Cx>())
        .with_state(state)
}

/// Mount the Delta Sharing surface under [`DELTA_SHARING_PREFIX`] and the Open
/// Sharing surface under [`OPENSHARING_PREFIX`], both served by `state`.
pub fn sharing_router<T, Cx>(state: T) -> Router
where
    T: ShareHandler<Cx>
        + SharingQueryHandler<Cx>
        + VolumeHandler<Cx>
        + SkillHandler<Cx>
        + Clone
        + Send
        + Sync
        + 'static,
    Cx: axum::extract::FromRequestParts<T> + Send + 'static,
{
    Router::new()
        .nest(DELTA_SHARING_PREFIX, get_router::<T, Cx>(state.clone()))
        .nest(OPENSHARING_PREFIX, opensharing_router::<T, Cx>(state))
}

// ---------------------------------------------------------------------------
// Hand-written NDJSON query routes (version / metadata / query) + the 501 gap
// routes (changes / queries poll).
// ---------------------------------------------------------------------------

async fn get_table_version<T, Cx>(
    State(handler): State<T>,
    context: Cx,
    request: GetTableVersionRequest,
) -> Result<Response>
where
    T: SharingQueryHandler<Cx> + Clone + Send + Sync + 'static,
    Cx: axum::extract::FromRequestParts<T> + Send,
{
    let result = handler.get_table_version(request, context).await?;
    Response::builder()
        .header(DELTA_TABLE_VERSION, result.version)
        .body(Body::empty())
        .map_err(|e| Error::generic(e.to_string()))
}

async fn get_table_metadata<T, Cx>(
    State(handler): State<T>,
    headers: HeaderMap,
    context: Cx,
    request: GetTableMetadataRequest,
) -> Result<Response>
where
    T: SharingQueryHandler<Cx> + Clone + Send + Sync + 'static,
    Cx: axum::extract::FromRequestParts<T> + Send,
{
    check_response_format(&headers)?;
    let result = handler.get_table_metadata(request, context).await?;
    Response::builder()
        .header(CONTENT_TYPE, "application/x-ndjson; charset=utf-8")
        .header(DELTA_SHARING_CAPABILITIES, DELTA_SHARING_CAPABILITIES_VALUE)
        .header(DELTA_TABLE_VERSION, result.version)
        .body(Body::from(result.body))
        .map_err(|e| Error::generic(e.to_string()))
}

async fn get_table_query<T, Cx>(
    State(handler): State<T>,
    headers: HeaderMap,
    context: Cx,
    request: QueryTableRequest,
) -> Result<Response>
where
    T: SharingQueryHandler<Cx> + Clone + Send + Sync + 'static,
    Cx: axum::extract::FromRequestParts<T> + Send,
{
    check_response_format(&headers)?;
    let result = handler.query_table(request, context).await?;
    Response::builder()
        .header(CONTENT_TYPE, "application/x-ndjson; charset=utf-8")
        .header(DELTA_SHARING_CAPABILITIES, DELTA_SHARING_CAPABILITIES_VALUE)
        .header(DELTA_TABLE_VERSION, result.version)
        .body(Body::from(result.body))
        .map_err(|e| Error::generic(e.to_string()))
}

/// OpenSharing temporary table credentials. The spec allows an optional request
/// body; nothing in it changes what is vended (always read-only), so it is not read.
async fn generate_temporary_table_credentials<T, Cx>(
    State(handler): State<T>,
    context: Cx,
    Path((share, schema, name)): Path<(String, String, String)>,
) -> Result<axum::Json<TemporaryCredentials>>
where
    T: SharingQueryHandler<Cx> + Clone + Send + Sync + 'static,
    Cx: axum::extract::FromRequestParts<T> + Send,
{
    let credentials = handler
        .generate_temporary_table_credentials(share, schema, name, context)
        .await?;
    Ok(axum::Json(credentials))
}

/// Change data feed. **Not implemented** (501). A GET with `startingVersion` /
/// `endingVersion` query params in the spec; here it only extracts the table path
/// and hands a minimally-populated request to the handler, which returns 501.
async fn get_table_changes<T, Cx>(
    State(handler): State<T>,
    context: Cx,
    Path((share, schema, name)): Path<(String, String, String)>,
) -> Response
where
    T: SharingQueryHandler<Cx> + Clone + Send + Sync + 'static,
    Cx: axum::extract::FromRequestParts<T> + Send,
{
    let request = QueryTableRequest {
        share,
        schema,
        name,
        ..Default::default()
    };
    match handler.get_table_changes(request, context).await {
        Ok(body) => Response::builder()
            .header(CONTENT_TYPE, "application/x-ndjson; charset=utf-8")
            .header(DELTA_SHARING_CAPABILITIES, DELTA_SHARING_CAPABILITIES_VALUE)
            .body(Body::from(body))
            .unwrap_or_else(|e| Error::generic(e.to_string()).into_response()),
        Err(e) => e.into_response(),
    }
}

/// Poll an asynchronous query by id. **Not implemented** (501).
async fn poll_query<T, Cx>(
    State(handler): State<T>,
    context: Cx,
    Path(query_id): Path<String>,
) -> Response
where
    T: SharingQueryHandler<Cx> + Clone + Send + Sync + 'static,
    Cx: axum::extract::FromRequestParts<T> + Send,
{
    match handler.poll_query(query_id, context).await {
        Ok(body) => Response::builder()
            .header(CONTENT_TYPE, "application/x-ndjson; charset=utf-8")
            .body(Body::from(body))
            .unwrap_or_else(|e| Error::generic(e.to_string()).into_response()),
        Err(e) => e.into_response(),
    }
}
