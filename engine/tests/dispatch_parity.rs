use std::fs;
use std::path::{Path, PathBuf};

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
