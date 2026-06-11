mod assembly;
mod budget;
mod rules;
mod types;
mod validation;

pub use assembly::{assemble_context_injection, ContextAssemblyConfig, ContextSource};
pub use budget::{apply_prune_policy, check_budget_compliance};
pub use rules::*;
pub use types::{ContextBudget, ContextLayers, MemoryDigest, RetrievalPolicy};
pub use validation::{
    validate_advisor_context_pack_v2, validate_context_layers, validate_context_retrieval_request,
    validate_context_retrieval_result, validate_model_context_pack_v2,
};

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};
    use std::collections::HashMap;

    fn make_advisor_pack() -> HashMap<String, Value> {
        let mut d = HashMap::new();
        d.insert(
            "schema_version".to_string(),
            json!("advisor_context_pack.v2"),
        );
        d.insert("pack_id".to_string(), json!("p1"));
        d.insert("task_id".to_string(), json!("t1"));
        d.insert("item_id".to_string(), json!("i1"));
        d.insert("call_type".to_string(), json!("preflight"));
        d.insert("objective".to_string(), json!("test"));
        d.insert("current_status".to_string(), json!("active"));
        d.insert("allowed_files".to_string(), json!([]));
        d.insert("forbidden_files".to_string(), json!([]));
        d.insert("artifact_refs".to_string(), json!([]));
        d.insert("evidence_refs".to_string(), json!([]));
        d.insert("quality_signals".to_string(), json!([]));
        d.insert("budget".to_string(), json!({"max_context_tokens": 4000}));
        d.insert(
            "retrieval_policy".to_string(),
            json!({"allow_retrieval": true}),
        );
        d.insert("created_at".to_string(), json!("2026-01-01T00:00:00Z"));
        d
    }

    #[test]
    fn test_advisor_pack_valid() {
        let d = make_advisor_pack();
        assert!(validate_advisor_context_pack_v2(&d).is_empty());
    }

    #[test]
    fn test_advisor_pack_bad_call_type() {
        let mut d = make_advisor_pack();
        d.insert("call_type".to_string(), json!("bogus"));
        let errs = validate_advisor_context_pack_v2(&d);
        assert!(errs.iter().any(|e| e.contains("call_type")));
    }

    #[test]
    fn test_advisor_pack_missing_field() {
        let mut d = make_advisor_pack();
        d.remove("pack_id");
        let errs = validate_advisor_context_pack_v2(&d);
        assert!(errs[0].contains("missing required field: pack_id"));
    }

    #[test]
    fn test_model_pack_valid() {
        let mut d = HashMap::new();
        d.insert("schema_version".to_string(), json!("model_context_pack.v2"));
        d.insert("pack_id".to_string(), json!("p1"));
        d.insert("task_id".to_string(), json!("t1"));
        d.insert("item_id".to_string(), json!("i1"));
        d.insert("model_tier".to_string(), json!("balanced_worker"));
        d.insert("model_harness_profile_id".to_string(), json!("h1"));
        d.insert("role".to_string(), json!("planner"));
        d.insert("task_summary".to_string(), json!("test"));
        d.insert("allowed_tools".to_string(), json!([]));
        d.insert("forbidden_tools".to_string(), json!([]));
        d.insert("allowed_files".to_string(), json!([]));
        d.insert("forbidden_files".to_string(), json!([]));
        d.insert("artifact_refs".to_string(), json!([]));
        d.insert("evidence_refs".to_string(), json!([]));
        d.insert(
            "context_budget".to_string(),
            json!({"max_context_tokens": 4000}),
        );
        d.insert(
            "retrieval_policy".to_string(),
            json!({"allow_retrieval": true}),
        );
        d.insert("created_at".to_string(), json!("2026-01-01T00:00:00Z"));
        assert!(validate_model_context_pack_v2(&d).is_empty());
    }

    #[test]
    fn test_model_pack_bad_role() {
        let mut d = HashMap::new();
        d.insert("schema_version".to_string(), json!("model_context_pack.v2"));
        d.insert("pack_id".to_string(), json!("p1"));
        d.insert("task_id".to_string(), json!("t1"));
        d.insert("item_id".to_string(), json!("i1"));
        d.insert("model_tier".to_string(), json!("balanced_worker"));
        d.insert("model_harness_profile_id".to_string(), json!("h1"));
        d.insert("role".to_string(), json!("bogus"));
        d.insert("task_summary".to_string(), json!("test"));
        d.insert("allowed_tools".to_string(), json!([]));
        d.insert("forbidden_tools".to_string(), json!([]));
        d.insert("allowed_files".to_string(), json!([]));
        d.insert("forbidden_files".to_string(), json!([]));
        d.insert("artifact_refs".to_string(), json!([]));
        d.insert("evidence_refs".to_string(), json!([]));
        d.insert(
            "context_budget".to_string(),
            json!({"max_context_tokens": 4000}),
        );
        d.insert(
            "retrieval_policy".to_string(),
            json!({"allow_retrieval": true}),
        );
        d.insert("created_at".to_string(), json!("2026-01-01T00:00:00Z"));
        let errs = validate_model_context_pack_v2(&d);
        assert!(errs.iter().any(|e| e.contains("role")));
    }

    #[test]
    fn test_retrieval_request_valid() {
        let mut d = HashMap::new();
        d.insert("request_id".to_string(), json!("r1"));
        d.insert("requester_id".to_string(), json!("u1"));
        d.insert("requester_type".to_string(), json!("advisor"));
        d.insert("task_id".to_string(), json!("t1"));
        d.insert("reason".to_string(), json!("need context"));
        d.insert("requested_refs".to_string(), json!([]));
        d.insert("token_budget".to_string(), json!(1000));
        d.insert("priority".to_string(), json!("normal"));
        d.insert("created_at".to_string(), json!("2026-01-01T00:00:00Z"));
        assert!(validate_context_retrieval_request(&d).is_empty());
    }

    #[test]
    fn test_retrieval_request_empty_reason() {
        let mut d = HashMap::new();
        d.insert("request_id".to_string(), json!("r1"));
        d.insert("requester_id".to_string(), json!("u1"));
        d.insert("requester_type".to_string(), json!("advisor"));
        d.insert("task_id".to_string(), json!("t1"));
        d.insert("reason".to_string(), json!(""));
        d.insert("requested_refs".to_string(), json!([]));
        d.insert("token_budget".to_string(), json!(1000));
        d.insert("priority".to_string(), json!("normal"));
        d.insert("created_at".to_string(), json!("2026-01-01T00:00:00Z"));
        let errs = validate_context_retrieval_request(&d);
        assert!(errs.iter().any(|e| e.contains("non-empty")));
    }

    #[test]
    fn test_retrieval_result_valid() {
        let mut d = HashMap::new();
        d.insert("request_id".to_string(), json!("r1"));
        d.insert("result_id".to_string(), json!("res1"));
        d.insert("status".to_string(), json!("fulfilled"));
        d.insert(
            "returned_refs".to_string(),
            json!([{"ref_id": "a1", "content_mode": "summary", "token_estimate": 100}]),
        );
        d.insert("total_token_estimate".to_string(), json!(100));
        d.insert("budget_remaining".to_string(), json!(900));
        d.insert("created_at".to_string(), json!("2026-01-01T00:00:00Z"));
        assert!(validate_context_retrieval_result(&d).is_empty());
    }

    #[test]
    fn test_retrieval_result_missing_token_estimate() {
        let mut d = HashMap::new();
        d.insert("request_id".to_string(), json!("r1"));
        d.insert("result_id".to_string(), json!("res1"));
        d.insert("status".to_string(), json!("fulfilled"));
        d.insert(
            "returned_refs".to_string(),
            json!([{"ref_id": "a1", "content_mode": "summary"}]),
        );
        d.insert("total_token_estimate".to_string(), json!(0));
        d.insert("budget_remaining".to_string(), json!(1000));
        d.insert("created_at".to_string(), json!("2026-01-01T00:00:00Z"));
        let errs = validate_context_retrieval_result(&d);
        assert!(errs.iter().any(|e| e.contains("missing token_estimate")));
    }

    #[test]
    fn test_context_layers_valid() {
        let mut d = HashMap::new();
        d.insert("invariants".to_string(), json!({}));
        d.insert("task_pack".to_string(), json!({}));
        d.insert("dynamic_refs".to_string(), json!([]));
        d.insert(
            "memory_digest".to_string(),
            json!({"source_refs": [], "expiry_policy": "on_prune", "conflict_resolution": "drop"}),
        );
        d.insert("recent_evidence".to_string(), json!([]));
        assert!(validate_context_layers(&d).is_empty());
    }

    #[test]
    fn test_context_layers_missing_memory_digest_field() {
        let mut d = HashMap::new();
        d.insert("invariants".to_string(), json!({}));
        d.insert("task_pack".to_string(), json!({}));
        d.insert("dynamic_refs".to_string(), json!([]));
        d.insert("memory_digest".to_string(), json!({"source_refs": []}));
        d.insert("recent_evidence".to_string(), json!([]));
        let errs = validate_context_layers(&d);
        assert!(errs.iter().any(|e| e.contains("expiry_policy")));
    }

    #[test]
    fn test_check_budget_compliance_within() {
        let mut d = HashMap::new();
        d.insert(
            "context_budget".to_string(),
            json!({"max_context_tokens": 4000}),
        );
        let (ok, reason) = check_budget_compliance(&d, 2000);
        assert!(ok);
        assert!(reason.contains("within budget"));
    }

    #[test]
    fn test_check_budget_compliance_over() {
        let mut d = HashMap::new();
        d.insert(
            "context_budget".to_string(),
            json!({"max_context_tokens": 1000}),
        );
        let (ok, reason) = check_budget_compliance(&d, 2000);
        assert!(!ok);
        assert!(reason.contains("over budget"));
    }

    #[test]
    fn test_check_budget_compliance_no_budget() {
        let d = HashMap::new();
        let (ok, reason) = check_budget_compliance(&d, 9999);
        assert!(ok);
        assert!(reason.contains("no budget"));
    }

    #[test]
    fn test_apply_prune_policy_deny_if_over() {
        let mut d = HashMap::new();
        d.insert(
            "pack_prune_policy".to_string(),
            json!("deny_if_over_budget"),
        );
        let result = apply_prune_policy(&d, 2000, 1000);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("deny_if_over_budget"));
    }

    #[test]
    fn test_apply_prune_policy_no_pruning_needed() {
        let d = HashMap::new();
        let (_, action) = apply_prune_policy(&d, 500, 1000).unwrap();
        assert_eq!(action, "no_pruning_needed");
    }

    #[test]
    fn test_apply_prune_policy_drop_recent_evidence() {
        let mut d = HashMap::new();
        d.insert(
            "pack_prune_policy".to_string(),
            json!("drop_recent_evidence_first"),
        );
        let mut layers = serde_json::Map::new();
        layers.insert("recent_evidence".to_string(), json!([{"ref": "e1"}]));
        layers.insert("memory_digest".to_string(), json!({}));
        d.insert("context_layers".to_string(), Value::Object(layers));
        let (pruned, action) = apply_prune_policy(&d, 2000, 1000).unwrap();
        assert_eq!(action, "dropped_recent_evidence");
        let new_layers = pruned.get("context_layers").unwrap().as_object().unwrap();
        assert!(new_layers
            .get("recent_evidence")
            .unwrap()
            .as_array()
            .unwrap()
            .is_empty());
    }
}
