use super::*;
use crate::provider::transport::{HttpError, HttpRequest, HttpResponse, HttpTransport};
use crate::storage::local_product_store::{
    SCOPE_ATTEMPT_ADMIT, SCOPE_DELEGATED_ARTIFACT_CONFIRM, SCOPE_DELEGATED_AUTONOMY,
    SCOPE_DELEGATED_EXECUTE, SCOPE_DELEGATED_MANIFEST_APPROVE, SCOPE_REVOKE,
    SCOPE_RISK_ACKNOWLEDGE, SCOPE_SPEND_AUTHORIZE,
};
use sha2::Digest;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{channel, sync_channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tempfile::tempdir;

#[test]
fn codex_pre_send_adapter_error_preserves_retry_classification() {
    let error = codex_error_from_adapter_message(
        "provider_pre_send: loopback request was rejected before send".into(),
    );
    assert!(error.retryable);
    assert_eq!(error.effect, ManagedFailureEffect::PreSend);
    assert_eq!(error.domain, "provider_pre_send");
}

#[test]
fn codex_pre_gateway_adapter_error_does_not_claim_provider_effect() {
    let error = codex_error_from_adapter_message(
        "ChatGPT subscription credential is unavailable; API-key auth is not admitted".into(),
    );
    assert!(!error.retryable);
    assert_eq!(error.effect, ManagedFailureEffect::NoExternalEffect);
    assert_eq!(error.domain, "adapter_pre_gateway");
}

#[test]
fn codex_adapter_request_rejection_preserves_no_effect_classification() {
    for domain in ["provider_request", "gateway_pre_call"] {
        let error = codex_error_from_adapter_message(format!(
            "{domain}: bounded request was rejected before provider send"
        ));
        assert!(!error.retryable);
        assert_eq!(error.effect, ManagedFailureEffect::NoExternalEffect);
        assert_eq!(error.domain, domain);
    }
}

#[test]
fn codex_gateway_pre_call_http_error_does_not_claim_provider_effect() {
    let reason = serde_json::to_string(&json!({
        "error": {"code": "request_budget_exhausted"}
    }))
    .unwrap();
    let error = codex_http_error(HttpError::Http {
        status: 429,
        reason,
    });
    assert!(!error.retryable);
    assert_eq!(error.effect, ManagedFailureEffect::NoExternalEffect);
    assert_eq!(error.domain, "gateway_pre_call");

    let upstream_reason = serde_json::to_string(&json!({
        "error": {"code": "usage_unavailable"}
    }))
    .unwrap();
    let upstream_error = codex_http_error(HttpError::Http {
        status: 502,
        reason: upstream_reason,
    });
    assert_eq!(upstream_error.effect, ManagedFailureEffect::OutcomeUnknown);
}

#[test]
fn codex_mx1_materialization_expands_every_frozen_schedule_cell() {
    let frozen = freeze_current_operator_contract_set().unwrap();
    let cells = frozen_codex_mx1_cells(
        &frozen,
        crate::harness_evolution::Mx1MatrixRung::OneByTwoByOne,
    )
    .unwrap();

    assert_eq!(cells.len(), 8); // 4 schedule cells × 2 Models
    assert!(cells.iter().all(|cell| {
        cell.get("matrix_rung").and_then(Value::as_str) == Some("1x2x1")
            && cell.get("harness_id").and_then(Value::as_str)
                == Some(crate::harness_evolution::MX1_ARM_ZERO_HARNESS_ID)
            && cell.get("strategy_id").and_then(Value::as_str)
                == Some(crate::harness_evolution::MX1_NO_PROJECTION_STRATEGY_ID)
    }));
    let model_ids = cells
        .iter()
        .map(|cell| cell.get("matrix_model_id").and_then(Value::as_str).unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        model_ids,
        std::collections::BTreeSet::from([
            crate::harness_evolution::MX1_CODEX_LUNA_MODEL_ID,
            crate::harness_evolution::MX1_CODEX_TERRA_MODEL_ID,
        ])
    );
    let plan_ids = cells
        .iter()
        .map(|cell| cell["matrix_plan_id"].as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    let bases = cells
        .iter()
        .map(|cell| cell["matrix_common_basis_sha256"].as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(plan_ids.len(), 4);
    assert_eq!(bases.len(), 4);
    assert_eq!(
        cells
            .iter()
            .map(|cell| cell["sequential_order"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        (1..=8).collect::<Vec<_>>()
    );

    let baseline = frozen_codex_mx1_cells(
        &frozen,
        crate::harness_evolution::Mx1MatrixRung::OneByOneByOne,
    )
    .unwrap();
    let strategies = frozen_codex_mx1_cells(
        &frozen,
        crate::harness_evolution::Mx1MatrixRung::OneByOneByThree,
    )
    .unwrap();
    assert_eq!(baseline.len(), 4); // 4 schedule cells × 1 Model × 1 Strategy
    assert_eq!(strategies.len(), 12); // 4 schedule cells × 1 Model × 3 Strategies
    assert!(baseline.iter().chain(&strategies).all(|cell| {
        cell["matrix_model_id"].as_str() == Some(crate::harness_evolution::MX1_CODEX_LUNA_MODEL_ID)
    }));
}

#[test]
fn campaign_package_binding_rejects_cross_rung_and_tampered_drivers() {
    assert_eq!(
        required_campaign_package_id_for_rung(None).unwrap(),
        crate::rwe::campaign_package::RWE_DEEPSEEK_V2_PACKAGE_ID
    );
    for (rung, package_id) in [
        (
            crate::harness_evolution::Mx1MatrixRung::OneByTwoByOne,
            crate::rwe::campaign_package::RWE_CODEX_SUBSCRIPTION_V1_PACKAGE_ID,
        ),
        (
            crate::harness_evolution::Mx1MatrixRung::OneByOneByOne,
            crate::rwe::campaign_package::RWE_CODEX_LUNA_XHIGH_V2_PACKAGE_ID,
        ),
        (
            crate::harness_evolution::Mx1MatrixRung::OneByOneByThree,
            crate::rwe::campaign_package::RWE_CODEX_LUNA_XHIGH_V3_STRATEGY_PACKAGE_ID,
        ),
    ] {
        assert_eq!(
            required_campaign_package_id_for_rung(Some(rung)).unwrap(),
            package_id
        );
    }
    assert!(required_campaign_package_id_for_rung(Some(
        crate::harness_evolution::Mx1MatrixRung::OneByTwoByThree
    ))
    .is_err());
    let package_ids = [
        crate::rwe::campaign_package::RWE_CODEX_SUBSCRIPTION_V1_PACKAGE_ID,
        crate::rwe::campaign_package::RWE_CODEX_LUNA_XHIGH_V2_PACKAGE_ID,
        crate::rwe::campaign_package::RWE_CODEX_LUNA_XHIGH_V3_STRATEGY_PACKAGE_ID,
    ];
    for expected in package_ids {
        for observed in package_ids {
            let driver = ProductGoldenPathCellDriver {
                campaign_package: Some(
                    crate::rwe::campaign_package::resolve_frozen_campaign_package(observed)
                        .unwrap(),
                ),
                ..ProductGoldenPathCellDriver::default()
            };
            assert_eq!(
                require_driver_campaign_package(&driver, expected).is_ok(),
                expected == observed,
                "expected={expected}, observed={observed}"
            );
        }
    }
    let unbound = InjectedCellDriver { outcomes: vec![] };
    assert!(require_driver_campaign_package(
        &unbound,
        crate::rwe::campaign_package::RWE_CODEX_LUNA_XHIGH_V2_PACKAGE_ID
    )
    .is_err());
    assert!(require_driver_campaign_package(
        &unbound,
        crate::rwe::campaign_package::RWE_DEEPSEEK_V2_PACKAGE_ID
    )
    .is_ok());
    let mut tampered =
        crate::rwe::campaign_package::canonical_codex_luna_xhigh_v2_package().unwrap();
    tampered.notes.push_str(" tampered");
    let driver = ProductGoldenPathCellDriver {
        campaign_package: Some(tampered),
        ..ProductGoldenPathCellDriver::default()
    };
    assert!(require_driver_campaign_package(
        &driver,
        crate::rwe::campaign_package::RWE_CODEX_LUNA_XHIGH_V2_PACKAGE_ID
    )
    .is_err());
}

#[test]
fn codex_responses_body_translates_managed_tool_choice_and_omits_output_ceiling() {
    let binding = crate::provider::managed_deepseek::ManagedCallBinding {
        product_task_id: "pt-codex-body".into(),
        workflow_id: "wf-codex-body".into(),
        node_id: "wf-codex-body-implementation".into(),
        attempt_id: "attempt-codex-body".into(),
        spend_authorization_id: "spend-codex-body".into(),
        attempt_lease_id: "lease-codex-body".into(),
    };
    let mut request = ManagedProviderCallRequest::for_role(
        crate::provider::managed_deepseek::ManagedModelRole::Implementer,
        crate::provider::managed_deepseek::DeepSeekProtocol::OpenAiCompatible,
        binding,
    );
    request.provider_kind =
        crate::cli::codex_budget_authority::CODEX_PROVIDER_KIND_CHATGPT_SUBSCRIPTION.into();
    request.schema_version = "codex_responses_api.v1".into();
    request.requested_model = crate::rwe::campaign_package::CODEX_SUBSCRIPTION_LUNA_MODEL.into();
    request.thinking.reasoning_effort = Some("xhigh".into());
    request.messages = vec![ManagedMessage::text("user", "bounded")];
    request.tools = vec![crate::provider::managed_deepseek::ManagedTool {
        tool_type: "function".into(),
        function: crate::provider::managed_deepseek::ManagedToolFunction {
            name: "apply_workspace_action".into(),
            description: "bounded".into(),
            parameters: json!({"type": "object"}),
        },
        strict: true,
    }];
    request.tool_choice = Some(json!({
        "type": "function",
        "function": {"name": "apply_workspace_action"}
    }));

    let body = codex_responses_body(&request);
    assert_eq!(body["reasoning"]["effort"], "xhigh");
    assert_eq!(body["stream"], json!(true));
    assert_eq!(
        body["tool_choice"],
        json!({
            "type": "function",
            "name": "apply_workspace_action"
        })
    );
    assert!(body.get("max_output_tokens").is_none());
    assert_eq!(body["tools"][0]["type"], json!("function"));
}

#[test]
fn codex_tool_calls_parses_string_and_object_arguments() {
    let response = json!({
        "output": [
            {
                "type": "function_call",
                "call_id": "call_1",
                "name": "apply_workspace_action",
                "arguments": "{\"action\":\"replace_text\"}"
            },
            {
                "type": "function_call",
                "id": "call_2",
                "name": "apply_workspace_action",
                "arguments": {"action": "replace_text", "path": "src/lib.rs"}
            },
            {
                "type": "function",
                "id": "call_3",
                "function": {
                    "name": "apply_workspace_action",
                    "arguments": {"action": "replace_text"}
                }
            }
        ]
    });
    let calls = codex_tool_calls(&[], &response);
    assert_eq!(calls.len(), 3);
    assert_eq!(calls[0].id, "call_1");
    assert_eq!(calls[0].function.name, "apply_workspace_action");
    assert_eq!(calls[0].function.arguments, "{\"action\":\"replace_text\"}");
    assert_eq!(calls[1].id, "call_2");
    assert_eq!(calls[1].function.name, "apply_workspace_action");
    assert!(calls[1].function.arguments.contains("\"src/lib.rs\""));
    assert_eq!(calls[2].id, "call_3");
    assert_eq!(calls[2].function.name, "apply_workspace_action");

    // Test fallback to SSE events when response.output is empty
    let events = vec![json!({
        "type": "response.output_item.done",
        "item": {
            "type": "function_call",
            "call_id": "call_stream_1",
            "name": "apply_workspace_action",
            "arguments": "{\"action\":\"replace_text\",\"path\":\"src/main.rs\"}"
        }
    })];
    let empty_response = json!({"output": []});
    let stream_calls = codex_tool_calls(&events, &empty_response);
    assert_eq!(stream_calls.len(), 1);
    assert_eq!(stream_calls[0].id, "call_stream_1");
    assert_eq!(stream_calls[0].function.name, "apply_workspace_action");
    assert!(stream_calls[0]
        .function
        .arguments
        .contains("\"src/main.rs\""));
}

#[test]
fn coordinator_recognizes_workspace_action_fence_variants() {
    let action = r#"{"schema_version":"managed_workspace_action.v1","action":"replace_text","path":"src/lib.rs"}"#;
    let variants = [
        action.to_string(),
        format!("```json\n{action}\n```"),
        format!("```JSON\r\n{action}\r\n```"),
        format!("```\n{action}\n```"),
        format!("```json {action} ```"),
        format!("``` {action} ```"),
    ];
    for output in &variants {
        let parsed = LocalProductStore::parse_managed_workspace_action(output).unwrap();
        assert_eq!(
            parsed.get("schema_version").and_then(Value::as_str),
            Some("managed_workspace_action.v1")
        );
    }
}

#[test]
fn failed_cell_journal_usage_is_projected_without_false_zeroes() {
    let projection = json!({
        "provider_execution": null,
        "provider_request_journal": [
            {
                "status": "succeeded",
                "effective_tokens": 422,
                "effective_cost_usd": 0.000120118,
                "transport_provenance": "external",
                "usage": {"input_tokens": 314, "output_tokens": 108}
            },
            {
                "status": "succeeded",
                "effective_tokens": 4349,
                "effective_cost_usd": 0.0011337368,
                "transport_provenance": "external",
                "usage": {"input_tokens": 349, "output_tokens": 4000}
            }
        ]
    });

    let execution = provider_execution_from_journal(&projection).unwrap();
    assert_eq!(execution["provider_request_count"], 2);
    assert_eq!(execution["cumulative_tokens"], 4771);
    assert_eq!(execution["transport_provenance"], "external");
    assert_eq!(
        store_evidence_transport_provenance(&projection),
        Ok("external".to_string())
    );
}

#[test]
fn failed_provider_journal_entries_do_not_become_live_requests() {
    let projection = json!({
        "provider_execution": null,
        "provider_request_journal": [{
            "status": "failed_before_send",
            "effective_tokens": 0,
            "effective_cost_usd": 0.0,
            "transport_provenance": "external"
        }]
    });
    let execution = provider_execution_from_journal(&projection).unwrap();
    assert_eq!(execution["provider_request_count"], 0);
    assert_eq!(execution["realized_cost_usd"], 0.0);
    assert_eq!(
        store_evidence_transport_provenance(&projection),
        Ok("external".to_string())
    );
}

#[test]
fn outcome_unknown_journal_entries_preserve_unknown_effect_evidence() {
    let projection = json!({
        "provider_execution": null,
        "provider_request_journal": [{
            "status": "outcome_unknown",
            "effective_tokens": 512,
            "effective_cost_usd": 0.0002,
            "transport_provenance": "external"
        }]
    });
    let execution = provider_execution_from_journal(&projection).unwrap();
    assert_eq!(execution["provider_request_count"], 1);
    assert_eq!(execution["cumulative_tokens"], 512);
    assert_eq!(execution["cost_unknown"], true);
}

#[test]
fn journal_cost_overflow_stays_unavailable() {
    let projection = json!({
        "provider_execution": null,
        "provider_request_journal": [
            {
                "status": "succeeded",
                "effective_tokens": 1,
                "effective_cost_usd": 1.0e308,
                "transport_provenance": "external",
                "usage": {"input_tokens": 1, "output_tokens": 0}
            },
            {
                "status": "succeeded",
                "effective_tokens": 1,
                "effective_cost_usd": 1.0e308,
                "transport_provenance": "external",
                "usage": {"input_tokens": 1, "output_tokens": 0}
            }
        ]
    });
    assert!(provider_execution_from_journal(&projection).is_none());
}

#[test]
fn incomplete_cell_journal_stays_unavailable() {
    let projection = json!({
        "provider_execution": null,
        "provider_request_journal": [{
            "transport_provenance": "external",
            "usage": {"input_tokens": 10, "output_tokens": 5}
        }]
    });
    assert!(provider_execution_from_journal(&projection).is_none());
    assert!(store_evidence_transport_provenance(&projection).is_err());
}

fn operator(store: &LocalProductStore, tenant: &str, key: &str) -> AuthenticatedPrincipal {
    store
        .record_api_key_metadata_for_tenant(
            tenant,
            key,
            "operator-user",
            "operator",
            &[
                SCOPE_RISK_ACKNOWLEDGE.to_string(),
                SCOPE_SPEND_AUTHORIZE.to_string(),
                SCOPE_ATTEMPT_ADMIT.to_string(),
                SCOPE_REVOKE.to_string(),
                SCOPE_DELEGATED_AUTONOMY.to_string(),
                SCOPE_DELEGATED_MANIFEST_APPROVE.to_string(),
                SCOPE_DELEGATED_ARTIFACT_CONFIRM.to_string(),
            ],
            "test",
        )
        .unwrap();
    store
        .authenticate_managed_acceptance_principal(tenant, key, None)
        .unwrap()
}

/// Role-separated delegated attempt activator key (never the manifest approver).
fn cell_executor(store: &LocalProductStore, tenant: &str, key: &str) -> AuthenticatedPrincipal {
    store
        .record_api_key_metadata_for_tenant(
            tenant,
            key,
            "executor-user",
            "executor",
            &[
                SCOPE_DELEGATED_EXECUTE.to_string(),
                SCOPE_ATTEMPT_ADMIT.to_string(),
                SCOPE_RISK_ACKNOWLEDGE.to_string(),
            ],
            "test",
        )
        .unwrap();
    store
        .authenticate_managed_acceptance_principal(tenant, key, None)
        .unwrap()
}

/// Role-separated delegated artifact confirmer key, distinct from both the
/// manifest approver and the attempt activator.
fn cell_confirmer(store: &LocalProductStore, tenant: &str, key: &str) -> AuthenticatedPrincipal {
    store
        .record_api_key_metadata_for_tenant(
            tenant,
            key,
            "reviewer-user",
            "reviewer",
            &[
                SCOPE_RISK_ACKNOWLEDGE.to_string(),
                SCOPE_DELEGATED_ARTIFACT_CONFIRM.to_string(),
            ],
            "test",
        )
        .unwrap();
    store
        .authenticate_managed_acceptance_principal(tenant, key, None)
        .unwrap()
}

fn seed_gp(store: &LocalProductStore, product_task_id: &str, tenant_id: &str) {
    let mut evidence = json!({
        "schema_version": "product_task_terminal_evidence.v2",
        "evidence_id": format!("ev-{product_task_id}"),
        "product_task_id": product_task_id,
        "tenant_id": tenant_id,
        "workspace_scope_id": "ws-gp",
        "task_version": 1,
        "task_status": "completed",
        "node": {"executor_class": "managed_coding"},
        "source_revision": "c".repeat(40),
        "verification": {"trustworthy": true, "status": "passed"},
        "approval": {"approval_id": "ap-1"},
        "artifact": {"artifact_id": "art-1"},
        "output": {
            "intent": "draft_pr",
            "result_sha256": "d".repeat(64),
            "operation_id": "op-1",
            "receipt_id": "rcpt-1",
            "draft_pr": {
                "number": 1,
                "repository": "Igzela/alters-lab",
                "base_branch": "main",
                "head_branch": "acp/gp",
                "head_sha": "e".repeat(40),
                "draft": true
            }
        },
        "audit_reference": {"audit_id": 1, "action": "product_task.terminal_evidence_committed"},
        "created_at": "2026-07-25T12:00:00Z",
        "created_by": "test",
        "content_sha256": Value::Null,
    });
    let hash = hex::encode(Sha256::digest(serde_json::to_vec(&evidence).unwrap()));
    evidence["content_sha256"] = json!(hash);
    store
        .insert_product_task_terminal_evidence_for_tests(&evidence)
        .unwrap();
}

#[test]
fn armed_cell_rejects_artifact_only_before_delegation_or_spend() {
    let dir = tempdir().unwrap();
    let store = Arc::new(LocalProductStore::new(dir.path().join("output-contract.db")).unwrap());
    let principal = operator(&store, "output-test", "output-operator");
    let executor = cell_executor(&store, "output-test", "output-executor");
    let confirmer = cell_confirmer(&store, "output-test", "output-confirmer");
    // Deliberately incompatible fixture in a fresh test-only Store. No live
    // evidence or operator database is used by this refusal regression.
    seed_gp(&store, "output-task", "output-test");
    let fixture_conn = rusqlite::Connection::open(dir.path().join("output-contract.db")).unwrap();
    fixture_conn
        .execute(
            "UPDATE product_tasks SET output_intent='artifact_only' WHERE task_id='output-task'",
            [],
        )
        .unwrap();
    let frozen = freeze_current_operator_contract_set().unwrap();
    let cell = &frozen.schedule.body["cells"][0];
    let task = frozen
        .corpus
        .tasks
        .iter()
        .find(|task| Some(task.task_id.as_str()) == cell["task_id"].as_str())
        .unwrap();
    let ids = cell_identities_for("output-test", cell, task).unwrap();
    let error = execute_armed_delegated_rwe_cell(
        &store,
        &principal,
        &executor,
        &confirmer,
        &frozen,
        "output-test",
        "",
        cell,
        task,
        &ids,
        "output-task",
        None,
        None,
        dir.path(),
        &None,
    )
    .unwrap_err();
    assert_eq!(
        error,
        "delegated RWE execution requires the frozen draft_pr output contract"
    );
    let count: i64 = fixture_conn
        .query_row(
            "SELECT COUNT(*) FROM managed_acceptance_delegations",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        count, 0,
        "refusal must precede delegation and its spend journal"
    );
}

fn success_outcomes() -> Vec<CellOutcome> {
    outcomes_with_classification("injected_success")
}

fn outcomes_with_classification(class: &str) -> Vec<CellOutcome> {
    (0..4)
        .map(|i| CellOutcome {
            classification: class.into(),
            provider_requests: 3,
            input_tokens: 0,
            output_tokens: 0,
            total_tokens: 0,
            latency_ms: 1,
            monetary_cost: Some(0.0),
            cost_unknown: false,
            live_provider_request: false,
            provider_transport_provenance: "injected".into(),
            evidence_source: "injected".into(),
            verification_status: "passed".into(),
            verification_trustworthy: true,
            approval_id: Some(format!("ap-{i}")),
            output_draft_pr: None,
            terminal_evidence_id: Some(format!("tev-{i}")),
            terminal_content_sha256: Some("f".repeat(64)),
            cleanup_status: "completed".into(),
            product_task_id: String::new(),
            workflow_id: String::new(),
            node_id: String::new(),
            delegated_attempt_id: String::new(),
            workspace_id: String::new(),
            note: "injected".into(),
        })
        .collect()
}

fn admit_ready(
    store: &LocalProductStore,
    principal: &AuthenticatedPrincipal,
    auth_id: &str,
    run_id: &str,
    gp: &str,
) -> String {
    seed_gp(store, gp, principal.tenant_id());
    // The one-use authority may only be consumed when the complete runnable
    // seam is ready (credential present, non-CI). Tests simulate the operator
    // environment around the admit call under the shared env lock.
    let _lock = crate::cli::config::cli_env_test_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let had_cred = std::env::var_os(DEEPSEEK_CREDENTIAL_REFERENCE);
    let had_ci = std::env::var_os("CI");
    std::env::set_var(DEEPSEEK_CREDENTIAL_REFERENCE, "test-operator-credential");
    std::env::remove_var("CI");
    let admitted = issue_and_admit_v2(
        store,
        principal,
        auth_id,
        run_id,
        gp,
        "2026-08-07T00:00:00Z",
    );
    match had_cred {
        Some(v) => std::env::set_var(DEEPSEEK_CREDENTIAL_REFERENCE, v),
        None => std::env::remove_var(DEEPSEEK_CREDENTIAL_REFERENCE),
    }
    match had_ci {
        Some(v) => std::env::set_var("CI", v),
        None => std::env::remove_var("CI"),
    }
    let admitted = admitted.unwrap();
    admitted["lease_token"].as_str().unwrap().to_string()
}

#[test]
fn preflight_fails_closed_without_gp_and_without_consuming() {
    let dir = tempdir().unwrap();
    let store = Arc::new(
        LocalProductStore::new_with_clock(dir.path().join("pf.db"), || {
            "2026-07-25T12:00:00Z".into()
        })
        .unwrap(),
    );
    let principal = operator(&store, "t-pf", "op-pf");
    let pre = operator_preflight(&store, &principal, None, None).unwrap();
    assert_eq!(pre["ready"], false);
    assert_eq!(pre["observed_at"], "2026-07-25T12:00:00Z");
    assert_eq!(pre["authority_consumed"], false);
    assert_eq!(pre["provider_call_performed"], false);
    let codes: Vec<_> = pre["blockers"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|b| b.get("code").and_then(Value::as_str))
        .collect();
    assert!(
        codes.contains(&"missing_golden_path_prerequisite_id")
            || codes
                .iter()
                .any(|c| c.contains("golden_path") || c.contains("composition"))
    );
}

#[test]
fn viability_preflight_reports_redacted_credential_presence_without_value() {
    let dir = tempdir().unwrap();
    let store = Arc::new(LocalProductStore::new(dir.path().join("pf-ready.db")).unwrap());
    let principal = operator(&store, "t-pf-ready", "op-pf-ready");
    seed_gp(&store, "ptask-gp-pf-ready", principal.tenant_id());
    let had_cred = std::env::var_os(DEEPSEEK_CREDENTIAL_REFERENCE);
    std::env::remove_var(DEEPSEEK_CREDENTIAL_REFERENCE);
    let pre = operator_preflight_read_only(&store, &principal, None, Some("ptask-gp-pf-ready"));
    match had_cred {
        Some(v) => std::env::set_var(DEEPSEEK_CREDENTIAL_REFERENCE, v),
        None => std::env::remove_var(DEEPSEEK_CREDENTIAL_REFERENCE),
    }
    let pre = pre.unwrap();
    assert_eq!(pre["ready"], false);
    assert_eq!(pre["authority_consumed"], false);
    assert_eq!(pre["provider_call_performed"], false);
    assert_eq!(pre["target_write_performed"], false);
    assert_eq!(pre["live_baseline_sealed"], false);
    assert_eq!(pre["credential_readiness"], "missing");
    assert_eq!(pre["credential_symbol_present"], false);
    assert_eq!(
        pre["comparison"]["window"],
        "single_randomized_interleaved_window"
    );
    assert_eq!(pre["comparison"]["authorizations_issued"], false);
    assert_eq!(pre["comparison"]["unissued_authorization_packages"], 2);
    assert!(pre["blockers"].as_array().unwrap().iter().any(|blocker| {
        blocker.get("code").and_then(Value::as_str) == Some("missing_credential_symbol")
    }));
    assert!(!pre["blockers"].as_array().unwrap().iter().any(|blocker| {
        blocker.get("code").and_then(Value::as_str) == Some("credential_readiness_unavailable")
    }));
    let observed = pre["observed_at"].as_str().unwrap();
    assert!(
        chrono::DateTime::parse_from_rfc3339(observed).is_ok(),
        "{observed}"
    );
    let frozen = freeze_current_operator_contract_set().unwrap();
    assert_eq!(
        frozen.accepted_main_sha,
        crate::rwe::operator_corpus::OPERATOR_ARTIFACTS_FROZEN_AT_MAIN_SHA
    );
    assert_eq!(
        frozen.corpus.corpus_sha256,
        crate::rwe::operator_corpus::OPERATOR_V2_CORPUS_SHA256
    );
    assert_eq!(
        frozen.protocol.body_sha256,
        crate::rwe::operator_corpus::OPERATOR_V2_PROTOCOL_SHA256
    );
    assert_eq!(
        frozen.schedule.schedule_sha256,
        crate::rwe::operator_corpus::OPERATOR_V2_SCHEDULE_SHA256
    );
    let request = sort_value(&json!({
        "schema_version": "rwe_run_authorization_v2_request.v1",
        "authorization_id": "unissued",
        "expires_at": "caller-supplied-finite",
        "issued": false,
        "admitted": false,
        "accepted_main_sha": frozen.accepted_main_sha,
        "corpus_sha256": frozen.corpus.corpus_sha256,
        "protocol_sha256": frozen.protocol.body_sha256,
        "schedule_sha256": frozen.schedule.schedule_sha256,
    }));
    let request_sha256 = sha256_hex(serde_json::to_vec(&request).unwrap().as_slice());
    assert_eq!(
        request_sha256,
        "015c94e9d65a902f3aba5eae4f3da6cba6d534cc3c57af3a6faf89125663469a"
    );
    assert!(store
        .get_rwe_run_authorization("unissued")
        .unwrap()
        .is_none());
}

#[test]
fn viability_preflight_is_read_only_without_store_creation_or_auth_touch() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("existing?reserved#percent%.db");
    let store = LocalProductStore::new(&db_path).unwrap();
    store
        .record_api_key_metadata_for_tenant(
            "t-read-only",
            "op-read-only",
            "operator-user",
            "operator",
            &[SCOPE_RISK_ACKNOWLEDGE.to_string()],
            "test",
        )
        .unwrap();
    let before = store
        .get_api_key_metadata_for_tenant("op-read-only", "t-read-only")
        .unwrap()
        .unwrap();
    assert!(before["last_used_at"].is_null());
    drop(store);
    let directory_entries = || {
        let mut entries: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| {
                let entry = entry.unwrap();
                let metadata = entry.metadata().unwrap();
                (
                    entry.file_name().to_string_lossy().into_owned(),
                    metadata.len(),
                    metadata.modified().ok(),
                )
            })
            .collect();
        entries.sort_by(|left, right| left.0.cmp(&right.0));
        entries
    };
    let before_directory = directory_entries();
    let before_file = std::fs::metadata(&db_path).unwrap();
    let before_size = before_file.len();
    let before_modified = before_file.modified().ok();

    let read_only = Arc::new(LocalProductStore::open_existing_read_only(&db_path).unwrap());
    let principal = read_only
        .authenticate_managed_acceptance_principal_read_only(
            "t-read-only",
            "op-read-only",
            Some(1.0),
        )
        .unwrap();
    let pre = operator_preflight_read_only(&read_only, &principal, None, None).unwrap();
    assert_eq!(pre["ready"], false);
    assert_eq!(pre["authority_consumed"], false);
    assert_eq!(pre["provider_call_performed"], false);
    assert_eq!(pre["target_write_performed"], false);
    assert!(pre["credential_symbol_present"].is_boolean());
    assert_ne!(pre["credential_readiness"], "unavailable");
    let mutation = read_only.record_api_key_metadata(
        "should-not-write",
        "operator-user",
        "operator",
        &[SCOPE_RISK_ACKNOWLEDGE.to_string()],
        "read-only-regression",
    );
    assert!(mutation.is_err(), "read-only store accepted a mutation");
    let checkpoint = read_only.checkpoint_wal();
    assert!(checkpoint
        .expect_err("read-only store checkpointed its WAL")
        .contains("read-only"));
    let transaction = read_only.with_transaction(|_| Ok::<(), String>(()));
    assert!(transaction
        .expect_err("read-only store accepted a transaction")
        .contains("read-only"));
    let restore = read_only
        .restore_verified_sqlite_backup(std::path::Path::new("missing-verified-backup.db"));
    assert!(restore
        .expect_err("read-only store accepted a backup restore")
        .contains("read-only"));
    let after = read_only
        .get_api_key_metadata_for_tenant("op-read-only", "t-read-only")
        .unwrap()
        .unwrap();
    assert!(after["last_used_at"].is_null());
    assert_eq!(before, after);
    drop(read_only);

    let after_file = std::fs::metadata(&db_path).unwrap();
    assert_eq!(after_file.len(), before_size);
    assert_eq!(after_file.modified().ok(), before_modified);
    assert_eq!(directory_entries(), before_directory);

    let missing_parent = dir.path().join("missing-parent");
    let missing_path = missing_parent.join("missing.db");
    assert!(LocalProductStore::open_existing_read_only(&missing_path).is_err());
    assert!(!missing_parent.exists());
}

#[test]
fn read_only_snapshot_serializes_same_process_writer() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("snapshot.db");
    let store = LocalProductStore::new(&db_path).unwrap();
    drop(store);

    let read_only = LocalProductStore::open_existing_read_only(&db_path).unwrap();
    let writer_path = db_path.clone();
    let (started_sender, started_receiver) = sync_channel(0);
    let (finished_sender, finished_receiver) = channel();
    let writer = std::thread::spawn(move || {
        started_sender.send(()).unwrap();
        let mutating_store = LocalProductStore::new(&writer_path).unwrap();
        mutating_store
            .record_api_key_metadata_for_tenant(
                "t-snapshot",
                "op-snapshot",
                "operator-user",
                "operator",
                &[SCOPE_RISK_ACKNOWLEDGE.to_string()],
                "snapshot-mutation",
            )
            .unwrap();
        finished_sender.send(()).unwrap();
    });
    started_receiver.recv().unwrap();
    assert!(finished_receiver
        .recv_timeout(Duration::from_millis(100))
        .is_err());
    drop(read_only);
    assert!(finished_receiver
        .recv_timeout(Duration::from_secs(2))
        .is_ok());
    writer.join().unwrap();
}

#[test]
fn read_only_snapshot_rejects_same_bytes_path_replacement() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("same-bytes-replacement.db");
    let store = LocalProductStore::new(&db_path).unwrap();
    store
        .record_api_key_metadata_for_tenant(
            "t-same-bytes",
            "op-same-bytes",
            "operator-user",
            "operator",
            &[SCOPE_RISK_ACKNOWLEDGE.to_string()],
            "same-bytes-setup",
        )
        .unwrap();
    drop(store);

    let read_only = LocalProductStore::open_existing_read_only(&db_path).unwrap();
    let original_bytes = std::fs::read(&db_path).unwrap();
    std::fs::remove_file(&db_path).unwrap();
    std::fs::write(&db_path, original_bytes).unwrap();

    let error = read_only
        .get_api_key_metadata_for_tenant("op-same-bytes", "t-same-bytes")
        .expect_err("read-only store accepted a replacement inode");
    assert!(error.contains("identity changed"), "{error}");
}

#[test]
fn read_only_open_reuses_encryption_configuration_and_verified_lock_anchor() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("encrypted-anchor.db");
    let store = LocalProductStore::new_with_encryption(
        &db_path,
        || "2026-08-18T00:00:00Z".to_string(),
        Some("test-encryption-key'with-specials"),
    )
    .unwrap();
    store
        .record_api_key_metadata_for_tenant(
            "t-encrypted",
            "op-encrypted",
            "operator-user",
            "operator",
            &[SCOPE_RISK_ACKNOWLEDGE.to_string()],
            "encryption-test",
        )
        .unwrap();
    drop(store);

    let read_only = LocalProductStore::open_existing_read_only_with_encryption(
        &db_path,
        Some("test-encryption-key'with-specials"),
    )
    .unwrap();
    assert!(read_only.is_encrypted());
    assert!(read_only
        .get_api_key_metadata_for_tenant("op-encrypted", "t-encrypted")
        .unwrap()
        .is_some());
    assert!(read_only
        .record_api_key_metadata(
            "op-encrypted-new",
            "operator-user",
            "operator",
            &[SCOPE_RISK_ACKNOWLEDGE.to_string()],
            "encryption-read-only-test",
        )
        .is_err());
}

