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
async fn axum_create_backup_uses_real_timestamp() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("team.db")).unwrap();

    let mut resolver = TenantResolver::new();
    let admin_scopes = HashSet::from(["backup:admin".to_string(), "health:read".to_string()]);
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
                    json!({"label": "ts-check", "confirm_local_backup": true}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create_resp.status(), StatusCode::OK);

    // List and verify timestamp is not the old hardcoded value
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
    let body = response_json(list_resp).await;
    let created_at = body["backups"][0]["created_at"].as_str().unwrap();
    assert_ne!(
        created_at, "2026-05-29T00:00:00Z",
        "backup timestamp should not be hardcoded"
    );
    assert!(
        created_at.starts_with("2026-") || created_at.starts_with("2027-"),
        "backup timestamp should be a recent ISO date, got: {created_at}"
    );
}

#[tokio::test]
async fn test_auto_adjustment_apply_env_gates_and_confirmation() {
    let _env_lock = auto_adjustment_env_lock().lock().await;
    let _cleanup = AutoAdjustmentEnvCleanup;
    std::env::remove_var("ACP_ENABLE_AUTO_ADJUSTMENT");
    std::env::remove_var("ACP_AUTO_ADJUSTMENT_DRY_RUN");
    std::env::remove_var("ACP_AUTO_ADJUSTMENT_ACTIVE");

    let GeneratedAppFixture { app, key, .. } = make_generated_fixture();
    let candidate_id = generated_high_cost_candidate_id(&app, &key).await;

    let missing_confirm = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/auto-adjustments/apply")
                .header(header::AUTHORIZATION, format!("Bearer {key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(missing_confirm.status(), StatusCode::BAD_REQUEST);

    let default_blocked = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/auto-adjustments/apply")
                .header(header::AUTHORIZATION, format!("Bearer {key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"confirm_auto_adjustment": true, "candidate_id": candidate_id.clone()})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(default_blocked.status(), StatusCode::BAD_REQUEST);

    std::env::set_var("ACP_ENABLE_AUTO_ADJUSTMENT", "1");
    let only_enable_blocked = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/auto-adjustments/apply")
                .header(header::AUTHORIZATION, format!("Bearer {key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"confirm_auto_adjustment": true, "candidate_id": candidate_id.clone()})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(only_enable_blocked.status(), StatusCode::BAD_REQUEST);

    std::env::set_var("ACP_AUTO_ADJUSTMENT_ACTIVE", "1");
    std::env::set_var("ACP_AUTO_ADJUSTMENT_DRY_RUN", "1");
    let dry_run_blocks_apply = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/auto-adjustments/apply")
                .header(header::AUTHORIZATION, format!("Bearer {key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"confirm_auto_adjustment": true, "candidate_id": candidate_id.clone()})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(dry_run_blocks_apply.status(), StatusCode::BAD_REQUEST);

    std::env::remove_var("ACP_AUTO_ADJUSTMENT_DRY_RUN");
    let active_apply = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/auto-adjustments/apply")
                .header(header::AUTHORIZATION, format!("Bearer {key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"confirm_auto_adjustment": true, "candidate_id": candidate_id.clone()})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(active_apply.status(), StatusCode::OK);
    let body = response_json(active_apply).await;
    assert_eq!(body["applied"], true);
    assert_eq!(body["status"], "active");
    assert!(body["adjustment_id"]
        .as_str()
        .unwrap()
        .starts_with("auto-adjustment-"));
    assert!(body["snapshot_id"]
        .as_str()
        .unwrap()
        .starts_with("policy-snapshot-"));

    let active = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/proposals?status=active&limit=500")
                .header(header::AUTHORIZATION, format!("Bearer {key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let active_body = response_json(active).await;
    assert_eq!(active_body["proposals"].as_array().unwrap().len(), 1);

    let report = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/auto-adjustments?limit=50")
                .header(header::AUTHORIZATION, format!("Bearer {key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let report_body = response_json(report).await;
    assert_eq!(report_body["mode"], "active");
    assert_eq!(report_body["active_apply_available"], true);
    assert_eq!(
        report_body["active_auto_adjustments"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    let audit = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/audit?limit=100")
                .header(header::AUTHORIZATION, format!("Bearer {key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let audit_body = response_json(audit).await;
    let actions: Vec<_> = audit_body["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|event| event["action"].as_str())
        .collect();
    assert!(actions.contains(&"auto_adjustment.snapshot.created"));
    assert!(actions.contains(&"auto_adjustment.apply.accepted"));
}

#[tokio::test]
async fn test_auto_adjustment_requires_team_admin() {
    let _env_lock = auto_adjustment_env_lock().lock().await;
    let _cleanup = AutoAdjustmentEnvCleanup;
    std::env::set_var("ACP_ENABLE_AUTO_ADJUSTMENT", "1");
    std::env::set_var("ACP_AUTO_ADJUSTMENT_ACTIVE", "1");
    std::env::remove_var("ACP_AUTO_ADJUSTMENT_DRY_RUN");

    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("readonly-auto.db")).unwrap();
    let mut resolver = TenantResolver::new();
    let scopes = HashSet::from(["dispatch:read".to_string()]);
    resolver.add_tenant(Tenant {
        tenant_id: "local".to_string(),
        name: "Local".to_string(),
        scopes: scopes.clone(),
        rate_limit: Some(10_000),
    });
    let (_key, raw_key) = resolver
        .create_api_key("local", Some(scopes), None, 1.0)
        .unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store).with_auth(
        resolver,
        RateLimiter::new(60.0, 10_000),
        Some(10_000),
        1.0,
    ));

    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/auto-adjustments/apply")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"confirm_auto_adjustment": true}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_auto_adjustment_reentry_blocks_same_policy_key_and_allows_different_key() {
    let _env_lock = auto_adjustment_env_lock().lock().await;
    let _cleanup = AutoAdjustmentEnvCleanup;
    std::env::set_var("ACP_ENABLE_AUTO_ADJUSTMENT", "1");
    std::env::set_var("ACP_AUTO_ADJUSTMENT_ACTIVE", "1");
    std::env::remove_var("ACP_AUTO_ADJUSTMENT_DRY_RUN");

    let GeneratedAppFixture {
        app,
        key,
        db_path,
        _dir,
        ..
    } = make_generated_fixture();
    let debug_candidate_id = generated_high_cost_candidate_id(&app, &key).await;
    let generate_candidate_id =
        generated_candidate_id_for_task(&app, &key, "code", "generate").await;

    let first = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/auto-adjustments/apply")
                .header(header::AUTHORIZATION, format!("Bearer {key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"confirm_auto_adjustment": true, "candidate_id": debug_candidate_id})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::OK);
    let first_body = response_json(first).await;
    assert_eq!(first_body["applied"], true);

    let duplicate = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/auto-adjustments/apply")
                .header(header::AUTHORIZATION, format!("Bearer {key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "confirm_auto_adjustment": true,
                        "candidate_id": first_body["candidate_id"].as_str().unwrap(),
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(duplicate.status(), StatusCode::OK);
    let duplicate_body = response_json(duplicate).await;
    assert_eq!(duplicate_body["applied"], false);
    assert!(duplicate_body["blocked_reasons"]
        .as_array()
        .unwrap()
        .iter()
        .any(|reason| reason.as_str().unwrap_or("").contains("already active")));

    let conn = rusqlite::Connection::open(&db_path).unwrap();
    conn.execute(
        "UPDATE controlled_loop_policy_snapshots
         SET candidate_id = 'other-candidate',
             snapshot_json = json_set(snapshot_json, '$.candidate_id', 'other-candidate')
         WHERE adjustment_id = ?1",
        [first_body["adjustment_id"].as_str().unwrap()],
    )
    .unwrap();
    let same_key_different_candidate = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/auto-adjustments/apply")
                .header(header::AUTHORIZATION, format!("Bearer {key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "confirm_auto_adjustment": true,
                        "candidate_id": first_body["candidate_id"].as_str().unwrap(),
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(same_key_different_candidate.status(), StatusCode::OK);
    let same_key_body = response_json(same_key_different_candidate).await;
    assert_eq!(same_key_body["applied"], false);
    assert!(same_key_body["blocked_reasons"]
        .as_array()
        .unwrap()
        .iter()
        .any(|reason| reason.as_str().unwrap_or("").contains("policy_key")));

    let different_key = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/auto-adjustments/apply")
                .header(header::AUTHORIZATION, format!("Bearer {key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "confirm_auto_adjustment": true,
                        "candidate_id": generate_candidate_id,
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(different_key.status(), StatusCode::OK);
    let different_key_body = response_json(different_key).await;
    assert_eq!(
        different_key_body["applied"], true,
        "different policy_key should be allowed: {different_key_body}"
    );

    let active = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/proposals?status=active&limit=500")
                .header(header::AUTHORIZATION, format!("Bearer {key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let active_body = response_json(active).await;
    assert_eq!(active_body["proposals"].as_array().unwrap().len(), 2);

    let audit = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/audit?limit=200")
                .header(header::AUTHORIZATION, format!("Bearer {key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let audit_body = response_json(audit).await;
    assert!(audit_body["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|event| event["action"] == "auto_adjustment.apply.rejected"
            && event["details"]["source"] == "auto_adjustment"
            && !event["details"]["blocked_reasons"]
                .as_array()
                .unwrap()
                .is_empty()));
}

#[tokio::test]
async fn test_auto_adjustment_rollback_requires_admin_and_blocks_stale_proposal_state() {
    let _env_lock = auto_adjustment_env_lock().lock().await;
    let _cleanup = AutoAdjustmentEnvCleanup;
    std::env::set_var("ACP_ENABLE_AUTO_ADJUSTMENT", "1");
    std::env::set_var("ACP_AUTO_ADJUSTMENT_ACTIVE", "1");
    std::env::remove_var("ACP_AUTO_ADJUSTMENT_DRY_RUN");

    let GeneratedAppFixture {
        app,
        key,
        readonly_key,
        ..
    } = make_generated_fixture();
    let candidate_id = generated_high_cost_candidate_id(&app, &key).await;

    let applied = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/auto-adjustments/apply")
                .header(header::AUTHORIZATION, format!("Bearer {key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"confirm_auto_adjustment": true, "candidate_id": candidate_id})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(applied.status(), StatusCode::OK);
    let applied_body = response_json(applied).await;
    let adjustment_id = applied_body["adjustment_id"].as_str().unwrap();

    let readonly_rollback = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/auto-adjustments/{adjustment_id}/rollback"))
                .header(header::AUTHORIZATION, format!("Bearer {readonly_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"confirm_auto_adjustment_rollback": true}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(readonly_rollback.status(), StatusCode::FORBIDDEN);

    let missing_snapshot = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/auto-adjustments/missing-adjustment/rollback")
                .header(header::AUTHORIZATION, format!("Bearer {key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"confirm_auto_adjustment_rollback": true}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(missing_snapshot.status(), StatusCode::BAD_REQUEST);

    let replacement = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/proposals")
                .header(header::AUTHORIZATION, format!("Bearer {key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "task_domain": "code",
                        "task_intent": "debug",
                        "target_tier": "verifier",
                        "payload": {"type": "tier_map_override"},
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(replacement.status(), StatusCode::OK);
    let replacement_body = response_json(replacement).await;
    let replacement_id = replacement_body["proposal"]["proposal_id"]
        .as_str()
        .unwrap();
    let replacement_approve = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/proposals/{replacement_id}/approve"))
                .header(header::AUTHORIZATION, format!("Bearer {key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"confirm_policy_override": true}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(replacement_approve.status(), StatusCode::OK);

    let stale_rollback = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/auto-adjustments/{adjustment_id}/rollback"))
                .header(header::AUTHORIZATION, format!("Bearer {key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"confirm_auto_adjustment_rollback": true}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(stale_rollback.status(), StatusCode::OK);
    let stale_body = response_json(stale_rollback).await;
    assert_eq!(stale_body["rolled_back"], false);
    assert!(stale_body["blocked_reasons"]
        .as_array()
        .unwrap()
        .iter()
        .any(|reason| reason.as_str().unwrap_or("").contains("not active")));

    let active = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/proposals?status=active&limit=500")
                .header(header::AUTHORIZATION, format!("Bearer {key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let active_body = response_json(active).await;
    let proposals = active_body["proposals"].as_array().unwrap();
    assert_eq!(proposals.len(), 1);
    assert_eq!(proposals[0]["proposal_id"], replacement_id);
    assert_eq!(proposals[0]["target_tier"], "verifier");

    let audit = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/audit?limit=200")
                .header(header::AUTHORIZATION, format!("Bearer {key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let audit_body = response_json(audit).await;
    assert!(audit_body["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(
            |event| event["action"] == "auto_adjustment.rollback.rejected"
                && event["details"]["source"] == "auto_adjustment"
                && !event["details"]["blocked_reasons"]
                    .as_array()
                    .unwrap()
                    .is_empty()
        ));
}

#[tokio::test]
async fn test_auto_adjustment_rollback_restores_previous_policy_and_validates_hash() {
    let _env_lock = auto_adjustment_env_lock().lock().await;
    let _cleanup = AutoAdjustmentEnvCleanup;
    std::env::set_var("ACP_ENABLE_AUTO_ADJUSTMENT", "1");
    std::env::set_var("ACP_AUTO_ADJUSTMENT_ACTIVE", "1");
    std::env::remove_var("ACP_AUTO_ADJUSTMENT_DRY_RUN");

    let GeneratedAppFixture {
        app,
        key,
        db_path,
        _dir,
        ..
    } = make_generated_fixture();
    let candidate_id = generated_high_cost_candidate_id(&app, &key).await;

    let previous = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/proposals")
                .header(header::AUTHORIZATION, format!("Bearer {key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "task_domain": "code",
                        "task_intent": "debug",
                        "target_tier": "verifier",
                        "payload": {"type": "tier_map_override"},
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(previous.status(), StatusCode::OK);
    let previous_body = response_json(previous).await;
    let previous_id = previous_body["proposal"]["proposal_id"].as_str().unwrap();

    let previous_approve = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/proposals/{previous_id}/approve"))
                .header(header::AUTHORIZATION, format!("Bearer {key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"confirm_policy_override": true}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(previous_approve.status(), StatusCode::OK);

    let applied = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/auto-adjustments/apply")
                .header(header::AUTHORIZATION, format!("Bearer {key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"confirm_auto_adjustment": true, "candidate_id": candidate_id})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(applied.status(), StatusCode::OK);
    let applied_body = response_json(applied).await;
    let adjustment_id = applied_body["adjustment_id"].as_str().unwrap().to_string();

    let missing_confirm = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/auto-adjustments/{adjustment_id}/rollback"))
                .header(header::AUTHORIZATION, format!("Bearer {key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(missing_confirm.status(), StatusCode::BAD_REQUEST);

    let conn = rusqlite::Connection::open(&db_path).unwrap();
    conn.execute(
        "UPDATE controlled_loop_policy_snapshots
         SET safety_hash = 'corrupted'
         WHERE adjustment_id = ?1",
        [&adjustment_id],
    )
    .unwrap();
    let corrupted = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/auto-adjustments/{adjustment_id}/rollback"))
                .header(header::AUTHORIZATION, format!("Bearer {key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"confirm_auto_adjustment_rollback": true}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(corrupted.status(), StatusCode::OK);
    let corrupted_body = response_json(corrupted).await;
    assert_eq!(corrupted_body["rolled_back"], false);
    assert!(corrupted_body["blocked_reasons"][0]
        .as_str()
        .unwrap()
        .contains("hash"));

    let real_hash: String = conn
        .query_row(
            "SELECT json_extract(snapshot_json, '$.safety_hash')
             FROM controlled_loop_policy_snapshots
             WHERE adjustment_id = ?1",
            [&adjustment_id],
            |row| row.get(0),
        )
        .unwrap();
    conn.execute(
        "UPDATE controlled_loop_policy_snapshots
         SET safety_hash = ?1
         WHERE adjustment_id = ?2",
        (&real_hash, &adjustment_id),
    )
    .unwrap();

    let rolled_back = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/auto-adjustments/{adjustment_id}/rollback"))
                .header(header::AUTHORIZATION, format!("Bearer {key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"confirm_auto_adjustment_rollback": true}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(rolled_back.status(), StatusCode::OK);
    let rollback_body = response_json(rolled_back).await;
    assert_eq!(rollback_body["rolled_back"], true);
    assert_eq!(rollback_body["status"], "rolled_back");

    let active = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/proposals?status=active&limit=500")
                .header(header::AUTHORIZATION, format!("Bearer {key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let active_body = response_json(active).await;
    let proposals = active_body["proposals"].as_array().unwrap();
    assert_eq!(proposals.len(), 1);
    assert_eq!(proposals[0]["proposal_id"], previous_id);
    assert_eq!(proposals[0]["target_tier"], "verifier");

    let repeated = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/auto-adjustments/{adjustment_id}/rollback"))
                .header(header::AUTHORIZATION, format!("Bearer {key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"confirm_auto_adjustment_rollback": true}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(repeated.status(), StatusCode::OK);
    let repeated_body = response_json(repeated).await;
    assert_eq!(repeated_body["rolled_back"], false);

    let audit = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/audit?limit=100")
                .header(header::AUTHORIZATION, format!("Bearer {key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let audit_body = response_json(audit).await;
    let actions: Vec<_> = audit_body["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|event| event["action"].as_str())
        .collect();
    assert!(actions.contains(&"auto_adjustment.rollback.rejected"));
    assert!(actions.contains(&"auto_adjustment.rollback.accepted"));
}

