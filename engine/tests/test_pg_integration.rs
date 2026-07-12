// PostgreSQL integration tests — gated behind pg-tests feature.
// Set ACP_TEST_DATABASE_URL=postgres://user:pass@localhost:5432/testdb to run.
// CI runs these with a PostgreSQL service container.

#[cfg(feature = "pg-tests")]
use engine::budget_forecast::{
    build_budget_forecast, BudgetForecastRequest, BudgetUsageObservation,
};
#[cfg(feature = "pg-tests")]
use engine::budget_manager::{
    BudgetAnomalyFinding, BudgetAnomalyKind, BudgetAnomalyMeasurement, BudgetAnomalySeverity,
    BudgetConfidence, BudgetConfidenceLevel, BudgetEvidenceCoverage, BudgetEvidenceOutcome,
    BudgetEvidenceReference, BudgetEvidenceScope, BudgetEvidenceWindow,
};
#[cfg(feature = "pg-tests")]
use engine::event_schema::canonical_event_json;
#[cfg(feature = "pg-tests")]
use engine::feedback::{
    ContextualPolicyPromotion, ContextualPolicyPromotionGate, ObjectiveProfile,
    CONTEXTUAL_POLICY_PROMOTION_SCHEMA_VERSION,
};
#[cfg(feature = "pg-tests")]
use engine::storage::local_product_store::BudgetAutoPausePolicy;
#[cfg(feature = "pg-tests")]
use engine::storage::local_product_store::LocalProductStore;
#[cfg(feature = "pg-tests")]
use serde_json::{json, Value};
#[cfg(feature = "pg-tests")]
use sha2::{Digest, Sha256};

#[cfg(feature = "pg-tests")]
fn utc_now_string() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

/// Returns a connected Postgres-backed LocalProductStore, or skips the test
/// by returning None when ACP_TEST_DATABASE_URL is not set.
#[cfg(feature = "pg-tests")]
fn test_store() -> Option<LocalProductStore> {
    let url = match std::env::var("ACP_TEST_DATABASE_URL") {
        Ok(u) => u,
        Err(_) => {
            eprintln!("ACP_TEST_DATABASE_URL not set; skipping pg-tests");
            return None;
        }
    };
    let store =
        LocalProductStore::new_postgres(&url, utc_now_string).expect("new_postgres should succeed");
    Some(store)
}

#[cfg(feature = "pg-tests")]
fn uuid_tag() -> String {
    uuid::Uuid::new_v4().to_string()
}

#[cfg(feature = "pg-tests")]
fn pg_regression_report(tag: &str) -> Value {
    let mut report = json!({
        "schema_version": "token_efficiency_regression_report.v1",
        "registry_id": format!("pe1-pg-{tag}"),
        "registry_sha256": "11".repeat(32),
        "scenario_id": format!("scenario-{tag}"),
        "scenario_digest": "22".repeat(32),
        "task_digest": "33".repeat(32),
        "read_only": true,
        "report_only": true,
        "provider_calls": "disabled",
        "mutation_authority": "none",
        "outcome": "pass",
        "reason_codes": [],
        "evidence": {},
        "comparisons": {}
    });
    let canonical = canonical_event_json(&report).expect("canonical report");
    report["report_sha256"] = json!(hex::encode(Sha256::digest(canonical.as_bytes())));
    report
}

#[cfg(feature = "pg-tests")]
fn pg_budget_forecast(tag: &str) -> engine::budget_manager::BudgetForecastEvidence {
    let observations = (0..3)
        .map(|index| BudgetUsageObservation {
            evidence_type: "provider_audit_event".to_string(),
            evidence_id: format!("pg-budget-{tag}-{index}"),
            content_sha256: Some(format!("{:064x}", index)),
            occurred_at: format!("2026-07-10T00:{:02}:00Z", 10 + index),
            run_id: None,
            workspace_id: None,
            provider_id: Some("provider-a".to_string()),
            model_id: Some("model-a".to_string()),
            input_tokens: Some(10),
            output_tokens: Some(10),
            total_tokens: Some(20),
            cost_usd: Some(0.01),
        })
        .collect::<Vec<_>>();
    build_budget_forecast(
        &BudgetForecastRequest {
            forecast_id: format!("forecast-{tag}"),
            scope: BudgetEvidenceScope {
                provider_id: Some("provider-a".to_string()),
                ..Default::default()
            },
            start_inclusive: "2026-07-10T00:00:00Z".to_string(),
            end_exclusive: "2026-07-10T01:00:00Z".to_string(),
            generated_at: "2026-07-10T01:01:00Z".to_string(),
            horizon_seconds: 60,
            remaining_tokens: Some(100),
            remaining_cost_usd: Some(1.0),
            required_dimensions: vec!["provider_id".to_string()],
            min_samples: 3,
            max_freshness_seconds: 600,
            max_duplicate_events: 1,
        },
        &observations,
    )
    .expect("build pg budget forecast")
}

