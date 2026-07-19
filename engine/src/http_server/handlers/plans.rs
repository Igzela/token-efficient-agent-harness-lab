use axum::extract::{Extension, Path as AxumPath, Query, State};
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::IntoResponse;
use axum::Json;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

use crate::http_server::middleware::{
    authorize, cors_headers, internal_error, require_store, ApiError, RequestId,
};
use crate::http_server::state::AxumApiState;
use crate::http_server::{
    AgentStepPlanApiRequest, ReadOnlyPlanApiRequest, AXUM_API_SCHEMA_VERSION,
};
use crate::provider::adaptive_observation::AdaptiveNodeExecutionConfig;
use crate::provider::redaction::contains_sensitive_patterns;
use crate::read_only_planner::ReadOnlyPlanner;

const MAX_AGENT_OBJECTIVE_BYTES: usize = 4096;

fn agent_step_creation_receipt_sha256(
    workflow_id: &str,
    node_id: &str,
    agent_id: &str,
    objective: &str,
) -> String {
    let canonical = json!({
        "kind": "agent_step_creation.v1",
        "workflow_id": workflow_id,
        "node_id": node_id,
        "agent_id": agent_id,
        "objective": objective,
    });
    hex::encode(Sha256::digest(canonical.to_string().as_bytes()))
}

pub(crate) async fn api_create_plan(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    Json(request): Json<ReadOnlyPlanApiRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let required_scope = if request.adaptive_execution.is_some() || request.agent_steps.is_some() {
        "dispatch:execute"
    } else {
        "dispatch:read"
    };
    let context = authorize(&state, &headers, required_scope, uri.path(), &request_id.0)?;
    if request.raw_request.trim().is_empty() {
        return Err(ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "raw_request_required",
            "raw_request is required",
        ));
    }
    if request.adaptive_execution.is_some() && request.confirm_adaptive_execution_plan != Some(true)
    {
        return Err(ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "adaptive_execution_confirmation_required",
            "confirm_adaptive_execution_plan must be true",
        ));
    }
    if request.agent_steps.is_some() && request.confirm_agent_runtime_plan != Some(true) {
        return Err(ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "agent_runtime_confirmation_required",
            "confirm_agent_runtime_plan must be true",
        ));
    }
    if request.agent_steps.is_some() {
        if request.raw_request.len() > MAX_AGENT_OBJECTIVE_BYTES {
            return Err(ApiError::with_code(
                StatusCode::BAD_REQUEST,
                "agent_runtime_objective_too_large",
                format!("agent runtime objective exceeds {MAX_AGENT_OBJECTIVE_BYTES} byte cap"),
            ));
        }
        if contains_sensitive_patterns(&request.raw_request) {
            return Err(ApiError::with_code(
                StatusCode::BAD_REQUEST,
                "agent_runtime_objective_sensitive",
                "agent runtime objective contains secret-shaped content",
            ));
        }
    }
    if request.adaptive_execution.is_some() && request.agent_steps.is_some() {
        return Err(ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "plan_executor_ambiguous",
            "adaptive_execution and agent_steps are mutually exclusive",
        ));
    }

    let store = require_store(&state)?;
    let request_source = request.request_source.as_deref().unwrap_or("api");
    let adaptive_execution = if let Some(value) = request.adaptive_execution.clone() {
        serde_json::from_value::<AdaptiveNodeExecutionConfig>(value.clone()).map_err(|_| {
            ApiError::with_code(
                StatusCode::BAD_REQUEST,
                "invalid_adaptive_execution",
                "adaptive_execution must contain a valid plan and limits",
            )
        })?;
        Some(value)
    } else {
        None
    };
    let provider_agent_model = if request.agent_steps.is_some() {
        state
            .configured_provider()
            .map(|provider| {
                provider
                    .default_model()
                    .filter(|model| !model.is_empty())
                    .map(str::to_string)
                    .ok_or_else(|| {
                        ApiError::with_code(
                            StatusCode::SERVICE_UNAVAILABLE,
                            "agent_runtime_provider_model_unavailable",
                            "configured agent decision provider has no default model",
                        )
                    })
            })
            .transpose()?
    } else {
        None
    };
    let agent_steps = request
        .agent_steps
        .as_ref()
        .map(|steps| validate_agent_step_plan_requests(steps, provider_agent_model.as_deref()))
        .transpose()?;
    let measured_agent_execution = provider_agent_model.is_some();
    let planner = ReadOnlyPlanner::new();
    let plan = store
        .create_workflow_plan(
            &request.raw_request,
            request_source,
            &context.api_key_id,
            |ids, created_at| {
                if let Some(agent_steps) = agent_steps {
                    Ok(agent_steps_plan(
                        ids,
                        &request.raw_request,
                        request_source,
                        created_at,
                        &agent_steps,
                        measured_agent_execution,
                    ))
                } else if let Some(adaptive_execution) = adaptive_execution {
                    Ok(adaptive_execution_plan(
                        ids,
                        &request.raw_request,
                        request_source,
                        created_at,
                        adaptive_execution,
                    ))
                } else {
                    planner.create_plan(ids, &request.raw_request, request_source, created_at)
                }
            },
        )
        .map_err(internal_error)?;

    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "plan": plan,
        })),
    ))
}

