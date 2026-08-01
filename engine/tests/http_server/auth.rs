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
use engine::infrastructure::auth::{
    hash_api_key, APIKey, Tenant, TenantResolver, LOCAL_BOOTSTRAP_API_KEY_ID,
};
use engine::infrastructure::rate_limiter::RateLimiter;
use engine::provider::audit::{ProviderAuditEvent, PROVIDER_AUDIT_EVENT_SCHEMA_VERSION};
use engine::storage::local_product_store::LocalProductStore;
use engine::storage::local_product_store::{
    ALL_MANAGED_ACCEPTANCE_SCOPES, MANAGED_OUTPUT_OPERATOR_KEY_SCOPES, MANAGED_REVIEWER_KEY_SCOPES,
    SCOPE_IDENTITY_DELEGATE,
};
use serde_json::{json, Value};
use std::sync::Arc;
use tempfile::{tempdir, TempDir};
use tokio::sync::Mutex;
use tower::ServiceExt;

use super::common::*;

#[tokio::test]
async fn axum_local_store_persists_dispatch_history_and_dashboard_summary() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("local-team.db");

    {
        let store = LocalProductStore::new(&db_path).unwrap();
        let app = build_axum_router(AxumApiState::new().with_local_store(store));
        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/dispatch")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "raw_request": "Summarize local team status without provider calls",
                            "request_source": "dashboard"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response_json(response).await;
        assert_eq!(body["execution_result"]["executor_type"], "noop");
    }

    let store = LocalProductStore::new(&db_path).unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));
    let history = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/dispatches")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(history.status(), StatusCode::OK);
    let history_body = response_json(history).await;
    assert_eq!(history_body["dispatches"].as_array().unwrap().len(), 1);
    assert_eq!(
        history_body["dispatches"][0]["final_status"],
        "not_executed"
    );
    assert_eq!(history_body["dispatches"][0]["request_source"], "dashboard");
    assert!(
        history_body["dispatches"][0]["reserved_cost"]
            .as_f64()
            .unwrap()
            > 0.0
    );

    let dashboard = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/dashboard")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(dashboard.status(), StatusCode::OK);
    let dashboard_body = response_json(dashboard).await;
    assert_eq!(dashboard_body["counts"]["dispatches"], 1);
    assert_eq!(
        dashboard_body["dispatches"][0]["raw_request"],
        "Summarize local team status without provider calls"
    );
    assert_eq!(dashboard_body["boundaries"]["provider_transport"], "noop");

    let decisions = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/decisions")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(decisions.status(), StatusCode::OK);
    let decisions_body = response_json(decisions).await;
    let decisions = decisions_body["decisions"].as_array().unwrap();
    assert_eq!(decisions.len(), 1);
    assert_eq!(decisions[0]["run_id"], "disp-0001");
    assert_eq!(decisions[0]["action"], "dispatch");
    assert_eq!(decisions[0]["executor"], "noop");
    assert_eq!(
        decisions[0]["input_signals"]["raw_request"],
        "Summarize local team status without provider calls"
    );
}

#[tokio::test]
async fn axum_local_store_exposes_team_config_costs_and_export() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("team.db")).unwrap();
    store
        .set_config_value("workspace_name", json!("Local Ops"), "test-admin")
        .unwrap();
    store
        .upsert_team_member("user-admin", "Admin User", "admin")
        .unwrap();
    store
        .record_api_key_metadata(
            "key-admin",
            "user-admin",
            "admin",
            &["health:read".to_string(), "dispatch:read".to_string()],
            "test-admin",
        )
        .unwrap();

    let app = build_axum_router(AxumApiState::new().with_local_store(store));
    let _ = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/dispatch")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"raw_request": "Review cost ledger", "request_source": "api"})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    let config = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/config")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(config.status(), StatusCode::OK);
    let config_body = response_json(config).await;
    assert_eq!(config_body["config"]["workspace_name"], "Local Ops");
    assert_eq!(config_body["config"]["provider_transport"], "stub/off");

    let team = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/team")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(team.status(), StatusCode::OK);
    let team_body = response_json(team).await;
    assert_eq!(team_body["members"][0]["role"], "admin");
    assert_eq!(team_body["api_keys"][0]["key_id"], "key-admin");
    assert!(team_body["api_keys"][0].get("raw_key").is_none());

    let costs = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/costs")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(costs.status(), StatusCode::OK);
    let costs_body = response_json(costs).await;
    assert_eq!(costs_body["dispatch_count"], 1);
    assert!(costs_body["total_reserved_cost"].as_f64().unwrap() > 0.0);

    let export = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/export")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(export.status(), StatusCode::OK);
    let export_body = response_json(export).await;
    assert_eq!(export_body["schema_version"], "local_team_export.v1");
    assert_eq!(export_body["dispatches"].as_array().unwrap().len(), 1);
    assert_eq!(export_body["team"]["members"][0]["user_id"], "user-admin");
}