#[cfg(feature = "pg-tests")]
fn pg_budget_anomaly(run_id: &str, tag: &str) -> BudgetAnomalyFinding {
    let mut finding = BudgetAnomalyFinding {
        schema_version: "budget_anomaly_finding.v1".to_string(),
        finding_id: format!("pg-anomaly-{tag}"),
        scope: BudgetEvidenceScope {
            run_id: Some(run_id.to_string()),
            ..Default::default()
        },
        outcome: BudgetEvidenceOutcome::Supported,
        window: BudgetEvidenceWindow {
            start_inclusive: "2026-07-11T00:00:00Z".to_string(),
            end_exclusive: "2026-07-11T00:10:00Z".to_string(),
            generated_at: "2026-07-11T00:10:10Z".to_string(),
            freshness_seconds: 10,
            sample_count: 3,
        },
        coverage: BudgetEvidenceCoverage {
            required_dimensions: vec!["run_id".to_string()],
            observed_dimensions: vec!["run_id".to_string()],
            pricing_complete: true,
            duplicate_events: 0,
            missing_fields: vec![],
        },
        confidence: BudgetConfidence {
            level: BudgetConfidenceLevel::High,
            score: 0.99,
            reason_codes: vec!["stable_baseline".to_string()],
        },
        reason_codes: vec!["token_spike".to_string()],
        evidence_references: vec![BudgetEvidenceReference {
            evidence_type: "provider_audit_event".to_string(),
            evidence_id: format!("event-{tag}"),
            content_sha256: Some("a".repeat(64)),
        }],
        detected: true,
        anomaly_kind: Some(BudgetAnomalyKind::TokenSpike),
        severity: Some(BudgetAnomalySeverity::Critical),
        measurement: Some(BudgetAnomalyMeasurement {
            metric: "total_tokens".to_string(),
            observed: 200.0,
            baseline: 100.0,
            threshold: 1.5,
            normalized_delta: 1.0,
        }),
        evidence_sha256: String::new(),
    };
    finding.seal().unwrap();
    finding
}

#[test]
#[cfg(feature = "pg-tests")]
fn pg_new_postgres_creates_store() {
    let Some(_store) = test_store() else { return };
}

#[test]
#[cfg(feature = "pg-tests")]
fn pg_ddl_and_migration() {
    let Some(store) = test_store() else { return };
    // Verify DDL ran: schema_migrations table exists (created by run_pg_migrations).
    // We prove it by upserting a config key — if tables don't exist this will fail.
    let key = format!("ddl-test-{}", uuid_tag());
    store
        .set_config_value(&key, json!({"ok": true}), "test")
        .expect("set_config_value should succeed after DDL+migration");
    let snap = store.config_snapshot().expect("config_snapshot");
    assert!(
        snap.get(&key).is_some(),
        "config key written after DDL should be readable"
    );
}

#[test]
#[cfg(feature = "pg-tests")]
fn pg_config_upsert_read() {
    let Some(store) = test_store() else { return };
    let key = format!("test-key-{}", uuid_tag());
    let value = json!({"nested": true, "count": 42});
    store
        .set_config_value(&key, value.clone(), "test-actor")
        .expect("set_config_value");
    let snap = store.config_snapshot().expect("config_snapshot");
    let read_back = snap.get(&key).expect("key should exist in config snapshot");
    assert_eq!(*read_back, value, "round-tripped JSON must match");
}

