#![recursion_limit = "256"]

use axum::body::{to_bytes, Body};
use axum::http::{Method, Request, StatusCode};
use engine::http_server::{build_axum_router, AxumApiState};
use engine::storage::local_product_store::LocalProductStore;
use serde_json::{json, Value};
use tempfile::tempdir;
use tower::ServiceExt;

async fn response_json(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), 1_048_576).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

fn make_store() -> (LocalProductStore, tempfile::TempDir) {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("team.db")).unwrap();
    (store, dir)
}

fn build_app(store: LocalProductStore) -> axum::Router {
    build_axum_router(AxumApiState::new().with_local_store(store))
}

fn sample_scorecard_artifact(run_id: &str) -> Value {
    json!({
        "schema_version": "native_scorecard_artifact.v1",
        "artifact_kind": "token_efficiency_scorecard",
        "storage": "app_owned_artifact_json_export",
        "read_only": true,
        "created_at": "2026-07-06T00:00:00Z",
        "artifact_id": format!("scorecard-{run_id}-abc123"),
        "content_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "scorecard_schema_version": "token_efficiency_scorecard.v1",
        "scorecard": {
            "schema_version": "token_efficiency_scorecard.v1",
            "adapter_run_id": run_id,
            "runtime_kind": "native_harness",
            "runtime_version": "provider-gated-real-runner.v1",
            "scenario_id": "provider_gated_remember_dont_reread_runner",
            "mode": "stateful_store",
            "state_strategy": "durable_state",
            "status": "pass",
            "pass_fail_reason": "same score threshold met",
            "quality_score": 1.0,
            "quality_method": "rule",
            "input_token_total": 100,
            "output_token_total": 80,
            "context_token_total": 100,
            "repeated_context_token_total": 10,
            "retrieved_ref_token_total": 8,
            "tool_call_count": 1,
            "redundant_tool_call_count": 0,
            "retry_count": 0,
            "step_count": 1,
            "duration_ms": 25,
            "estimated_cost_usd": 0.01,
            "raw_trace_artifact_id": "bounded-provider-gated-runner-stateful_store",
            "redaction_status": "redacted",
            "derived_metrics": {
                "total_tokens": 180,
                "context_share": 0.555556,
                "repeated_context_ratio": 0.1,
                "tool_redundancy_ratio": 0.0,
                "tokens_per_passing_run": 180,
                "cost_per_passing_run": 0.01,
                "step_retry_ratio": 0.0
            },
            "steps": [{
                "adapter_step_id": format!("{run_id}-iter-00"),
                "adapter_run_id": run_id,
                "step_index": 0,
                "node_name": "real_experiment_iteration_00",
                "agent_role": "executor",
                "operation_kind": "model_call",
                "input_tokens": 100,
                "output_tokens": 80,
                "context_tokens": 100,
                "repeated_context_tokens": 10,
                "retrieved_refs_count": 1,
                "retrieved_ref_tokens": 8,
                "tool_name": null,
                "tool_call_id": null,
                "status": "pass",
                "error_kind": "none",
                "state_read_bytes": 3,
                "state_write_bytes": 96
            }]
        }
    })
}

async fn get_evidence(app: &axum::Router, run_id: &str) -> (StatusCode, Value) {
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/v1/operator/evidence/{run_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = resp.status();
    let body = response_json(resp).await;
    (status, body)
}

#[tokio::test]
async fn test_empty_run_returns_safe_defaults() {
    let (store, _dir) = make_store();
    let app = build_app(store);

    let (status, body) = get_evidence(&app, "nonexistent-run").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["run_id"], "nonexistent-run");
    assert_eq!(body["agent_count"], 0);
    assert_eq!(body["pending_mailbox_count"], 0);
    assert_eq!(body["proposals"].as_array().unwrap().len(), 0);
    assert_eq!(body["review_count"], 0);
    assert_eq!(body["debate_count"], 0);
    assert_eq!(body["scorecard_artifact_count"], 0);
    assert_eq!(body["blocked_signals_count"], 0);
    assert_eq!(body["needs_human_decision"], false);
    assert!(body.get("error").is_none());
}

