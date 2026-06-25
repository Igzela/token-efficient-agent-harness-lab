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
async fn axum_dynamic_tick_recovers_failed_run_with_graph_mutation() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("tick-dynamic.db")).unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    let plan_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/plans")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"raw_request": "dynamic recovery test", "request_source": "test"})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(plan_resp.status(), StatusCode::OK);
    let plan_body = response_json(plan_resp).await;
    let plan_id = plan_body["plan"]["plan_id"].as_str().unwrap();

    let run_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/workflow-runs")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"plan_id": plan_id}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let run_body = response_json(run_resp).await;
    let run_id = run_body["run"]["run_id"].as_str().unwrap();

    let fail_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/workflow-runs/{run_id}/tick"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"actor": "test", "executor": "fail"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(fail_resp.status(), StatusCode::OK);

    let dynamic_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/workflow-runs/{run_id}/tick"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"actor": "test", "executor": "dynamic"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(dynamic_resp.status(), StatusCode::OK);
    let dynamic_body = response_json(dynamic_resp).await;
    assert_eq!(dynamic_body["tick"]["action"], "dynamic_tick");
    assert!(
        dynamic_body["tick"]["mutations_applied"].as_i64().unwrap() >= 1,
        "dynamic tick should apply a recovery mutation: {dynamic_body}"
    );
    let actions = dynamic_body["tick"]["actions"].as_array().unwrap();
    assert!(
        actions.iter().any(|a| a["type"] == "graph_mutated"),
        "dynamic tick should report graph_mutated: {dynamic_body}"
    );

    let detail_resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/v1/workflow-runs/{run_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let detail_body = response_json(detail_resp).await;
    assert_eq!(detail_body["run"]["status"], "running");
    assert!(
        detail_body["run"]["nodes"].as_array().unwrap().len() >= 3,
        "failed run should have original plus recovery nodes"
    );
}

#[tokio::test]
async fn axum_tick_with_adaptive_provider_requires_adaptive_gate_and_auth() {
    let _guard = provider_cli_env_lock().lock().await;
    std::env::set_var("ACP_ENABLE_PROVIDER_EXECUTION", "1");
    std::env::remove_var("ACP_ENABLE_ADAPTIVE_FUSION_EXECUTION");

    let dir = tempdir().unwrap();
    let store = std::sync::Arc::new(
        LocalProductStore::new(dir.path().join("tick-adaptive-provider-400.db")).unwrap(),
    );
    let provider: std::sync::Arc<dyn engine::provider::Provider> =
        std::sync::Arc::new(engine::provider::stub::StubProvider::new("primary"));
    let adaptive = std::sync::Arc::new(
        engine::provider::adaptive_execution::AdaptiveExecutionExecutor::new(
            std::collections::BTreeMap::from([("primary".to_string(), provider)]),
            std::sync::Arc::new(engine::provider::ProviderAuditRecorder::with_store(
                store.clone(),
            )),
            engine::provider::adaptive_execution::AdaptiveExecutionKillSwitch::new(),
        ),
    );
    let plan = store
        .create_workflow_plan("adaptive task", "test", "actor", |ids, _| {
            Ok(json!({
                "schema_version": "read_only_plan.v1",
                "plan_id": ids.plan_id,
                "status": "planned_read_only",
                "workflow_id": ids.workflow_id,
                "dispatch_id": ids.dispatch_id,
                "analysis": {"analysis_id": "a-1", "task_domain": "test"},
                "graph": {
                    "schema_version": "workflow_graph.v1",
                    "workflow_id": ids.workflow_id,
                    "dispatch_id": ids.dispatch_id,
                    "status": "decomposed",
                    "created_at": "2026-06-21T00:00:00Z",
                    "updated_at": "2026-06-21T00:00:00Z",
                    "nodes": [{
                        "node_id": "node-a",
                        "task_type": "implementation",
                        "status": "pending",
                        "adaptive_execution": {
                            "plan": {
                                "mode": "single",
                                "endpoint": {
                                    "endpoint_id": "primary",
                                    "model": "stub-model",
                                    "reserved_cost_usd": 0.1
                                }
                            },
                            "limits": {
                                "max_calls": 1,
                                "max_cost_usd": 0.2,
                                "max_elapsed_ms": 1000,
                                "max_concurrency": 1
                            }
                        }
                    }],
                    "edges": []
                },
                "boundaries": {
                    "execution_authority": "disabled",
                    "target_repository_writes": "disabled",
                    "runtime_workers": "disabled"
                }
            }))
        })
        .unwrap();
    let run = store
        .create_workflow_run_from_plan(plan["plan_id"].as_str().unwrap(), "test")
        .unwrap();
    let run_id = run["run_id"].as_str().unwrap();
    let app = build_axum_router(
        AxumApiState::new()
            .with_local_store_arc(store)
            .with_adaptive_provider_executor(adaptive),
    );

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/workflow-runs/{run_id}/tick"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"actor": "test", "executor": "adaptive_provider"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = response_json(response).await;
    assert_eq!(body["code"], "adaptive_provider_not_available");
    std::env::remove_var("ACP_ENABLE_PROVIDER_EXECUTION");
}

