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
async fn test_generated_endpoint_returns_candidates_with_evidence() {
    let (app, key) = make_generated_app();

    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/proposals/generated?limit=50")
                .header(header::AUTHORIZATION, format!("Bearer {key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = response_json(resp).await;
    assert_eq!(body["schema_version"], "generated_proposals.v1");
    let candidates = body["candidates"].as_array().unwrap();
    assert!(
        !candidates.is_empty(),
        "should generate candidates from seeded failure data"
    );
    let first = &candidates[0];
    assert!(
        !first["evidence"]["evidence_trace_ids"]
            .as_array()
            .unwrap()
            .is_empty(),
        "generated candidate must include evidence_trace_ids"
    );
}

#[tokio::test]
async fn test_generated_endpoint_does_not_persist_proposals() {
    let (app, key) = make_generated_app();

    // Get initial proposal count
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/proposals?limit=500")
                .header(header::AUTHORIZATION, format!("Bearer {key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let before = response_json(resp).await;
    let count_before = before["proposals"].as_array().unwrap().len();

    // Call generated endpoint
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/proposals/generated?limit=50")
                .header(header::AUTHORIZATION, format!("Bearer {key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Verify no new rows were created
    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/proposals?limit=500")
                .header(header::AUTHORIZATION, format!("Bearer {key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let after = response_json(resp).await;
    let count_after = after["proposals"].as_array().unwrap().len();
    assert_eq!(
        count_before, count_after,
        "GET generated must not create proposal rows"
    );
}

#[tokio::test]
async fn test_generated_endpoint_does_not_change_active_routing_policy() {
    let (app, key) = make_generated_app();

    // Verify no active policy before
    let resp = app
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
    let before = response_json(resp).await;
    assert!(
        before["proposals"].as_array().unwrap().is_empty(),
        "no active proposals before generated call"
    );

    // Call generated endpoint
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/proposals/generated?limit=50")
                .header(header::AUTHORIZATION, format!("Bearer {key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Verify still no active proposals
    let resp = app
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
    let after = response_json(resp).await;
    assert!(
        after["proposals"].as_array().unwrap().is_empty(),
        "GET generated must not activate any proposals"
    );
}

#[tokio::test]
async fn test_generated_candidates_have_safety_flags_and_approval_requirement() {
    let (app, key) = make_generated_app();

    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/proposals/generated?limit=50")
                .header(header::AUTHORIZATION, format!("Bearer {key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = response_json(resp).await;
    let candidates = body["candidates"].as_array().unwrap();
    assert!(!candidates.is_empty());

    for candidate in candidates {
        assert_eq!(
            candidate["requires_human_approval"], true,
            "every generated candidate must require human approval"
        );
        assert_eq!(
            candidate["evidence"]["safety_flags"]["no_auto_activation"], true,
            "every generated candidate must have no_auto_activation flag"
        );
        assert_eq!(
            candidate["evidence"]["safety_flags"]["no_provider_cli_boundary_expansion"], true,
            "auto-adjustment must not expand provider/CLI boundary"
        );
        assert_eq!(
            candidate["evidence"]["safety_flags"]["no_auth_security_change"], true,
            "auto-adjustment must not change auth/security behavior"
        );
        assert_eq!(
            candidate["evidence"]["safety_flags"]["no_db_migration_required"], true,
            "generated candidates must not require DB migrations"
        );
        assert_eq!(
            candidate["evidence"]["safety_flags"]["no_hard_constraint_mutation"], true,
            "auto-adjustment must not mutate hard constraints"
        );
        assert_eq!(
            candidate["evidence"]["safety_flags"]["no_target_repo_write"], true,
            "auto-adjustment must not write target repositories"
        );
        assert_eq!(
            candidate["evidence"]["safety_flags"]["no_destructive_operation"], true,
            "auto-adjustment must not create destructive/release/deploy side effects"
        );
        assert_ne!(
            candidate["status"].as_str().unwrap_or(""),
            "active",
            "generated candidates must never have active status"
        );
    }
}

#[tokio::test]
async fn axum_adaptive_policy_promote_get_and_rollback_cycle() {
    let _env_lock = auto_adjustment_env_lock().lock().await;
    let _cleanup = AdaptivePolicyEnvCleanup;
    std::env::remove_var("ACP_ENABLE_ADAPTIVE_POLICY_PROMOTION");
    std::env::remove_var("ACP_ADAPTIVE_POLICY_PROMOTION_ACTIVE");
    std::env::set_var("ACP_ENABLE_ADAPTIVE_POLICY_PROMOTION", "1");
    std::env::set_var("ACP_ADAPTIVE_POLICY_PROMOTION_ACTIVE", "1");

    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("adaptive-policy.db")).unwrap();
    for index in 0..30 {
        store
            .record_dispatch(
                &format!("adaptive evidence {index}"),
                "api",
                &json!({
                    "record": {
                        "dispatch_id": format!("run-{index}"),
                        "final_status": "completed"
                    },
                    "decision": {
                        "selected_tier": "adaptive_provider",
                        "budget_reservation": {"reserved_cost": 0.02},
                        "shadow_routes": []
                    },
                    "analysis": {"task_class": "coding", "risk_level": "low"},
                    "execution_result": {
                        "executor_type": "adaptive_provider",
                        "status": "completed",
                        "success": true,
                        "estimated_cost": 0.02,
                        "latency_ms": 100
                    },
                    "evaluation_result": {"status": "pass"}
                }),
                "test",
            )
            .unwrap();
    }

    let mut resolver = TenantResolver::new();
    let admin_scopes = HashSet::from([
        "dispatch:read".to_string(),
        "team:admin".to_string(),
        "audit:read".to_string(),
    ]);
    resolver.add_tenant(Tenant {
        tenant_id: "local".to_string(),
        name: "Local".to_string(),
        scopes: admin_scopes.clone(),
        rate_limit: Some(10_000),
    });
    let (_key, raw_key) = resolver
        .create_api_key("local", Some(admin_scopes), None, 1.0)
        .unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store).with_auth(
        resolver,
        RateLimiter::new(60.0, 10_000),
        Some(10_000),
        1.0,
    ));

    let promotion = |evidence_run_ids: Vec<String>| {
        json!({
            "actor": "operator",
            "promotion": {
                "schema_version": "adaptive_policy_promotion.v1",
                "task_class": "coding",
                "objective": "quality",
                "candidate_id": "strong",
                "baseline_candidate_id": "cheap",
                "sample_count": 30,
                "confidence": 0.9,
                "mean_quality_delta": 0.1,
                "mean_cost_reduction": 0.02,
                "failure_rate_delta": 0.0,
                "evidence_run_ids": evidence_run_ids,
                "risk_level": "low",
                "confirm_adaptive_policy_promotion": true
            }
        })
    };

    let missing_evidence = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/adaptive-fusion/policies/promote")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    promotion(
                        (0..30)
                            .map(|index| format!("run-missing-{index}"))
                            .collect(),
                    )
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(missing_evidence.status(), StatusCode::BAD_REQUEST);
    let missing_body = response_json(missing_evidence).await;
    assert_eq!(missing_body["code"], "adaptive_policy_evidence_missing");

    let evidence_run_ids = (0..30).map(|index| format!("run-{index}")).collect();
    let promoted = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/adaptive-fusion/policies/promote")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(promotion(evidence_run_ids).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(promoted.status(), StatusCode::OK);
    let promoted_body = response_json(promoted).await;
    assert_eq!(promoted_body["decision"]["eligible"], true);
    assert_eq!(promoted_body["result"]["applied"], true);
    assert_eq!(promoted_body["result"]["live_execution_authority"], false);
    let adjustment_id = promoted_body["result"]["adjustment_id"]
        .as_str()
        .unwrap()
        .to_string();

    let listed = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/adaptive-fusion/policies")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(listed.status(), StatusCode::OK);
    let listed_body = response_json(listed).await;
    assert_eq!(listed_body["policies"].as_array().unwrap().len(), 1);
    assert_eq!(listed_body["live_execution_authority"], false);
    assert_eq!(listed_body["requires_explicit_adaptive_plan"], true);

    let rollback = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!(
                    "/api/v1/adaptive-fusion/policies/{adjustment_id}/rollback"
                ))
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"confirm_adaptive_policy_rollback": true}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(rollback.status(), StatusCode::OK);
    let rollback_body = response_json(rollback).await;
    assert_eq!(rollback_body["rolled_back"], true);

    let listed_after = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/adaptive-fusion/policies")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let listed_after_body = response_json(listed_after).await;
    assert!(listed_after_body["policies"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn test_auto_adjustments_endpoint_disabled_and_dry_run_read_only() {
    let _env_lock = auto_adjustment_env_lock().lock().await;
    let _cleanup = AutoAdjustmentEnvCleanup;
    std::env::remove_var("ACP_ENABLE_AUTO_ADJUSTMENT");
    std::env::remove_var("ACP_AUTO_ADJUSTMENT_DRY_RUN");
    std::env::remove_var("ACP_AUTO_ADJUSTMENT_ACTIVE");

    let (app, key) = make_generated_app();

    let disabled_resp = app
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
    assert_eq!(disabled_resp.status(), StatusCode::OK);
    let disabled = response_json(disabled_resp).await;
    assert_eq!(disabled["schema_version"], "auto_adjustments_report.v1");
    assert_eq!(disabled["mode"], "disabled");
    assert_eq!(disabled["env_gate"], false);
    assert_eq!(disabled["dry_run"], false);
    assert_eq!(disabled["no_live_mutation"], true);
    assert_eq!(disabled["active_apply_available"], false);

    let proposals_before_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/proposals?limit=500")
                .header(header::AUTHORIZATION, format!("Bearer {key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let proposals_before = response_json(proposals_before_resp).await;
    let proposal_count_before = proposals_before["proposals"].as_array().unwrap().len();

    let active_before_resp = app
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
    let active_before = response_json(active_before_resp).await;
    assert!(active_before["proposals"].as_array().unwrap().is_empty());

    std::env::set_var("ACP_ENABLE_AUTO_ADJUSTMENT", "1");
    std::env::set_var("ACP_AUTO_ADJUSTMENT_DRY_RUN", "1");

    let dry_run_resp = app
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
    assert_eq!(dry_run_resp.status(), StatusCode::OK);
    let dry_run = response_json(dry_run_resp).await;
    assert_eq!(dry_run["mode"], "dry_run");
    assert_eq!(dry_run["env_gate"], true);
    assert_eq!(dry_run["dry_run"], true);
    assert_eq!(dry_run["no_live_mutation"], true);
    assert_eq!(dry_run["guard"]["max_adjustments_remaining"], 0);
    assert!(
        !dry_run["decisions"].as_array().unwrap().is_empty(),
        "dry-run should emit policy decisions for generated candidates"
    );
    assert!(
        !dry_run["snapshot_previews"].as_array().unwrap().is_empty(),
        "dry-run should emit snapshot previews"
    );

    let proposals_after_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/proposals?limit=500")
                .header(header::AUTHORIZATION, format!("Bearer {key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let proposals_after = response_json(proposals_after_resp).await;
    assert_eq!(
        proposal_count_before,
        proposals_after["proposals"].as_array().unwrap().len(),
        "dry-run must not create controlled_loop_policy_proposals rows"
    );

    let active_after_resp = app
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
    let active_after = response_json(active_after_resp).await;
    assert!(
        active_after["proposals"].as_array().unwrap().is_empty(),
        "dry-run must not activate active_routing_policy"
    );
}