#[tokio::test]
async fn test_operator_evidence_includes_scorecard_metadata_only() {
    let (store, _dir) = make_store();
    store
        .record_native_scorecard_artifact(&sample_scorecard_artifact("run-scorecard"), "tester")
        .unwrap();

    let app = build_app(store);
    let (status, body) = get_evidence(&app, "run-scorecard").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["scorecard_artifact_count"], 1);
    assert_eq!(
        body["scorecards"][0]["artifact_id"],
        "scorecard-run-scorecard-abc123"
    );
    assert_eq!(body["scorecards"][0]["read_only"], true);
    assert_eq!(body["scorecards"][0]["runtime_kind"], "native_harness");
    assert!(body["scorecards"][0].get("steps").is_none());
    assert!(body["scorecards"][0].get("scorecard").is_none());
    assert!(body["scorecards"][0].get("raw_trace_artifact_id").is_none());
    assert!(!body.to_string().contains("raw_trace"));
    assert!(!body.to_string().contains("real_experiment_iteration_00"));
    assert!(!body.to_string().contains("raw_prompt"));
    assert!(!body.to_string().contains("raw_output"));
    assert!(!body.to_string().contains("transcript"));
}

#[tokio::test]
async fn test_run_with_agents_and_pending_mailbox() {
    let (store, _dir) = make_store();

    store
        .create_agent_state(
            "agent-a",
            "run-1",
            "implementer",
            &["code".to_string()],
            Some("build feature"),
            "busy",
            &json!({}),
        )
        .unwrap();
    store
        .create_agent_state(
            "agent-b",
            "run-1",
            "reviewer",
            &["review".to_string()],
            Some("review code"),
            "idle",
            &json!({}),
        )
        .unwrap();

    for i in 0..3 {
        store
            .send_message(
                &format!("msg-{i}"),
                "agent-a",
                "agent-b",
                "task_assign",
                Some(&format!("task body {i}")),
                None,
                Some("run-1"),
                None,
                None,
                &json!({}),
            )
            .unwrap();
    }

    let app = build_app(store);
    let (status, body) = get_evidence(&app, "run-1").await;

    assert_eq!(status, StatusCode::OK);

    let agents = body["agents"].as_array().unwrap();
    assert_eq!(agents.len(), 2);

    for agent in agents {
        let agent_id = agent["agent_id"].as_str().unwrap();
        assert!(
            agent_id == "agent-a" || agent_id == "agent-b",
            "unexpected agent_id: {agent_id}"
        );
        assert!(agent.get("role").is_some(), "agent must have role");
        assert!(
            agent.get("scratchpad").is_none(),
            "agent entry must not expose scratchpad"
        );
        assert!(
            agent.get("metadata").is_none(),
            "agent entry must not expose metadata"
        );
    }

    assert_eq!(body["pending_mailbox_count"], 3);
}

#[tokio::test]
async fn test_proposal_counts_by_type() {
    let (store, _dir) = make_store();

    store
        .create_agent_state(
            "agent-1",
            "run-pp",
            "planner",
            &[],
            None,
            "idle",
            &json!({}),
        )
        .unwrap();

    store
        .create_proposal(
            "pp-rr-1",
            "corr-1",
            "run-pp",
            "root",
            "agent-1",
            "review_request",
            "review this code",
            "summary",
            None,
            None,
            None,
        )
        .unwrap();
    store.update_proposal_status("pp-rr-1", "accepted").unwrap();

    store
        .create_proposal(
            "pp-dr-1",
            "corr-2",
            "run-pp",
            "root",
            "agent-1",
            "debate_request",
            "debate the approach",
            "summary",
            None,
            None,
            None,
        )
        .unwrap();
    store.update_proposal_status("pp-dr-1", "rejected").unwrap();

    store
        .create_proposal(
            "pp-ho-1",
            "corr-3",
            "run-pp",
            "root",
            "agent-1",
            "handoff",
            "handoff to reviewer",
            "summary",
            Some("agent-2"),
            None,
            None,
        )
        .unwrap();
    // remains pending

    store
        .create_proposal(
            "pp-ct-1",
            "corr-4",
            "run-pp",
            "root",
            "agent-1",
            "child_task",
            "create subtask",
            "summary",
            None,
            None,
            None,
        )
        .unwrap();
    store.update_proposal_status("pp-ct-1", "accepted").unwrap();

    let app = build_app(store);
    let (_status, body) = get_evidence(&app, "run-pp").await;

    let proposals = body["proposals"].as_array().unwrap();
    assert_eq!(proposals.len(), 4);

    let review_requests: Vec<_> = proposals
        .iter()
        .filter(|p| p["type"] == "review_request")
        .collect();
    assert_eq!(review_requests.len(), 1);
    assert_eq!(review_requests[0]["count"], 1);
    assert_eq!(review_requests[0]["terminal_count"], 1);

    let debate_requests: Vec<_> = proposals
        .iter()
        .filter(|p| p["type"] == "debate_request")
        .collect();
    assert_eq!(debate_requests.len(), 1);
    assert_eq!(debate_requests[0]["count"], 1);
    assert_eq!(debate_requests[0]["terminal_count"], 1);

    let handoffs: Vec<_> = proposals
        .iter()
        .filter(|p| p["type"] == "handoff")
        .collect();
    assert_eq!(handoffs.len(), 1);
    assert_eq!(handoffs[0]["count"], 1);
    assert_eq!(handoffs[0]["terminal_count"], 0);

    let child_tasks: Vec<_> = proposals
        .iter()
        .filter(|p| p["type"] == "child_task")
        .collect();
    assert_eq!(child_tasks.len(), 1);
    assert_eq!(child_tasks[0]["count"], 1);
    assert_eq!(child_tasks[0]["terminal_count"], 1);
}

