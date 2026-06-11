use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::policy_proposer::ProposalCandidate;

pub const POLICY_SNAPSHOT_SCHEMA_VERSION: &str = "policy_snapshot.v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PolicySnapshotPreview {
    pub schema_version: String,
    pub snapshot_id: String,
    pub created_at: Option<String>,
    pub actor: String,
    pub active_policy_before: Value,
    pub candidate_reference: Value,
    pub rollback_target: Value,
    pub evidence_ids: Vec<String>,
    pub safety_hash: String,
    pub preview_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PolicySnapshotRecord {
    pub schema_version: String,
    pub adjustment_id: String,
    pub snapshot_id: String,
    pub created_at: String,
    pub updated_at: String,
    pub actor: String,
    pub created_by: String,
    pub source: String,
    pub candidate_id: String,
    pub proposal_id: String,
    pub policy_key: String,
    pub active_policy_before: Value,
    pub target_tier: String,
    pub rollback_target: Value,
    pub evidence_ids: Vec<String>,
    pub safety_hash: String,
    pub status: String,
}

impl PolicySnapshotPreview {
    pub fn preview(
        active_policy_before: Value,
        candidate: &ProposalCandidate,
        created_at: Option<String>,
    ) -> Self {
        let evidence_ids = candidate.evidence.evidence_trace_ids.clone();
        let candidate_reference = json!({
            "candidate_id": candidate.candidate_id,
            "policy_key": candidate.policy_key,
            "target_tier": candidate.target_tier,
            "source": candidate.source,
        });
        let rollback_target = json!({
            "restore_active_policy": active_policy_before,
            "active_apply_available": false,
        });
        let hash_input = json!({
            "active_policy_before": rollback_target["restore_active_policy"],
            "candidate_reference": candidate_reference,
            "evidence_ids": evidence_ids,
            "preview_only": true,
        });
        let safety_hash = stable_hash(&hash_input);

        Self {
            schema_version: POLICY_SNAPSHOT_SCHEMA_VERSION.to_string(),
            snapshot_id: format!("snapshot-preview-{}", &safety_hash[..16]),
            created_at,
            actor: "auto_adjustment".to_string(),
            active_policy_before: rollback_target["restore_active_policy"].clone(),
            candidate_reference,
            rollback_target,
            evidence_ids,
            safety_hash,
            preview_only: true,
        }
    }
}

