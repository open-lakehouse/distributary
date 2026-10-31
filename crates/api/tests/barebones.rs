//! The barebones contract: a host implements the handler traits directly, with
//! no ports and no table format, and mounts the router. A Delta Sharing server
//! needs only [`SharingHandler`] and [`SharingQueryHandler`].

use async_trait::async_trait;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use bytes::Bytes;
use opensharing_api::{
    Error, Result, SharingHandler, SharingQueryHandler, TableResponse, get_router,
};
use opensharing_common::models::opensharing::v1::*;
use tower::ServiceExt;

/// One share, `letters`, holding nothing: every lookup below it is `NotFound`.
#[derive(Clone)]
struct Stub;

#[async_trait]
impl SharingHandler for Stub {
    async fn list_shares(&self, _: ListSharesRequest, _: ()) -> Result<ListSharesResponse> {
        Ok(ListSharesResponse {
            items: vec![Share {
                name: "letters".to_string(),
                ..Default::default()
            }],
            ..Default::default()
        })
    }

    async fn get_share(&self, _: GetShareRequest, _: ()) -> Result<Share> {
        Err(Error::NotFound)
    }

    async fn list_schemas(&self, _: ListSchemasRequest, _: ()) -> Result<ListSchemasResponse> {
        Err(Error::NotFound)
    }

    async fn list_tables(&self, _: ListTablesRequest, _: ()) -> Result<ListTablesResponse> {
        Err(Error::NotFound)
    }

    async fn list_all_tables(
        &self,
        _: ListAllTablesRequest,
        _: (),
    ) -> Result<ListAllTablesResponse> {
        Err(Error::NotFound)
    }
}

#[async_trait]
impl SharingQueryHandler for Stub {
    async fn get_table_version(
        &self,
        _: GetTableVersionRequest,
        _: (),
    ) -> Result<GetTableVersionResponse> {
        Err(Error::NotFound)
    }

    async fn get_table_metadata(&self, _: GetTableMetadataRequest, _: ()) -> Result<TableResponse> {
        Err(Error::NotFound)
    }

    async fn query_table(&self, _: QueryTableRequest, _: ()) -> Result<TableResponse> {
        Ok(TableResponse {
            version: 7,
            body: Bytes::from_static(b"{\"protocol\":{\"minReaderVersion\":1}}\n"),
        })
    }

    async fn generate_temporary_table_credentials(
        &self,
        _: String,
        _: String,
        _: String,
        _: (),
    ) -> Result<SharingTemporaryCredentials> {
        Err(Error::not_implemented("temporary table credentials"))
    }

    async fn get_table_changes(&self, _: QueryTableRequest, _: ()) -> Result<Bytes> {
        Err(Error::not_implemented("change data feed"))
    }

    async fn poll_query(&self, _: String, _: ()) -> Result<Bytes> {
        Err(Error::not_implemented("asynchronous queries"))
    }
}

async fn send(request: Request<Body>) -> (StatusCode, axum::http::HeaderMap, Bytes) {
    let response = get_router::<Stub, ()>(Stub).oneshot(request).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, headers, body)
}

#[tokio::test]
async fn serves_discovery_from_the_handler() {
    let (status, _, body) = send(Request::get("/shares").body(Body::empty()).unwrap()).await;
    assert_eq!(status, StatusCode::OK);
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["items"][0]["name"], "letters");

    let (status, _, body) = send(Request::get("/shares/other").body(Body::empty()).unwrap()).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["errorCode"], "NOT_FOUND");
}

#[tokio::test]
async fn serves_the_query_path_from_the_handler() {
    let (status, headers, body) = send(
        Request::post("/shares/letters/schemas/s/tables/t/query")
            .header("content-type", "application/json")
            .body(Body::from("{}"))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers["delta-table-version"], "7");
    assert_eq!(&body[..], b"{\"protocol\":{\"minReaderVersion\":1}}\n");
}

#[tokio::test]
async fn unserved_additions_are_not_implemented() {
    let (status, _, _) = send(
        Request::get("/shares/letters/schemas/s/tables/t/changes")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_IMPLEMENTED);
}
