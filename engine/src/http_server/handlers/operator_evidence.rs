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

    let audit_events = store.search_audit_events(50, 0, None)?;

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

    let needs_human_decision = pending_proposals > 0 && (review_count > 0 || debate_count > 0);

    Ok(json!({
        "schema_version": AXUM_API_SCHEMA_VERSION,
        "run_id": run_id,
        "agent_count": agents.len(),
        "agents": agent_views,
        "pending_mailbox_count": pending_mailbox_count,
        "proposals": proposals_array,
        "review_count": review_count,
        "debate_count": debate_count,
        "blocked_signals_count": blocked_signals,
        "needs_human_decision": needs_human_decision,
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

        assert!(agent.get("objective").is_none());
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
}