#[tokio::test]
async fn test_review_and_debate_counts() {
    let (store, _dir) = make_store();

    store
        .create_agent_state(
            "agent-r",
            "run-rd",
            "reviewer",
            &[],
            None,
            "idle",
            &json!({}),
        )
        .unwrap();

    store
        .create_proposal(
            "rr-1",
            "c1",
            "run-rd",
            "root",
            "agent-r",
            "review_request",
            "review the PR",
            "summary",
            None,
            None,
            None,
        )
        .unwrap();

    store
        .create_proposal(
            "rv-1",
            "c1",
            "run-rd",
            "root",
            "agent-r",
            "review_verdict",
            "verdict: approve",
            "summary",
            None,
            None,
            None,
        )
        .unwrap();
    store.update_proposal_status("rv-1", "accepted").unwrap();

    store
        .create_proposal(
            "db-1",
            "c2",
            "run-rd",
            "root",
            "agent-r",
            "debate_request",
            "debate the design",
            "summary",
            None,
            None,
            None,
        )
        .unwrap();

    store
        .create_proposal(
            "dp-1",
            "c2",
            "run-rd",
            "root",
            "agent-r",
            "debate_position",
            "position: for",
            "summary",
            None,
            None,
            None,
        )
        .unwrap();

    store
        .create_proposal(
            "drs-1",
            "c2",
            "run-rd",
            "root",
            "agent-r",
            "debate_resolution",
            "resolved: proceed",
            "summary",
            None,
            None,
            None,
        )
        .unwrap();
    store.update_proposal_status("drs-1", "accepted").unwrap();

    let app = build_app(store);
    let (_status, body) = get_evidence(&app, "run-rd").await;

    assert_eq!(body["review_count"], 2);
    assert_eq!(body["debate_count"], 3);
}

#[tokio::test]
async fn test_blocked_signals_from_audit() {
    let (store, _dir) = make_store();

    // Create agents so audit events get run_id association
    store
        .create_agent_state("agent-x", "run-x", "worker", &[], None, "idle", &json!({}))
        .unwrap();

    // These audit events have run-x in resource/details
    store
        .append_audit(
            "system",
            "conflict.detected",
            "node/run-x/1",
            &json!({"run_id": "run-x"}),
        )
        .unwrap();
    store
        .append_audit(
            "system",
            "blocked.resource",
            "node/run-x/2",
            &json!({"run_id": "run-x"}),
        )
        .unwrap();
    // Normal action — should not count as blocked
    store
        .append_audit(
            "system",
            "normal.action",
            "node/run-x/3",
            &json!({"run_id": "run-x"}),
        )
        .unwrap();
    store
        .append_audit(
            "system",
            "conflict.resolved",
            "node/run-x/4",
            &json!({"run_id": "run-x"}),
        )
        .unwrap();

    let app = build_app(store);
    let (_status, body) = get_evidence(&app, "run-x").await;

    // 3 events match blocked/conflict pattern for run-x
    assert_eq!(body["blocked_signals_count"], 3);
}