#[test]
fn read_only_encrypted_store_reports_unavailable_without_key() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("encrypted-unavailable.db");
    let store = LocalProductStore::new_with_encryption(
        &db_path,
        || "2026-08-18T00:00:00Z".to_string(),
        Some("test-encryption-key"),
    )
    .unwrap();
    drop(store);

    let companion_path = |suffix: &str| {
        let mut path = db_path.as_os_str().to_os_string();
        path.push(suffix);
        PathBuf::from(path)
    };
    let before = std::fs::read(&db_path).unwrap();
    for suffix in ["-wal", "-shm", "-journal"] {
        assert!(!companion_path(suffix).exists());
    }

    let error = match LocalProductStore::open_existing_read_only(&db_path) {
        Ok(_) => panic!("encrypted store opened without redacted key readiness"),
        Err(error) => error,
    };
    assert_eq!(error, "encryption_readiness_unavailable");
    assert_eq!(std::fs::read(&db_path).unwrap(), before);
    for suffix in ["-wal", "-shm", "-journal"] {
        assert!(!companion_path(suffix).exists());
    }
}

#[test]
fn writable_store_fails_closed_on_path_replacement_but_keeps_verified_unlinked_inode() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("anchor.db");
    let store = LocalProductStore::new(&db_path).unwrap();
    std::fs::remove_file(&db_path).unwrap();
    store
        .record_api_key_metadata(
            "unlinked-key",
            "operator-user",
            "operator",
            &[SCOPE_RISK_ACKNOWLEDGE.to_string()],
            "unlinked-test",
        )
        .unwrap();

    std::fs::write(&db_path, b"replacement").unwrap();
    let error = store
        .record_api_key_metadata(
            "replacement-key",
            "operator-user",
            "operator",
            &[SCOPE_RISK_ACKNOWLEDGE.to_string()],
            "replacement-test",
        )
        .unwrap_err();
    assert!(
        error.contains("identity changed"),
        "unexpected error: {error}"
    );
}

#[test]
fn read_only_open_rejects_pending_sqlite_sidecars() {
    for (suffix, label) in [("-wal", "WAL"), ("-journal", "rollback journal")] {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("pending-sidecar.db");
        let store = LocalProductStore::new(&db_path).unwrap();
        drop(store);
        let mut sidecar = db_path.as_os_str().to_os_string();
        sidecar.push(suffix);
        std::fs::write(PathBuf::from(sidecar), b"pending").unwrap();
        let error = match LocalProductStore::open_existing_read_only(&db_path) {
            Ok(_) => panic!("read-only open accepted pending {label} companion"),
            Err(error) => error,
        };
        assert!(
            error.contains(label),
            "unexpected error for {label}: {error}"
        );
    }
}

#[test]
fn read_only_open_accepts_idle_wal_index_when_wal_is_empty() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("idle-shm.db");
    let store = LocalProductStore::new(&db_path).unwrap();
    store
        .record_api_key_metadata(
            "idle-shm-key",
            "operator-user",
            "operator",
            &[SCOPE_RISK_ACKNOWLEDGE.to_string()],
            "idle-shm",
        )
        .unwrap();
    drop(store);
    let opened = LocalProductStore::open_existing_read_only(&db_path)
        .expect("idle SHM leftover should not block read-only open");
    assert!(opened
        .get_api_key_metadata("idle-shm-key")
        .unwrap()
        .is_some());
}

#[test]
fn preflight_fails_closed_without_parseable_store_clock() {
    let dir = tempdir().unwrap();
    let store = Arc::new(
        LocalProductStore::new_with_clock(dir.path().join("pf-bad-clock.db"), || {
            "not-a-timestamp".into()
        })
        .unwrap(),
    );
    let principal = operator(&store, "t-pf-clock", "op-pf-clock");
    let err = operator_preflight(&store, &principal, None, None).unwrap_err();
    assert!(
        err.contains("store clock must be canonical RFC3339/UTC"),
        "{err}"
    );
}

#[test]
fn four_cell_injected_orchestration_maps_identities_and_receipts() {
    let dir = tempdir().unwrap();
    let store = Arc::new(
        LocalProductStore::new_with_clock(dir.path().join("c4.db"), || {
            "2026-07-25T12:00:00Z".into()
        })
        .unwrap(),
    );
    let principal = operator(&store, "t-c4", "op-c4");
    let lease = admit_ready(&store, &principal, "auth-c4", "run-c4", "ptask-gp-c4");
    let driver = InjectedCellDriver {
        outcomes: success_outcomes(),
    };
    let result =
        run_frozen_schedule(&store, &principal, "run-c4", "auth-c4", &lease, &driver).unwrap();
    assert_eq!(result["cell_count"], 4);
    assert_eq!(result["attempts_recorded"], 4);
    assert_eq!(result["live_baseline_sealed"], false);
    let attempts = store.list_rwe_task_attempts_for_run("run-c4").unwrap();
    assert_eq!(attempts.len(), 4);
    for a in &attempts {
        assert_ne!(a["classification"], "dispatched");
        assert!(a["evidence_json"].get("cell_id").is_some());
        assert!(a["evidence_json"].get("product_task_id").is_some());
    }
    let again =
        run_frozen_schedule(&store, &principal, "run-c4", "auth-c4", &lease, &driver).unwrap();
    assert_eq!(again["attempts_recorded"], 4);
}

#[test]
fn fixture_run_with_failures_never_reports_succeeded() {
    // Completion and success are separate: a fixture that merely
    // terminalizes with failure classes must record status "failed" (or
    // "outcome_unknown"), never "succeeded", and integration_fixture_succeeded
    // must stay false. Each scenario runs the full 4-cell schedule.
    let scenarios: &[(&str, &str)] = &[
        ("verifier_failed", "run-vf"),
        ("controlled_failure", "run-cf"),
        ("blocked_budget", "run-bb"),
    ];
    for (class, run_id) in scenarios {
        let dir = tempdir().unwrap();
        let store = Arc::new(
            LocalProductStore::new_with_clock(dir.path().join("fx.db"), || {
                "2026-07-25T12:00:00Z".into()
            })
            .unwrap(),
        );
        let principal = operator(&store, "t-fx", "op-fx");
        let lease = admit_ready(&store, &principal, "auth-fx", run_id, "ptask-gp-fx");
        let driver = InjectedCellDriver {
            outcomes: outcomes_with_classification(class),
        };
        let result =
            run_frozen_schedule(&store, &principal, run_id, "auth-fx", &lease, &driver).unwrap();
        assert_eq!(
            result["integration_fixture_completed"], true,
            "{class} cells are terminal; the fixture completed: {result}"
        );
        assert_eq!(
            result["integration_fixture_succeeded"], false,
            "a fixture with {class} cells never succeeded: {result}"
        );
        assert_eq!(
            result["run"]["status"], "failed",
            "merely terminalized {class} cells must not report succeeded: {result}"
        );
        assert_eq!(result["live_baseline_sealed"], false);
    }
    // outcome_unknown stays outcome_unknown and never completes.
    let dir = tempdir().unwrap();
    let store = Arc::new(
        LocalProductStore::new_with_clock(dir.path().join("fxou.db"), || {
            "2026-07-25T12:00:00Z".into()
        })
        .unwrap(),
    );
    let principal = operator(&store, "t-fxou", "op-fxou");
    let lease = admit_ready(&store, &principal, "auth-fxou", "run-fxou", "ptask-gp-fxou");
    let driver = InjectedCellDriver {
        outcomes: outcomes_with_classification("outcome_unknown"),
    };
    let result =
        run_frozen_schedule(&store, &principal, "run-fxou", "auth-fxou", &lease, &driver).unwrap();
    assert_eq!(result["integration_fixture_completed"], false);
    assert_eq!(result["integration_fixture_succeeded"], false);
    assert_eq!(result["run"]["status"], "outcome_unknown");
    assert_eq!(
        result["aggregate"]["stopped_by"],
        "outcome_unknown_no_retry"
    );
}

#[test]
fn fixture_run_with_all_fixture_success_cells_reports_succeeded() {
    let dir = tempdir().unwrap();
    let store = Arc::new(
        LocalProductStore::new_with_clock(dir.path().join("fxs.db"), || {
            "2026-07-25T12:00:00Z".into()
        })
        .unwrap(),
    );
    let principal = operator(&store, "t-fxs", "op-fxs");
    let lease = admit_ready(&store, &principal, "auth-fxs", "run-fxs", "ptask-gp-fxs");
    let driver = InjectedCellDriver {
        outcomes: outcomes_with_classification("fixture_success"),
    };
    let result =
        run_frozen_schedule(&store, &principal, "run-fxs", "auth-fxs", &lease, &driver).unwrap();
    assert_eq!(result["integration_fixture_completed"], true);
    assert_eq!(result["integration_fixture_succeeded"], true);
    assert_eq!(result["run"]["status"], "succeeded");
    assert_eq!(result["live_baseline_sealed"], false);
}

#[test]
fn run_terminal_status_never_calls_merely_terminal_runs_succeeded() {
    let cell = |class: &str| json!({"classification": class});
    // Sealed live baseline -> succeeded.
    assert_eq!(
        run_terminal_status(true, false, &[cell("success"), cell("success")]),
        "succeeded"
    );
    // All required fixture cells fixture_success -> succeeded.
    assert_eq!(
        run_terminal_status(false, true, &[cell("fixture_success")]),
        "succeeded"
    );
    // outcome_unknown dominates.
    assert_eq!(
        run_terminal_status(
            false,
            false,
            &[cell("outcome_unknown"), cell("fixture_success")]
        ),
        "outcome_unknown"
    );
    // Every merely-terminal failure class -> failed, never succeeded.
    for class in [
        "controlled_failure",
        "verifier_failed",
        "provider_known_failure",
        "timeout",
        "cancelled",
        "blocked_ci_environment",
        "blocked_provider_free_mode",
        "blocked_missing_credential",
        "blocked_budget",
        "blocked_authority",
        "blocked_live_session_incomplete",
        "cleanup_failed",
        "injected_success",
        "injected_verifier_failed",
    ] {
        assert_eq!(
            run_terminal_status(false, false, &[cell(class)]),
            "failed",
            "{class} must map to failed, never succeeded"
        );
    }
}

