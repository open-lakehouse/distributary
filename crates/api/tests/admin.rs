//! The admin routers, mounted the way a catalog host would: its own handler
//! implementation over its own state, and its own request context, extracted
//! per request and handed to every handler.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::body::Body;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::{Request, StatusCode};
use opensharing_admin::models::recipients::v1::{
    CreateRecipientRequest, DeleteRecipientRequest, GetRecipientRequest, ListRecipientsRequest,
    ListRecipientsResponse, Recipient, RotateRecipientTokenRequest, UpdateRecipientRequest,
};
use opensharing_admin::models::shares::v1::{
    CreateShareRequest, DeleteShareRequest, GetPermissionsRequest, GetPermissionsResponse,
    GetShareRequest, ListSharesRequest, ListSharesResponse, Share, UpdatePermissionsRequest,
    UpdatePermissionsResponse, UpdateShareRequest,
};
use opensharing_api::admin::{
    ADMIN_PREFIX, RecipientHandler, ShareHandler, admin_router, shares_router,
};
use opensharing_api::{Error, Result};
use serde_json::{Value, json};
use tower::ServiceExt;

/// A host's request context: the caller, from a header its own auth set.
#[derive(Debug, Clone)]
struct Principal(String);

impl<S: Send + Sync> FromRequestParts<S> for Principal {
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self> {
        parts
            .headers
            .get("x-principal")
            .and_then(|v| v.to_str().ok())
            .map(|v| Principal(v.to_string()))
            .ok_or(Error::Unauthenticated)
    }
}

/// Shares by name, with the principal that created each one as its owner.
#[derive(Clone, Default)]
struct Host {
    shares: Arc<Mutex<BTreeMap<String, Share>>>,
}

impl Host {
    fn admin(context: &Principal) -> Result<()> {
        if context.0 == "admin" {
            Ok(())
        } else {
            Err(Error::NotAllowed)
        }
    }
}

#[async_trait]
impl ShareHandler<Principal> for Host {
    async fn list_shares(
        &self,
        _request: ListSharesRequest,
        _context: Principal,
    ) -> Result<ListSharesResponse> {
        Ok(ListSharesResponse {
            shares: self.shares.lock().unwrap().values().cloned().collect(),
            ..Default::default()
        })
    }

    async fn create_share(&self, request: CreateShareRequest, context: Principal) -> Result<Share> {
        Host::admin(&context)?;
        let share = Share {
            name: request.name.clone(),
            owner: Some(context.0),
            comment: request.comment,
            ..Default::default()
        };
        self.shares
            .lock()
            .unwrap()
            .insert(request.name, share.clone());
        Ok(share)
    }

    async fn get_share(&self, request: GetShareRequest, _context: Principal) -> Result<Share> {
        self.shares
            .lock()
            .unwrap()
            .get(&request.name)
            .cloned()
            .ok_or(Error::NotFound)
    }

    async fn update_share(
        &self,
        _request: UpdateShareRequest,
        _context: Principal,
    ) -> Result<Share> {
        Err(Error::not_implemented("update_share"))
    }

    async fn delete_share(&self, request: DeleteShareRequest, context: Principal) -> Result<()> {
        Host::admin(&context)?;
        self.shares
            .lock()
            .unwrap()
            .remove(&request.name)
            .map(|_| ())
            .ok_or(Error::NotFound)
    }

    async fn get_permissions(
        &self,
        _request: GetPermissionsRequest,
        _context: Principal,
    ) -> Result<GetPermissionsResponse> {
        Ok(GetPermissionsResponse::default())
    }

    async fn update_permissions(
        &self,
        _request: UpdatePermissionsRequest,
        _context: Principal,
    ) -> Result<UpdatePermissionsResponse> {
        Err(Error::not_implemented("update_permissions"))
    }
}

#[async_trait]
impl RecipientHandler<Principal> for Host {
    async fn list_recipients(
        &self,
        _request: ListRecipientsRequest,
        _context: Principal,
    ) -> Result<ListRecipientsResponse> {
        Ok(ListRecipientsResponse::default())
    }

    async fn create_recipient(
        &self,
        request: CreateRecipientRequest,
        context: Principal,
    ) -> Result<Recipient> {
        Ok(Recipient {
            name: request.name,
            owner: Some(context.0),
            ..Default::default()
        })
    }

    async fn get_recipient(
        &self,
        _request: GetRecipientRequest,
        _context: Principal,
    ) -> Result<Recipient> {
        Err(Error::NotFound)
    }

    async fn update_recipient(
        &self,
        _request: UpdateRecipientRequest,
        _context: Principal,
    ) -> Result<Recipient> {
        Err(Error::NotFound)
    }

    async fn delete_recipient(
        &self,
        _request: DeleteRecipientRequest,
        _context: Principal,
    ) -> Result<()> {
        Err(Error::NotFound)
    }

    async fn rotate_recipient_token(
        &self,
        _request: RotateRecipientTokenRequest,
        _context: Principal,
    ) -> Result<Recipient> {
        Err(Error::NotFound)
    }
}

async fn call(
    app: &axum::Router,
    method: &str,
    path: &str,
    principal: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut req = Request::builder()
        .method(method)
        .uri(format!("{ADMIN_PREFIX}{path}"));
    if let Some(principal) = principal {
        req = req.header("x-principal", principal);
    }
    let body = match body {
        Some(body) => {
            req = req.header("content-type", "application/json");
            Body::from(body.to_string())
        }
        None => Body::empty(),
    };
    let resp = app.clone().oneshot(req.body(body).unwrap()).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, value)
}

fn app() -> axum::Router {
    axum::Router::new().nest(
        ADMIN_PREFIX,
        admin_router::<Host, Principal>(Host::default()),
    )
}

#[tokio::test]
async fn the_host_context_reaches_the_handlers() {
    let app = app();
    let (status, share) = call(
        &app,
        "POST",
        "/shares",
        Some("admin"),
        Some(json!({ "name": "sales", "comment": "q3" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(share["owner"], "admin");

    let (status, share) = call(&app, "GET", "/shares/sales", Some("someone"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(share["comment"], "q3");

    let (status, listed) = call(&app, "GET", "/shares", Some("someone"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(listed["shares"][0]["name"], "sales");

    let (status, recipient) = call(
        &app,
        "POST",
        "/recipients",
        Some("ops"),
        Some(json!({ "name": "acme" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(recipient["owner"], "ops");

    let (status, _) = call(&app, "DELETE", "/shares/sales", Some("admin"), None).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = call(&app, "GET", "/shares/sales", Some("admin"), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn handler_errors_use_the_error_envelope() {
    let app = app();
    let (status, _) = call(&app, "GET", "/shares", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, body) = call(
        &app,
        "POST",
        "/shares",
        Some("someone"),
        Some(json!({ "name": "sales" })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(body["errorCode"].is_string());

    let (status, body) = call(
        &app,
        "PATCH",
        "/shares/sales/permissions",
        Some("admin"),
        Some(json!({ "changes": [] })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_IMPLEMENTED);
    assert_eq!(body["errorCode"], "NOT_IMPLEMENTED");
}

#[tokio::test]
async fn a_host_mounts_only_the_services_it_serves() {
    let app = axum::Router::new().nest(
        ADMIN_PREFIX,
        shares_router::<Host, Principal>(Host::default()),
    );
    let (status, _) = call(&app, "GET", "/shares", Some("admin"), None).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = call(&app, "GET", "/recipients", Some("admin"), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
