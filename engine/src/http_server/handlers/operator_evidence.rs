use axum::extract::{Extension, State};
use axum::http::{HeaderMap, Uri};
use axum::response::IntoResponse;
use axum::Json;
use serde_json::json;

use crate::http_server::middleware::{
    authorize, cors_headers, internal_error, require_store, ApiError, RequestId,
};
use crate::http_server::state::AxumApiState;
use crate::http_server::AXUM_API_SCHEMA_VERSION;
use crate::storage::local_product_store::LocalProductStore;

pub(crate) fn build_operator_evidence(
    store: &LocalProductStore,
    run_id: &str,
) -> Result<serde_json::Value, String> {
    let agents = store.list_agent_state_by_run(run_id)?;
    let pending_mailbox_count = store.count_mailbox(None, Some(run_id), Some("pending"))?;
    let raw_proposals = store.list_proposals_by_run(run_id, 500, 0)?;
    let scorecard_artifacts = store.native_scorecard_artifacts_by_run(run_id, 20)?;

    let mut type_counts: std::collections::HashMap<String, (i64, i64)> =
        std::collections::HashMap::new();
    let mut pending_proposals: i64 = 0;
    let mut review_count: i64 = 0;
    let mut debate_count: i64 = 0;

    for p in &raw_proposals {
        let status = p.get("status").and_then(|v| v.as_str()).unwrap_or("");
        let ptype = p
            .get("proposal_type")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        if status == "pending" {
            pending_proposals += 1;
        }
        let is_terminal = status == "accepted" || status == "rejected" || status == "cancelled";
        let entry = type_counts.entry(ptype.clone()).or_insert((0, 0));
        entry.0 += 1;
        if is_terminal {
            entry.1 += 1;
        }

        if ptype == "review_request" || ptype == "review_verdict" {
            review_count += 1;
        }
        if ptype == "debate_request" || ptype == "debate_position" || ptype == "debate_resolution" {
            debate_count += 1;
        }
    }

    let proposals_array: Vec<serde_json::Value> = type_counts
        .iter()
        .map(|(ptype, (count, terminal))| {
            json!({
                "type": ptype,
                "count": count,
                "terminal_count": terminal,
            })
        })
        .collect();

    // Run-scoped audit only — no global audit leak
    let audit_events = store.search_audit_events_by_run(run_id, 50, 0)?;

    let mut blocked_signals: i64 = 0;
    let mut recent_audit = Vec::new();
    let mut last_updated: Option<String> = None;

    for event in &audit_events {
        let action = event.get("action").and_then(|v| v.as_str()).unwrap_or("");
        if action.contains("conflict") || action.contains("blocked") {
            blocked_signals += 1;
        }

        recent_audit.push(json!({
            "audit_id": event.get("audit_id"),
            "created_at": event.get("created_at"),
            "actor": event.get("actor"),
            "action": event.get("action"),
            "resource": event.get("resource"),
        }));

        if let Some(created_at) = event.get("created_at").and_then(|v| v.as_str()) {
            match &last_updated {
                None => last_updated = Some(created_at.to_string()),
                Some(prev) => {
                    if created_at > prev.as_str() {
                        last_updated = Some(created_at.to_string());
                    }
                }
            }
        }
    }

    for agent in &agents {
        match &last_updated {
            None => last_updated = Some(agent.updated_at.clone()),
            Some(prev) => {
                if agent.updated_at > *prev {
                    last_updated = Some(agent.updated_at.clone());
                }
            }
        }
    }

    let agent_views: Vec<serde_json::Value> = agents
        .iter()
        .map(|a| {
            json!({
                "agent_id": a.agent_id,
                "role": a.role,
                "status": a.status,
                "updated_at": a.updated_at,
            })
        })
        .collect();

    let scorecard_views: Vec<serde_json::Value> = scorecard_artifacts
        .iter()
        .map(|artifact| {
            let scorecard = artifact
                .get("scorecard")
                .unwrap_or(&serde_json::Value::Null);
            json!({
                "artifact_id": artifact.get("artifact_id"),
                "created_at": artifact.get("created_at"),
                "schema_version": artifact.get("schema_version"),
                "scorecard_schema_version": artifact.get("scorecard_schema_version"),
                "read_only": artifact.get("read_only"),
                "content_sha256": artifact.get("content_sha256"),
                "status": scorecard.get("status"),
                "runtime_kind": scorecard.get("runtime_kind"),
                "derived_metrics": scorecard.get("derived_metrics"),
            })
        })
        .collect();

    let needs_human_decision = pending_proposals > 0 && (review_count > 0 || debate_count > 0);

    // Bounded operator summary — metadata-only, no raw text
    let what_happened =
        if agents.is_empty() && raw_proposals.is_empty() && scorecard_views.is_empty() {
            "No activity recorded for this run".to_string()
        } else {
            let terminal = type_counts.values().map(|(_, t)| t).sum::<i64>();
            format!(
                "{} agents, {} proposals ({} processed), {} scorecards",
                agents.len(),
                raw_proposals.len(),
                terminal,
                scorecard_views.len()
            )
        };

    let what_is_pending = if pending_mailbox_count == 0 && pending_proposals == 0 {
        "Nothing pending".to_string()
    } else {
        format!(
            "{} pending mailbox, {} pending proposals",
            pending_mailbox_count, pending_proposals
        )
    };

    let what_is_blocked = if blocked_signals == 0 {
        "No blockers".to_string()
    } else {
        format!("{} blocked/conflict signals", blocked_signals)
    };

    Ok(json!({
        "schema_version": AXUM_API_SCHEMA_VERSION,
        "run_id": run_id,
        "agent_count": agents.len(),
        "agents": agent_views,
        "pending_mailbox_count": pending_mailbox_count,
        "proposals": proposals_array,
        "review_count": review_count,
        "debate_count": debate_count,
        "scorecard_artifact_count": scorecard_views.len(),
        "scorecards": scorecard_views,
        "blocked_signals_count": blocked_signals,
        "needs_human_decision": needs_human_decision,
        "operator_summary": {
            "what_happened": what_happened,
            "what_is_pending": what_is_pending,
            "what_is_blocked": what_is_blocked,
            "needs_human_decision": needs_human_decision,
        },
        "recent_audit": recent_audit,
        "last_updated": last_updated,
    }))
}

