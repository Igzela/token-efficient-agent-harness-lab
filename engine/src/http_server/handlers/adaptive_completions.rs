use std::collections::BTreeMap;

use axum::extract::{Extension, State};
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::IntoResponse;
use axum::Json;
use serde_json::{json, Value};

use crate::feedback::policy_snapshot::stable_hash;
use crate::feedback::{
    AdaptiveCandidate, AdaptiveCandidateConfig, AdaptiveCandidateGenerator, AdaptiveCandidateKind,
    AdaptiveExperimentController, AdaptiveExperimentGate, AdaptiveExperimentLimits,
    AdaptiveExperimentPolicy, AdaptiveExperimentRequest, CandidateGenerationRequest, FusionRole,
    ObjectiveProfile, PromotedAdaptivePolicy,
};
use crate::http_server::middleware::{
    authorize, cors_headers, internal_error, require_store, ApiError, RequestId,
};
use crate::http_server::state::AxumApiState;
use crate::http_server::AdaptiveFusionCompletionApiRequest;
use crate::provider::adaptive_execution::maybe_auto_promote_from_observation;
use crate::provider::adaptive_execution::{
    AdaptiveEndpointInvocation, AdaptiveExecutionGate, AdaptiveExecutionLimits,
    AdaptiveExecutionPlan, AdaptiveExecutionRequest,
};
use crate::provider::{check_cost_gates, CostGateConfig};
use crate::storage::local_product_store::{
    AdaptiveObservationInput, AdaptiveObservationSummary, LocalProductStore,
    ADAPTIVE_OBSERVATION_SCHEMA_VERSION,
};

const MAX_METADATA_BYTES: usize = 16 * 1024;
const DEFAULT_MAX_COST_USD: f64 = 1.0;
const DEFAULT_MAX_TOKENS: u64 = 32_768;
const DEFAULT_MAX_LATENCY_MS: u64 = 300_000;
const DEFAULT_OUTPUT_TOKENS: u64 = 1_024;

pub(crate) async fn api_adaptive_completion(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    Json(request): Json<AdaptiveFusionCompletionApiRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(
        &state,
        &headers,
        "dispatch:execute",
        uri.path(),
        &request_id.0,
    )?;
    let response =
        execute_adaptive_completion(&state, request, &request_id.0, &context.api_key_id).await?;
    Ok((cors_headers(), Json(response)))
}

