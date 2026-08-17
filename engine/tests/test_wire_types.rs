use engine::wire_types::*;
use serde_json::json;

#[test]
fn test_enum_serde_roundtrip_all_variants() {
    // RequestSource
    for (variant, expected) in [
        (RequestSource::Cli, "\"cli\""),
        (RequestSource::Api, "\"api\""),
        (RequestSource::Dashboard, "\"dashboard\""),
        (RequestSource::Agent, "\"agent\""),
        (RequestSource::Workflow, "\"workflow\""),
        (RequestSource::TestFixture, "\"test_fixture\""),
    ] {
        let serialized = serde_json::to_string(&variant).unwrap();
        assert_eq!(serialized, expected);
        let deserialized: RequestSource = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized, variant);
    }

    // ModelTier
    for (variant, expected) in [
        (ModelTier::CheapExecutor, "\"cheap_executor\""),
        (ModelTier::BalancedWorker, "\"balanced_worker\""),
        (ModelTier::StrongPlanner, "\"strong_planner\""),
        (ModelTier::Verifier, "\"verifier\""),
        (ModelTier::Advisor, "\"advisor\""),
    ] {
        let serialized = serde_json::to_string(&variant).unwrap();
        assert_eq!(serialized, expected);
        let deserialized: ModelTier = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized, variant);
    }

    // TaskDomain
    for (variant, expected) in [
        (TaskDomain::Code, "\"code\""),
        (TaskDomain::Docs, "\"docs\""),
        (TaskDomain::Config, "\"config\""),
        (TaskDomain::Infra, "\"infra\""),
        (TaskDomain::Math, "\"math\""),
        (TaskDomain::Architecture, "\"architecture\""),
        (TaskDomain::RepoOps, "\"repo_ops\""),
        (TaskDomain::Governance, "\"governance\""),
        (TaskDomain::Other, "\"other\""),
    ] {
        let serialized = serde_json::to_string(&variant).unwrap();
        assert_eq!(serialized, expected);
        let deserialized: TaskDomain = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized, variant);
    }

    // TaskIntent
    for (variant, expected) in [
        (TaskIntent::Generate, "\"generate\""),
        (TaskIntent::Review, "\"review\""),
        (TaskIntent::Debug, "\"debug\""),
        (TaskIntent::Summarize, "\"summarize\""),
        (TaskIntent::Audit, "\"audit\""),
        (TaskIntent::Plan, "\"plan\""),
        (TaskIntent::Refactor, "\"refactor\""),
        (TaskIntent::Compare, "\"compare\""),
        (TaskIntent::Explain, "\"explain\""),
        (TaskIntent::Classify, "\"classify\""),
    ] {
        let serialized = serde_json::to_string(&variant).unwrap();
        assert_eq!(serialized, expected);
        let deserialized: TaskIntent = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized, variant);
    }

    // RiskFlag
    for (variant, expected) in [
        (RiskFlag::TargetWrite, "\"target_write\""),
        (RiskFlag::ProviderCall, "\"provider_call\""),
        (RiskFlag::SandboxExecution, "\"sandbox_execution\""),
        (RiskFlag::Deployment, "\"deployment\""),
        (RiskFlag::SecretHandling, "\"secret_handling\""),
        (RiskFlag::DestructiveOperation, "\"destructive_operation\""),
        (RiskFlag::LongContext, "\"long_context\""),
        (RiskFlag::HighUncertainty, "\"high_uncertainty\""),
    ] {
        let serialized = serde_json::to_string(&variant).unwrap();
        assert_eq!(serialized, expected);
        let deserialized: RiskFlag = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized, variant);
    }

    // QualityRequirement
    for (variant, expected) in [
        (QualityRequirement::Draft, "\"draft\""),
        (QualityRequirement::Standard, "\"standard\""),
        (QualityRequirement::High, "\"high\""),
        (QualityRequirement::Critical, "\"critical\""),
    ] {
        let serialized = serde_json::to_string(&variant).unwrap();
        assert_eq!(serialized, expected);
        let deserialized: QualityRequirement = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized, variant);
    }

    // RiskLevel
    for (variant, expected) in [
        (RiskLevel::Low, "\"low\""),
        (RiskLevel::Medium, "\"medium\""),
        (RiskLevel::High, "\"high\""),
        (RiskLevel::Critical, "\"critical\""),
    ] {
        let serialized = serde_json::to_string(&variant).unwrap();
        assert_eq!(serialized, expected);
        let deserialized: RiskLevel = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized, variant);
    }

    // ConfidenceLabel
    for (variant, expected) in [
        (ConfidenceLabel::Low, "\"low\""),
        (ConfidenceLabel::Medium, "\"medium\""),
        (ConfidenceLabel::High, "\"high\""),
    ] {
        let serialized = serde_json::to_string(&variant).unwrap();
        assert_eq!(serialized, expected);
        let deserialized: ConfidenceLabel = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized, variant);
    }

    // EvidencePolarity
    for (variant, expected) in [
        (EvidencePolarity::Positive, "\"positive\""),
        (EvidencePolarity::Negative, "\"negative\""),
    ] {
        let serialized = serde_json::to_string(&variant).unwrap();
        assert_eq!(serialized, expected);
        let deserialized: EvidencePolarity = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized, variant);
    }

    // EvidenceSource
    for (variant, expected) in [
        (EvidenceSource::RawRequest, "\"raw_request\""),
        (EvidenceSource::RepoContext, "\"repo_context\""),
        (EvidenceSource::UserConstraints, "\"user_constraints\""),
        (EvidenceSource::TargetMetadata, "\"target_metadata\""),
    ] {
        let serialized = serde_json::to_string(&variant).unwrap();
        assert_eq!(serialized, expected);
        let deserialized: EvidenceSource = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized, variant);
    }

    // ExpectedQualityBand
    for (variant, expected) in [
        (ExpectedQualityBand::Low, "\"low\""),
        (ExpectedQualityBand::Medium, "\"medium\""),
        (ExpectedQualityBand::High, "\"high\""),
        (ExpectedQualityBand::Unknown, "\"unknown\""),
    ] {
        let serialized = serde_json::to_string(&variant).unwrap();
        assert_eq!(serialized, expected);
        let deserialized: ExpectedQualityBand = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized, variant);
    }

    // DecisionStatus
    for (variant, expected) in [
        (DecisionStatus::Decided, "\"decided\""),
        (DecisionStatus::NeedsApproval, "\"needs_approval\""),
        (DecisionStatus::Blocked, "\"blocked\""),
        (DecisionStatus::DiagnosticOnly, "\"diagnostic_only\""),
    ] {
        let serialized = serde_json::to_string(&variant).unwrap();
        assert_eq!(serialized, expected);
        let deserialized: DecisionStatus = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized, variant);
    }

    // GateSeverity
    for (variant, expected) in [
        (GateSeverity::Info, "\"info\""),
        (GateSeverity::Warning, "\"warning\""),
        (GateSeverity::Block, "\"block\""),
        (GateSeverity::Critical, "\"critical\""),
    ] {
        let serialized = serde_json::to_string(&variant).unwrap();
        assert_eq!(serialized, expected);
        let deserialized: GateSeverity = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized, variant);
    }

    // ExecutorType
    for (variant, expected) in [
        (ExecutorType::Noop, "\"noop\""),
        (ExecutorType::Mock, "\"mock\""),
        (ExecutorType::Manual, "\"manual\""),
        (ExecutorType::Provider, "\"provider\""),
        (ExecutorType::ClaudeCodeCli, "\"claude_code_cli\""),
        (ExecutorType::CodexCli, "\"codex_cli\""),
    ] {
        let serialized = serde_json::to_string(&variant).unwrap();
        assert_eq!(serialized, expected);
        let deserialized: ExecutorType = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized, variant);
    }

    // ExecutionStatus
    for (variant, expected) in [
        (ExecutionStatus::NotExecuted, "\"not_executed\""),
        (ExecutionStatus::PreviewGenerated, "\"preview_generated\""),
        (ExecutionStatus::MockCompleted, "\"mock_completed\""),
        (ExecutionStatus::ManualPending, "\"manual_pending\""),
        (ExecutionStatus::ManualCompleted, "\"manual_completed\""),
        (ExecutionStatus::Failed, "\"failed\""),
        (ExecutionStatus::CliCompleted, "\"cli_completed\""),
        (ExecutionStatus::ProviderCompleted, "\"provider_completed\""),
    ] {
        let serialized = serde_json::to_string(&variant).unwrap();
        assert_eq!(serialized, expected);
        let deserialized: ExecutionStatus = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized, variant);
    }

    // EvaluationStatus
    for (variant, expected) in [
        (EvaluationStatus::Pass, "\"pass\""),
        (EvaluationStatus::Fail, "\"fail\""),
        (EvaluationStatus::NeedsHumanReview, "\"needs_human_review\""),
        (EvaluationStatus::NotEvaluated, "\"not_evaluated\""),
    ] {
        let serialized = serde_json::to_string(&variant).unwrap();
        assert_eq!(serialized, expected);
        let deserialized: EvaluationStatus = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized, variant);
    }

    // CheckStatus
    for (variant, expected) in [
        (CheckStatus::Pass, "\"pass\""),
        (CheckStatus::Fail, "\"fail\""),
        (CheckStatus::Warning, "\"warning\""),
        (CheckStatus::Skipped, "\"skipped\""),
    ] {
        let serialized = serde_json::to_string(&variant).unwrap();
        assert_eq!(serialized, expected);
        let deserialized: CheckStatus = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized, variant);
    }

    // FinalStatus
    for (variant, expected) in [
        (FinalStatus::Dispatched, "\"dispatched\""),
        (FinalStatus::Executing, "\"executing\""),
        (FinalStatus::Completed, "\"completed\""),
        (FinalStatus::Failed, "\"failed\""),
        (FinalStatus::Escalated, "\"escalated\""),
        (FinalStatus::Cancelled, "\"cancelled\""),
        (FinalStatus::NotExecuted, "\"not_executed\""),
        (FinalStatus::ManualPending, "\"manual_pending\""),
    ] {
        let serialized = serde_json::to_string(&variant).unwrap();
        assert_eq!(serialized, expected);
        let deserialized: FinalStatus = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized, variant);
    }
}