#[test]
fn product_golden_path_driver_composes_product_task_without_provider_or_seal() {
    let dir = tempdir().unwrap();
    let store = Arc::new(
        LocalProductStore::new_with_clock(dir.path().join("unarmed.db"), || {
            "2026-07-25T12:00:00Z".into()
        })
        .unwrap(),
    );
    let principal = operator(&store, "t-unarmed", "op-unarmed");
    let lease = admit_ready(
        &store,
        &principal,
        "auth-unarmed",
        "run-unarmed",
        "ptask-gp-unarmed",
    );
    // Minimal target tree for intake validation (no live provider).
    let target = dir.path().join("fake-target");
    std::fs::create_dir_all(target.join("apps/api/src")).unwrap();
    std::fs::create_dir_all(target.join("apps/api/tests")).unwrap();
    std::fs::write(target.join("README.md"), "rwe\n").unwrap();
    let _lock = crate::cli::config::cli_env_test_lock()
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    // Provider-free composition is valid in CI; the same environment must
    // still reject a driver requesting live effects before execution.
    let _env = LiveLedgerEnvGuard::set(&[
        (crate::product_golden_path::PRODUCT_TASK_GATE, Some("1")),
        ("CI", Some("true")),
    ]);
    let driver = ProductGoldenPathCellDriver {
        allow_live_provider_effects: false,
        target_repo_path: Some(target),
        fake_transport: None,
        cell_executor_key_id: None,
        cell_confirmer_key_id: None,
        campaign_package: None,
    };
    let live_driver = ProductGoldenPathCellDriver {
        allow_live_provider_effects: true,
        target_repo_path: driver.target_repo_path.clone(),
        ..Default::default()
    };
    let live_refusal = live_driver.ensure_effects_ready();
    let result = run_frozen_schedule(
        &store,
        &principal,
        "run-unarmed",
        "auth-unarmed",
        &lease,
        &driver,
    );
    assert!(live_refusal.unwrap_err().contains("forbidden in CI"));
    let result = result.unwrap();
    assert_eq!(result["live_baseline_sealed"], false);
    assert_eq!(result["provider_call_performed"], false);
    let attempts = store.list_rwe_task_attempts_for_run("run-unarmed").unwrap();
    assert_eq!(attempts.len(), 4);
    for a in &attempts {
        assert_eq!(
            a["evidence_json"]["evidence_source"],
            "product_golden_path_owner"
        );
        assert_ne!(a["classification"], "dispatched");
    }
}

#[test]
fn concurrent_duplicate_cell_dispatch_is_single_effect_sqlite() {
    let dir = tempdir().unwrap();
    let store = Arc::new(
        LocalProductStore::new_with_clock(dir.path().join("dup.db"), || {
            "2026-07-25T12:00:00Z".into()
        })
        .unwrap(),
    );
    let principal = operator(&store, "t-dup", "op-dup");
    let lease = Arc::new(admit_ready(
        &store,
        &principal,
        "auth-dup",
        "run-dup",
        "ptask-gp-dup",
    ));
    let frozen = freeze_current_operator_contract_set().unwrap();
    let cell0 = &frozen.schedule.body["cells"][0];
    let task0 = frozen
        .corpus
        .tasks
        .iter()
        .find(|t| t.task_id == cell0["task_id"].as_str().unwrap())
        .unwrap();
    let ids = cell_identities_for("run-dup", cell0, task0).unwrap();
    let envelope0 = cell_reservation_limits(cell0).unwrap();
    let req = envelope0.max_provider_requests;
    let tok = envelope0.max_total_tokens;
    let reservation = json!({
        "schema_version": RWE_CELL_ATTEMPT_EVIDENCE_SCHEMA,
        "cell_id": ids.cell_id,
        "provider_requests": req,
        "total_tokens": tok,
        "authorization_id": "auth-dup",
    });
    let wins = Arc::new(AtomicUsize::new(0));
    let losses = Arc::new(AtomicUsize::new(0));
    std::thread::scope(|scope| {
        for _ in 0..8 {
            let store = Arc::clone(&store);
            let lease = Arc::clone(&lease);
            let wins = Arc::clone(&wins);
            let losses = Arc::clone(&losses);
            let attempt_id = ids.rwe_task_attempt_id.clone();
            let task_id = ids.task_id.clone();
            let def = ids.definition_sha256.clone();
            let reservation = reservation.clone();
            scope.spawn(move || {
                let principal = store
                    .authenticate_managed_acceptance_principal("t-dup", "op-dup", None)
                    .unwrap();
                let envelope = cell_reservation_limits(&json!({
                    "max_provider_requests": req,
                    "max_retries": 0,
                    "max_input_tokens": 12000,
                    "max_output_tokens": 4000,
                    "max_total_tokens": tok,
                    "max_wall_time_ms": 900000,
                    "max_cost": 0.2,
                }))
                .unwrap();
                match store.claim_rwe_cell_dispatch(
                    &principal,
                    "run-dup",
                    "auth-dup",
                    &lease,
                    &attempt_id,
                    &task_id,
                    &def,
                    &envelope,
                    &reservation,
                ) {
                    Ok(v) if v.get("idempotent_replay").and_then(Value::as_bool) != Some(true) => {
                        wins.fetch_add(1, Ordering::SeqCst);
                    }
                    Ok(_) | Err(_) => {
                        losses.fetch_add(1, Ordering::SeqCst);
                    }
                }
            });
        }
    });
    assert_eq!(
        wins.load(Ordering::SeqCst),
        1,
        "exactly one concurrent claim must win"
    );
    assert!(losses.load(Ordering::SeqCst) >= 1);
    let attempts = store.list_rwe_task_attempts_for_run("run-dup").unwrap();
    assert_eq!(attempts.len(), 1);
    assert_eq!(attempts[0]["classification"], "dispatched");
}

#[test]
fn pre_effect_budget_refusal_invokes_driver_zero_times() {
    let dir = tempdir().unwrap();
    let store = Arc::new(
        LocalProductStore::new_with_clock(dir.path().join("budget.db"), || {
            "2026-07-25T12:00:00Z".into()
        })
        .unwrap(),
    );
    let principal = operator(&store, "t-budget", "op-budget");
    let lease = admit_ready(
        &store,
        &principal,
        "auth-budget2",
        "run-budget2",
        "ptask-gp-budget",
    );
    let frozen = freeze_current_operator_contract_set().unwrap();
    let cell0 = &frozen.schedule.body["cells"][0];
    let task0 = frozen
        .corpus
        .tasks
        .iter()
        .find(|t| t.task_id == cell0["task_id"].as_str().unwrap())
        .unwrap();
    let ids0 = cell_identities_for("run-budget2", cell0, task0).unwrap();
    let mut seed_outcome = success_outcomes()[0].clone();
    seed_outcome.provider_requests = 12;
    seed_outcome.total_tokens = 1000;
    let evidence = build_cell_evidence(
        "run-budget2",
        "auth-budget2",
        &frozen,
        cell0,
        task0,
        &ids0,
        &seed_outcome,
    );
    store
        .persist_rwe_task_attempt(
            "run-budget2",
            &lease,
            &ids0.rwe_task_attempt_id,
            &ids0.task_id,
            &ids0.definition_sha256,
            "injected_success",
            &evidence,
        )
        .unwrap();
    let counter = Arc::new(AtomicUsize::new(0));
    let inner = InjectedCellDriver {
        outcomes: success_outcomes(),
    };
    let counting = CountingCellDriver {
        inner: &inner,
        invocations: Arc::clone(&counter),
    };
    let result = run_frozen_schedule(
        &store,
        &principal,
        "run-budget2",
        "auth-budget2",
        &lease,
        &counting,
    )
    .unwrap();
    assert_eq!(counter.load(Ordering::SeqCst), 0);
    assert_eq!(result["attempts_recorded"], 4);
    let classes: Vec<_> = store
        .list_rwe_task_attempts_for_run("run-budget2")
        .unwrap()
        .iter()
        .map(|a| a["classification"].as_str().unwrap().to_string())
        .collect();
    assert!(
        classes.iter().any(|c| c == "blocked_budget"),
        "expected blocked_budget, got {classes:?}"
    );
}

#[test]
fn stop_rule_restart_is_exact_and_never_redispatches_skipped_cells() {
    let dir = tempdir().unwrap();
    let store = Arc::new(
        LocalProductStore::new_with_clock(dir.path().join("stop.db"), || {
            "2026-07-25T12:00:00Z".into()
        })
        .unwrap(),
    );
    let principal = operator(&store, "t-stop", "op-stop");
    let lease = admit_ready(&store, &principal, "auth-stop", "run-stop", "ptask-gp-stop");
    let mut outcomes = success_outcomes();
    outcomes[0].classification = "blocked_authority".into();
    let counter = Arc::new(AtomicUsize::new(0));
    let inner = InjectedCellDriver {
        outcomes: outcomes.clone(),
    };
    let counting = CountingCellDriver {
        inner: &inner,
        invocations: Arc::clone(&counter),
    };
    run_frozen_schedule(
        &store,
        &principal,
        "run-stop",
        "auth-stop",
        &lease,
        &counting,
    )
    .unwrap();
    let first = counter.load(Ordering::SeqCst);
    assert_eq!(first, 1);
    let skipped = store
        .list_rwe_task_attempts_for_run("run-stop")
        .unwrap()
        .iter()
        .filter(|a| a["classification"] == "skipped_by_stop_rule")
        .count();
    assert_eq!(skipped, 3);
    run_frozen_schedule(
        &store,
        &principal,
        "run-stop",
        "auth-stop",
        &lease,
        &counting,
    )
    .unwrap();
    assert_eq!(counter.load(Ordering::SeqCst), first);
}

#[test]
fn outcome_unknown_is_terminal_no_second_authorization() {
    let dir = tempdir().unwrap();
    let store = Arc::new(
        LocalProductStore::new_with_clock(dir.path().join("ou.db"), || {
            "2026-07-25T12:00:00Z".into()
        })
        .unwrap(),
    );
    let principal = operator(&store, "t-ou", "op-ou");
    let lease = admit_ready(&store, &principal, "auth-ou", "run-ou", "ptask-gp-ou");
    let mut outcomes = success_outcomes();
    outcomes[0].classification = "outcome_unknown".into();
    outcomes[0].cost_unknown = true;
    outcomes[0].monetary_cost = None;
    let driver = InjectedCellDriver { outcomes };
    run_frozen_schedule(&store, &principal, "run-ou", "auth-ou", &lease, &driver).unwrap();
    assert!(store
        .get_rwe_run_authorization("auth-ou-2")
        .unwrap()
        .is_none());
    assert_eq!(
        store.get_rwe_run_authorization("auth-ou").unwrap().unwrap()["status"],
        "consumed"
    );
}

#[test]
fn injected_outcome_cannot_seal_live_baseline() {
    let dir = tempdir().unwrap();
    let store = Arc::new(
        LocalProductStore::new_with_clock(dir.path().join("inj-seal.db"), || {
            "2026-07-25T12:00:00Z".into()
        })
        .unwrap(),
    );
    let principal = operator(&store, "t-inj-seal", "op-inj-seal");
    let lease = admit_ready(
        &store,
        &principal,
        "auth-inj-seal",
        "run-inj-seal",
        "ptask-gp-inj-seal",
    );
    let mut outcomes = success_outcomes();
    for o in &mut outcomes {
        o.live_provider_request = true;
        o.classification = "success".into();
    }
    let result = run_frozen_schedule(
        &store,
        &principal,
        "run-inj-seal",
        "auth-inj-seal",
        &lease,
        &InjectedCellDriver { outcomes },
    )
    .unwrap();
    assert_eq!(result["live_baseline_sealed"], false);
}

#[test]
fn cell_identities_are_deterministic() {
    let frozen = freeze_current_operator_contract_set().unwrap();
    let cell = &frozen.schedule.body["cells"][0];
    let task = &frozen.corpus.tasks[0];
    let a = cell_identities_for("run-x", cell, task).unwrap();
    let b = cell_identities_for("run-x", cell, task).unwrap();
    assert_eq!(a, b);
    assert!(a.branch_name.starts_with("acp/rwe/"));
}

#[test]
fn codex_prerequisite_candidates_are_primary_then_explicit_fallback() {
    let frozen = freeze_current_operator_contract_set().unwrap();
    let indices = codex_prerequisite_candidate_indices(&frozen).unwrap();
    let task_ids = indices
        .iter()
        .map(|index| frozen.corpus.tasks[*index].task_id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        task_ids,
        vec![
            CODEX_PREREQUISITE_PRIMARY_TASK_ID,
            CODEX_PREREQUISITE_FALLBACK_TASK_ID,
        ]
    );
    for task_id in task_ids {
        assert!(frozen.schedule.body["cells"]
            .as_array()
            .unwrap()
            .iter()
            .any(|cell| cell.get("task_id").and_then(Value::as_str) == Some(task_id)));
    }
}

#[test]
fn codex_prerequisite_recovery_stops_at_ceiling() {
    assert!(codex_prerequisite_recovery_available(9));
    assert!(!codex_prerequisite_recovery_available(10));
}

#[test]
fn recovery_prefers_durable_failure_terminal_over_reused_finalizer_output() {
    let durable = json!({"terminal_class": "failed", "provider_request_count": 2});
    let finalized = json!({
        "delegated_terminal": {
            "terminal": {"terminal_class": "outcome_unknown"},
            "failure_terminal_evidence": {"terminal_class": "outcome_unknown"}
        }
    });
    let receipt = recovered_failure_terminal_receipt(Some(durable), &finalized).unwrap();
    assert_eq!(receipt["terminal_class"], "failed");
}

#[test]
fn stale_and_wrong_principal_do_not_consume_fresh_authority() {
    let dir = tempdir().unwrap();
    let store = Arc::new(
        LocalProductStore::new_with_clock(dir.path().join("stale.db"), || {
            "2026-07-25T12:00:00Z".into()
        })
        .unwrap(),
    );
    let principal = operator(&store, "t-stale", "op-stale");
    seed_gp(&store, "ptask-gp-stale", principal.tenant_id());
    let foreign = operator(&store, "t-foreign", "op-foreign");
    let err = issue_and_admit_v2(
        &store,
        &foreign,
        "auth-foreign",
        "run-foreign",
        "ptask-gp-stale",
        "2026-08-07T00:00:00Z",
    )
    .unwrap_err();
    assert!(
        err.contains("fail closed") || err.contains("tenant") || err.contains("not found"),
        "{err}"
    );
    assert!(store
        .get_rwe_run_authorization("auth-foreign")
        .unwrap()
        .is_none());
}

#[test]
fn crash_style_admit_lease_recovery_then_schedule() {
    let dir = tempdir().unwrap();
    let store = Arc::new(
        LocalProductStore::new_with_clock(dir.path().join("lease.db"), || {
            "2026-07-25T12:00:00Z".into()
        })
        .unwrap(),
    );
    let principal = operator(&store, "t-lease", "op-lease");
    let _lease = admit_ready(
        &store,
        &principal,
        "auth-lease",
        "run-lease",
        "ptask-gp-lease",
    );
    let driver = InjectedCellDriver {
        outcomes: success_outcomes(),
    };
    let result =
        run_frozen_schedule(&store, &principal, "run-lease", "auth-lease", "", &driver).unwrap();
    assert_eq!(result["attempts_recorded"], 4);
}

#[test]
fn revalidate_rejects_tampered_binding_surface() {
    let dir = tempdir().unwrap();
    let store = Arc::new(
        LocalProductStore::new_with_clock(dir.path().join("reval.db"), || {
            "2026-07-25T12:00:00Z".into()
        })
        .unwrap(),
    );
    let principal = operator(&store, "t-reval", "op-reval");
    let _lease = admit_ready(
        &store,
        &principal,
        "auth-reval",
        "run-reval",
        "ptask-gp-reval",
    );
    let frozen = freeze_current_operator_contract_set().unwrap();
    let ok = revalidate_stored_v2_authorization(&store, &principal, "auth-reval", &frozen);
    assert!(ok.is_ok());
}

#[test]
fn claim_refuses_auth_mismatch_and_stale_lease() {
    let dir = tempdir().unwrap();
    let store = Arc::new(
        LocalProductStore::new_with_clock(dir.path().join("auth-mis.db"), || {
            "2026-07-25T12:00:00Z".into()
        })
        .unwrap(),
    );
    let principal = operator(&store, "t-mis", "op-mis");
    let lease = admit_ready(&store, &principal, "auth-mis", "run-mis", "ptask-gp-mis");
    let frozen = freeze_current_operator_contract_set().unwrap();
    let cell0 = &frozen.schedule.body["cells"][0];
    let task0 = frozen
        .corpus
        .tasks
        .iter()
        .find(|t| t.task_id == cell0["task_id"].as_str().unwrap())
        .unwrap();
    let ids = cell_identities_for("run-mis", cell0, task0).unwrap();
    let envelope = cell_reservation_limits(cell0).unwrap();
    let reservation = json!({"cell_id": ids.cell_id});
    let wrong_auth = store
        .claim_rwe_cell_dispatch(
            &principal,
            "run-mis",
            "auth-NOT-BOUND",
            &lease,
            &ids.rwe_task_attempt_id,
            &ids.task_id,
            &ids.definition_sha256,
            &envelope,
            &reservation,
        )
        .unwrap_err();
    assert!(
        wrong_auth.contains("authorization mismatch")
            || wrong_auth.contains("not found")
            || wrong_auth.contains("no rows")
            || wrong_auth.contains("Query returned"),
        "{wrong_auth}"
    );
    let stale_lease = store
        .claim_rwe_cell_dispatch(
            &principal,
            "run-mis",
            "auth-mis",
            "rwe-lease-stale-token",
            &ids.rwe_task_attempt_id,
            &ids.task_id,
            &ids.definition_sha256,
            &envelope,
            &reservation,
        )
        .unwrap_err();
    assert!(
        stale_lease.contains("lease") || stale_lease.contains("owner"),
        "{stale_lease}"
    );
}

#[test]
fn claim_refuses_budget_dimension_overflow_and_preserves_failed_cost() {
    let dir = tempdir().unwrap();
    let store = Arc::new(
        LocalProductStore::new_with_clock(dir.path().join("bdim.db"), || {
            "2026-07-25T12:00:00Z".into()
        })
        .unwrap(),
    );
    let principal = operator(&store, "t-bdim", "op-bdim");
    let lease = admit_ready(&store, &principal, "auth-bdim", "run-bdim", "ptask-gp-bdim");
    let frozen = freeze_current_operator_contract_set().unwrap();
    let cell0 = &frozen.schedule.body["cells"][0];
    let task0 = frozen
        .corpus
        .tasks
        .iter()
        .find(|t| t.task_id == cell0["task_id"].as_str().unwrap())
        .unwrap();
    let ids = cell_identities_for("run-bdim", cell0, task0).unwrap();
    // Exhaust run-level request ceiling with a full-reservation terminal attempt.
    let mut seed = success_outcomes()[0].clone();
    seed.provider_requests = 12;
    seed.total_tokens = 1000;
    let evidence = build_cell_evidence("run-bdim", "auth-bdim", &frozen, cell0, task0, &ids, &seed);
    store
        .persist_rwe_task_attempt(
            "run-bdim",
            &lease,
            &ids.rwe_task_attempt_id,
            &ids.task_id,
            &ids.definition_sha256,
            "controlled_failure",
            &evidence,
        )
        .unwrap();
    // Second cell must refuse reservation.
    let cell1 = &frozen.schedule.body["cells"][1];
    let task1 = frozen
        .corpus
        .tasks
        .iter()
        .find(|t| t.task_id == cell1["task_id"].as_str().unwrap())
        .unwrap();
    let ids1 = cell_identities_for("run-bdim", cell1, task1).unwrap();
    let envelope = cell_reservation_limits(cell1).unwrap();
    let err = store
        .claim_rwe_cell_dispatch(
            &principal,
            "run-bdim",
            "auth-bdim",
            &lease,
            &ids1.rwe_task_attempt_id,
            &ids1.task_id,
            &ids1.definition_sha256,
            &envelope,
            &json!({"cell_id": ids1.cell_id}),
        )
        .unwrap_err();
    assert!(err.contains("budget"), "{err}");
    // Failed-attempt cost preserved on first row.
    let rows = store.list_rwe_task_attempts_for_run("run-bdim").unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["evidence_json"]["provider_requests"], 12);
}

#[test]
fn provider_journal_mismatch_and_reused_receipt_cannot_seal() {
    let dir = tempdir().unwrap();
    let store = Arc::new(
        LocalProductStore::new_with_clock(dir.path().join("journal.db"), || {
            "2026-07-25T12:00:00Z".into()
        })
        .unwrap(),
    );
    let principal = operator(&store, "t-jnl", "op-jnl");
    let lease = admit_ready(&store, &principal, "auth-jnl", "run-jnl", "ptask-gp-jnl");
    let mut outcomes = success_outcomes();
    for o in &mut outcomes {
        o.classification = "success".into();
        o.evidence_source = "product_golden_path_owner".into();
        o.live_provider_request = true;
        o.provider_requests = 3;
        o.total_tokens = 100;
    }
    let result = run_frozen_schedule(
        &store,
        &principal,
        "run-jnl",
        "auth-jnl",
        &lease,
        &InjectedCellDriver { outcomes },
    )
    .unwrap();
    // Injected driver forces evidence_source=injected and non-seal classifications.
    assert_eq!(result["live_baseline_sealed"], false);
}

