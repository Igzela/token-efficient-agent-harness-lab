use std::collections::BTreeMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::adaptive_execution::{
    AdaptiveExecutionExecutor, AdaptiveExecutionGate, AdaptiveExecutionLimits,
    AdaptiveExecutionPlan, AdaptiveProviderNodeExecutor,
};
use super::cost_gate::CostGateConfig;
use crate::feedback::{
    AdaptiveAutoPromotionGate, AdaptiveExperimentGate, AdaptiveExperimentPolicy,
    AdaptiveExplorationGate, ContextualBanditObservation, ContextualPolicyRequest,
    ObjectiveProfile, TaskClassEvaluation,
};
use crate::node_executor::{NodeExecutionInput, NodeExecutionOutput, NodeExecutor};
use crate::storage::local_product_store::{
    AdaptiveObservationInput, AdaptiveObservationSummary, LocalProductStore,
    ADAPTIVE_OBSERVATION_SCHEMA_VERSION,
};
use crate::trusted_local::EffectiveExecutionGates;

pub struct PersistingAdaptiveProviderNodeExecutor {
    executor: Arc<AdaptiveExecutionExecutor>,
    gate: AdaptiveExecutionGate,
    execution_gates: EffectiveExecutionGates,
    store: Arc<LocalProductStore>,
    actor: String,
}

impl PersistingAdaptiveProviderNodeExecutor {
    pub fn new(
        executor: Arc<AdaptiveExecutionExecutor>,
        gate: AdaptiveExecutionGate,
        store: Arc<LocalProductStore>,
        actor: impl Into<String>,
    ) -> Self {
        Self::new_with_effective_gates(
            executor,
            gate,
            EffectiveExecutionGates::from_env(),
            store,
            actor,
        )
    }

    pub fn new_with_effective_gates(
        executor: Arc<AdaptiveExecutionExecutor>,
        gate: AdaptiveExecutionGate,
        execution_gates: EffectiveExecutionGates,
        store: Arc<LocalProductStore>,
        actor: impl Into<String>,
    ) -> Self {
        Self {
            executor,
            gate,
            execution_gates,
            store,
            actor: actor.into(),
        }
    }
}

impl NodeExecutor for PersistingAdaptiveProviderNodeExecutor {
    fn executor_type_name(&self) -> &str {
        "adaptive_provider"
    }

    fn execute_node(&self, input: &NodeExecutionInput) -> NodeExecutionOutput {
        let contextual_policies = match self.store.active_adaptive_fusion_policies() {
            Ok(policies) => policies,
            Err(_) => return adaptive_worker_context_error("policy"),
        };
        let persisted_observations = match self.store.adaptive_bandit_observations() {
            Ok(observations) => observations,
            Err(_) => return adaptive_worker_context_error("observation"),
        };
        let today_prefix = chrono::Utc::now().format("%Y-%m-%d").to_string();
        let dispatch_cost = match self.store.daily_estimated_cost_usd(&today_prefix) {
            Ok(cost) => cost,
            Err(_) => return adaptive_worker_context_error("dispatch cost"),
        };
        let observation_cost = match self
            .store
            .daily_adaptive_observation_cost_usd(&today_prefix)
        {
            Ok(cost) => cost,
            Err(_) => return adaptive_worker_context_error("observation cost"),
        };
        let daily_cost = dispatch_cost + observation_cost;
        let experiment_gate = AdaptiveExperimentGate::from_effective_gates(&self.execution_gates);
        let executor = AdaptiveProviderNodeExecutor::new(self.executor.clone(), self.gate)
            .with_contextual_policies(contextual_policies, AdaptiveExplorationGate::from_env())
            .with_persisted_observations(persisted_observations);
        let executor = if experiment_gate.is_configured() {
            executor.with_online_experiments(AdaptiveExperimentPolicy::from_env(), experiment_gate)
        } else {
            executor
        }
        .with_cost_gate(CostGateConfig::from_env(), daily_cost);
        let output = executor.execute_node(input);
        let promotion_gate = AdaptiveAutoPromotionGate::from_effective_gates(&self.execution_gates);
        persist_adaptive_observation_with_gate(
            &self.store,
            &executor,
            &self.actor,
            &promotion_gate,
        );
        output
    }
}

fn adaptive_worker_context_error(context: &str) -> NodeExecutionOutput {
    NodeExecutionOutput {
        status: "failed".to_string(),
        executor_type: "adaptive_provider".to_string(),
        output: None,
        error_domain: Some("adaptive_worker_context_unavailable".to_string()),
        error_message: Some(format!("adaptive worker {context} context unavailable")),
        input_tokens: None,
        output_tokens: None,
        estimated_cost: None,
        latency_ms: Some(0),
    }
}