pub(crate) async fn execute_adaptive_completion(
    state: &AxumApiState,
    request: AdaptiveFusionCompletionApiRequest,
    request_id: &str,
    actor: &str,
) -> Result<Value, ApiError> {
    validate_request(&request)?;
    let gate = AdaptiveExecutionGate::from_env(state.tenant_resolver.is_some());
    if !gate.is_enabled() {
        return Err(ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "adaptive_provider_not_available",
            "adaptive provider execution requires provider, adaptive, and auth gates",
        ));
    }
    let executor = state
        .adaptive_provider_executor
        .clone()
        .ok_or_else(|| unavailable("adaptive provider executor is not configured"))?;
    let registry = state
        .adaptive_registry_snapshot
        .as_ref()
        .ok_or_else(|| unavailable("adaptive model registry is not configured"))?;
    let store = require_store(state)?;

    let task_class = request
        .task_class
        .as_deref()
        .unwrap_or("general")
        .to_string();
    let objective = request.objective.unwrap_or(ObjectiveProfile::Efficient);
    let risk_level = request.risk_level.as_deref().unwrap_or("low").to_string();
    let max_cost_usd =
        env_f64("ACP_ADAPTIVE_COMPLETION_MAX_COST_USD").unwrap_or(DEFAULT_MAX_COST_USD);
    let max_tokens = env_u64("ACP_ADAPTIVE_COMPLETION_MAX_TOKENS").unwrap_or(DEFAULT_MAX_TOKENS);
    let max_latency_ms =
        env_u64("ACP_ADAPTIVE_COMPLETION_MAX_LATENCY_MS").unwrap_or(DEFAULT_MAX_LATENCY_MS);
    let estimated_input_tokens = ((request.prompt.len() as u64).saturating_add(3) / 4).max(1);
    let candidate_request = CandidateGenerationRequest {
        task_class: task_class.clone(),
        objective: objective_name(objective).to_string(),
        risk_level: risk_level.clone(),
        required_capabilities: vec!["completion".to_string()],
        max_estimated_cost_usd: max_cost_usd,
        max_estimated_tokens: max_tokens,
        max_estimated_latency_ms: max_latency_ms,
    };
    let candidate_config = AdaptiveCandidateConfig {
        estimated_input_tokens,
        estimated_output_tokens: env_u64("ACP_ADAPTIVE_COMPLETION_OUTPUT_TOKENS")
            .unwrap_or(DEFAULT_OUTPUT_TOKENS),
        ..Default::default()
    };
    let mut candidate_set = AdaptiveCandidateGenerator::generate(
        &candidate_request,
        &registry.endpoints,
        &candidate_config,
        &registry.snapshot_hash,
    );
    candidate_set
        .candidates
        .retain(|candidate| candidate.estimated_cost_usd > 0.0);
    let active_policy = store
        .active_adaptive_fusion_policies()
        .map_err(internal_error)?
        .into_iter()
        .find(|policy| policy.task_class == task_class && policy.objective == objective);
    let (candidate, experiment_assigned) = select_candidate(
        &candidate_set.candidates,
        active_policy.as_ref(),
        &store,
        request_id,
        &task_class,
        objective,
        &risk_level,
    )?;
    let plan = candidate_plan(&candidate)?;
    let total_tokens = candidate
        .endpoint_bindings
        .iter()
        .map(|binding| binding.estimated_input_tokens + binding.estimated_output_tokens)
        .sum::<u64>();
    let concurrency = if candidate.candidate_kind == AdaptiveCandidateKind::Fusion {
        candidate
            .endpoint_bindings
            .iter()
            .filter(|binding| binding.fusion_role == Some(FusionRole::Panel))
            .count()
            .clamp(1, 3)
    } else {
        1
    };
    let limits = AdaptiveExecutionLimits::new(
        candidate.endpoint_bindings.len(),
        candidate.estimated_cost_usd,
        max_latency_ms,
        concurrency,
    )
    .with_max_total_tokens(total_tokens.min(max_tokens));
    check_global_cost(&store, candidate.estimated_cost_usd)?;
    store
        .append_audit(
            actor,
            "adaptive_completion.selected",
            &candidate.candidate_id,
            &json!({
                "candidate_id": candidate.candidate_id,
                "candidate_hash": candidate.candidate_hash,
                "objective": objective_name(objective),
                "experiment_assigned": experiment_assigned,
            }),
        )
        .map_err(internal_error)?;

    let execution_request =
        AdaptiveExecutionRequest::new(request_id, &request.prompt, plan, limits);
    let result = executor.execute(&execution_request, &gate).await;
    match result {
        Ok(result) => {
            let observation = record_observation(
                &store,
                actor,
                request_id,
                &task_class,
                objective,
                &risk_level,
                &candidate,
                active_policy.as_ref(),
                true,
                result.total_provider_cost_usd,
                result.elapsed_ms,
                result.total_input_token_count,
                result.total_output_token_count,
            );
            Ok(completion_response(
                result.output,
                result.total_input_token_count,
                result.total_output_token_count,
                result.total_provider_cost_usd,
                result.elapsed_ms,
                request.include_routing_metadata.unwrap_or(false),
                &candidate,
                active_policy.as_ref(),
                observation.as_ref(),
                experiment_assigned,
            ))
        }
        Err(error) => {
            record_observation(
                &store,
                actor,
                request_id,
                &task_class,
                objective,
                &risk_level,
                &candidate,
                active_policy.as_ref(),
                false,
                error
                    .total_provider_cost_usd
                    .max(error.total_reserved_cost_usd),
                error.elapsed_ms,
                error.total_input_token_count,
                error.total_output_token_count,
            );
            Err(ApiError::with_code(
                StatusCode::BAD_REQUEST,
                error.code.as_ref(),
                error.message.as_ref(),
            ))
        }
    }
}