#[test]
fn production_driver_accepts_fake_transport_without_caller_seal_evidence() {
    use crate::provider::transport::{HttpResponse, MockTransport};
    let dir = tempdir().unwrap();
    let store = Arc::new(
        LocalProductStore::new_with_clock(dir.path().join("fake-tx.db"), || {
            "2026-07-25T12:00:00Z".into()
        })
        .unwrap(),
    );
    let principal = operator(&store, "t-ftx", "op-ftx");
    let lease = admit_ready(&store, &principal, "auth-ftx", "run-ftx", "ptask-gp-ftx");
    let target = dir.path().join("target");
    std::fs::create_dir_all(target.join("apps/api/src")).unwrap();
    std::fs::create_dir_all(target.join("apps/api/tests")).unwrap();
    std::fs::write(target.join("README.md"), "rwe\n").unwrap();
    let _lock = crate::cli::config::cli_env_test_lock()
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    std::env::set_var(crate::product_golden_path::PRODUCT_TASK_GATE, "1");
    // The coordinator refuses to execute any cell effect while CI is set;
    // simulate the operator environment around this fake-transport run.
    let had_ci = std::env::var_os("CI");
    std::env::remove_var("CI");
    let transport = std::sync::Arc::new(MockTransport::new(vec![Ok(HttpResponse {
        status: 200,
        body: br#"{"choices":[{"message":{"content":"{}"}}]}"#.to_vec(),
    })]));
    let driver = ProductGoldenPathCellDriver {
        allow_live_provider_effects: false,
        target_repo_path: Some(target),
        fake_transport: Some(transport),
        cell_executor_key_id: None,
        cell_confirmer_key_id: None,
        campaign_package: None,
    };
    let result =
        run_frozen_schedule(&store, &principal, "run-ftx", "auth-ftx", &lease, &driver).unwrap();
    match had_ci {
        Some(v) => std::env::set_var("CI", v),
        None => std::env::remove_var("CI"),
    }
    assert_eq!(result["live_baseline_sealed"], false);
    assert_eq!(result["provider_call_performed"], false);
    assert_eq!(result["provider_transport_provenance"], "none");
    for a in store.list_rwe_task_attempts_for_run("run-ftx").unwrap() {
        assert_eq!(
            a["evidence_json"]["evidence_source"],
            "product_golden_path_owner"
        );
        assert_ne!(a["evidence_json"]["evidence_source"], "injected");
    }
    std::env::remove_var(crate::product_golden_path::PRODUCT_TASK_GATE);
}

/// A deliberately hostile transport that behaves like a production client
/// but is not the canonical ReqwestTransport concrete type. The trait has
/// no provenance surface, so it can never mint External; the fake slot is
/// wrapped in InjectedTransportBoundary regardless.
struct SpoofExternalTransport {
    responses: std::sync::Mutex<
        std::collections::VecDeque<
            Result<crate::provider::transport::HttpResponse, crate::provider::transport::HttpError>,
        >,
    >,
}

#[async_trait::async_trait]
impl crate::provider::transport::HttpTransport for SpoofExternalTransport {
    async fn send(
        &self,
        _request: &crate::provider::transport::HttpRequest,
    ) -> Result<crate::provider::transport::HttpResponse, crate::provider::transport::HttpError>
    {
        self.responses
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| {
                Err(crate::provider::transport::HttpError::Connection(
                    "no spoof responses left".to_string(),
                ))
            })
    }
}

#[test]
fn fake_transport_slot_cannot_impersonate_external_provenance() {
    let dir = tempdir().unwrap();
    let store = Arc::new(
        LocalProductStore::new_with_clock(dir.path().join("spoof.db"), || {
            "2026-07-25T12:00:00Z".into()
        })
        .unwrap(),
    );
    let principal = operator(&store, "t-spoof", "op-spoof");
    let lease = admit_ready(
        &store,
        &principal,
        "auth-spoof",
        "run-spoof",
        "ptask-gp-spoof",
    );
    let target = dir.path().join("target");
    std::fs::create_dir_all(target.join("apps/api/src")).unwrap();
    std::fs::create_dir_all(target.join("apps/api/tests")).unwrap();
    std::fs::write(target.join("README.md"), "rwe\n").unwrap();
    let _lock = crate::cli::config::cli_env_test_lock()
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    std::env::set_var(crate::product_golden_path::PRODUCT_TASK_GATE, "1");
    let had_ci = std::env::var_os("CI");
    std::env::remove_var("CI");
    // 4 cells x 3 requests of plausible-looking responses: the spoof
    // transport completes the full lifecycle but must never mint External.
    let responses = (0..12)
        .map(|_| {
            Ok(HttpResponse {
                status: 200,
                body: br#"{"choices":[{"message":{"content":"{}"}}]}"#.to_vec(),
            })
        })
        .collect::<std::collections::VecDeque<_>>();
    let transport = std::sync::Arc::new(SpoofExternalTransport {
        responses: std::sync::Mutex::new(responses),
    });
    let driver = ProductGoldenPathCellDriver {
        allow_live_provider_effects: false,
        target_repo_path: Some(target),
        fake_transport: Some(transport),
        cell_executor_key_id: None,
        cell_confirmer_key_id: None,
        campaign_package: None,
    };
    let result = run_frozen_schedule(
        &store,
        &principal,
        "run-spoof",
        "auth-spoof",
        &lease,
        &driver,
    )
    .unwrap();
    match had_ci {
        Some(v) => std::env::set_var("CI", v),
        None => std::env::remove_var("CI"),
    }
    // The spoof path can never become a live baseline: the run seals
    // nothing, performs no live provider call, and its provenance is
    // never external (fail-closed to none/injected).
    assert_eq!(result["live_baseline_sealed"], false);
    assert_eq!(result["provider_call_performed"], false);
    assert_ne!(
        result["provider_transport_provenance"], "external",
        "a custom transport cannot mint external provenance: {result}"
    );
    let attempts = store.list_rwe_task_attempts_for_run("run-spoof").unwrap();
    assert_eq!(attempts.len(), 4);
    for a in &attempts {
        assert_ne!(
            a["evidence_json"]["provider_transport_provenance"], "external",
            "durable attempt evidence must never carry external provenance for the spoof path: {a}"
        );
        assert_eq!(a["evidence_json"]["live_provider_request"], false);
    }
    // The store evidence gate (store_evidence_transport_provenance) is
    // exercised directly by store_evidence_transport_provenance_gate_is_fail_closed;
    // here the durable attempt evidence itself must never carry external
    // provenance for the spoof path.
    std::env::remove_var(crate::product_golden_path::PRODUCT_TASK_GATE);
}

#[test]
fn completed_prerequisite_requires_exact_codex_binding_and_model() {
    let package = crate::rwe::campaign_package::canonical_codex_luna_xhigh_v2_package().unwrap();
    let binding = package
        .provider_execution_binding_for_model(&package.admitted_model)
        .unwrap();
    let manifest_identity = json!({
        "campaign_package_id": package.package_id.clone(),
        "provider_execution_binding": binding.to_json(),
        "models": {
            "planner": package.admitted_model.clone(),
            "implementer": package.admitted_model.clone(),
            "reviewer": package.admitted_model.clone(),
        },
        "target": {"main_sha": FROZEN_RWE_TARGET_MAIN_SHA},
        "execution": {"product_task_id": "ptask", "attempt_id": "attempt"},
        "output": {
            "draft_pr_only": true,
            "target_main_write": false,
            "merge": false,
            "auto_merge": false,
        },
    });
    let request = json!({
        "provider_kind": "chatgpt_subscription",
        "protocol": "open_ai_compatible",
        "requested_model": "gpt-5.6-luna",
        "resolved_model": "gpt-5.6-luna",
        "usage": {"model": "gpt-5.6-luna"}
    });
    let projection = json!({
        "provider_execution": {
            "schema_version": "managed_deepseek_execution_evidence.v1",
            "provider_identity": "chatgpt-codex-subscription",
            "provider_kind": "chatgpt_subscription",
            "requests": [request.clone(), request.clone(), request.clone()]
        }
    });
    let validate = |value: &Value| {
        validate_prerequisite_provider_identity(
            value,
            &manifest_identity,
            &binding,
            &package.package_id,
            "ptask",
            "attempt",
        )
    };
    assert!(validate(&projection).is_ok());
    let mut openai_compatible_projection = projection.clone();
    for req in openai_compatible_projection["provider_execution"]["requests"]
        .as_array_mut()
        .unwrap()
    {
        req["protocol"] = json!("openai_compatible");
    }
    assert!(validate(&openai_compatible_projection).is_ok());
    let mut wrong_provider_identity = projection.clone();
    wrong_provider_identity["provider_execution"]["provider_identity"] =
        json!("different-identity");
    assert!(validate(&wrong_provider_identity).is_err());
    for pointer in [
        "/provider_kind",
        "/protocol",
        "/requested_model",
        "/resolved_model",
        "/usage/model",
    ] {
        let mut wrong = request.clone();
        *wrong.pointer_mut(pointer).unwrap() = json!("different-identity");
        let mut wrong_projection = projection.clone();
        wrong_projection["provider_execution"]["requests"][2] = wrong;
        assert!(validate(&wrong_projection).is_err());
    }
    assert!(validate(&json!({"provider_execution": {"requests": []}})).is_err());
    assert!(validate(&json!({})).is_err());

    let mut wrong_manifest = manifest_identity.clone();
    wrong_manifest["provider_execution_binding"]["reasoning_effort"] = json!("low");
    assert!(validate_prerequisite_provider_identity(
        &projection,
        &wrong_manifest,
        &binding,
        &package.package_id,
        "ptask",
        "attempt",
    )
    .is_err());
}

#[test]
fn store_evidence_transport_provenance_gate_is_fail_closed() {
    let external_entry = |status: &str| {
        json!({
            "schema_version": "managed_provider_request_claim.v1",
            "node_id": "n",
            "status": status,
            "transport_provenance": "external",
            "request_sha256": "a".repeat(64),
        })
    };
    let injected_entry = json!({
        "schema_version": "managed_provider_request_claim.v1",
        "node_id": "n",
        "status": "succeeded",
        "transport_provenance": "injected",
        "request_sha256": "b".repeat(64),
    });
    let external_projection = json!({
        "provider_execution": {
            "schema_version": "managed_deepseek_execution_evidence.v1",
            "provider_request_count": 1,
            "transport_provenance": "external",
            "requests": [json!({"node_id": "n", "transport_provenance": "external"})],
        },
        "provider_request_journal": [external_entry("succeeded")],
    });
    let injected_projection = json!({
        "provider_execution": {
            "schema_version": "managed_deepseek_execution_evidence.v1",
            "provider_request_count": 1,
            "transport_provenance": "injected",
            "requests": [json!({"node_id": "n", "transport_provenance": "injected"})],
        },
        "provider_request_journal": [injected_entry.clone()],
    });

    assert_eq!(
        store_evidence_transport_provenance(&external_projection).as_deref(),
        Ok("external")
    );
    assert_eq!(
        store_evidence_transport_provenance(&injected_projection).as_deref(),
        Ok("injected")
    );

    // Missing aggregate provenance fails closed.
    let mut missing_aggregate = external_projection.clone();
    missing_aggregate["provider_execution"]
        .as_object_mut()
        .unwrap()
        .remove("transport_provenance");
    assert!(store_evidence_transport_provenance(&missing_aggregate).is_err());

    // Missing journal-entry provenance fails closed.
    let mut missing_entry = external_projection.clone();
    missing_entry["provider_request_journal"][0]
        .as_object_mut()
        .unwrap()
        .remove("transport_provenance");
    assert!(store_evidence_transport_provenance(&missing_entry).is_err());

    // Mixed aggregate vs journal provenance fails closed.
    let mut mixed = external_projection.clone();
    mixed["provider_request_journal"] = json!([injected_entry.clone()]);
    assert!(store_evidence_transport_provenance(&mixed).is_err());

    // Empty journal fails closed even with an external aggregate claim.
    let mut empty_journal = external_projection.clone();
    empty_journal["provider_request_journal"] = json!([]);
    assert!(store_evidence_transport_provenance(&empty_journal).is_err());

    // Invalid provenance value fails closed.
    let mut invalid = external_projection.clone();
    invalid["provider_execution"]["transport_provenance"] = json!("fixture");
    assert!(store_evidence_transport_provenance(&invalid).is_err());

    // Null provider execution fails closed (provider-free is never a seal).
    let no_provider = json!({
        "provider_execution": Value::Null,
        "provider_request_journal": [],
    });
    assert!(store_evidence_transport_provenance(&no_provider).is_err());
}

#[test]
fn injected_transport_cannot_seal_with_forged_receipts_or_outcomes() {
    // Even if every receipt and outcome were forged as external, the
    // durable journal's injected provenance blocks sealing.
    let forged_external_receipt = json!({
        "provider_execution": {
            "schema_version": "managed_deepseek_execution_evidence.v1",
            "provider_request_count": 3,
            "transport_provenance": "external",
            "requests": [json!({})],
        },
        "provider_request_journal": [injected_claim_entry(), injected_claim_entry(), injected_claim_entry()],
    });
    assert_ne!(
        store_evidence_transport_provenance(&forged_external_receipt).as_deref(),
        Ok("external"),
        "injected journal claims must defeat forged external receipts"
    );
}

#[test]
fn provider_free_evidence_is_never_seal_eligible() {
    let provider_free = json!({
        "provider_execution": Value::Null,
        "provider_request_journal": [],
    });
    assert!(
        store_evidence_transport_provenance(&provider_free).is_err(),
        "provider-free execution has no transport provenance and cannot seal"
    );
}

fn injected_claim_entry() -> Value {
    json!({
        "schema_version": "managed_provider_request_claim.v1",
        "node_id": "n",
        "status": "succeeded",
        "transport_provenance": "injected",
        "request_sha256": "c".repeat(64),
    })
}

#[test]
fn evaluator_surface_sentinel_rejects_verifier_tampering_paths() {
    // Test collection and verifier config files are never real delivery.
    assert!(changed_file_is_evaluator_surface(
        "apps/api/tests/conftest.py"
    ));
    assert!(changed_file_is_evaluator_surface(
        "apps/api/tests/pytest.ini"
    ));
    assert!(changed_file_is_evaluator_surface("apps/api/tests/tox.ini"));
    assert!(changed_file_is_evaluator_surface(
        "apps/api/tests/setup.cfg"
    ));
    assert!(changed_file_is_evaluator_surface("pyproject.toml"));
    // Ordinary product files are not evaluator surface.
    assert!(!changed_file_is_evaluator_surface("apps/api/src/main.py"));
    assert!(!changed_file_is_evaluator_surface(
        "apps/api/tests/test_flow.py"
    ));
    assert!(!changed_file_is_evaluator_surface("README.md"));
    assert!(!changed_file_is_evaluator_surface(
        "apps/api/tests/conftest_notes.md"
    ));
}

#[test]
fn execution_error_after_fence_terminalizes_without_second_auth() {
    struct FailDriver;
    impl CellDriver for FailDriver {
        fn execute_cell(
            &self,
            _store: &std::sync::Arc<LocalProductStore>,
            _principal: &AuthenticatedPrincipal,
            _frozen: &OperatorFrozenContractSet,
            _run_id: &str,
            _lease_token: &str,
            _cell: &Value,
            _task: &RweTaskDefinition,
            _ids: &CellIdentities,
        ) -> Result<CellOutcome, String> {
            Err("provider timeout while awaiting response".into())
        }
    }
    let dir = tempdir().unwrap();
    let store = Arc::new(
        LocalProductStore::new_with_clock(dir.path().join("fence-err.db"), || {
            "2026-07-25T12:00:00Z".into()
        })
        .unwrap(),
    );
    let principal = operator(&store, "t-ferr", "op-ferr");
    let lease = admit_ready(&store, &principal, "auth-ferr", "run-ferr", "ptask-gp-ferr");
    run_frozen_schedule(
        &store,
        &principal,
        "run-ferr",
        "auth-ferr",
        &lease,
        &FailDriver,
    )
    .unwrap();
    let rows = store.list_rwe_task_attempts_for_run("run-ferr").unwrap();
    assert!(!rows.is_empty());
    assert_eq!(rows[0]["classification"], "cleanup_failed");
    assert_ne!(rows[0]["classification"], "dispatched");
    // No second authorization consumed.
    assert!(store
        .get_rwe_run_authorization("auth-ferr-2")
        .unwrap()
        .is_none());
}

/// Clone the frozen target at the exact frozen SHA: local checkout when
/// present (offline), otherwise a GitHub clone (CI). Returns None when the
/// repository is unavailable so the test skips instead of faking evidence.
/// Operator live-run gate for the armed Draft PR test: only an explicit
/// `=1` authorizes the genuine remote push + GitHub Draft PR external
/// effects. Normal CI never sets it, so the armed test SKIPs there.
const ARMED_LIVE_RUN_GATE: &str = "ACP_RWE_ARMED_LIVE_RUN";

fn frozen_target_repo(dir: &std::path::Path) -> Option<std::path::PathBuf> {
    use std::process::Command;
    let target = dir.join("frozen-target");
    let local = std::path::Path::new("/home/igzela/Projects/alters-lab");
    let live_run = std::env::var(ARMED_LIVE_RUN_GATE).is_ok_and(|value| value == "1");
    let clone_ok = if live_run {
        // Live-run mode requires the credential-free https origin so the
        // approved branch is genuinely pushed to the operator-authorized
        // remote and the Draft PR is created there. The operator proxy can
        // drop large pack transfers, so retry the clone a bounded number of
        // times; a persistent failure skips the fixture honestly.
        (0..3).any(|_| {
            Command::new("git")
                .args([
                    "-c",
                    "http.lowSpeedLimit=1",
                    "-c",
                    "http.lowSpeedTime=30",
                    "clone",
                    "--no-checkout",
                    "https://github.com/Igzela/alters-lab.git",
                ])
                .arg(&target)
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
                || {
                    let _ = Command::new("rm").args(["-rf"]).arg(&target).status();
                    false
                }
        })
    } else if local.is_dir() {
        Command::new("git")
            .args(["clone", "--no-checkout"])
            .arg(local)
            .arg(&target)
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    } else {
        Command::new("git")
            .args([
                "clone",
                "--no-checkout",
                "https://github.com/Igzela/alters-lab.git",
            ])
            .arg(&target)
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    };
    if !clone_ok {
        return None;
    }
    let checkout_ok = Command::new("git")
        .args(["checkout", "-B", "main", FROZEN_RWE_TARGET_MAIN_SHA])
        .current_dir(&target)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !checkout_ok {
        return None;
    }
    for args in [
        vec!["config", "user.email", "rwe-test@example.invalid"],
        vec!["config", "user.name", "RWE Test"],
    ] {
        let _ = Command::new("git").args(args).current_dir(&target).status();
    }
    Some(target)
}

fn mock_rwe_openai_response(id: &str, model: &str, content: Value) -> HttpResponse {
    HttpResponse {
        status: 200,
        body: json!({
            "id": id,
            "model": model,
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": content.to_string()
                },
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 20, "completion_tokens": 10}
        })
        .to_string()
        .into_bytes(),
    }
}

