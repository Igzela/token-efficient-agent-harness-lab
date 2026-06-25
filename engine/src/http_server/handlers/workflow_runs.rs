use axum::extract::{Extension, Path as AxumPath, Query, State};
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::IntoResponse;
use axum::Json;
use serde_json::json;

use crate::http_server::middleware::{
    authorize, cors_headers, internal_error, require_store, ApiError, RequestId,
};
use crate::http_server::state::AxumApiState;
use crate::http_server::{
    WorkflowRunActionApiRequest, WorkflowRunApprovalApiRequest, WorkflowRunCreateApiRequest,
    WorkflowRunEventApiRequest, WorkflowRunTickApiRequest, AXUM_API_SCHEMA_VERSION,
};
use crate::workflow::dynamic_controller::{
    ControllerAction, ControllerTickResult, DynamicControllerConfig, DynamicWorkflowController,
};
use crate::workflow::orchestration_decision::{
    action_to_string, confidence_from_inputs, OrchestrationAction,
};

pub(crate) async fn api_create_workflow_run(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    Json(request): Json<WorkflowRunCreateApiRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(&state, &headers, "dispatch:read", uri.path(), &request_id.0)?;
    if request.plan_id.trim().is_empty() {
        return Err(ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "plan_id_required",
            "plan_id is required",
        ));
    }
    let store = require_store(&state)?;
    match store.create_workflow_run_from_plan(&request.plan_id, &context.api_key_id) {
        Ok(run) => Ok((cors_headers(), Json(json_response("run", run)))),
        Err(e) if e.starts_with("plan not found:") => Err(ApiError::with_code(
            StatusCode::NOT_FOUND,
            "plan_not_found",
            "plan not found",
        )),
        Err(e) => Err(internal_error(e)),
    }
}

pub(crate) async fn api_workflow_runs(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "dispatch:read", uri.path(), &request_id.0)?;
    let store = require_store(&state)?;
    let limit = query_i64(&params, "limit", 100).clamp(0, 500);
    let offset = query_i64(&params, "offset", 0).max(0);
    let search = params.get("search").map(String::as_str);
    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "runs": store.search_workflow_runs(limit, offset, search).map_err(internal_error)?,
        })),
    ))
}

pub(crate) async fn api_workflow_run_detail(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    AxumPath(run_id): AxumPath<String>,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "dispatch:read", uri.path(), &request_id.0)?;
    let store = require_store(&state)?;
    match store.get_workflow_run(&run_id).map_err(internal_error)? {
        Some(run) => Ok((cors_headers(), Json(json_response("run", run)))),
        None => Err(not_found()),
    }
}

pub(crate) async fn api_create_workflow_run_event(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    AxumPath(run_id): AxumPath<String>,
    Json(request): Json<WorkflowRunEventApiRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(&state, &headers, "dispatch:read", uri.path(), &request_id.0)?;
    if request.event_type.trim().is_empty() {
        return Err(ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "event_type_required",
            "event_type is required",
        ));
    }
    let store = require_store(&state)?;
    match store.append_workflow_run_event(
        &run_id,
        request.node_id.as_deref(),
        &request.event_type,
        &request.details.unwrap_or(serde_json::Value::Null),
        &context.api_key_id,
    ) {
        Ok(event) => Ok((cors_headers(), Json(json_response("event", event)))),
        Err(e) if e.starts_with("workflow run not found:") => Err(not_found()),
        Err(e) => Err(internal_error(e)),
    }
}

pub(crate) async fn api_workflow_run_events(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    AxumPath(run_id): AxumPath<String>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "dispatch:read", uri.path(), &request_id.0)?;
    let store = require_store(&state)?;
    let limit = query_i64(&params, "limit", 100).clamp(0, 500);
    match store.workflow_run_events(&run_id, limit) {
        Ok(events) => Ok((
            cors_headers(),
            Json(json!({
                "schema_version": AXUM_API_SCHEMA_VERSION,
                "events": events,
            })),
        )),
        Err(e) if e.starts_with("workflow run not found:") => Err(not_found()),
        Err(e) => Err(internal_error(e)),
    }
}