fn validate_request(request: &AdaptiveFusionCompletionApiRequest) -> Result<(), ApiError> {
    if request.prompt.trim().is_empty() {
        return Err(ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "prompt_required",
            "prompt is required",
        ));
    }
    if request.prompt.len() > 131_072 {
        return Err(ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "prompt_too_large",
            "prompt exceeds the adaptive completion limit",
        ));
    }
    if request
        .task_class
        .as_deref()
        .is_some_and(|value| !valid_id(value))
        || request
            .risk_level
            .as_deref()
            .is_some_and(|value| !matches!(value, "low" | "medium" | "high" | "critical"))
    {
        return Err(ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "adaptive_completion_context_invalid",
            "adaptive completion context is invalid",
        ));
    }
    if let Some(metadata) = &request.metadata {
        let serialized = serde_json::to_vec(metadata).unwrap_or_default();
        if !metadata.is_object() || serialized.len() > MAX_METADATA_BYTES {
            return Err(ApiError::with_code(
                StatusCode::BAD_REQUEST,
                "adaptive_completion_metadata_invalid",
                "metadata must be a bounded JSON object",
            ));
        }
    }
    Ok(())
}

fn select_candidate(
    candidates: &[AdaptiveCandidate],
    active_policy: Option<&PromotedAdaptivePolicy>,
    store: &LocalProductStore,
    request_id: &str,
    task_class: &str,
    objective: ObjectiveProfile,
    risk_level: &str,
) -> Result<(AdaptiveCandidate, bool), ApiError> {
    let Some(first) = candidates.first() else {
        return Err(ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "adaptive_candidate_unavailable",
            "no bounded adaptive candidate is available",
        ));
    };
    let mut selected = active_policy
        .and_then(|policy| {
            let rollout_bucket = deterministic_bucket(request_id, &policy.policy_hash);
            if rollout_bucket < policy.rollout_percentage as f64 / 100.0 {
                candidates
                    .iter()
                    .find(|candidate| candidate.candidate_id == policy.candidate_id)
            } else {
                candidates
                    .iter()
                    .find(|candidate| candidate.candidate_id == policy.baseline_candidate_id)
            }
        })
        .unwrap_or(first)
        .clone();

    let experiment_gate = AdaptiveExperimentGate::from_env();
    let experiment_policy = AdaptiveExperimentPolicy::from_env();
    let experiment = if experiment_gate.is_configured() {
        AdaptiveExperimentController::decide(
            &AdaptiveExperimentRequest {
                request_id: request_id.to_string(),
                exploration_seed: 0,
                risk_level: risk_level.to_string(),
            },
            &experiment_policy,
            &experiment_gate,
        )
        .ok()
    } else {
        None
    };
    let traffic_assigned = experiment
        .as_ref()
        .is_some_and(|decision| decision.assigned);
    let mut alternative_selected = false;
    if traffic_assigned {
        let counts = observation_counts(store, task_class, objective);
        let mut alternatives = candidates
            .iter()
            .filter(|candidate| candidate.candidate_id != selected.candidate_id)
            .filter(|candidate| {
                AdaptiveExperimentController::validate_limits(
                    &experiment_limits(candidate),
                    &experiment_policy,
                )
                .is_ok()
            })
            .collect::<Vec<_>>();
        alternatives.sort_by(|left, right| {
            counts
                .get(&left.candidate_id)
                .copied()
                .unwrap_or_default()
                .cmp(&counts.get(&right.candidate_id).copied().unwrap_or_default())
                .then_with(|| left.candidate_id.cmp(&right.candidate_id))
        });
        if let Some(alternative) = alternatives.first() {
            selected = (*alternative).clone();
            alternative_selected = true;
        }
    }
    Ok((selected, alternative_selected))
}

