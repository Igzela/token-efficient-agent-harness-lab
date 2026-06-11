use serde_json::Value;

use super::policy_proposer::ProposalCandidate;
use crate::dispatch_decision::{TASK_DOMAINS, TASK_INTENTS};
use crate::model_selector::is_safe_policy_override_tier;

const MIN_GENERATED_CONFIDENCE: f64 = 0.5;

// ---------------------------------------------------------------------------
// ValidationResult
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct ValidationResult {
    pub valid: bool,
    pub errors: Vec<String>,
}

impl ValidationResult {
    fn ok() -> Self {
        Self {
            valid: true,
            errors: Vec::new(),
        }
    }

    fn fail(errors: Vec<String>) -> Self {
        Self {
            valid: false,
            errors,
        }
    }
}

// ---------------------------------------------------------------------------
// ProposalValidator
// ---------------------------------------------------------------------------

pub struct ProposalValidator;

impl ProposalValidator {
    pub fn validate_generated(candidate: &ProposalCandidate) -> ValidationResult {
        let mut errors = Vec::new();

        if !is_safe_policy_override_tier(&candidate.target_tier) {
            errors.push(format!(
                "target_tier '{}' is not a safe policy override tier",
                candidate.target_tier
            ));
        }

        if !TASK_DOMAINS.contains(&candidate.task_domain.as_str()) {
            errors.push(format!(
                "task_domain '{}' is not in TASK_DOMAINS",
                candidate.task_domain
            ));
        }

        if !TASK_INTENTS.contains(&candidate.task_intent.as_str()) {
            errors.push(format!(
                "task_intent '{}' is not in TASK_INTENTS",
                candidate.task_intent
            ));
        }

        if candidate.evidence.pattern_ids.is_empty()
            && candidate.evidence.evidence_trace_ids.is_empty()
        {
            errors.push(
                "evidence must have at least one pattern_id or evidence_trace_id".to_string(),
            );
        }

        if candidate.confidence < MIN_GENERATED_CONFIDENCE {
            errors.push(format!(
                "confidence {} is below minimum {}",
                candidate.confidence, MIN_GENERATED_CONFIDENCE
            ));
        }

        if !candidate.requires_human_approval {
            errors.push("requires_human_approval must be true".to_string());
        }

        if !candidate.safety_flags.no_auto_activation {
            errors.push("safety_flags.no_auto_activation must be true".to_string());
        }

        if errors.is_empty() {
            ValidationResult::ok()
        } else {
            ValidationResult::fail(errors)
        }
    }