pub(crate) async fn api_create_workflow_run_approval(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    AxumPath(run_id): AxumPath<String>,
    Json(request): Json<WorkflowRunApprovalApiRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(&state, &headers, "dispatch:read", uri.path(), &request_id.0)?;
    if request.node_id.trim().is_empty() {
        return Err(ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "node_id_required",
            "node_id is required",
        ));
    }
    let store = require_store(&state)?;
    match store.record_workflow_run_approval(
        &run_id,
        &request.node_id,
        &request.decision,
        &context.api_key_id,
        request.reason.as_deref(),
        request.bound_patch_hash.as_deref(),
        request.bound_source_revision.as_deref(),
        request.bound_changed_files.as_deref(),
        request.expires_at.as_deref(),
    ) {
        Ok(approval) => Ok((cors_headers(), Json(json_response("approval", approval)))),
        Err(e) if e.starts_with("workflow run not found:") => Err(not_found()),
        Err(e) if e.starts_with("invalid workflow approval decision:") => Err(ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "invalid_approval_decision",
            "invalid approval decision",
        )),
        Err(e) => Err(internal_error(e)),
    }
}

pub(crate) async fn api_workflow_run_approvals(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    AxumPath(run_id): AxumPath<String>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "dispatch:read", uri.path(), &request_id.0)?;
    let store = require_store(&state)?;
    let limit = query_i64(&params, "limit", 100).clamp(0, 500);
    match store.workflow_run_approvals(&run_id, limit) {
        Ok(approvals) => Ok((
            cors_headers(),
            Json(json!({
                "schema_version": AXUM_API_SCHEMA_VERSION,
                "approvals": approvals,
            })),
        )),
        Err(e) if e.starts_with("workflow run not found:") => Err(not_found()),
        Err(e) => Err(internal_error(e)),
    }
}

pub(crate) async fn api_resume_workflow_run(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    AxumPath(run_id): AxumPath<String>,
    Json(request): Json<WorkflowRunActionApiRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(&state, &headers, "dispatch:read", uri.path(), &request_id.0)?;
    let store = require_store(&state)?;
    match store.request_workflow_run_resume(&run_id, &context.api_key_id, request.reason.as_deref())
    {
        Ok(run) => Ok((cors_headers(), Json(json_response("run", run)))),
        Err(e) if e.starts_with("workflow run not found:") => Err(not_found()),
        Err(e) => Err(internal_error(e)),
    }
}

pub(crate) async fn api_cancel_workflow_run(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    AxumPath(run_id): AxumPath<String>,
    Json(request): Json<WorkflowRunActionApiRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(&state, &headers, "dispatch:read", uri.path(), &request_id.0)?;
    let store = require_store(&state)?;
    match store.request_workflow_run_cancel(&run_id, &context.api_key_id, request.reason.as_deref())
    {
        Ok(run) => Ok((cors_headers(), Json(json_response("run", run)))),
        Err(e) if e.starts_with("workflow run not found:") => Err(not_found()),
        Err(e) => Err(internal_error(e)),
    }
}