#[test]
#[cfg(feature = "pg-tests")]
fn pg_regression_report_artifact_is_idempotent_and_readable() {
    let Some(store) = test_store() else { return };
    let report = pg_regression_report(&uuid_tag());
    let first = store
        .record_regression_report_artifact(&report, "pg-test")
        .expect("record regression report");
    let repeated = store
        .record_regression_report_artifact(&report, "pg-test")
        .expect("repeat regression report");
    assert_eq!(first, repeated);
    let artifact_id = first["artifact_id"].as_str().expect("artifact id");
    assert_eq!(
        store
            .get_regression_report_artifact(artifact_id)
            .expect("get regression report"),
        Some(first)
    );
    let scenario_id = report["scenario_id"].as_str().expect("scenario id");
    let trend = store
        .regression_report_trend(scenario_id, 10)
        .expect("regression trend");
    assert_eq!(trend["point_count"], 1);
    assert_eq!(trend["latest"]["outcome"], "pass");
}

#[test]
#[cfg(feature = "pg-tests")]
fn pg_budget_evidence_artifact_is_idempotent_and_readable() {
    let Some(store) = test_store() else { return };
    let forecast = pg_budget_forecast(&uuid_tag());
    let first = store
        .record_budget_forecast_evidence(&forecast, "pg-test")
        .expect("record budget forecast");
    let repeated = store
        .record_budget_forecast_evidence(&forecast, "pg-test")
        .expect("repeat budget forecast");
    assert_eq!(first, repeated);
    let artifact_id = first["artifact_id"].as_str().expect("artifact id");
    assert_eq!(
        store
            .get_budget_evidence_artifact(artifact_id)
            .expect("get budget evidence"),
        Some(first.clone())
    );
    assert!(store
        .budget_evidence_artifacts(Some("forecast"), 100, 0)
        .expect("list budget evidence")
        .iter()
        .any(|artifact| artifact["artifact_id"] == first["artifact_id"]));
}

#[test]
#[cfg(feature = "pg-tests")]
fn pg_operator_decision_queue_derives_requested_approval_without_mutation() {
    let Some(store) = test_store() else { return };
    let tag = uuid_tag();
    let plan = store
        .create_workflow_plan(
            &format!("decision queue {tag}"),
            "pg-test",
            "pg-test",
            |ids, _| {
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
            },
        )
        .unwrap();
    let run = store
        .create_workflow_run_from_plan(plan["plan_id"].as_str().unwrap(), "pg-test")
        .unwrap();
    let run_id = run["run_id"].as_str().unwrap();
    store
        .record_workflow_run_approval(
            run_id,
            "node-a",
            "requested",
            "pg-test",
            Some("operator review"),
            None,
            None,
            None,
            None,
        )
        .unwrap();
    let audits_before = store.audit_events(100).unwrap();

    let queue = store
        .operator_decision_queue(&utc_now_string(), 300, 100, 0)
        .unwrap();

    for (suffix, action) in [
        (
            "approve",
            engine::operator_decision::OperatorDecisionAction::Approve,
        ),
        (
            "reject",
            engine::operator_decision::OperatorDecisionAction::Reject,
        ),
    ] {
        let expected_key = format!("{run_id}:node-a:approval:{suffix}");
        let item = queue
            .items
            .iter()
            .find(|item| item.conflict_key == expected_key)
            .expect("requested approval decision");
        assert_eq!(
            item.outcome,
            engine::operator_decision::OperatorDecisionOutcome::Ready
        );
        assert_eq!(item.recommended_action, Some(action));
        let source = item.selected_source.as_ref().expect("selected source");
        assert_eq!(source.evidence_type, "approval");
        assert!(source.evidence_id.ends_with(&format!(":{suffix}")));
    }
    assert_eq!(store.audit_events(100).unwrap(), audits_before);
}

