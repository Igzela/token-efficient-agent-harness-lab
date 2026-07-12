use engine::operator_decision::{
    derive_operator_decision_item, OperatorDecisionAction, OperatorDecisionEvidenceReference,
    OperatorDecisionOutcome, OperatorDecisionSeverity, OperatorDecisionSource,
    OperatorDecisionSourceKind, OperatorDecisionSourceState,
    OPERATOR_DECISION_SOURCE_SCHEMA_VERSION,
};

fn source(
    kind: OperatorDecisionSourceKind,
    id: &str,
    action: OperatorDecisionAction,
) -> OperatorDecisionSource {
    let mut source = OperatorDecisionSource {
        schema_version: OPERATOR_DECISION_SOURCE_SCHEMA_VERSION.to_string(),
        source_kind: kind,
        source_id: id.to_string(),
        resource_id: "run-1".to_string(),
        conflict_key: "run-1:control".to_string(),
        action,
        state: OperatorDecisionSourceState::Actionable,
        severity: OperatorDecisionSeverity::Warning,
        confidence: 0.9,
        observed_at: "2026-07-11T00:00:00Z".to_string(),
        expires_at: Some("2026-07-11T01:00:00Z".to_string()),
        reason_codes: vec!["operator_attention_required".to_string()],
        evidence_references: vec![],
        evidence_sha256: String::new(),
    };
    source.seal().unwrap();
    source
}

#[test]
fn source_contract_round_trips_and_rejects_tamper() {
    let source = source(
        OperatorDecisionSourceKind::Approval,
        "approval-1",
        OperatorDecisionAction::Approve,
    );
    source.validate().unwrap();
    let encoded = serde_json::to_string(&source).unwrap();
    let mut decoded: OperatorDecisionSource = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded, source);
    decoded.confidence = 0.8;
    assert!(decoded.validate().unwrap_err().contains("hash mismatch"));
}

#[test]
fn source_seal_sorts_deduplicates_and_hash_binds_original_evidence() {
    let mut source = source(
        OperatorDecisionSourceKind::Workflow,
        "workflow-1",
        OperatorDecisionAction::Retry,
    );
    let first = OperatorDecisionEvidenceReference {
        evidence_type: "workflow_run".to_string(),
        evidence_id: "run-1".to_string(),
        content_sha256: None,
    };
    let second = OperatorDecisionEvidenceReference {
        evidence_type: "workflow_run_event".to_string(),
        evidence_id: "event-2".to_string(),
        content_sha256: Some("ab".repeat(32)),
    };
    source.evidence_references = vec![second.clone(), first.clone(), second.clone()];
    source.reason_codes = vec![
        "workflow_blocked_ready_node".to_string(),
        "operator_attention_required".to_string(),
        "workflow_blocked_ready_node".to_string(),
    ];
    source.seal().unwrap();
    assert_eq!(source.evidence_references, vec![first, second]);
    assert_eq!(source.reason_codes.len(), 2);

    let mut tampered = source.clone();
    tampered.evidence_references[0].evidence_id = "run-2".to_string();
    assert!(tampered.validate().unwrap_err().contains("hash mismatch"));
}

#[test]
fn precedence_deduplication_and_ordering_are_deterministic() {
    let approval = source(
        OperatorDecisionSourceKind::Approval,
        "approval-1",
        OperatorDecisionAction::Approve,
    );
    let budget = source(
        OperatorDecisionSourceKind::Budget,
        "budget-1",
        OperatorDecisionAction::Pause,
    );
    let first = derive_operator_decision_item(
        "run-1:control",
        &[budget.clone(), approval.clone(), approval.clone()],
        "2026-07-11T00:05:00Z",
        600,
    )
    .unwrap();
    let second = derive_operator_decision_item(
        "run-1:control",
        &[approval, budget],
        "2026-07-11T00:05:00Z",
        600,
    )
    .unwrap();
    assert_eq!(first, second);
    assert_eq!(first.outcome, OperatorDecisionOutcome::Ready);
    assert_eq!(
        first.recommended_action,
        Some(OperatorDecisionAction::Approve)
    );
    assert_eq!(first.evidence_references.len(), 2);
}

#[test]
fn shared_source_id_across_kinds_uses_exact_selected_identity() {
    let mut approval = source(
        OperatorDecisionSourceKind::Approval,
        "shared-1",
        OperatorDecisionAction::Approve,
    );
    approval.confidence = 0.71;
    approval.seal().unwrap();

    let mut budget = source(
        OperatorDecisionSourceKind::Budget,
        "shared-1",
        OperatorDecisionAction::Pause,
    );
    budget.confidence = 0.99;
    budget.seal().unwrap();

    let item = derive_operator_decision_item(
        "run-1:control",
        &[budget, approval],
        "2026-07-11T00:05:00Z",
        600,
    )
    .unwrap();
    assert_eq!(
        item.recommended_action,
        Some(OperatorDecisionAction::Approve)
    );
    assert_eq!(item.confidence, 0.71);
    let selected = item.selected_source.expect("selected source");
    assert_eq!(selected.evidence_type, "approval");
    assert_eq!(selected.evidence_id, "shared-1");
}

