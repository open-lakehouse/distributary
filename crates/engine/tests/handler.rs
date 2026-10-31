//! Handler integration tests: drive the discovery + asset surface through the
//! generated handler traits over the in-memory backend.

use opensharing_api::codegen::shares::ShareHandler;
use opensharing_api::codegen::skills::SkillHandler;
use opensharing_api::codegen::volumes::VolumeHandler;
use opensharing_api::{Error, SharingQueryHandler};
use opensharing_common::models::opensharing::v1::*;
use opensharing_engine::SharingService;
use opensharing_engine::catalog::AssetKind;
use opensharing_engine::testing::{FixtureObject, FixtureShare, in_memory_service};

fn backend() -> SharingService<()> {
    in_memory_service(vec![FixtureShare {
        name: "share1".to_string(),
        id: Some("share1-id".to_string()),
        comment: Some("a shared dataset".to_string()),
        objects: vec![
            FixtureObject::table("schema1", "table1", "s3://bucket/table1"),
            FixtureObject::new(AssetKind::Volume, "schema1", "vol1", "s3://bucket/vol1"),
            FixtureObject::new(
                AssetKind::AgentSkill,
                "schema1",
                "skill1",
                "s3://bucket/skill1",
            ),
            // A schema that only holds a volume must still be listed.
            FixtureObject::new(AssetKind::Volume, "schema2", "vol2", "s3://bucket/vol2"),
        ],
    }])
}

#[tokio::test]
async fn lists_shares() {
    let b = backend();
    let resp = ShareHandler::<()>::list_shares(&b, ListSharesRequest::default(), ())
        .await
        .unwrap();
    assert_eq!(resp.items.len(), 1);
    assert_eq!(resp.items[0].name, "share1");
    assert_eq!(resp.items[0].comment.as_deref(), Some("a shared dataset"));
}

#[tokio::test]
async fn gets_share() {
    let b = backend();
    let share = ShareHandler::<()>::get_share(
        &b,
        GetShareRequest {
            name: "share1".to_string(),
            ..Default::default()
        },
        (),
    )
    .await
    .unwrap();
    assert_eq!(share.name, "share1");
    assert_eq!(share.id.as_deref(), Some("share1-id"));
}

#[tokio::test]
async fn lists_schemas_and_tables() {
    let b = backend();
    let schemas = ShareHandler::<()>::list_schemas(
        &b,
        ListSchemasRequest {
            share: "share1".to_string(),
            ..Default::default()
        },
        (),
    )
    .await
    .unwrap();
    let names: Vec<_> = schemas.items.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, vec!["schema1", "schema2"]);

    let tables = ShareHandler::<()>::list_tables(
        &b,
        ListTablesRequest {
            share: "share1".to_string(),
            name: "schema1".to_string(),
            ..Default::default()
        },
        (),
    )
    .await
    .unwrap();
    assert_eq!(tables.items.len(), 1);
    assert_eq!(tables.items[0].name, "table1");
    assert_eq!(tables.items[0].schema, "schema1");
    assert_eq!(tables.items[0].share, "share1");

    let all = ShareHandler::<()>::list_all_tables(
        &b,
        ListAllTablesRequest {
            name: "share1".to_string(),
            ..Default::default()
        },
        (),
    )
    .await
    .unwrap();
    assert_eq!(all.items.len(), 1);
    assert_eq!(all.items[0].name, "table1");
}

#[tokio::test]
async fn lists_and_vends_volumes() {
    let b = backend();
    let vols = VolumeHandler::<()>::list_volumes(
        &b,
        ListVolumesRequest {
            share: "share1".to_string(),
            schema: "schema1".to_string(),
            ..Default::default()
        },
        (),
    )
    .await
    .unwrap();
    assert_eq!(vols.items.len(), 1);
    assert_eq!(vols.items[0].name, "vol1");

    let creds = VolumeHandler::<()>::generate_temporary_volume_credentials(
        &b,
        GenerateTemporaryVolumeCredentialsRequest {
            share: "share1".to_string(),
            schema: "schema1".to_string(),
            name: "vol1".to_string(),
            ..Default::default()
        },
        (),
    )
    .await
    .unwrap();
    assert_eq!(creds.url.as_deref(), Some("s3://bucket/vol1"));
    assert!(creds.credentials.is_some());
}