#[test]
#[cfg(feature = "pg-tests")]
fn pg_atomic_requested_approval_resolution_allows_one_winner() {
    use std::sync::{Arc, Barrier};

    let Some(store) = test_store() else { return };
    let tag = uuid_tag();
    let plan = store
        .create_workflow_plan(
            &format!("approval race {tag}"),
            "pg-test",
            "pg-test",
            |ids, _| {
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
            },
        )
        .unwrap();
    let run = store
        .create_workflow_run_from_plan(plan["plan_id"].as_str().unwrap(), "pg-test")
        .unwrap();
    let run_id = run["run_id"].as_str().unwrap().to_string();
    let request = store
        .record_workflow_run_approval(
            &run_id,
            "node-a",
            "requested",
            "pg-test",
            Some("operator review"),
            None,
            None,
            None,
            None,
        )
        .unwrap();
    let request_id = request["approval_id"].as_str().unwrap().to_string();
    let store = Arc::new(store);
    let barrier = Arc::new(Barrier::new(3));
    let mut handles = Vec::new();
    for resolution in ["approved", "rejected"] {
        let store = Arc::clone(&store);
        let barrier = Arc::clone(&barrier);
        let run_id = run_id.clone();
        let request_id = request_id.clone();
        handles.push(std::thread::spawn(move || {
            barrier.wait();
            store.resolve_requested_workflow_run_approval(
                &run_id,
                &request_id,
                resolution,
                "pg-test",
                Some("race"),
            )
        }));
    }
    barrier.wait();
    let results = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(results.iter().filter(|result| result.is_err()).count(), 1);
    let resolved = store
        .workflow_run_approvals(&run_id, 100)
        .unwrap()
        .into_iter()
        .filter(|approval| matches!(approval["decision"].as_str(), Some("approved" | "rejected")))
        .count();
    assert_eq!(resolved, 1);
}

#[test]
#[cfg(feature = "pg-tests")]
fn pg_budget_auto_pause_and_recovery_are_atomic_and_idempotent() {
    let Some(store) = test_store() else { return };
    let tag = uuid_tag();
    let plan = store.create_workflow_plan(&format!("pause {tag}"), "pg-test", "pg-test", |ids, _| Ok(json!({"status":"planned_read_only","graph":{"nodes":[],"edges":[],"workflow_id":ids.workflow_id,"dispatch_id":ids.dispatch_id},"analysis":{},"boundaries":{"execution_authority":"disabled"}}))).unwrap();
    let run = store
        .create_workflow_run_from_plan(plan["plan_id"].as_str().unwrap(), "pg-test")
        .unwrap();
    let run_id = run["run_id"].as_str().unwrap();
    let artifact = store
        .record_budget_anomaly_finding(&pg_budget_anomaly(run_id, &tag), "pg-test")
        .unwrap();
    let policy = BudgetAutoPausePolicy {
        enabled: true,
        ..Default::default()
    };
    let first = store
        .apply_budget_auto_pause(
            artifact["artifact_id"].as_str().unwrap(),
            run_id,
            &policy,
            "pg-test",
        )
        .unwrap();
    let repeated = store
        .apply_budget_auto_pause(
            artifact["artifact_id"].as_str().unwrap(),
            run_id,
            &policy,
            "pg-test",
        )
        .unwrap();
    assert_eq!(first, repeated);
    assert!(
        store.get_workflow_run(run_id).unwrap().unwrap()["pause_reason"]
            .as_str()
            .unwrap()
            .starts_with("budget_auto_pause:")
    );
    let recovered = store
        .recover_budget_auto_pause(run_id, "resume", "pg operator review", "pg-test")
        .unwrap();
    assert_eq!(recovered["state"], "resume");
    assert!(store.get_workflow_run(run_id).unwrap().unwrap()["pause_reason"].is_null());
}

#[test]
#[cfg(feature = "pg-tests")]
fn pg_adaptive_policy_apply_snapshot_and_rollback_cycle() {
    let Some(store) = test_store() else { return };
    let tag = uuid_tag();
    let promotion = ContextualPolicyPromotion {
        schema_version: CONTEXTUAL_POLICY_PROMOTION_SCHEMA_VERSION.to_string(),
        task_class: format!("coding-{tag}"),
        objective: ObjectiveProfile::Quality,
        candidate_id: format!("strong-{tag}"),
        baseline_candidate_id: format!("cheap-{tag}"),
        sample_count: 30,
        confidence: 0.9,
        mean_quality_delta: 0.1,
        mean_cost_reduction: 0.02,
        failure_rate_delta: 0.0,
        evidence_run_ids: (0..30)
            .map(|index| format!("adaptive-pg-run-{tag}-{index}"))
            .collect(),
        risk_level: "low".to_string(),
        confirm_adaptive_policy_promotion: true,
    };
    let verdict = ContextualPolicyPromotionGate::from_flags(true, true).evaluate(&promotion);
    let applied = store
        .apply_adaptive_fusion_policy(&verdict, "pg-test")
        .expect("apply adaptive policy");
    assert_eq!(applied["applied"], true);
    let adjustment_id = applied["adjustment_id"].as_str().unwrap();
    assert!(store
        .active_adaptive_fusion_policies()
        .expect("active policies")
        .iter()
        .any(|policy| policy.task_class == promotion.task_class));

    let rollback = store
        .rollback_adaptive_fusion_policy(adjustment_id, true, "pg-test")
        .expect("rollback adaptive policy");
    assert_eq!(rollback["rolled_back"], true);
}