#[tokio::test]
async fn test_blocked_signals_run_scoped() {
    let (store, _dir) = make_store();

    store
        .create_agent_state("a1", "run-A", "worker", &[], None, "idle", &json!({}))
        .unwrap();
    store
        .create_agent_state("b1", "run-B", "worker", &[], None, "idle", &json!({}))
        .unwrap();

    // Blocked events for run-A
    store
        .append_audit(
            "system",
            "conflict.detected",
            "node/run-A/1",
            &json!({"run_id": "run-A"}),
        )
        .unwrap();
    store
        .append_audit(
            "system",
            "blocked.resource",
            "node/run-A/2",
            &json!({"run_id": "run-A"}),
        )
        .unwrap();

    // Blocked event for run-B
    store
        .append_audit(
            "system",
            "conflict.detected",
            "node/run-B/1",
            &json!({"run_id": "run-B"}),
        )
        .unwrap();

    let app = build_app(store);

    let (_status_a, body_a) = get_evidence(&app, "run-A").await;
    assert_eq!(body_a["blocked_signals_count"], 2);

    let (_status_b, body_b) = get_evidence(&app, "run-B").await;
    assert_eq!(body_b["blocked_signals_count"], 1);

    // Verify no cross-run leak in recent_audit
    for event in body_a["recent_audit"].as_array().unwrap() {
        let resource = event["resource"].as_str().unwrap();
        assert!(
            !resource.contains("run-B"),
            "run-A evidence must not contain run-B audit events"
        );
    }
    for event in body_b["recent_audit"].as_array().unwrap() {
        let resource = event["resource"].as_str().unwrap();
        assert!(
            !resource.contains("run-A"),
            "run-B evidence must not contain run-A audit events"
        );
    }
}

#[tokio::test]
async fn test_needs_human_decision_flag() {
    let (store, _dir) = make_store();

    store
        .create_agent_state(
            "agent-h",
            "run-human",
            "reviewer",
            &[],
            None,
            "idle",
            &json!({}),
        )
        .unwrap();

    store
        .create_proposal(
            "hr-1",
            "c1",
            "run-human",
            "root",
            "agent-h",
            "review_request",
            "needs human review",
            "summary",
            None,
            None,
            None,
        )
        .unwrap();
    // proposal remains pending -> needs_human_decision should be true

    let app = build_app(store);
    let (_status, body) = get_evidence(&app, "run-human").await;

    assert_eq!(body["needs_human_decision"], true);
    assert_eq!(
        body["operator_summary"]["needs_human_decision"], true,
        "operator_summary.needs_human_decision must match top-level"
    );

    // Now test with only accepted proposals
    let (store2, _dir2) = make_store();
    store2
        .create_agent_state(
            "agent-h2",
            "run-nohuman",
            "reviewer",
            &[],
            None,
            "idle",
            &json!({}),
        )
        .unwrap();
    store2
        .create_proposal(
            "hr-2",
            "c2",
            "run-nohuman",
            "root",
            "agent-h2",
            "review_request",
            "already decided",
            "summary",
            None,
            None,
            None,
        )
        .unwrap();
    store2.update_proposal_status("hr-2", "accepted").unwrap();

    let app2 = build_app(store2);
    let (_status2, body2) = get_evidence(&app2, "run-nohuman").await;

    assert_eq!(body2["needs_human_decision"], false);
    assert_eq!(body2["operator_summary"]["needs_human_decision"], false);
}

