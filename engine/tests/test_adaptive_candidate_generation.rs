use engine::feedback::adaptive_candidate::{
    AdaptiveCandidate, AdaptiveCandidateConfig, AdaptiveCandidateGenerator,
    CandidateGenerationRequest, CandidateKind, FusionRole, ADAPTIVE_CANDIDATE_SCHEMA_VERSION,
    ADAPTIVE_CANDIDATE_SET_SCHEMA_VERSION,
};
use engine::feedback::endpoint_registry::{EndpointHealth, EndpointPricing, ModelEndpointSpec};
use engine::feedback::ENDPOINT_REGISTRY_SCHEMA_VERSION;

fn spec(
    endpoint_id: &str,
    enabled: bool,
    health_status: &str,
    health_score: f64,
    capabilities: Vec<String>,
    input_cost: f64,
    output_cost: f64,
) -> ModelEndpointSpec {
    ModelEndpointSpec {
        schema_version: ENDPOINT_REGISTRY_SCHEMA_VERSION.to_string(),
        endpoint_id: endpoint_id.to_string(),
        provider_id: format!("provider-{endpoint_id}"),
        model_id: format!("model-{endpoint_id}"),
        enabled,
        capabilities,
        context_window_tokens: 100_000,
        supports_tools: true,
        supports_parallel_tools: false,
        pricing: EndpointPricing {
            input_cost_per_1k_usd: input_cost,
            output_cost_per_1k_usd: output_cost,
            cache_read_cost_per_1k_usd: None,
            cache_write_cost_per_1k_usd: None,
        },
        health: EndpointHealth {
            status: health_status.to_string(),
            score: health_score,
            observed_at: None,
        },
        credential_reference: None,
    }
}

fn request(
    task_class: &str,
    objective: &str,
    risk_level: &str,
    capabilities: Vec<String>,
    max_cost: f64,
    max_tokens: u64,
    max_latency: u64,
) -> CandidateGenerationRequest {
    CandidateGenerationRequest {
        task_class: task_class.to_string(),
        objective: objective.to_string(),
        risk_level: risk_level.to_string(),
        required_capabilities: capabilities,
        max_estimated_cost_usd: max_cost,
        max_estimated_tokens: max_tokens,
        max_estimated_latency_ms: max_latency,
    }
}

fn default_config() -> AdaptiveCandidateConfig {
    AdaptiveCandidateConfig::default()
}

#[test]
fn deterministic_candidate_ids_and_hashes() {
    let eps = vec![
        spec(
            "ep-a",
            true,
            "healthy",
            0.9,
            vec!["code".into()],
            0.01,
            0.03,
        ),
        spec(
            "ep-b",
            true,
            "healthy",
            0.8,
            vec!["code".into()],
            0.005,
            0.015,
        ),
    ];
    let req = request(
        "coding",
        "efficient",
        "low",
        vec!["code".into()],
        1.0,
        100_000,
        10_000,
    );

    let first = AdaptiveCandidateGenerator::generate(&req, &eps, &default_config(), "snap-001");
    let second = AdaptiveCandidateGenerator::generate(&req, &eps, &default_config(), "snap-001");

    assert_eq!(first.candidates.len(), second.candidates.len());
    for (a, b) in first.candidates.iter().zip(second.candidates.iter()) {
        assert_eq!(a.candidate_id, b.candidate_id);
        assert_eq!(a.candidate_hash, b.candidate_hash);
        assert_eq!(a.estimated_cost_usd, b.estimated_cost_usd);
        assert_eq!(a.member_endpoint_ids, b.member_endpoint_ids);
    }
    assert_eq!(first.rejected_endpoints, second.rejected_endpoints);
}

