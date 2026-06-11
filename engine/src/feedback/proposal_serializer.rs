use serde_json::{json, Value};

use super::policy_proposer::ProposalCandidate;

const POLICY_PROPOSAL_SCHEMA_VERSION: &str = "controlled_loop_policy_proposal.v1";

pub fn serialize_candidate_to_proposal_request(candidate: &ProposalCandidate) -> Value {
    let mut simulation_deltas = json!({});
    if let Some(v) = candidate.evidence.success_rate_delta {
        simulation_deltas["success_rate_delta"] = json!(v);
    }
    if let Some(v) = candidate.evidence.cost_delta {
        simulation_deltas["cost_delta"] = json!(v);
    }
    if let Some(v) = candidate.evidence.latency_delta {
        simulation_deltas["latency_delta"] = json!(v);
    }
    if let Some(v) = candidate.evidence.human_review_rate_delta {
        simulation_deltas["human_review_rate_delta"] = json!(v);
    }

    json!({
        "title": candidate.title,
        "summary": candidate.summary,
        "task_domain": candidate.task_domain,
        "task_intent": candidate.task_intent,
        "target_tier": candidate.target_tier,
        "evidence": {
            "schema_version": candidate.schema_version,
            "source": candidate.source,
            "pattern_ids": candidate.evidence.pattern_ids,
            "evidence_trace_ids": candidate.evidence.evidence_trace_ids,
            "simulation_scenario_id": candidate.evidence.simulation_scenario_id,
            "confidence": candidate.confidence,
            "risk_level": candidate.risk_level,
            "safety_flags": {
                "no_provider_cli_boundary_expansion": candidate.safety_flags.no_provider_cli_boundary_expansion,
                "no_auth_security_change": candidate.safety_flags.no_auth_security_change,
                "no_db_migration_required": candidate.safety_flags.no_db_migration_required,
                "no_hard_constraint_mutation": candidate.safety_flags.no_hard_constraint_mutation,
                "no_target_repo_write": candidate.safety_flags.no_target_repo_write,
                "no_destructive_operation": candidate.safety_flags.no_destructive_operation,
                "no_auto_activation": candidate.safety_flags.no_auto_activation,
            },
            "simulation_deltas": simulation_deltas,
        }
    })
}