pub(crate) async fn api_operator_evidence(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    axum::extract::Path(run_id): axum::extract::Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "dispatch:read", uri.path(), &request_id.0)?;
    let store = require_store(&state)?;
    let evidence = build_operator_evidence(&store, &run_id).map_err(internal_error)?;
    Ok((cors_headers(), Json(evidence)))
}

#[cfg(test)]
mod tests {
    use super::build_operator_evidence;
    use crate::storage::local_product_store::LocalProductStore;
    use serde_json::json;

    fn make_store() -> LocalProductStore {
        LocalProductStore::new(":memory:").expect("in-memory store")
    }

    #[test]
    fn test_operator_evidence_empty_run() {
        let store = make_store();
        let evidence = build_operator_evidence(&store, "run-nonexistent").unwrap();

        assert_eq!(evidence["run_id"], "run-nonexistent");
        assert_eq!(evidence["agent_count"], 0);
        assert_eq!(evidence["agents"].as_array().unwrap().len(), 0);
        assert_eq!(evidence["pending_mailbox_count"], 0);
        assert_eq!(evidence["proposals"].as_array().unwrap().len(), 0);
        assert_eq!(evidence["blocked_signals_count"], 0);
        assert_eq!(evidence["review_count"], 0);
        assert_eq!(evidence["debate_count"], 0);
        assert_eq!(evidence["needs_human_decision"], false);
    }

    #[test]
    fn test_operator_evidence_agent_fields_limited() {
        let store = make_store();
        store
            .create_agent_state(
                "agent-1",
                "run-1",
                "implementer",
                &[],
                Some("do stuff"),
                "busy",
                &json!({"custom_key": "value"}),
            )
            .unwrap();

        let evidence = build_operator_evidence(&store, "run-1").unwrap();
        let agents = evidence["agents"].as_array().unwrap();
        assert_eq!(agents.len(), 1);

        let agent = &agents[0];
        assert_eq!(agent["agent_id"], "agent-1");
        assert_eq!(agent["role"], "implementer");
        assert_eq!(agent["status"], "busy");
        assert!(agent.get("objective").is_none());
        assert!(agent.get("updated_at").is_some());
        assert!(agent.get("scratchpad").is_none());
        assert!(agent.get("metadata").is_none());
        assert!(agent.get("capability_profile").is_none());
        assert!(agent.get("redaction_filter").is_none());
    }

    #[test]
    fn test_operator_evidence_no_raw_proposal_text() {
        let store = make_store();
        store
            .create_proposal(
                "prop-1",
                "corr-1",
                "run-2",
                "node-1",
                "agent-1",
                "child_task",
                "secret objective text",
                "secret context details",
                None,
                None,
                None,
            )
            .unwrap();

        let evidence = build_operator_evidence(&store, "run-2").unwrap();
        let proposals = evidence["proposals"].as_array().unwrap();
        assert_eq!(proposals.len(), 1);
        assert_eq!(proposals[0]["type"], "child_task");
        assert_eq!(proposals[0]["count"], 1);
        assert_eq!(proposals[0]["terminal_count"], 0);

        let recent = evidence["recent_audit"].as_array().unwrap();
        let create_event = recent
            .iter()
            .find(|e| e["action"] == "agent_proposal.create")
            .expect("should have proposal create audit event");
        assert!(create_event.get("details").is_none());
    }