#[tokio::test]
async fn axum_auth_rejects_missing_key_when_configured() {
    let state = AxumApiState::new().with_auth(
        TenantResolver::new(),
        RateLimiter::new(60.0, 10),
        Some(60),
        1.0,
    );
    let app = build_axum_router(state);
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/config")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn axum_auth_allows_scoped_dispatch_key() {
    let mut resolver = TenantResolver::new();
    let scopes = HashSet::from(["dispatch:read".to_string(), "health:read".to_string()]);
    resolver.add_tenant(Tenant {
        tenant_id: "tenant-a".to_string(),
        name: "Tenant A".to_string(),
        scopes: scopes.clone(),
        rate_limit: Some(5),
    });
    let (_key, raw_key) = resolver
        .create_api_key("tenant-a", Some(scopes), None, 1.0)
        .unwrap();
    let app = build_axum_router(AxumApiState::new().with_auth(
        resolver,
        RateLimiter::new(60.0, 10),
        Some(60),
        1.0,
    ));

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/dispatch")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"raw_request": "Summarize docs", "request_source": "api"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn axum_rate_limit_blocks_after_tenant_limit() {
    let mut resolver = TenantResolver::new();
    let scopes = HashSet::from(["health:read".to_string()]);
    resolver.add_tenant(Tenant {
        tenant_id: "tenant-a".to_string(),
        name: "Tenant A".to_string(),
        scopes: scopes.clone(),
        rate_limit: Some(1),
    });
    let (_key, raw_key) = resolver
        .create_api_key("tenant-a", Some(scopes), None, 1.0)
        .unwrap();
    let app = build_axum_router(AxumApiState::new().with_auth(
        resolver,
        RateLimiter::new(60.0, 10),
        Some(60),
        1.0,
    ));

    let request = || {
        Request::builder()
            .method(Method::GET)
            .uri("/api/v1/health")
            .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
            .body(Body::empty())
            .unwrap()
    };

    let first = app.clone().oneshot(request()).await.unwrap();
    let second = app.oneshot(request()).await.unwrap();

    assert_eq!(first.status(), StatusCode::OK);
    assert_eq!(second.status(), StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn axum_preflight_returns_cors_headers() {
    let app = build_axum_router(AxumApiState::new());
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::OPTIONS)
                .uri("/api/v1/dispatch")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        response.headers().get(header::ACCESS_CONTROL_ALLOW_ORIGIN),
        Some(&"*".parse().unwrap())
    );
}