fn validate_agent_step_plan_request(
    request: &AgentStepPlanApiRequest,
    required_provider_model: Option<&str>,
) -> Result<AgentStepPlanApiRequest, ApiError> {
    fn bounded_identifier(value: &str) -> bool {
        !value.is_empty()
            && value.len() <= 256
            && value.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':')
            })
    }

    if !bounded_identifier(&request.agent_id)
        || !bounded_identifier(&request.profile_id)
        || !bounded_identifier(&request.role)
    {
        return Err(ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "invalid_agent_step_identity",
            "agent_id, profile_id, and role must be bounded identifiers",
        ));
    }
    if request.capability_profile.is_empty()
        || request.capability_profile.len() > 16
        || request
            .capability_profile
            .iter()
            .any(|capability| !bounded_identifier(capability))
        || request
            .capability_profile
            .iter()
            .collect::<HashSet<_>>()
            .len()
            != request.capability_profile.len()
    {
        return Err(ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "invalid_agent_capability_profile",
            "capability_profile must contain 1..=16 unique bounded identifiers",
        ));
    }
    if request
        .model
        .as_deref()
        .is_some_and(|model| !bounded_identifier(model))
    {
        return Err(ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "invalid_agent_model",
            "model must be a bounded identifier",
        ));
    }
    if let Some(required_model) = required_provider_model {
        let Some(requested_model) = request.model.as_deref() else {
            return Err(ApiError::with_code(
                StatusCode::BAD_REQUEST,
                "agent_runtime_model_required",
                "provider-backed agent_steps require the configured provider model",
            ));
        };
        if requested_model != required_model {
            return Err(ApiError::with_code(
                StatusCode::BAD_REQUEST,
                "agent_runtime_model_mismatch",
                "agent_steps model must exactly match the configured provider model",
            ));
        }
    }
    Ok(request.clone())
}

fn validate_agent_step_plan_requests(
    requests: &[AgentStepPlanApiRequest],
    required_provider_model: Option<&str>,
) -> Result<Vec<AgentStepPlanApiRequest>, ApiError> {
    if requests.is_empty() || requests.len() > 16 {
        return Err(ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "invalid_agent_step_count",
            "agent_steps must contain 1..=16 entries",
        ));
    }
    let mut identities: HashMap<String, (String, String, Vec<String>)> = HashMap::new();
    let mut validated = Vec::with_capacity(requests.len());
    for request in requests {
        let request = validate_agent_step_plan_request(request, required_provider_model)?;
        if let Some((role, profile_id, capabilities)) = identities.get(&request.agent_id) {
            if *role != request.role
                || *profile_id != request.profile_id
                || *capabilities != request.capability_profile
            {
                return Err(ApiError::with_code(
                    StatusCode::BAD_REQUEST,
                    "conflicting_agent_step_identity",
                    "repeated agent_steps entries must use identical role, profile_id, and capability_profile",
                ));
            }
        } else {
            identities.insert(
                request.agent_id.clone(),
                (
                    request.role.clone(),
                    request.profile_id.clone(),
                    request.capability_profile.clone(),
                ),
            );
        }
        validated.push(request);
    }
    Ok(validated)
}