#[test]
fn deterministic_with_different_registry_snapshot_produces_different_hashes() {
    let eps = vec![spec(
        "ep-a",
        true,
        "healthy",
        0.9,
        vec!["code".into()],
        0.01,
        0.03,
    )];
    let req = request(
        "coding",
        "efficient",
        "low",
        vec!["code".into()],
        1.0,
        100_000,
        10_000,
    );

    let a = AdaptiveCandidateGenerator::generate(&req, &eps, &default_config(), "snap-001");
    let b = AdaptiveCandidateGenerator::generate(&req, &eps, &default_config(), "snap-002");

    assert_ne!(a.candidates[0].candidate_id, b.candidates[0].candidate_id);
    assert_ne!(
        a.candidates[0].candidate_hash,
        b.candidates[0].candidate_hash
    );
}

#[test]
fn single_candidate_generation() {
    let eps = vec![
        spec(
            "ep-a",
            true,
            "healthy",
            0.9,
            vec!["code".into()],
            0.01,
            0.03,
        ),
        spec(
            "ep-b",
            true,
            "healthy",
            0.8,
            vec!["code".into()],
            0.005,
            0.015,
        ),
    ];
    let req = request(
        "coding",
        "efficient",
        "low",
        vec!["code".into()],
        1.0,
        100_000,
        10_000,
    );
    let result = AdaptiveCandidateGenerator::generate(&req, &eps, &default_config(), "snap-1");

    let singles: Vec<&AdaptiveCandidate> = result
        .candidates
        .iter()
        .filter(|c| c.candidate_kind == CandidateKind::Single)
        .collect();
    assert_eq!(singles.len(), 2);
    assert!(singles[0].candidate_id.starts_with("single-"));
    assert!(singles[1].candidate_id.starts_with("single-"));
    assert_eq!(singles[0].member_endpoint_ids, vec!["ep-a"]);
    assert_eq!(singles[1].member_endpoint_ids, vec!["ep-b"]);
    assert_eq!(singles[0].endpoint_bindings.len(), 1);
    assert!(singles[0].estimated_cost_usd > 0.0);
}

#[test]
fn ordered_fallback_endpoint_ordering() {
    let eps = vec![
        spec(
            "ep-a",
            true,
            "healthy",
            0.9,
            vec!["code".into()],
            0.01,
            0.03,
        ),
        spec(
            "ep-b",
            true,
            "healthy",
            0.8,
            vec!["code".into()],
            0.005,
            0.015,
        ),
        spec(
            "ep-c",
            true,
            "healthy",
            0.7,
            vec!["code".into()],
            0.001,
            0.003,
        ),
    ];
    let req = request(
        "coding",
        "efficient",
        "low",
        vec!["code".into()],
        1.0,
        100_000,
        10_000,
    );
    let result = AdaptiveCandidateGenerator::generate(&req, &eps, &default_config(), "snap-1");

    let fallbacks: Vec<&AdaptiveCandidate> = result
        .candidates
        .iter()
        .filter(|c| c.candidate_kind == CandidateKind::OrderedFallback)
        .collect();
    assert_eq!(fallbacks.len(), 1);
    let fb = fallbacks[0];
    assert!(fb.candidate_id.starts_with("fallback-"));
    assert!(fb.member_endpoint_ids.len() >= 2);
    assert_eq!(fb.member_endpoint_ids[0], "ep-a");
    assert_eq!(fb.member_endpoint_ids[1], "ep-b");
    assert_eq!(fb.endpoint_bindings.len(), fb.member_endpoint_ids.len());
    for (binding, id) in fb
        .endpoint_bindings
        .iter()
        .zip(fb.member_endpoint_ids.iter())
    {
        assert_eq!(&binding.endpoint_id, id);
        assert!(binding.fusion_role.is_none());
    }
}