#[tokio::test]
async fn test_no_data_leak_across_runs() {
    let (store_a, _dir_a) = make_store();

    store_a
        .create_agent_state(
            "agent-a1",
            "run-A",
            "planner",
            &["plan".to_string()],
            Some("plan task"),
            "busy",
            &json!({"secret_key": "xyz"}),
        )
        .unwrap();
    store_a
        .send_message(
            "msg-a1",
            "agent-a1",
            "agent-a2",
            "task_assign",
            Some("secret body"),
            None,
            Some("run-A"),
            None,
            None,
            &json!({}),
        )
        .unwrap();
    store_a
        .create_proposal(
            "prop-a1",
            "corr-a",
            "run-A",
            "root",
            "agent-a1",
            "handoff",
            "handoff secret objective",
            "secret context",
            Some("agent-a2"),
            None,
            None,
        )
        .unwrap();

    let app_a = build_app(store_a);
    let (_status_a, body_a) = get_evidence(&app_a, "run-A").await;
    assert_eq!(body_a["agents"].as_array().unwrap().len(), 1);
    assert_eq!(body_a["pending_mailbox_count"], 1);

    let (store_b, _dir_b) = make_store();
    let app_b = build_app(store_b);
    let (_status_b, body_b) = get_evidence(&app_b, "run-B").await;

    assert_eq!(body_b["run_id"], "run-B");
    assert_eq!(body_b["agents"].as_array().unwrap().len(), 0);
    assert_eq!(body_b["pending_mailbox_count"], 0);
    assert_eq!(body_b["proposals"].as_array().unwrap().len(), 0);
    assert_eq!(body_b["review_count"], 0);
    assert_eq!(body_b["debate_count"], 0);
}

#[tokio::test]
async fn test_evidence_redaction_no_secrets() {
    let (store, _dir) = make_store();

    store
        .create_agent_state(
            "agent-s",
            "run-secret",
            "implementer",
            &[],
            Some("use API key sk-live-abc123def456 to authenticate"),
            "busy",
            &json!({}),
        )
        .unwrap();
    store
        .create_proposal(
            "prop-s1",
            "corr-s",
            "run-secret",
            "root",
            "agent-s",
            "handoff",
            "handoff with secret token=sk-secret-xyz789",
            "context with password hunter2",
            Some("agent-t"),
            None,
            None,
        )
        .unwrap();

    let app = build_app(store);
    let (_status, body) = get_evidence(&app, "run-secret").await;

    let full_text = body.to_string();
    assert!(
        !full_text.contains("sk-live-"),
        "response must not contain raw secret key"
    );
    assert!(
        !full_text.contains("sk-secret-"),
        "response must not contain raw secret token"
    );

    let agents = body["agents"].as_array().unwrap();
    for agent in agents {
        let agent_text = agent.to_string();
        assert!(
            !agent_text.contains("sk-live-"),
            "agent entry must not expose raw objective"
        );
        assert!(
            agent.get("objective").is_none(),
            "agent entry must not expose objective field"
        );
        assert!(
            agent.get("scratchpad").is_none(),
            "agent entry must not expose scratchpad"
        );
        assert!(
            agent.get("context_summary").is_none(),
            "agent entry must not expose context_summary"
        );
    }
}

#[tokio::test]
async fn test_evidence_no_raw_prompt_or_output() {
    let (store, _dir) = make_store();

    store
        .create_agent_state(
            "agent-p",
            "run-prompt",
            "implementer",
            &["code".to_string()],
            Some("Write a function that processes user input and returns output"),
            "busy",
            &json!({}),
        )
        .unwrap();
    store
        .update_agent_state(
            "agent-p",
            "run-prompt",
            None,
            Some("Working on parser module, 40% complete"),
            None,
            None,
        )
        .unwrap();
    store
        .create_proposal(
            "prop-p1",
            "corr-p",
            "run-prompt",
            "root",
            "agent-p",
            "review_request",
            "Review the parser implementation",
            "Parser handles user input and outputs JSON",
            None,
            None,
            None,
        )
        .unwrap();

    let app = build_app(store);
    let (_status, body) = get_evidence(&app, "run-prompt").await;

    let full_text = body.to_string();

    assert!(
        !full_text.contains("scratchpad_summary"),
        "response must not contain scratchpad_summary field"
    );
    assert!(
        !full_text.contains("Write a function that processes"),
        "response must not contain raw objective text"
    );

    let agents = body["agents"].as_array().unwrap();
    for agent in agents {
        assert!(
            agent.get("scratchpad_summary").is_none(),
            "agent entry must not expose scratchpad_summary"
        );
        assert!(
            agent.get("objective").is_none(),
            "agent entry must not expose objective"
        );
    }

    let proposals = body["proposals"].as_array().unwrap();
    for proposal in proposals {
        assert!(
            proposal.get("body").is_none(),
            "proposal summary must not expose body"
        );
    }
}

