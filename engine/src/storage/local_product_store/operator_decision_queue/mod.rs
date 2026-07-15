use std::collections::{BTreeMap, BTreeSet};

use serde_json::{json, Value};

use crate::feedback::policy_snapshot::stable_hash;

use crate::operator_decision::{
    derive_operator_decision_item, OperatorDecisionAction, OperatorDecisionEvidenceReference,
    OperatorDecisionQueue, OperatorDecisionSeverity, OperatorDecisionSource,
    OperatorDecisionSourceKind, OperatorDecisionSourceState,
    OPERATOR_DECISION_QUEUE_SCHEMA_VERSION, OPERATOR_DECISION_SOURCE_SCHEMA_VERSION,
};

use super::LocalProductStore;

const SOURCE_READ_LIMIT: i64 = 100;

impl LocalProductStore {
    pub fn operator_decision_now(&self) -> String {
        self.now()
    }

    pub fn operator_decision_queue(
        &self,
        generated_at: &str,
        maximum_freshness_seconds: u64,
        limit: i64,
        offset: i64,
    ) -> Result<OperatorDecisionQueue, String> {
        let mut sources = Vec::new();
        collect_workflow_and_approval_sources(self, &mut sources)?;
        collect_budget_sources(self, &mut sources)?;
        collect_benchmark_sources(self, &mut sources)?;
        collect_policy_sources(self, &mut sources)?;
        collect_scheduler_sources(self, &mut sources)?;
        collect_rollback_recovery_sources(self, &mut sources)?;

        let mut source_counts = BTreeMap::new();
        for source in &sources {
            *source_counts
                .entry(source.source_kind.as_identifier().to_string())
                .or_insert(0) += 1;
        }

        let conflict_keys = sources
            .iter()
            .map(|source| source.conflict_key.clone())
            .collect::<BTreeSet<_>>();
        let mut items = conflict_keys
            .iter()
            .map(|key| {
                derive_operator_decision_item(
                    key,
                    &sources,
                    generated_at,
                    maximum_freshness_seconds,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut acknowledgement_applied = false;
        for item in &items {
            if item.recommended_action != Some(OperatorDecisionAction::Acknowledge) {
                continue;
            }
            let Some(reference) = &item.selected_source else {
                continue;
            };
            let Some(source_sha256) = reference.content_sha256.as_deref() else {
                continue;
            };
            if !self.is_operator_source_acknowledged(
                &reference.evidence_type,
                &reference.evidence_id,
                source_sha256,
            )? {
                continue;
            }
            if let Some(source) = sources.iter_mut().find(|source| {
                source.source_kind.as_identifier() == reference.evidence_type
                    && source.source_id == reference.evidence_id
                    && reference.content_sha256.as_deref() == Some(&source.evidence_sha256)
            }) {
                source.state = OperatorDecisionSourceState::Resolved;
                source
                    .reason_codes
                    .push("operator_source_hash_acknowledged".to_string());
                source.seal()?;
                acknowledgement_applied = true;
            }
        }
        if acknowledgement_applied {
            items = conflict_keys
                .iter()
                .map(|key| {
                    derive_operator_decision_item(
                        key,
                        &sources,
                        generated_at,
                        maximum_freshness_seconds,
                    )
                })
                .collect::<Result<Vec<_>, _>>()?;
        }
        items.sort_by(|left, right| {
            right
                .severity
                .rank()
                .cmp(&left.severity.rank())
                .then_with(|| right.outcome.rank().cmp(&left.outcome.rank()))
                .then_with(|| left.conflict_key.cmp(&right.conflict_key))
                .then_with(|| left.decision_id.cmp(&right.decision_id))
        });

        let total = items.len();
        let limit = limit.clamp(1, 100) as usize;
        let offset = offset.clamp(0, 10_000) as usize;
        let items = items.into_iter().skip(offset).take(limit).collect();
        let mut queue = OperatorDecisionQueue {
            schema_version: OPERATOR_DECISION_QUEUE_SCHEMA_VERSION.to_string(),
            generated_at: generated_at.to_string(),
            maximum_freshness_seconds,
            total,
            limit,
            offset,
            source_counts,
            items,
            queue_sha256: String::new(),
        };
        queue.seal()?;
        Ok(queue)
    }
}

fn collect_workflow_and_approval_sources(
    store: &LocalProductStore,
    sources: &mut Vec<OperatorDecisionSource>,
) -> Result<(), String> {
    for run_summary in store.list_workflow_runs_with_offset(SOURCE_READ_LIMIT, 0)? {
        let Some(run_id) = string(&run_summary, "run_id") else {
            continue;
        };
        let run_id = run_id.to_string();
        let run = store.get_workflow_run(&run_id)?.ok_or_else(|| {
            format!("workflow run disappeared while deriving decisions: {run_id}")
        })?;
        let status = string(&run, "status").unwrap_or("unknown");
        let observed_at = string(&run, "updated_at").or_else(|| string(&run, "created_at"));
        if let Some(observed_at) = observed_at {
            let workflow_ref = original_reference("workflow_run", &run_id, None);
            if let Some(pause_reason) = run.get("pause_reason").and_then(Value::as_str) {
                let budget_decision = pause_reason.strip_prefix("budget_auto_pause:");
                let (kind, source_id, reason, evidence_references) =
                    if let Some(decision_id) = budget_decision {
                        (
                            OperatorDecisionSourceKind::Recovery,
                            decision_id.to_string(),
                            "budget_pause_recovery_required",
                            vec![original_reference(
                                "budget_pause_decision",
                                decision_id,
                                None,
                            )],
                        )
                    } else {
                        (
                            OperatorDecisionSourceKind::Workflow,
                            format!("workflow-pause-{run_id}"),
                            "workflow_paused",
                            vec![workflow_ref.clone()],
                        )
                    };
                push_source(
                    sources,
                    SourceInput {
                        kind,
                        id: source_id,
                        resource: run_id.clone(),
                        conflict_key: format!("{run_id}:control"),
                        action: OperatorDecisionAction::Resume,
                        state: OperatorDecisionSourceState::Actionable,
                        severity: OperatorDecisionSeverity::Warning,
                        confidence: 1.0,
                        observed_at,
                        expires_at: None,
                        reason,
                        evidence_references,
                    },
                )?;
            } else if status == "blocked" {
                let (state, reason) = if workflow_has_ready_node(&run) {
                    (
                        OperatorDecisionSourceState::Actionable,
                        "workflow_blocked_ready_node",
                    )
                } else {
                    (
                        OperatorDecisionSourceState::InsufficientEvidence,
                        "workflow_blocked_no_ready_node",
                    )
                };
                let confidence = if matches!(state, OperatorDecisionSourceState::Actionable) {
                    1.0
                } else {
                    0.0
                };
                push_source(
                    sources,
                    SourceInput {
                        kind: OperatorDecisionSourceKind::Workflow,
                        id: format!("workflow-status-{run_id}"),
                        resource: run_id.clone(),
                        conflict_key: format!("{run_id}:execution"),
                        action: OperatorDecisionAction::Retry,
                        state,
                        severity: OperatorDecisionSeverity::Warning,
                        confidence,
                        observed_at,
                        expires_at: None,
                        reason,
                        evidence_references: vec![workflow_ref.clone()],
                    },
                )?;
            } else if status == "failed" {
                push_source(
                    sources,
                    SourceInput {
                        kind: OperatorDecisionSourceKind::Workflow,
                        id: format!("workflow-status-{run_id}"),
                        resource: run_id.clone(),
                        conflict_key: format!("{run_id}:execution"),
                        action: OperatorDecisionAction::Retry,
                        state: OperatorDecisionSourceState::Resolved,
                        severity: OperatorDecisionSeverity::Info,
                        confidence: 1.0,
                        observed_at,
                        expires_at: None,
                        reason: "workflow_failed_terminal",
                        evidence_references: vec![workflow_ref],
                    },
                )?;
            }
        }

        let approvals = store.workflow_run_approvals(&run_id, SOURCE_READ_LIMIT)?;
        let mut latest_by_node = BTreeMap::<String, Value>::new();
        for approval in approvals {
            let Some(node_id) = string(&approval, "node_id") else {
                continue;
            };
            let sequence = approval
                .get("approval_sequence")
                .and_then(Value::as_i64)
                .unwrap_or(i64::MIN);
            let should_replace = latest_by_node
                .get(node_id)
                .and_then(|current| current.get("approval_sequence"))
                .and_then(Value::as_i64)
                .is_none_or(|current| sequence > current);
            if should_replace {
                latest_by_node.insert(node_id.to_string(), approval);
            }
        }

        for (node_id, approval) in latest_by_node {
            let Some(approval_id) = string(&approval, "approval_id") else {
                continue;
            };
            let Some(created_at) = string(&approval, "created_at") else {
                continue;
            };
            let decision = string(&approval, "decision").unwrap_or("requested");
            let reference = original_reference("workflow_run_approval", approval_id, None);
            match decision {
                "requested" => {
                    for (suffix, action) in [
                        ("approve", OperatorDecisionAction::Approve),
                        ("reject", OperatorDecisionAction::Reject),
                    ] {
                        push_source(
                            sources,
                            SourceInput {
                                kind: OperatorDecisionSourceKind::Approval,
                                id: format!("{approval_id}:{suffix}"),
                                resource: run_id.clone(),
                                conflict_key: format!("{run_id}:{node_id}:approval:{suffix}"),
                                action,
                                state: OperatorDecisionSourceState::Actionable,
                                severity: OperatorDecisionSeverity::Warning,
                                confidence: 1.0,
                                observed_at: created_at,
                                expires_at: string(&approval, "expires_at"),
                                reason: "approval_requested",
                                evidence_references: vec![reference.clone()],
                            },
                        )?;
                    }
                }
                "approved" | "rejected" => {
                    push_source(
                        sources,
                        SourceInput {
                            kind: OperatorDecisionSourceKind::Approval,
                            id: format!("{approval_id}:{decision}"),
                            resource: run_id.clone(),
                            conflict_key: format!("{run_id}:{node_id}:approval:{decision}"),
                            action: if decision == "approved" {
                                OperatorDecisionAction::Approve
                            } else {
                                OperatorDecisionAction::Reject
                            },
                            state: OperatorDecisionSourceState::Resolved,
                            severity: OperatorDecisionSeverity::Info,
                            confidence: 1.0,
                            observed_at: created_at,
                            expires_at: None,
                            reason: if decision == "approved" {
                                "approval_completed"
                            } else {
                                "rejection_completed"
                            },
                            evidence_references: vec![reference],
                        },
                    )?;
                }
                _ => {}
            }
        }
    }
    Ok(())
}

fn workflow_has_ready_node(run: &Value) -> bool {
    let nodes = run
        .get("nodes")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let edges = run
        .get("edges")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let statuses = nodes
        .iter()
        .filter_map(|node| {
            Some((
                string(node, "node_id")?.to_string(),
                string(node, "db_status")
                    .or_else(|| string(node, "status"))
                    .unwrap_or("unknown")
                    .to_string(),
            ))
        })
        .collect::<BTreeMap<_, _>>();

    nodes.iter().any(|node| {
        let Some(node_id) = string(node, "node_id") else {
            return false;
        };
        let status = string(node, "db_status")
            .or_else(|| string(node, "status"))
            .unwrap_or("unknown");
        if status != "pending" {
            return false;
        }
        edges
            .iter()
            .filter(|edge| string(edge, "to_node_id") == Some(node_id))
            .all(|edge| {
                string(edge, "from_node_id")
                    .and_then(|from| statuses.get(from))
                    .is_some_and(|status| status == "completed")
            })
    })
}

fn collect_budget_sources(
    store: &LocalProductStore,
    sources: &mut Vec<OperatorDecisionSource>,
) -> Result<(), String> {
    for artifact in store.recent_budget_anomaly_artifacts(SOURCE_READ_LIMIT)? {
        let Some(artifact_id) = string(&artifact, "artifact_id") else {
            continue;
        };
        let evidence = artifact.get("evidence").unwrap_or(&Value::Null);
        let scope = evidence.get("scope").unwrap_or(&Value::Null);
        let resource = string(scope, "run_id")
            .or_else(|| string(scope, "workspace_id"))
            .or_else(|| string(scope, "provider_id"));
        let Some(resource) = resource else {
            continue;
        };
        let observed_at = evidence
            .pointer("/window/generated_at")
            .and_then(Value::as_str)
            .or_else(|| string(&artifact, "created_at"));
        let Some(observed_at) = observed_at else {
            continue;
        };
        let outcome = string(evidence, "outcome").unwrap_or("insufficient_evidence");
        let detected = evidence
            .get("detected")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let confidence = evidence
            .pointer("/confidence/score")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        let critical = string(evidence, "severity") == Some("critical");
        let (action, state, severity, reason) = if outcome == "supported" && detected && critical {
            (
                OperatorDecisionAction::Pause,
                OperatorDecisionSourceState::Actionable,
                OperatorDecisionSeverity::Critical,
                "budget_critical_anomaly_detected",
            )
        } else if outcome == "supported" && detected {
            (
                OperatorDecisionAction::Inspect,
                OperatorDecisionSourceState::Actionable,
                OperatorDecisionSeverity::Warning,
                "budget_noncritical_anomaly_observed",
            )
        } else {
            (
                OperatorDecisionAction::Inspect,
                OperatorDecisionSourceState::InsufficientEvidence,
                OperatorDecisionSeverity::Info,
                "budget_evidence_insufficient",
            )
        };
        push_source(
            sources,
            SourceInput {
                kind: OperatorDecisionSourceKind::Budget,
                id: artifact_id.to_string(),
                resource: resource.to_string(),
                conflict_key: format!("{resource}:control"),
                action,
                state,
                severity,
                confidence,
                observed_at,
                expires_at: None,
                reason,
                evidence_references: vec![original_reference(
                    "budget_anomaly_finding",
                    artifact_id,
                    first_valid_hash(
                        &artifact,
                        &[
                            &["content_sha256"],
                            &["evidence_sha256"],
                            &["evidence", "content_sha256"],
                            &["evidence", "evidence_sha256"],
                        ],
                    ),
                )],
            },
        )?;
    }
    Ok(())
}

fn collect_benchmark_sources(
    store: &LocalProductStore,
    sources: &mut Vec<OperatorDecisionSource>,
) -> Result<(), String> {
    for artifact in store.regression_report_artifacts(SOURCE_READ_LIMIT)? {
        if string(&artifact, "artifact_kind") != Some("report") {
            continue;
        }
        let Some(artifact_id) = string(&artifact, "artifact_id") else {
            continue;
        };
        let Some(scenario) = string(&artifact, "scenario_id") else {
            continue;
        };
        let Some(observed_at) = string(&artifact, "created_at") else {
            continue;
        };
        let outcome = artifact
            .pointer("/report/outcome")
            .and_then(Value::as_str)
            .unwrap_or("incomparable");
        let (state, severity, reason) = match outcome {
            "regression" | "quality_failure" => (
                OperatorDecisionSourceState::Actionable,
                OperatorDecisionSeverity::Warning,
                "benchmark_regression_observed",
            ),
            "pass" => (
                OperatorDecisionSourceState::Resolved,
                OperatorDecisionSeverity::Info,
                "benchmark_passed",
            ),
            _ => (
                OperatorDecisionSourceState::InsufficientEvidence,
                OperatorDecisionSeverity::Info,
                "benchmark_incomparable",
            ),
        };
        let confidence = if matches!(state, OperatorDecisionSourceState::InsufficientEvidence) {
            0.0
        } else {
            1.0
        };
        push_source(
            sources,
            SourceInput {
                kind: OperatorDecisionSourceKind::Benchmark,
                id: artifact_id.to_string(),
                resource: scenario.to_string(),
                conflict_key: format!("benchmark:{scenario}"),
                action: OperatorDecisionAction::Inspect,
                state,
                severity,
                confidence,
                observed_at,
                expires_at: None,
                reason,
                evidence_references: vec![original_reference(
                    "token_efficiency_regression_artifact",
                    artifact_id,
                    first_valid_hash(
                        &artifact,
                        &[&["content_sha256"], &["report", "report_sha256"]],
                    ),
                )],
            },
        )?;
    }
    Ok(())
}

fn collect_policy_sources(
    store: &LocalProductStore,
    sources: &mut Vec<OperatorDecisionSource>,
) -> Result<(), String> {
    let response = store.list_policy_proposals(SOURCE_READ_LIMIT, 0, None)?;
    for proposal in response
        .get("proposals")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(id) = string(proposal, "proposal_id") else {
            continue;
        };
        let Some(observed_at) =
            string(proposal, "updated_at").or_else(|| string(proposal, "created_at"))
        else {
            continue;
        };
        let status = string(proposal, "status").unwrap_or("pending");
        let state = if matches!(status, "rejected" | "rolled_back" | "deactivated") {
            OperatorDecisionSourceState::Resolved
        } else {
            OperatorDecisionSourceState::Actionable
        };
        let reason = if matches!(state, OperatorDecisionSourceState::Resolved) {
            "policy_proposal_resolved"
        } else {
            "policy_proposal_requires_owner_specific_control"
        };
        push_source(
            sources,
            SourceInput {
                kind: OperatorDecisionSourceKind::Policy,
                id: id.to_string(),
                resource: id.to_string(),
                conflict_key: format!("policy:{id}"),
                action: OperatorDecisionAction::Inspect,
                state,
                severity: OperatorDecisionSeverity::Info,
                confidence: 1.0,
                observed_at,
                expires_at: None,
                reason,
                evidence_references: vec![original_reference("policy_proposal", id, None)],
            },
        )?;
    }
    let active_policy_hashes = store
        .active_adaptive_fusion_policies()?
        .into_iter()
        .map(|policy| (policy.policy_key, policy.policy_hash))
        .collect::<BTreeMap<_, _>>();
    for snapshot in store.adaptive_fusion_policy_snapshots()? {
        if snapshot.status != "active" || !snapshot.hash_is_valid() {
            continue;
        }
        if active_policy_hashes.get(&snapshot.policy_key)
            != Some(&snapshot.promoted_policy.policy_hash)
        {
            continue;
        }
        push_source(
            sources,
            SourceInput {
                kind: OperatorDecisionSourceKind::Rollback,
                id: snapshot.adjustment_id.clone(),
                resource: snapshot.adjustment_id.clone(),
                conflict_key: format!("adaptive-policy:{}:rollback", snapshot.policy_key),
                action: OperatorDecisionAction::Rollback,
                state: OperatorDecisionSourceState::Actionable,
                severity: OperatorDecisionSeverity::Warning,
                confidence: 1.0,
                observed_at: &snapshot.updated_at,
                expires_at: None,
                reason: "active_adaptive_policy_snapshot_can_rollback",
                evidence_references: vec![original_reference(
                    "adaptive_policy_snapshot",
                    &snapshot.adjustment_id,
                    Some(snapshot.safety_hash.clone()),
                )],
            },
        )?;
    }
    Ok(())
}

fn collect_scheduler_sources(
    store: &LocalProductStore,
    sources: &mut Vec<OperatorDecisionSource>,
) -> Result<(), String> {
    let Some(heartbeat) = store.read_heartbeat()? else {
        return Ok(());
    };
    if heartbeat.updated_at.is_empty() {
        return Ok(());
    }
    let heartbeat_id = format!(
        "scheduler-heartbeat-{}-{}",
        heartbeat.tick_count, heartbeat.error_count
    );
    push_source(
        sources,
        SourceInput {
            kind: OperatorDecisionSourceKind::Scheduler,
            id: heartbeat_id.clone(),
            resource: "scheduler".to_string(),
            conflict_key: "scheduler:control".to_string(),
            action: if heartbeat.error_count > 0 {
                OperatorDecisionAction::Acknowledge
            } else {
                OperatorDecisionAction::Inspect
            },
            state: if heartbeat.error_count > 0 {
                OperatorDecisionSourceState::Actionable
            } else {
                OperatorDecisionSourceState::Resolved
            },
            severity: if heartbeat.error_count > 0 {
                OperatorDecisionSeverity::Warning
            } else {
                OperatorDecisionSeverity::Info
            },
            confidence: 1.0,
            observed_at: &heartbeat.updated_at,
            expires_at: None,
            reason: if heartbeat.error_count > 0 {
                "scheduler_errors_observed"
            } else {
                "scheduler_heartbeat_healthy"
            },
            evidence_references: vec![original_reference(
                "scheduler_heartbeat",
                &heartbeat_id,
                Some(stable_hash(&json!({
                    "updated_at": heartbeat.updated_at,
                    "tick_count": heartbeat.tick_count,
                    "error_count": heartbeat.error_count,
                }))),
            )],
        },
    )?;
    Ok(())
}

fn collect_rollback_recovery_sources(
    store: &LocalProductStore,
    sources: &mut Vec<OperatorDecisionSource>,
) -> Result<(), String> {
    for event in store.audit_events(SOURCE_READ_LIMIT)? {
        let Some(action) = string(&event, "action") else {
            continue;
        };
        let is_rollback = action.contains("rollback");
        let is_recovery =
            action.contains("resume") || action.contains("override") || action.contains("recover");
        if !is_rollback && !is_recovery {
            continue;
        }
        let Some(id) = event
            .get("audit_id")
            .and_then(Value::as_i64)
            .map(|id| format!("audit-{id}"))
        else {
            continue;
        };
        let Some(resource) = string(&event, "resource") else {
            continue;
        };
        let Some(observed_at) = string(&event, "created_at") else {
            continue;
        };
        push_source(
            sources,
            SourceInput {
                kind: if is_rollback {
                    OperatorDecisionSourceKind::Rollback
                } else {
                    OperatorDecisionSourceKind::Recovery
                },
                id: id.clone(),
                resource: resource.to_string(),
                conflict_key: format!("{resource}:control"),
                action: if is_rollback {
                    OperatorDecisionAction::Rollback
                } else {
                    OperatorDecisionAction::Resume
                },
                state: OperatorDecisionSourceState::Resolved,
                severity: OperatorDecisionSeverity::Info,
                confidence: 1.0,
                observed_at,
                expires_at: None,
                reason: if is_rollback {
                    "rollback_recorded"
                } else {
                    "recovery_recorded"
                },
                evidence_references: vec![original_reference("audit_event", &id, None)],
            },
        )?;
    }
    Ok(())
}

struct SourceInput<'a> {
    kind: OperatorDecisionSourceKind,
    id: String,
    resource: String,
    conflict_key: String,
    action: OperatorDecisionAction,
    state: OperatorDecisionSourceState,
    severity: OperatorDecisionSeverity,
    confidence: f64,
    observed_at: &'a str,
    expires_at: Option<&'a str>,
    reason: &'a str,
    evidence_references: Vec<OperatorDecisionEvidenceReference>,
}

fn push_source(
    sources: &mut Vec<OperatorDecisionSource>,
    input: SourceInput<'_>,
) -> Result<(), String> {
    let mut source = OperatorDecisionSource {
        schema_version: OPERATOR_DECISION_SOURCE_SCHEMA_VERSION.to_string(),
        source_kind: input.kind,
        source_id: input.id,
        resource_id: input.resource,
        conflict_key: input.conflict_key,
        action: input.action,
        state: input.state,
        severity: input.severity,
        confidence: input.confidence,
        observed_at: input.observed_at.to_string(),
        expires_at: input.expires_at.map(str::to_string),
        reason_codes: vec![input.reason.to_string()],
        evidence_references: input.evidence_references,
        evidence_sha256: String::new(),
    };
    source.seal()?;
    sources.push(source);
    Ok(())
}

fn original_reference(
    evidence_type: &str,
    evidence_id: &str,
    content_sha256: Option<String>,
) -> OperatorDecisionEvidenceReference {
    OperatorDecisionEvidenceReference {
        evidence_type: evidence_type.to_string(),
        evidence_id: evidence_id.to_string(),
        content_sha256,
    }
}

fn first_valid_hash(value: &Value, paths: &[&[&str]]) -> Option<String> {
    paths.iter().find_map(|path| {
        let mut current = value;
        for part in *path {
            current = current.get(*part)?;
        }
        let hash = current.as_str()?;
        if hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            Some(hash.to_ascii_lowercase())
        } else {
            None
        }
    })
}

