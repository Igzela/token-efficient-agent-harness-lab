use engine::feedback::{
    ContextualPolicyRequest, ObjectiveProfile, CONTEXTUAL_POLICY_SCHEMA_VERSION,
};
use engine::storage::local_product_store::{
    AdaptiveObservationInput, LocalProductStore, ADAPTIVE_OBSERVATION_SCHEMA_VERSION,
};
use serde_json::json;
use tempfile::tempdir;

fn input(run_id: &str, candidate_id: &str) -> AdaptiveObservationInput {
    AdaptiveObservationInput {
        schema_version: ADAPTIVE_OBSERVATION_SCHEMA_VERSION.to_string(),
        run_id: run_id.to_string(),
        request_id: format!("request-{run_id}"),
        task_class: "coding".to_string(),
        objective: ObjectiveProfile::Quality,
        risk_level: "low".to_string(),
        candidate_id: candidate_id.to_string(),
        candidate_hash: "a".repeat(64),
        policy_hash: Some("b".repeat(64)),
        candidate_kind: "fusion".to_string(),
        success: true,
        quality_score: 0.9,
        quality_score_source: "execution_success_proxy".to_string(),
        tool_success_score: 1.0,
        cost_usd: 0.08,
        latency_ms: 240,
        input_tokens: 120,
        output_tokens: 40,
    }
}

#[test]
fn persists_only_safe_summary_and_feeds_contextual_scoring() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();
    let recorded = store
        .record_adaptive_observation(&input("run-1", "candidate-a"), "operator")
        .unwrap();

    assert_eq!(recorded.sequence, 1);
    assert_eq!(recorded.candidate_id, "candidate-a");
    let serialized = serde_json::to_string(&recorded).unwrap();
    for forbidden in [
        "raw_prompt",
        "raw_output",
        "transcript",
        "repo_content",
        "sk-live-secret",
        "/home/operator/private/repo",
    ] {
        assert!(!serialized.contains(forbidden));
    }

    let request = ContextualPolicyRequest {
        schema_version: CONTEXTUAL_POLICY_SCHEMA_VERSION.to_string(),
        request_id: "request-score".to_string(),
        task_class: "coding".to_string(),
        objective: ObjectiveProfile::Quality,
        risk_level: "low".to_string(),
        exploration_seed: 0,
    };
    let observations = store
        .adaptive_contextual_observations(&request, &["candidate-a".to_string()])
        .unwrap();
    assert_eq!(observations.len(), 1);
    assert_eq!(observations[0].observation_id, recorded.observation_id);
    assert_eq!(observations[0].cost_efficiency_score, 1.0 / 1.08);
    assert_eq!(observations[0].latency_efficiency_score, 1.0 / 1.24);
    assert_eq!(
        store
            .daily_adaptive_observation_cost_usd(&recorded.created_at[..10])
            .unwrap(),
        0.08
    );
}

#[test]
fn duplicate_run_candidate_is_idempotent_but_conflicts_are_rejected() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();
    let original = input("run-1", "candidate-a");
    let first = store
        .record_adaptive_observation(&original, "operator")
        .unwrap();
    let duplicate = store
        .record_adaptive_observation(&original, "operator")
        .unwrap();

    assert_eq!(duplicate, first);
    assert_eq!(store.adaptive_observations().unwrap().len(), 1);

    let mut conflicting = original;
    conflicting.success = false;
    let error = store
        .record_adaptive_observation(&conflicting, "operator")
        .unwrap_err();
    assert_eq!(error, "adaptive observation run/candidate conflict");
}

#[test]
fn rejects_sensitive_malformed_oversized_and_unknown_candidate_data() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();

    let mut sensitive = input("run-1", "candidate-a");
    sensitive.request_id = format!("sk-{}", "x".repeat(24));
    assert_eq!(
        store
            .record_adaptive_observation(&sensitive, "operator")
            .unwrap_err(),
        "adaptive observation contains sensitive data"
    );

    let mut path_like = input("run-2", "candidate-a");
    path_like.task_class = "/home/operator/private/repo".to_string();
    assert_eq!(
        store
            .record_adaptive_observation(&path_like, "operator")
            .unwrap_err(),
        "adaptive observation identity is invalid"
    );

    let mut oversized = input("run-3", "candidate-a");
    oversized.candidate_id = "x".repeat(161);
    assert_eq!(
        store
            .record_adaptive_observation(&oversized, "operator")
            .unwrap_err(),
        "adaptive observation identity is invalid"
    );

    let mut invalid_score = input("run-4", "candidate-a");
    invalid_score.quality_score = 1.1;
    assert_eq!(
        store
            .record_adaptive_observation(&invalid_score, "operator")
            .unwrap_err(),
        "adaptive observation score is invalid"
    );

    store
        .record_adaptive_observation(&input("run-5", "candidate-a"), "operator")
        .unwrap();
    let request = ContextualPolicyRequest {
        schema_version: CONTEXTUAL_POLICY_SCHEMA_VERSION.to_string(),
        request_id: "request-score".to_string(),
        task_class: "coding".to_string(),
        objective: ObjectiveProfile::Quality,
        risk_level: "low".to_string(),
        exploration_seed: 0,
    };
    assert_eq!(
        store
            .adaptive_contextual_observations(&request, &["candidate-b".to_string()])
            .unwrap_err(),
        "adaptive observation references unknown candidate"
    );
}

#[test]
fn tampered_persisted_observation_is_ignored() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();
    let recorded = store
        .record_adaptive_observation(&input("run-1", "candidate-a"), "operator")
        .unwrap();
    let mut tampered = serde_json::to_value(recorded).unwrap();
    tampered["observation_id"] = json!("adaptive-observation-tampered");
    store
        .set_config_value(
            "adaptive_fusion_observations",
            json!([tampered]),
            "operator",
        )
        .unwrap();

    assert!(store.adaptive_observations().unwrap().is_empty());
    assert!(store.adaptive_bandit_observations().unwrap().is_empty());
}
