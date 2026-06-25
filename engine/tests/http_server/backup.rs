use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use axum::body::{to_bytes, Body};
use axum::http::{header, Method, Request, StatusCode};
use engine::http_server::{
    build_axum_router, build_axum_router_with_dashboard, AxumApiState, CliCapability,
};
use engine::infrastructure::auth::{Tenant, TenantResolver};
use engine::infrastructure::rate_limiter::RateLimiter;
use engine::provider::audit::{ProviderAuditEvent, PROVIDER_AUDIT_EVENT_SCHEMA_VERSION};
use engine::storage::local_product_store::LocalProductStore;
use serde_json::{json, Value};
use tempfile::{tempdir, TempDir};
use tokio::sync::Mutex;
use tower::ServiceExt;

use super::common::*;

#[tokio::test]
async fn axum_admin_backup_requires_scope_confirmation_and_writes_audit_log() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("team.db")).unwrap();

    let mut resolver = TenantResolver::new();
    let admin_scopes = HashSet::from([
        "backup:admin".to_string(),
        "audit:read".to_string(),
        "health:read".to_string(),
    ]);
    let readonly_scopes = HashSet::from(["health:read".to_string()]);
    resolver.add_tenant(Tenant {
        tenant_id: "local-team".to_string(),
        name: "Local Team".to_string(),
        scopes: admin_scopes
            .union(&readonly_scopes)
            .cloned()
            .collect::<HashSet<String>>(),
        rate_limit: Some(100),
    });
    let (_readonly_key, readonly_raw) = resolver
        .create_api_key("local-team", Some(readonly_scopes), None, 1.0)
        .unwrap();
    let (_admin_key, admin_raw) = resolver
        .create_api_key("local-team", Some(admin_scopes), None, 1.0)
        .unwrap();

    let app = build_axum_router(
        AxumApiState::new()
            .with_local_store(store)
            .with_backup_dir(dir.path().join("backups"))
            .with_auth(resolver, RateLimiter::new(60.0, 100), Some(100), 1.0),
    );

    let readonly = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/backups")
                .header(header::AUTHORIZATION, format!("Bearer {readonly_raw}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"label": "nightly", "confirm_local_backup": true}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(readonly.status(), StatusCode::FORBIDDEN);

    let missing_confirmation = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/backups")
                .header(header::AUTHORIZATION, format!("Bearer {admin_raw}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"label": "nightly"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(missing_confirmation.status(), StatusCode::BAD_REQUEST);

    let backup = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/backups")
                .header(header::AUTHORIZATION, format!("Bearer {admin_raw}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"label": "nightly", "confirm_local_backup": true}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(backup.status(), StatusCode::OK);
    let backup_body = response_json(backup).await;
    assert_eq!(backup_body["backup"]["label"], "nightly");

    let audit = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/audit")
                .header(header::AUTHORIZATION, format!("Bearer {admin_raw}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(audit.status(), StatusCode::OK);
    let audit_body = response_json(audit).await;
    assert_eq!(audit_body["events"][0]["action"], "backup.create");
}

#[tokio::test]
async fn axum_backup_requires_auth_boundary_even_with_confirmation() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("team.db")).unwrap();
    let app = build_axum_router(
        AxumApiState::new()
            .with_local_store(store)
            .with_backup_dir(dir.path().join("backups")),
    );

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/backups")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"label": "manual", "confirm_local_backup": true}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body = response_json(response).await;
    assert_eq!(body["code"], "backup_admin_required");
    assert_eq!(body["error"], "admin auth is required for local backup");
}

#[tokio::test]
async fn axum_backup_verify_and_restore_dry_run_are_non_destructive() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("team.db");
    let store = LocalProductStore::new(&db_path).unwrap();
    store
        .record_dispatch(
            "Backup verification row",
            "api",
            &json!({
                "record": {"dispatch_id": "disp-backup-verify", "final_status": "not_executed"},
                "decision": {"selected_tier": "cheap_executor", "budget_reservation": {"reserved_cost": 0.1}},
                "analysis": {"risk_level": "low"},
                "execution_result": {"executor_type": "noop"},
            }),
            "test",
        )
        .unwrap();

    let mut resolver = TenantResolver::new();
    let admin_scopes = HashSet::from([
        "backup:admin".to_string(),
        "audit:read".to_string(),
        "health:read".to_string(),
    ]);
    resolver.add_tenant(Tenant {
        tenant_id: "local-team".to_string(),
        name: "Local Team".to_string(),
        scopes: admin_scopes.clone(),
        rate_limit: Some(100),
    });
    let (_key, raw_key) = resolver
        .create_api_key("local-team", Some(admin_scopes), None, 1.0)
        .unwrap();

    let app = build_axum_router(
        AxumApiState::new()
            .with_local_store(store)
            .with_backup_dir(dir.path().join("backups"))
            .with_auth(resolver, RateLimiter::new(60.0, 100), Some(100), 1.0),
    );

    let create = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/backups")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"label": "verify", "confirm_local_backup": true}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::OK);
    let create_body = response_json(create).await;
    let backup_id = create_body["backup"]["backup_id"].as_str().unwrap();

    let verify = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/v1/backups/{backup_id}/verify"))
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(verify.status(), StatusCode::OK);
    let verify_body = response_json(verify).await;
    assert_eq!(verify_body["verification"]["success"], true);
    assert_eq!(verify_body["verification"]["checksum_ok"], true);
    assert_eq!(verify_body["verification"]["integrity_ok"], true);
    assert_eq!(verify_body["verification"]["dry_run"], false);
    assert!(
        verify_body["verification"]["records_checked"]
            .as_i64()
            .unwrap()
            > 0
    );

    let dry_run = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/backups/{backup_id}/restore/dry-run"))
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"confirm_restore_dry_run": true}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(dry_run.status(), StatusCode::OK);
    let dry_run_body = response_json(dry_run).await;
    assert_eq!(dry_run_body["restore_dry_run"]["success"], true);
    assert_eq!(dry_run_body["restore_dry_run"]["dry_run"], true);
    assert_eq!(
        dry_run_body["restore_dry_run"]["restore_would_overwrite"],
        true
    );

    let integrity = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/storage/integrity")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(integrity.status(), StatusCode::OK);
    let integrity_body = response_json(integrity).await;
    assert_eq!(integrity_body["integrity"]["status"], "ok");
}

#[tokio::test]
async fn axum_list_backups_requires_admin_scope() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("team.db")).unwrap();

    let mut resolver = TenantResolver::new();
    let readonly_scopes = HashSet::from(["health:read".to_string()]);
    resolver.add_tenant(Tenant {
        tenant_id: "local-team".to_string(),
        name: "Local Team".to_string(),
        scopes: readonly_scopes.clone(),
        rate_limit: Some(100),
    });
    let (_key, raw_key) = resolver
        .create_api_key("local-team", Some(readonly_scopes), None, 1.0)
        .unwrap();

    let app = build_axum_router(
        AxumApiState::new()
            .with_local_store(store)
            .with_backup_dir(dir.path().join("backups"))
            .with_auth(resolver, RateLimiter::new(60.0, 100), Some(100), 1.0),
    );

    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/backups")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn axum_list_backups_returns_empty_when_no_backups() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("team.db")).unwrap();

    let mut resolver = TenantResolver::new();
    let admin_scopes = HashSet::from(["backup:admin".to_string()]);
    resolver.add_tenant(Tenant {
        tenant_id: "local-team".to_string(),
        name: "Local Team".to_string(),
        scopes: admin_scopes.clone(),
        rate_limit: Some(100),
    });
    let (_key, raw_key) = resolver
        .create_api_key("local-team", Some(admin_scopes), None, 1.0)
        .unwrap();

    let app = build_axum_router(
        AxumApiState::new()
            .with_local_store(store)
            .with_backup_dir(dir.path().join("backups"))
            .with_auth(resolver, RateLimiter::new(60.0, 100), Some(100), 1.0),
    );

    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/backups")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = response_json(resp).await;
    assert_eq!(body["backups"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn axum_list_backups_after_create() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("team.db")).unwrap();

    let mut resolver = TenantResolver::new();
    let admin_scopes = HashSet::from(["backup:admin".to_string()]);
    resolver.add_tenant(Tenant {
        tenant_id: "local-team".to_string(),
        name: "Local Team".to_string(),
        scopes: admin_scopes.clone(),
        rate_limit: Some(100),
    });
    let (_key, raw_key) = resolver
        .create_api_key("local-team", Some(admin_scopes), None, 1.0)
        .unwrap();

    let app = build_axum_router(
        AxumApiState::new()
            .with_local_store(store)
            .with_backup_dir(dir.path().join("backups"))
            .with_auth(resolver, RateLimiter::new(60.0, 100), Some(100), 1.0),
    );

    // Create a backup
    let create_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/backups")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"label": "test", "confirm_local_backup": true}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create_resp.status(), StatusCode::OK);

    // List backups
    let list_resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/backups")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(list_resp.status(), StatusCode::OK);
    let body = response_json(list_resp).await;
    assert_eq!(body["backups"].as_array().unwrap().len(), 1);
    assert_eq!(body["backups"][0]["label"], "test");
}

#[tokio::test]
async fn axum_delete_backup_requires_admin_scope() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("team.db")).unwrap();

    let mut resolver = TenantResolver::new();
    let readonly_scopes = HashSet::from(["health:read".to_string()]);
    resolver.add_tenant(Tenant {
        tenant_id: "local-team".to_string(),
        name: "Local Team".to_string(),
        scopes: readonly_scopes.clone(),
        rate_limit: Some(100),
    });
    let (_key, raw_key) = resolver
        .create_api_key("local-team", Some(readonly_scopes), None, 1.0)
        .unwrap();

    let app = build_axum_router(
        AxumApiState::new()
            .with_local_store(store)
            .with_backup_dir(dir.path().join("backups"))
            .with_auth(resolver, RateLimiter::new(60.0, 100), Some(100), 1.0),
    );

    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::DELETE)
                .uri("/api/v1/backups/backup-0001")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn axum_delete_backup_404_for_missing() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("team.db")).unwrap();

    let mut resolver = TenantResolver::new();
    let admin_scopes = HashSet::from(["backup:admin".to_string(), "audit:read".to_string()]);
    resolver.add_tenant(Tenant {
        tenant_id: "local-team".to_string(),
        name: "Local Team".to_string(),
        scopes: admin_scopes.clone(),
        rate_limit: Some(100),
    });
    let (_key, raw_key) = resolver
        .create_api_key("local-team", Some(admin_scopes), None, 1.0)
        .unwrap();

    let app = build_axum_router(
        AxumApiState::new()
            .with_local_store(store)
            .with_backup_dir(dir.path().join("backups"))
            .with_auth(resolver, RateLimiter::new(60.0, 100), Some(100), 1.0),
    );

    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::DELETE)
                .uri("/api/v1/backups/nonexistent-backup")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn axum_delete_backup_removes_backup() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("team.db")).unwrap();

    let mut resolver = TenantResolver::new();
    let admin_scopes = HashSet::from([
        "backup:admin".to_string(),
        "audit:read".to_string(),
        "health:read".to_string(),
    ]);
    resolver.add_tenant(Tenant {
        tenant_id: "local-team".to_string(),
        name: "Local Team".to_string(),
        scopes: admin_scopes.clone(),
        rate_limit: Some(100),
    });
    let (_key, raw_key) = resolver
        .create_api_key("local-team", Some(admin_scopes), None, 1.0)
        .unwrap();

    let app = build_axum_router(
        AxumApiState::new()
            .with_local_store(store)
            .with_backup_dir(dir.path().join("backups"))
            .with_auth(resolver, RateLimiter::new(60.0, 100), Some(100), 1.0),
    );

    // Create a backup
    let create_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/backups")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"label": "to-delete", "confirm_local_backup": true}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create_resp.status(), StatusCode::OK);
    let create_body = response_json(create_resp).await;
    let backup_id = create_body["backup"]["backup_id"]
        .as_str()
        .unwrap()
        .to_string();

    // Delete it
    let delete_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::DELETE)
                .uri(format!("/api/v1/backups/{backup_id}"))
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(delete_resp.status(), StatusCode::OK);
    let delete_body = response_json(delete_resp).await;
    assert_eq!(delete_body["ok"], true);

    // Verify list is empty
    let list_resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/backups")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(list_resp.status(), StatusCode::OK);
    let list_body = response_json(list_resp).await;
    assert_eq!(list_body["backups"].as_array().unwrap().len(), 0);
}