pub fn serialize_candidate_to_api_response(candidate: &ProposalCandidate) -> Value {
    json!({
        "schema_version": POLICY_PROPOSAL_SCHEMA_VERSION,
        "proposal_id": candidate.candidate_id,
        "status": "generated",
        "title": candidate.title,
        "summary": candidate.summary,
        "task_domain": candidate.task_domain,
        "task_intent": candidate.task_intent,
        "task_class": candidate.task_class,
        "policy_key": candidate.policy_key,
        "target_tier": candidate.target_tier,
        "tier": candidate.target_tier,
        "requires_human_approval": true,
        "scope": "tier_map_override",
        "source": "generated_from_feedback",
        "evidence": {
            "schema_version": candidate.schema_version,
            "source": candidate.source,
            "pattern_ids": candidate.evidence.pattern_ids,
            "evidence_trace_ids": candidate.evidence.evidence_trace_ids,
            "simulation_scenario_id": candidate.evidence.simulation_scenario_id,
            "confidence": candidate.confidence,
            "risk_level": candidate.risk_level,
            "safety_flags": {
                "no_provider_cli_boundary_expansion": candidate.safety_flags.no_provider_cli_boundary_expansion,
                "no_auth_security_change": candidate.safety_flags.no_auth_security_change,
                "no_db_migration_required": candidate.safety_flags.no_db_migration_required,
                "no_hard_constraint_mutation": candidate.safety_flags.no_hard_constraint_mutation,
                "no_target_repo_write": candidate.safety_flags.no_target_repo_write,
                "no_destructive_operation": candidate.safety_flags.no_destructive_operation,
                "no_auto_activation": candidate.safety_flags.no_auto_activation,
            },
            "simulation_deltas": {
                "success_rate_delta": candidate.evidence.success_rate_delta,
                "cost_delta": candidate.evidence.cost_delta,
                "latency_delta": candidate.evidence.latency_delta,
                "human_review_rate_delta": candidate.evidence.human_review_rate_delta,
            },
        },
        "confidence": candidate.confidence,
        "risk_level": candidate.risk_level,
        "boundaries": {
            "provider_cli_execution_boundary_expansion": "requires_separate_human_approval",
            "auth_security_boundary_changes": "requires_separate_human_approval",
            "db_migrations": "limited_to_policy_proposal_v12",
            "hard_constraint_mutation": "disabled",
            "target_repository_writes": "disabled",
            "destructive_operations": "requires_separate_human_approval"
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feedback::policy_proposer::{CandidateEvidence, SafetyFlags};

    fn base_candidate() -> ProposalCandidate {
        ProposalCandidate {
            schema_version: "policy_proposal_candidate.v1".to_string(),
            candidate_id: "proposal-0001".to_string(),
            title: "Route code_generate away from cheap_executor".to_string(),
            summary: "Tier cheap_executor has a high failure rate".to_string(),
            task_domain: "code".to_string(),
            task_intent: "generate".to_string(),
            task_class: "code_generate".to_string(),
            policy_key: "tier_override:cheap_executor->balanced_worker".to_string(),
            target_tier: "balanced_worker".to_string(),
            source: "pattern_detector".to_string(),
            evidence: CandidateEvidence {
                pattern_ids: vec!["p1".to_string()],
                evidence_trace_ids: vec!["t1".to_string()],
                simulation_scenario_id: Some("sim-1".to_string()),
                actual_success_rate: Some(0.8),
                simulated_success_rate: Some(0.9),
                success_rate_delta: Some(0.1),
                actual_cost: Some(0.10),
                simulated_cost: Some(0.08),
                cost_delta: Some(-0.02),
                actual_latency_ms: Some(1000.0),
                simulated_latency_ms: Some(900.0),
                latency_delta: Some(-100.0),
                actual_human_review_rate: Some(0.2),
                simulated_human_review_rate: Some(0.1),
                human_review_rate_delta: Some(-0.1),
            },
            confidence: 0.7,
            risk_level: "medium".to_string(),
            requires_human_approval: true,
            safety_flags: SafetyFlags::all_safe(),
        }
    }

    #[test]
    fn serialize_to_request_preserves_shape() {
        let candidate = base_candidate();
        let req = serialize_candidate_to_proposal_request(&candidate);

        assert_eq!(req["task_domain"], "code");
        assert_eq!(req["task_intent"], "generate");
        assert_eq!(req["target_tier"], "balanced_worker");
        assert_eq!(req["title"], "Route code_generate away from cheap_executor");
        assert!(req["evidence"].is_object());
        assert_eq!(req["evidence"]["pattern_ids"][0], "p1");
        assert_eq!(req["evidence"]["evidence_trace_ids"][0], "t1");
    }

    #[test]
    fn serialize_to_request_includes_safety_flags() {
        let candidate = base_candidate();
        let req = serialize_candidate_to_proposal_request(&candidate);

        let flags = &req["evidence"]["safety_flags"];
        assert_eq!(flags["no_provider_cli_boundary_expansion"], true);
        assert_eq!(flags["no_auth_security_change"], true);
        assert_eq!(flags["no_db_migration_required"], true);
        assert_eq!(flags["no_hard_constraint_mutation"], true);
        assert_eq!(flags["no_target_repo_write"], true);
        assert_eq!(flags["no_destructive_operation"], true);
        assert_eq!(flags["no_auto_activation"], true);
    }

    #[test]
    fn serialize_to_response_compatible_with_proposal_schema() {
        let candidate = base_candidate();
        let resp = serialize_candidate_to_api_response(&candidate);

        assert_eq!(resp["schema_version"], "controlled_loop_policy_proposal.v1");
        assert_eq!(resp["status"], "generated");
        assert_eq!(resp["proposal_id"], "proposal-0001");
        assert_eq!(resp["task_domain"], "code");
        assert_eq!(resp["task_intent"], "generate");
        assert_eq!(resp["task_class"], "code_generate");
        assert_eq!(resp["tier"], "balanced_worker");
        assert_eq!(resp["requires_human_approval"], true);
        assert_eq!(resp["scope"], "tier_map_override");
        assert_eq!(resp["source"], "generated_from_feedback");
        assert!(resp["boundaries"].is_object());
    }

    #[test]
    fn serialize_to_response_includes_confidence() {
        let candidate = base_candidate();
        let resp = serialize_candidate_to_api_response(&candidate);

        assert_eq!(resp["confidence"], 0.7);
        assert_eq!(resp["risk_level"], "medium");
    }
}
