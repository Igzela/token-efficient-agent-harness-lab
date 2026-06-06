use engine::dispatch_engine::DispatchEngine;

// Basic tests
#[test]
fn test_dispatch_returns_valid_json() {
    let engine = DispatchEngine::new();
    let v = engine.dispatch("Summarize the README", "test_fixture");
    assert!(v.get("record").is_some());
    assert!(v.get("analysis").is_some());
    assert!(v.get("decision").is_some());
}

#[test]
fn test_dispatch_record_has_dispatch_id() {
    let engine = DispatchEngine::new();
    let v = engine.dispatch("Summarize the README", "test_fixture");
    assert!(!v["record"]["dispatch_id"].as_str().unwrap().is_empty());
}

#[test]
fn test_dispatch_final_status_not_executed() {
    let engine = DispatchEngine::new();
    let v = engine.dispatch("Summarize the README", "test_fixture");
    assert_eq!(v["record"]["final_status"], "not_executed");
}

#[test]
fn test_dispatch_populates_record_fields() {
    let engine = DispatchEngine::new();
    let v = engine.dispatch("Test request", "test_fixture");
    assert!(!v["record"]["task_analysis_id"].as_str().unwrap().is_empty());
    assert!(!v["record"]["decision_id"].as_str().unwrap().is_empty());
    assert!(!v["record"]["budget_reservation_id"]
        .as_str()
        .unwrap()
        .is_empty());
}

#[test]
fn test_dispatch_bundle_contains_full_chain() {
    let engine = DispatchEngine::new();
    let bundle = engine.dispatch_bundle("Summarize the README", "test_fixture");
    let analysis_id = bundle.analysis["analysis_id"].as_str().unwrap();
    let decision_analysis_id = bundle.decision["analysis_id"].as_str().unwrap();
    assert_eq!(analysis_id, decision_analysis_id);
}

// Safety invariant tests
#[test]
fn test_no_provider_executor_type() {
    let engine = DispatchEngine::new();
    let v = engine.dispatch("Test request", "test_fixture");
    assert_ne!(v["execution_result"]["executor_type"], "provider");
}

#[test]
fn test_multi_executor_without_cli_tier_reports_noop_policy() {
    let engine = DispatchEngine::with_multi_executor(engine::cli::MultiExecutor::new(
        std::collections::HashMap::new(),
    ));
    let v = engine.dispatch(
        "Generate code: function parseCsv(input: string): string[][].",
        "test_fixture",
    );

    assert_eq!(v["decision"]["selected_tier"], "codex_cli");
    assert_eq!(v["decision"]["execution_policy"]["executor_type"], "noop");
    assert_eq!(v["execution_result"]["executor_type"], "noop");
    let constraints = v["decision"]["hard_constraints"].as_array().unwrap();
    assert!(constraints
        .iter()
        .any(|value| value.as_str() == Some("no_provider_call")));
}

#[test]
fn test_decision_has_shadow_routes_or_reason() {
    let engine = DispatchEngine::new();
    let v = engine.dispatch("Summarize the README", "test_fixture");
    let has_routes = v["decision"]["shadow_routes"]
        .as_array()
        .is_some_and(|a| !a.is_empty());
    let has_reason = v["decision"]["no_shadow_route_reason"].as_str().is_some();
    assert!(has_routes || has_reason);
}

#[test]
fn test_budget_exists_before_execution() {
    let engine = DispatchEngine::new();
    let v = engine.dispatch("Test request", "test_fixture");
    assert!(v["record"]["budget_reservation_id"].as_str().is_some());
    assert!(v["decision"]["budget_reservation"].is_object());
}

#[test]
fn test_execution_result_links_to_record() {
    let engine = DispatchEngine::new();
    let v = engine.dispatch("Test request", "test_fixture");
    assert_eq!(
        v["execution_result"]["dispatch_id"],
        v["record"]["dispatch_id"]
    );
    assert_eq!(
        v["execution_result"]["decision_id"],
        v["decision"]["decision_id"]
    );
}

#[test]
fn test_evaluation_result_links_to_record() {
    let engine = DispatchEngine::new();
    let v = engine.dispatch("Test request", "test_fixture");
    assert_eq!(
        v["evaluation_result"]["dispatch_id"],
        v["record"]["dispatch_id"]
    );
    assert_eq!(
        v["evaluation_result"]["execution_result_id"],
        v["execution_result"]["result_id"]
    );
}

// Gate tests
#[test]
fn test_provider_disabled_gate_always_present() {
    let engine = DispatchEngine::new();
    let v = engine.dispatch("Test request", "test_fixture");
    let gates = v["decision"]["execution_gates"].as_array().unwrap();
    let gate_types: Vec<&str> = gates
        .iter()
        .map(|g| g["gate_type"].as_str().unwrap())
        .collect();
    assert!(gate_types.contains(&"provider_disabled"));
    assert!(gate_types.contains(&"sandbox_disabled"));
}

#[test]
fn test_low_risk_noop_is_decided() {
    let engine = DispatchEngine::new();
    let v = engine.dispatch("Summarize the README", "test_fixture");
    assert_eq!(v["decision"]["decision_status"], "decided");
    assert_eq!(v["record"]["final_status"], "not_executed");
}