#[test]
#[cfg(feature = "pg-tests")]
fn pg_plan_create_list_detail() {
    let Some(store) = test_store() else { return };
    let tag = uuid_tag();
    let raw_req = format!("test request {tag}");
    let plan = store
        .create_workflow_plan(&raw_req, "pg-test", "test-actor", |ids, _created_at| {
            Ok(json!({
                "status": "planned_read_only",
                "graph": {"nodes": [], "edges": [], "workflow_id": ids.workflow_id, "dispatch_id": ids.dispatch_id},
                "analysis": {"summary": "test"},
                "boundaries": {"execution_authority": "disabled"},
            }))
        })
        .expect("create_workflow_plan");

    let plan_id = plan["plan_id"].as_str().expect("plan should have plan_id");

    let plans = store
        .search_workflow_plans(10, 0, None)
        .expect("list_workflow_plans");
    assert!(!plans.is_empty(), "at least one plan should be listed");

    let detail = store.get_workflow_plan(plan_id).expect("get_workflow_plan");
    assert!(detail.is_some(), "plan detail should exist");
    assert_eq!(detail.unwrap()["plan_id"].as_str().unwrap(), plan_id);
}

#[test]
#[cfg(feature = "pg-tests")]
fn pg_workflow_run_create_detail() {
    let Some(store) = test_store() else { return };
    let tag = uuid_tag();
    let plan = store
        .create_workflow_plan(
            &format!("run test {tag}"),
            "pg-test",
            "test-actor",
            |ids, _| {
                Ok(json!({
                    "status": "planned_read_only",
                    "graph": {
                        "nodes": [{"node_id": "n1", "task_type": "noop"}],
                        "edges": [],
                        "workflow_id": ids.workflow_id,
                        "dispatch_id": ids.dispatch_id
                    },
                    "analysis": {"summary": "test"},
                    "boundaries": {"execution_authority": "disabled"},
                }))
            },
        )
        .expect("create_workflow_plan");

    let plan_id = plan["plan_id"].as_str().unwrap();
    let run = store
        .create_workflow_run_from_plan(plan_id, "test-actor")
        .expect("create_workflow_run_from_plan");

    let run_id = run["run_id"].as_str().unwrap();
    let detail = store.get_workflow_run(run_id).expect("get_workflow_run");
    assert!(detail.is_some(), "workflow run detail should exist");
    let detail = detail.unwrap();
    assert_eq!(detail["run_id"].as_str().unwrap(), run_id);
    assert_eq!(detail["plan_id"].as_str().unwrap(), plan_id);
    assert_eq!(detail["status"].as_str().unwrap(), "created");
}

#[test]
#[cfg(feature = "pg-tests")]
fn pg_decision_record() {
    let Some(store) = test_store() else { return };
    let run_id = format!("decision-run-{}", uuid_tag());
    let rec = store
        .record_orchestration_decision(
            &run_id,
            Some("node-1"),
            "dispatch",
            "test reason",
            "executor-a",
            None,
            "high",
            0.95,
            &json!({"source": "pg-test"}),
        )
        .expect("record_orchestration_decision");

    assert!(rec.decision_id.starts_with(&format!("decision-{run_id}-")));
    assert_eq!(rec.run_id, run_id);
    assert_eq!(rec.action, "dispatch");

    let found = store
        .get_decision_by_id(&rec.decision_id)
        .expect("get_decision_by_id");
    assert!(found.is_some(), "decision should be retrievable by id");
    assert_eq!(found.unwrap().action, "dispatch");
}