fn string<'a>(value: &'a Value, field: &str) -> Option<&'a str> {
    value.get(field).and_then(Value::as_str)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use serde_json::json;
    use tempfile::tempdir;

    use crate::operator_decision::{OperatorDecisionAction, OperatorDecisionOutcome};

    use super::*;

    const NOW: &str = "2026-07-11T00:01:00Z";

    fn test_store(path: impl AsRef<Path>) -> LocalProductStore {
        LocalProductStore::new_with_clock(path, || NOW.to_string()).unwrap()
    }

    fn create_run_with_nodes(
        store: &LocalProductStore,
        name: &str,
        nodes: Value,
        edges: Value,
    ) -> String {
        let plan = store
            .create_workflow_plan(name, "test", "operator", |ids, _| {
                Ok(json!({
                    "status": "planned_read_only",
                    "graph": {
                        "nodes": nodes,
                        "edges": edges,
                        "workflow_id": ids.workflow_id,
                        "dispatch_id": ids.dispatch_id
                    },
                    "analysis": {},
                    "boundaries": {"execution_authority": "disabled"}
                }))
            })
            .unwrap();
        store
            .create_workflow_run_from_plan(plan["plan_id"].as_str().unwrap(), "operator")
            .unwrap()["run_id"]
            .as_str()
            .unwrap()
            .to_string()
    }

    fn create_empty_run(store: &LocalProductStore, name: &str) -> String {
        create_run_with_nodes(store, name, json!([]), json!([]))
    }

    fn request_approval(store: &LocalProductStore, run_id: &str, node_id: &str) {
        store
            .record_workflow_run_approval(
                run_id,
                node_id,
                "requested",
                "operator",
                Some("review required"),
                None,
                None,
                None,
                Some("2026-07-11T01:01:00Z"),
            )
            .unwrap();
    }

    fn set_run_status(store: &LocalProductStore, run_id: &str, status: &str) {
        store
            .with_conn(|connection| {
                connection
                    .execute(
                        "UPDATE workflow_runs SET status = ?1, updated_at = ?2 WHERE run_id = ?3",
                        rusqlite::params![status, NOW, run_id],
                    )
                    .map_err(|error| error.to_string())?;
                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn queue_is_empty_bounded_and_hash_bound_without_sources() {
        let directory = tempdir().unwrap();
        let store = test_store(directory.path().join("store.db"));
        let queue = store.operator_decision_queue(NOW, 300, 500, -5).unwrap();
        assert_eq!(queue.total, 0);
        assert_eq!(queue.limit, 100);
        assert_eq!(queue.offset, 0);
        assert!(queue.items.is_empty());
        queue.validate().unwrap();
    }

    #[test]
    fn pending_approval_exposes_exact_approve_and_reject_decisions_with_original_reference() {
        let directory = tempdir().unwrap();
        let store = test_store(directory.path().join("store.db"));
        let run = create_empty_run(&store, "approval");
        request_approval(&store, &run, "node-a");
        let queue = store.operator_decision_queue(NOW, 300, 100, 0).unwrap();
        let actions = queue
            .items
            .iter()
            .filter(|item| item.resource_id == run)
            .filter_map(|item| item.recommended_action)
            .collect::<BTreeSet<_>>();
        assert_eq!(
            actions,
            BTreeSet::from([
                OperatorDecisionAction::Approve,
                OperatorDecisionAction::Reject
            ])
        );
        assert!(queue.items.iter().all(|item| {
            item.evidence_references
                .iter()
                .all(|reference| reference.content_sha256.is_some())
        }));
    }

    #[test]
    fn derived_approval_source_binds_original_evidence_without_fabricated_hash() {
        let directory = tempdir().unwrap();
        let store = test_store(directory.path().join("store.db"));
        let run = create_empty_run(&store, "approval-evidence");
        request_approval(&store, &run, "node-a");
        let mut sources = Vec::new();
        collect_workflow_and_approval_sources(&store, &mut sources).unwrap();
        let approval = sources
            .iter()
            .find(|source| {
                source.source_kind == OperatorDecisionSourceKind::Approval
                    && source.action == OperatorDecisionAction::Approve
            })
            .unwrap();
        assert_eq!(approval.evidence_references.len(), 1);
        assert_eq!(
            approval.evidence_references[0].evidence_type,
            "workflow_run_approval"
        );
        assert!(approval.evidence_references[0].content_sha256.is_none());
        let mut tampered = approval.clone();
        tampered.evidence_references[0].evidence_id = "other".to_string();
        assert!(tampered.validate().unwrap_err().contains("hash mismatch"));
    }

    #[test]
    fn latest_approval_state_resolves_old_requested_decisions() {
        let directory = tempdir().unwrap();
        let store = test_store(directory.path().join("store.db"));
        let run = create_empty_run(&store, "approval");
        request_approval(&store, &run, "node-a");
        store
            .record_workflow_run_approval(
                &run, "node-a", "approved", "operator", None, None, None, None, None,
            )
            .unwrap();
        let queue = store.operator_decision_queue(NOW, 300, 100, 0).unwrap();
        assert!(queue
            .items
            .iter()
            .filter(|item| item.resource_id == run)
            .all(|item| item.outcome != OperatorDecisionOutcome::Ready));
    }

    #[test]
    fn blocked_ready_node_is_retryable_but_terminal_failed_is_not() {
        let directory = tempdir().unwrap();
        let store = test_store(directory.path().join("store.db"));
        let blocked = create_run_with_nodes(
            &store,
            "blocked",
            json!([{"node_id": "n1", "task_type": "noop", "status": "pending"}]),
            json!([]),
        );
        set_run_status(&store, &blocked, "blocked");
        let failed = create_empty_run(&store, "failed");
        set_run_status(&store, &failed, "failed");
        let queue = store.operator_decision_queue(NOW, 300, 100, 0).unwrap();
        assert!(queue.items.iter().any(|item| {
            item.resource_id == blocked
                && item.recommended_action == Some(OperatorDecisionAction::Retry)
        }));
        assert!(queue.items.iter().all(|item| {
            item.resource_id != failed
                || item.recommended_action != Some(OperatorDecisionAction::Retry)
        }));
    }

    #[test]
    fn blocked_without_ready_node_is_not_recommended_for_retry() {
        let directory = tempdir().unwrap();
        let store = test_store(directory.path().join("store.db"));
        let run = create_run_with_nodes(
            &store,
            "blocked-no-ready",
            json!([
                {"node_id": "a", "task_type": "noop", "status": "failed"},
                {"node_id": "b", "task_type": "noop", "status": "pending"}
            ]),
            json!([{
                "edge_id": "a-b",
                "from_node_id": "a",
                "to_node_id": "b",
                "edge_type": "dependency"
            }]),
        );
        set_run_status(&store, &run, "blocked");
        let queue = store.operator_decision_queue(NOW, 300, 100, 0).unwrap();
        assert!(queue.items.iter().all(|item| {
            item.resource_id != run
                || item.recommended_action != Some(OperatorDecisionAction::Retry)
        }));
    }

    #[test]
    fn queue_derivation_is_deterministic_read_only_and_restart_safe() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("store.db");
        let store = test_store(&path);
        let first_run = create_empty_run(&store, "first");
        let second_run = create_empty_run(&store, "second");
        request_approval(&store, &first_run, "node-a");
        request_approval(&store, &second_run, "node-b");
        let audits_before = store.audit_events(100).unwrap();

        let first = store.operator_decision_queue(NOW, 300, 100, 0).unwrap();
        let repeated = store.operator_decision_queue(NOW, 300, 100, 0).unwrap();

        assert_eq!(first, repeated);
        assert_eq!(store.audit_events(100).unwrap(), audits_before);
        assert_eq!(first.total, 4);

        drop(store);
        let reopened = test_store(&path);
        assert_eq!(
            reopened.operator_decision_queue(NOW, 300, 100, 0).unwrap(),
            first
        );
    }

    #[test]
    fn queue_pagination_is_stable_and_reports_unpaged_total() {
        let directory = tempdir().unwrap();
        let store = test_store(directory.path().join("store.db"));
        for index in 0..3 {
            let run = create_empty_run(&store, &format!("run-{index}"));
            request_approval(&store, &run, &format!("node-{index}"));
        }

        let first = store.operator_decision_queue(NOW, 300, 2, 0).unwrap();
        let second = store.operator_decision_queue(NOW, 300, 2, 2).unwrap();

        assert_eq!(first.total, 6);
        assert_eq!(first.items.len(), 2);
        assert_eq!(second.items.len(), 2);
        assert!(first
            .items
            .iter()
            .all(|item| item.decision_id != second.items[0].decision_id));
    }

    #[test]
    fn queue_fails_closed_when_a_source_owner_cannot_be_read() {
        let directory = tempdir().unwrap();
        let store = test_store(directory.path().join("store.db"));
        store
            .with_conn(|connection| {
                connection
                    .execute(
                        "INSERT INTO regression_report_artifacts
                         (artifact_sequence, artifact_id, artifact_kind, report_schema_version,
                          registry_id, registry_sha256, scenario_id, content_sha256,
                          created_at, artifact_json)
                         VALUES (1, 'broken', 'report', 'token_efficiency_regression_report.v1',
                                 'registry', '00', 'scenario', '00', ?1, '{')",
                        [NOW],
                    )
                    .map_err(|error| error.to_string())?;
                Ok(())
            })
            .unwrap();

        assert!(store.operator_decision_queue(NOW, 300, 100, 0).is_err());
    }

    #[test]
    fn scheduler_acknowledgement_resolves_exact_hash_without_approving() {
        let directory = tempdir().unwrap();
        let store = test_store(directory.path().join("store.db"));
        store.write_heartbeat(4, 1, 10.0, "{}").unwrap();
        let queue = store.operator_decision_queue(NOW, 300, 100, 0).unwrap();
        let item = queue
            .items
            .iter()
            .find(|item| item.conflict_key == "scheduler:control")
            .unwrap();
        assert_eq!(
            item.recommended_action,
            Some(OperatorDecisionAction::Acknowledge)
        );
        let reference = item.selected_source.as_ref().unwrap();
        let acknowledgement = store
            .acknowledge_operator_source(
                &item.decision_id,
                &reference.evidence_type,
                &reference.evidence_id,
                reference.content_sha256.as_deref().unwrap(),
                Some("observed, not approved"),
                "operator",
            )
            .unwrap();
        assert_eq!(acknowledgement["approval_granted"], false);

        let after = store.operator_decision_queue(NOW, 300, 100, 0).unwrap();
        assert!(after.items.iter().all(|candidate| {
            candidate.conflict_key != "scheduler:control"
                || candidate.recommended_action != Some(OperatorDecisionAction::Acknowledge)
        }));
    }
}
