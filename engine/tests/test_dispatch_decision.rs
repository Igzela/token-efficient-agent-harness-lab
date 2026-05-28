use engine::dispatch_decision::*;

// ---------------------------------------------------------------------------
// EvidenceTests
// ---------------------------------------------------------------------------

#[test]
fn test_evidence_creation() {
    let e = Evidence {
        feature: "test_feature".to_string(),
        polarity: "positive".to_string(),
        ..Default::default()
    };
    assert_eq!(e.feature, "test_feature");
    assert_eq!(e.polarity, "positive");
}

#[test]
fn test_evidence_to_dict() {
    let e = Evidence {
        feature: "f".to_string(),
        text: "t".to_string(),
        span: [0, 5],
        polarity: "positive".to_string(),
        source: "raw_request".to_string(),
        ..Default::default()
    };
    let v = e.to_value();
    assert!(v.get("feature").is_some());
    assert!(v.get("span").is_some());
    assert!(v["span"].is_array());
}

#[test]
fn test_evidence_frozen() {
    let e = Evidence {
        feature: "test_feature".to_string(),
        ..Default::default()
    };
    let _ = e.feature;
}

// ---------------------------------------------------------------------------
// ShadowRouteTests
// ---------------------------------------------------------------------------

#[test]
fn test_shadow_route_creation() {
    let sr = ShadowRoute {
        tier: "balanced_worker".to_string(),
        profile_id: None,
        reason: "test".to_string(),
        admission_scope: "diagnostic".to_string(),
        ..Default::default()
    };
    assert_eq!(sr.admission_scope, "diagnostic");
}

#[test]
fn test_shadow_route_to_dict() {
    let sr = ShadowRoute::default();
    let v = sr.to_value();
    assert!(v.get("tier").is_some());
    assert!(v.get("admission_scope").is_some());
}

// ---------------------------------------------------------------------------
// BudgetReservationTests
// ---------------------------------------------------------------------------

#[test]
fn test_budget_reservation_creation() {
    let br = BudgetReservation {
        status: "reserved".to_string(),
        reserved_total_tokens: 5000,
        ..Default::default()
    };
    assert_eq!(br.status, "reserved");
    assert_eq!(br.reserved_total_tokens, 5000);
}

#[test]
fn test_budget_reservation_to_dict() {
    let br = BudgetReservation::default();
    let v = br.to_value();
    assert!(v.get("reservation_id").is_some());
    assert!(v.get("budget_violation").is_some());
}

// ---------------------------------------------------------------------------
// ExecutionGateTests
// ---------------------------------------------------------------------------

#[test]
fn test_execution_gate_creation() {
    let g = ExecutionGate {
        gate_type: "provider_disabled".to_string(),
        ..Default::default()
    };
    assert_eq!(g.gate_type, "provider_disabled");
    assert!(!g.cleared);
}

#[test]
fn test_execution_gate_to_dict() {
    let g = ExecutionGate::default();
    let v = g.to_value();
    assert!(v.get("gate_id").is_some());
    assert!(v.get("clearance_required").is_some());
}

// ---------------------------------------------------------------------------
// RejectedCandidateTests
// ---------------------------------------------------------------------------

#[test]
fn test_rejected_candidate_creation() {
    let rc = RejectedCandidate {
        tier: "cheap_executor".to_string(),
        profile_id: None,
        reason: "test".to_string(),
        ..Default::default()
    };
    assert_eq!(rc.tier, "cheap_executor");
}

#[test]
fn test_rejected_candidate_to_dict() {
    let rc = RejectedCandidate::default();
    let v = rc.to_value();
    assert!(v.get("tier").is_some());
}

// ---------------------------------------------------------------------------
// DispatchDecisionTests
// ---------------------------------------------------------------------------

#[test]
fn test_dispatch_decision_creation() {
    let dd = DispatchDecision {
        selected_tier: "balanced_worker".to_string(),
        decision_status: "decided".to_string(),
        ..Default::default()
    };
    assert_eq!(dd.selected_tier, "balanced_worker");
    assert_eq!(dd.decision_status, "decided");
}

#[test]
fn test_dispatch_decision_to_dict() {
    let dd = DispatchDecision::default();
    let v = dd.to_value();
    assert!(v.get("decision_id").is_some());
    assert!(v.get("budget_reservation").is_some());
    assert!(v["budget_reservation"].is_object());
}

#[test]
fn test_dispatch_decision_with_gates() {
    let dd = DispatchDecision {
        execution_gates: vec![ExecutionGate::default(), ExecutionGate::default()],
        ..Default::default()
    };
    assert_eq!(dd.execution_gates.len(), 2);
}

#[test]
fn test_dispatch_decision_with_shadow_routes() {
    let dd = DispatchDecision {
        shadow_routes: vec![ShadowRoute::default(), ShadowRoute::default()],
        ..Default::default()
    };
    assert_eq!(dd.shadow_routes.len(), 2);
}

// ---------------------------------------------------------------------------
// ConstantsTests
// ---------------------------------------------------------------------------

#[test]
fn test_complexity_weights_sum_to_one() {
    let weights = complexity_weights();
    let sum: f64 = weights.values().sum();
    assert!((sum - 1.0).abs() < 1e-6, "sum was {}", sum);
}

#[test]
fn test_execution_gate_types() {
    assert!(EXECUTION_GATE_TYPES.contains(&"provider_disabled"));
    assert!(EXECUTION_GATE_TYPES.contains(&"sandbox_disabled"));
    assert!(EXECUTION_GATE_TYPES.contains(&"target_write"));
}

#[test]
fn test_decision_statuses() {
    assert!(DECISION_STATUSES.contains(&"decided"));
    assert!(DECISION_STATUSES.contains(&"needs_approval"));
}