#[tokio::test]
async fn lists_skills() {
    let b = backend();
    let skills = SkillHandler::<()>::list_skills(
        &b,
        ListSkillsRequest {
            share: "share1".to_string(),
            schema: "schema1".to_string(),
            ..Default::default()
        },
        (),
    )
    .await
    .unwrap();
    assert_eq!(skills.items.len(), 1);
    assert_eq!(skills.items[0].name, "skill1");
}

#[tokio::test]
async fn missing_share_is_not_found() {
    let b = backend();
    let err = ShareHandler::<()>::get_share(
        &b,
        GetShareRequest {
            name: "nope".to_string(),
            ..Default::default()
        },
        (),
    )
    .await
    .unwrap_err();
    assert!(matches!(err, Error::NotFound));
}

#[tokio::test]
async fn gap_endpoints_are_not_implemented() {
    let b = backend();
    let changes = SharingQueryHandler::<()>::get_table_changes(
        &b,
        QueryTableRequest {
            share: "share1".to_string(),
            schema: "schema1".to_string(),
            name: "table1".to_string(),
            ..Default::default()
        },
        (),
    )
    .await
    .unwrap_err();
    assert!(matches!(changes, Error::NotImplemented(_)));

    let poll = SharingQueryHandler::<()>::poll_query(&b, "q1".to_string(), ())
        .await
        .unwrap_err();
    assert!(matches!(poll, Error::NotImplemented(_)));
}

#[tokio::test]
async fn list_schemas_paginates() {
    let b = backend();
    let first = ShareHandler::<()>::list_schemas(
        &b,
        ListSchemasRequest {
            share: "share1".to_string(),
            max_results: Some(1),
            ..Default::default()
        },
        (),
    )
    .await
    .unwrap();
    assert_eq!(first.items.len(), 1);
    assert_eq!(first.items[0].name, "schema1");
    let token = first.next_page_token.expect("a second page");

    let second = ShareHandler::<()>::list_schemas(
        &b,
        ListSchemasRequest {
            share: "share1".to_string(),
            max_results: Some(1),
            page_token: Some(token),
            ..Default::default()
        },
        (),
    )
    .await
    .unwrap();
    assert_eq!(second.items.len(), 1);
    assert_eq!(second.items[0].name, "schema2");
    assert_eq!(second.next_page_token, None);
}

#[tokio::test]
async fn list_shares_rejects_non_positive_page_sizes() {
    let b = backend();
    for max_results in [0, -1] {
        let err = ShareHandler::<()>::list_shares(
            &b,
            ListSharesRequest {
                max_results: Some(max_results),
                ..Default::default()
            },
            (),
        )
        .await
        .unwrap_err();
        assert!(matches!(err, Error::InvalidArgument(_)), "{max_results}");
    }
}

#[tokio::test]
async fn list_shares_paginates() {
    let b = backend();
    let page = ShareHandler::<()>::list_shares(
        &b,
        ListSharesRequest {
            max_results: Some(i32::MAX),
            ..Default::default()
        },
        (),
    )
    .await
    .unwrap();
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.next_page_token, None);
}

const SKILL_MD: &str = "---
name: pdf-processing
description: Extract text and tables from PDF files.
license: Apache-2.0
compatibility: Requires poppler-utils
allowed-tools: Bash(pdftotext:*) Read
metadata:
  author: example-org
---

# PDF processing
";

/// A share with two skills on local storage: `good`, whose directory holds a
/// SKILL.md, and `broken`, whose directory does not.
fn skill_backend() -> (tempfile::TempDir, SharingService<()>) {
    let dir = tempfile::tempdir().unwrap();
    let good = dir.path().join("good");
    std::fs::create_dir_all(&good).unwrap();
    std::fs::write(good.join("SKILL.md"), SKILL_MD).unwrap();
    std::fs::create_dir_all(dir.path().join("broken")).unwrap();
    let location = |name: &str| {
        url::Url::from_directory_path(dir.path().join(name))
            .unwrap()
            .to_string()
    };
    let service = in_memory_service(vec![FixtureShare {
        name: "share1".to_string(),
        id: Some("share1-id".to_string()),
        comment: None,
        objects: vec![
            FixtureObject::new(AssetKind::AgentSkill, "schema1", "good", &location("good")),
            FixtureObject::new(
                AssetKind::AgentSkill,
                "schema1",
                "broken",
                &location("broken"),
            ),
        ],
    }]);
    (dir, service)
}