fn candidate_plan(candidate: &AdaptiveCandidate) -> Result<AdaptiveExecutionPlan, ApiError> {
    let invocation = |binding: &crate::feedback::CandidateEndpointBinding| {
        AdaptiveEndpointInvocation::new(
            &binding.endpoint_id,
            &binding.model_id,
            binding.estimated_cost_usd,
        )
    };
    match candidate.candidate_kind {
        AdaptiveCandidateKind::Single => {
            candidate
                .endpoint_bindings
                .first()
                .map(|binding| AdaptiveExecutionPlan::Single {
                    endpoint: invocation(binding),
                })
        }
        AdaptiveCandidateKind::OrderedFallback => Some(AdaptiveExecutionPlan::OrderedFallback {
            endpoints: candidate.endpoint_bindings.iter().map(invocation).collect(),
        }),
        AdaptiveCandidateKind::Fusion => {
            let panel = candidate
                .endpoint_bindings
                .iter()
                .filter(|binding| binding.fusion_role == Some(FusionRole::Panel))
                .map(invocation)
                .collect::<Vec<_>>();
            let judge = candidate
                .endpoint_bindings
                .iter()
                .find(|binding| binding.fusion_role == Some(FusionRole::Judge))
                .map(invocation);
            let synthesizer = candidate
                .endpoint_bindings
                .iter()
                .find(|binding| binding.fusion_role == Some(FusionRole::Synthesizer))
                .map(invocation);
            judge
                .zip(synthesizer)
                .map(|(judge, synthesizer)| AdaptiveExecutionPlan::Fusion {
                    panel,
                    judge,
                    synthesizer,
                })
        }
    }
    .ok_or_else(|| {
        ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "adaptive_candidate_invalid",
            "selected adaptive candidate has an invalid execution plan",
        )
    })
}

fn check_global_cost(store: &LocalProductStore, reserved_cost_usd: f64) -> Result<(), ApiError> {
    let config = CostGateConfig::from_env();
    let today_prefix = &crate::http_server::middleware::chrono_free_today()[..10];
    let daily_cost = store.daily_estimated_cost_usd(today_prefix).unwrap_or(0.0)
        + store
            .daily_adaptive_observation_cost_usd(today_prefix)
            .unwrap_or(0.0);
    check_cost_gates(&config, reserved_cost_usd, daily_cost).map_err(|error| {
        ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "adaptive_global_cost_gate_blocked",
            error.to_string(),
        )
    })
}

