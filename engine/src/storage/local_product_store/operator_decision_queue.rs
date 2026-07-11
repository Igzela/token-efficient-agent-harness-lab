use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use crate::operator_decision::{
    derive_operator_decision_item, OperatorDecisionAction, OperatorDecisionQueue,
    OperatorDecisionSeverity, OperatorDecisionSource, OperatorDecisionSourceKind,
    OperatorDecisionSourceState, OPERATOR_DECISION_QUEUE_SCHEMA_VERSION,
    OPERATOR_DECISION_SOURCE_SCHEMA_VERSION,
};

use super::LocalProductStore;

const SOURCE_READ_LIMIT: i64 = 100;

impl LocalProductStore {
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
                .entry(source_kind_name(&source.source_kind))
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
    for run in store.list_workflow_runs_with_offset(SOURCE_READ_LIMIT, 0)? {
        let Some(run_id) = string(&run, "run_id") else {
            continue;
        };
        let status = string(&run, "status").unwrap_or("unknown");
        let observed = string(&run, "updated_at").or_else(|| string(&run, "created_at"));
        if let Some(observed_at) = observed {
            let pause_reason = run.get("pause_reason").and_then(Value::as_str);
            if pause_reason.is_some() {
                push_source(
                    sources,
                    SourceInput {
                        kind: OperatorDecisionSourceKind::Workflow,
                        id: format!("workflow-pause-{run_id}"),
                        resource: run_id.to_string(),
                        conflict_key: format!("{run_id}:control"),
                        action: OperatorDecisionAction::Resume,
                        state: OperatorDecisionSourceState::Actionable,
                        severity: OperatorDecisionSeverity::Warning,
                        confidence: 1.0,
                        observed_at,
                        expires_at: None,
                        reason: "workflow_paused",
                    },
                )?;
            } else if matches!(status, "failed" | "blocked") {
                push_source(
                    sources,
                    SourceInput {
                        kind: OperatorDecisionSourceKind::Workflow,
                        id: format!("workflow-status-{run_id}"),
                        resource: run_id.to_string(),
                        conflict_key: format!("{run_id}:execution"),
                        action: OperatorDecisionAction::Retry,
                        state: OperatorDecisionSourceState::Actionable,
                        severity: OperatorDecisionSeverity::Warning,
                        confidence: 1.0,
                        observed_at,
                        expires_at: None,
                        reason: "workflow_failed_or_blocked",
                    },
                )?;
            }
        }
        for approval in store.workflow_run_approvals(run_id, SOURCE_READ_LIMIT)? {
            let Some(approval_id) = string(&approval, "approval_id") else {
                continue;
            };
            let Some(node_id) = string(&approval, "node_id") else {
                continue;
            };
            let Some(created_at) = string(&approval, "created_at") else {
                continue;
            };
            let decision = string(&approval, "decision").unwrap_or("requested");
            let (action, state, reason) = match decision {
                "approved" => (
                    OperatorDecisionAction::Approve,
                    OperatorDecisionSourceState::Resolved,
                    "approval_completed",
                ),
                "rejected" => (
                    OperatorDecisionAction::Reject,
                    OperatorDecisionSourceState::Resolved,
                    "rejection_completed",
                ),
                _ => (
                    OperatorDecisionAction::Approve,
                    OperatorDecisionSourceState::Actionable,
                    "approval_requested",
                ),
            };
            push_source(
                sources,
                SourceInput {
                    kind: OperatorDecisionSourceKind::Approval,
                    id: approval_id.to_string(),
                    resource: run_id.to_string(),
                    conflict_key: format!("{run_id}:{node_id}:approval"),
                    action,
                    state,
                    severity: OperatorDecisionSeverity::Warning,
                    confidence: 1.0,
                    observed_at: created_at,
                    expires_at: string(&approval, "expires_at"),
                    reason,
                },
            )?;
        }
    }
    Ok(())
}

