//! The service asks the [`SharingPolicy`] before every operation, whatever the
//! catalog says, with the host's own context type.

use std::sync::Arc;

use async_trait::async_trait;
use opensharing_api::codegen::sharing::SharingHandler;
use opensharing_api::{Error, Result, SharingQueryHandler};
use opensharing_common::models::opensharing::v1::*;
use opensharing_engine::SharingService;
use opensharing_engine::policy::{SharingAction, SharingPolicy};
use opensharing_engine::testing::{
    FixtureObject, FixtureShare, InMemoryShareCatalog, LocalStorageAccess,
};

/// The host's request context: the caller's name.
#[derive(Clone)]
struct Caller(&'static str);

/// Only `alice` may read `share1`; nobody may read anything else.
struct OnlyAlice;

#[async_trait]
impl SharingPolicy<Caller> for OnlyAlice {
    async fn authorize(&self, action: SharingAction<'_>, cx: &Caller) -> Result<()> {
        match action {
            SharingAction::ListShares => Ok(()),
            SharingAction::ReadShare { share } if share == "share1" && cx.0 == "alice" => Ok(()),
            SharingAction::ReadShare { .. } => Err(Error::NotFound),
        }
    }
}

fn service() -> SharingService<Caller> {
    let catalog = InMemoryShareCatalog::new(vec![FixtureShare {
        name: "share1".to_string(),
        objects: vec![FixtureObject::table(
            "schema1",
            "table1",
            "s3://bucket/table1",
        )],
        ..Default::default()
    }]);
    SharingService::new(
        Arc::new(catalog),
        Arc::new(LocalStorageAccess),
        Arc::new(OnlyAlice),
    )
}

fn get_share() -> GetShareRequest {
    GetShareRequest {
        name: "share1".to_string(),
        ..Default::default()
    }
}

#[tokio::test]
async fn allows_what_the_policy_allows() {
    let share = service()
        .get_share(get_share(), Caller("alice"))
        .await
        .unwrap();
    assert_eq!(share.name, "share1");
}

#[tokio::test]
async fn refuses_discovery_the_policy_refuses() {
    let err = service()
        .get_share(get_share(), Caller("bob"))
        .await
        .unwrap_err();
    assert!(matches!(err, Error::NotFound), "{err:?}");

    let err = service()
        .list_all_tables(
            ListAllTablesRequest {
                name: "share1".to_string(),
                ..Default::default()
            },
            Caller("bob"),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, Error::NotFound), "{err:?}");
}

#[tokio::test]
async fn refuses_table_access_the_policy_refuses() {
    let err = service()
        .generate_temporary_table_credentials(
            "share1".to_string(),
            "schema1".to_string(),
            "table1".to_string(),
            Caller("bob"),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, Error::NotFound), "{err:?}");

    let err = service()
        .query_table(
            QueryTableRequest {
                share: "share1".to_string(),
                schema: "schema1".to_string(),
                name: "table1".to_string(),
                ..Default::default()
            },
            Caller("bob"),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, Error::NotFound), "{err:?}");
}