pub(crate) async fn api_tick_workflow_run(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    AxumPath(run_id): AxumPath<String>,
    Json(request): Json<WorkflowRunTickApiRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(&state, &headers, "dispatch:read", uri.path(), &request_id.0)?;
    let store = require_store(&state)?;
    let actor = request.actor.as_deref().unwrap_or(&context.api_key_id);
    let max_retries = request.max_retries.unwrap_or(0).clamp(0, 10);
    let executor_type = request.executor.as_deref().unwrap_or("noop");
    let timeout_ms = request.timeout_ms.unwrap_or(30_000).clamp(1000, 300_000);

    match executor_type {
        "command" => {
            use crate::node_executor::CommandNodeExecutor;
            let executor = CommandNodeExecutor::default().with_timeout(timeout_ms);
            match store.tick_with_executor_and_command(
                &run_id,
                actor,
                max_retries,
                &executor,
                request.command.as_deref(),
            ) {
                Ok(result) => {
                    record_tick_decision(&store, &run_id, &result, "command");
                    Ok((cors_headers(), Json(json_response("tick", result))))
                }
                Err(e) if e.starts_with("workflow run not found:") => Err(not_found()),
                Err(e) if e.contains("terminal") => Err(ApiError::with_code(
                    StatusCode::CONFLICT,
                    "run_terminal",
                    &e,
                )),
                Err(e) => Err(internal_error(e)),
            }
        }
        "fail" => {
            use crate::node_executor::FailNodeExecutor;
            let executor = FailNodeExecutor::default();
            match store.tick_with_executor_and_command(
                &run_id,
                actor,
                max_retries,
                &executor,
                request.command.as_deref(),
            ) {
                Ok(result) => {
                    record_tick_decision(&store, &run_id, &result, "fail");
                    Ok((cors_headers(), Json(json_response("tick", result))))
                }
                Err(e) if e.starts_with("workflow run not found:") => Err(not_found()),
                Err(e) if e.contains("terminal") => Err(ApiError::with_code(
                    StatusCode::CONFLICT,
                    "run_terminal",
                    &e,
                )),
                Err(e) => Err(internal_error(e)),
            }
        }
        "dynamic" | "dynamic_noop" | "dynamic_workflow" => {
            use crate::node_executor::NoopNodeExecutor;
            let executor = NoopNodeExecutor;
            let mut controller = DynamicWorkflowController::new(DynamicControllerConfig {
                max_ticks_per_run: 1,
                executor_pool_accounting_enabled: false,
                ..DynamicControllerConfig::default()
            });
            match controller.tick(&store, &run_id, actor, &executor) {
                Ok(result) => Ok((
                    cors_headers(),
                    Json(json_response("tick", controller_tick_to_value(&result))),
                )),
                Err(e) if e.starts_with("workflow run not found:") => Err(not_found()),
                Err(e) => Err(internal_error(e)),
            }
        }
        "claude_code_cli" | "codex_cli" => {
            authorize(
                &state,
                &headers,
                "dispatch:execute",
                uri.path(),
                &request_id.0,
            )?;
            let cli_config = crate::cli::CliConfig::from_env();
            match crate::cli::CliNodeExecutor::from_config(&cli_config) {
                Some(executor) => {
                    match store.tick_with_executor_and_command(
                        &run_id,
                        actor,
                        max_retries,
                        &executor,
                        request.command.as_deref(),
                    ) {
                        Ok(result) => {
                            record_tick_decision(&store, &run_id, &result, executor_type);
                            Ok((cors_headers(), Json(json_response("tick", result))))
                        }
                        Err(e) if e.starts_with("workflow run not found:") => Err(not_found()),
                        Err(e) if e.contains("terminal") => Err(ApiError::with_code(
                            StatusCode::CONFLICT,
                            "run_terminal",
                            &e,
                        )),
                        Err(e) => Err(internal_error(e)),
                    }
                }
                None => {
                    let reason = if !cli_config.enabled {
                        "CLI execution not enabled (ACP_ENABLE_CLI_EXECUTION=1 required)"
                    } else {
                        "CLI executor binary not found"
                    };
                    Err(ApiError::with_code(
                        StatusCode::BAD_REQUEST,
                        "cli_not_available",
                        reason,
                    ))
                }
            }
        }
        "provider" | "provider_model" => {
            authorize(
                &state,
                &headers,
                "dispatch:execute",
                uri.path(),
                &request_id.0,
            )?;
            if !state.effective_execution_gates().provider_execution {
                return Err(ApiError::with_code(
                    StatusCode::BAD_REQUEST,
                    "provider_not_available",
                    "provider execution not enabled (ACP_ENABLE_PROVIDER_EXECUTION=1 or ready ACP_TRUSTED_LOCAL_PROFILE=1 required)",
                ));
            }
            let Some(provider) = state.provider.clone() else {
                return Err(ApiError::with_code(
                    StatusCode::BAD_REQUEST,
                    "provider_not_available",
                    "provider is not configured",
                ));
            };
            if !provider.is_enabled() {
                return Err(ApiError::with_code(
                    StatusCode::BAD_REQUEST,
                    "provider_not_available",
                    "provider is disabled",
                ));
            }
            let cost_config = crate::provider::CostGateConfig::from_env();
            let today_prefix = &crate::http_server::middleware::chrono_free_today()[..10];
            let daily_cost = store.daily_estimated_cost_usd(today_prefix).unwrap_or(0.0)
                + store
                    .daily_adaptive_observation_cost_usd(today_prefix)
                    .unwrap_or(0.0);
            let recorder = std::sync::Arc::new(crate::provider::ProviderAuditRecorder::with_store(
                store.clone(),
            ));
            let executor = crate::provider::executor::ProviderNodeExecutor::new(provider)
                .with_audit_recorder(recorder)
                .with_cost_gate(cost_config, daily_cost)
                .with_max_retries(max_retries);
            match store.tick_with_executor_and_command(
                &run_id,
                actor,
                max_retries,
                &executor,
                request.command.as_deref(),
            ) {
                Ok(result) => {
                    record_tick_decision(&store, &run_id, &result, "provider");
                    Ok((cors_headers(), Json(json_response("tick", result))))
                }
                Err(e) if e.starts_with("workflow run not found:") => Err(not_found()),
                Err(e) if e.contains("terminal") => Err(ApiError::with_code(
                    StatusCode::CONFLICT,
                    "run_terminal",
                    &e,
                )),
                Err(e) => Err(internal_error(e)),
            }
        }
        "adaptive_provider" => {
            authorize(
                &state,
                &headers,
                "dispatch:execute",
                uri.path(),
                &request_id.0,
            )?;
            let effective_gates = state.effective_execution_gates();
            let gate = crate::provider::adaptive_execution::AdaptiveExecutionGate::from_flags(
                effective_gates.provider_execution,
                effective_gates.adaptive_execution,
                state.tenant_resolver.is_some(),
            );
            if !gate.is_enabled() {
                return Err(ApiError::with_code(
                    StatusCode::BAD_REQUEST,
                    "adaptive_provider_not_available",
                    "adaptive provider execution requires provider, adaptive, and auth gates",
                ));
            }
            if max_retries != 0 {
                return Err(ApiError::with_code(
                    StatusCode::BAD_REQUEST,
                    "adaptive_retries_not_supported",
                    "adaptive provider execution owns its bounded fallback path",
                ));
            }
            let Some(adaptive_executor) = state.adaptive_provider_executor.clone() else {
                return Err(ApiError::with_code(
                    StatusCode::BAD_REQUEST,
                    "adaptive_provider_not_available",
                    "adaptive provider executor is not configured",
                ));
            };
            let cost_config = crate::provider::CostGateConfig::from_env();
            let today_prefix = &crate::http_server::middleware::chrono_free_today()[..10];
            let daily_cost = store.daily_estimated_cost_usd(today_prefix).unwrap_or(0.0)
                + store
                    .daily_adaptive_observation_cost_usd(today_prefix)
                    .unwrap_or(0.0);
            let contextual_policies = store
                .active_adaptive_fusion_policies()
                .map_err(internal_error)?;
            let persisted_observations = store
                .adaptive_bandit_observations()
                .map_err(internal_error)?;
            let experiment_gate =
                crate::feedback::AdaptiveExperimentGate::from_effective_gates(&effective_gates);
            let executor = crate::provider::adaptive_execution::AdaptiveProviderNodeExecutor::new(
                adaptive_executor,
                gate,
            )
            .with_contextual_policies(
                contextual_policies,
                crate::feedback::AdaptiveExplorationGate::from_env(),
            )
            .with_persisted_observations(persisted_observations);
            let executor = if experiment_gate.is_configured() {
                executor.with_online_experiments(
                    crate::feedback::AdaptiveExperimentPolicy::from_env(),
                    experiment_gate,
                )
            } else {
                executor
            }
            .with_cost_gate(cost_config, daily_cost);
            match store.tick_with_executor_and_command(
                &run_id,
                actor,
                0,
                &executor,
                request.command.as_deref(),
            ) {
                Ok(result) => {
                    let promotion_gate =
                        crate::feedback::AdaptiveAutoPromotionGate::from_effective_gates(
                            &effective_gates,
                        );
                    crate::provider::adaptive_observation::persist_adaptive_observation_with_gate(
                        &store,
                        &executor,
                        actor,
                        &promotion_gate,
                    );
                    record_tick_decision(&store, &run_id, &result, "adaptive_provider");
                    Ok((cors_headers(), Json(json_response("tick", result))))
                }
                Err(e) if e.starts_with("workflow run not found:") => Err(not_found()),
                Err(e) if e.contains("terminal") => Err(ApiError::with_code(
                    StatusCode::CONFLICT,
                    "run_terminal",
                    &e,
                )),
                Err(e) => Err(internal_error(e)),
            }
        }
        _ => match store.tick_with_retry(&run_id, actor, max_retries) {
            Ok(result) => {
                record_tick_decision(&store, &run_id, &result, "noop");
                Ok((cors_headers(), Json(json_response("tick", result))))
            }
            Err(e) if e.starts_with("workflow run not found:") => Err(not_found()),
            Err(e) if e.contains("terminal") => Err(ApiError::with_code(
                StatusCode::CONFLICT,
                "run_terminal",
                &e,
            )),
            Err(e) => Err(internal_error(e)),
        },
    }
}