#[test]
fn fusion_role_binding() {
    let eps = vec![
        spec(
            "ep-a",
            true,
            "healthy",
            0.9,
            vec!["code".into()],
            0.01,
            0.03,
        ),
        spec(
            "ep-b",
            true,
            "healthy",
            0.8,
            vec!["code".into()],
            0.005,
            0.015,
        ),
        spec(
            "ep-c",
            true,
            "healthy",
            0.7,
            vec!["code".into()],
            0.001,
            0.003,
        ),
    ];
    let req = request(
        "coding",
        "quality",
        "low",
        vec!["code".into()],
        1.0,
        100_000,
        10_000,
    );
    let result = AdaptiveCandidateGenerator::generate(&req, &eps, &default_config(), "snap-1");

    let fusions: Vec<&AdaptiveCandidate> = result
        .candidates
        .iter()
        .filter(|c| c.candidate_kind == CandidateKind::Fusion)
        .collect();
    assert_eq!(fusions.len(), 1);
    let fusion = fusions[0];
    assert!(fusion.candidate_id.starts_with("fusion-"));
    assert!(fusion.member_endpoint_ids.len() >= 3);

    let panel_count = fusion
        .endpoint_bindings
        .iter()
        .filter(|b| b.fusion_role == Some(FusionRole::Panel))
        .count();
    let judge_count = fusion
        .endpoint_bindings
        .iter()
        .filter(|b| b.fusion_role == Some(FusionRole::Judge))
        .count();
    let synth_count = fusion
        .endpoint_bindings
        .iter()
        .filter(|b| b.fusion_role == Some(FusionRole::Synthesizer))
        .count();

    assert!(panel_count >= 2, "fusion needs at least 2 panel endpoints");
    assert_eq!(judge_count, 1, "fusion needs exactly 1 judge");
    assert_eq!(synth_count, 1, "fusion needs exactly 1 synthesizer");
    assert_eq!(
        panel_count + judge_count + synth_count,
        fusion.member_endpoint_ids.len()
    );
}

#[test]
fn max_candidates_caps_total_emitted_set() {
    let eps: Vec<_> = (0..20)
        .map(|i| {
            spec(
                &format!("ep-{i}"),
                true,
                "healthy",
                0.8,
                vec!["code".into()],
                0.01,
                0.03,
            )
        })
        .collect();
    let req = request(
        "coding",
        "efficient",
        "low",
        vec!["code".into()],
        1.0,
        100_000,
        10_000,
    );
    let mut cfg = default_config();
    cfg.max_candidates = 3;
    cfg.max_fallback_endpoints = 3;
    let result = AdaptiveCandidateGenerator::generate(&req, &eps, &cfg, "snap-1");

    assert_eq!(
        result.candidates.len(),
        3,
        "max_candidates caps total candidates"
    );
    let singles: Vec<&AdaptiveCandidate> = result
        .candidates
        .iter()
        .filter(|c| c.candidate_kind == CandidateKind::Single)
        .collect();
    assert_eq!(
        singles.len(),
        3,
        "with cap=3 and 20 endpoints, 3 singles survive truncation"
    );
    assert_eq!(singles[0].member_endpoint_ids[0], "ep-0");
    assert_eq!(singles[1].member_endpoint_ids[0], "ep-1");
    assert_eq!(singles[2].member_endpoint_ids[0], "ep-10");
}

#[test]
fn max_candidates_caps_below_single_count() {
    let eps: Vec<_> = (0..5)
        .map(|i| {
            spec(
                &format!("ep-{i}"),
                true,
                "healthy",
                0.8,
                vec!["code".into()],
                0.01,
                0.03,
            )
        })
        .collect();
    let req = request(
        "coding",
        "efficient",
        "low",
        vec!["code".into()],
        1.0,
        100_000,
        10_000,
    );
    let mut cfg = default_config();
    cfg.max_candidates = 2;
    let result = AdaptiveCandidateGenerator::generate(&req, &eps, &cfg, "snap-1");

    assert_eq!(result.candidates.len(), 2, "truncated to max_candidates=2");
    for c in &result.candidates {
        assert_eq!(c.candidate_kind, CandidateKind::Single);
    }
}