#[allow(clippy::too_many_arguments)]
fn record_observation(
    store: &LocalProductStore,
    actor: &str,
    request_id: &str,
    task_class: &str,
    objective: ObjectiveProfile,
    risk_level: &str,
    candidate: &AdaptiveCandidate,
    active_policy: Option<&PromotedAdaptivePolicy>,
    success: bool,
    cost_usd: f64,
    latency_ms: u64,
    input_tokens: u64,
    output_tokens: u64,
) -> Option<AdaptiveObservationSummary> {
    let input = AdaptiveObservationInput {
        schema_version: ADAPTIVE_OBSERVATION_SCHEMA_VERSION.to_string(),
        run_id: request_id.to_string(),
        request_id: request_id.to_string(),
        task_class: task_class.to_string(),
        objective,
        risk_level: risk_level.to_string(),
        candidate_id: candidate.candidate_id.clone(),
        candidate_hash: candidate.candidate_hash.clone(),
        policy_hash: active_policy.map(|policy| policy.policy_hash.clone()),
        candidate_kind: candidate_kind(candidate).to_string(),
        success,
        quality_score: f64::from(success),
        quality_score_source: "execution_success_proxy".to_string(),
        tool_success_score: f64::from(success),
        cost_usd,
        latency_ms,
        input_tokens,
        output_tokens,
    };
    match store.record_adaptive_observation(&input, actor) {
        Ok(observation) => {
            maybe_auto_promote_from_observation(store, &observation, actor);
            Some(observation)
        }
        Err(_) => {
            let _ = store.append_audit(
                actor,
                "adaptive_observation.rejected",
                "adaptive_completion",
                &json!({"error_domain": "adaptive_observation_rejected"}),
            );
            None
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn completion_response(
    output: Option<String>,
    input_tokens: u64,
    output_tokens: u64,
    cost_usd: f64,
    latency_ms: u64,
    include_routing_metadata: bool,
    candidate: &AdaptiveCandidate,
    active_policy: Option<&PromotedAdaptivePolicy>,
    observation: Option<&AdaptiveObservationSummary>,
    experiment_assigned: bool,
) -> Value {
    let mut response = json!({
        "schema_version": "adaptive_completion.v1",
        "output": output,
        "usage": {
            "input_tokens": input_tokens,
            "output_tokens": output_tokens,
            "estimated_cost_usd": cost_usd,
            "latency_ms": latency_ms,
        },
    });
    if include_routing_metadata {
        response["routing_metadata"] = json!({
            "candidate_id": candidate.candidate_id,
            "candidate_hash": candidate.candidate_hash,
            "candidate_kind": candidate_kind(candidate),
            "policy_hash": active_policy.map(|policy| policy.policy_hash.clone()),
            "policy_rollout_percentage": active_policy.map(|policy| policy.rollout_percentage),
            "observation_id": observation.map(|observation| observation.observation_id.clone()),
            "experiment_assigned": experiment_assigned,
        });
    }
    response
}

fn experiment_limits(candidate: &AdaptiveCandidate) -> AdaptiveExperimentLimits {
    AdaptiveExperimentLimits {
        reserved_cost_usd: candidate.estimated_cost_usd,
        max_cost_usd: candidate.estimated_cost_usd,
        max_total_tokens: candidate
            .endpoint_bindings
            .iter()
            .map(|binding| binding.estimated_input_tokens + binding.estimated_output_tokens)
            .sum(),
        max_calls: candidate.endpoint_bindings.len(),
        max_elapsed_ms: candidate.estimated_latency_ms.max(1),
        max_concurrency: if candidate.candidate_kind == AdaptiveCandidateKind::Fusion {
            candidate
                .endpoint_bindings
                .iter()
                .filter(|binding| binding.fusion_role == Some(FusionRole::Panel))
                .count()
        } else {
            1
        },
    }
}

fn observation_counts(
    store: &LocalProductStore,
    task_class: &str,
    objective: ObjectiveProfile,
) -> BTreeMap<String, usize> {
    store
        .adaptive_observations()
        .unwrap_or_default()
        .into_iter()
        .filter(|observation| {
            observation.task_class == task_class && observation.objective == objective
        })
        .fold(BTreeMap::new(), |mut counts, observation| {
            *counts.entry(observation.candidate_id).or_default() += 1;
            counts
        })
}

fn candidate_kind(candidate: &AdaptiveCandidate) -> &'static str {
    match candidate.candidate_kind {
        AdaptiveCandidateKind::Single => "single",
        AdaptiveCandidateKind::OrderedFallback => "ordered_fallback",
        AdaptiveCandidateKind::Fusion => "fusion",
    }
}

fn objective_name(objective: ObjectiveProfile) -> &'static str {
    match objective {
        ObjectiveProfile::Efficient => "efficient",
        ObjectiveProfile::Quality => "quality",
    }
}

fn deterministic_bucket(request_id: &str, policy_hash: &str) -> f64 {
    let hash = stable_hash(&json!({
        "request_id": request_id,
        "policy_hash": policy_hash,
    }));
    let sample = u64::from_str_radix(&hash[..16], 16).unwrap_or_default();
    sample as f64 / u64::MAX as f64
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 160
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
}

fn unavailable(message: &str) -> ApiError {
    ApiError::with_code(
        StatusCode::BAD_REQUEST,
        "adaptive_provider_not_available",
        message,
    )
}

fn env_f64(key: &str) -> Option<f64> {
    std::env::var(key)
        .ok()?
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite() && *value > 0.0)
}

fn env_u64(key: &str) -> Option<u64> {
    std::env::var(key).ok()?.trim().parse().ok()
}
