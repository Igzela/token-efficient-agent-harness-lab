use std::collections::HashSet;
use std::sync::OnceLock;

use axum::body::{to_bytes, Body};
use axum::http::{header, Method, Request, StatusCode};
use engine::http_server::{build_axum_router, AxumApiState};
use engine::infrastructure::auth::{Tenant, TenantResolver};
use engine::infrastructure::rate_limiter::RateLimiter;
use engine::product_golden_path::PRODUCT_TASK_GATE;
use engine::storage::local_product_store::LocalProductStore;
use serde_json::{json, Value};
use tokio::sync::Mutex;
use tower::ServiceExt;

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

async fn response_json(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), 1_048_576).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

async fn post(app: &axum::Router, key: &str, path: &str, body: Value) -> axum::response::Response {
    app.clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(path)
                .header(header::AUTHORIZATION, format!("Bearer {key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn product_approval_and_output_have_separate_authority_and_confirmation() {
    let _guard = env_lock().lock().await;
    std::env::set_var(PRODUCT_TASK_GATE, "1");
    let dir = tempfile::tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("authority.db")).unwrap();
    let mut resolver = TenantResolver::new();
    let available = HashSet::from(["dispatch:execute".to_string(), "team:admin".to_string()]);
    resolver.add_tenant(Tenant {
        tenant_id: "local".to_string(),
        name: "Local".to_string(),
        scopes: available,
        rate_limit: Some(100),
    });
    let (_, execute_key) = resolver
        .create_api_key(
            "local",
            Some(HashSet::from(["dispatch:execute".to_string()])),
            None,
            1.0,
        )
        .unwrap();
    let (_, admin_key) = resolver
        .create_api_key(
            "local",
            Some(HashSet::from(["team:admin".to_string()])),
            None,
            1.0,
        )
        .unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store).with_auth(
        resolver,
        RateLimiter::new(60.0, 100),
        Some(100),
        1.0,
    ));

    let unauthorized_approval = post(
        &app,
        &execute_key,
        "/api/v1/product/tasks/missing/approve",
        json!({"expected_task_version": 7}),
    )
    .await;
    assert_eq!(unauthorized_approval.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        response_json(unauthorized_approval).await["code"],
        "missing_scope"
    );

    let unauthorized_combined = post(
        &app,
        &execute_key,
        "/api/v1/product/tasks/missing/approve-and-output",
        json!({"confirm_output": true}),
    )
    .await;
    assert_eq!(unauthorized_combined.status(), StatusCode::FORBIDDEN);

    let authorized_approval = post(
        &app,
        &admin_key,
        "/api/v1/product/tasks/missing/approve",
        json!({"expected_task_version": 7}),
    )
    .await;
    assert_eq!(authorized_approval.status(), StatusCode::NOT_FOUND);

    let missing_confirmation = post(
        &app,
        &execute_key,
        "/api/v1/product/tasks/missing/output",
        json!({"expected_task_version": 7, "approval_id": "approval-1"}),
    )
    .await;
    assert_eq!(missing_confirmation.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        response_json(missing_confirmation).await["code"],
        "product_task_output_confirmation_required"
    );

    std::env::remove_var(PRODUCT_TASK_GATE);
}
