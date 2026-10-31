//! A catalog that keeps shares and recipients in its own store and reuses the
//! server's protocol side on top of it. It implements only the read-only
//! [`ShareReader`] and [`TokenReader`] views and builds the sharing router
//! itself, from [`StoreCatalog`], [`StoreAuthenticator`] and
//! [`SourceStorageAccess`]. Runs without any feature.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use opensharing_api::auth::{Recipient, require_recipient};
use opensharing_api::{SharingService, sharing_router};
use opensharing_server::access::SourceStorageAccess;
use opensharing_server::catalog::StoreCatalog;
use opensharing_server::config::EncryptionConfig;
use opensharing_server::recipients::{StoreAuthenticator, token_hash};
use opensharing_server::secrets::Secrets;
use opensharing_server::source::fixed::{StaticSource, StaticTable};
use opensharing_server::source::{AccessMode, RegisteredSource, SourceRegistry};
use opensharing_server::store::memory::InMemoryStore;
use opensharing_server::store::{
    RecipientRecord, ShareObject, ShareObjectKind, ShareReader, ShareRecord, TokenReader,
    TokenRecord,
};
use opensharing_server::{Error, Result};
use serde_json::Value;
use tower::ServiceExt;

const TOKEN: &str = "host-issued-token";

/// The catalog's own state: one share, granted to one recipient with one token.
struct HostStore {
    share: ShareRecord,
    recipient: RecipientRecord,
}

impl HostStore {
    fn new() -> Self {
        Self {
            share: ShareRecord {
                id: "share-1".to_string(),
                name: "letters".to_string(),
                comment: None,
                objects: vec![ShareObject::new(
                    ShareObjectKind::Table,
                    "demo.sales.letters",
                )],
                created_at: 0,
                updated_at: 0,
            },
            recipient: RecipientRecord {
                id: "recipient-1".to_string(),
                name: "acme".to_string(),
                comment: None,
                created_at: 0,
            },
        }
    }
}

#[async_trait]
impl ShareReader for HostStore {
    async fn get_share(&self, name: &str) -> Result<ShareRecord> {
        (name == self.share.name)
            .then(|| self.share.clone())
            .ok_or(Error::NotFound)
    }

    async fn shares_of(&self, recipient_id: &str) -> Result<Vec<ShareRecord>> {
        Ok(if recipient_id == self.recipient.id {
            vec![self.share.clone()]
        } else {
            Vec::new()
        })
    }

    async fn is_granted(&self, share: &str, recipient_id: &str) -> Result<bool> {
        Ok(share == self.share.name && recipient_id == self.recipient.id)
    }
}

#[async_trait]
impl TokenReader for HostStore {
    async fn token_by_hash(&self, hash: &[u8]) -> Result<(TokenRecord, RecipientRecord)> {
        if hash != token_hash(TOKEN).as_slice() {
            return Err(Error::NotFound);
        }
        let token = TokenRecord {
            id: "token-1".to_string(),
            recipient_id: self.recipient.id.clone(),
            token_hash: hash.to_vec(),
            sealed_token: None,
            activation_code: None,
            expires_at: None,
            created_at: 0,
        };
        Ok((token, self.recipient.clone()))
    }
}

fn sources() -> SourceRegistry {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../api/tests/data/basic_partitioned")
        .canonicalize()
        .unwrap();
    let mut source = StaticSource::default();
    source.tables.insert(
        "demo.sales.letters".to_string(),
        StaticTable {
            location: url::Url::from_directory_path(dir).unwrap().to_string(),
            format: "delta".to_string(),
        },
    );
    SourceRegistry::new(vec![RegisteredSource {
        name: "static".to_string(),
        catalogs: vec!["demo".to_string()],
        access: AccessMode::Local,
        source: Arc::new(source),
    }])
}

/// The host's sharing router, assembled from the server's protocol pieces.
fn router(host: Arc<HostStore>) -> Router {
    let secrets = Secrets::new(EncryptionConfig::insecure_dev().build().unwrap());
    let catalog = Arc::new(StoreCatalog::new(host.clone(), sources()));
    // The host stores no storage credentials; local access reads the table directly.
    let access = Arc::new(SourceStorageAccess::new(
        sources(),
        Arc::new(InMemoryStore::new()),
        secrets,
        Duration::from_secs(300),
        None,
    ));
    let service = SharingService::<Recipient>::new(catalog, access);
    require_recipient(
        sharing_router::<SharingService<Recipient>, Recipient>(service),
        Arc::new(StoreAuthenticator::new(host)),
    )
}

async fn call(app: &Router, method: &str, uri: &str, token: Option<&str>) -> (StatusCode, String) {
    let mut req = Request::builder().method(method).uri(uri);
    if let Some(token) = token {
        req = req.header("authorization", format!("Bearer {token}"));
    }
    if method == "POST" {
        req = req.header("content-type", "application/json");
    }
    let body = if method == "POST" { "{}" } else { "" };
    let resp = app
        .clone()
        .oneshot(req.body(Body::from(body)).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, String::from_utf8(bytes.to_vec()).unwrap())
}

#[tokio::test]
async fn serves_the_protocol_over_a_host_store() {
    let app = router(Arc::new(HostStore::new()));

    let (status, _) = call(&app, "GET", "/api/1.0/opensharing/shares", Some("wrong")).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, shares) = call(&app, "GET", "/api/1.0/opensharing/shares", Some(TOKEN)).await;
    assert_eq!(status, StatusCode::OK, "{shares}");
    let shares: Value = serde_json::from_str(&shares).unwrap();
    assert_eq!(shares["items"][0]["name"], "letters");

    let (status, lines) = call(
        &app,
        "POST",
        "/api/v1/delta-sharing/shares/letters/schemas/sales/tables/letters/query",
        Some(TOKEN),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{lines}");
    assert!(
        lines
            .lines()
            .map(|l| serde_json::from_str::<Value>(l).unwrap())
            .any(|l| l.get("file").is_some()),
        "expected file lines: {lines}"
    );
}