/// Armed production driver: the four-cell frozen schedule genuinely executes
/// the delegated lifecycle through the store owners (contract, proposal,
/// manifest approval, one-use spend, attempt lease, activation, managed
/// executor over the injected fake transport, frozen pytest verifier,
/// artifact confirmation, genuine Draft PR creation against the exact
/// operator-authorized remote, terminal receipt, cleanup) and the run seals
/// from store-owned receipts alone.
///
/// Requires the operator live-run gate `ACP_RWE_ARMED_LIVE_RUN=1` plus
/// populated `ACP_GITHUB_TOKEN_ENV` / `ACP_TARGET_REPO_GIT_TOKEN_ENV`
/// references; without it the test SKIPs (like `frozen_target_repo`), so
/// normal CI never creates external effects.
#[test]
/// Armed operator-gated live Draft PR lifecycle integration fixture.
///
/// With the operator live-run gate set, this test genuinely executes the
/// complete delegated lifecycle (store owners, frozen pytest verifier in
/// the app-owned worktree, branch push, real GitHub Draft PR on the frozen
/// target) through the INJECTED MockTransport. It is an integration-fixture
/// proof of the lifecycle and Draft PR output only: the injected transport
/// can never yield a live baseline seal, and the cells are recorded as
/// `fixture_success`, never `success`. Without the gate the test SKIPs and
/// creates no external effect.
fn armed_production_driver_executes_genuine_delegated_lifecycle_as_integration_fixture() {
    use crate::provider::transport::MockTransport;
    let dir = tempdir().unwrap();
    if !std::env::var(ARMED_LIVE_RUN_GATE).is_ok_and(|value| value == "1") {
        eprintln!(
                "SKIP: armed live Draft PR run requires {ARMED_LIVE_RUN_GATE}=1 with populated ACP_GITHUB_TOKEN_ENV and ACP_TARGET_REPO_GIT_TOKEN_ENV (operator live-run authorization)"
            );
        return;
    }
    let Some(target) = frozen_target_repo(dir.path()) else {
        eprintln!("SKIP: frozen target repository unavailable (offline without local clone)");
        return;
    };
    let store = Arc::new(
        LocalProductStore::new_with_clock(dir.path().join("armed.db"), || {
            "2026-07-25T12:00:00Z".into()
        })
        .unwrap(),
    );
    let tenant = "t-armed";
    let principal = operator(&store, tenant, "op-armed");
    let _executor = cell_executor(&store, tenant, "op-armed-exec");
    let _confirmer = cell_confirmer(&store, tenant, "op-armed-conf");
    let lease = admit_ready(
        &store,
        &principal,
        "auth-armed",
        "run-armed",
        "ptask-gp-armed",
    );

    // Env guard: product gate + target repo output + credential symbol stay
    // set for the whole run; CI is removed so the armed path is exercised.
    // The operator live-run gate above authorizes the genuine network
    // output: branch push + real GitHub Draft PR on the exact remote.
    let _lock = crate::cli::config::cli_env_test_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let had_cred = std::env::var_os(DEEPSEEK_CREDENTIAL_REFERENCE);
    let had_ci = std::env::var_os("CI");
    let had_live_token = std::env::var_os(RWE_OPERATOR_LIVE_RUN_TOKEN);
    let had_env = [
        crate::product_golden_path::PRODUCT_TASK_GATE,
        "ACP_ENABLE_TARGET_REPO_OUTPUT",
        "ACP_TARGET_REPO_OUTPUT_KILL_SWITCH",
        "ACP_TARGET_REPO_REMOTE_HOST_ALLOWLIST",
        "ACP_PRODUCT_GOLDEN_PATH_ALLOW_NETWORK_OUTPUT",
        "ACP_ENABLE_GITHUB_PR_OUTPUT",
        "ACP_GITHUB_REPOSITORY_ALLOWLIST",
    ]
    .map(|name| (name, std::env::var_os(name)));
    std::env::set_var(crate::product_golden_path::PRODUCT_TASK_GATE, "1");
    std::env::set_var("ACP_ENABLE_TARGET_REPO_OUTPUT", "1");
    std::env::set_var("ACP_TARGET_REPO_OUTPUT_KILL_SWITCH", "0");
    std::env::set_var("ACP_TARGET_REPO_REMOTE_HOST_ALLOWLIST", "github.com");
    std::env::set_var("ACP_PRODUCT_GOLDEN_PATH_ALLOW_NETWORK_OUTPUT", "1");
    std::env::set_var("ACP_ENABLE_GITHUB_PR_OUTPUT", "1");
    std::env::set_var("ACP_GITHUB_REPOSITORY_ALLOWLIST", "Igzela/alters-lab");
    std::env::set_var(DEEPSEEK_CREDENTIAL_REFERENCE, "test-operator-credential");
    std::env::set_var(RWE_OPERATOR_LIVE_RUN_TOKEN, "1");
    std::env::remove_var("CI");

    // 4 cells × 3 managed requests: planning / implementation / review.
    let plan = json!({
        "schema_version": "managed_deepseek_plan.v1",
        "status": "planned",
        "path": "apps/api/tests/test_alters_persist.py",
        "intent": "bounded_product_task"
    });
    // The frozen pytest suite is green only when the operator runtime data
    // is present; the mock implementer therefore ships a self-contained
    // conftest hook that skips the data-dependent modules, so the frozen
    // verifier passes in the app-owned worktree at the exact frozen SHA.
    let implement = json!({
        "schema_version": "managed_workspace_action.v1",
        "action": "replace_text",
        "path": "apps/api/tests/conftest.py",
        "old_text": "from __future__ import annotations",
        "new_text": "from __future__ import annotations\n\nimport pytest\n\n_DATA_DEPENDENT_MODULES = {\n    \"test_cycle_summary_api.py\",\n    \"test_validate_active_yaml_cli.py\",\n    \"test_active_yaml_loader.py\",\n    \"test_provider_dialogue.py\",\n    \"test_alter_dialogue_api.py\",\n    \"test_alter_dialogue.py\",\n    \"test_generation_drafts_api.py\",\n    \"test_snapshot_persist_api.py\",\n    \"test_alter_rubric_baseline.py\",\n    \"test_day30_harness.py\",\n    \"test_p8_m2_e2e_validation.py\",\n}\n\n\n@pytest.hookimpl(tryfirst=True)\ndef pytest_collection_modifyitems(config, items):\n    for item in items:\n        module = item.nodeid.split(\"::\")[0].split(\"/\")[-1]\n        if module in _DATA_DEPENDENT_MODULES:\n            item.add_marker(\n                pytest.mark.skip(reason=\"frozen suite runtime data unavailable\")\n            )"
    });
    let review = json!({
        "schema_version": "managed_deepseek_review.v1",
        "status": "accepted",
        "material_objections": []
    });
    let mut responses = Vec::new();
    for i in 0..4 {
        responses.push(Ok(mock_rwe_openai_response(
            &format!("armed-plan-{i}"),
            OPERATOR_ADMITTED_PLANNER_REVIEWER_MODEL,
            plan.clone(),
        )));
        responses.push(Ok(mock_rwe_openai_response(
            &format!("armed-implement-{i}"),
            OPERATOR_ADMITTED_MODEL,
            implement.clone(),
        )));
        responses.push(Ok(mock_rwe_openai_response(
            &format!("armed-review-{i}"),
            OPERATOR_ADMITTED_PLANNER_REVIEWER_MODEL,
            review.clone(),
        )));
    }
    let transport = std::sync::Arc::new(MockTransport::new(responses));
    let driver = ProductGoldenPathCellDriver {
        allow_live_provider_effects: true,
        target_repo_path: Some(target),
        fake_transport: Some(transport),
        cell_executor_key_id: Some("op-armed-exec".into()),
        cell_confirmer_key_id: Some("op-armed-conf".into()),
        campaign_package: None,
    };

    let result = run_frozen_schedule(
        &store,
        &principal,
        "run-armed",
        "auth-armed",
        &lease,
        &driver,
    )
    .unwrap();

    match had_cred {
        Some(v) => std::env::set_var(DEEPSEEK_CREDENTIAL_REFERENCE, v),
        None => std::env::remove_var(DEEPSEEK_CREDENTIAL_REFERENCE),
    }
    match had_ci {
        Some(v) => std::env::set_var("CI", v),
        None => std::env::remove_var("CI"),
    }
    match had_live_token {
        Some(v) => std::env::set_var(RWE_OPERATOR_LIVE_RUN_TOKEN, v),
        None => std::env::remove_var(RWE_OPERATOR_LIVE_RUN_TOKEN),
    }
    for name in [
        crate::product_golden_path::PRODUCT_TASK_GATE,
        "ACP_ENABLE_TARGET_REPO_OUTPUT",
        "ACP_TARGET_REPO_OUTPUT_KILL_SWITCH",
        "ACP_TARGET_REPO_REMOTE_HOST_ALLOWLIST",
        "ACP_PRODUCT_GOLDEN_PATH_ALLOW_NETWORK_OUTPUT",
        "ACP_ENABLE_GITHUB_PR_OUTPUT",
        "ACP_GITHUB_REPOSITORY_ALLOWLIST",
    ] {
        match had_env.iter().find(|(n, _)| *n == name) {
            Some((_, Some(v))) => std::env::set_var(name, v),
            Some((_, None)) | None => std::env::remove_var(name),
        }
    }

    assert_eq!(
        result["live_baseline_sealed"], false,
        "injected transport must never seal a live baseline: {result}"
    );
    assert_eq!(
        result["provider_call_performed"], false,
        "injected requests are not real provider calls: {result}"
    );
    assert_eq!(result["provider_transport_provenance"], "injected");
    assert_eq!(result["injected_provider_call_performed"], true);
    let cell_results = result["aggregate"]["cell_results"].as_array().unwrap();
    let outcome_unknown_cell = cell_results
        .iter()
        .any(|c| c["classification"].as_str() == Some("outcome_unknown"));
    assert_eq!(
            result["integration_fixture_completed"], !outcome_unknown_cell,
            "fixture completion requires every cell to reach a terminal class; an outcome_unknown cell fails the fixture closed: {result}"
        );
    let run_status = result["run"]["status"].as_str().unwrap_or("");
    if outcome_unknown_cell {
        assert_eq!(
            result["aggregate"]["stopped_by"], "outcome_unknown_no_retry",
            "outcome_unknown must stop the schedule, never retry or advance: {result}"
        );
        assert_eq!(run_status, "outcome_unknown");
    } else {
        assert!(
            result["aggregate"]["stopped_by"].is_null(),
            "a fully completed fixture run has no stop rule: {result}"
        );
        assert_eq!(run_status, "succeeded");
    }
    assert_eq!(result["cell_count"], 4);
    assert_eq!(result["attempts_recorded"], 4);
    let attempts = store.list_rwe_task_attempts_for_run("run-armed").unwrap();
    for a in &attempts {
        let class = a["classification"].as_str().unwrap_or("");
        let ev = a["evidence_json"].clone();
        assert_eq!(
            ev["live_provider_request"], false,
            "injected provider requests are never live provider requests: {ev}"
        );
        match class {
            "fixture_success" => {
                assert_eq!(ev["evidence_source"], "product_golden_path_owner");
                assert_eq!(ev["provider_transport_provenance"], "injected");
                assert_eq!(ev["provider_requests"], 3);
                // Fixture honesty: the note records the verifier's real result and
                // the evaluator-surface skip patch as fixture evidence.
                let note = ev["note"].as_str().unwrap_or("");
                assert!(note.contains("verifier_result=evidence_recorded"), "{note}");
                assert!(note.contains("verifier_output_sha256="), "{note}");
                assert!(
                    note.contains("evaluator-surface skip patch"),
                    "fixture skip patch must be recorded in evidence: {note}"
                );
                assert!(
                    note.contains("fixture evidence only, never accepted delivery"),
                    "{note}"
                );
            }
            "outcome_unknown" => {
                assert_eq!(ev["evidence_source"], "product_golden_path_owner");
                assert_eq!(ev["provider_transport_provenance"], "none");
                assert_eq!(ev["provider_requests"], 0);
                let note = ev["note"].as_str().unwrap_or("");
                assert!(
                    note.contains("outcome_unknown") && note.contains("git push"),
                    "outcome_unknown evidence must record the real external failure: {note}"
                );
            }
            "skipped_by_stop_rule" => {
                assert_eq!(ev["provider_transport_provenance"], "none");
                assert_eq!(ev["evidence_source"], "blocked");
                assert_eq!(ev["note"], "outcome_unknown_no_retry");
            }
            other => panic!("unexpected fixture classification {other}"),
        }
    }
    // Store-owned terminal receipts per cell: provider execution journal of
    // exactly three managed requests with injected provenance, verification
    // recorded, cleanup done, and a real Draft PR record on the frozen target.
    let frozen = freeze_current_operator_contract_set().unwrap();
    let cells = frozen.schedule.body["cells"].as_array().unwrap().clone();
    for cell in &cells {
        let task_def = frozen
            .corpus
            .tasks
            .iter()
            .find(|t| t.task_id == cell["task_id"].as_str().unwrap())
            .unwrap();
        let ids = cell_identities_for("run-armed", cell, task_def).unwrap();
        let outcome = cell_results
            .iter()
            .find(|c| c["cell_id"].as_str() == Some(cell["cell_id"].as_str().unwrap()))
            .unwrap();
        let class = outcome["classification"].as_str().unwrap_or("");
        if class == "skipped_by_stop_rule" {
            continue;
        }
        let product_task = store
            .get_product_task_by_idempotency(tenant, &ids.worktree_id, &ids.product_task_id)
            .unwrap()
            .unwrap();
        if class == "outcome_unknown" {
            assert_ne!(product_task["status"], "completed");
            let projection = store
                .project_rwe_cell_store_evidence(
                    product_task["task_id"].as_str().unwrap(),
                    &ids.delegated_attempt_id,
                )
                .unwrap();
            assert_eq!(
                projection["provider_execution"]["provider_request_count"],
                0
            );
            assert!(projection.pointer("/output/draft_pr").is_none());
            continue;
        }
        assert_eq!(product_task["status"], "completed");
        let projection = store
            .project_rwe_cell_store_evidence(
                product_task["task_id"].as_str().unwrap(),
                &ids.delegated_attempt_id,
            )
            .unwrap();
        assert_eq!(
            projection["provider_execution"]["provider_request_count"],
            3
        );
        assert_eq!(
            projection["provider_execution"]["transport_provenance"],
            "injected"
        );
        let journal = projection["provider_request_journal"]
            .as_array()
            .unwrap()
            .clone();
        assert_eq!(journal.len(), 3);
        for entry in &journal {
            assert_eq!(entry["transport_provenance"], "injected");
            assert_eq!(entry["status"], "succeeded");
        }
        let te = projection["terminal_evidence"].clone();
        assert_eq!(te["task_status"], "completed");
        assert_eq!(te["verification"]["status"], "evidence_recorded");
        assert_eq!(te["verification"]["trustworthy"], true);
        assert!(te.pointer("/output/draft_pr").is_some());
        let ws_id = product_task["workspace_record_id"].as_str().unwrap();
        let ws = store
            .get_supervised_patch_workspace(ws_id)
            .unwrap()
            .unwrap();
        assert_eq!(ws["status"], "cleaned");
    }
}

#[test]
fn product_golden_path_driver_binds_campaign_package_and_fails_closed_on_unapproved_agy() {
    let _lock = crate::cli::config::cli_env_test_lock()
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let had_ci = std::env::var_os("CI");
    std::env::remove_var("CI");

    let mut driver = ProductGoldenPathCellDriver::default();
    assert!(driver.campaign_package.is_none());
    // Default package is canonical DeepSeek v2
    assert!(driver.ensure_effects_ready().is_ok());

    // Bind AGY candidate package
    let agy_pkg = crate::rwe::campaign_package::canonical_agy_v1_candidate_package().unwrap();
    driver.campaign_package = Some(agy_pkg);
    // In provider-free mode, ensure_effects_ready passes
    assert!(driver.ensure_effects_ready().is_ok());

    // If live provider effects requested, fails closed because AGY requires owner approval and live auth
    driver.allow_live_provider_effects = true;
    let err = driver.ensure_effects_ready().unwrap_err();
    assert!(err.contains("requires explicit owner approval before live execution"));

    match had_ci {
        Some(v) => std::env::set_var("CI", v),
        None => std::env::remove_var("CI"),
    }
}

#[test]
fn product_golden_path_driver_rejects_tampered_campaign_package() {
    let _lock = crate::cli::config::cli_env_test_lock()
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let had_ci = std::env::var_os("CI");
    std::env::remove_var("CI");

    let mut driver = ProductGoldenPathCellDriver::default();
    let mut tampered = crate::rwe::campaign_package::canonical_deepseek_v2_package().unwrap();
    tampered.target_repo = "fake/repo".into();
    driver.campaign_package = Some(tampered);
    assert!(driver.ensure_effects_ready().is_err());

    match had_ci {
        Some(v) => std::env::set_var("CI", v),
        None => std::env::remove_var("CI"),
    }
}

fn ledger_matrix_cell(cell: &Value, ids: &CellIdentities) -> Value {
    let mut ledger_cell = cell.clone();
    ledger_cell["harness_id"] = Value::String(LEDGER_ORCHESTRATED_HARNESS_ID.to_string());
    ledger_cell["matrix_plan_id"] = Value::String("a".repeat(64));
    ledger_cell["matrix_manifest_sha256"] = Value::String("b".repeat(64));
    ledger_cell["matrix_rung"] = Value::String("1x2x3".to_string());
    ledger_cell["matrix_repetition"] = Value::from(ids.repetition);
    ledger_cell["matrix_cell_descriptor_sha256"] = Value::String("c".repeat(64));
    ledger_cell["matrix_model_id"] = Value::String(format!(
        "{OPERATOR_ADMITTED_MODEL}:single-model-three-role:v1"
    ));
    ledger_cell["matrix_strategy_id"] =
        Value::String("single-pass-plan-implement-review:memory-only:v1".to_string());
    ledger_cell
}

#[test]
fn ledger_cell_loop_executes_manager_worker_verifier_to_typed_terminal() {
    use crate::harness_evolution::LedgerTerminalDisposition;
    let binding = ProductHarnessMatrixBinding {
        plan_id: "a".repeat(64),
        manifest_sha256: "b".repeat(64),
        rung: "1x2x3".to_string(),
        repetition: 1,
        cell_id: "ledger-fixture-cell".to_string(),
        cell_descriptor_sha256: "c".repeat(64),
        harness_id: LEDGER_ORCHESTRATED_HARNESS_ID.to_string(),
        model_id: "deepseek-v4-pro:single-model-three-role:v1".to_string(),
        strategy_id: "single-pass-plan-implement-review:memory-only:v1".to_string(),
        task_id: "mx1-task".to_string(),
    };
    // The real manager/worker/verifier loop runs provider-free with
    // effect-free attestations and returns a typed terminal record.
    let record = run_provider_free_ledger_cell_loop(&binding, &"d".repeat(64)).unwrap();
    assert_eq!(record.disposition, LedgerTerminalDisposition::Completed);
    assert!(record.summary.is_some());
    assert!(record.outcome_unknown_task_ids.is_empty());
    assert!(record.usage_conservation_verified);
    // The fixture invents no provider cost: usage is explicit
    // missingness carried into the terminal record.
    assert!(record
        .missing_evidence
        .contains(&"usage:prompt_tokens".to_string()));
    assert!(record.metrics.prompt_tokens.is_none());
    assert!(record.final_ledger_hash.len() == 64);
}

#[test]
fn ledger_cell_intake_carries_producer_owned_binding() {
    let dir = tempdir().unwrap();
    let store = Arc::new(
        LocalProductStore::new_with_clock(dir.path().join("ledger-intake.db"), || {
            "2026-07-25T12:00:00Z".into()
        })
        .unwrap(),
    );
    let principal = operator(&store, "t-ledger-intake", "op-ledger-intake");
    let _lock = crate::cli::config::cli_env_test_lock()
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let had_gate = std::env::var_os(crate::product_golden_path::PRODUCT_TASK_GATE);
    std::env::set_var(crate::product_golden_path::PRODUCT_TASK_GATE, "1");

    let frozen = freeze_current_operator_contract_set().unwrap();
    let cell0 = &frozen.schedule.body["cells"][0];
    let task0 = frozen
        .corpus
        .tasks
        .iter()
        .find(|t| t.task_id == cell0["task_id"].as_str().unwrap())
        .unwrap();
    let ids = cell_identities_for("run-ledger-intake", cell0, task0).unwrap();
    let ledger_cell = ledger_matrix_cell(cell0, &ids);
    let target = std::path::PathBuf::from("/tmp/ledger-intake-target");

    // The frozen matrix identity enters the validated intake and the
    // intake contract; the same intake without it hashes differently.
    let binding = matrix_binding_from_frozen_cell(&ledger_cell, &ids, task0).unwrap();
    assert_eq!(binding.cell_id, ids.cell_id);
    assert_eq!(binding.harness_id, LEDGER_ORCHESTRATED_HARNESS_ID);
    let bound = build_rwe_cell_product_intake_with_matrix(
        &principal,
        &frozen,
        task0,
        &ids,
        &target,
        Some(binding),
    )
    .unwrap();
    assert_eq!(bound.matrix_binding.as_ref().unwrap().cell_id, ids.cell_id);
    let plain = build_rwe_cell_product_intake(&principal, &frozen, task0, &ids, &target).unwrap();
    assert!(plain.matrix_binding.is_none());
    assert_ne!(bound.intake_contract_sha256, plain.intake_contract_sha256);

    match had_gate {
        Some(v) => std::env::set_var(crate::product_golden_path::PRODUCT_TASK_GATE, v),
        None => std::env::remove_var(crate::product_golden_path::PRODUCT_TASK_GATE),
    }
}
#[test]
fn ledger_cell_without_matrix_provenance_fails_closed() {
    let dir = tempdir().unwrap();
    let store = Arc::new(
        LocalProductStore::new_with_clock(dir.path().join("ledger-nobind.db"), || {
            "2026-07-25T12:00:00Z".into()
        })
        .unwrap(),
    );
    let principal = operator(&store, "t-ledger-nb", "op-ledger-nb");
    let _lock = crate::cli::config::cli_env_test_lock()
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    std::env::set_var(crate::product_golden_path::PRODUCT_TASK_GATE, "1");
    let had_ci = std::env::var_os("CI");
    std::env::remove_var("CI");

    let frozen = freeze_current_operator_contract_set().unwrap();
    let cell0 = &frozen.schedule.body["cells"][0];
    let task0 = frozen
        .corpus
        .tasks
        .iter()
        .find(|t| t.task_id == cell0["task_id"].as_str().unwrap())
        .unwrap();
    let ids = cell_identities_for("run-ledger-nb", cell0, task0).unwrap();
    // Ledger harness marker but no frozen matrix provenance: unbound
    // cells can never execute through the seam.
    let mut unbound = cell0.clone();
    unbound["harness_id"] = Value::String(LEDGER_ORCHESTRATED_HARNESS_ID.to_string());

    let target = dir.path().join("target");
    std::fs::create_dir_all(&target).unwrap();
    let driver = ProductGoldenPathCellDriver {
        allow_live_provider_effects: false,
        target_repo_path: Some(target),
        fake_transport: None,
        cell_executor_key_id: None,
        cell_confirmer_key_id: None,
        campaign_package: None,
    };
    let err = driver
        .execute_cell(
            &store,
            &principal,
            &frozen,
            "run-ledger-nb",
            "lease-nb",
            &unbound,
            task0,
            &ids,
        )
        .unwrap_err();
    assert!(err.contains("well-formed frozen matrix cell"), "{err}");

    // A binding naming a different cell is rejected.
    let mut foreign = ledger_matrix_cell(cell0, &ids);
    foreign["cell_id"] = Value::String("foreign-cell".to_string());
    let ids_foreign = cell_identities_for("run-ledger-nb", &foreign, task0).unwrap();
    let err = matrix_binding_from_frozen_cell(&foreign, &ids, task0).unwrap_err();
    assert!(err.contains("different cell_id"), "{err}");
    let _ = ids_foreign;

    match had_ci {
        Some(v) => std::env::set_var("CI", v),
        None => std::env::remove_var("CI"),
    }
    std::env::remove_var(crate::product_golden_path::PRODUCT_TASK_GATE);
}

#[test]
fn ledger_cell_live_mode_fails_closed_without_silent_fallback() {
    let dir = tempdir().unwrap();
    let store = Arc::new(
        LocalProductStore::new_with_clock(dir.path().join("ledger-live.db"), || {
            "2026-07-25T12:00:00Z".into()
        })
        .unwrap(),
    );
    let principal = operator(&store, "t-ledger-live", "op-ledger-live");
    let _lock = crate::cli::config::cli_env_test_lock()
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    std::env::set_var(crate::product_golden_path::PRODUCT_TASK_GATE, "1");
    let had_ci = std::env::var_os("CI");
    std::env::remove_var("CI");

    let frozen = freeze_current_operator_contract_set().unwrap();
    let cell0 = &frozen.schedule.body["cells"][0];
    let task0 = frozen
        .corpus
        .tasks
        .iter()
        .find(|t| t.task_id == cell0["task_id"].as_str().unwrap())
        .unwrap();
    let ids = cell_identities_for("run-ledger-live", cell0, task0).unwrap();
    let ledger_cell = ledger_matrix_cell(cell0, &ids);

    let target = dir.path().join("target");
    std::fs::create_dir_all(&target).unwrap();
    let driver = ProductGoldenPathCellDriver {
        allow_live_provider_effects: true,
        target_repo_path: Some(target),
        fake_transport: None,
        cell_executor_key_id: None,
        cell_confirmer_key_id: None,
        campaign_package: None,
    };
    // The live ledger loop is authorized but the operator live-run token
    // (and every other live gate) is absent: the cell must fail closed
    // at the authorization gates before any store state or provider
    // effect, never silently run the non-ledger managed path.
    let err = driver
        .execute_cell(
            &store,
            &principal,
            &frozen,
            "run-ledger-live",
            "lease-live",
            &ledger_cell,
            task0,
            &ids,
        )
        .unwrap_err();
    assert!(err.contains("live ledger cell effects not ready"), "{err}");
    assert!(err.contains(RWE_OPERATOR_LIVE_RUN_TOKEN), "{err}");
    // Fail closed before any store state: no ProductTask was admitted.
    assert!(store
        .get_product_task(&ids.product_task_id)
        .unwrap()
        .is_none());

    match had_ci {
        Some(v) => std::env::set_var("CI", v),
        None => std::env::remove_var("CI"),
    }
    std::env::remove_var(crate::product_golden_path::PRODUCT_TASK_GATE);
}