fn collect_budget_sources(
    store: &LocalProductStore,
    sources: &mut Vec<OperatorDecisionSource>,
) -> Result<(), String> {
    for artifact in store.budget_evidence_artifacts(Some("anomaly"), SOURCE_READ_LIMIT, 0)? {
        let Some(artifact_id) = string(&artifact, "artifact_id") else {
            continue;
        };
        let evidence = artifact.get("evidence").unwrap_or(&Value::Null);
        let scope = evidence.get("scope").unwrap_or(&Value::Null);
        let resource = string(scope, "run_id")
            .or_else(|| string(scope, "workspace_id"))
            .or_else(|| string(scope, "provider_id"));
        let Some(resource) = resource else { continue };
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
        let (action, state, severity, reason) = if outcome == "supported" && detected {
            (
                if critical {
                    OperatorDecisionAction::Pause
                } else {
                    OperatorDecisionAction::Inspect
                },
                OperatorDecisionSourceState::Actionable,
                if critical {
                    OperatorDecisionSeverity::Critical
                } else {
                    OperatorDecisionSeverity::Warning
                },
                "budget_anomaly_detected",
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
                "benchmark_regression",
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
        let state = if matches!(status, "pending" | "proposed") {
            OperatorDecisionSourceState::Actionable
        } else if matches!(status, "rejected" | "rolled_back" | "deactivated") {
            OperatorDecisionSourceState::Resolved
        } else {
            OperatorDecisionSourceState::Informational
        };
        let reason = if matches!(state, OperatorDecisionSourceState::Actionable) {
            "policy_proposal_pending"
        } else {
            "policy_proposal_not_pending"
        };
        push_source(
            sources,
            SourceInput {
                kind: OperatorDecisionSourceKind::Policy,
                id: id.to_string(),
                resource: id.to_string(),
                conflict_key: format!("policy:{id}"),
                action: OperatorDecisionAction::Approve,
                state,
                severity: OperatorDecisionSeverity::Warning,
                confidence: 1.0,
                observed_at,
                expires_at: None,
                reason,
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
    push_source(
        sources,
        SourceInput {
            kind: OperatorDecisionSourceKind::Scheduler,
            id: format!(
                "scheduler-heartbeat-{}-{}",
                heartbeat.tick_count, heartbeat.error_count
            ),
            resource: "scheduler".to_string(),
            conflict_key: "scheduler:control".to_string(),
            action: OperatorDecisionAction::Inspect,
            state: if heartbeat.error_count > 0 {
                OperatorDecisionSourceState::Actionable
            } else {
                OperatorDecisionSourceState::Informational
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
                id,
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
        evidence_references: vec![],
        evidence_sha256: String::new(),
    };
    source.seal()?;
    sources.push(source);
    Ok(())
}

fn string<'a>(value: &'a Value, field: &str) -> Option<&'a str> {
    value.get(field).and_then(Value::as_str)
}

fn source_kind_name(kind: &OperatorDecisionSourceKind) -> String {
    format!("{kind:?}").to_ascii_lowercase()
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

    fn create_run(store: &LocalProductStore, name: &str) -> String {
        let plan = store
            .create_workflow_plan(name, "test", "operator", |ids, _| {
                Ok(json!({
                    "status": "planned_read_only",
                    "graph": {
                        "nodes": [],
                        "edges": [],
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

    #[test]
    fn queue_is_empty_bounded_and_hash_bound_without_sources() {
        let directory = tempdir().unwrap();
        let store = test_store(directory.path().join("store.db"));

        let queue = store.operator_decision_queue(NOW, 300, 500, -5).unwrap();

        assert_eq!(queue.total, 0);
        assert_eq!(queue.limit, 100);
        assert_eq!(queue.offset, 0);
        assert!(queue.items.is_empty());
        assert!(queue.source_counts.is_empty());
        queue.validate().unwrap();
    }

    #[test]
    fn queue_derivation_is_deterministic_read_only_and_run_isolated() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("store.db");
        let store = test_store(&path);
        let first_run = create_run(&store, "first");
        let second_run = create_run(&store, "second");
        request_approval(&store, &first_run, "node-a");
        request_approval(&store, &second_run, "node-b");
        let audits_before = store.audit_events(100).unwrap();

        let first = store.operator_decision_queue(NOW, 300, 100, 0).unwrap();
        let repeated = store.operator_decision_queue(NOW, 300, 100, 0).unwrap();

        assert_eq!(first, repeated);
        assert_eq!(store.audit_events(100).unwrap(), audits_before);
        assert_eq!(first.total, 2);
        assert_eq!(first.source_counts.get("approval"), Some(&2));
        let keys = first
            .items
            .iter()
            .map(|item| item.conflict_key.as_str())
            .collect::<BTreeSet<_>>();
        assert!(keys.contains(format!("{first_run}:node-a:approval").as_str()));
        assert!(keys.contains(format!("{second_run}:node-b:approval").as_str()));
        assert!(first.items.iter().all(|item| {
            item.outcome == OperatorDecisionOutcome::Ready
                && item.recommended_action == Some(OperatorDecisionAction::Approve)
        }));

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
            let run = create_run(&store, &format!("run-{index}"));
            request_approval(&store, &run, &format!("node-{index}"));
        }

        let first = store.operator_decision_queue(NOW, 300, 2, 0).unwrap();
        let second = store.operator_decision_queue(NOW, 300, 2, 2).unwrap();

        assert_eq!(first.total, 3);
        assert_eq!(first.items.len(), 2);
        assert_eq!(second.total, 3);
        assert_eq!(second.items.len(), 1);
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
}
