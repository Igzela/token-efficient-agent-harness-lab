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
    assert_eq!(body["blocked_signals_count"], 0);
    assert_eq!(body["needs_human_decision"], false);
    assert!(body.get("error").is_none());
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

    store
        .append_audit("system", "conflict.detected", "node-1", &json!({}))
        .unwrap();
    store
        .append_audit("system", "blocked.resource", "node-2", &json!({}))
        .unwrap();
    store
        .append_audit("system", "normal.action", "node-3", &json!({}))
        .unwrap();
    store
        .append_audit("system", "conflict.resolved", "node-4", &json!({}))
        .unwrap();

    let app = build_app(store);
    let (_status, body) = get_evidence(&app, "any-run").await;

    assert_eq!(body["blocked_signals_count"], 3);
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