#[test]
fn disabled_endpoint_rejected() {
    let eps = vec![
        spec(
            "disabled",
            false,
            "healthy",
            0.9,
            vec!["code".into()],
            0.01,
            0.03,
        ),
        spec(
            "enabled",
            true,
            "healthy",
            0.8,
            vec!["code".into()],
            0.005,
            0.015,
        ),
    ];
    let req = request(
        "coding",
        "efficient",
        "low",
        vec!["code".into()],
        1.0,
        100_000,
        10_000,
    );
    let result = AdaptiveCandidateGenerator::generate(&req, &eps, &default_config(), "snap-1");

    assert!(
        result
            .rejected_endpoints
            .iter()
            .any(|r| r.endpoint_id == "disabled" && r.reasons.contains(&"endpoint_disabled".into())),
        "disabled endpoint should be rejected"
    );
    assert!(
        result
            .candidates
            .iter()
            .any(|c| c.member_endpoint_ids.contains(&"enabled".into())),
        "enabled endpoint should be eligible"
    );
}

#[test]
fn unhealthy_endpoint_rejected() {
    let eps = vec![
        spec(
            "unhealthy",
            true,
            "unavailable",
            0.0,
            vec!["code".into()],
            0.01,
            0.03,
        ),
        spec(
            "healthy",
            true,
            "healthy",
            0.8,
            vec!["code".into()],
            0.005,
            0.015,
        ),
    ];
    let req = request(
        "coding",
        "efficient",
        "low",
        vec!["code".into()],
        1.0,
        100_000,
        10_000,
    );
    let result = AdaptiveCandidateGenerator::generate(&req, &eps, &default_config(), "snap-1");

    assert!(
        result
            .rejected_endpoints
            .iter()
            .any(|r| r.endpoint_id == "unhealthy"
                && r.reasons.contains(&"endpoint_unavailable".into())),
        "unhealthy endpoint should be rejected"
    );
    assert!(
        result
            .candidates
            .iter()
            .any(|c| c.member_endpoint_ids.contains(&"healthy".into())),
        "healthy endpoint should be eligible"
    );
}

#[test]
fn duplicate_endpoint_all_occurrences_rejected() {
    let eps = vec![
        spec("dup", true, "healthy", 0.9, vec!["code".into()], 0.01, 0.03),
        spec("dup", true, "healthy", 0.9, vec!["code".into()], 0.01, 0.03),
        spec(
            "other",
            true,
            "healthy",
            0.8,
            vec!["code".into()],
            0.005,
            0.015,
        ),
    ];
    let req = request(
        "coding",
        "efficient",
        "low",
        vec!["code".into()],
        1.0,
        100_000,
        10_000,
    );
    let result = AdaptiveCandidateGenerator::generate(&req, &eps, &default_config(), "snap-1");

    assert!(
        result
            .rejected_endpoints
            .iter()
            .filter(|r| r.endpoint_id == "dup")
            .count()
            == 2,
        "both duplicate endpoint occurrences should be rejected"
    );
    assert!(
        result
            .rejected_endpoints
            .iter()
            .all(|r| r.endpoint_id != "dup" || r.reasons.contains(&"duplicate_endpoint_id".into())),
        "every 'dup' rejection must include duplicate_endpoint_id reason"
    );
}

#[test]
fn no_candidate_contains_duplicated_endpoint_id() {
    let eps = vec![
        spec("dup", true, "healthy", 0.9, vec!["code".into()], 0.01, 0.03),
        spec("dup", true, "healthy", 0.9, vec!["code".into()], 0.01, 0.03),
        spec(
            "other",
            true,
            "healthy",
            0.8,
            vec!["code".into()],
            0.005,
            0.015,
        ),
    ];
    let req = request(
        "coding",
        "efficient",
        "low",
        vec!["code".into()],
        1.0,
        100_000,
        10_000,
    );
    let result = AdaptiveCandidateGenerator::generate(&req, &eps, &default_config(), "snap-1");

    for candidate in &result.candidates {
        assert!(
            !candidate.member_endpoint_ids.contains(&"dup".into()),
            "no candidate should contain a duplicated endpoint_id"
        );
    }
    assert!(
        result
            .candidates
            .iter()
            .any(|c| c.member_endpoint_ids.contains(&"other".into())),
        "unique endpoint 'other' should still be eligible"
    );
}