#[test]
#[cfg(feature = "pg-tests")]
fn pg_executor_pool() {
    let Some(store) = test_store() else { return };
    use engine::executor_pool::{
        CostProfile, ExecutorCapabilities, ExecutorMetrics, ExecutorPoolEntry, ExecutorStatus,
    };
    let tag = uuid_tag();
    let entry = ExecutorPoolEntry {
        executor_type: format!("pg-test-exec-{tag}"),
        capabilities: ExecutorCapabilities {
            supported_task_types: vec!["noop".into()],
            supported_task_domains: vec![],
            requires_auth: false,
            requires_cli: false,
            max_timeout_ms: 300_000,
        },
        status: ExecutorStatus {
            available: true,
            active_count: 0,
            concurrency_limit: 10,
            cooldown_until: None,
            failure_score: 0.0,
        },
        cost_profile: CostProfile {
            cost_per_execution_usd: Some(0.01),
            daily_cost_usd: Some(0.0),
            daily_cost_limit_usd: Some(10.0),
        },
        metrics: ExecutorMetrics {
            total_executions: 100,
            successful_executions: 98,
            failed_executions: 2,
            avg_latency_ms: 150.0,
            total_latency_ms: 15_000,
            last_executed_at: None,
        },
    };

    store
        .save_executor_pool_snapshot(&[entry])
        .expect("save_executor_pool_snapshot");

    let pool = store
        .load_executor_pool_snapshot()
        .expect("load_executor_pool_snapshot");
    let found = pool
        .iter()
        .find(|e| e.executor_type == format!("pg-test-exec-{tag}"));
    assert!(found.is_some(), "registered executor should appear in pool");
}