fn agent_steps_plan(
    ids: &crate::read_only_planner::WorkflowPlanIds,
    raw_request: &str,
    request_source: &str,
    created_at: &str,
    agent_steps: &[AgentStepPlanApiRequest],
    measured_execution: bool,
) -> serde_json::Value {
    let recursive_usage_contract = if measured_execution {
        json!({"kind": "measured"})
    } else {
        json!({"kind": "fixture", "calls": 1, "tokens": 1, "cost_micros": 1, "time_ms": 1})
    };
    let decision_source = if measured_execution {
        "provider_typed_action"
    } else {
        "fixture"
    };
    let nodes = agent_steps
        .iter()
        .enumerate()
        .map(|(index, agent_step)| {
            let node_id = format!("agent-node-{:03}", index + 1);
            let recursive_capabilities = agent_step
                .capability_profile
                .iter()
                .cloned()
                .collect::<std::collections::BTreeSet<_>>();
            let mut node = json!({
                "node_id": node_id.clone(),
                "task_type": "agent_step",
                "status": "pending",
                "agent_id": agent_step.agent_id,
                "assigned_agent_id": agent_step.agent_id,
                "agent_role": agent_step.role,
                "agent_objective": raw_request,
                "capability_profile": agent_step.capability_profile,
                "profile_id": agent_step.profile_id,
                "model": agent_step.model,
                "decision_source": decision_source,
                "max_actions": 1,
            });
            if index == 0 {
                let object = node.as_object_mut().expect("agent node object");
                object.insert("recursive_root_node_id".to_string(), json!(node_id));
                object.insert(
                    "recursive_root_authority".to_string(),
                    json!({
                        "schema_version": crate::recursive_execution::RECURSIVE_ROOT_AUTHORITY_VERSION,
                        "scope": {
                            "repository": null,
                            "allowed_paths": [],
                            "capabilities": recursive_capabilities.clone(),
                        },
                        "capabilities": recursive_capabilities,
                        "tree_budget": crate::recursive_execution::default_recursive_tree_budget(),
                        "child_budget": crate::recursive_execution::default_recursive_child_budget(),
                        "usage_contract": recursive_usage_contract,
                    }),
                );
                object.insert(
                    "creation_receipt_sha256".to_string(),
                    json!(agent_step_creation_receipt_sha256(
                        &ids.workflow_id,
                        &format!("agent-node-{:03}", index + 1),
                        &agent_step.agent_id,
                        raw_request,
                    )),
                );
            }
            node
        })
        .collect::<Vec<_>>();
    let edges = (1..agent_steps.len())
        .map(|index| {
            json!({
                "edge_id": format!("agent-edge-{:03}-{:03}", index, index + 1),
                "from_node_id": format!("agent-node-{index:03}"),
                "to_node_id": format!("agent-node-{:03}", index + 1),
            })
        })
        .collect::<Vec<_>>();
    json!({
        "schema_version": "read_only_plan.v1",
        "plan_id": ids.plan_id,
        "status": "planned_read_only",
        "workflow_id": ids.workflow_id,
        "dispatch_id": ids.dispatch_id,
        "analysis": {
            "analysis_id": format!("analysis-{}", ids.dispatch_id),
            "task_domain": "agent_runtime",
            "request_source": request_source,
            "raw_request_snapshot": raw_request,
        },
        "graph": {
            "schema_version": "workflow_graph.v1",
            "workflow_id": ids.workflow_id,
            "dispatch_id": ids.dispatch_id,
            "status": "decomposed",
            "created_at": created_at,
            "updated_at": created_at,
            "nodes": nodes,
            "edges": edges,
        },
        "boundaries": {
            "execution": "explicit_tick_or_scheduler_lease",
            "execution_authority": "rust_scheduler_only",
            "provider_execution": "default_off_fail_closed",
            "target_repository_writes": "disabled",
            "runtime_workers": "bounded_one_step_executor",
        },
        "advisory": {
            "schema_version": "plan_advisory.v1",
            "mode": "explicit_agent_runtime_plan",
            "requires_executor": "agent_step",
        },
    })
}