#[test]
fn test_target_write_generates_gate() {
    let engine = DispatchEngine::new();
    let v = engine.dispatch("Fix the bug and commit the changes to main", "test_fixture");
    let gates = v["decision"]["execution_gates"].as_array().unwrap();
    let gate_types: Vec<&str> = gates
        .iter()
        .map(|g| g["gate_type"].as_str().unwrap())
        .collect();
    assert!(gate_types.contains(&"target_write"));
}

#[test]
fn test_low_confidence_generates_gate() {
    let engine = DispatchEngine::new();
    let v = engine.dispatch("Make it better", "test_fixture");
    let gates = v["decision"]["execution_gates"].as_array().unwrap();
    let gate_types: Vec<&str> = gates
        .iter()
        .map(|g| g["gate_type"].as_str().unwrap())
        .collect();
    assert!(gate_types.contains(&"confidence"));
}

#[test]
fn test_high_risk_needs_approval() {
    let engine = DispatchEngine::new();
    let v = engine.dispatch("Fix the bug and commit changes to main", "test_fixture");
    assert_eq!(v["decision"]["decision_status"], "needs_approval");
}

// Deterministic replay tests
#[test]
fn test_same_input_same_domain() {
    let engine = DispatchEngine::new();
    let v1 = engine.dispatch("Summarize the README", "test_fixture");
    let v2 = engine.dispatch("Summarize the README", "test_fixture");
    assert_eq!(v1["analysis"]["task_domain"], v2["analysis"]["task_domain"]);
    assert_eq!(v1["analysis"]["task_intent"], v2["analysis"]["task_intent"]);
    assert_eq!(v1["record"]["final_status"], v2["record"]["final_status"]);
}

#[test]
fn test_bundle_analysis_matches_request() {
    let engine = DispatchEngine::new();
    let v = engine.dispatch("Summarize the README", "test_fixture");
    assert_eq!(
        v["analysis"]["raw_request_snapshot"],
        "Summarize the README"
    );
    assert_eq!(v["analysis"]["task_domain"], "docs");
    assert_eq!(v["analysis"]["task_intent"], "summarize");
}

// Golden fixture e2e tests (20 tests)
macro_rules! golden_e2e {
    ($name:ident, $file:expr) => {
        #[test]
        fn $name() {
            let engine = DispatchEngine::new();
            let path = Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .expect("engine crate has workspace parent")
                .join("tests/fixtures/dispatch_wire/v1")
                .join($file);
            let raw = std::fs::read_to_string(&path).expect("fixture readable");
            let fixture: serde_json::Value = serde_json::from_str(&raw).expect("fixture json");
            let request = &fixture["request"];
            let raw_request = request["raw_request"].as_str().unwrap();
            let request_source = request["request_source"].as_str().unwrap_or("test_fixture");
            let v = engine.dispatch(raw_request, request_source);
            assert!(!v["record"]["dispatch_id"].as_str().unwrap().is_empty());
            assert!(!v["record"]["task_analysis_id"].as_str().unwrap().is_empty());
            let status = v["record"]["final_status"].as_str().unwrap();
            assert!([
                "not_executed",
                "completed",
                "failed",
                "escalated",
                "manual_pending"
            ]
            .contains(&status));
        }
    };
}

use std::path::Path;

golden_e2e!(e2e_fixture_01, "fixture_01_low_risk_summary.json");
golden_e2e!(e2e_fixture_02, "fixture_02_doc_audit.json");
golden_e2e!(e2e_fixture_03, "fixture_03_code_review.json");
golden_e2e!(e2e_fixture_04, "fixture_04_code_gen.json");
golden_e2e!(e2e_fixture_05, "fixture_05_debug.json");
golden_e2e!(e2e_fixture_06, "fixture_06_architecture.json");
golden_e2e!(e2e_fixture_07, "fixture_07_math.json");
golden_e2e!(e2e_fixture_08, "fixture_08_config_review.json");
golden_e2e!(e2e_fixture_09, "fixture_09_infra_deploy.json");
golden_e2e!(e2e_fixture_10, "fixture_10_provider_boundary.json");
golden_e2e!(e2e_fixture_11, "fixture_11_target_write.json");
golden_e2e!(e2e_fixture_12, "fixture_12_secret_handling.json");
golden_e2e!(e2e_fixture_13, "fixture_13_long_context.json");
golden_e2e!(e2e_fixture_14, "fixture_14_ambiguous.json");
golden_e2e!(e2e_fixture_15, "fixture_15_conflicting.json");
golden_e2e!(e2e_fixture_16, "fixture_16_read_only_high_risk.json");
golden_e2e!(e2e_fixture_17, "fixture_17_negated_no_write.json");
golden_e2e!(e2e_fixture_18, "fixture_18_negated_no_execute.json");
golden_e2e!(e2e_fixture_19, "fixture_19_budget_constrained.json");
golden_e2e!(e2e_fixture_20, "fixture_20_high_quality_critical.json");