impl PolicySnapshotRecord {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        adjustment_id: String,
        snapshot_id: String,
        created_at: String,
        actor: String,
        created_by: String,
        candidate: &ProposalCandidate,
        proposal_id: String,
        active_policy_before: Value,
        restore_active_proposal_ids: Vec<String>,
    ) -> Self {
        let evidence_ids = candidate.evidence.evidence_trace_ids.clone();
        let policy_key = format!("{}_{}", candidate.task_domain, candidate.task_intent);
        let rollback_target = json!({
            "restore_active_policy": active_policy_before,
            "policy_key": policy_key,
            "proposal_id": proposal_id,
            "restore_active_proposal_ids": restore_active_proposal_ids,
        });
        let safety_hash = snapshot_safety_hash(
            &snapshot_id,
            &adjustment_id,
            &proposal_id,
            &candidate.candidate_id,
            &policy_key,
            &candidate.target_tier,
            &rollback_target["restore_active_policy"],
            rollback_target["restore_active_proposal_ids"]
                .as_array()
                .map(|values| {
                    values
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
                .as_slice(),
            &evidence_ids,
        );
        Self {
            schema_version: POLICY_SNAPSHOT_SCHEMA_VERSION.to_string(),
            adjustment_id,
            snapshot_id,
            created_at: created_at.clone(),
            updated_at: created_at,
            actor,
            created_by,
            source: "active_auto_adjustment".to_string(),
            candidate_id: candidate.candidate_id.clone(),
            proposal_id,
            policy_key,
            active_policy_before: rollback_target["restore_active_policy"].clone(),
            target_tier: candidate.target_tier.clone(),
            rollback_target,
            evidence_ids,
            safety_hash,
            status: "active".to_string(),
        }
    }

    pub fn expected_safety_hash(&self) -> String {
        snapshot_safety_hash(
            &self.snapshot_id,
            &self.adjustment_id,
            &self.proposal_id,
            &self.candidate_id,
            &self.policy_key,
            &self.target_tier,
            &self.active_policy_before,
            self.rollback_target
                .get("restore_active_proposal_ids")
                .and_then(Value::as_array)
                .map(|values| {
                    values
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
                .as_slice(),
            &self.evidence_ids,
        )
    }

    pub fn hash_is_valid(&self) -> bool {
        self.safety_hash == self.expected_safety_hash()
    }
}

pub fn snapshot_safety_hash(
    snapshot_id: &str,
    adjustment_id: &str,
    proposal_id: &str,
    candidate_id: &str,
    policy_key: &str,
    target_tier: &str,
    active_policy_before: &Value,
    restore_active_proposal_ids: &[String],
    evidence_ids: &[String],
) -> String {
    stable_hash(&json!({
        "schema_version": POLICY_SNAPSHOT_SCHEMA_VERSION,
        "snapshot_id": snapshot_id,
        "adjustment_id": adjustment_id,
        "proposal_id": proposal_id,
        "candidate_id": candidate_id,
        "policy_key": policy_key,
        "target_tier": target_tier,
        "active_policy_before": active_policy_before,
        "restore_active_proposal_ids": restore_active_proposal_ids,
        "evidence_ids": evidence_ids,
    }))
}

pub fn stable_hash(value: &Value) -> String {
    let canonical = serde_json::to_string(value).unwrap_or_else(|_| "null".to_string());
    hex::encode(Sha256::digest(canonical.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feedback::policy_proposer::{CandidateEvidence, ProposalCandidate, SafetyFlags};

    fn candidate() -> ProposalCandidate {
        ProposalCandidate {
            schema_version: "policy_proposal_candidate.v1".to_string(),
            candidate_id: "candidate-001".to_string(),
            title: "Test".to_string(),
            summary: "Test".to_string(),
            task_domain: "code".to_string(),
            task_intent: "generate".to_string(),
            task_class: "code_generate".to_string(),
            policy_key: "task_class_tier_override:code_generate->balanced_worker".to_string(),
            target_tier: "balanced_worker".to_string(),
            source: "pattern_detector".to_string(),
            evidence: CandidateEvidence {
                pattern_ids: vec!["pattern-1".to_string()],
                evidence_trace_ids: vec!["trace-1".to_string()],
                simulation_scenario_id: Some("sim-1".to_string()),
                actual_success_rate: None,
                simulated_success_rate: None,
                success_rate_delta: Some(0.1),
                actual_cost: None,
                simulated_cost: None,
                cost_delta: Some(-0.01),
                actual_latency_ms: None,
                simulated_latency_ms: None,
                latency_delta: Some(-1.0),
                actual_human_review_rate: None,
                simulated_human_review_rate: None,
                human_review_rate_delta: Some(-0.1),
            },
            confidence: 0.9,
            risk_level: "low".to_string(),
            requires_human_approval: true,
            safety_flags: SafetyFlags::all_safe(),
        }
    }

    #[test]
    fn snapshot_preview_is_deterministic_and_read_only() {
        let active = json!({"policy_id": "controlled_loop_v1"});
        let first =
            PolicySnapshotPreview::preview(active.clone(), &candidate(), Some("now".to_string()));
        let second = PolicySnapshotPreview::preview(active, &candidate(), Some("now".to_string()));
        assert_eq!(first.safety_hash, second.safety_hash);
        assert_eq!(first.schema_version, POLICY_SNAPSHOT_SCHEMA_VERSION);
        assert_eq!(first.actor, "auto_adjustment");
        assert!(first.preview_only);
        assert_eq!(first.evidence_ids, vec!["trace-1".to_string()]);
    }

    #[test]
    fn snapshot_record_hash_detects_tamper() {
        let active = json!({"policy_id": "controlled_loop_v1"});
        let mut record = PolicySnapshotRecord::new(
            "auto-adjustment-0001".to_string(),
            "policy-snapshot-0001".to_string(),
            "now".to_string(),
            "auto_adjustment".to_string(),
            "tester".to_string(),
            &candidate(),
            "policy-proposal-0001".to_string(),
            active,
            Vec::new(),
        );
        assert!(record.hash_is_valid());
        record.target_tier = "cheap_executor".to_string();
        assert!(!record.hash_is_valid());
    }
}
