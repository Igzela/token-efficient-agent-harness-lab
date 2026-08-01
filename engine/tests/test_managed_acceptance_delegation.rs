use std::collections::HashSet;
use std::sync::Arc;

use axum::body::{to_bytes, Body};
use axum::http::{header, Method, Request, StatusCode};
use engine::http_server::{build_axum_router, AxumApiState};
use engine::infrastructure::auth::{
    hash_api_key, APIKey, Tenant, TenantResolver, LOCAL_BOOTSTRAP_API_KEY_ID,
};
use engine::infrastructure::rate_limiter::RateLimiter;
use engine::storage::local_product_store::{
    LocalProductStore, ALL_MANAGED_ACCEPTANCE_SCOPES, MANAGED_OUTPUT_OPERATOR_KEY_SCOPES,
    MANAGED_REVIEWER_KEY_SCOPES, SCOPE_IDENTITY_DELEGATE,
};
use serde_json::{json, Value};
use tempfile::tempdir;
use tower::ServiceExt;

async fn response_json(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), 1_048_576).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

fn bootstrap_resolver(bootstrap_raw: &str) -> TenantResolver {
    let mut tenant_scopes: HashSet<String> = [
        "team:read",
        "team:admin",
        "dispatch:execute",
        SCOPE_IDENTITY_DELEGATE,
    ]
    .into_iter()
    .map(String::from)
    .collect();
    tenant_scopes.extend(
        ALL_MANAGED_ACCEPTANCE_SCOPES
            .iter()
            .map(|scope| (*scope).to_string()),
    );
    let mut resolver = TenantResolver::new();
    resolver.add_tenant(Tenant {
        tenant_id: "local".into(),
        name: "Local bootstrap".into(),
        scopes: tenant_scopes,
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
    resolver.add_api_key(APIKey {
        key_id: LOCAL_BOOTSTRAP_API_KEY_ID.into(),
        tenant_id: "local".into(),
        key_hash: hash_api_key(bootstrap_raw, "bootstrap-test-salt"),
        key_salt: "bootstrap-test-salt".into(),
        scopes: [
            "team:read".to_string(),
            "team:admin".to_string(),
            SCOPE_IDENTITY_DELEGATE.to_string(),
        ]
        .into_iter()
        .collect(),
        created_at: 1.0,
        expires_at: None,
        revoked_at: None,
        last_used_at: None,
    });
    let ordinary_raw = format!("harness_{}", "c".repeat(64));
    resolver.add_api_key(APIKey {
        key_id: "ordinary-test-key".into(),
        tenant_id: "ordinary".into(),
        key_hash: hash_api_key(&ordinary_raw, "ordinary-test-salt"),
        key_salt: "ordinary-test-salt".into(),
        scopes: ["team:read".to_string(), "team:admin".to_string()]
            .into_iter()
            .collect(),
        created_at: 1.0,
        expires_at: None,
        revoked_at: None,
        last_used_at: None,
    });
    resolver
}

fn bootstrap_app(store: Arc<LocalProductStore>, bootstrap_raw: &str) -> axum::Router {
    store
        .upsert_team_member("local-admin", "Local Admin", "admin")
        .unwrap();
    store
        .record_api_key_metadata_for_tenant(
            "local",
            LOCAL_BOOTSTRAP_API_KEY_ID,
            "local-admin",
            "admin",
            &[
                "team:read".to_string(),
                "team:admin".to_string(),
                SCOPE_IDENTITY_DELEGATE.to_string(),
            ],
            "bootstrap-test",
        )
        .unwrap();
    build_axum_router(AxumApiState::new().with_local_store_arc(store).with_auth(
        bootstrap_resolver(bootstrap_raw),
        RateLimiter::new(60.0, 10_000),
        Some(10_000),
        1.0,
    ))
}

fn auth_header(raw: &str) -> String {
    format!("Bearer {raw}")
}

#[tokio::test]
async fn bootstrap_only_delegates_minimal_managed_identities_and_reissues_after_restart() {
    let dir = tempdir().unwrap();
    let store = Arc::new(LocalProductStore::new(dir.path().join("team.db")).unwrap());
    let bootstrap_raw = format!("harness_{}", "b".repeat(64));
    let app = bootstrap_app(store.clone(), &bootstrap_raw);

    let reviewer = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/keys")
                .header(header::AUTHORIZATION, auth_header(&bootstrap_raw))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "user_id": "managed-reviewer",
                        "role": "reviewer",
                        "scopes": MANAGED_REVIEWER_KEY_SCOPES
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(reviewer.status(), StatusCode::OK);
    let reviewer = response_json(reviewer).await;
    assert_eq!(reviewer["scopes"], json!(MANAGED_REVIEWER_KEY_SCOPES));
    let reviewer_id = reviewer["key_id"].as_str().unwrap();

    let operator = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/keys")
                .header(header::AUTHORIZATION, auth_header(&bootstrap_raw))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "user_id": "managed-output-operator",
                        "role": "output_operator",
                        "scopes": MANAGED_OUTPUT_OPERATOR_KEY_SCOPES
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(operator.status(), StatusCode::OK);
    assert_eq!(
        response_json(operator).await["scopes"],
        json!(MANAGED_OUTPUT_OPERATOR_KEY_SCOPES)
    );

    let ordinary_raw = format!("harness_{}", "c".repeat(64));
    let ordinary_create = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/keys")
                .header(header::AUTHORIZATION, auth_header(&ordinary_raw))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "user_id": "forbidden",
                        "role": "output_operator",
                        "scopes": ["managed_acceptance:attempt_admit"]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ordinary_create.status(), StatusCode::FORBIDDEN);

    let ordinary_role_without_managed_scope = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/keys")
                .header(header::AUTHORIZATION, auth_header(&ordinary_raw))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "user_id": "forbidden-role-only",
                        "role": "output_operator",
                        "scopes": ["team:admin"]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        ordinary_role_without_managed_scope.status(),
        StatusCode::FORBIDDEN
    );

    let bootstrap_unapproved_role_scope = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/keys")
                .header(header::AUTHORIZATION, auth_header(&bootstrap_raw))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "user_id": "forbidden-unapproved-role-scope",
                        "role": "output_operator",
                        "scopes": ["team:admin"]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        bootstrap_unapproved_role_scope.status(),
        StatusCode::BAD_REQUEST
    );

    let bootstrap_delegate = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/keys")
                .header(header::AUTHORIZATION, auth_header(&bootstrap_raw))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "user_id": "forbidden",
                        "role": "operator",
                        "scopes": [SCOPE_IDENTITY_DELEGATE]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(bootstrap_delegate.status(), StatusCode::FORBIDDEN);

    let unknown_scope = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/keys")
                .header(header::AUTHORIZATION, auth_header(&bootstrap_raw))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "user_id": "forbidden-unknown-scope",
                        "role": "operator",
                        "scopes": ["managed_acceptance:not-a-canonical-scope"]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unknown_scope.status(), StatusCode::BAD_REQUEST);

    let wrong_role_scope = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/keys")
                .header(header::AUTHORIZATION, auth_header(&bootstrap_raw))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "user_id": "forbidden-role-scope",
                        "role": "reviewer",
                        "scopes": [
                            "team:admin",
                            "managed_acceptance:risk_acknowledge",
                            "managed_acceptance:delegated_execute"
                        ]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(wrong_role_scope.status(), StatusCode::BAD_REQUEST);

    for endpoint in [
        format!("/api/v1/keys/{reviewer_id}/scopes"),
        format!("/api/v1/keys/{reviewer_id}/rotate"),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(endpoint)
                    .header(header::AUTHORIZATION, auth_header(&ordinary_raw))
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({"scopes": ["managed_acceptance:attempt_admit"]}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    // A tenant admin must not be able to rewrite a managed identity into an
    // ordinary scope profile by naming its key id. The bootstrap authority is
    // the only owner of managed identity mutation, even when the requested
    // replacement scopes contain no managed capability.
    let ordinary_rewrite = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/keys/{reviewer_id}/scopes"))
                .header(header::AUTHORIZATION, auth_header(&ordinary_raw))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"scopes": ["team:admin"]}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ordinary_rewrite.status(), StatusCode::FORBIDDEN);

    // Restart keeps only the parent bootstrap credential in the resolver. The
    // child identity is reissued through the canonical API, not SQL mutation.
    let reissued = bootstrap_app(store.clone(), &bootstrap_raw)
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/keys")
                .header(header::AUTHORIZATION, auth_header(&bootstrap_raw))
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
    let reissued = response_json(reissued).await;
    assert_eq!(
        reissued["scopes"],
        json!(["managed_acceptance:risk_acknowledge"])
    );
    assert!(store
        .get_api_key_metadata(reissued["key_id"].as_str().unwrap())
        .unwrap()
        .is_some());
}