#[tokio::test]
async fn axum_openapi_document_lists_dispatch_endpoint() {
    let app = build_axum_router(AxumApiState::new());
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/openapi.json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body["openapi"], "3.1.0");
    assert!(body["paths"]["/api/v1/dispatch"]["post"].is_object());
    assert!(body["paths"]["/api/v1/plans"]["post"].is_object());
    assert!(body["paths"]["/api/v1/plans"]["get"].is_object());
    assert!(body["paths"]["/api/v1/plans/{plan_id}"]["get"].is_object());
    assert!(body["paths"]["/api/v1/workflow-runs"]["post"].is_object());
    assert!(body["paths"]["/api/v1/workflow-runs"]["get"].is_object());
    assert!(body["paths"]["/api/v1/workflow-runs/{run_id}"]["get"].is_object());
    assert!(body["paths"]["/api/v1/workflow-runs/{run_id}/events"]["get"].is_object());
    assert!(body["paths"]["/api/v1/workflow-runs/{run_id}/approvals"]["get"].is_object());
    assert!(body["paths"]["/api/v1/supervised-patch/workspaces"]["get"].is_object());
    assert!(body["paths"]["/api/v1/supervised-patch/workspaces/{workspace_id}"]["get"].is_object());
    assert!(body["paths"]["/api/v1/supervised-patch/artifacts"]["get"].is_object());
    assert!(body["paths"]["/api/v1/supervised-patch/artifacts/{artifact_id}"]["get"].is_object());
    assert!(body["paths"]["/api/v1/scorecards"]["get"].is_object());
    assert!(body["paths"]["/api/v1/scorecards/{artifact_id}"]["get"].is_object());
    assert!(body["paths"]["/api/v1/metrics"]["get"].is_object());
    assert!(body["paths"]["/api/v1/backups/{backup_id}/verify"]["get"].is_object());
    assert!(body["paths"]["/api/v1/backups/{backup_id}/restore/dry-run"]["post"].is_object());
    assert_eq!(
        body["paths"]["/api/v1/audit"]["get"]["parameters"][3]["name"],
        "redact"
    );
    assert!(body["paths"]["/api/v1/provider/audit"]["get"].is_object());
    assert_eq!(
        body["paths"]["/api/v1/provider/audit"]["get"]["parameters"][0]["name"],
        "limit"
    );
    assert_eq!(
        body["paths"]["/api/v1/provider/audit"]["get"]["parameters"][1]["name"],
        "offset"
    );
    assert!(body["paths"]["/api/v1/provider/endpoints"]["get"].is_object());
    assert!(body["paths"]["/api/v1/provider/endpoints"]["put"].is_object());
}