#[test]
fn test_enum_fail_closed_on_invalid_variant() {
    assert!(serde_json::from_str::<RequestSource>("\"unknown_source\"").is_err());
    assert!(serde_json::from_str::<ModelTier>("\"super_intelligence\"").is_err());
    assert!(serde_json::from_str::<TaskDomain>("\"quantum_computing\"").is_err());
    assert!(serde_json::from_str::<FinalStatus>("\"pending_approval\"").is_err());
}

#[test]
fn test_dispatch_request_serde_roundtrip() {
    let req = DispatchRequest {
        schema_version: "dispatch_request.v1".to_string(),
        raw_request: "Refactor auth middleware".to_string(),
        request_source: RequestSource::Cli,
    };
    let json_str = serde_json::to_string(&req).unwrap();
    let decoded: DispatchRequest = serde_json::from_str(&json_str).unwrap();
    assert_eq!(decoded, req);
}

#[test]
fn test_task_analysis_serde_roundtrip() {
    let evidence = Evidence {
        feature: "auth_keyword".to_string(),
        text: "auth".to_string(),
        span: (9, 13),
        polarity: EvidencePolarity::Positive,
        source: EvidenceSource::RawRequest,
        rule_id: Some("rule_auth_01".to_string()),
        confidence: 0.95,
        negation_scope: None,
    };

    let analysis = TaskAnalysis {
        schema_version: "task_analysis.v1".to_string(),
        analysis_id: "ta_12345".to_string(),
        raw_request_snapshot: "Refactor auth middleware".to_string(),
        request_source: RequestSource::Cli,
        primary_task_type: "refactoring".to_string(),
        task_domain: TaskDomain::Code,
        task_intent: TaskIntent::Refactor,
        risk_flags: vec![RiskFlag::TargetWrite],
        complexity_score: 0.45,
        cognitive_complexity: 0.5,
        context_complexity: 0.4,
        execution_risk: 0.3,
        ambiguity_score: 0.1,
        required_capabilities: vec!["rust".to_string(), "ast".to_string()],
        context_budget_estimate: 8000,
        execution_budget_estimate: 4000,
        quality_requirement: QualityRequirement::High,
        risk_level: RiskLevel::Medium,
        confidence: 0.92,
        confidence_label: ConfidenceLabel::High,
        uncertainty_reason: vec![],
        safe_default: "review_first".to_string(),
        escalation_trigger: None,
        positive_evidence: vec![evidence.clone()],
        negative_evidence: vec![],
        features_detected: json!({"has_rust_code": true}),
        analysis_method: "rule_only".to_string(),
        created_at: "2026-08-17T07:00:00Z".to_string(),
    };

    let json_str = serde_json::to_string(&analysis).unwrap();
    let decoded: TaskAnalysis = serde_json::from_str(&json_str).unwrap();
    assert_eq!(decoded, analysis);
}