#[tokio::test]
async fn test_evidence_memory_digest_is_aggregate_only() {
    let (store, _dir) = make_store();

    store
        .create_agent_state(
            "agent-m",
            "run-memory",
            "implementer",
            &["code".to_string()],
            Some("raw objective must not leak"),
            "busy",
            &json!({
                "memory_digest": {
                    "source_refs": ["agent_state:agent-m:scratchpad_summary"],
                    "expiry_policy": "on_prune",
                    "conflict_resolution": "latest_summary_wins",
                    "summary": "raw memory summary must not leak",
                    "updated_at": "2026-07-08T00:00:00Z"
                },
                "other_raw_metadata": "must not leak"
            }),
        )
        .unwrap();
    store
        .update_agent_state(
            "agent-m",
            "run-memory",
            None,
            Some("scratchpad text must not leak"),
            None,
            None,
        )
        .unwrap();

    let app = build_app(store);
    let (_status, body) = get_evidence(&app, "run-memory").await;
    let full_text = body.to_string();

    assert!(!full_text.contains("raw memory summary must not leak"));
    assert!(!full_text.contains("scratchpad text must not leak"));
    assert!(!full_text.contains("raw objective must not leak"));
    assert!(!full_text.contains("other_raw_metadata"));

    let agent = &body["agents"].as_array().unwrap()[0];
    assert_eq!(agent["memory_digest_present"], true);
    assert_eq!(agent["memory_source_ref_count"], 1);
    assert_eq!(agent["memory_updated_at"], "2026-07-08T00:00:00Z");
    assert!(agent["memory_estimated_bytes"].as_i64().unwrap() > 0);
    assert!(agent.get("memory_digest").is_none());
    assert!(agent.get("metadata").is_none());
}

#[tokio::test]
async fn test_operator_summary_present_and_bounded() {
    let (store, _dir) = make_store();

    store
        .create_agent_state("a1", "run-sum", "worker", &[], None, "busy", &json!({}))
        .unwrap();
    store
        .send_message(
            "m1",
            "a1",
            "a2",
            "task_assign",
            Some("body"),
            None,
            Some("run-sum"),
            None,
            None,
            &json!({}),
        )
        .unwrap();
    store
        .create_proposal(
            "p1",
            "c1",
            "run-sum",
            "root",
            "a1",
            "review_request",
            "review objective",
            "context",
            None,
            None,
            None,
        )
        .unwrap();

    let app = build_app(store);
    let (_status, body) = get_evidence(&app, "run-sum").await;

    let summary = &body["operator_summary"];
    assert!(summary.is_object(), "operator_summary must be an object");
    assert!(summary.get("what_happened").is_some());
    assert!(summary.get("what_is_pending").is_some());
    assert!(summary.get("what_is_blocked").is_some());
    assert!(summary.get("needs_human_decision").is_some());

    let happened = summary["what_happened"].as_str().unwrap();
    assert!(
        happened.contains("1 agents"),
        "what_happened should mention agent count"
    );
    assert!(
        happened.contains("1 proposals"),
        "what_happened should mention proposal count"
    );

    let pending = summary["what_is_pending"].as_str().unwrap();
    assert!(
        pending.contains("1 pending mailbox"),
        "what_is_pending should mention mailbox count"
    );
    assert!(
        pending.contains("1 pending proposals"),
        "what_is_pending should mention proposal count"
    );

    assert_eq!(summary["what_is_blocked"], "No blockers");
    assert_eq!(summary["needs_human_decision"], true);
}

#[tokio::test]
async fn test_operator_summary_empty_run() {
    let (store, _dir) = make_store();
    let app = build_app(store);
    let (_status, body) = get_evidence(&app, "run-empty").await;

    let summary = &body["operator_summary"];
    assert_eq!(
        summary["what_happened"],
        "No activity recorded for this run"
    );
    assert_eq!(summary["what_is_pending"], "Nothing pending");
    assert_eq!(summary["what_is_blocked"], "No blockers");
    assert_eq!(summary["needs_human_decision"], false);
}

#[tokio::test]
async fn test_operator_summary_no_raw_text() {
    let (store, _dir) = make_store();
    store
        .create_agent_state(
            "a1",
            "run-sec",
            "worker",
            &[],
            Some("api_key: sk-live-abc123secret"),
            "busy",
            &json!({}),
        )
        .unwrap();

    let app = build_app(store);
    let (_status, body) = get_evidence(&app, "run-sec").await;

    let summary_text = body["operator_summary"].to_string();
    assert!(
        !summary_text.contains("sk-live-"),
        "operator_summary must not contain raw secret text"
    );
}

