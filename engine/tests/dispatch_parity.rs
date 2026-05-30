use std::fs;
use std::path::{Path, PathBuf};

use engine::dispatch_decision::DispatchDecision;
use engine::dispatch_ledger::{DispatchBundle, DispatchRecord};
use engine::evaluation_stub::EvaluationResult;
use engine::executor_adapter::ExecutionResult;
use engine::task_analyzer::TaskAnalysis;
use engine::{build_dispatch_bundle, DispatchEngine};
use serde_json::Value;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("engine crate has workspace parent")
        .to_path_buf()
}

fn golden_paths() -> Vec<PathBuf> {
    let dir = repo_root().join("tests/fixtures/dispatch_wire/v1");
    let mut paths: Vec<PathBuf> = fs::read_dir(dir)
        .expect("dispatch wire fixture dir exists")
        .map(|entry| entry.expect("fixture entry").path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("json"))
        .collect();
    paths.sort();
    paths
}

#[test]
fn rust_dispatch_bundle_matches_python_golden_fixtures() {
    let paths = golden_paths();
    assert_eq!(paths.len(), 20);

    for path in paths {
        let raw = fs::read_to_string(&path).expect("fixture is readable");
        let fixture: Value = serde_json::from_str(&raw).expect("fixture is json");
        let request = &fixture["request"];
        let actual = build_dispatch_bundle(
            request["raw_request"].as_str().expect("raw_request string"),
            request["request_source"]
                .as_str()
                .expect("request_source string"),
        );
        assert_eq!(actual, fixture["golden_bundle"], "fixture drift: {path:?}");
    }
}

#[test]
fn golden_fixtures_round_trip_through_current_rust_wire_structs() {
    let paths = golden_paths();
    assert_eq!(paths.len(), 20);

    for path in paths {
        let raw = fs::read_to_string(&path).expect("fixture is readable");
        let fixture: Value = serde_json::from_str(&raw).expect("fixture is json");
        let golden = &fixture["golden_bundle"];

        let record: DispatchRecord =
            serde_json::from_value(golden["record"].clone()).expect("record should deserialize");
        let analysis: TaskAnalysis = serde_json::from_value(golden["analysis"].clone())
            .expect("analysis should deserialize");
        let decision: DispatchDecision = serde_json::from_value(golden["decision"].clone())
            .expect("decision should deserialize");
        let execution_result: ExecutionResult =
            serde_json::from_value(golden["execution_result"].clone())
                .expect("execution result should deserialize");
        let evaluation_result: EvaluationResult =
            serde_json::from_value(golden["evaluation_result"].clone())
                .expect("evaluation result should deserialize");
        let bundle: DispatchBundle =
            serde_json::from_value(golden.clone()).expect("bundle should deserialize");

        assert_eq!(
            record.to_value(),
            golden["record"],
            "record drift: {path:?}"
        );
        assert_eq!(
            analysis.to_value(),
            golden["analysis"],
            "analysis drift: {path:?}"
        );
        assert_eq!(
            decision.to_value(),
            golden["decision"],
            "decision drift: {path:?}"
        );
        assert_eq!(
            execution_result.to_value(),
            golden["execution_result"],
            "execution result drift: {path:?}"
        );
        assert_eq!(
            evaluation_result.to_value(),
            golden["evaluation_result"],
            "evaluation result drift: {path:?}"
        );
        assert_eq!(bundle.to_value(), *golden, "bundle drift: {path:?}");

        assert_eq!(record.task_analysis_id, analysis.analysis_id);
        assert_eq!(record.decision_id, decision.decision_id);
        assert_eq!(decision.analysis_id, analysis.analysis_id);
        assert_eq!(decision.analysis_snapshot, golden["analysis"]);
        assert_eq!(execution_result.dispatch_id, record.dispatch_id);
        assert_eq!(execution_result.decision_id, decision.decision_id);
        assert_eq!(
            evaluation_result.execution_result_id,
            execution_result.result_id
        );
        assert_eq!(
            record.budget_reservation_id.as_deref(),
            Some(decision.budget_reservation.reservation_id.as_str())
        );
    }
}

#[test]
fn execution_result_schema_includes_active_runtime_variants() {
    let path = repo_root().join("wire_contract/v1/execution_result.schema.json");
    let raw = fs::read_to_string(path).expect("execution result schema is readable");
    let schema: Value = serde_json::from_str(&raw).expect("execution result schema is json");
    let executor_types = schema["properties"]["executor_type"]["enum"]
        .as_array()
        .expect("executor types enum");
    let statuses = schema["properties"]["status"]["enum"]
        .as_array()
        .expect("execution statuses enum");

    for executor_type in ["claude_code_cli", "codex_cli"] {
        assert!(
            executor_types.iter().any(|item| item == executor_type),
            "missing active executor type: {executor_type}"
        );
    }
    for status in ["cli_completed", "provider_completed"] {
        assert!(
            statuses.iter().any(|item| item == status),
            "missing active execution status: {status}"
        );
    }
}

#[test]
fn dispatch_engine_default_path_is_noop_and_read_only() {
    let bundle = DispatchEngine::new().dispatch(
        "Review this module without provider calls, sandbox execution, or target repo writes.",
        "test_fixture",
    );

    assert_eq!(bundle["execution_result"]["executor_type"], "noop");
    assert_eq!(bundle["execution_result"]["status"], "not_executed");
    assert_eq!(bundle["record"]["final_status"], "not_executed");

    let hard_constraints = bundle["decision"]["hard_constraints"]
        .as_array()
        .expect("hard_constraints array");
    assert!(hard_constraints
        .iter()
        .any(|item| item == "no_target_write"));
    assert!(hard_constraints
        .iter()
        .any(|item| item == "no_provider_call"));

    let gate_types: Vec<&str> = bundle["decision"]["execution_gates"]
        .as_array()
        .expect("execution_gates array")
        .iter()
        .map(|gate| gate["gate_type"].as_str().expect("gate type string"))
        .collect();
    assert!(gate_types.contains(&"provider_disabled"));
    assert!(gate_types.contains(&"sandbox_disabled"));
}

#[test]
fn dispatch_engine_records_budget_reservation_in_ledger_bundle() {
    let bundle = DispatchEngine::new().dispatch(
        "Generate a Python function with tests for a small config parser.",
        "test_fixture",
    );
    let analysis = &bundle["analysis"];
    let reservation = &bundle["decision"]["budget_reservation"];

    let expected_total = analysis["context_budget_estimate"].as_i64().unwrap()
        + analysis["execution_budget_estimate"].as_i64().unwrap();
    assert_eq!(reservation["reserved_total_tokens"], expected_total);
    assert_eq!(
        bundle["record"]["budget_reservation_id"],
        reservation["reservation_id"]
    );
    assert!(reservation["reserved_cost"].as_f64().unwrap() > 0.0);
}