fn adaptive_execution_plan(
    ids: &crate::read_only_planner::WorkflowPlanIds,
    raw_request: &str,
    request_source: &str,
    created_at: &str,
    adaptive_execution: serde_json::Value,
) -> serde_json::Value {
    json!({
        "schema_version": "read_only_plan.v1",
        "plan_id": ids.plan_id,
        "status": "planned_read_only",
        "workflow_id": ids.workflow_id,
        "dispatch_id": ids.dispatch_id,
        "analysis": {
            "analysis_id": format!("analysis-{}", ids.dispatch_id),
            "task_domain": "adaptive",
            "request_source": request_source,
            "raw_request_snapshot": raw_request,
        },
        "graph": {
            "schema_version": "workflow_graph.v1",
            "workflow_id": ids.workflow_id,
            "dispatch_id": ids.dispatch_id,
            "status": "decomposed",
            "created_at": created_at,
            "updated_at": created_at,
            "nodes": [{
                "node_id": "adaptive-node-1",
                "task_type": "adaptive_provider",
                "status": "pending",
                "adaptive_execution": adaptive_execution,
            }],
            "edges": [],
        },
        "boundaries": {
            "execution": "explicit_tick_or_scheduler_lease",
            "execution_authority": "rust_scheduler_only",
            "target_repository_writes": "disabled",
            "runtime_workers": "env_gated_supervised",
        },
        "advisory": {
            "schema_version": "plan_advisory.v1",
            "mode": "explicit_adaptive_execution_plan",
            "requires_executor": "adaptive_provider",
        },
    })
}

pub(crate) async fn api_plans(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "dispatch:read", uri.path(), &request_id.0)?;
    let store = require_store(&state)?;
    let limit = params
        .get("limit")
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(100)
        .clamp(0, 500);
    let offset = params
        .get("offset")
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(0)
        .max(0);
    let search = params.get("search").map(String::as_str);
    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "plans": store.search_workflow_plans(limit, offset, search).map_err(internal_error)?,
        })),
    ))
}