#[tokio::test]
async fn axum_tick_with_adaptive_provider_executes_explicit_node_plan() {
    let _guard = provider_cli_env_lock().lock().await;
    std::env::set_var("ACP_ENABLE_PROVIDER_EXECUTION", "1");
    std::env::set_var("ACP_ENABLE_ADAPTIVE_FUSION_EXECUTION", "1");

    let dir = tempdir().unwrap();
    let store = std::sync::Arc::new(
        LocalProductStore::new(dir.path().join("tick-adaptive-provider-ok.db")).unwrap(),
    );
    let providers = [
        ("panel-a", "panel-a-model"),
        ("panel-b", "panel-b-model"),
        ("judge", "judge-model"),
        ("synth", "synth-model"),
    ]
    .into_iter()
    .map(|(endpoint_id, model)| {
        (
            endpoint_id.to_string(),
            std::sync::Arc::new(
                engine::provider::stub::StubProvider::new(endpoint_id).with_default_model(model),
            ) as std::sync::Arc<dyn engine::provider::Provider>,
        )
    })
    .collect::<std::collections::BTreeMap<_, _>>();
    let adaptive = std::sync::Arc::new(
        engine::provider::adaptive_execution::AdaptiveExecutionExecutor::new(
            providers,
            std::sync::Arc::new(engine::provider::ProviderAuditRecorder::with_store(
                store.clone(),
            )),
            engine::provider::adaptive_execution::AdaptiveExecutionKillSwitch::new(),
        ),
    );
    let plan = store
        .create_workflow_plan("adaptive task", "test", "actor", |ids, _| {
            Ok(json!({
                "schema_version": "read_only_plan.v1",
                "plan_id": ids.plan_id,
                "status": "planned_read_only",
                "workflow_id": ids.workflow_id,
                "dispatch_id": ids.dispatch_id,
                "analysis": {"analysis_id": "a-1", "task_domain": "test"},
                "graph": {
                    "schema_version": "workflow_graph.v1",
                    "workflow_id": ids.workflow_id,
                    "dispatch_id": ids.dispatch_id,
                    "status": "decomposed",
                    "created_at": "2026-06-21T00:00:00Z",
                    "updated_at": "2026-06-21T00:00:00Z",
                    "nodes": [{
                        "node_id": "node-a",
                        "task_type": "implementation",
                        "status": "pending",
                        "adaptive_execution": {
                            "observation_context": {
                                "request_id": "request-http-adaptive",
                                "task_class": "coding",
                                "objective": "quality",
                                "risk_level": "low",
                                "candidate_id": "fusion-http-candidate",
                                "policy_hash": null
                            },
                            "plan": {
                                "mode": "fusion",
                                "panel": [
                                    {
                                        "endpoint_id": "panel-a",
                                        "model": "panel-a-model",
                                        "reserved_cost_usd": 0.02
                                    },
                                    {
                                        "endpoint_id": "panel-b",
                                        "model": "panel-b-model",
                                        "reserved_cost_usd": 0.02
                                    }
                                ],
                                "judge": {
                                    "endpoint_id": "judge",
                                    "model": "judge-model",
                                    "reserved_cost_usd": 0.02
                                },
                                "synthesizer": {
                                    "endpoint_id": "synth",
                                    "model": "synth-model",
                                    "reserved_cost_usd": 0.02
                                }
                            },
                            "limits": {
                                "max_calls": 4,
                                "max_cost_usd": 0.2,
                                "max_elapsed_ms": 1000,
                                "max_concurrency": 1
                            }
                        }
                    }],
                    "edges": []
                },
                "boundaries": {
                    "execution_authority": "disabled",
                    "target_repository_writes": "disabled",
                    "runtime_workers": "disabled"
                }
            }))
        })
        .unwrap();
    let run = store
        .create_workflow_run_from_plan(plan["plan_id"].as_str().unwrap(), "test")
        .unwrap();
    let run_id = run["run_id"].as_str().unwrap();

    let mut resolver = TenantResolver::new();
    let scopes = HashSet::from(["dispatch:read".to_string(), "dispatch:execute".to_string()]);
    resolver.add_tenant(Tenant {
        tenant_id: "local".to_string(),
        name: "Local".to_string(),
        scopes: scopes.clone(),
        rate_limit: Some(100),
    });
    let (_key, raw_key) = resolver
        .create_api_key("local", Some(scopes), None, 1.0)
        .unwrap();
    let app = build_axum_router(
        AxumApiState::new()
            .with_local_store_arc(store.clone())
            .with_auth(resolver, RateLimiter::new(60.0, 100), Some(100), 1.0)
            .with_adaptive_provider_executor(adaptive),
    );

    let retry_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/workflow-runs/{run_id}/tick"))
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "actor": "test",
                        "executor": "adaptive_provider",
                        "max_retries": 1
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(retry_response.status(), StatusCode::BAD_REQUEST);
    let retry_body = response_json(retry_response).await;
    assert_eq!(retry_body["code"], "adaptive_retries_not_supported");

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/workflow-runs/{run_id}/tick"))
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"actor": "test", "executor": "adaptive_provider"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body["tick"]["result"]["status"], "completed");
    assert_eq!(body["tick"]["executor_type"], "adaptive_provider");
    assert_eq!(
        body["tick"]["result"]["trace"]["executor_type"],
        "adaptive_provider"
    );
    assert_eq!(
        body["tick"]["result"]["trace"]["env_gate"],
        "provider_plus_adaptive"
    );
    assert_eq!(
        body["tick"]["result"]["trace"]["kill_path"],
        "adaptive_kill_switch_or_provider_timeout"
    );
    let body_text = body.to_string();
    assert!(!body_text.contains("adaptive_observation"));
    let observations = store.adaptive_observations().unwrap();
    assert_eq!(observations.len(), 1);
    assert_eq!(observations[0].run_id, run_id);
    assert_eq!(observations[0].candidate_id, "fusion-http-candidate");
    assert_eq!(observations[0].candidate_kind, "fusion");
    assert!(observations[0].success);
    let event_types = store
        .provider_audit_events(20)
        .unwrap()
        .into_iter()
        .filter_map(|event| event["event_type"].as_str().map(str::to_string))
        .collect::<Vec<_>>();
    assert!(event_types.contains(&"adaptive_panel_request".to_string()));
    assert!(event_types.contains(&"adaptive_panel_response".to_string()));
    assert!(event_types.contains(&"adaptive_judge_request".to_string()));
    assert!(event_types.contains(&"adaptive_judge_response".to_string()));
    assert!(event_types.contains(&"adaptive_synthesizer_request".to_string()));
    assert!(event_types.contains(&"adaptive_synthesizer_response".to_string()));
    assert!(event_types.contains(&"adaptive_execution_completed".to_string()));

    std::env::remove_var("ACP_ENABLE_PROVIDER_EXECUTION");
    std::env::remove_var("ACP_ENABLE_ADAPTIVE_FUSION_EXECUTION");
}