#[test]
#[cfg(feature = "pg-tests")]
fn pg_heartbeat() {
    let Some(store) = test_store() else { return };
    store
        .write_heartbeat(99, 2, 1234.5, r#"{"test":"pg"}"#)
        .expect("write_heartbeat");

    let hb = store
        .read_heartbeat()
        .expect("read_heartbeat")
        .expect("heartbeat row should exist");
    assert_eq!(hb.tick_count, 99);
    assert_eq!(hb.error_count, 2);
    assert!((hb.uptime_seconds - 1234.5).abs() < f64::EPSILON);
    assert_eq!(hb.metadata_json, r#"{"test":"pg"}"#);
}

#[test]
#[cfg(feature = "pg-tests")]
fn pg_audit_record() {
    let Some(store) = test_store() else { return };
    let resource = format!("pg-audit-{}", uuid_tag());
    let result = store
        .append_audit(
            "pg-test-actor",
            "test.action",
            &resource,
            &json!({"tag": "pg"}),
        )
        .expect("append_audit");
    assert!(result["audit_id"].as_i64().unwrap() > 0);

    let events = store
        .search_audit_events(100, 0, Some(&resource))
        .expect("search_audit_events");
    let found = events
        .iter()
        .any(|e| e["resource"].as_str() == Some(&resource));
    assert!(found, "audit entry should be searchable by resource");
}

#[test]
#[cfg(feature = "pg-tests")]
fn pg_provider_audit() {
    let Some(store) = test_store() else { return };
    use engine::provider::ProviderAuditEvent;
    let event_id = format!("pa-{}", uuid_tag());
    let dispatch_id = format!("d-{}", uuid_tag());
    let event = ProviderAuditEvent {
        schema_version: "provider_audit_event.v1".into(),
        event_id: event_id.clone(),
        dispatch_id: dispatch_id.clone(),
        provider_id: "test-provider".into(),
        event_type: "completion".into(),
        input_token_count: Some(100),
        output_token_count: Some(50),
        cost: Some(0.002),
        currency: Some("USD".into()),
        latency_ms: Some(200),
        error_domain: None,
        redaction_status: "redacted".into(),
        created_at: utc_now_string(),
    };
    store
        .record_provider_audit_event(&event)
        .expect("record_provider_audit_event");

    let events = store
        .provider_audit_events_for_dispatch(&dispatch_id)
        .expect("provider_audit_events_for_dispatch");
    let found = events
        .iter()
        .any(|e| e["event_id"].as_str() == Some(&event_id));
    assert!(
        found,
        "provider audit event should be retrievable by dispatch_id"
    );
}

#[test]
#[cfg(feature = "pg-tests")]
fn pg_supervised_patch_metadata() {
    let Some(store) = test_store() else { return };
    let tag = uuid_tag();
    let run_id = format!("sp-run-{tag}");
    let workspace_path = format!("/var/tmp/pg-test-ws-{tag}");
    let workspace = json!({
        "schema_version": "supervised_patch_workspace.v1",
        "workspace_id": format!("ws-{tag}"),
        "run_id": run_id,
        "target_id": "target-1",
        "target_repo_path": "/tmp",
        "target_repo_canonical_path": "/tmp",
        "workspace_path": workspace_path,
        "workspace_canonical_path": workspace_path,
        "source_revision": "abc123",
        "status": "requested",
        "metadata_only": true,
        "execution_authority": "disabled",
        "workspace_directory_creation": "not_performed",
        "target_repository_writes": "disabled",
        "registered_git_worktree": "forbidden",
        "git_worktree_add": "forbidden",
        "process_execution": "disabled",
        "provider_calls": "disabled",
        "push_merge_deploy_apply": "disabled",
    });
    store
        .import_supervised_patch_workspace(&workspace)
        .expect("import_supervised_patch_workspace");

    let workspaces = store
        .supervised_patch_workspaces(100)
        .expect("supervised_patch_workspaces");
    let found = workspaces
        .iter()
        .any(|w| w["workspace_id"].as_str() == Some(&format!("ws-{tag}")));
    assert!(found, "imported workspace should appear in list");

    let ws_id = format!("ws-{tag}");
    let artifact_request = json!({
        "workspace_id": ws_id,
        "patch_hash": format!("sha256:{}", tag),
        "changed_files": ["+file.txt"],
        "redaction_status": "redacted",
    });
    let artifact = store
        .record_supervised_patch_artifact(&artifact_request, "test-actor")
        .expect("record_supervised_patch_artifact");
    let artifact_id = artifact["artifact_id"].as_str().unwrap();

    let artifacts = store
        .supervised_patch_artifacts(100)
        .expect("supervised_patch_artifacts");
    let found_art = artifacts
        .iter()
        .any(|a| a["artifact_id"].as_str() == Some(artifact_id));
    assert!(found_art, "recorded artifact should appear in list");
}

/// PostgreSQL active trial: exercises the full auto-adjustment apply + rollback
/// cycle against a real PostgreSQL database. Seeds dispatches via record_dispatch,
/// enables active auto-adjustment gates, applies a candidate, verifies
/// snapshot/proposal/audit state, rolls back, and verifies restoration.
///
/// Gracefully skips if pattern detection produces no candidate from seeded dispatches.
#[test]
#[cfg(feature = "pg-tests")]
fn pg_auto_adjustment_apply_and_rollback_cycle() {
    let Some(store) = test_store() else { return };
    let tag = uuid_tag();

    // Seed dispatches to feed pattern detection → candidate generation.
    // 10 failing cheap_executor/code_generate dispatches.
    for i in 0..10 {
        let bundle = json!({
            "record": {
                "dispatch_id": format!("aa-cheap-{tag}-{i}"),
                "created_at": utc_now_string(),
                "final_status": "failure"
            },
            "decision": {
                "selected_tier": "cheap_executor",
                "budget_reservation": {"reserved_cost": 0.001}
            },
            "analysis": {
                "task_class": "code_generate",
                "risk_level": "low",
                "complexity_score": 0.3
            },
            "execution_result": {
                "executor_type": "noop",
                "input_tokens": 50,
                "output_tokens": 20,
                "estimated_cost": 0.0001,
                "latency_ms": 100
            }
        });
        store
            .record_dispatch("pg-aa-test", "pg-test", &bundle, "pg-test")
            .expect("record_dispatch cheap");
    }

    // 10 successful strong_planner/code_debug dispatches (high cost).
    for i in 0..10 {
        let bundle = json!({
            "record": {
                "dispatch_id": format!("aa-strong-{tag}-{i}"),
                "created_at": utc_now_string(),
                "final_status": "success"
            },
            "decision": {
                "selected_tier": "strong_planner",
                "budget_reservation": {"reserved_cost": 0.05}
            },
            "analysis": {
                "task_class": "code_debug",
                "risk_level": "medium",
                "complexity_score": 0.8
            },
            "execution_result": {
                "executor_type": "noop",
                "input_tokens": 500,
                "output_tokens": 200,
                "estimated_cost": 0.01,
                "latency_ms": 2000
            }
        });
        store
            .record_dispatch("pg-aa-test", "pg-test", &bundle, "pg-test")
            .expect("record_dispatch strong");
    }

    // Enable active auto-adjustment gates.
    std::env::set_var("ACP_ENABLE_AUTO_ADJUSTMENT", "1");
    std::env::set_var("ACP_AUTO_ADJUSTMENT_ACTIVE", "1");
    std::env::remove_var("ACP_AUTO_ADJUSTMENT_DRY_RUN");

    // Apply auto-adjustment.
    let apply_result =
        store.apply_auto_adjustment(&json!({"confirm_auto_adjustment": true}), "pg-trial-test");

    // If no candidate was generated, pattern detection didn't trigger — skip gracefully.
    let apply = match apply_result {
        Ok(v) => v,
        Err(e) if e.contains("no generated candidate") => {
            std::env::remove_var("ACP_ENABLE_AUTO_ADJUSTMENT");
            std::env::remove_var("ACP_AUTO_ADJUSTMENT_ACTIVE");
            eprintln!("skipping auto-adjustment apply/rollback: no candidate generated from {tag} dispatches");
            return;
        }
        Err(e) => panic!("apply_auto_adjustment failed: {e}"),
    };

    // Policy evaluator may block the candidate (confidence, evidence, safety flags).
    // A blocked result still exercises the PG storage path for rejection audit events.
    if apply["status"].as_str() == Some("blocked") {
        let reasons = apply["blocked_reasons"].as_str().unwrap_or("unknown");
        eprintln!("candidate blocked by policy evaluator: {reasons}");
        // Verify rejection was audited.
        let events = store
            .search_audit_events(100, 0, Some("auto_adjustment.apply.rejected"))
            .expect("search_audit_events for rejected");
        assert!(
            !events.is_empty(),
            "blocked apply should produce audit event"
        );
        std::env::remove_var("ACP_ENABLE_AUTO_ADJUSTMENT");
        std::env::remove_var("ACP_AUTO_ADJUSTMENT_ACTIVE");
        return;
    }

    // Full apply+rollback cycle: candidate was eligible.
    assert_eq!(apply["status"].as_str().unwrap(), "active");
    assert!(apply["applied"].as_bool().unwrap());
    let adjustment_id = apply["adjustment_id"].as_str().unwrap().to_string();

    // Verify snapshot persisted.
    let detail = store
        .get_auto_adjustment(&adjustment_id)
        .expect("get_auto_adjustment");
    assert!(detail.is_some(), "adjustment should exist in store");
    assert_eq!(detail.unwrap()["status"].as_str().unwrap(), "active");

    // Verify active list.
    let active = store
        .active_auto_adjustments()
        .expect("active_auto_adjustments");
    assert!(
        active
            .iter()
            .any(|a| a["adjustment_id"].as_str() == Some(&adjustment_id)),
        "adjustment should appear in active list"
    );

    // Rollback.
    let rb = store
        .rollback_auto_adjustment(
            &adjustment_id,
            &json!({"confirm_auto_adjustment_rollback": true}),
            "pg-trial-test",
        )
        .expect("rollback_auto_adjustment");
    assert_eq!(rb["status"].as_str().unwrap(), "rolled_back");
    assert!(rb["rolled_back"].as_bool().unwrap());

    // Verify rolled-back state.
    let after = store
        .get_auto_adjustment(&adjustment_id)
        .expect("get_auto_adjustment after rollback");
    assert_eq!(after.unwrap()["status"].as_str().unwrap(), "rolled_back");

    // Verify no active adjustments remain.
    let active_after = store
        .active_auto_adjustments()
        .expect("active_auto_adjustments after rollback");
    assert!(
        active_after
            .iter()
            .all(|a| a["adjustment_id"].as_str() != Some(&adjustment_id)),
        "rolled-back adjustment should not appear in active list"
    );

    // Verify audit events.
    let events = store
        .search_audit_events(100, 0, Some(&adjustment_id))
        .expect("search_audit_events");
    assert!(
        events.len() >= 2,
        "expected at least 2 audit events for apply+rollback, got {}",
        events.len()
    );

    // Clean up env vars.
    std::env::remove_var("ACP_ENABLE_AUTO_ADJUSTMENT");
    std::env::remove_var("ACP_AUTO_ADJUSTMENT_ACTIVE");
}