#[test]
fn missing_capability_rejected() {
    let eps = vec![spec(
        "no-tools",
        true,
        "healthy",
        0.9,
        vec!["code".into()],
        0.01,
        0.03,
    )];
    let req = request(
        "coding",
        "efficient",
        "low",
        vec!["tools".into()],
        1.0,
        100_000,
        10_000,
    );
    let result = AdaptiveCandidateGenerator::generate(&req, &eps, &default_config(), "snap-1");

    assert!(
        result
            .rejected_endpoints
            .iter()
            .any(|r| r.reasons.contains(&"missing_capability:tools".into())),
        "endpoint missing capability should be rejected"
    );
    assert!(
        result.candidates.is_empty(),
        "no candidates when no endpoint has the required capability"
    );
}

#[test]
fn over_budget_endpoint_rejected() {
    let eps = vec![spec(
        "expensive",
        true,
        "healthy",
        0.9,
        vec!["code".into()],
        10.0,
        30.0,
    )];
    let req = request(
        "coding",
        "efficient",
        "low",
        vec!["code".into()],
        0.001,
        100_000,
        10_000,
    );
    let result = AdaptiveCandidateGenerator::generate(&req, &eps, &default_config(), "snap-1");

    assert!(
        result
            .rejected_endpoints
            .iter()
            .any(|r| r.reasons.contains(&"estimated_cost_exceeds_budget".into())),
        "over-budget endpoint should be rejected"
    );
    assert!(result.candidates.is_empty());
}

#[test]
fn over_token_cap_endpoint_accepted_when_cap_is_zero() {
    let eps = vec![spec(
        "big",
        true,
        "healthy",
        0.9,
        vec!["code".into()],
        0.01,
        0.03,
    )];
    let req = request(
        "coding",
        "efficient",
        "low",
        vec!["code".into()],
        1.0,
        0,
        10_000,
    );
    let result = AdaptiveCandidateGenerator::generate(&req, &eps, &default_config(), "snap-1");

    assert!(
        !result.candidates.is_empty(),
        "token cap of 0 should not reject"
    );
}

#[test]
fn over_latency_cap_endpoint_accepted_when_cap_is_zero() {
    let eps = vec![spec(
        "slow",
        true,
        "healthy",
        0.2,
        vec!["code".into()],
        0.01,
        0.03,
    )];
    let req = request(
        "coding",
        "efficient",
        "low",
        vec!["code".into()],
        1.0,
        100_000,
        0,
    );
    let result = AdaptiveCandidateGenerator::generate(&req, &eps, &default_config(), "snap-1");

    assert!(
        !result.candidates.is_empty(),
        "latency cap of 0 should not reject"
    );
}

#[test]
fn no_provider_execution_during_generation() {
    let eps = vec![
        spec(
            "ep-a",
            true,
            "healthy",
            0.9,
            vec!["code".into()],
            0.01,
            0.03,
        ),
        spec(
            "ep-b",
            true,
            "healthy",
            0.8,
            vec!["code".into()],
            0.005,
            0.015,
        ),
    ];
    let req = request(
        "coding",
        "efficient",
        "low",
        vec!["code".into()],
        1.0,
        100_000,
        10_000,
    );
    let result = AdaptiveCandidateGenerator::generate(&req, &eps, &default_config(), "snap-1");

    assert!(
        !result.candidates.is_empty(),
        "candidates should be generated without provider calls"
    );
    assert_eq!(result.schema_version, ADAPTIVE_CANDIDATE_SET_SCHEMA_VERSION);
    for candidate in &result.candidates {
        assert_eq!(candidate.schema_version, ADAPTIVE_CANDIDATE_SCHEMA_VERSION);
    }
}

