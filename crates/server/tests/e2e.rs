//! End-to-end: a provider sets up shares and recipients through the admin API, a
//! recipient activates its token and reads a real Delta table through both
//! protocol surfaces — with local (static source) and delegated (catalog-vended)
//! storage access.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use olai_storage_credentials::{AccessLevel, StorageCredentials, VendedCredential};
use opensharing_admin_api::auth::Provider;
use opensharing_engine::source::fixed::{StaticSource, StaticTable, StaticVolume};
use opensharing_engine::source::{
    AccessMode, CatalogSource, RegisteredSource, SourceRegistry, SourceTable, SourceVolume,
};
use opensharing_provider::admin::{
    self, AdminAction, AdminPolicy, ProviderService, ProviderTokens,
};
use opensharing_provider::secrets::Secrets;
use opensharing_server::Result;
use opensharing_server::config::{ConfigValue, EncryptionConfig, KeyConfig, KeyProviderKind};
use opensharing_server::embed::{self, Settings, Stores};
use opensharing_server::store::SharingStore;
use opensharing_server::store::memory::InMemoryStore;
use serde_json::{Value, json};
use tower::ServiceExt;

const ADMIN: &str = "provider-secret";
const PUBLIC_URL: &str = "https://sharing.example.com";

fn table_location() -> String {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../engine/tests/data/basic_partitioned")
        .canonicalize()
        .unwrap();
    url::Url::from_directory_path(dir).unwrap().to_string()
}

fn settings() -> Settings {
    Settings {
        public_url: PUBLIC_URL.to_string(),
        provider_tokens: vec![ADMIN.to_string()],
        presign_ttl: Duration::from_secs(300),
        aws_region: None,
    }
}

fn key(id: &str, byte: u8) -> KeyConfig {
    use base64::Engine as _;
    KeyConfig {
        id: id.to_string(),
        key: ConfigValue::Value(base64::engine::general_purpose::STANDARD.encode([byte; 32])),
    }
}

fn secrets(active: KeyConfig, retired: Vec<KeyConfig>) -> Secrets {
    Secrets::new(
        EncryptionConfig {
            provider: KeyProviderKind::Local,
            active,
            retired,
        }
        .build()
        .unwrap(),
    )
}