#[tokio::test]
async fn test_audit_run_isolation_no_substring_collision() {
    let (store, _dir) = make_store();

    // Create agents for both runs
    store
        .create_agent_state("a1", "run-1", "worker", &[], None, "idle", &json!({}))
        .unwrap();
    store
        .create_agent_state("a2", "run-10", "worker", &[], None, "idle", &json!({}))
        .unwrap();

    // Audit events for run-1
    store
        .append_audit(
            "system",
            "conflict.detected",
            "node/run-1/step-1",
            &json!({"run_id": "run-1"}),
        )
        .unwrap();
    store
        .append_audit(
            "system",
            "blocked.resource",
            "agent_state/agent-1/run-1",
            &json!({"run_id": "run-1"}),
        )
        .unwrap();

    // Audit events for run-10 (substring collision with run-1)
    store
        .append_audit(
            "system",
            "conflict.detected",
            "node/run-10/step-1",
            &json!({"run_id": "run-10"}),
        )
        .unwrap();
    store
        .append_audit(
            "system",
            "blocked.resource",
            "agent_state/agent-2/run-10",
            &json!({"run_id": "run-10"}),
        )
        .unwrap();
    store
        .append_audit(
            "system",
            "normal.action",
            "node/run-10/step-2",
            &json!({"run_id": "run-10"}),
        )
        .unwrap();

    let app = build_app(store);

    // Query run-1 — must NOT include run-10 events
    let (_status_1, body_1) = get_evidence(&app, "run-1").await;
    assert_eq!(
        body_1["blocked_signals_count"], 2,
        "run-1 blocked_signals must count only run-1 events"
    );
    let audit_1 = body_1["recent_audit"].as_array().unwrap();
    assert!(
        !audit_1.is_empty(),
        "run-1 must have audit events from agent_state create"
    );
    for event in audit_1 {
        let resource = event["resource"].as_str().unwrap();
        assert!(
            !resource.contains("run-10"),
            "run-1 evidence must not include run-10 audit events, got: {}",
            resource
        );
    }

    // Query run-10 — must NOT include run-1 events
    let (_status_10, body_10) = get_evidence(&app, "run-10").await;
    assert_eq!(
        body_10["blocked_signals_count"], 2,
        "run-10 blocked_signals must count only run-10 events"
    );
    let audit_10 = body_10["recent_audit"].as_array().unwrap();
    for event in audit_10 {
        let resource = event["resource"].as_str().unwrap();
        assert!(
            !resource.contains("run-1/"),
            "run-10 evidence must not include run-1 audit events, got: {}",
            resource
        );
    }
}

#[tokio::test]
async fn test_audit_unrelated_details_text_not_matched() {
    let (store, _dir) = make_store();

    store
        .create_agent_state("a1", "run-42", "worker", &[], None, "idle", &json!({}))
        .unwrap();

    // Audit event where details_json contains the run_id string but NOT as details.run_id
    store
        .append_audit(
            "system",
            "normal.action",
            "node/global/1",
            &json!({"note": "run-42 was mentioned in passing", "other": "data"}),
        )
        .unwrap();

    // Audit event with proper details.run_id
    store
        .append_audit(
            "system",
            "conflict.detected",
            "node/run-42/step-1",
            &json!({"run_id": "run-42"}),
        )
        .unwrap();

    let app = build_app(store);
    let (_status, body) = get_evidence(&app, "run-42").await;

    // The "normal.action" event should be filtered out because details.run_id is absent
    // and resource doesn't contain "run-42" as a segment
    let audit = body["recent_audit"].as_array().unwrap();
    let conflict_events: Vec<_> = audit
        .iter()
        .filter(|e| e["action"] == "conflict.detected")
        .collect();
    assert_eq!(
        conflict_events.len(),
        1,
        "must find the real conflict event"
    );

    // The "normal.action" with unrelated details text should not appear
    let normal_events: Vec<_> = audit
        .iter()
        .filter(|e| e["action"] == "normal.action")
        .collect();
    assert!(
        normal_events.is_empty(),
        "unrelated details text containing run_id must not match"
    );
}