#[test]
fn all_candidate_fields_are_populated() {
    let eps = vec![
        spec(
            "ep-a",
            true,
            "healthy",
            0.9,
            vec!["code".into()],
            0.01,
            0.03,
        ),
        spec(
            "ep-b",
            true,
            "healthy",
            0.8,
            vec!["code".into()],
            0.005,
            0.015,
        ),
        spec(
            "ep-c",
            true,
            "healthy",
            0.7,
            vec!["code".into()],
            0.001,
            0.003,
        ),
    ];
    let req = request(
        "coding",
        "quality",
        "medium",
        vec!["code".into()],
        1.0,
        100_000,
        10_000,
    );
    let result = AdaptiveCandidateGenerator::generate(&req, &eps, &default_config(), "snap-1");

    for candidate in &result.candidates {
        assert!(!candidate.candidate_id.is_empty());
        assert!(!candidate.candidate_hash.is_empty());
        assert_eq!(candidate.task_class, "coding");
        assert_eq!(candidate.objective, "quality");
        assert!(!candidate.member_endpoint_ids.is_empty());
        assert!(!candidate.endpoint_bindings.is_empty());
        assert_eq!(candidate.required_capabilities, vec!["code"]);
        assert_eq!(candidate.registry_snapshot_hash, "snap-1");
        for binding in &candidate.endpoint_bindings {
            assert!(!binding.endpoint_id.is_empty());
            assert!(!binding.model_id.is_empty());
            assert!(binding.estimated_cost_usd >= 0.0);
            assert!(binding.estimated_latency_ms > 0);
        }
    }
}

#[test]
fn empty_endpoints_produces_no_candidates() {
    let req = request("coding", "efficient", "low", vec![], 1.0, 100_000, 10_000);
    let result = AdaptiveCandidateGenerator::generate(&req, &[], &default_config(), "snap-1");
    assert!(result.candidates.is_empty());
    assert!(result.rejected_endpoints.is_empty());
    assert_eq!(result.registry_snapshot_hash, "snap-1");
}

#[test]
fn fusion_not_generated_with_less_than_three_endpoints() {
    let eps = vec![
        spec(
            "ep-a",
            true,
            "healthy",
            0.9,
            vec!["code".into()],
            0.01,
            0.03,
        ),
        spec(
            "ep-b",
            true,
            "healthy",
            0.8,
            vec!["code".into()],
            0.005,
            0.015,
        ),
    ];
    let req = request(
        "coding",
        "quality",
        "low",
        vec!["code".into()],
        1.0,
        100_000,
        10_000,
    );
    let result = AdaptiveCandidateGenerator::generate(&req, &eps, &default_config(), "snap-1");

    let fusions: Vec<&AdaptiveCandidate> = result
        .candidates
        .iter()
        .filter(|c| c.candidate_kind == CandidateKind::Fusion)
        .collect();
    assert!(
        fusions.is_empty(),
        "fusion requires at least 3 eligible endpoints"
    );
}

#[test]
fn fallback_not_generated_with_single_endpoint() {
    let eps = vec![spec(
        "ep-a",
        true,
        "healthy",
        0.9,
        vec!["code".into()],
        0.01,
        0.03,
    )];
    let req = request(
        "coding",
        "efficient",
        "low",
        vec!["code".into()],
        1.0,
        100_000,
        10_000,
    );
    let result = AdaptiveCandidateGenerator::generate(&req, &eps, &default_config(), "snap-1");

    let fallbacks: Vec<&AdaptiveCandidate> = result
        .candidates
        .iter()
        .filter(|c| c.candidate_kind == CandidateKind::OrderedFallback)
        .collect();
    assert!(
        fallbacks.is_empty(),
        "fallback requires at least 2 eligible endpoints"
    );

    let singles: Vec<&AdaptiveCandidate> = result
        .candidates
        .iter()
        .filter(|c| c.candidate_kind == CandidateKind::Single)
        .collect();
    assert_eq!(
        singles.len(),
        1,
        "single candidate still generated with 1 endpoint"
    );
}