pub(crate) async fn api_plan_detail(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    AxumPath(plan_id): AxumPath<String>,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "dispatch:read", uri.path(), &request_id.0)?;
    let store = require_store(&state)?;
    match store.get_workflow_plan(&plan_id).map_err(internal_error)? {
        Some(plan) => Ok((
            cors_headers(),
            Json(json!({
                "schema_version": AXUM_API_SCHEMA_VERSION,
                "plan": plan,
            })),
        )),
        None => Err(ApiError::with_code(
            StatusCode::NOT_FOUND,
            "plan_not_found",
            "plan not found",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node_executor::{NodeExecutionInput, NodeExecutionOutput, NodeExecutor};

    struct FixtureAgentStepExecutor;

    impl NodeExecutor for FixtureAgentStepExecutor {
        fn executor_type_name(&self) -> &str {
            "agent_step"
        }

        fn execute_node(&self, _input: &NodeExecutionInput) -> NodeExecutionOutput {
            NodeExecutionOutput {
                status: "completed".to_string(),
                executor_type: "agent_step".to_string(),
                output: Some("fixture root completed".to_string()),
                error_domain: None,
                error_message: None,
                input_tokens: None,
                output_tokens: None,
                estimated_cost: None,
                latency_ms: Some(1),
            }
        }
    }

    fn agent_step(model: Option<&str>) -> AgentStepPlanApiRequest {
        AgentStepPlanApiRequest {
            agent_id: "agent-model-admission".to_string(),
            role: "reviewer".to_string(),
            capability_profile: vec!["mailbox".to_string()],
            profile_id: "bounded-reviewer".to_string(),
            model: model.map(str::to_string),
        }
    }

    #[test]
    fn fixture_agent_plan_may_omit_model() {
        let validated = validate_agent_step_plan_requests(&[agent_step(None)], None).unwrap();
        assert_eq!(validated[0].model, None);
    }

    #[test]
    fn provider_agent_plan_requires_exact_configured_model() {
        let missing = validate_agent_step_plan_requests(&[agent_step(None)], Some("bounded-model"))
            .unwrap_err();
        assert_eq!(missing.code, "agent_runtime_model_required");

        let mismatch = validate_agent_step_plan_requests(
            &[agent_step(Some("other-model"))],
            Some("bounded-model"),
        )
        .unwrap_err();
        assert_eq!(mismatch.code, "agent_runtime_model_mismatch");

        let validated = validate_agent_step_plan_requests(
            &[agent_step(Some("bounded-model"))],
            Some("bounded-model"),
        )
        .unwrap();
        assert_eq!(validated[0].model.as_deref(), Some("bounded-model"));
    }

    #[test]
    fn agent_plan_binds_deterministic_recursive_root_creation_identity() {
        let ids = crate::read_only_planner::WorkflowPlanIds::for_sequence(7);
        let request = agent_step(None);
        let plan = agent_steps_plan(
            &ids,
            "review docs",
            "fixture",
            "2026-07-18T00:00:00Z",
            std::slice::from_ref(&request),
            false,
        );
        let node = &plan["graph"]["nodes"][0];
        assert_eq!(node["recursive_root_node_id"], node["node_id"]);
        assert_eq!(
            node["recursive_root_authority"]["usage_contract"],
            json!({"kind": "fixture", "calls": 1, "tokens": 1, "cost_micros": 1, "time_ms": 1})
        );
        let receipt = node["creation_receipt_sha256"]
            .as_str()
            .expect("creation receipt");
        assert_eq!(receipt.len(), 64);
        assert_eq!(
            receipt,
            agent_step_creation_receipt_sha256(
                &ids.workflow_id,
                "agent-node-001",
                &request.agent_id,
                "review docs",
            )
        );
    }

    #[test]
    fn caller_fixture_provenance_cannot_downgrade_measured_usage() {
        let ids = crate::read_only_planner::WorkflowPlanIds::for_sequence(8);
        let request = agent_step(Some("bounded-model"));
        let plan = agent_steps_plan(
            &ids,
            "review docs",
            "fixture",
            "2026-07-18T00:00:00Z",
            std::slice::from_ref(&request),
            true,
        );
        let node = &plan["graph"]["nodes"][0];
        assert_eq!(node["decision_source"], "provider_typed_action");
        assert_eq!(
            node["recursive_root_authority"]["usage_contract"],
            json!({"kind": "measured"})
        );
    }

    #[test]
    fn api_created_fixture_plan_runs_recursive_root_to_completion() {
        let _guard = crate::recursive_execution::test_env_lock().lock().unwrap();
        std::env::set_var("ACP_RECURSIVE_EXECUTION_ENABLED", "1");
        std::env::remove_var("ACP_RECURSIVE_EXECUTION_KILL_SWITCH");
        let store = crate::storage::LocalProductStore::new(":memory:").expect("store");
        let request = agent_step(None);
        let plan = store
            .create_workflow_plan("review docs", "fixture", "test", |ids, created_at| {
                Ok(agent_steps_plan(
                    ids,
                    "review docs",
                    "fixture",
                    created_at,
                    std::slice::from_ref(&request),
                    false,
                ))
            })
            .expect("fixture plan");
        let run = store
            .create_workflow_run_from_plan(plan["plan_id"].as_str().expect("plan id"), "test")
            .expect("fixture run");
        let run_id = run["run_id"].as_str().expect("run id");

        let tick = store
            .tick_with_executor(run_id, "fixture-scheduler", 0, &FixtureAgentStepExecutor)
            .expect("fixture scheduler tick");
        assert_eq!(tick["action"], "node_executed");
        let tree = store
            .load_recursive_tree(run_id)
            .expect("load tree")
            .expect("recursive tree");
        assert_eq!(
            tree.execution_state,
            crate::recursive_execution::RecursiveExecutionState::Completed
        );
        assert_eq!(tree.spent_budget.calls_remaining, 1);
        assert_eq!(tree.spent_budget.tokens_remaining, 1);
        assert_eq!(tree.spent_budget.cost_micros_remaining, 1);
        assert_eq!(tree.spent_budget.time_ms_remaining, 1);

        std::env::remove_var("ACP_RECURSIVE_EXECUTION_ENABLED");
    }
}