fn query_i64(params: &std::collections::HashMap<String, String>, key: &str, default: i64) -> i64 {
    params
        .get(key)
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(default)
}

fn json_response(key: &str, value: serde_json::Value) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    map.insert(
        "schema_version".to_string(),
        serde_json::Value::String(AXUM_API_SCHEMA_VERSION.to_string()),
    );
    map.insert(key.to_string(), value);
    serde_json::Value::Object(map)
}

fn controller_tick_to_value(result: &ControllerTickResult) -> serde_json::Value {
    json!({
        "action": "dynamic_tick",
        "actions": result.actions.iter().map(controller_action_to_value).collect::<Vec<_>>(),
        "run_status": result.run_status,
        "mutations_applied": result.mutations_applied,
        "should_continue": result.should_continue,
        "suggested_executor_type": result.suggested_executor_type,
        "pool_failure_score": result.pool_failure_score,
        "pool_active_count": result.pool_active_count,
        "queue_position": result.queue_position,
        "priority": result.priority,
        "admission_allowed": result.admission_allowed,
        "admission_reason": result.admission_reason,
    })
}

fn controller_action_to_value(action: &ControllerAction) -> serde_json::Value {
    match action {
        ControllerAction::NodeExecuted { node_id, status } => {
            json!({"type": "node_executed", "node_id": node_id, "status": status})
        }
        ControllerAction::NodeRetried { node_id, attempt } => {
            json!({"type": "node_retried", "node_id": node_id, "attempt": attempt})
        }
        ControllerAction::GraphMutated {
            proposal_id,
            mutation_type,
        } => {
            json!({"type": "graph_mutated", "proposal_id": proposal_id, "mutation_type": mutation_type})
        }
        ControllerAction::ApprovalRequested { node_id, reason } => {
            json!({"type": "approval_requested", "node_id": node_id, "reason": reason})
        }
        ControllerAction::RunCompleted => json!({"type": "run_completed"}),
        ControllerAction::RunFailed { reason } => json!({"type": "run_failed", "reason": reason}),
        ControllerAction::NoAction { reason } => json!({"type": "no_action", "reason": reason}),
    }
}