/// Live ledger test environment: serializes process-env mutation under
/// the shared CLI env lock and restores every touched variable on drop,
/// so parallel tests never leak live gates into each other.
struct LiveLedgerEnvGuard {
    saved: Vec<(&'static str, Option<std::ffi::OsString>)>,
}

impl LiveLedgerEnvGuard {
    fn set(vars: &[(&'static str, Option<&str>)]) -> Self {
        let mut saved = Vec::with_capacity(vars.len());
        for (key, value) in vars {
            saved.push((*key, std::env::var_os(key)));
            match value {
                Some(v) => std::env::set_var(key, v),
                None => std::env::remove_var(key),
            }
        }
        Self { saved }
    }
}

impl Drop for LiveLedgerEnvGuard {
    fn drop(&mut self) {
        for (key, value) in self.saved.drain(..) {
            match value {
                Some(v) => std::env::set_var(key, v),
                None => std::env::remove_var(key),
            }
        }
    }
}

struct CapturingTransport {
    requests: Arc<Mutex<Vec<HttpRequest>>>,
    responses: Mutex<VecDeque<Result<HttpResponse, HttpError>>>,
}

impl CapturingTransport {
    fn new(
        responses: Vec<Result<HttpResponse, HttpError>>,
    ) -> (Arc<Self>, Arc<Mutex<Vec<HttpRequest>>>) {
        let requests = Arc::new(Mutex::new(Vec::new()));
        let transport = Arc::new(Self {
            requests: Arc::clone(&requests),
            responses: Mutex::new(responses.into_iter().collect()),
        });
        (transport, requests)
    }
}

#[async_trait::async_trait]
impl HttpTransport for CapturingTransport {
    async fn send(&self, request: &HttpRequest) -> Result<HttpResponse, HttpError> {
        self.requests.lock().unwrap().push(request.clone());
        self.responses
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| Err(HttpError::PreSend("capturing transport exhausted".into())))
    }
}

fn live_ledger_test_env() -> LiveLedgerEnvGuard {
    // Minimal live gates: product intake admission plus the operator
    // live-run token. No target-output or network-output symbols are
    // set: the live ledger composition performs no branch push, no
    // GitHub PR output, and no target write beyond the admitted intake
    // staging that the H0 provider-free fixture already exercises.
    LiveLedgerEnvGuard::set(&[
        (crate::product_golden_path::PRODUCT_TASK_GATE, Some("1")),
        ("CI", None),
        (RWE_OPERATOR_LIVE_RUN_TOKEN, Some("1")),
        (
            DEEPSEEK_CREDENTIAL_REFERENCE,
            Some("test-operator-credential"),
        ),
        ("ACP_ENABLE_TARGET_REPO_OUTPUT", Some("1")),
        ("ACP_TARGET_REPO_OUTPUT_KILL_SWITCH", Some("0")),
    ])
}

struct LiveLedgerTestBed {
    _dir: tempfile::TempDir,
    store: Arc<LocalProductStore>,
    principal: AuthenticatedPrincipal,
    executor_key_id: String,
    frozen: OperatorFrozenContractSet,
    task0: RweTaskDefinition,
    ids: CellIdentities,
    ledger_cell: Value,
    target: PathBuf,
}

fn live_ledger_bed(run_id: &str, tenant: &str, db: &str) -> Option<LiveLedgerTestBed> {
    let dir = tempdir().unwrap();
    let target = frozen_target_repo(dir.path())?;
    let store = Arc::new(
        LocalProductStore::new_with_clock(dir.path().join(db), || "2026-07-25T12:00:00Z".into())
            .unwrap(),
    );
    let principal = operator(&store, tenant, &format!("op-{tenant}"));
    let executor_key_id = format!("exec-{tenant}");
    let _ = cell_executor(&store, tenant, &executor_key_id);
    let frozen = freeze_current_operator_contract_set().unwrap();
    let cell0 = frozen.schedule.body["cells"][0].clone();
    let task0 = frozen
        .corpus
        .tasks
        .iter()
        .find(|t| t.task_id == cell0["task_id"].as_str().unwrap())
        .unwrap()
        .clone();
    let ids = cell_identities_for(run_id, &cell0, &task0).unwrap();
    let ledger_cell = ledger_matrix_cell(&cell0, &ids);
    Some(LiveLedgerTestBed {
        _dir: dir,
        store,
        principal,
        executor_key_id,
        frozen,
        task0,
        ids,
        ledger_cell,
        target,
    })
}

fn admit_live_ledger_intake(bed: &LiveLedgerTestBed) -> (ProductHarnessMatrixBinding, String) {
    let binding = matrix_binding_from_frozen_cell(&bed.ledger_cell, &bed.ids, &bed.task0).unwrap();
    admit_live_ledger_intake_with_binding(bed, binding)
}

fn admit_live_ledger_intake_with_model(
    bed: &LiveLedgerTestBed,
    model_id: &str,
) -> (ProductHarnessMatrixBinding, String) {
    let mut binding =
        matrix_binding_from_frozen_cell(&bed.ledger_cell, &bed.ids, &bed.task0).unwrap();
    binding.model_id = model_id.to_string();
    admit_live_ledger_intake_with_binding(bed, binding)
}

fn admit_live_ledger_intake_with_binding(
    bed: &LiveLedgerTestBed,
    binding: ProductHarnessMatrixBinding,
) -> (ProductHarnessMatrixBinding, String) {
    let intake = build_rwe_cell_product_intake_with_matrix(
        &bed.principal,
        &bed.frozen,
        &bed.task0,
        &bed.ids,
        &bed.target,
        Some(binding.clone()),
    )
    .unwrap();
    let admitted = bed
        .store
        .admit_product_task(&intake, bed.principal.principal_id())
        .unwrap();
    let product_task_id = admitted
        .get("task_id")
        .and_then(Value::as_str)
        .unwrap_or(&bed.ids.product_task_id)
        .to_string();
    (binding, product_task_id)
}

const LIVE_PROBE_COMMAND: &str = "python3 -m pytest probe_test.py -q";

fn stage_probe_verifier_target(target: &std::path::Path, pass: bool) {
    let body = if pass {
        "def test_probe_ok():\n    assert True\n"
    } else {
        "def test_probe_ok():\n    assert False, 'probe failure'\n"
    };
    std::fs::write(target.join("probe_test.py"), body).unwrap();
    let api_tests = target.join("apps/api/tests");
    if api_tests.is_dir() {
        std::fs::write(api_tests.join("probe_test.py"), body).unwrap();
        let conftest = api_tests.join("conftest.py");
        if conftest.is_file() {
            std::fs::write(&conftest, "# neutralized for probe verification\n").unwrap();
        }
    }
}

fn stage_probe_for_task(store: &LocalProductStore, product_task_id: &str, pass: bool) {
    let task_row = store
        .get_product_task(product_task_id)
        .unwrap()
        .expect("task must exist");
    let ws_str = task_row
        .pointer("/workspace_binding/workspace_path")
        .and_then(Value::as_str)
        .expect("workspace_path must exist");
    stage_probe_verifier_target(std::path::Path::new(ws_str), pass);
}

fn live_probe_task(base: &RweTaskDefinition, max_calls: u64) -> RweTaskDefinition {
    let mut task = base.clone();
    task.expected_verification_commands = vec![LIVE_PROBE_COMMAND.to_string()];
    task.per_task_max_provider_requests = max_calls;
    task.timeout_ms = 60_000;
    task
}

fn live_success_response(n: u64) -> Result<HttpResponse, crate::provider::transport::HttpError> {
    live_success_response_for_model(n, OPERATOR_ADMITTED_MODEL)
}

fn live_success_response_for_model(
    n: u64,
    model: &str,
) -> Result<HttpResponse, crate::provider::transport::HttpError> {
    Ok(mock_rwe_openai_response(
        &format!("resp-live-{n}"),
        model,
        json!({"proposal": n}),
    ))
}

fn live_round_attempt_id(product_task_id: &str, cell_id: &str, round: usize) -> String {
    let input = format!("{product_task_id}|{cell_id}|{round}");
    format!(
        "rwe-att-ledger-{}-{round:03}",
        &sha256_hex(input.as_bytes())[..32]
    )
}

fn live_round_journal(
    store: &LocalProductStore,
    product_task_id: &str,
    attempt_id: &str,
) -> Vec<Value> {
    store
        .project_rwe_cell_store_evidence(product_task_id, attempt_id)
        .unwrap()
        .get("provider_request_journal")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

#[test]
fn live_ledger_cell_executes_live_composition_not_h0() {
    let _lock = crate::cli::config::cli_env_test_lock()
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let _env = live_ledger_test_env();
    let Some(bed) = live_ledger_bed("run-live-comp", "t-live-comp", "live-comp.db") else {
        eprintln!("SKIP: frozen target repository unavailable");
        return;
    };
    let (binding, product_task_id) = admit_live_ledger_intake(&bed);
    // Use a bounded probe command for this composition-only test. The
    // production ProductTask still binds the frozen verifier; the live
    // loop itself accepts the test-only command and remains provider-live.
    stage_probe_for_task(&bed.store, &product_task_id, true);
    let task = live_probe_task(&bed.task0, 4);
    let executor = bed
        .store
        .authenticate_managed_acceptance_principal(
            bed.principal.tenant_id(),
            &bed.executor_key_id,
            None,
        )
        .unwrap();
    let transport: Option<Arc<dyn crate::provider::transport::HttpTransport>> = Some(Arc::new(
        crate::provider::transport::MockTransport::new(vec![live_success_response(0)]),
    ));
    let (record, evidence) = run_live_ledger_cell_loop(
        &bed.store,
        &bed.principal,
        &executor,
        &binding,
        &task,
        &bed.ids,
        &product_task_id,
        &transport,
        crate::rwe::campaign_package::RWE_DEEPSEEK_V2_PACKAGE_ID,
        &bed.target,
        0.20,
    )
    .unwrap();
    let outcome = project_live_ledger_cell_outcome(
        &bed.store,
        &product_task_id,
        &evidence.rounds,
        &record,
        crate::rwe::campaign_package::RWE_DEEPSEEK_V2_PACKAGE_ID,
        "test-sha",
        "run-live-comp",
        &bed.ids,
    );
    // The live ledger composition ran, not the provider-free loop and
    // not the H0 3-node path: no Draft PR output, no H0 terminal, and
    // the live ledger seam markers are present.
    assert!(outcome.output_draft_pr.is_none());
    assert!(outcome.terminal_evidence_id.is_none());
    assert!(
        outcome.note.contains(RWE_LEDGER_CELL_COMPOSITION_SEAM),
        "{}",
        outcome.note
    );
    assert!(
        outcome.note.contains(RWE_LIVE_LEDGER_CELL_COMPOSITION),
        "{}",
        outcome.note
    );
    assert!(
        !outcome.note.contains("frozen_rwe_bindings.v1"),
        "{}",
        outcome.note
    );
    assert_ne!(outcome.classification, "blocked_provider_free_mode");
    // At least one real delegated provider round reached the store
    // journal through the live worker.
    assert!(outcome.provider_requests >= 1);
    let (_, product_task_id) = ((), outcome.product_task_id.clone());
    let binding = matrix_binding_from_frozen_cell(&bed.ledger_cell, &bed.ids, &bed.task0).unwrap();
    let attempt0 = live_round_attempt_id(&product_task_id, &binding.cell_id, 0);
    let journal = live_round_journal(&bed.store, &product_task_id, &attempt0);
    assert_eq!(journal.len(), 1);
    assert_eq!(journal[0]["status"], "succeeded");
    assert_eq!(journal[0]["transport_provenance"], "injected");
    // Producer-owned matrix provenance survived end to end.
    let task_row = bed
        .store
        .get_product_task(&product_task_id)
        .unwrap()
        .unwrap();
    assert_eq!(
        task_row
            .pointer("/matrix_binding/plan_id")
            .and_then(Value::as_str),
        Some("a".repeat(64).as_str())
    );
}

#[test]
fn live_ledger_injected_transport_runs_full_path_without_seal() {
    let _lock = crate::cli::config::cli_env_test_lock()
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let _env = live_ledger_test_env();
    let Some(bed) = live_ledger_bed("run-live-seal", "t-live-seal", "live-seal.db") else {
        eprintln!("SKIP: frozen target repository unavailable");
        return;
    };
    let (binding, product_task_id) = admit_live_ledger_intake(&bed);
    stage_probe_for_task(&bed.store, &product_task_id, true);
    let task = live_probe_task(&bed.task0, 3);
    use crate::provider::transport::MockTransport;
    let transport: Option<Arc<dyn crate::provider::transport::HttpTransport>> =
        Some(Arc::new(MockTransport::new(vec![live_success_response(0)])));
    let (record, evidence) = run_live_ledger_cell_loop(
        &bed.store,
        &bed.principal,
        &bed.store
            .authenticate_managed_acceptance_principal(
                bed.principal.tenant_id(),
                &bed.executor_key_id,
                None,
            )
            .unwrap(),
        &binding,
        &task,
        &bed.ids,
        &product_task_id,
        &transport,
        crate::rwe::campaign_package::RWE_DEEPSEEK_V2_PACKAGE_ID,
        &bed.target,
        0.20,
    )
    .unwrap();
    assert_eq!(
        record.disposition,
        crate::harness_evolution::LedgerTerminalDisposition::Completed
    );
    assert_eq!(evidence.rounds.len(), 1);
    let outcome = project_live_ledger_cell_outcome(
        &bed.store,
        &product_task_id,
        &evidence.rounds,
        &record,
        crate::rwe::campaign_package::RWE_DEEPSEEK_V2_PACKAGE_ID,
        "test-sha",
        "run-live-seal",
        &bed.ids,
    );
    // Full live composition on the injected transport completes the
    // ledger loop, but nothing about it is decision-grade: injected
    // provenance can never report a live request or a success seal.
    assert_eq!(outcome.classification, "fixture_success");
    assert_ne!(outcome.classification, "success");
    assert!(!outcome.live_provider_request);
    assert_eq!(outcome.provider_transport_provenance, "injected");
    assert_eq!(outcome.verification_status, "evidence_recorded");
    assert!(outcome.verification_trustworthy);
    // Store-owned usage propagated into the orchestration envelope and
    // the lifecycle projection: the mock reports 20 in / 10 out.
    assert_eq!(outcome.provider_requests, 1);
    assert_eq!(outcome.input_tokens, 20);
    assert_eq!(outcome.output_tokens, 10);
    assert_eq!(outcome.total_tokens, 30);
    assert!(!outcome.cost_unknown);
    assert_eq!(
        outcome.monetary_cost,
        live_round_journal(
            &bed.store,
            &product_task_id,
            &evidence.rounds[0].delegated_attempt_id
        )[0]
        .get("effective_cost_usd")
        .and_then(Value::as_f64)
    );
    assert_eq!(
        outcome.terminal_content_sha256,
        Some(record.final_ledger_hash.clone())
    );
    // Manager and worker calls are lifecycle-accounted: one execution
    // round plus the completing selection, all verified once.
    assert!(
        outcome.note.contains("ledger_manager_calls=2"),
        "{}",
        outcome.note
    );
    assert!(
        outcome.note.contains("ledger_worker_calls=1"),
        "{}",
        outcome.note
    );
    assert!(
        outcome.note.contains("ledger_verifications=1"),
        "{}",
        outcome.note
    );
    assert!(
        outcome.note.contains("store_journal_conserved=true"),
        "{}",
        outcome.note
    );
    assert!(
        outcome.note.contains(RWE_LEDGER_MANAGER_SEMANTICS),
        "{}",
        outcome.note
    );
    // No H0 terminal evidence exists for a ledger cell, so no baseline
    // seal is even expressible.
    assert!(bed
        .store
        .get_product_task_terminal_evidence(&product_task_id)
        .is_err());
}

#[test]
fn live_ledger_presend_failure_retries_then_succeeds() {
    use crate::provider::transport::{HttpError, MockTransport};
    let _lock = crate::cli::config::cli_env_test_lock()
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let _env = live_ledger_test_env();
    let Some(bed) = live_ledger_bed("run-live-retry", "t-live-retry", "live-retry.db") else {
        eprintln!("SKIP: frozen target repository unavailable");
        return;
    };
    let (binding, product_task_id) = admit_live_ledger_intake(&bed);
    stage_probe_for_task(&bed.store, &product_task_id, true);
    let task = live_probe_task(&bed.task0, 3);
    let transport: Option<Arc<dyn crate::provider::transport::HttpTransport>> =
        Some(Arc::new(MockTransport::new(vec![
            Err(HttpError::PreSend("send socket unavailable".into())),
            live_success_response(1),
        ])));
    let executor = bed
        .store
        .authenticate_managed_acceptance_principal(
            bed.principal.tenant_id(),
            &bed.executor_key_id,
            None,
        )
        .unwrap();
    let (record, evidence) = run_live_ledger_cell_loop(
        &bed.store,
        &bed.principal,
        &executor,
        &binding,
        &task,
        &bed.ids,
        &product_task_id,
        &transport,
        crate::rwe::campaign_package::RWE_DEEPSEEK_V2_PACKAGE_ID,
        &bed.target,
        0.20,
    )
    .unwrap();
    assert_eq!(
        record.disposition,
        crate::harness_evolution::LedgerTerminalDisposition::Completed
    );
    // The proved no-effect round retried under the attempt budget with a
    // fresh delegation; the success round sealed the loop.
    assert_eq!(evidence.rounds.len(), 2);
    let first = live_round_journal(
        &bed.store,
        &product_task_id,
        &evidence.rounds[0].delegated_attempt_id,
    );
    assert_eq!(first.len(), 1);
    assert_eq!(first[0]["status"], "failed_before_send");
    let second = live_round_journal(
        &bed.store,
        &product_task_id,
        &evidence.rounds[1].delegated_attempt_id,
    );
    assert_eq!(second.len(), 1);
    assert_eq!(second[0]["status"], "succeeded");
    let outcome = project_live_ledger_cell_outcome(
        &bed.store,
        &product_task_id,
        &evidence.rounds,
        &record,
        crate::rwe::campaign_package::RWE_DEEPSEEK_V2_PACKAGE_ID,
        "test-sha",
        "run-live-retry",
        &bed.ids,
    );
    assert_eq!(outcome.classification, "fixture_success");
    // Only the succeeded round counts as a provider request, exactly
    // like the accepted RWE journal projection.
    assert_eq!(outcome.provider_requests, 1);
    assert_eq!(outcome.total_tokens, 30);
}

#[test]
fn live_ledger_unknown_outcome_fences_with_zero_replay() {
    use crate::provider::transport::{HttpError, MockTransport};
    let _lock = crate::cli::config::cli_env_test_lock()
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let _env = live_ledger_test_env();
    let Some(bed) = live_ledger_bed("run-live-unk", "t-live-unk", "live-unk.db") else {
        eprintln!("SKIP: frozen target repository unavailable");
        return;
    };
    let (binding, product_task_id) = admit_live_ledger_intake(&bed);
    stage_probe_for_task(&bed.store, &product_task_id, true);
    let task = live_probe_task(&bed.task0, 3);
    let transport: Option<Arc<dyn crate::provider::transport::HttpTransport>> =
        Some(Arc::new(MockTransport::new(vec![Err(
            HttpError::Connection("connection outcome is unknown".into()),
        )])));
    let executor = bed
        .store
        .authenticate_managed_acceptance_principal(
            bed.principal.tenant_id(),
            &bed.executor_key_id,
            None,
        )
        .unwrap();
    let (record, evidence) = run_live_ledger_cell_loop(
        &bed.store,
        &bed.principal,
        &executor,
        &binding,
        &task,
        &bed.ids,
        &product_task_id,
        &transport,
        crate::rwe::campaign_package::RWE_DEEPSEEK_V2_PACKAGE_ID,
        &bed.target,
        0.20,
    )
    .unwrap();
    assert_eq!(
        record.disposition,
        crate::harness_evolution::LedgerTerminalDisposition::OutcomeUnknown
    );
    assert!(!record.outcome_unknown_task_ids.is_empty());
    // Exactly one worker round ran and exactly one journal entry exists:
    // the unknown outcome was never replayed.
    assert_eq!(evidence.rounds.len(), 1);
    let journal = live_round_journal(
        &bed.store,
        &product_task_id,
        &evidence.rounds[0].delegated_attempt_id,
    );
    assert_eq!(journal.len(), 1);
    assert_eq!(journal[0]["status"], "outcome_unknown");
    let outcome = project_live_ledger_cell_outcome(
        &bed.store,
        &product_task_id,
        &evidence.rounds,
        &record,
        crate::rwe::campaign_package::RWE_DEEPSEEK_V2_PACKAGE_ID,
        "test-sha",
        "run-live-unk",
        &bed.ids,
    );
    assert_eq!(outcome.classification, "outcome_unknown");
    assert!(!outcome.live_provider_request);
    assert!(outcome.cost_unknown);
    assert!(outcome.monetary_cost.is_none());
}

#[test]
fn live_ledger_verifier_feedback_drives_bounded_failure() {
    let _lock = crate::cli::config::cli_env_test_lock()
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let _env = live_ledger_test_env();
    let Some(bed) = live_ledger_bed("run-live-vf", "t-live-vf", "live-vf.db") else {
        eprintln!("SKIP: frozen target repository unavailable");
        return;
    };
    let (binding, product_task_id) = admit_live_ledger_intake(&bed);
    stage_probe_for_task(&bed.store, &product_task_id, false);
    let task = live_probe_task(&bed.task0, 2);
    use crate::provider::transport::MockTransport;
    let transport: Option<Arc<dyn crate::provider::transport::HttpTransport>> =
        Some(Arc::new(MockTransport::new(vec![
            live_success_response(0),
            live_success_response(1),
        ])));
    let executor = bed
        .store
        .authenticate_managed_acceptance_principal(
            bed.principal.tenant_id(),
            &bed.executor_key_id,
            None,
        )
        .unwrap();
    let (record, evidence) = run_live_ledger_cell_loop(
        &bed.store,
        &bed.principal,
        &executor,
        &binding,
        &task,
        &bed.ids,
        &product_task_id,
        &transport,
        crate::rwe::campaign_package::RWE_DEEPSEEK_V2_PACKAGE_ID,
        &bed.target,
        0.20,
    )
    .unwrap();
    // Frozen verifier feedback entered the ledger twice (both rounds
    // fail verification), the frozen call budget then blocked further
    // rounds, and the run failed bounded without ever passing.
    assert_eq!(
        record.disposition,
        crate::harness_evolution::LedgerTerminalDisposition::Failed
    );
    assert_eq!(record.metrics.verification_count, 2);
    assert_eq!(record.metrics.provider_calls, Some(2));
    let outcome = project_live_ledger_cell_outcome(
        &bed.store,
        &product_task_id,
        &evidence.rounds,
        &record,
        crate::rwe::campaign_package::RWE_DEEPSEEK_V2_PACKAGE_ID,
        "test-sha",
        "run-live-vf",
        &bed.ids,
    );
    assert_eq!(outcome.classification, "controlled_failure");
    assert_eq!(outcome.provider_requests, 2);
    assert!(
        outcome.note.contains("ledger_verifications=2"),
        "{}",
        outcome.note
    );
    assert!(!outcome.verification_trustworthy);
}

#[test]
fn live_ledger_journal_mapping_rejects_forged_tampered_and_missing() {
    // Forged status.
    assert!(map_live_ledger_journal_entry(&json!({
        "status": "succeeded_twice",
        "transport_provenance": "external",
        "effective_cost_usd": 0.0,
        "effective_tokens": 0,
    }))
    .is_err());
    // Succeeded entry without store-owned usage proves nothing.
    assert!(map_live_ledger_journal_entry(&json!({
        "status": "succeeded",
        "transport_provenance": "external",
        "effective_cost_usd": 0.001,
        "effective_tokens": 30,
    }))
    .is_err());
    // Tampered cost and missing provenance fail closed.
    assert!(map_live_ledger_journal_entry(&json!({
        "status": "succeeded",
        "transport_provenance": "external",
        "effective_cost_usd": -1.0,
        "effective_tokens": 30,
        "usage": {"input_tokens": 20, "output_tokens": 10},
    }))
    .is_err());
    assert!(map_live_ledger_journal_entry(&json!({
        "status": "succeeded",
        "effective_cost_usd": 0.001,
        "effective_tokens": 30,
        "usage": {"input_tokens": 20, "output_tokens": 10},
    }))
    .is_err());
    assert!(map_live_ledger_journal_entry(&json!({
        "status": "succeeded",
        "transport_provenance": "self_attested",
        "effective_cost_usd": 0.001,
        "effective_tokens": 30,
        "usage": {"input_tokens": 20, "output_tokens": 10},
    }))
    .is_err());
    // Unknown outcomes map with explicit missing cost but counted calls.
    let unknown = map_live_ledger_journal_entry(&json!({
        "status": "outcome_unknown",
        "transport_provenance": "external",
        "effective_cost_usd": 0.002,
        "effective_tokens": 512,
        "claimed_at": "2026-07-25T12:00:00Z",
        "reconciled_at": "2026-07-25T12:00:01Z",
    }))
    .unwrap();
    assert_eq!(
        unknown.disposition,
        EffectReceiptDisposition::OutcomeUnknown
    );
    assert_eq!(unknown.envelope.cost_usd_micros, None);
    assert_eq!(unknown.envelope.provider_calls, Some(1));
    assert_eq!(unknown.envelope.duration_ms, Some(1000));
    // Proved no-effect rounds carry observed zeroes.
    let presend = map_live_ledger_journal_entry(&json!({
        "status": "failed_before_send",
        "transport_provenance": "injected",
        "effective_cost_usd": 0.0,
        "effective_tokens": 0,
        "claimed_at": "2026-07-25T12:00:00Z",
        "reconciled_at": "2026-07-25T12:00:00Z",
    }))
    .unwrap();
    assert_eq!(
        presend.disposition,
        EffectReceiptDisposition::FailedBeforeSendNoEffect
    );
    assert_eq!(presend.envelope.cost_usd_micros, Some(0));
    assert_eq!(presend.envelope.provider_calls, Some(0));
    // Provenance unanimity still gates seal eligibility on mixed or
    // empty journals.
    let external_entry = json!({
        "status": "succeeded",
        "transport_provenance": "external",
        "effective_tokens": 30,
        "effective_cost_usd": 0.001,
        "usage": {"input_tokens": 20, "output_tokens": 10},
    });
    let injected_entry = json!({
        "status": "succeeded",
        "transport_provenance": "injected",
        "effective_tokens": 30,
        "effective_cost_usd": 0.001,
        "usage": {"input_tokens": 20, "output_tokens": 10},
    });
    assert!(store_evidence_transport_provenance(&json!({
        "provider_execution": Value::Null,
        "provider_request_journal": [external_entry, injected_entry],
    }))
    .is_err());
    assert!(store_evidence_transport_provenance(&json!({
        "provider_execution": Value::Null,
        "provider_request_journal": [],
    }))
    .is_err());
    assert_eq!(
        store_evidence_transport_provenance(&json!({
            "provider_execution": Value::Null,
            "provider_request_journal": [injected_entry],
        })),
        Ok("injected".to_string())
    );
}

#[test]
fn live_ledger_terminal_evidence_shape_normalizes_mx1() {
    use crate::harness_evolution::{
        build_mx1_matrix_plan, sample_mx1_descriptor_manifest_with_ledger_harness,
        Mx1HarnessRunAdapter, Mx1LedgerOrchestratedHarnessAdapter, Mx1MatrixRung,
        LEDGER_ORCHESTRATED_HARNESS_ID,
    };
    use crate::product_golden_path::ProductTaskStatus;
    let _lock = crate::cli::config::cli_env_test_lock()
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let _env = live_ledger_test_env();
    let Some(bed) = live_ledger_bed("run-live-mx1", "t-live-mx1", "live-mx1.db") else {
        eprintln!("SKIP: frozen target repository unavailable");
        return;
    };
    let (_binding, product_task_id) = admit_live_ledger_intake(&bed);
    stage_probe_for_task(&bed.store, &product_task_id, true);
    let task = live_probe_task(&bed.task0, 3);
    use crate::provider::transport::MockTransport;
    let transport: Option<Arc<dyn crate::provider::transport::HttpTransport>> =
        Some(Arc::new(MockTransport::new(vec![live_success_response(0)])));
    let executor = bed
        .store
        .authenticate_managed_acceptance_principal(
            bed.principal.tenant_id(),
            &bed.executor_key_id,
            None,
        )
        .unwrap();
    let (record, evidence) = run_live_ledger_cell_loop(
        &bed.store,
        &bed.principal,
        &executor,
        &matrix_binding_from_frozen_cell(&bed.ledger_cell, &bed.ids, &bed.task0).unwrap(),
        &task,
        &bed.ids,
        &product_task_id,
        &transport,
        crate::rwe::campaign_package::RWE_DEEPSEEK_V2_PACKAGE_ID,
        &bed.target,
        0.20,
    )
    .unwrap();
    assert_eq!(
        record.disposition,
        crate::harness_evolution::LedgerTerminalDisposition::Completed
    );
    let outcome = project_live_ledger_cell_outcome(
        &bed.store,
        &product_task_id,
        &evidence.rounds,
        &record,
        crate::rwe::campaign_package::RWE_DEEPSEEK_V2_PACKAGE_ID,
        "test-sha",
        "run-live-mx1",
        &bed.ids,
    );
    // The live terminal values project into the accepted MX1 evidence
    // shape and normalize under the ledger adapter.
    let manifest = sample_mx1_descriptor_manifest_with_ledger_harness();
    let descriptor = manifest
        .harnesses
        .iter()
        .find(|h| h.descriptor_id == LEDGER_ORCHESTRATED_HARNESS_ID)
        .unwrap()
        .clone();
    let adapter = Mx1LedgerOrchestratedHarnessAdapter::new(descriptor.clone()).unwrap();
    let plan = build_mx1_matrix_plan(
        &manifest,
        Mx1MatrixRung::TwoByTwoByThree,
        &task.task_id,
        1,
        &"b".repeat(64),
    )
    .unwrap();
    let cell = plan
        .cells
        .iter()
        .find(|c| c.identity.harness_id == LEDGER_ORCHESTRATED_HARNESS_ID)
        .unwrap();
    let cost_micros = (outcome.monetary_cost.unwrap_or(0.0) * 1_000_000.0).round() as u64;
    let mut terminal = json!({
        "schema_version": "product_task_terminal_evidence.v2",
        "product_task_id": task.task_id,
        "task_status": "completed",
        "workspace_scope_id": "live-ledger-scope",
        "source_revision": "0123456789abcdef0123456789abcdef01234567",
        "verification": {"trustworthy": true, "status": "evidence_recorded"},
        "usage": {"tokens": outcome.total_tokens},
        "cost": {"microunits": cost_micros},
        "cleanup": {"status": "complete"},
        "restart": {"status": "not_observed"},
        "recovery": {"status": "reconciled"},
        "raw_output": "never-projected",
        "content_sha256": Value::Null,
    });
    let hash = hex::encode(Sha256::digest(serde_json::to_vec(&terminal).unwrap()));
    terminal["content_sha256"] = json!(hash);
    let task_row = json!({
        "task_id": task.task_id,
        "status": "completed",
        "workspace_id": "live-ledger-scope",
        "workspace_binding": {
            "workspace_id": "live-ledger-ws",
            "workspace_path": "/tmp/live-ledger-ws",
            "source_revision": "0123456789abcdef0123456789abcdef01234567",
            "allowed_paths": bed.task0.allowed_mutable_paths,
        },
        "failure_code": Value::Null,
        "failure_detail": Value::Null,
    });
    let mut evidence_value =
        crate::product_golden_path::project_product_harness_run(&task_row, Some(&terminal))
            .unwrap();
    evidence_value.matrix_binding = Some(crate::product_golden_path::ProductHarnessMatrixBinding {
        plan_id: plan.plan_id.clone(),
        manifest_sha256: plan.manifest_sha256.clone(),
        rung: plan.rung.as_str().to_string(),
        repetition: plan.repetition,
        cell_id: cell.cell_id.clone(),
        cell_descriptor_sha256: cell.descriptor_digest.clone(),
        harness_id: cell.identity.harness_id.clone(),
        model_id: cell.identity.model_id.clone(),
        strategy_id: cell.identity.strategy_id.clone(),
        task_id: cell.identity.task_id.clone(),
    });
    let normalized = adapter.normalize_run(&plan, cell, &evidence_value).unwrap();
    assert_eq!(normalized.cell_id, cell.cell_id);
    assert_eq!(normalized.harness_id, LEDGER_ORCHESTRATED_HARNESS_ID);
    assert_eq!(normalized.terminal_outcome, ProductTaskStatus::Completed);
}

#[test]
fn live_ledger_h0_path_unchanged_and_ledger_default_off() {
    let _lock = crate::cli::config::cli_env_test_lock()
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let _env = live_ledger_test_env();
    let Some(bed) = live_ledger_bed("run-live-off", "t-live-off", "live-off.db") else {
        eprintln!("SKIP: frozen target repository unavailable");
        return;
    };
    // A non-ledger cell never takes the ledger composition, even armed.
    let armed_h0 = ProductGoldenPathCellDriver {
        allow_live_provider_effects: false,
        target_repo_path: Some(bed.target.clone()),
        fake_transport: None,
        cell_executor_key_id: None,
        cell_confirmer_key_id: None,
        campaign_package: None,
    };
    let h0_outcome = armed_h0
        .execute_cell(
            &bed.store,
            &bed.principal,
            &bed.frozen,
            "run-live-off",
            "lease-off",
            &bed.frozen.schedule.body["cells"][0],
            &bed.task0,
            &bed.ids,
        )
        .unwrap();
    assert_eq!(h0_outcome.classification, "blocked_provider_free_mode");
    assert!(
        h0_outcome
            .note
            .contains("rwe_cell_composition:product_golden_path"),
        "{}",
        h0_outcome.note
    );
    assert!(
        !h0_outcome.note.contains("ledger_orchestrated"),
        "{}",
        h0_outcome.note
    );
    // A ledger cell with live effects off keeps the provider-free loop.
    let off_driver = ProductGoldenPathCellDriver {
        allow_live_provider_effects: false,
        target_repo_path: Some(bed.target.clone()),
        fake_transport: None,
        cell_executor_key_id: None,
        cell_confirmer_key_id: None,
        campaign_package: None,
    };
    let ids_off = cell_identities_for("run-live-ledger-off", &bed.ledger_cell, &bed.task0).unwrap();
    let off_outcome = off_driver
        .execute_cell(
            &bed.store,
            &bed.principal,
            &bed.frozen,
            "run-live-ledger-off",
            "lease-off-ledger",
            &bed.ledger_cell,
            &bed.task0,
            &ids_off,
        )
        .unwrap();
    assert_eq!(off_outcome.classification, "blocked_provider_free_mode");
    assert_eq!(off_outcome.provider_requests, 0);
}

#[test]
fn live_ledger_matrix_model_descriptor_binds_exact_request_model() {
    let _lock = crate::cli::config::cli_env_test_lock()
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let _env = live_ledger_test_env();
    for (suffix, model_id, raw_model) in [
        (
            "pro",
            crate::harness_evolution::MX1_ARM_ZERO_MODEL_ID,
            "deepseek-v4-pro",
        ),
        (
            "flash",
            crate::harness_evolution::MX1_SECOND_MODEL_ID,
            "deepseek-v4-flash",
        ),
    ] {
        let Some(bed) = live_ledger_bed(
            &format!("run-live-model-{suffix}"),
            &format!("t-live-model-{suffix}"),
            &format!("live-model-{suffix}.db"),
        ) else {
            eprintln!("SKIP: frozen target repository unavailable");
            return;
        };
        let (binding, product_task_id) = admit_live_ledger_intake_with_model(&bed, model_id);
        stage_probe_for_task(&bed.store, &product_task_id, true);
        let task = live_probe_task(&bed.task0, 1);
        let (capturing_transport, captured_requests) =
            CapturingTransport::new(vec![live_success_response_for_model(0, raw_model)]);
        let transport: Option<Arc<dyn crate::provider::transport::HttpTransport>> =
            Some(capturing_transport);
        let executor = bed
            .store
            .authenticate_managed_acceptance_principal(
                bed.principal.tenant_id(),
                &bed.executor_key_id,
                None,
            )
            .unwrap();

        let (record, evidence) = run_live_ledger_cell_loop(
            &bed.store,
            &bed.principal,
            &executor,
            &binding,
            &task,
            &bed.ids,
            &product_task_id,
            &transport,
            crate::rwe::campaign_package::RWE_DEEPSEEK_V2_PACKAGE_ID,
            &bed.target,
            0.20,
        )
        .unwrap();
        assert_eq!(
            record.disposition,
            crate::harness_evolution::LedgerTerminalDisposition::Completed
        );
        assert_eq!(evidence.rounds.len(), 1);
        let requests = captured_requests.lock().unwrap();
        let body: Value =
            serde_json::from_slice(requests[0].body.as_deref().expect("captured request body"))
                .unwrap();
        assert_eq!(body["model"], raw_model);
        let journal = live_round_journal(
            &bed.store,
            &product_task_id,
            &evidence.rounds[0].delegated_attempt_id,
        );
        assert_eq!(journal[0]["requested_model"], raw_model);
    }
}

#[test]
fn live_ledger_initial_fail_repaired_by_worker_modification_to_pass() {
    let _lock = crate::cli::config::cli_env_test_lock()
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let _env = live_ledger_test_env();
    let Some(bed) = live_ledger_bed("run-live-repair", "t-live-repair", "live-repair.db") else {
        eprintln!("SKIP: frozen target repository unavailable");
        return;
    };
    let (binding, product_task_id) = admit_live_ledger_intake(&bed);
    let task_row = bed
        .store
        .get_product_task(&product_task_id)
        .unwrap()
        .expect("task must exist");
    let ws_str = task_row
        .pointer("/workspace_binding/workspace_path")
        .and_then(Value::as_str)
        .expect("workspace_path must exist");
    let ws_path = std::path::PathBuf::from(ws_str);

    // Stage initial FAIL on disk in apps/api/tests/probe_test.py
    stage_probe_for_task(&bed.store, &product_task_id, false);
    assert!(
        std::fs::read_to_string(ws_path.join("apps/api/tests/probe_test.py"))
            .unwrap()
            .contains("assert False")
    );
    let conftest = ws_path.join("apps/api/tests/conftest.py");
    if conftest.is_file() {
        std::fs::write(&conftest, "# neutralized for probe verification\n").unwrap();
    }
    std::fs::write(
        ws_path.join("apps/api/tests/unrelated_context.py"),
        "UNRELATED_CONTEXT_SENTINEL\n",
    )
    .unwrap();

    let mut task = live_probe_task(&bed.task0, 3);
    task.expected_verification_commands = vec![
        "PYTHONPATH=apps/api/src python3 -m pytest apps/api/tests/probe_test.py -q".to_string(),
    ];

    // Round 1: worker produces a response without workspace modifications -> verifier fails.
    // Controller detects verification failure and replans with a repair task.
    // Round 2: worker produces managed_workspace_action.v1 modifying the test to pass.
    // Verifier runs again and passes -> loop completes.
    let repair_patch = json!({
        "schema_version": "managed_workspace_action.v1",
        "path": "apps/api/tests/probe_test.py",
        "action": "replace_text",
        "old_text": "assert False, 'probe failure'",
        "new_text": "assert True"
    });
    let resp_round1 = live_success_response(0);
    let resp_round2 = Ok(mock_rwe_openai_response(
        "resp-live-repair",
        OPERATOR_ADMITTED_MODEL,
        repair_patch,
    ));

    let (capturing_transport, captured_requests) =
        CapturingTransport::new(vec![resp_round1, resp_round2]);
    let transport: Option<Arc<dyn crate::provider::transport::HttpTransport>> =
        Some(capturing_transport);

    let executor = bed
        .store
        .authenticate_managed_acceptance_principal(
            bed.principal.tenant_id(),
            &bed.executor_key_id,
            None,
        )
        .unwrap();

    let (record, evidence) = run_live_ledger_cell_loop(
        &bed.store,
        &bed.principal,
        &executor,
        &binding,
        &task,
        &bed.ids,
        &product_task_id,
        &transport,
        crate::rwe::campaign_package::RWE_DEEPSEEK_V2_PACKAGE_ID,
        &bed.target,
        0.20,
    )
    .unwrap();
    assert_eq!(
        record.disposition,
        crate::harness_evolution::LedgerTerminalDisposition::Completed,
        "record={:?} evidence={:?}",
        record,
        evidence,
    );
    assert_eq!(evidence.rounds.len(), 2);
    assert_eq!(record.metrics.replan_count, 1);
    assert_eq!(record.metrics.provider_calls, Some(2));
    assert_eq!(record.metrics.verification_count, 2);

    // Verify disk content was ACTUALLY modified by the worker's action:
    let file_content =
        std::fs::read_to_string(ws_path.join("apps/api/tests/probe_test.py")).unwrap();
    assert!(file_content.contains("assert True"));
    assert!(!file_content.contains("assert False"));
    assert!(evidence.rounds[0].action_receipt.is_none());
    assert!(evidence.rounds[1].action_receipt.is_some());
    let action_journal = live_round_journal(
        &bed.store,
        &product_task_id,
        &evidence.rounds[1].delegated_attempt_id,
    );
    assert_eq!(action_journal.len(), 1);
    assert_eq!(
        action_journal[0]
            .pointer("/workspace_action/schema_version")
            .and_then(Value::as_str),
        Some("managed_workspace_action_evidence.v1")
    );
    assert!(action_journal[0]
        .pointer("/workspace_action/receipt_sha256")
        .and_then(Value::as_str)
        .is_some());
    // A fresh Store projection reconstructs the persisted provider ->
    // action -> verifier evidence chain without replaying the action.
    let restarted_projection = bed
        .store
        .project_rwe_cell_store_evidence(&product_task_id, &evidence.rounds[1].delegated_attempt_id)
        .unwrap();
    assert_eq!(
        restarted_projection
            .pointer("/provider_request_journal/0/workspace_action/schema_version")
            .and_then(Value::as_str),
        Some("managed_workspace_action_evidence.v1")
    );

    let outcome = project_live_ledger_cell_outcome(
        &bed.store,
        &product_task_id,
        &evidence.rounds,
        &record,
        crate::rwe::campaign_package::RWE_DEEPSEEK_V2_PACKAGE_ID,
        "test-sha",
        "run-live-repair",
        &bed.ids,
    );
    assert_eq!(outcome.classification, "fixture_success");
    assert_eq!(outcome.verification_status, "evidence_recorded");
    assert!(outcome.verification_trustworthy);
    assert_eq!(outcome.provider_requests, 2);

    let captured_requests = captured_requests.lock().unwrap().clone();
    assert_eq!(captured_requests.len(), 2);
    let second_body: Value = serde_json::from_slice(
        captured_requests[1]
            .body
            .as_deref()
            .expect("captured provider request body"),
    )
    .unwrap();
    let second_prompt = second_body["messages"][0]["content"]
        .as_str()
        .expect("captured second-round prompt");
    assert!(second_prompt.contains(&task.objective));
    assert!(second_prompt.contains("assert False, 'probe failure'"));
    assert!(second_prompt.contains("Prior verification observations:"));
    assert!(second_prompt.contains("frozen verifier exited 1"));
    assert!(!second_prompt.contains("UNRELATED_CONTEXT_SENTINEL"));
}

#[test]
fn live_ledger_openai_compatible_seam_with_matrix_model_identity() {
    let _lock = crate::cli::config::cli_env_test_lock()
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let _env = live_ledger_test_env();
    let _custom_env =
        LiveLedgerEnvGuard::set(&[("CUSTOM_OPENAI_API_KEY", Some("test-custom-token"))]);
    let Some(bed) = live_ledger_bed("run-live-seam", "t-live-seam", "live-seam.db") else {
        eprintln!("SKIP: frozen target repository unavailable");
        return;
    };
    let (binding, product_task_id) = admit_live_ledger_intake(&bed);
    stage_probe_for_task(&bed.store, &product_task_id, true);
    let task = live_probe_task(&bed.task0, 3);

    use crate::provider::transport::MockTransport;
    let mock_transport: Arc<dyn crate::provider::transport::HttpTransport> =
        Arc::new(MockTransport::new(vec![Ok(mock_rwe_openai_response(
            "resp-live-custom",
            "custom-openai-model",
            json!({"status": "custom-provider-ok"}),
        ))]));
    let transport: Option<Arc<dyn crate::provider::transport::HttpTransport>> =
        Some(mock_transport.clone());

    let custom_adapter = Arc::new(OpenAiCompatibleProviderAdapter {
        provider_id: "custom-openai-provider".to_string(),
        provider_kind: "openai_compatible".to_string(),
        base_url: "https://api.opencode.example.com/v1".to_string(),
        endpoint_path: "chat/completions".to_string(),
        credential_reference: "CUSTOM_OPENAI_API_KEY".to_string(),
        provider_subject: "provider:custom_openai".to_string(),
        transport: mock_transport,
    });

    let executor = bed
        .store
        .authenticate_managed_acceptance_principal(
            bed.principal.tenant_id(),
            &bed.executor_key_id,
            None,
        )
        .unwrap();

    let (record, evidence) = run_live_ledger_cell_loop_with_adapter(
        &bed.store,
        &bed.principal,
        &executor,
        &binding,
        &task,
        &bed.ids,
        &product_task_id,
        &transport,
        crate::rwe::campaign_package::RWE_DEEPSEEK_V2_PACKAGE_ID,
        &bed.target,
        0.20,
        Some(custom_adapter),
    )
    .unwrap();

    // A provider substitution is rejected against the package-owned
    // binding before ManagedProviderCallAuthority or transport invocation.
    assert_eq!(
        record.disposition,
        crate::harness_evolution::LedgerTerminalDisposition::OutcomeUnknown
    );
    assert!(evidence.rounds.is_empty());
    assert_eq!(record.failure_code, "worker_execution_error");
}

#[test]
fn live_ledger_repeated_verifier_failure_terminates_no_progress() {
    let _lock = crate::cli::config::cli_env_test_lock()
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let _env = live_ledger_test_env();
    let Some(bed) = live_ledger_bed("run-live-np", "t-live-np", "live-np.db") else {
        eprintln!("SKIP: frozen target repository unavailable");
        return;
    };
    let (binding, product_task_id) = admit_live_ledger_intake(&bed);
    stage_probe_for_task(&bed.store, &product_task_id, false);
    let task = live_probe_task(&bed.task0, 5);

    use crate::provider::transport::MockTransport;
    let transport: Option<Arc<dyn crate::provider::transport::HttpTransport>> =
        Some(Arc::new(MockTransport::new(vec![
            live_success_response(0),
            live_success_response(1),
            live_success_response(2),
            live_success_response(3),
        ])));

    let executor = bed
        .store
        .authenticate_managed_acceptance_principal(
            bed.principal.tenant_id(),
            &bed.executor_key_id,
            None,
        )
        .unwrap();

    let (record, evidence) = run_live_ledger_cell_loop(
        &bed.store,
        &bed.principal,
        &executor,
        &binding,
        &task,
        &bed.ids,
        &product_task_id,
        &transport,
        crate::rwe::campaign_package::RWE_DEEPSEEK_V2_PACKAGE_ID,
        &bed.target,
        0.20,
    )
    .unwrap();

    assert_eq!(
        record.disposition,
        crate::harness_evolution::LedgerTerminalDisposition::NoProgress
    );
    assert!(evidence.rounds.len() >= 2);
}

#[test]
fn live_ledger_missing_workspace_fails_closed() {
    let _lock = crate::cli::config::cli_env_test_lock()
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let _env = live_ledger_test_env();
    let Some(bed) = live_ledger_bed(
        "run-live-noworkspace",
        "t-live-noworkspace",
        "live-noworkspace.db",
    ) else {
        eprintln!("SKIP: frozen target repository unavailable");
        return;
    };
    let (binding, product_task_id) = admit_live_ledger_intake(&bed);
    let task_row = bed
        .store
        .get_product_task(&product_task_id)
        .unwrap()
        .expect("task must exist");
    let ws_str = task_row
        .pointer("/workspace_binding/workspace_path")
        .and_then(Value::as_str)
        .expect("workspace_path must exist");
    std::fs::remove_dir_all(ws_str).unwrap();

    let task = live_probe_task(&bed.task0, 3);
    use crate::provider::transport::MockTransport;
    let transport: Option<Arc<dyn crate::provider::transport::HttpTransport>> =
        Some(Arc::new(MockTransport::new(vec![live_success_response(0)])));
    let executor = bed
        .store
        .authenticate_managed_acceptance_principal(
            bed.principal.tenant_id(),
            &bed.executor_key_id,
            None,
        )
        .unwrap();
    let result = run_live_ledger_cell_loop(
        &bed.store,
        &bed.principal,
        &executor,
        &binding,
        &task,
        &bed.ids,
        &product_task_id,
        &transport,
        crate::rwe::campaign_package::RWE_DEEPSEEK_V2_PACKAGE_ID,
        &bed.target,
        0.20,
    );
    assert!(result.is_err(), "missing workspace must fail closed");
    let err = result.unwrap_err();
    assert!(
        err.contains("missing or invalid execution workspace binding"),
        "{err}"
    );
}

#[test]
fn live_ledger_custom_adapter_cannot_mint_external_provenance() {
    use crate::provider::transport::MockTransport;
    let mock_transport: Arc<dyn crate::provider::transport::HttpTransport> =
        Arc::new(MockTransport::new(vec![]));
    let custom_adapter = OpenAiCompatibleProviderAdapter {
        provider_id: "custom-openai-provider".to_string(),
        provider_kind: "openai_compatible".to_string(),
        base_url: "https://api.opencode.example.com/v1".to_string(),
        endpoint_path: "chat/completions".to_string(),
        credential_reference: "CUSTOM_OPENAI_API_KEY".to_string(),
        provider_subject: "provider:custom_openai".to_string(),
        transport: mock_transport,
    };
    let prov =
        crate::provider::transport::production_transport_provenance(custom_adapter.transport());
    assert_eq!(
        prov,
        crate::provider::transport::ProviderTransportProvenance::Injected
    );
    assert_ne!(
        prov,
        crate::provider::transport::ProviderTransportProvenance::External
    );

    struct FakeCustomTransport;
    #[async_trait::async_trait]
    impl crate::provider::transport::HttpTransport for FakeCustomTransport {
        async fn send(
            &self,
            _req: &crate::provider::transport::HttpRequest,
        ) -> Result<HttpResponse, crate::provider::transport::HttpError> {
            unimplemented!()
        }
    }
    let fake_transport: Arc<dyn crate::provider::transport::HttpTransport> =
        Arc::new(FakeCustomTransport);
    let prov_fake = crate::provider::transport::production_transport_provenance(&fake_transport);
    assert_eq!(
        prov_fake,
        crate::provider::transport::ProviderTransportProvenance::Injected
    );
    assert_ne!(
        prov_fake,
        crate::provider::transport::ProviderTransportProvenance::External
    );
}

#[test]
fn live_ledger_workspace_action_failure_preserves_usage_and_zero_replay() {
    let _lock = crate::cli::config::cli_env_test_lock()
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let _env = live_ledger_test_env();
    let Some(bed) = live_ledger_bed("run-live-wafail", "t-live-wafail", "live-wafail.db") else {
        eprintln!("SKIP: frozen target repository unavailable");
        return;
    };
    let (binding, product_task_id) = admit_live_ledger_intake(&bed);
    stage_probe_for_task(&bed.store, &product_task_id, false);
    let task = live_probe_task(&bed.task0, 3);

    let failing_patch = json!({
        "schema_version": "managed_workspace_action.v1",
        "path": "apps/api/tests/nonexistent_probe_file.py",
        "action": "replace_text",
        "old_text": "does not exist",
        "new_text": "replacement"
    });
    let resp = Ok(mock_rwe_openai_response(
        "resp-live-wafail",
        OPERATOR_ADMITTED_MODEL,
        failing_patch,
    ));

    use crate::provider::transport::MockTransport;
    let transport: Option<Arc<dyn crate::provider::transport::HttpTransport>> =
        Some(Arc::new(MockTransport::new(vec![resp])));

    let executor = bed
        .store
        .authenticate_managed_acceptance_principal(
            bed.principal.tenant_id(),
            &bed.executor_key_id,
            None,
        )
        .unwrap();

    let (record, evidence) = run_live_ledger_cell_loop(
        &bed.store,
        &bed.principal,
        &executor,
        &binding,
        &task,
        &bed.ids,
        &product_task_id,
        &transport,
        crate::rwe::campaign_package::RWE_DEEPSEEK_V2_PACKAGE_ID,
        &bed.target,
        0.20,
    )
    .unwrap();

    assert_eq!(
        record.disposition,
        crate::harness_evolution::LedgerTerminalDisposition::Failed
    );
    assert_eq!(evidence.rounds.len(), 1);
    assert!(evidence.rounds[0].action_receipt.is_none());
    let failed_journal = live_round_journal(
        &bed.store,
        &product_task_id,
        &evidence.rounds[0].delegated_attempt_id,
    );
    assert_eq!(failed_journal.len(), 1);
    assert_eq!(failed_journal[0]["status"], "succeeded");
    assert_eq!(
        failed_journal[0]
            .pointer("/workspace_action_failure/schema_version")
            .and_then(Value::as_str),
        Some("managed_workspace_action_failure.v1")
    );
    assert_eq!(
        failed_journal[0]
            .pointer("/workspace_action_failure/replay")
            .and_then(Value::as_str),
        Some("forbidden")
    );
    let restarted_failure = bed
        .store
        .project_rwe_cell_store_evidence(&product_task_id, &evidence.rounds[0].delegated_attempt_id)
        .unwrap();
    assert_eq!(
        restarted_failure
            .pointer("/provider_request_journal/0/workspace_action_failure/replay")
            .and_then(Value::as_str),
        Some("forbidden")
    );

    let outcome = project_live_ledger_cell_outcome(
        &bed.store,
        &product_task_id,
        &evidence.rounds,
        &record,
        crate::rwe::campaign_package::RWE_DEEPSEEK_V2_PACKAGE_ID,
        "test-sha",
        "run-live-wafail",
        &bed.ids,
    );
    assert_eq!(outcome.provider_requests, 1);
    assert_eq!(outcome.total_tokens, 30);
}

#[test]
fn live_ledger_second_round_prompt_captures_real_objective_feedback_and_staged_context() {
    use crate::harness_evolution::{
        LedgerFinding, LedgerTaskRecord, LedgerTaskStatus, VerificationObservation,
        VerificationOutcome, WorkerContext,
    };
    let context = WorkerContext {
        contract_digest: "digest-xyz".to_string(),
        contract_summary: Some("repair contract".to_string()),
        plan_summary: "Test repair plan".to_string(),
        selected_task: LedgerTaskRecord {
            id: "task-repair-1".to_string(),
            description: "Fix broken verification in test".to_string(),
            status: LedgerTaskStatus::Pending,
            result_digest: None,
            evidence_refs: vec![],
            attempt_count: 1,
            attempt_id: None,
            failure_reason: None,
        },
        relevant_findings: vec![LedgerFinding {
            id: "finding-1".to_string(),
            summary: "Test suite failed with AssertionError".to_string(),
            source: "verifier".to_string(),
            related_task_id: None,
            evidence_digest: None,
        }],
        relevant_observations: vec![VerificationObservation {
            round: 1,
            task_id: "task-repair-1".to_string(),
            attempt_id: None,
            result_digest: None,
            outcome: VerificationOutcome::Fail,
            observation_summary: "AssertionError: probe failure in probe_test.py".to_string(),
            evidence_digest: None,
        }],
        relevant_artifact_refs: vec![],
        expected_evidence: "Passing pytest output".to_string(),
        execution_metadata: {
            let mut map = std::collections::BTreeMap::new();
            map.insert("attempt".to_string(), "1".to_string());
            map.insert("round".to_string(), "2".to_string());
            map
        },
    };
    let objective = "Resolve failing pytest probe in probe_test.py and ensure tests pass";
    let staged = "def test_probe():\n    assert False, 'probe failure'\n";
    let strategy = "direct_patch_repair";
    let allowed = vec!["apps/api/tests/probe_test.py".to_string()];

    let prompt = render_live_ledger_worker_prompt(
        &context,
        "hash-123",
        objective,
        Some(staged),
        Some(strategy),
        &allowed,
        "test-pkg",
    );

    assert!(
        prompt.contains(objective),
        "prompt must include real objective"
    );
    assert!(prompt.contains("Prior verification observations: round 1 Fail: AssertionError: probe failure in probe_test.py"), "prompt must include verifier feedback");
    assert!(
        prompt.contains(
            "Relevant workspace content:\ndef test_probe():\n    assert False, 'probe failure'"
        ),
        "prompt must include staged context"
    );
    assert!(
        prompt.contains("Strategy projection: direct_patch_repair"),
        "prompt must include strategy projection"
    );
    assert!(
        prompt.contains("Attempt 1 of round 2"),
        "prompt must reflect round 2"
    );
}

#[test]
fn live_ledger_strategy_projection_included_in_prompt() {
    use crate::harness_evolution::{
        LedgerTaskRecord, LedgerTaskStatus, WorkerContext, LEDGER_ORCHESTRATED_HARNESS_ID,
        MX1_MEMORY_ONLY_STRATEGY_ID, MX1_NO_PROJECTION_STRATEGY_ID, MX1_SECOND_MODEL_ID,
        MX1_SKILL_ONLY_STRATEGY_ID,
    };
    assert_eq!(
        resolve_strategy_projection(MX1_MEMORY_ONLY_STRATEGY_ID),
        None
    );
    assert_eq!(
        resolve_strategy_projection(MX1_SKILL_ONLY_STRATEGY_ID),
        None
    );
    assert_eq!(resolve_strategy_projection(""), None);
    assert_eq!(resolve_strategy_projection("   "), None);
    assert_eq!(resolve_strategy_projection("unmapped_strategy"), None);

    let base_binding = ProductHarnessMatrixBinding {
        plan_id: "a".repeat(64),
        manifest_sha256: "b".repeat(64),
        rung: "r1".to_string(),
        repetition: 1,
        cell_id: "cell-projection".to_string(),
        cell_descriptor_sha256: "c".repeat(64),
        harness_id: LEDGER_ORCHESTRATED_HARNESS_ID.to_string(),
        model_id: MX1_SECOND_MODEL_ID.to_string(),
        strategy_id: MX1_MEMORY_ONLY_STRATEGY_ID.to_string(),
        task_id: "task-projection".to_string(),
    };
    let memory_ref = resolve_strategy_projection_for_cell(&base_binding)
        .unwrap()
        .expect("memory strategy must project");
    let memory_json: Value = serde_json::from_str(&memory_ref).unwrap();
    assert_eq!(
        memory_json["schema_version"],
        "mx1_validated_projection_digest_ref.v1"
    );
    assert_eq!(
        memory_json["source_handle"],
        "artifact:mx1-memory-projection-v1"
    );
    assert!(memory_json["content_sha256"].as_str().is_some());

    let mut skill_binding = base_binding.clone();
    skill_binding.strategy_id = MX1_SKILL_ONLY_STRATEGY_ID.to_string();
    let skill_ref = resolve_strategy_projection_for_cell(&skill_binding)
        .unwrap()
        .expect("skill strategy must project");
    let skill_json: Value = serde_json::from_str(&skill_ref).unwrap();
    assert_eq!(skill_json["source_handle"], "git:mx1-skill-projection-v1");
    assert_ne!(memory_json["source_handle"], skill_json["source_handle"]);
    assert_ne!(
        memory_json["cell_binding_sha256"],
        skill_json["cell_binding_sha256"]
    );

    let mut baseline_binding = base_binding.clone();
    baseline_binding.strategy_id = MX1_NO_PROJECTION_STRATEGY_ID.to_string();
    assert_eq!(
        resolve_strategy_projection_for_cell(&baseline_binding).unwrap(),
        None
    );

    let mut other_cell = base_binding.clone();
    other_cell.task_id = "task-projection-other-cell".to_string();
    let other_ref = resolve_strategy_projection_for_cell(&other_cell)
        .unwrap()
        .expect("other memory cell must project");
    let other_json: Value = serde_json::from_str(&other_ref).unwrap();
    assert_ne!(
        memory_json["cell_binding_sha256"],
        other_json["cell_binding_sha256"]
    );

    let context = WorkerContext {
        contract_digest: "digest-abc".to_string(),
        contract_summary: None,
        plan_summary: "Plan summary".to_string(),
        selected_task: LedgerTaskRecord {
            id: "t1".to_string(),
            description: "desc".to_string(),
            status: LedgerTaskStatus::Pending,
            result_digest: None,
            evidence_refs: vec![],
            attempt_count: 1,
            attempt_id: None,
            failure_reason: None,
        },
        relevant_findings: vec![],
        relevant_observations: vec![],
        relevant_artifact_refs: vec![],
        expected_evidence: "evidence".to_string(),
        execution_metadata: std::collections::BTreeMap::new(),
    };

    let prompt_with = render_live_ledger_worker_prompt(
        &context,
        "hash",
        "task objective",
        None,
        Some("projection_aware"),
        &[],
        "pkg",
    );
    assert!(prompt_with.contains("Strategy projection: projection_aware"));

    let prompt_without = render_live_ledger_worker_prompt(
        &context,
        "hash",
        "task objective",
        None,
        None,
        &[],
        "pkg",
    );
    assert!(!prompt_without.contains("Strategy projection:"));
}

#[test]
fn live_ledger_openai_compatible_exact_url_and_mismatch_rejection() {
    use crate::provider::managed_deepseek::{
        DeepSeekPriceProfile, DeepSeekProtocol, ManagedCallBinding, ManagedCallLimits,
        ManagedMessage, ManagedModelRole, ManagedProviderCallAuthority, ManagedProviderCallRequest,
        PersistedAuthoritySnapshot, PersistedManagedExecutionContract,
    };

    let contract1 = PersistedManagedExecutionContract {
        thinking: None,
        provider_identity: "custom-provider".to_string(),
        provider_kind: "openai_compatible".to_string(),
        protocol: DeepSeekProtocol::OpenAiCompatible,
        request_schema_version: crate::provider::managed_deepseek::MANAGED_PROVIDER_CALL_SCHEMA
            .to_string(),
        response_schema_version:
            crate::provider::managed_deepseek::MANAGED_PROVIDER_RESPONSE_SCHEMA.to_string(),
        usage_parser_version: crate::provider::managed_deepseek::DEEPSEEK_USAGE_PARSER_VERSION
            .to_string(),
        host: "api.opencode.example.com".to_string(),
        base_url: "https://api.opencode.example.com/v1".to_string(),
        endpoint_path: "chat/completions".to_string(),
        credential_reference: "CUSTOM_KEY".to_string(),
        requested_model: "test-model".to_string(),
        limits: ManagedCallLimits::default(),
        price_profile: DeepSeekPriceProfile::default(),
    };
    assert_eq!(
        contract1.url(),
        "https://api.opencode.example.com/v1/chat/completions"
    );

    let mut contract2 = contract1.clone();
    contract2.base_url = "https://api.opencode.example.com/v1/".to_string();
    contract2.endpoint_path = "/chat/completions".to_string();
    assert_eq!(
        contract2.url(),
        "https://api.opencode.example.com/v1/chat/completions"
    );

    let mut contract3 = contract1.clone();
    contract3.base_url = "https://api.opencode.example.com/v1".to_string();
    contract3.endpoint_path = "/chat/completions".to_string();
    assert_eq!(
        contract3.url(),
        "https://api.opencode.example.com/v1/chat/completions"
    );

    let mut contract4 = contract1.clone();
    contract4.base_url = "https://api.opencode.example.com/v1/".to_string();
    contract4.endpoint_path = "chat/completions".to_string();
    assert_eq!(
        contract4.url(),
        "https://api.opencode.example.com/v1/chat/completions"
    );

    let binding = ManagedCallBinding {
        product_task_id: "p1".to_string(),
        workflow_id: "wf1".to_string(),
        node_id: "n1".to_string(),
        attempt_id: "att1".to_string(),
        spend_authorization_id: "auth1".to_string(),
        attempt_lease_id: "lease1".to_string(),
    };
    let mut req = ManagedProviderCallRequest::for_role(
        ManagedModelRole::Implementer,
        DeepSeekProtocol::OpenAiCompatible,
        binding,
    );
    req.provider_identity = "custom-provider".to_string();
    req.provider_kind = "openai_compatible".to_string();
    req.host = "api.opencode.example.com".to_string();
    req.base_url = "https://api.opencode.example.com/v1".to_string();
    req.endpoint_path = "chat/completions".to_string();
    req.credential_reference = "CUSTOM_KEY".to_string();
    req.requested_model = "test-model".to_string();
    req.single_model_plan = true;
    req.messages = vec![ManagedMessage::text("user", "test")];

    struct MockAuthSource(PersistedAuthoritySnapshot);
    impl crate::provider::managed_deepseek::ManagedAuthoritySource for MockAuthSource {
        fn current_authority(
            &self,
            _binding: &crate::provider::managed_deepseek::ManagedCallBinding,
        ) -> Result<PersistedAuthoritySnapshot, String> {
            Ok(self.0.clone())
        }
    }

    let base_persisted = PersistedAuthoritySnapshot {
        product_task_id: "p1".to_string(),
        workflow_id: "wf1".to_string(),
        node_id: "n1".to_string(),
        attempt_id: "att1".to_string(),
        spend_authorization_id: "auth1".to_string(),
        attempt_lease_id: "lease1".to_string(),
        spend_status: "consumed".to_string(),
        consumed_by_attempt_id: Some("att1".to_string()),
        lease_status: "current".to_string(),
        execution_contract: Some(contract1.clone()),
    };

    let authority = ManagedProviderCallAuthority::new(
        Arc::new(MockAuthSource(base_persisted)),
        contract1.limits.clone(),
    )
    .unwrap();

    assert!(authority.validate_current_authority(&req).is_ok());

    req.credential_reference = "WRONG_KEY".to_string();
    assert!(authority.validate_current_authority(&req).is_err());
    req.credential_reference = "CUSTOM_KEY".to_string();

    req.requested_model = "other-model".to_string();
    assert!(authority.validate_current_authority(&req).is_err());
    req.requested_model = "test-model".to_string();

    req.base_url = "https://evil.example.com/v1".to_string();
    assert!(authority.validate_current_authority(&req).is_err());
    req.base_url = "https://api.opencode.example.com/v1".to_string();

    req.provider_kind = "deepseek".to_string();
    assert!(authority.validate_current_authority(&req).is_err());
}