    pub fn validate_create_request(request: &Value) -> ValidationResult {
        let mut errors = Vec::new();

        let domain = request
            .get("task_domain")
            .and_then(|v| v.as_str())
            .map(String::from)
            .or_else(|| {
                request
                    .get("task_class")
                    .and_then(|v| v.as_str())
                    .and_then(|tc| tc.split_once('_'))
                    .map(|(d, _)| d.to_string())
            });

        let intent = request
            .get("task_intent")
            .and_then(|v| v.as_str())
            .map(String::from)
            .or_else(|| {
                request
                    .get("task_class")
                    .and_then(|v| v.as_str())
                    .and_then(|tc| tc.split_once('_'))
                    .map(|(_, i)| i.to_string())
            });

        if domain.is_none() {
            errors.push("missing task_domain (and no parseable task_class)".to_string());
        }

        if intent.is_none() {
            errors.push("missing task_intent (and no parseable task_class)".to_string());
        }

        let tier = request.get("target_tier").and_then(|v| v.as_str());

        if tier.is_none() {
            errors.push("missing target_tier".to_string());
        }

        if let Some(t) = tier {
            if !is_safe_policy_override_tier(t) {
                errors.push(format!(
                    "target_tier '{}' is not a safe policy override tier",
                    t
                ));
            }
        }

        if let Some(ref d) = domain {
            if !TASK_DOMAINS.contains(&d.as_str()) {
                errors.push(format!("task_domain '{}' is not in TASK_DOMAINS", d));
            }
        }

        if let Some(ref i) = intent {
            if !TASK_INTENTS.contains(&i.as_str()) {
                errors.push(format!("task_intent '{}' is not in TASK_INTENTS", i));
            }
        }

        if errors.is_empty() {
            ValidationResult::ok()
        } else {
            ValidationResult::fail(errors)
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feedback::policy_proposer::{CandidateEvidence, SafetyFlags};

    fn base_candidate() -> ProposalCandidate {
        ProposalCandidate {
            schema_version: "policy_proposal_candidate.v1".to_string(),
            candidate_id: "test-0001".to_string(),
            title: "Test".to_string(),
            summary: "Test".to_string(),
            task_domain: "code".to_string(),
            task_intent: "generate".to_string(),
            task_class: "code_generate".to_string(),
            policy_key: "tier_override:cheap_executor->balanced_worker".to_string(),
            target_tier: "balanced_worker".to_string(),
            source: "pattern_detector".to_string(),
            evidence: CandidateEvidence {
                pattern_ids: vec!["p1".to_string()],
                evidence_trace_ids: vec![],
                simulation_scenario_id: None,
                actual_success_rate: None,
                simulated_success_rate: None,
                success_rate_delta: None,
                actual_cost: None,
                simulated_cost: None,
                cost_delta: None,
                actual_latency_ms: None,
                simulated_latency_ms: None,
                latency_delta: None,
                actual_human_review_rate: None,
                simulated_human_review_rate: None,
                human_review_rate_delta: None,
            },
            confidence: 0.7,
            risk_level: "medium".to_string(),
            requires_human_approval: true,
            safety_flags: SafetyFlags::all_safe(),
        }
    }

    #[test]
    fn validate_generated_rejects_unsafe_tier() {
        let mut c = base_candidate();
        c.target_tier = "codex_cli".to_string();
        let r = ProposalValidator::validate_generated(&c);
        assert!(!r.valid);
        assert!(r
            .errors
            .iter()
            .any(|e| e.contains("safe policy override tier")));
    }

    #[test]
    fn validate_generated_rejects_unsupported_domain() {
        let mut c = base_candidate();
        c.task_domain = "invalid_domain".to_string();
        let r = ProposalValidator::validate_generated(&c);
        assert!(!r.valid);
        assert!(r.errors.iter().any(|e| e.contains("task_domain")));
    }

    #[test]
    fn validate_generated_rejects_unsupported_intent() {
        let mut c = base_candidate();
        c.task_intent = "invalid_intent".to_string();
        let r = ProposalValidator::validate_generated(&c);
        assert!(!r.valid);
        assert!(r.errors.iter().any(|e| e.contains("task_intent")));
    }

    #[test]
    fn validate_generated_rejects_missing_evidence() {
        let mut c = base_candidate();
        c.evidence.pattern_ids = vec![];
        c.evidence.evidence_trace_ids = vec![];
        let r = ProposalValidator::validate_generated(&c);
        assert!(!r.valid);
        assert!(r.errors.iter().any(|e| e.contains("evidence")));
    }

    #[test]
    fn validate_generated_accepts_valid_candidate() {
        let c = base_candidate();
        let r = ProposalValidator::validate_generated(&c);
        assert!(r.valid, "expected valid, got errors: {:?}", r.errors);
        assert!(r.errors.is_empty());
    }

    #[test]
    fn validate_generated_rejects_low_confidence() {
        let mut c = base_candidate();
        c.confidence = 0.3;
        let r = ProposalValidator::validate_generated(&c);
        assert!(!r.valid);
        assert!(r.errors.iter().any(|e| e.contains("confidence")));
    }

    #[test]
    fn validate_create_request_rejects_missing_domain() {
        let req = serde_json::json!({
            "task_intent": "generate",
            "target_tier": "balanced_worker"
        });
        let r = ProposalValidator::validate_create_request(&req);
        assert!(!r.valid);
        assert!(r.errors.iter().any(|e| e.contains("task_domain")));
    }

    #[test]
    fn validate_create_request_accepts_valid() {
        let req = serde_json::json!({
            "task_domain": "code",
            "task_intent": "generate",
            "target_tier": "balanced_worker"
        });
        let r = ProposalValidator::validate_create_request(&req);
        assert!(r.valid, "expected valid, got errors: {:?}", r.errors);
        assert!(r.errors.is_empty());
    }
}