fn record_tick_decision(
    store: &crate::storage::local_product_store::LocalProductStore,
    run_id: &str,
    result: &serde_json::Value,
    executor_type: &str,
) {
    let action_str = result
        .get("action")
        .and_then(|v| v.as_str())
        .unwrap_or("tick");
    let node_id = result.get("node_id").and_then(|v| v.as_str());
    let action = match action_str {
        "node_completed" => OrchestrationAction::RunCompleted,
        "node_failed" => OrchestrationAction::RunFailed,
        "node_retry" => OrchestrationAction::RetryNode,
        _ => OrchestrationAction::ExecuteNode,
    };
    let (confidence, score) =
        confidence_from_inputs("running", node_id.or(Some("pending")), true, None, None);

    let quality_signal = result.get("result").and_then(|r| r.get("quality")).cloned();

    let base = serde_json::json!({"source": "http_tick", "action": action_str});
    let enriched = crate::workflow::orchestration_decision::build_enriched_input_signals(
        &base,
        quality_signal.as_ref(),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    );

    let _ = store.record_orchestration_decision(
        run_id,
        node_id,
        action_to_string(&action),
        "http tick result",
        executor_type,
        None,
        confidence.as_str(),
        score,
        &enriched,
    );
}

fn not_found() -> ApiError {
    ApiError::with_code(
        StatusCode::NOT_FOUND,
        "workflow_run_not_found",
        "workflow run not found",
    )
}