    #[test]
    fn test_operator_evidence_no_audit_details() {
        let store = make_store();
        store
            .append_audit(
                "system",
                "test.action",
                "resource/1",
                &json!({"secret": true}),
            )
            .unwrap();

        let evidence = build_operator_evidence(&store, "run-3").unwrap();
        let recent = evidence["recent_audit"].as_array().unwrap();

        for event in recent {
            assert!(
                event.get("details").is_none(),
                "audit events must not expose details"
            );
            assert!(event.get("audit_id").is_some());
            assert!(event.get("created_at").is_some());
            assert!(event.get("actor").is_some());
            assert!(event.get("action").is_some());
            assert!(event.get("resource").is_some());
        }
    }

    #[test]
    fn test_operator_evidence_audit_run_scoped() {
        let store = make_store();

        // Audit events for run-A
        store
            .create_agent_state("a1", "run-A", "worker", &[], None, "idle", &json!({}))
            .unwrap();
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

        // Audit events for run-B
        store
            .create_agent_state("b1", "run-B", "worker", &[], None, "idle", &json!({}))
            .unwrap();
        store
            .append_audit(
                "system",
                "conflict.detected",
                "node/run-B/1",
                &json!({"run_id": "run-B"}),
            )
            .unwrap();

        let evidence_a = build_operator_evidence(&store, "run-A").unwrap();
        let evidence_b = build_operator_evidence(&store, "run-B").unwrap();

        // run-A should see its own 2 blocked signals (plus agent_state create audit)
        assert_eq!(evidence_a["blocked_signals_count"], 2);
        let audit_a = evidence_a["recent_audit"].as_array().unwrap();
        for event in audit_a {
            let resource = event["resource"].as_str().unwrap();
            assert!(
                resource.contains("run-A"),
                "run-A audit must not leak run-B events: {}",
                resource
            );
        }

        // run-B should see only its own 1 blocked signal
        assert_eq!(evidence_b["blocked_signals_count"], 1);
        let audit_b = evidence_b["recent_audit"].as_array().unwrap();
        for event in audit_b {
            let resource = event["resource"].as_str().unwrap();
            assert!(
                resource.contains("run-B"),
                "run-B audit must not leak run-A events: {}",
                resource
            );
        }
    }

    #[test]
    fn test_operator_summary_empty_run() {
        let store = make_store();
        let evidence = build_operator_evidence(&store, "run-empty").unwrap();
        let summary = &evidence["operator_summary"];

        assert_eq!(
            summary["what_happened"],
            "No activity recorded for this run"
        );
        assert_eq!(summary["what_is_pending"], "Nothing pending");
        assert_eq!(summary["what_is_blocked"], "No blockers");
        assert_eq!(summary["needs_human_decision"], false);
    }

    #[test]
    fn test_operator_summary_pending_reflected() {
        let store = make_store();
        store
            .create_agent_state("a1", "run-p", "worker", &[], None, "busy", &json!({}))
            .unwrap();
        store
            .send_message(
                "m1",
                "a1",
                "a2",
                "task_assign",
                Some("body"),
                None,
                Some("run-p"),
                None,
                None,
                &json!({}),
            )
            .unwrap();
        store
            .create_proposal(
                "p1",
                "c1",
                "run-p",
                "root",
                "a1",
                "handoff",
                "handoff objective",
                "context",
                None,
                None,
                None,
            )
            .unwrap();

        let evidence = build_operator_evidence(&store, "run-p").unwrap();
        let summary = &evidence["operator_summary"];

        assert!(summary["what_happened"]
            .as_str()
            .unwrap()
            .contains("1 agents"));
        assert!(summary["what_is_pending"]
            .as_str()
            .unwrap()
            .contains("1 pending mailbox"));
        assert!(summary["what_is_pending"]
            .as_str()
            .unwrap()
            .contains("1 pending proposals"));
    }

    #[test]
    fn test_operator_summary_blocked_reflected() {
        let store = make_store();
        store
            .create_agent_state("a1", "run-b", "worker", &[], None, "idle", &json!({}))
            .unwrap();
        store
            .append_audit(
                "system",
                "conflict.detected",
                "node/run-b/1",
                &json!({"run_id": "run-b"}),
            )
            .unwrap();

        let evidence = build_operator_evidence(&store, "run-b").unwrap();
        let summary = &evidence["operator_summary"];

        assert!(summary["what_is_blocked"]
            .as_str()
            .unwrap()
            .contains("blocked/conflict"));
    }

    #[test]
    fn test_operator_summary_no_raw_text() {
        let store = make_store();
        store
            .create_agent_state(
                "a1",
                "run-s",
                "worker",
                &[],
                Some("secret: sk-live-abc123"),
                "busy",
                &json!({}),
            )
            .unwrap();

        let evidence = build_operator_evidence(&store, "run-s").unwrap();
        let summary_text = evidence["operator_summary"].to_string();

        assert!(
            !summary_text.contains("sk-live-"),
            "operator_summary must not contain raw secret text"
        );
        assert!(
            !summary_text.contains("secret:"),
            "operator_summary must not contain raw objective text"
        );
    }
}