#[test]
fn equal_precedence_action_conflict_fails_closed() {
    let approve = source(
        OperatorDecisionSourceKind::Approval,
        "approval-a",
        OperatorDecisionAction::Approve,
    );
    let reject = source(
        OperatorDecisionSourceKind::Approval,
        "approval-b",
        OperatorDecisionAction::Reject,
    );
    let item = derive_operator_decision_item(
        "run-1:control",
        &[approve, reject],
        "2026-07-11T00:05:00Z",
        600,
    )
    .unwrap();
    assert_eq!(item.outcome, OperatorDecisionOutcome::Conflict);
    assert!(item.recommended_action.is_none());
    assert!(item.selected_source.is_none());
}

#[test]
fn conflicting_duplicate_identity_is_rejected_in_every_input_order() {
    let approve = source(
        OperatorDecisionSourceKind::Approval,
        "approval-1",
        OperatorDecisionAction::Approve,
    );
    let mut reject = approve.clone();
    reject.action = OperatorDecisionAction::Reject;
    reject.seal().unwrap();
    for inputs in [vec![approve.clone(), reject.clone()], vec![reject, approve]] {
        assert!(derive_operator_decision_item(
            "run-1:control",
            &inputs,
            "2026-07-11T00:05:00Z",
            600,
        )
        .unwrap_err()
        .contains("conflicting duplicate"));
    }
}

#[test]
fn conflict_key_spanning_resources_is_rejected() {
    let first = source(
        OperatorDecisionSourceKind::Workflow,
        "workflow-1",
        OperatorDecisionAction::Retry,
    );
    let mut second = source(
        OperatorDecisionSourceKind::Budget,
        "budget-1",
        OperatorDecisionAction::Pause,
    );
    second.resource_id = "run-2".to_string();
    second.seal().unwrap();
    assert!(derive_operator_decision_item(
        "run-1:control",
        &[first, second],
        "2026-07-11T00:05:00Z",
        600,
    )
    .unwrap_err()
    .contains("multiple resources"));
}

#[test]
fn expiry_staleness_low_confidence_and_missing_sources_are_explicit() {
    let mut expired = source(
        OperatorDecisionSourceKind::Workflow,
        "workflow-1",
        OperatorDecisionAction::Retry,
    );
    expired.expires_at = Some("2026-07-11T00:02:00Z".to_string());
    expired.seal().unwrap();
    assert_eq!(
        derive_operator_decision_item("run-1:control", &[expired], "2026-07-11T00:05:00Z", 600,)
            .unwrap()
            .outcome,
        OperatorDecisionOutcome::Expired
    );

    let stale = source(
        OperatorDecisionSourceKind::Workflow,
        "workflow-stale",
        OperatorDecisionAction::Retry,
    );
    assert_eq!(
        derive_operator_decision_item("run-1:control", &[stale], "2026-07-11T00:20:01Z", 600,)
            .unwrap()
            .outcome,
        OperatorDecisionOutcome::InsufficientEvidence
    );

    let mut low = source(
        OperatorDecisionSourceKind::Budget,
        "budget-low",
        OperatorDecisionAction::Pause,
    );
    low.confidence = 0.4;
    low.seal().unwrap();
    assert_eq!(
        derive_operator_decision_item("run-1:control", &[low], "2026-07-11T00:05:00Z", 600,)
            .unwrap()
            .outcome,
        OperatorDecisionOutcome::InsufficientEvidence
    );
    assert_eq!(
        derive_operator_decision_item("other:control", &[], "2026-07-11T00:05:00Z", 600)
            .unwrap()
            .outcome,
        OperatorDecisionOutcome::InsufficientEvidence
    );
}

#[test]
fn resolved_sources_never_recommend_actions() {
    let mut resolved = source(
        OperatorDecisionSourceKind::Recovery,
        "recovery-1",
        OperatorDecisionAction::Resume,
    );
    resolved.state = OperatorDecisionSourceState::Resolved;
    resolved.seal().unwrap();
    let item =
        derive_operator_decision_item("run-1:control", &[resolved], "2026-07-11T00:05:00Z", 600)
            .unwrap();
    assert_eq!(item.outcome, OperatorDecisionOutcome::Resolved);
    assert!(item.recommended_action.is_none());
    assert!(item.selected_source.is_none());
}