fn static_sources() -> SourceRegistry {
    let mut source = StaticSource::default();
    source.tables.insert(
        "demo.sales.letters".to_string(),
        StaticTable {
            location: table_location(),
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

struct Client {
    app: Router,
}

impl Client {
    async fn call(
        &self,
        method: &str,
        uri: &str,
        token: Option<&str>,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut req = Request::builder().method(method).uri(uri);
        if let Some(token) = token {
            req = req.header("authorization", format!("Bearer {token}"));
        }
        let req = match body {
            Some(body) => req
                .header("content-type", "application/json")
                .body(Body::from(body.to_string())),
            None => req.body(Body::empty()),
        }
        .unwrap();
        let resp = self.app.clone().oneshot(req).await.unwrap();
        let status = resp.status();
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let value = serde_json::from_slice(&bytes).unwrap_or_else(|_| {
            // NDJSON responses: an array of lines.
            Value::Array(
                bytes
                    .split(|b| *b == b'\n')
                    .filter(|l| !l.is_empty())
                    .map(|l| serde_json::from_slice(l).unwrap_or(Value::Null))
                    .collect(),
            )
        });
        (status, value)
    }

    async fn admin(&self, method: &str, path: &str, body: Option<Value>) -> Value {
        let (status, value) = self
            .call(
                method,
                &format!("/api/2.1/unity-catalog{path}"),
                Some(ADMIN),
                body,
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{method} {path}: {value}");
        value
    }

    /// Activate `recipient`'s pending token, returning the bearer token.
    async fn activate(&self, recipient: &Value) -> String {
        let link = recipient["tokens"][0]["activation_url"].as_str().unwrap();
        let path = link
            .strip_prefix(PUBLIC_URL)
            .expect("link under the public url");
        let (status, profile) = self.call("GET", path, None, None).await;
        assert_eq!(status, StatusCode::OK, "{profile}");
        assert_eq!(
            profile["endpoint"],
            format!("{PUBLIC_URL}/api/v1/delta-sharing")
        );
        // An activation link works once.
        assert_eq!(
            self.call("GET", path, None, None).await.0,
            StatusCode::NOT_FOUND
        );
        profile["bearerToken"].as_str().unwrap().to_string()
    }

    /// Create share `share` holding `object`, and recipient `recipient` granted
    /// it; returns the recipient's bearer token.
    async fn provision(&self, share: &str, object: Value, recipient: &str) -> String {
        self.admin("POST", "/shares", Some(json!({"name": share})))
            .await;
        self.admin(
            "PATCH",
            &format!("/shares/{share}"),
            Some(json!({"updates": [{"action": "ADD", "data_object": object}]})),
        )
        .await;
        let created = self
            .admin(
                "POST",
                "/recipients",
                Some(json!({"name": recipient, "authentication_type": "TOKEN"})),
            )
            .await;
        let token = self.activate(&created).await;
        self.admin(
            "PATCH",
            &format!("/shares/{share}/permissions"),
            Some(json!({"changes": [{"principal": recipient, "add": ["SELECT"]}]})),
        )
        .await;
        token
    }

    async fn query(
        &self,
        token: &str,
        share: &str,
        schema: &str,
        table: &str,
        body: Value,
    ) -> (StatusCode, Vec<Value>) {
        let (status, lines) = self
            .call(
                "POST",
                &format!(
                    "/api/v1/delta-sharing/shares/{share}/schemas/{schema}/tables/{table}/query"
                ),
                Some(token),
                Some(body),
            )
            .await;
        let lines = match lines {
            Value::Array(lines) => lines,
            other => vec![other],
        };
        (status, lines)
    }
}

fn files(lines: &[Value]) -> Vec<&Value> {
    lines.iter().filter_map(|l| l.get("file")).collect()
}

fn app(stores: Stores, secrets: Secrets, sources: SourceRegistry) -> Client {
    Client {
        app: embed::router(&stores, secrets.clone(), sources, settings())
            .merge(embed::admin_router(&stores, secrets, &settings())),
    }
}

#[tokio::test]
async fn provider_shares_a_table_with_a_recipient() {
    let client = app(
        Stores::memory(),
        secrets(key("v1", 1), vec![]),
        static_sources(),
    );

    // The admin API requires a provider token.
    let (status, _) = client
        .call("GET", "/api/2.1/unity-catalog/shares", Some("wrong"), None)
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let token = client
        .provision(
            "sales",
            json!({
                "name": "demo.sales.letters",
                "data_object_type": "TABLE",
                "history_data_sharing_status": "ENABLED"
            }),
            "acme",
        )
        .await;

    // Discovery on both protocol surfaces.
    for prefix in ["/api/v1/delta-sharing", "/api/1.0/opensharing"] {
        let (status, shares) = client
            .call("GET", &format!("{prefix}/shares"), Some(&token), None)
            .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(shares["items"][0]["name"], "sales", "{shares}");
        let (_, tables) = client
            .call(
                "GET",
                &format!("{prefix}/shares/sales/all-tables"),
                Some(&token),
                None,
            )
            .await;
        assert_eq!(tables["items"][0]["schema"], "sales");
        assert_eq!(tables["items"][0]["name"], "letters");
    }

    // Query the latest version and time-travel to version 0.
    let (status, lines) = client
        .query(&token, "sales", "sales", "letters", json!({}))
        .await;
    assert_eq!(status, StatusCode::OK, "{lines:?}");
    assert_eq!(files(&lines).len(), 6);
    assert!(
        files(&lines)[0]["url"]
            .as_str()
            .unwrap()
            .starts_with("file://")
    );
    let (_, lines) = client
        .query(&token, "sales", "sales", "letters", json!({"version": 0}))
        .await;
    assert_eq!(files(&lines).len(), 3);

    // Temporary table credentials (OpenSharing).
    let (status, credentials) = client
        .call(
            "POST",
            "/api/1.0/opensharing/shares/sales/schemas/sales/tables/letters/temporary-table-credentials",
            Some(&token),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{credentials}");
    assert_eq!(credentials["url"], table_location());

    // Revoking the grant hides the share.
    client
        .admin(
            "PATCH",
            "/shares/sales/permissions",
            Some(json!({"changes": [{"principal": "acme", "remove": ["SELECT"]}]})),
        )
        .await;
    let (_, shares) = client
        .call("GET", "/api/v1/delta-sharing/shares", Some(&token), None)
        .await;
    assert!(
        shares["items"].as_array().is_none_or(Vec::is_empty),
        "{shares}"
    );
    let (status, _) = client
        .query(&token, "sales", "sales", "letters", json!({}))
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn history_is_shared_only_when_enabled() {
    let client = app(
        Stores::memory(),
        secrets(key("v1", 1), vec![]),
        static_sources(),
    );
    let token = client
        .provision(
            "sales",
            json!({"name": "demo.sales.letters", "data_object_type": "TABLE"}),
            "acme",
        )
        .await;
    let (status, _) = client
        .query(&token, "sales", "sales", "letters", json!({}))
        .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = client
        .query(&token, "sales", "sales", "letters", json!({"version": 0}))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn schemas_expand_to_their_tables() {
    let client = app(
        Stores::memory(),
        secrets(key("v1", 1), vec![]),
        static_sources(),
    );
    let token = client
        .provision(
            "everything",
            json!({"name": "demo.sales", "data_object_type": "SCHEMA", "shared_as": "shared"}),
            "acme",
        )
        .await;
    let (_, tables) = client
        .call(
            "GET",
            "/api/v1/delta-sharing/shares/everything/schemas/shared/tables",
            Some(&token),
            None,
        )
        .await;
    assert_eq!(tables["items"][0]["name"], "letters", "{tables}");
    let (status, lines) = client
        .query(&token, "everything", "shared", "letters", json!({}))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(files(&lines).len(), 6);
}

#[tokio::test]
async fn shared_skills_carry_their_skill_md_metadata() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data/pdf-skill")
        .canonicalize()
        .unwrap();
    let mut source = StaticSource::default();
    source.volumes.insert(
        "demo.agents.pdf".to_string(),
        StaticVolume {
            location: url::Url::from_directory_path(dir).unwrap().to_string(),
        },
    );
    let sources = SourceRegistry::new(vec![RegisteredSource {
        name: "static".to_string(),
        catalogs: vec!["demo".to_string()],
        access: AccessMode::Local,
        source: Arc::new(source),
    }]);
    let client = app(Stores::memory(), secrets(key("v1", 1), vec![]), sources);
    let token = client
        .provision(
            "tools",
            json!({"name": "demo.agents.pdf", "data_object_type": "AGENT_SKILL"}),
            "acme",
        )
        .await;

    let base = "/api/1.0/opensharing/shares/tools";
    let (status, skill) = client
        .call(
            "GET",
            &format!("{base}/schemas/agents/skills/pdf"),
            Some(&token),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{skill}");
    let (status, listed) = client
        .call("GET", &format!("{base}/all-skills"), Some(&token), None)
        .await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    for skill in [&skill, &listed["items"][0]] {
        assert_eq!(skill["name"], "pdf", "{skill}");
        assert_eq!(
            skill["description"],
            "Extract text and tables from PDF files."
        );
        assert_eq!(skill["license"], "Apache-2.0");
        assert_eq!(skill["compatibility"], "Requires poppler-utils");
        assert_eq!(skill["allowed_tools"], json!(["Bash(pdftotext:*)", "Read"]));
        assert_eq!(skill["metadata"], json!({"author": "example-org"}));
    }
}

#[tokio::test]
async fn rotating_a_token_invalidates_the_old_one() {
    let client = app(
        Stores::memory(),
        secrets(key("v1", 1), vec![]),
        static_sources(),
    );
    let old = client
        .provision(
            "sales",
            json!({"name": "demo.sales.letters", "data_object_type": "TABLE"}),
            "acme",
        )
        .await;
    let rotated = client
        .admin("POST", "/recipients/acme/rotate-token", Some(json!({})))
        .await;
    let new = client.activate(&rotated).await;
    let shares = |token: String| {
        let client = &client;
        async move {
            client
                .call("GET", "/api/v1/delta-sharing/shares", Some(&token), None)
                .await
                .0
        }
    };
    assert_eq!(shares(old).await, StatusCode::UNAUTHORIZED);
    assert_eq!(shares(new).await, StatusCode::OK);
}

/// A proto JSON int64: a string, or absent for 0.
fn int64(value: &Value) -> i64 {
    value.as_str().map_or(0, |s| s.parse().unwrap())
}

#[tokio::test]
async fn rotation_keeps_the_old_token_for_the_grace_period() {
    let client = app(
        Stores::memory(),
        secrets(key("v1", 1), vec![]),
        static_sources(),
    );
    let first = client
        .provision(
            "sales",
            json!({"name": "demo.sales.letters", "data_object_type": "TABLE"}),
            "acme",
        )
        .await;
    let shares = |token: &str| {
        let client = &client;
        let token = token.to_string();
        async move {
            client
                .call("GET", "/api/v1/delta-sharing/shares", Some(&token), None)
                .await
                .0
        }
    };

    let rotated = client
        .admin(
            "POST",
            "/recipients/acme/rotate-token",
            Some(json!({"existing_token_expire_in_seconds": 3600})),
        )
        .await;
    let tokens = rotated["tokens"].as_array().unwrap();
    assert_eq!(tokens.len(), 2, "{rotated}");
    // The old token now expires; the new one, like the old one did, never.
    assert!(int64(&tokens[1]["expiration_time"]) > 0, "{rotated}");
    assert_eq!(int64(&tokens[0]["expiration_time"]), 0, "{rotated}");
    let second = client.activate(&rotated).await;
    assert_eq!(shares(&first).await, StatusCode::OK);
    assert_eq!(shares(&second).await, StatusCode::OK);

    // A zero grace period revokes the others at once.
    let rotated = client
        .admin(
            "POST",
            "/recipients/acme/rotate-token",
            Some(json!({"existing_token_expire_in_seconds": 0})),
        )
        .await;
    assert_eq!(rotated["tokens"].as_array().unwrap().len(), 1, "{rotated}");
    let third = client.activate(&rotated).await;
    assert_eq!(shares(&first).await, StatusCode::UNAUTHORIZED);
    assert_eq!(shares(&second).await, StatusCode::UNAUTHORIZED);
    assert_eq!(shares(&third).await, StatusCode::OK);
}

#[tokio::test]
async fn updating_the_expiration_time_expires_and_renews_tokens() {
    let client = app(
        Stores::memory(),
        secrets(key("v1", 1), vec![]),
        static_sources(),
    );
    let token = client
        .provision(
            "sales",
            json!({"name": "demo.sales.letters", "data_object_type": "TABLE"}),
            "acme",
        )
        .await;
    let shares = |token: String| {
        let client = &client;
        async move {
            client
                .call("GET", "/api/v1/delta-sharing/shares", Some(&token), None)
                .await
                .0
        }
    };

    // Expired: the bearer token is refused.
    let updated = client
        .admin(
            "PATCH",
            "/recipients/acme",
            Some(json!({"expiration_time": 1})),
        )
        .await;
    assert_eq!(
        int64(&updated["tokens"][0]["expiration_time"]),
        1,
        "{updated}"
    );
    assert_eq!(shares(token).await, StatusCode::UNAUTHORIZED);

    // A rotated token inherits the expiry, so its link does not activate…
    let rotated = client
        .admin("POST", "/recipients/acme/rotate-token", Some(json!({})))
        .await;
    let link = rotated["tokens"][0]["activation_url"].as_str().unwrap();
    let path = link.strip_prefix(PUBLIC_URL).unwrap().to_string();
    assert_eq!(
        client.call("GET", &path, None, None).await.0,
        StatusCode::NOT_FOUND
    );

    // …until the expiry is lifted (0: never), and the same link works.
    let renewed = client
        .admin(
            "PATCH",
            "/recipients/acme",
            Some(json!({"expiration_time": 0})),
        )
        .await;
    assert_eq!(renewed["tokens"][0]["activation_url"], link, "{renewed}");
    let token = client.activate(&renewed).await;
    assert_eq!(shares(token).await, StatusCode::OK);
}

/// A catalog that vends (local) credentials and counts how often.
struct VendingCatalog {
    location: String,
    vends: AtomicUsize,
}

#[async_trait]
impl CatalogSource for VendingCatalog {
    async fn get_table(&self, full_name: &str) -> opensharing_engine::Result<SourceTable> {
        Ok(SourceTable {
            full_name: full_name.to_string(),
            name: full_name.rsplit('.').next().unwrap().to_string(),
            id: Some("11111111-1111-1111-1111-111111111111".to_string()),
            location: self.location.clone(),
            format: opensharing_engine::catalog::DataFormat::Delta,
        })
    }

    async fn list_tables(
        &self,
        _catalog: &str,
        _schema: &str,
    ) -> opensharing_engine::Result<Vec<SourceTable>> {
        Ok(Vec::new())
    }

    async fn get_volume(&self, _full_name: &str) -> opensharing_engine::Result<SourceVolume> {
        Err(opensharing_engine::Error::NotFound)
    }

    async fn vend_table(
        &self,
        table: &SourceTable,
    ) -> opensharing_engine::Result<VendedCredential> {
        self.vends.fetch_add(1, Ordering::SeqCst);
        let scope = table.location.parse().expect("table location parses");
        Ok(
            VendedCredential::new(StorageCredentials::None, scope, AccessLevel::Read)
                .with_expiry(Some(chrono::Utc::now() + chrono::TimeDelta::hours(1))),
        )
    }
}

#[tokio::test]
async fn delegated_access_uses_catalog_vended_credentials() {
    let catalog = Arc::new(VendingCatalog {
        location: table_location(),
        vends: AtomicUsize::new(0),
    });
    let sources = SourceRegistry::new(vec![RegisteredSource {
        name: "uc".to_string(),
        catalogs: vec!["*".to_string()],
        access: AccessMode::Delegated,
        source: catalog.clone(),
    }]);
    // No storage credentials are kept at all: everything comes from the catalog.
    let stores = Stores::new(Arc::new(InMemoryStore::new()));
    let client = app(stores, secrets(key("v1", 1), vec![]), sources);
    let token = client
        .provision(
            "sales",
            json!({"name": "main.sales.letters", "data_object_type": "TABLE"}),
            "acme",
        )
        .await;
    for _ in 0..2 {
        let (status, lines) = client
            .query(&token, "sales", "sales", "letters", json!({}))
            .await;
        assert_eq!(status, StatusCode::OK, "{lines:?}");
        assert_eq!(files(&lines).len(), 6);
    }
    // The vended credential is cached until close to its expiry.
    assert_eq!(catalog.vends.load(Ordering::SeqCst), 1);
}

/// The stores over the SQLite database at `url` (already migrated), sealing
/// with `secrets`, as `Config::stores` builds them for the server.
#[cfg(all(feature = "unity", feature = "sqlite"))]
async fn sqlite_stores(url: &str, secrets: &Secrets) -> Stores {
    use opensharing_server::config::{Config, DatabaseConfig};

    let config = Config {
        database: DatabaseConfig::Sqlite {
            url: ConfigValue::Value(url.to_string()),
            max_connections: 2,
        },
        ..Config::default()
    };
    config.stores(secrets).await.unwrap()
}

// Stores a credential through the UC `credentials` route, in SQLite: the
// in-memory storage-access store keeps no sealed secrets to re-wrap.
#[cfg(all(feature = "unity", feature = "sqlite"))]
#[tokio::test]
async fn rotating_keys_rewraps_stored_secrets() {
    use opensharing_server::store::{sqlite, sqlite_migrator};

    let dir = tempfile::tempdir().unwrap();
    let url = format!("sqlite://{}", dir.path().join("sharing.db").display());
    let pool = sqlite::connect(&url, 1).await.unwrap();
    sqlite::migrate(&sqlite_migrator(), &pool).await.unwrap();
    pool.close().await;

    // Provision under key v1: a stored credential governing a location, and a
    // pending recipient token.
    let v1_secrets = secrets(key("v1", 1), vec![]);
    let v1 = app(
        sqlite_stores(&url, &v1_secrets).await,
        v1_secrets,
        static_sources(),
    );
    v1.admin(
        "POST",
        "/credentials",
        Some(json!({
            "name": "storage",
            "purpose": "STORAGE",
            "azure_storage_key": {"account_name": "acct", "account_key": "a2V5"}
        })),
    )
    .await;
    v1.admin(
        "POST",
        "/external-locations",
        Some(json!({
            "name": "data",
            "url": "abfss://data@acct.dfs.core.windows.net/",
            "credential_name": "storage"
        })),
    )
    .await;
    let recipient = v1
        .admin(
            "POST",
            "/recipients",
            Some(json!({"name": "acme", "authentication_type": "TOKEN"})),
        )
        .await;

    // Rotate: v2 becomes active, v1 is retired.
    let rotating = secrets(key("v2", 2), vec![key("v1", 1)]);
    let stores = sqlite_stores(&url, &rotating).await;
    assert_eq!(
        embed::rotate_keys(&stores, &rotating).await.unwrap(),
        (1, 1)
    );
    assert_eq!(
        embed::rotate_keys(&stores, &rotating).await.unwrap(),
        (0, 0),
        "everything is already under v2"
    );

    // With v1 gone entirely, the credential still opens and the pending token
    // still activates.
    let v2_secrets = secrets(key("v2", 2), vec![]);
    let stores = sqlite_stores(&url, &v2_secrets).await;
    let governed = stores
        .storage_access
        .clone()
        .unwrap()
        .governing_location(&"abfss://data@acct.dfs.core.windows.net/x".parse().unwrap())
        .await
        .unwrap();
    assert_eq!(
        governed
            .credential
            .azure_storage_key
            .as_option()
            .map(|k| k.account_key.as_str()),
        Some("a2V5")
    );
    let v2 = app(stores, v2_secrets, static_sources());
    let token = v2.activate(&recipient).await;
    assert!(!token.is_empty());
}

/// A static source serving one table at `location`, with local access.
#[cfg(feature = "unity")]
fn local_source(location: &str) -> SourceRegistry {
    let mut source = StaticSource::default();
    source.tables.insert(
        "demo.sales.letters".to_string(),
        StaticTable {
            location: location.to_string(),
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

#[cfg(feature = "unity")]
#[tokio::test]
async fn stored_credentials_govern_local_access() {
    const TABLE: &str = "abfss://data@acct.dfs.core.windows.net/sales/letters";
    let client = app(
        Stores::memory(),
        secrets(key("v1", 1), vec![]),
        local_source(TABLE),
    );
    let token = client
        .provision(
            "sales",
            json!({"name": "demo.sales.letters", "data_object_type": "TABLE"}),
            "acme",
        )
        .await;
    let vend = || {
        client.call(
            "POST",
            "/api/1.0/opensharing/shares/sales/schemas/sales/tables/letters/temporary-table-credentials",
            Some(&token),
            Some(json!({})),
        )
    };
    let call = |method: &'static str, path: &'static str, body: Option<Value>| {
        let client = &client;
        async move {
            client
                .call(
                    method,
                    &format!("/api/2.1/unity-catalog{path}"),
                    Some(ADMIN),
                    body,
                )
                .await
        }
    };

    // Nothing governs the table yet.
    let (status, body) = vend().await;
    assert_ne!(status, StatusCode::OK, "{body}");

    // Register a credential (returned without its secret) and a location.
    let credential = client
        .admin(
            "POST",
            "/credentials",
            Some(json!({
                "name": "storage",
                "purpose": "STORAGE",
                "azure_storage_key": {"account_name": "acct", "account_key": "a2V5"}
            })),
        )
        .await;
    assert!(!credential.to_string().contains("a2V5"), "{credential}");
    client.admin("GET", "/credentials/storage", None).await;
    client
        .admin(
            "POST",
            "/external-locations",
            Some(json!({
                "name": "sales",
                "url": "abfss://data@acct.dfs.core.windows.net/sales",
                "credential_name": "storage"
            })),
        )
        .await;

    // Locations never overlap, in either direction.
    for url in [
        "abfss://data@acct.dfs.core.windows.net/sales/letters",
        "abfss://data@acct.dfs.core.windows.net/",
    ] {
        let (status, body) = call(
            "POST",
            "/external-locations",
            Some(json!({"name": "other", "url": url, "credential_name": "storage"})),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{url}: {body}");
    }

    // The table now vends a SAS minted from the stored account key.
    let (status, body) = vend().await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.to_string().to_lowercase().contains("sig="), "{body}");

    // A credential in use cannot be deleted; once its location is gone it can.
    let (status, body) = call("DELETE", "/credentials/storage", None).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    let updated = client
        .admin(
            "PATCH",
            "/external-locations/sales",
            Some(json!({"comment": "letters"})),
        )
        .await;
    assert_eq!(updated["comment"], "letters", "{updated}");
    client
        .admin("DELETE", "/external-locations/sales", None)
        .await;
    client.admin("DELETE", "/credentials/storage", None).await;
    let listed = client.admin("GET", "/credentials", None).await;
    assert!(
        listed["credentials"].as_array().is_none_or(Vec::is_empty),
        "{listed}"
    );
}

/// Without stored credentials (`storage_credentials: none`) the credential
/// routes are not mounted.
#[cfg(feature = "unity")]
#[tokio::test]
async fn without_stored_credentials_the_routes_are_absent() {
    let client = app(
        Stores::new(Arc::new(InMemoryStore::new())),
        secrets(key("v1", 1), vec![]),
        static_sources(),
    );
    for path in ["/credentials", "/external-locations"] {
        let (status, _) = client
            .call(
                "GET",
                &format!("/api/2.1/unity-catalog{path}"),
                Some(ADMIN),
                None,
            )
            .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
    }
}

/// Allows everything but deleting shares and managing storage.
struct NoDeletes;

#[async_trait]
impl AdminPolicy<Provider> for NoDeletes {
    async fn authorize(&self, action: AdminAction<'_>, _context: &Provider) -> Result<()> {
        match action {
            AdminAction::DeleteShare { .. } | AdminAction::ManageStorage => {
                Err(opensharing_server::Error::NotAllowed)
            }
            _ => Ok(()),
        }
    }
}

#[tokio::test]
async fn the_admin_policy_authorizes_every_action() {
    let store: Arc<dyn SharingStore> = Arc::new(InMemoryStore::new());
    let service =
        ProviderService::new(store, secrets(key("v1", 1), vec![]), PUBLIC_URL.to_string())
            .with_policy(Arc::new(NoDeletes));
    let client = Client {
        app: admin::mount(
            embed::admin_routes(service, &Stores::memory()),
            Arc::new(ProviderTokens::new([ADMIN])),
        ),
    };

    client
        .admin("POST", "/shares", Some(json!({"name": "sales"})))
        .await;
    let (status, body) = client
        .call(
            "DELETE",
            "/api/2.1/unity-catalog/shares/sales",
            Some(ADMIN),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    client.admin("GET", "/shares/sales", None).await;

    #[cfg(feature = "unity")]
    {
        let (status, _) = client
            .call(
                "GET",
                "/api/2.1/unity-catalog/credentials",
                Some(ADMIN),
                None,
            )
            .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }
}