#[test]
fn test_dispatch_decision_and_bundle_roundtrip() {
    let reservation = BudgetReservation {
        schema_version: "budget_reservation.v1".to_string(),
        reservation_id: "res_001".to_string(),
        decision_id: "dec_001".to_string(),
        currency: "USD".to_string(),
        pricing_snapshot_id: Some("price_snap_01".to_string()),
        pre_budget: 10000,
        reserved_input_tokens: 4000,
        reserved_output_tokens: 2000,
        reserved_total_tokens: 6000,
        reserved_cost: 0.05,
        budget_policy_id: Some("policy_std".to_string()),
        budget_gate: None,
        status: "reserved".to_string(),
        actual_usage_ref: None,
        budget_delta: None,
        budget_violation: false,
        created_at: "2026-08-17T07:00:00Z".to_string(),
        updated_at: "2026-08-17T07:00:00Z".to_string(),
        expires_at: Some("2026-08-17T08:00:00Z".to_string()),
    };

    let gate = ExecutionGate {
        gate_id: "gate_target_write".to_string(),
        gate_type: "approval".to_string(),
        severity: GateSeverity::Warning,
        reason: "Modifies target repository code".to_string(),
        evidence_refs: vec!["ev_001".to_string()],
        clearance_required: "user_confirmation".to_string(),
        cleared: true,
        cleared_by: Some("operator".to_string()),
        cleared_at: Some("2026-08-17T07:01:00Z".to_string()),
    };

    let decision = DispatchDecision {
        schema_version: "dispatch_decision.v1".to_string(),
        decision_id: "dec_001".to_string(),
        analysis_id: "ta_12345".to_string(),
        analysis_snapshot: json!({"domain": "code"}),
        selected_tier: ModelTier::StrongPlanner,
        selected_profile_id: Some("claude_opus".to_string()),
        fallback_tier: ModelTier::BalancedWorker,
        fallback_profile_id: Some("claude_sonnet".to_string()),
        shadow_routes: vec![ShadowRoute {
            tier: ModelTier::BalancedWorker,
            profile_id: Some("claude_sonnet".to_string()),
            reason: "Cost comparison".to_string(),
            admission_scope: "shadow_eval".to_string(),
            estimated_cost: Some(0.02),
            expected_tradeoff: "lower latency".to_string(),
        }],
        hard_constraints: vec!["budget_under_1_usd".to_string()],
        rejected_candidates: vec![RejectedCandidate {
            tier: ModelTier::CheapExecutor,
            profile_id: Some("gpt4o_mini".to_string()),
            reason: "Insufficient cognitive complexity capacity".to_string(),
            constraint_failed: Some("complexity_gate".to_string()),
            estimated_cost: Some(0.005),
        }],
        no_shadow_route_reason: None,
        max_input_tokens: 8000,
        max_output_tokens: 4000,
        routing_reason: "High cognitive complexity requires strong planner".to_string(),
        quality_requirement: QualityRequirement::High,
        expected_quality_band: ExpectedQualityBand::High,
        confidence: 0.94,
        confidence_label: ConfidenceLabel::High,
        budget_reservation: reservation.clone(),
        execution_policy: json!({"retry_budget": 2}),
        execution_gates: vec![gate.clone()],
        routing_mode: "standard".to_string(),
        routing_experiment_id: None,
        decision_status: DecisionStatus::Decided,
        created_at: "2026-08-17T07:00:00Z".to_string(),
    };

    let execution_result = ExecutionResult {
        schema_version: "execution_result.v1".to_string(),
        result_id: "res_exec_001".to_string(),
        dispatch_id: "disp_001".to_string(),
        decision_id: "dec_001".to_string(),
        executor_type: ExecutorType::ClaudeCodeCli,
        status: ExecutionStatus::CliCompleted,
        output: Some("Patch generated and applied".to_string()),
        prompt_pack: Some(json!({"version": 1})),
        input_tokens: Some(3500),
        output_tokens: Some(1800),
        estimated_cost: Some(0.042),
        latency_ms: Some(1250),
        error_domain: None,
        error_message: None,
        provider_request_id: Some("req_xyz123".to_string()),
        attempt_number: Some(1),
        finish_reason: Some("stop".to_string()),
        usage_source: Some("cli_report".to_string()),
        created_at: "2026-08-17T07:02:00Z".to_string(),
    };

    let evaluation_result = EvaluationResult {
        schema_version: "evaluation_result.v1".to_string(),
        evaluation_id: "eval_001".to_string(),
        dispatch_id: "disp_001".to_string(),
        decision_id: "dec_001".to_string(),
        execution_result_id: "res_exec_001".to_string(),
        status: EvaluationStatus::Pass,
        checks: vec![EvaluationCheck {
            check_id: "chk_01".to_string(),
            name: "cargo_check".to_string(),
            status: CheckStatus::Pass,
            reason: "Passed without errors".to_string(),
        }],
        quality_score: Some(0.98),
        requires_retry: false,
        retry_reason: None,
        created_at: "2026-08-17T07:03:00Z".to_string(),
    };

    let record = DispatchRecord {
        schema_version: "dispatch_record.v1".to_string(),
        dispatch_id: "disp_001".to_string(),
        request_snapshot: "Refactor auth middleware".to_string(),
        task_analysis_id: "ta_12345".to_string(),
        decision_id: "dec_001".to_string(),
        execution_result_id: Some("res_exec_001".to_string()),
        evaluation_result_id: Some("eval_001".to_string()),
        usage_ledger_row_id: Some("ledger_001".to_string()),
        budget_reservation_id: Some("res_001".to_string()),
        final_status: FinalStatus::Completed,
        created_at: "2026-08-17T07:00:00Z".to_string(),
        updated_at: "2026-08-17T07:03:00Z".to_string(),
    };

    let analysis = TaskAnalysis {
        schema_version: "task_analysis.v1".to_string(),
        analysis_id: "ta_12345".to_string(),
        raw_request_snapshot: "Refactor auth middleware".to_string(),
        request_source: RequestSource::Cli,
        primary_task_type: "refactoring".to_string(),
        task_domain: TaskDomain::Code,
        task_intent: TaskIntent::Refactor,
        risk_flags: vec![RiskFlag::TargetWrite],
        complexity_score: 0.45,
        cognitive_complexity: 0.5,
        context_complexity: 0.4,
        execution_risk: 0.3,
        ambiguity_score: 0.1,
        required_capabilities: vec!["rust".to_string()],
        context_budget_estimate: 8000,
        execution_budget_estimate: 4000,
        quality_requirement: QualityRequirement::High,
        risk_level: RiskLevel::Medium,
        confidence: 0.92,
        confidence_label: ConfidenceLabel::High,
        uncertainty_reason: vec![],
        safe_default: "review_first".to_string(),
        escalation_trigger: None,
        positive_evidence: vec![],
        negative_evidence: vec![],
        features_detected: json!({}),
        analysis_method: "rule_only".to_string(),
        created_at: "2026-08-17T07:00:00Z".to_string(),
    };

    let bundle = DispatchBundle {
        record: record.clone(),
        analysis: analysis.clone(),
        decision: decision.clone(),
        execution_result: execution_result.clone(),
        evaluation_result: evaluation_result.clone(),
    };

    let bundle_json = serde_json::to_string(&bundle).unwrap();
    let decoded_bundle: DispatchBundle = serde_json::from_str(&bundle_json).unwrap();
    assert_eq!(decoded_bundle.record, record);
    assert_eq!(decoded_bundle.analysis, analysis);
    assert_eq!(decoded_bundle.decision, decision);
    assert_eq!(decoded_bundle.execution_result, execution_result);
    assert_eq!(decoded_bundle.evaluation_result, evaluation_result);
}

#[test]
fn test_forward_compatibility_extra_fields_ignored() {
    let json_with_extra = json!({
        "schema_version": "dispatch_request.v1",
        "raw_request": "Execute task",
        "request_source": "api",
        "future_routing_tag": "v2_preview",
        "unrecognized_int": 42
    });

    let decoded: Result<DispatchRequest, _> = serde_json::from_value(json_with_extra);
    assert!(decoded.is_ok());
    let req = decoded.unwrap();
    assert_eq!(req.raw_request, "Execute task");
    assert_eq!(req.request_source, RequestSource::Api);
}

#[test]
fn test_api_status_serde_roundtrip() {
    let status = ApiStatus {
        schema_version: "axum_api.v1".to_string(),
        status: "healthy".to_string(),
        tenant_id: Some("org_main".to_string()),
    };

    let json_str = serde_json::to_string(&status).unwrap();
    let decoded: ApiStatus = serde_json::from_str(&json_str).unwrap();
    assert_eq!(decoded, status);
}