pub fn persist_adaptive_observation(
    store: &LocalProductStore,
    executor: &AdaptiveProviderNodeExecutor,
    actor: &str,
) {
    let gate = AdaptiveAutoPromotionGate::from_env();
    persist_adaptive_observation_with_gate(store, executor, actor, &gate);
}

pub fn persist_adaptive_observation_with_gate(
    store: &LocalProductStore,
    executor: &AdaptiveProviderNodeExecutor,
    actor: &str,
    gate: &AdaptiveAutoPromotionGate,
) {
    let Some(draft) = executor.take_observation() else {
        return;
    };
    let input = AdaptiveObservationInput {
        schema_version: ADAPTIVE_OBSERVATION_SCHEMA_VERSION.to_string(),
        run_id: draft.run_id,
        request_id: draft.request_id,
        task_class: draft.task_class,
        objective: draft.objective,
        risk_level: draft.risk_level,
        candidate_id: draft.candidate_id,
        candidate_hash: draft.candidate_hash,
        policy_hash: draft.policy_hash,
        candidate_kind: draft.candidate_kind,
        success: draft.success,
        quality_score: draft.quality_score,
        quality_score_source: draft.quality_score_source,
        tool_success_score: draft.tool_success_score,
        cost_usd: draft.cost_usd,
        latency_ms: draft.latency_ms,
        input_tokens: draft.input_tokens,
        output_tokens: draft.output_tokens,
    };
    match store.record_adaptive_observation(&input, actor) {
        Ok(observation) => {
            record_evidence_chain_candidate_with_gate(store, &observation, actor, gate)
        }
        Err(_) => {
            let _ = store.append_audit(
                actor,
                "adaptive_observation.rejected",
                "adaptive_fusion_observation",
                &serde_json::json!({"error_domain": "adaptive_observation_rejected"}),
            );
        }
    }
}

pub fn record_evidence_chain_candidate(
    store: &LocalProductStore,
    observation: &AdaptiveObservationSummary,
    actor: &str,
) {
    let gate = AdaptiveAutoPromotionGate::from_env();
    record_evidence_chain_candidate_with_gate(store, observation, actor, &gate);
}

pub fn record_evidence_chain_candidate_with_gate(
    store: &LocalProductStore,
    observation: &AdaptiveObservationSummary,
    actor: &str,
    gate: &AdaptiveAutoPromotionGate,
) {
    if !gate.is_configured() {
        return;
    }
    let Ok(active_policies) = store.active_adaptive_fusion_policies() else {
        return;
    };
    let Some(active) = active_policies.into_iter().find(|policy| {
        policy.task_class == observation.task_class && policy.objective == observation.objective
    }) else {
        return;
    };
    if active.candidate_id == observation.candidate_id {
        return;
    }
    let _ = store.append_audit(
        actor,
        "adaptive_policy.evidence_chain_candidate",
        &format!("{}:{}", observation.task_class, observation.candidate_id),
        &serde_json::json!({
            "run_id": observation.run_id,
            "task_class": observation.task_class,
            "objective": observation.objective,
            "risk_level": observation.risk_level,
            "candidate_id": observation.candidate_id,
            "candidate_hash": observation.candidate_hash,
            "active_candidate_id": active.candidate_id,
            "active_policy_hash": active.policy_hash,
            "required_next_owner": "offline_replay_evidence_chain_operator",
            "mutation_authority": "none",
            "provider_calls": "disabled",
        }),
    );
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AdaptiveObservationDraft {
    pub run_id: String,
    pub request_id: String,
    pub task_class: String,
    pub objective: ObjectiveProfile,
    pub risk_level: String,
    pub candidate_id: String,
    pub candidate_hash: String,
    pub policy_hash: Option<String>,
    pub candidate_kind: String,
    pub success: bool,
    pub quality_score: f64,
    pub quality_score_source: String,
    pub tool_success_score: f64,
    pub cost_usd: f64,
    pub latency_ms: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AdaptiveObservationContext {
    pub request_id: String,
    pub task_class: String,
    pub objective: ObjectiveProfile,
    pub risk_level: String,
    pub candidate_id: String,
    #[serde(default)]
    pub policy_hash: Option<String>,
}

#[derive(Clone, Deserialize)]
pub struct AdaptiveNodeExecutionConfig {
    pub plan: AdaptiveExecutionPlan,
    pub limits: AdaptiveExecutionLimits,
    #[serde(default)]
    pub observation_context: Option<AdaptiveObservationContext>,
}

#[derive(Deserialize)]
pub struct AdaptivePolicyNodeExecutionConfig {
    pub request: ContextualPolicyRequest,
    pub evaluation: TaskClassEvaluation,
    #[serde(default)]
    pub observations: Vec<ContextualBanditObservation>,
    pub candidate_plans: BTreeMap<String, AdaptiveNodeExecutionConfig>,
}