fn get_skill_request(name: &str) -> GetSkillRequest {
    GetSkillRequest {
        share: "share1".to_string(),
        schema: "schema1".to_string(),
        name: name.to_string(),
        ..Default::default()
    }
}

fn assert_declared(skill: &Skill) {
    assert_eq!(
        skill.description.as_deref(),
        Some("Extract text and tables from PDF files.")
    );
    assert_eq!(skill.license.as_deref(), Some("Apache-2.0"));
    assert_eq!(
        skill.compatibility.as_deref(),
        Some("Requires poppler-utils")
    );
    assert_eq!(skill.allowed_tools, vec!["Bash(pdftotext:*)", "Read"]);
    assert_eq!(
        skill.metadata.get("author").map(String::as_str),
        Some("example-org")
    );
}

#[tokio::test]
async fn get_skill_serves_skill_md_metadata() {
    let (_dir, b) = skill_backend();
    let skill = SkillHandler::<()>::get_skill(&b, get_skill_request("good"), ())
        .await
        .unwrap();
    assert_eq!(skill.name, "good");
    assert_eq!(skill.share_id.as_deref(), Some("share1-id"));
    assert!(skill.storage_location.is_some());
    assert_declared(&skill);
}

#[tokio::test]
async fn get_skill_without_skill_md_fails() {
    let (_dir, b) = skill_backend();
    let err = SkillHandler::<()>::get_skill(&b, get_skill_request("broken"), ())
        .await
        .unwrap_err();
    assert!(err.to_string().contains("SKILL.md"), "{err}");
}

#[tokio::test]
async fn list_skills_serves_metadata_and_keeps_broken_skills() {
    let (_dir, b) = skill_backend();
    let listed = SkillHandler::<()>::list_skills(
        &b,
        ListSkillsRequest {
            share: "share1".to_string(),
            schema: "schema1".to_string(),
            ..Default::default()
        },
        (),
    )
    .await
    .unwrap();
    let all = SkillHandler::<()>::list_all_skills(
        &b,
        ListAllSkillsRequest {
            share: "share1".to_string(),
            ..Default::default()
        },
        (),
    )
    .await
    .unwrap();
    for items in [listed.items, all.items] {
        let names: Vec<_> = items.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["good", "broken"]);
        assert_declared(&items[0]);
        assert_eq!(items[1].description, None);
        assert!(items[1].storage_location.is_some());
        assert!(
            items
                .iter()
                .all(|s| s.share_id.as_deref() == Some("share1-id"))
        );
    }
}

#[tokio::test]
async fn list_skills_paginates_over_objects() {
    let (_dir, b) = skill_backend();
    let first = SkillHandler::<()>::list_all_skills(
        &b,
        ListAllSkillsRequest {
            share: "share1".to_string(),
            max_results: Some(1),
            ..Default::default()
        },
        (),
    )
    .await
    .unwrap();
    assert_eq!(first.items.len(), 1);
    assert_declared(&first.items[0]);
    let second = SkillHandler::<()>::list_all_skills(
        &b,
        ListAllSkillsRequest {
            share: "share1".to_string(),
            max_results: Some(1),
            page_token: first.next_page_token,
            ..Default::default()
        },
        (),
    )
    .await
    .unwrap();
    assert_eq!(second.items.len(), 1);
    assert_eq!(second.items[0].name, "broken");
    assert_eq!(second.next_page_token, None);
}

#[tokio::test]
async fn skill_metadata_is_cached_until_its_ttl() {
    let (dir, b) = skill_backend();
    let rewritten = SKILL_MD.replace("Apache-2.0", "MIT");
    let read = |b: &SharingService<()>| {
        let b = b.clone();
        async move {
            SkillHandler::<()>::get_skill(&b, get_skill_request("good"), ())
                .await
                .unwrap()
                .license
        }
    };

    assert_eq!(read(&b).await.as_deref(), Some("Apache-2.0"));
    std::fs::write(dir.path().join("good/SKILL.md"), &rewritten).unwrap();
    assert_eq!(read(&b).await.as_deref(), Some("Apache-2.0"));

    let uncached = b.with_skill_manifest_ttl(std::time::Duration::ZERO);
    assert_eq!(read(&uncached).await.as_deref(), Some("MIT"));
}