#[test]
fn fallback_supressed_when_aggregate_exceeds_cost_cap() {
    // Each single endpoint: (2000/1000)*0.01 + (1000/1000)*0.03 = 0.05 < 0.08
    // Fallback aggregate (3): 0.05+0.04+0.025 = 0.115 > 0.08
    let eps = vec![
        spec(
            "ep-a",
            true,
            "healthy",
            0.9,
            vec!["code".into()],
            0.01,
            0.03,
        ),
        spec(
            "ep-b",
            true,
            "healthy",
            0.8,
            vec!["code".into()],
            0.008,
            0.024,
        ),
        spec(
            "ep-c",
            true,
            "healthy",
            0.7,
            vec!["code".into()],
            0.005,
            0.015,
        ),
    ];
    let req = request(
        "coding",
        "efficient",
        "low",
        vec!["code".into()],
        0.08,
        100_000,
        10_000,
    );
    let result = AdaptiveCandidateGenerator::generate(&req, &eps, &default_config(), "snap-1");

    assert!(
        !result
            .candidates
            .iter()
            .any(|c| c.candidate_kind == CandidateKind::OrderedFallback),
        "fallback suppressed when aggregate cost exceeds max_estimated_cost_usd"
    );
    assert!(
        result
            .candidates
            .iter()
            .any(|c| c.candidate_kind == CandidateKind::Single),
        "individual under-budget singles should be emitted"
    );
}

#[test]
fn fusion_supressed_when_aggregate_exceeds_cost_cap() {
    // Each single endpoint: 0.05 < 0.08
    // Fusion aggregate (5 call sites): well above 0.08
    let eps = vec![
        spec(
            "ep-a",
            true,
            "healthy",
            0.9,
            vec!["code".into()],
            0.01,
            0.03,
        ),
        spec(
            "ep-b",
            true,
            "healthy",
            0.8,
            vec!["code".into()],
            0.008,
            0.024,
        ),
        spec(
            "ep-c",
            true,
            "healthy",
            0.7,
            vec!["code".into()],
            0.005,
            0.015,
        ),
    ];
    let req = request(
        "coding",
        "quality",
        "low",
        vec!["code".into()],
        0.08,
        100_000,
        10_000,
    );
    let result = AdaptiveCandidateGenerator::generate(&req, &eps, &default_config(), "snap-1");

    assert!(
        !result
            .candidates
            .iter()
            .any(|c| c.candidate_kind == CandidateKind::Fusion),
        "fusion suppressed when aggregate cost exceeds max_estimated_cost_usd"
    );
    assert!(
        result
            .candidates
            .iter()
            .any(|c| c.candidate_kind == CandidateKind::Single),
        "individual under-budget singles should be emitted"
    );
}

#[test]
fn fallback_supressed_when_aggregate_exceeds_token_cap() {
    // Single: 3000 tokens each < cap 6000
    // Fallback (3): 9000 > 6000
    let eps = vec![
        spec(
            "ep-a",
            true,
            "healthy",
            0.9,
            vec!["code".into()],
            0.01,
            0.03,
        ),
        spec(
            "ep-b",
            true,
            "healthy",
            0.8,
            vec!["code".into()],
            0.005,
            0.015,
        ),
        spec(
            "ep-c",
            true,
            "healthy",
            0.7,
            vec!["code".into()],
            0.001,
            0.003,
        ),
    ];
    let config = AdaptiveCandidateConfig {
        estimated_input_tokens: 2000,
        estimated_output_tokens: 1000,
        ..default_config()
    };
    let req = request(
        "coding",
        "efficient",
        "low",
        vec!["code".into()],
        1.0,
        6000,
        10_000,
    );
    let result = AdaptiveCandidateGenerator::generate(&req, &eps, &config, "snap-1");

    assert!(
        !result
            .candidates
            .iter()
            .any(|c| c.candidate_kind == CandidateKind::OrderedFallback),
        "fallback suppressed when aggregate tokens exceed max_estimated_tokens"
    );
    assert!(
        result
            .candidates
            .iter()
            .any(|c| c.candidate_kind == CandidateKind::Single),
        "individual under-token-cap singles should be emitted"
    );
}