#[tokio::test]
async fn axum_create_api_key_returns_raw_key() {
    let (app, admin_key) = make_admin_app();
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/keys")
                .header(header::AUTHORIZATION, format!("Bearer {admin_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"user_id": "u1", "role": "admin", "scopes": ["dispatch:read"]})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert!(body["key_id"].as_str().unwrap().starts_with("key_"));
    assert!(body["raw_key"].as_str().unwrap().starts_with("harness_"));
    assert_eq!(body["user_id"], "u1");
    assert_eq!(body["role"], "admin");
}

#[tokio::test]
async fn axum_managed_acceptance_key_delegation_is_bootstrap_only_and_restart_reissues() {
    let dir = tempdir().unwrap();
    let store = Arc::new(LocalProductStore::new(dir.path().join("team.db")).unwrap());
    let bootstrap_raw = format!("harness_{}", "b".repeat(64));
    store
        .record_api_key_metadata_for_tenant(
            "local",
            LOCAL_BOOTSTRAP_API_KEY_ID,
            "local-admin",
            "admin",
            &[SCOPE_IDENTITY_DELEGATE.to_string()],
            "test-bootstrap",
        )
        .unwrap();

    let mut local_scopes: HashSet<String> = ["team:read", "team:admin", "dispatch:execute"]
        .into_iter()
        .map(String::from)
        .collect();
    local_scopes.extend(
        ALL_MANAGED_ACCEPTANCE_SCOPES
            .iter()
            .map(|scope| (*scope).to_string()),
    );
    local_scopes.insert(SCOPE_IDENTITY_DELEGATE.to_string());
    let mut resolver = TenantResolver::new();
    resolver.add_tenant(Tenant {
        tenant_id: "local".into(),
        name: "Local bootstrap".into(),
        scopes: local_scopes.clone(),
        rate_limit: Some(10_000),
    });
    resolver.add_tenant(Tenant {
        tenant_id: "ordinary".into(),
        name: "Ordinary tenant".into(),
        scopes: ["team:read", "team:admin"]
            .into_iter()
            .map(String::from)
            .collect(),
        rate_limit: Some(10_000),
    });
    let bootstrap_scopes: HashSet<String> = ["team:read", "team:admin", SCOPE_IDENTITY_DELEGATE]
        .into_iter()
        .map(String::from)
        .collect();
    resolver.add_api_key(APIKey {
        key_id: LOCAL_BOOTSTRAP_API_KEY_ID.into(),
        tenant_id: "local".into(),
        key_hash: hash_api_key(&bootstrap_raw, "bootstrap-test-salt"),
        key_salt: "bootstrap-test-salt".into(),
        scopes: bootstrap_scopes,
        created_at: 1.0,
        expires_at: None,
        revoked_at: None,
        last_used_at: None,
    });
    let (_ordinary_key, ordinary_raw) = resolver
        .create_api_key(
            "ordinary",
            Some(
                ["team:read", "team:admin"]
                    .into_iter()
                    .map(String::from)
                    .collect(),
            ),
            None,
            1.0,
        )
        .unwrap();

    let app = build_axum_router(
        AxumApiState::new()
            .with_local_store_arc(store.clone())
            .with_auth(resolver, RateLimiter::new(60.0, 10_000), Some(10_000), 1.0),
    );
    let reviewer = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/keys")
                .header(header::AUTHORIZATION, format!("Bearer {bootstrap_raw}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "user_id": "managed-reviewer",
                        "role": "reviewer",
                        "scopes": [
                            "managed_acceptance:risk_acknowledge",
                            "managed_acceptance:delegated_manifest_approve"
                        ]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(reviewer.status(), StatusCode::OK);
    let reviewer_body = response_json(reviewer).await;
    assert_eq!(
        reviewer_body["scopes"],
        json!([
            "managed_acceptance:risk_acknowledge",
            "managed_acceptance:delegated_manifest_approve"
        ])
    );
    let reviewer_id = reviewer_body["key_id"].as_str().unwrap();
    let reviewer_raw = reviewer_body["raw_key"].as_str().unwrap();

    // Review uses the dedicated approval capability and never receives the
    // broad tenant-admin/key-management scope.
    let reviewer_create = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/keys")
                .header(header::AUTHORIZATION, format!("Bearer {reviewer_raw}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "user_id": "forbidden-by-managed-reviewer",
                        "role": "admin",
                        "scopes": ["team:admin"]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(reviewer_create.status(), StatusCode::FORBIDDEN);

    let ordinary = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/keys")
                .header(header::AUTHORIZATION, format!("Bearer {bootstrap_raw}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "user_id": "ordinary-key-managed-by-bootstrap",
                        "role": "admin",
                        "scopes": ["team:admin"]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ordinary.status(), StatusCode::OK);
    let ordinary_id = response_json(ordinary).await["key_id"]
        .as_str()
        .unwrap()
        .to_string();
    for (method, uri, body) in [
        (
            Method::POST,
            format!("/api/v1/keys/{ordinary_id}/revoke"),
            Body::empty(),
        ),
        (
            Method::POST,
            format!("/api/v1/keys/{ordinary_id}/rotate"),
            Body::empty(),
        ),
        (
            Method::DELETE,
            format!("/api/v1/keys/{ordinary_id}"),
            Body::empty(),
        ),
        (
            Method::POST,
            format!("/api/v1/keys/{ordinary_id}/scopes"),
            Body::from(json!({"scopes": ["team:admin"]}).to_string()),
        ),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(uri)
                    .header(header::AUTHORIZATION, format!("Bearer {reviewer_raw}"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(body)
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    let operator = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/keys")
                .header(header::AUTHORIZATION, format!("Bearer {bootstrap_raw}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "user_id": "managed-output-operator",
                        "role": "output_operator",
                        "scopes": [
                            "managed_acceptance:risk_acknowledge",
                            "managed_acceptance:delegated_execute",
                            "managed_acceptance:attempt_admit"
                        ]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(operator.status(), StatusCode::OK);
    let operator_body = response_json(operator).await;
    assert_eq!(
        operator_body["scopes"],
        json!([
            "managed_acceptance:risk_acknowledge",
            "managed_acceptance:delegated_execute",
            "managed_acceptance:attempt_admit"
        ])
    );
    let operator_id = operator_body["key_id"].as_str().unwrap().to_string();

    // The canonical bootstrap owner may mutate managed identities through the
    // same API, while repeated terminal operations fail closed instead of
    // creating a second durable authority record.
    let update_operator = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/keys/{operator_id}/scopes"))
                .header(header::AUTHORIZATION, format!("Bearer {bootstrap_raw}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"scopes": MANAGED_OUTPUT_OPERATOR_KEY_SCOPES}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let update_operator_status = update_operator.status();
    let update_operator_body = response_json(update_operator).await;
    assert_eq!(
        update_operator_status,
        StatusCode::OK,
        "{update_operator_body}"
    );

    let rotated_operator = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/keys/{operator_id}/rotate"))
                .header(header::AUTHORIZATION, format!("Bearer {bootstrap_raw}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(rotated_operator.status(), StatusCode::OK);
    let rotated_operator_id = response_json(rotated_operator).await["key_id"]
        .as_str()
        .unwrap()
        .to_string();
    let revoked_operator = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/keys/{rotated_operator_id}/revoke"))
                .header(header::AUTHORIZATION, format!("Bearer {bootstrap_raw}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(revoked_operator.status(), StatusCode::OK);
    let repeated_revoke = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/keys/{rotated_operator_id}/revoke"))
                .header(header::AUTHORIZATION, format!("Bearer {bootstrap_raw}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(repeated_revoke.status(), StatusCode::NOT_FOUND);

    let disposable_reviewer = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/keys")
                .header(header::AUTHORIZATION, format!("Bearer {bootstrap_raw}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "user_id": "managed-reviewer-disposable",
                        "role": "reviewer",
                        "scopes": MANAGED_REVIEWER_KEY_SCOPES
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(disposable_reviewer.status(), StatusCode::OK);
    let disposable_reviewer_id = response_json(disposable_reviewer).await["key_id"]
        .as_str()
        .unwrap()
        .to_string();
    let deleted_reviewer = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::DELETE)
                .uri(format!("/api/v1/keys/{disposable_reviewer_id}"))
                .header(header::AUTHORIZATION, format!("Bearer {bootstrap_raw}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(deleted_reviewer.status(), StatusCode::OK);
    let repeated_delete = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::DELETE)
                .uri(format!("/api/v1/keys/{disposable_reviewer_id}"))
                .header(header::AUTHORIZATION, format!("Bearer {bootstrap_raw}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(repeated_delete.status(), StatusCode::NOT_FOUND);

    let ordinary_create = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/keys")
                .header(header::AUTHORIZATION, format!("Bearer {ordinary_raw}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "user_id": "forbidden",
                        "role": "operator",
                        "scopes": ["managed_acceptance:attempt_admit"]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ordinary_create.status(), StatusCode::FORBIDDEN);

    // A reserved bootstrap key ID in a foreign resolver tenant is not the
    // canonical local bootstrap authority and must not issue managed identities.
    let foreign_bootstrap_raw = format!("harness_{}", "d".repeat(64));
    let mut foreign_resolver = TenantResolver::new();
    foreign_resolver.add_tenant(Tenant {
        tenant_id: "foreign".into(),
        name: "Foreign tenant".into(),
        scopes: ["team:admin", SCOPE_IDENTITY_DELEGATE]
            .into_iter()
            .map(String::from)
            .collect(),
        rate_limit: Some(10_000),
    });
    foreign_resolver.add_api_key(APIKey {
        key_id: LOCAL_BOOTSTRAP_API_KEY_ID.into(),
        tenant_id: "foreign".into(),
        key_hash: hash_api_key(&foreign_bootstrap_raw, "foreign-bootstrap-salt"),
        key_salt: "foreign-bootstrap-salt".into(),
        scopes: ["team:admin", SCOPE_IDENTITY_DELEGATE]
            .into_iter()
            .map(String::from)
            .collect(),
        created_at: 1.0,
        expires_at: None,
        revoked_at: None,
        last_used_at: None,
    });
    let foreign_app = build_axum_router(
        AxumApiState::new()
            .with_local_store_arc(store.clone())
            .with_auth(
                foreign_resolver,
                RateLimiter::new(60.0, 10_000),
                Some(10_000),
                1.0,
            ),
    );
    let foreign_create = foreign_app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/keys")
                .header(
                    header::AUTHORIZATION,
                    format!("Bearer {foreign_bootstrap_raw}"),
                )
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "user_id": "foreign-reviewer",
                        "role": "reviewer",
                        "scopes": ["managed_acceptance:risk_acknowledge"]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(foreign_create.status(), StatusCode::FORBIDDEN);

    let ordinary_update = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/keys/{reviewer_id}/scopes"))
                .header(header::AUTHORIZATION, format!("Bearer {ordinary_raw}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"scopes": ["managed_acceptance:attempt_admit"]}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ordinary_update.status(), StatusCode::FORBIDDEN);

    let ordinary_rotate = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/keys/{reviewer_id}/rotate"))
                .header(header::AUTHORIZATION, format!("Bearer {ordinary_raw}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ordinary_rotate.status(), StatusCode::FORBIDDEN);

    // Simulate an engine restart: the resolver is reconstructed from the
    // bootstrap environment, while the same store is retained. Reissuance is
    // through the API owner; no key/scope row is edited directly.
    let mut restarted_resolver = TenantResolver::new();
    restarted_resolver.add_tenant(Tenant {
        tenant_id: "local".into(),
        name: "Local bootstrap".into(),
        scopes: ALL_MANAGED_ACCEPTANCE_SCOPES
            .iter()
            .map(|scope| (*scope).to_string())
            .chain([
                "team:read".into(),
                "team:admin".into(),
                SCOPE_IDENTITY_DELEGATE.into(),
            ])
            .collect(),
        rate_limit: Some(10_000),
    });
    restarted_resolver.add_api_key(APIKey {
        key_id: LOCAL_BOOTSTRAP_API_KEY_ID.into(),
        tenant_id: "local".into(),
        key_hash: hash_api_key(&bootstrap_raw, "bootstrap-test-salt"),
        key_salt: "bootstrap-test-salt".into(),
        scopes: ["team:read", "team:admin", SCOPE_IDENTITY_DELEGATE]
            .into_iter()
            .map(String::from)
            .collect(),
        created_at: 1.0,
        expires_at: None,
        revoked_at: None,
        last_used_at: None,
    });
    let restarted_app =
        build_axum_router(AxumApiState::new().with_local_store_arc(store).with_auth(
            restarted_resolver,
            RateLimiter::new(60.0, 10_000),
            Some(10_000),
            1.0,
        ));
    let reissued = restarted_app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/keys")
                .header(header::AUTHORIZATION, format!("Bearer {bootstrap_raw}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "user_id": "managed-reviewer-reissued",
                        "role": "reviewer",
                        "scopes": ["managed_acceptance:risk_acknowledge"]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(reissued.status(), StatusCode::OK);
    assert_eq!(
        response_json(reissued).await["scopes"],
        json!(["managed_acceptance:risk_acknowledge"])
    );
}

#[tokio::test]
async fn axum_list_api_keys_returns_metadata() {
    let (app, admin_key) = make_admin_app();
    let app_clone = app.clone();

    // Create two keys
    for uid in &["u1", "u2"] {
        app_clone
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/keys")
                    .header(header::AUTHORIZATION, format!("Bearer {admin_key}"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({"user_id": uid, "role": "readonly", "scopes": ["dispatch:read"]})
                            .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
    }

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/keys")
                .header(header::AUTHORIZATION, format!("Bearer {admin_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    let keys = body["keys"].as_array().unwrap();
    assert!(keys.len() >= 2);
    // Keys should have metadata fields but no raw_key
    for key in keys {
        assert!(key["key_id"].as_str().is_some());
        assert!(key["user_id"].as_str().is_some());
        assert!(key["role"].as_str().is_some());
        assert!(key["scopes"].as_array().is_some());
        assert!(key["created_at"].as_str().is_some());
        assert!(
            key.get("raw_key").is_none(),
            "list must not return raw keys"
        );
    }
}

#[tokio::test]
async fn axum_list_api_keys_requires_team_read_scope() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("team.db")).unwrap();
    let mut resolver = TenantResolver::new();
    let mut readonly_scopes = HashSet::new();
    readonly_scopes.insert("dispatch:read".to_string());
    resolver.add_tenant(Tenant {
        tenant_id: "local".to_string(),
        name: "Local".to_string(),
        scopes: readonly_scopes.clone(),
        rate_limit: Some(10_000),
    });
    let (_key, raw_key) = resolver
        .create_api_key("local", Some(readonly_scopes), None, 1.0)
        .unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store).with_auth(
        resolver,
        RateLimiter::new(60.0, 10_000),
        Some(10_000),
        1.0,
    ));

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/keys")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn axum_revoke_api_key_blocks_future_auth() {
    let (app, admin_key) = make_admin_app();
    let app_clone = app.clone();

    let create_resp = app_clone
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/keys")
                .header(header::AUTHORIZATION, format!("Bearer {admin_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"user_id": "u1", "role": "admin", "scopes": ["dispatch:read", "team:read"]}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let create_body = response_json(create_resp).await;
    let new_key = create_body["raw_key"].as_str().unwrap().to_string();
    let key_id = create_body["key_id"].as_str().unwrap().to_string();

    let app_clone = app.clone();
    let use_resp = app_clone
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/team")
                .header(header::AUTHORIZATION, format!("Bearer {new_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(use_resp.status(), StatusCode::OK);

    let app_clone = app.clone();
    let revoke_resp = app_clone
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/keys/{key_id}/revoke"))
                .header(header::AUTHORIZATION, format!("Bearer {admin_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(revoke_resp.status(), StatusCode::OK);

    let app_clone = app.clone();
    let blocked_resp = app_clone
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/team")
                .header(header::AUTHORIZATION, format!("Bearer {new_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(blocked_resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn axum_rotate_api_key_creates_new_revokes_old() {
    let (app, admin_key) = make_admin_app();
    let app_clone = app.clone();

    let create_resp = app_clone
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/keys")
                .header(header::AUTHORIZATION, format!("Bearer {admin_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"user_id": "u1", "role": "admin", "scopes": ["dispatch:read", "team:read", "team:admin"]}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let create_body = response_json(create_resp).await;
    let old_key = create_body["raw_key"].as_str().unwrap().to_string();
    let key_id = create_body["key_id"].as_str().unwrap().to_string();

    let app_clone = app.clone();
    let rotate_resp = app_clone
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/keys/{key_id}/rotate"))
                .header(header::AUTHORIZATION, format!("Bearer {admin_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(rotate_resp.status(), StatusCode::OK);
    let rotate_body = response_json(rotate_resp).await;
    let new_key = rotate_body["raw_key"].as_str().unwrap().to_string();
    assert_ne!(old_key, new_key);
    assert_eq!(rotate_body["rotated_from"], key_id);

    let app_clone = app.clone();
    let old_resp = app_clone
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/team")
                .header(header::AUTHORIZATION, format!("Bearer {old_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(old_resp.status(), StatusCode::UNAUTHORIZED);

    let app_clone = app.clone();
    let new_resp = app_clone
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/team")
                .header(header::AUTHORIZATION, format!("Bearer {new_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(new_resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn axum_delete_api_key_removes_metadata() {
    let (app, admin_key) = make_admin_app();
    let app_clone = app.clone();

    let create_resp = app_clone
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/keys")
                .header(header::AUTHORIZATION, format!("Bearer {admin_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"user_id": "u1", "role": "admin", "scopes": ["dispatch:read"]})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let create_body = response_json(create_resp).await;
    let key_id = create_body["key_id"].as_str().unwrap().to_string();

    let app_clone = app.clone();
    let del_resp = app_clone
        .oneshot(
            Request::builder()
                .method(Method::DELETE)
                .uri(format!("/api/v1/keys/{key_id}"))
                .header(header::AUTHORIZATION, format!("Bearer {admin_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(del_resp.status(), StatusCode::OK);

    let app_clone = app.clone();
    let team_resp = app_clone
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/team")
                .header(header::AUTHORIZATION, format!("Bearer {admin_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let team_body = response_json(team_resp).await;
    let keys = team_body["api_keys"].as_array().unwrap();
    assert!(!keys.iter().any(|k| k["key_id"] == key_id));
}

#[tokio::test]
async fn axum_create_team_member_adds_to_snapshot() {
    let (app, admin_key) = make_admin_app();

    let create_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/team")
                .header(header::AUTHORIZATION, format!("Bearer {admin_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"user_id": "alice", "display_name": "Alice", "role": "admin"})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create_resp.status(), StatusCode::OK);

    let team_resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/team")
                .header(header::AUTHORIZATION, format!("Bearer {admin_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let team_body = response_json(team_resp).await;
    let members = team_body["members"].as_array().unwrap();
    assert!(members.iter().any(|m| m["user_id"] == "alice"));
}

#[tokio::test]
async fn axum_update_member_role_changes_role() {
    let (app, admin_key) = make_admin_app();

    app.clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/team")
                .header(header::AUTHORIZATION, format!("Bearer {admin_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"user_id": "bob", "display_name": "Bob", "role": "readonly"})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    let update_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::PUT)
                .uri("/api/v1/team/bob")
                .header(header::AUTHORIZATION, format!("Bearer {admin_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"role": "admin"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(update_resp.status(), StatusCode::OK);

    let team_resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/team")
                .header(header::AUTHORIZATION, format!("Bearer {admin_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let team_body = response_json(team_resp).await;
    let members = team_body["members"].as_array().unwrap();
    let bob = members.iter().find(|m| m["user_id"] == "bob").unwrap();
    assert_eq!(bob["role"], "admin");
}

#[tokio::test]
async fn axum_delete_member_removes_from_snapshot() {
    let (app, admin_key) = make_admin_app();

    app.clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/team")
                .header(header::AUTHORIZATION, format!("Bearer {admin_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"user_id": "carol", "display_name": "Carol", "role": "admin"})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    let del_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::DELETE)
                .uri("/api/v1/team/carol")
                .header(header::AUTHORIZATION, format!("Bearer {admin_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(del_resp.status(), StatusCode::OK);

    let team_resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/team")
                .header(header::AUTHORIZATION, format!("Bearer {admin_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let team_body = response_json(team_resp).await;
    let members = team_body["members"].as_array().unwrap();
    assert!(!members.iter().any(|m| m["user_id"] == "carol"));
}

#[tokio::test]
async fn axum_team_mutation_requires_admin_scope() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("team.db")).unwrap();
    let mut resolver = TenantResolver::new();
    let mut readonly_scopes = HashSet::new();
    readonly_scopes.insert("team:read".to_string());
    resolver.add_tenant(Tenant {
        tenant_id: "local".to_string(),
        name: "Local".to_string(),
        scopes: readonly_scopes.clone(),
        rate_limit: Some(10_000),
    });
    let (_key, raw_key) = resolver
        .create_api_key("local", Some(readonly_scopes), None, 1.0)
        .unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store).with_auth(
        resolver,
        RateLimiter::new(60.0, 10_000),
        Some(10_000),
        1.0,
    ));

    let create_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/keys")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"user_id": "u1", "role": "admin", "scopes": ["dispatch:read"]})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create_resp.status(), StatusCode::FORBIDDEN);

    let member_resp = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/team")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"user_id": "u1", "display_name": "U1", "role": "admin"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(member_resp.status(), StatusCode::FORBIDDEN);
}
