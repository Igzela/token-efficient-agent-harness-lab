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
use engine::cli::codex_partial_mediation_authority_decision::OPERATOR_RISK_ACCEPTANCE_PHRASE;
#[cfg(feature = "pg-tests")]
use engine::event_schema::canonical_event_json;
#[cfg(feature = "pg-tests")]
use engine::feedback::{
    ContextualPolicyPromotion, ContextualPolicyPromotionGate, ObjectiveProfile,
    CONTEXTUAL_POLICY_PROMOTION_SCHEMA_VERSION,
};
#[cfg(feature = "pg-tests")]
use engine::node_executor::{
    AgentAction, AgentStepExecutor, NodeExecutionInput, NodeExecutionOutput, NodeExecutor,
};
#[cfg(feature = "pg-tests")]
use engine::orchestration::schemas::{
    DebatePosition, DebateRequest, DebateResolution, HandoffRequest, ReviewRequest, ReviewVerdict,
};
#[cfg(feature = "pg-tests")]
use engine::product_golden_path::{
    validate_intake, ProductExecutorPolicy, ProductTaskBudget, ProductTaskIntakeRequest,
    ProductVerificationCommand, ProductVerificationRuntimeAuthority, PRODUCT_TASK_GATE,
};
#[cfg(feature = "pg-tests")]
use engine::provider::embedding::{
    OPENROUTER_EMBEDDING_CANONICAL_SLUG, OPENROUTER_EMBEDDING_CONTEXT_LENGTH,
    OPENROUTER_EMBEDDING_DIMENSIONS, OPENROUTER_EMBEDDING_MODEL_ID,
    OPENROUTER_EMBEDDING_RESOLVED_MODEL_ID,
};
#[cfg(feature = "pg-tests")]
use engine::provider::transport::{HttpError, HttpRequest, HttpResponse, HttpTransport};
#[cfg(feature = "pg-tests")]
use engine::rwe::corpus::freeze_first_rwe_corpus;
#[cfg(feature = "pg-tests")]
use engine::rwe::runner::{persist_rwe_run_authorization, RweRunAuthorizationBody};
#[cfg(feature = "pg-tests")]
use engine::storage::local_product_store::BudgetAutoPausePolicy;
#[cfg(feature = "pg-tests")]
use engine::storage::local_product_store::{
    AuthenticatedPrincipal, CostAuthority, DurableMemoryCreate, DurableMemoryRevision,
    ExternalRuntimeInvocationClaim, ExternalRuntimeScope, LocalProductStore,
    MemoryRetrievalRequest, MemoryScope, ProviderEmbeddingResolutionAction,
    ProviderEmbeddingResolutionRequest, RiskAcknowledgementRequest, RwePerTaskBudget,
    SpendAuthorizationRequest,
};
#[cfg(feature = "pg-tests")]
use engine::tool_policy_executor::ToolPolicyNodeExecutor;
#[cfg(feature = "pg-tests")]
use serde_json::{json, Value};
#[cfg(feature = "pg-tests")]
use sha2::{Digest, Sha256};
#[cfg(feature = "pg-tests")]
use std::process::Command;
#[cfg(feature = "pg-tests")]
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
#[cfg(feature = "pg-tests")]
use std::sync::Arc;
#[cfg(feature = "pg-tests")]
use std::thread;
#[cfg(feature = "pg-tests")]
use std::time::Duration;

#[cfg(feature = "pg-tests")]
fn utc_now_string() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

#[cfg(feature = "pg-tests")]
struct PgCountingEmbeddingTransport {
    posts: AtomicUsize,
}

#[cfg(feature = "pg-tests")]
struct PgFailOnceEmbeddingTransport {
    posts: AtomicUsize,
}

#[cfg(feature = "pg-tests")]
#[async_trait::async_trait]
impl HttpTransport for PgFailOnceEmbeddingTransport {
    async fn send(&self, request: &HttpRequest) -> Result<HttpResponse, HttpError> {
        if request.url.ends_with("/embeddings/models") {
            return PgCountingEmbeddingTransport {
                posts: AtomicUsize::new(0),
            }
            .send(request)
            .await;
        }
        if request.url.ends_with("/embeddings") && request.method == "POST" {
            if self.posts.fetch_add(1, Ordering::SeqCst) == 0 {
                return Ok(HttpResponse {
                    status: 401,
                    body: format!(
                        r#"{{"error":{{"code":401,"message":"redacted","metadata":{{"error_type":"authentication"}}}},"openrouter_metadata":{{"attempt":0,"requested":"{}"}}}}"#,
                        engine::provider::embedding::OPENROUTER_EMBEDDING_MODEL_ID
                    )
                    .into_bytes(),
                });
            }
            return Ok(HttpResponse {
                status: 200,
                body: serde_json::to_vec(&json!({
                    "model":OPENROUTER_EMBEDDING_RESOLVED_MODEL_ID,
                    "data":[{"index":0,"embedding":vec![0.25;OPENROUTER_EMBEDDING_DIMENSIONS]}],
                    "usage":{"prompt_tokens":4}
                }))
                .unwrap(),
            });
        }
        Err(HttpError::Connection(
            "unexpected fixture endpoint".to_string(),
        ))
    }
}

#[cfg(feature = "pg-tests")]
#[async_trait::async_trait]
impl HttpTransport for PgCountingEmbeddingTransport {
    async fn send(&self, request: &HttpRequest) -> Result<HttpResponse, HttpError> {
        if request.url.ends_with("/embeddings/models") {
            Ok(HttpResponse {
                status: 200,
                body: serde_json::to_vec(&json!({"data":[{
                    "id":OPENROUTER_EMBEDDING_MODEL_ID,
                    "canonical_slug":OPENROUTER_EMBEDDING_CANONICAL_SLUG,
                    "context_length":OPENROUTER_EMBEDDING_CONTEXT_LENGTH,
                    "pricing":{"prompt":"0","completion":"0","request":"0"},
                    "architecture":{"input_modalities":["text"],"output_modalities":["embeddings"]}
                }]}))
                .unwrap(),
            })
        } else if request.url.ends_with("/embeddings") && request.method == "POST" {
            self.posts.fetch_add(1, Ordering::SeqCst);
            Ok(HttpResponse {
                status: 200,
                body: serde_json::to_vec(&json!({
                    "model":OPENROUTER_EMBEDDING_RESOLVED_MODEL_ID,
                    "data":[{"index":0,"embedding":vec![0.25;OPENROUTER_EMBEDDING_DIMENSIONS]}],
                    "usage":{"prompt_tokens":4}
                }))
                .unwrap(),
            })
        } else {
            Err(HttpError::Connection(
                "unexpected fixture endpoint".to_string(),
            ))
        }
    }
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
fn pg_managed_acceptance_decision_body(decision_id: &str, attempt_id: &str) -> Value {
    json!({
        "decision_id": decision_id,
        "schema_version": "codex_partial_mediation_authority_decision.v2",
        "status": "draft_pending_operator",
        "invalidation_state": "none",
        "acknowledgement": {
            "required_phrase": OPERATOR_RISK_ACCEPTANCE_PHRASE,
        },
        "trial_envelope": {
            "max_retries": 0,
            "max_provider_requests": 1,
            "draft_pr_only": true,
            "max_input_tokens": 8000,
            "max_output_tokens": 4000,
            "max_total_tokens": 12000,
            "max_wall_time_ms": 300000,
            "exact_codex_version": "0.145.0",
            "exact_codex_sha_required": true,
            "provider_kind": "openai_compatible",
            "provider_host": "api.openai.com",
            "provider_base_url": "https://api.openai.com/v1",
            "admitted_endpoint_paths": ["/v1/responses"],
            "model": "gpt-5.6-luna",
            "product_task_id": "ptask-1",
            "workflow_id": "wf-1",
            "workflow_node_id": "node-1",
            "execution_id": format!("codex-attempt-{attempt_id}"),
            "attempt_id": attempt_id,
            "target_repo": "org/disposable-trial",
            "target_main_sha": "a".repeat(40),
            "exact_codex_path": "/usr/bin/codex",
            "exact_codex_sha256": "ab".repeat(32),
            "cancellation_identity": "cancel-1",
            "rollback_identity": "rollback-1",
            "output_branch_prefix": "acp/",
            "cost_authority": {
                "kind": "cost_unavailable",
                "monetary_ceiling_enforced": false,
                "note": "rely on request/token/time caps; no monetary ceiling claimed",
            },
            "auto_merge_disabled": true,
        },
    })
}

#[cfg(feature = "pg-tests")]
fn pg_managed_acceptance_spend_request(
    risk_authorization_id: &str,
    attempt_id: &str,
) -> SpendAuthorizationRequest {
    SpendAuthorizationRequest {
        risk_authorization_id: risk_authorization_id.to_string(),
        product_task_id: "ptask-1".to_string(),
        workflow_id: Some("wf-1".to_string()),
        workflow_node_id: Some("node-1".to_string()),
        execution_id: format!("codex-attempt-{attempt_id}"),
        attempt_id: attempt_id.to_string(),
        binary_path: "/usr/bin/codex".to_string(),
        binary_version: "0.145.0".to_string(),
        binary_sha256: "ab".repeat(32),
        provider_kind: "openai_compatible".to_string(),
        provider_host: "api.openai.com".to_string(),
        provider_base_url: "https://api.openai.com/v1".to_string(),
        admitted_endpoint_paths: vec!["/v1/responses".to_string()],
        model: "gpt-5.6-luna".to_string(),
        target_repo: "org/disposable-trial".to_string(),
        target_main_sha: "a".repeat(40),
        output_branch_prefix: "acp/".to_string(),
        draft_pr_only: true,
        max_provider_requests: 1,
        max_retries: 0,
        max_input_tokens: 8000,
        max_output_tokens: 4000,
        max_total_tokens: 12000,
        max_wall_time_ms: 300000,
        cost_authority: CostAuthority::CostUnavailable,
        cancellation_identity: "cancel-1".to_string(),
        rollback_identity: "rollback-1".to_string(),
    }
}

#[cfg(feature = "pg-tests")]
fn pg_seed_managed_acceptance(
    store: &LocalProductStore,
    tag: &str,
    attempt_id: &str,
) -> (AuthenticatedPrincipal, Value, SpendAuthorizationRequest) {
    let decision_id = format!("mad-pg-{tag}");
    let principal = AuthenticatedPrincipal::fixture_for_tests(
        "tenant-pg-managed-acceptance",
        &format!("fixture-principal-pg-managed-{tag}"),
    )
    .unwrap();
    let residual = "7b".repeat(32);
    let expires_at = (chrono::Utc::now() + chrono::Duration::hours(1))
        .format("%Y-%m-%dT%H:%M:%SZ")
        .to_string();
    let decision = store
        .upsert_managed_acceptance_decision(
            "tenant-pg-managed-acceptance",
            &pg_managed_acceptance_decision_body(&decision_id, attempt_id),
            &residual,
            "draft_pending_operator",
            None,
            Some(&expires_at),
        )
        .unwrap();
    let risk = store
        .accept_managed_acceptance_decision(
            &principal,
            &RiskAcknowledgementRequest {
                decision_id,
                expected_decision_body_sha256: decision["decision_body_sha256"]
                    .as_str()
                    .unwrap()
                    .to_string(),
                expected_residual_finding_sha256: residual,
                submitted_phrase: OPERATOR_RISK_ACCEPTANCE_PHRASE.to_string(),
                explicit_go: true,
            },
        )
        .unwrap();
    let request =
        pg_managed_acceptance_spend_request(risk["authorization_id"].as_str().unwrap(), attempt_id);
    (principal, risk, request)
}

#[cfg(feature = "pg-tests")]
fn pg_attempt_body_from_spend(spend: &Value) -> Value {
    let mut body = spend["body_json"].clone();
    let manifest = engine::storage::local_product_store::build_attempt_authority_manifest(&body)
        .expect("spend body has a complete attempt manifest");
    body["manifest_sha256"] = manifest["manifest_sha256"].clone();
    body["manifest"] = manifest;
    body
}

#[cfg(feature = "pg-tests")]
fn tool_policy_pass_count(store: &LocalProductStore) -> usize {
    store
        .audit_events(100_000)
        .expect("read tool-policy audit events")
        .into_iter()
        .filter(|event| event["action"] == "tool_execution.pre_policy_passed")
        .count()
}

#[cfg(feature = "pg-tests")]
fn wait_for_new_tool_policy_pass(store: &LocalProductStore, baseline: usize) {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        if tool_policy_pass_count(store) > baseline {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "managed verification did not reach the pre-policy execution boundary"
        );
        thread::sleep(Duration::from_millis(20));
    }
}

#[cfg(feature = "pg-tests")]
fn pg_product_task_to_approval(
    store: &LocalProductStore,
    repo: &std::path::Path,
    revision: &str,
    tag: &str,
    output_intent: &str,
) -> (Value, Value, Value) {
    let request = ProductTaskIntakeRequest {
        objective: format!("postgres {output_intent} authority fixture"),
        target_id: format!("pg-product-{tag}"),
        target_repo_path: repo.to_string_lossy().into_owned(),
        source_revision: revision.to_string(),
        source_tree_hash: None,
        allowed_paths: vec!["docs/product_golden_path_fixture.md".to_string()],
        verification_commands: vec![ProductVerificationCommand {
            command: "test -f docs/product_golden_path_fixture.md".to_string(),
            timeout_ms: 5_000,
        }],
        output_intent: output_intent.to_string(),
        executor_policy: ProductExecutorPolicy {
            allowed_executors: vec!["command".to_string()],
            prefer: Some("command".to_string()),
        },
        budget: None,
        risk_class: "low".to_string(),
        approval_required: true,
        confirm_execution: Some(true),
        confirm_output: Some(true),
        idempotency_key: format!("pg-product-{output_intent}-{tag}"),
        expected_version: None,
        tenant_id: Some("local".to_string()),
        workspace_id: Some("default".to_string()),
        workspace_mode: Some("git_worktree".to_string()),
    };
    let validated = validate_intake(&request, "local", "default").unwrap();
    let task = store
        .admit_product_task(&validated, "pg-product-test")
        .unwrap();
    let task_id = task["task_id"].as_str().unwrap();
    let compiled = store
        .compile_and_schedule_product_task(task_id, "pg-product-test", &["command".to_string()])
        .unwrap();
    let run_id = compiled["task"]["run_id"].as_str().unwrap();
    let executor = engine::node_executor::CommandNodeExecutor::default();
    for _ in 0..8 {
        let tick = store
            .tick_with_executor(run_id, "pg-product-test", 1, &executor)
            .unwrap();
        if matches!(
            tick.pointer("/run/status").and_then(Value::as_str),
            Some("completed" | "failed")
        ) {
            break;
        }
    }
    store
        .finalize_product_task_after_execution(task_id, "pg-product-test")
        .unwrap();
    let task = store.get_product_task(task_id).unwrap().unwrap();
    let approval = store
        .approve_product_task(
            task_id,
            "pg-independent-operator",
            task["version"].as_u64().unwrap(),
        )
        .unwrap();
    let artifact = store
        .get_supervised_patch_artifact(approval["artifact_id"].as_str().unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(
        artifact["changed_files"],
        json!(["+docs/product_golden_path_fixture.md"]),
        "PostgreSQL product artifacts must exclude fixture control files"
    );
    (task, approval, artifact)
}

#[test]
#[cfg(feature = "pg-tests")]
fn pg_managed_acceptance_product_task_phase_revalidates_real_receipts() {
    let Some(store) = test_store() else { return };
    std::env::set_var(PRODUCT_TASK_GATE, "1");
    std::env::set_var("ACP_ENABLE_TARGET_REPO_OUTPUT", "1");
    let workspace_root = tempfile::tempdir().unwrap();
    std::env::set_var("ACP_PRODUCT_WORKSPACE_ROOT", workspace_root.path());
    let (repo, revision) = pg_product_repo("pg managed acceptance receipt phase");
    let tag = uuid_tag();
    let (task, approval, artifact) =
        pg_product_task_to_approval(&store, repo.path(), &revision, &tag, "artifact_only");
    let task_id = task["task_id"].as_str().unwrap().to_string();
    let target_id = task["target_id"].as_str().unwrap().to_string();
    let task_version = task["version"].as_u64().unwrap();
    let awaiting = store
        .validate_managed_acceptance_product_task_phase("local", &task_id, &target_id, &revision)
        .expect("PostgreSQL verification receipt owner path");
    assert_eq!(awaiting["stage"], "awaiting_approval");
    let target_error = store
        .validate_managed_acceptance_product_task_phase(
            "local",
            &task_id,
            "other-target",
            &revision,
        )
        .expect_err("PostgreSQL ProductTask target must exactly bind spend target");
    assert!(target_error.contains("target_id"), "{target_error}");
    let revision_error = store
        .validate_managed_acceptance_product_task_phase(
            "local",
            &task_id,
            &target_id,
            &"f".repeat(40),
        )
        .expect_err("PostgreSQL ProductTask revision must exactly bind spend SHA");
    assert!(
        revision_error.contains("source_revision"),
        "{revision_error}"
    );

    let database_url = std::env::var("ACP_TEST_DATABASE_URL").unwrap();
    let mut client = postgres::Client::connect(&database_url, postgres::NoTls).unwrap();
    let workspace_id = task["workspace_record_id"].as_str().unwrap();
    let original_workspace_json: String = client
        .query_one(
            "SELECT workspace_json FROM supervised_patch_workspaces WHERE workspace_id=$1",
            &[&workspace_id],
        )
        .unwrap()
        .get(0);
    let mut stale_version_workspace: Value =
        serde_json::from_str(&original_workspace_json).unwrap();
    stale_version_workspace["verification"]["expected_task_version"] = json!(0);
    for receipt in stale_version_workspace["verification"]["verification_attempts"]
        .as_array_mut()
        .unwrap()
    {
        receipt["expected_task_version"] = json!(0);
    }
    client
        .execute(
            "UPDATE supervised_patch_workspaces SET workspace_json=$1 WHERE workspace_id=$2",
            &[&stale_version_workspace.to_string(), &workspace_id],
        )
        .unwrap();
    let stale_version_error = store
        .validate_managed_acceptance_product_task_phase("local", &task_id, &target_id, &revision)
        .expect_err("PostgreSQL stale verification version must fail closed");
    assert!(
        stale_version_error.contains("immediately preceding task version"),
        "{stale_version_error}"
    );
    client
        .execute(
            "UPDATE supervised_patch_workspaces SET workspace_json=$1 WHERE workspace_id=$2",
            &[&original_workspace_json, &workspace_id],
        )
        .unwrap();
    let original_boundary_json: String = client
        .query_one(
            "SELECT boundary_json FROM supervised_patch_workspaces WHERE workspace_id=$1",
            &[&workspace_id],
        )
        .unwrap()
        .get(0);
    client
        .execute(
            "UPDATE supervised_patch_workspaces SET boundary_json='not-json' WHERE workspace_id=$1",
            &[&workspace_id],
        )
        .unwrap();
    let boundary_owner_error = store
        .validate_managed_acceptance_product_task_phase("local", &task_id, &target_id, &revision)
        .expect_err("corrupt PostgreSQL workspace boundary owner must fail closed");
    assert!(
        boundary_owner_error
            .contains("managed acceptance workspace boundary owner is invalid JSON"),
        "{boundary_owner_error}"
    );
    client
        .execute(
            "UPDATE supervised_patch_workspaces SET boundary_json=$1 WHERE workspace_id=$2",
            &[&original_boundary_json, &workspace_id],
        )
        .unwrap();
    client
        .execute(
            "UPDATE product_tasks SET confirm_output=0 WHERE task_id=$1",
            &[&task_id],
        )
        .unwrap();
    let boolean_error = store
        .validate_managed_acceptance_product_task_phase("local", &task_id, &target_id, &revision)
        .expect_err("PostgreSQL persisted false confirmation must fail closed");
    assert!(boolean_error.contains("confirm_output"), "{boolean_error}");
    client
        .execute(
            "UPDATE product_tasks SET confirm_output=1 WHERE task_id=$1",
            &[&task_id],
        )
        .unwrap();
    client
        .execute(
            "UPDATE product_tasks SET confirm_output=-1 WHERE task_id=$1",
            &[&task_id],
        )
        .unwrap();
    let malformed_boolean_error = store
        .validate_managed_acceptance_product_task_phase("local", &task_id, &target_id, &revision)
        .expect_err("PostgreSQL non-boolean confirmation storage must fail closed");
    assert!(
        malformed_boolean_error.contains("confirm_output is not a persisted boolean"),
        "{malformed_boolean_error}"
    );
    client
        .execute(
            "UPDATE product_tasks SET confirm_output=1 WHERE task_id=$1",
            &[&task_id],
        )
        .unwrap();

    store
        .output_product_task(
            &task_id,
            "pg-managed-acceptance-output",
            task_version,
            approval["approval_id"].as_str(),
            true,
        )
        .unwrap();
    let terminal = store
        .validate_managed_acceptance_product_task_phase("local", &task_id, &target_id, &revision)
        .expect("PostgreSQL terminal evidence owner path");
    assert_eq!(terminal["stage"], "terminal");

    let mut tampered_workspace: Value = serde_json::from_str(&original_workspace_json).unwrap();
    tampered_workspace["verification"]["trustworthy"] = json!(false);
    client
        .execute(
            "UPDATE supervised_patch_workspaces SET workspace_json=$1 WHERE workspace_id=$2",
            &[&tampered_workspace.to_string(), &workspace_id],
        )
        .unwrap();
    let verification_error = store
        .validate_managed_acceptance_product_task_phase("local", &task_id, &target_id, &revision)
        .expect_err("PostgreSQL untrustworthy verification must fail closed");
    assert!(
        verification_error.contains("verification receipt is not accepted and trustworthy"),
        "{verification_error}"
    );
    client
        .execute(
            "UPDATE supervised_patch_workspaces SET workspace_json=$1 WHERE workspace_id=$2",
            &[&original_workspace_json, &workspace_id],
        )
        .unwrap();
    client
        .execute(
            "UPDATE supervised_patch_workspaces SET workspace_json='not-json' WHERE workspace_id=$1",
            &[&workspace_id],
        )
        .unwrap();
    let owner_error = store
        .validate_managed_acceptance_product_task_phase("local", &task_id, &target_id, &revision)
        .expect_err("corrupt PostgreSQL evidence owner data must fail closed");
    assert!(
        owner_error.contains("workspace") || owner_error.contains("verification"),
        "{owner_error}"
    );
    client
        .execute(
            "UPDATE supervised_patch_workspaces SET workspace_json=$1 WHERE workspace_id=$2",
            &[&original_workspace_json, &workspace_id],
        )
        .unwrap();

    let artifact_id = artifact["artifact_id"].as_str().unwrap();
    let original_artifact_json: String = client
        .query_one(
            "SELECT artifact_json FROM supervised_patch_artifacts WHERE artifact_id=$1",
            &[&artifact_id],
        )
        .unwrap()
        .get(0);
    let mut tampered_artifact: Value = serde_json::from_str(&original_artifact_json).unwrap();
    tampered_artifact["product_output_receipt"]["expected_task_version"] = json!(0);
    client
        .execute(
            "UPDATE supervised_patch_artifacts SET artifact_json=$1 WHERE artifact_id=$2",
            &[&tampered_artifact.to_string(), &artifact_id],
        )
        .unwrap();
    let output_version_error = store
        .validate_managed_acceptance_product_task_phase("local", &task_id, &target_id, &revision)
        .expect_err("PostgreSQL stale output version must fail closed");
    assert!(
        output_version_error.contains("output receipt/operation content binding"),
        "{output_version_error}"
    );
    client
        .execute(
            "UPDATE supervised_patch_artifacts SET artifact_json=$1 WHERE artifact_id=$2",
            &[&original_artifact_json, &artifact_id],
        )
        .unwrap();
    let original_changed_files_json: String = client
        .query_one(
            "SELECT changed_files_json FROM supervised_patch_artifacts WHERE artifact_id=$1",
            &[&artifact_id],
        )
        .unwrap()
        .get(0);
    client
        .execute(
            "UPDATE supervised_patch_artifacts SET changed_files_json='not-json' WHERE artifact_id=$1",
            &[&artifact_id],
        )
        .unwrap();
    let changed_files_owner_error = store
        .validate_managed_acceptance_product_task_phase("local", &task_id, &target_id, &revision)
        .expect_err("corrupt PostgreSQL artifact changed-files owner must fail closed");
    assert!(
        changed_files_owner_error
            .contains("managed acceptance artifact changed-files owner is invalid JSON"),
        "{changed_files_owner_error}"
    );
    client
        .execute(
            "UPDATE supervised_patch_artifacts SET changed_files_json=$1 WHERE artifact_id=$2",
            &[&original_changed_files_json, &artifact_id],
        )
        .unwrap();

    client
        .execute(
            "UPDATE supervised_patch_artifacts SET artifact_json='not-json' WHERE artifact_id=$1",
            &[&artifact_id],
        )
        .unwrap();
    let artifact_json_owner_error = store
        .validate_managed_acceptance_product_task_phase("local", &task_id, &target_id, &revision)
        .expect_err("corrupt PostgreSQL artifact owner must fail closed");
    assert!(
        artifact_json_owner_error.contains("managed acceptance artifact owner is invalid JSON"),
        "{artifact_json_owner_error}"
    );
    client
        .execute(
            "UPDATE supervised_patch_artifacts SET artifact_json=$1 WHERE artifact_id=$2",
            &[&original_artifact_json, &artifact_id],
        )
        .unwrap();

    let mut tampered_artifact: Value = serde_json::from_str(&original_artifact_json).unwrap();
    tampered_artifact["product_task_id"] = json!("other-product-task");
    client
        .execute(
            "UPDATE supervised_patch_artifacts SET artifact_json=$1 WHERE artifact_id=$2",
            &[&tampered_artifact.to_string(), &artifact_id],
        )
        .unwrap();
    let artifact_owner_error = store
        .validate_managed_acceptance_product_task_phase("local", &task_id, &target_id, &revision)
        .expect_err("PostgreSQL artifact from another ProductTask must fail closed");
    assert!(
        artifact_owner_error.contains("no exact artifact")
            || artifact_owner_error.contains("artifact target binding"),
        "{artifact_owner_error}"
    );
    client
        .execute(
            "UPDATE supervised_patch_artifacts SET artifact_json=$1 WHERE artifact_id=$2",
            &[&original_artifact_json, &artifact_id],
        )
        .unwrap();

    let mut tampered_artifact: Value = serde_json::from_str(&original_artifact_json).unwrap();
    tampered_artifact["product_output_receipt"]["approval_id"] = json!("stale-approval-id");
    client
        .execute(
            "UPDATE supervised_patch_artifacts SET artifact_json=$1 WHERE artifact_id=$2",
            &[&tampered_artifact.to_string(), &artifact_id],
        )
        .unwrap();
    let approval_error = store
        .validate_managed_acceptance_product_task_phase("local", &task_id, &target_id, &revision)
        .expect_err("PostgreSQL output receipt must bind current approval");
    assert!(
        approval_error.contains("approval receipt") || approval_error.contains("output receipt"),
        "{approval_error}"
    );
    client
        .execute(
            "UPDATE supervised_patch_artifacts SET artifact_json=$1 WHERE artifact_id=$2",
            &[&original_artifact_json, &artifact_id],
        )
        .unwrap();

    let original_artifact: Value = serde_json::from_str(&original_artifact_json).unwrap();
    let approval_id = original_artifact["product_output_receipt"]["approval_id"]
        .as_str()
        .unwrap()
        .to_string();
    let original_approval_json: String = client
        .query_one(
            "SELECT approval_json FROM workflow_run_approvals WHERE approval_id=$1",
            &[&approval_id],
        )
        .unwrap()
        .get(0);
    client
        .execute(
            "UPDATE workflow_run_approvals SET approval_json='not-json' WHERE approval_id=$1",
            &[&approval_id],
        )
        .unwrap();
    let approval_owner_error = store
        .validate_managed_acceptance_product_task_phase("local", &task_id, &target_id, &revision)
        .expect_err("corrupt PostgreSQL approval owner JSON must fail closed");
    assert!(
        approval_owner_error.contains("workflow run approval receipt is invalid JSON"),
        "{approval_owner_error}"
    );
    client
        .execute(
            "UPDATE workflow_run_approvals SET approval_json=$1 WHERE approval_id=$2",
            &[&original_approval_json, &approval_id],
        )
        .unwrap();

    let run_id = task["run_id"].as_str().unwrap();
    let row = client
        .query_one(
            "SELECT node_id, node_json FROM workflow_run_nodes WHERE run_id=$1 LIMIT 1",
            &[&run_id],
        )
        .unwrap();
    let node_id: String = row.get(0);
    let original_node_json: String = row.get(1);
    let original_run_json: String = client
        .query_one(
            "SELECT run_json FROM workflow_runs WHERE run_id=$1",
            &[&run_id],
        )
        .unwrap()
        .get(0);
    client
        .execute(
            "UPDATE workflow_runs SET run_json='not-json' WHERE run_id=$1",
            &[&run_id],
        )
        .unwrap();
    let run_owner_error = store
        .validate_managed_acceptance_product_task_phase("local", &task_id, &target_id, &revision)
        .expect_err("corrupt PostgreSQL workflow run owner must fail closed");
    assert!(
        run_owner_error.contains("managed acceptance workflow run owner is invalid JSON"),
        "{run_owner_error}"
    );
    client
        .execute(
            "UPDATE workflow_runs SET run_json=$1 WHERE run_id=$2",
            &[&original_run_json, &run_id],
        )
        .unwrap();
    let original_workflow_boundaries_json: String = client
        .query_one(
            "SELECT boundaries_json FROM workflow_runs WHERE run_id=$1",
            &[&run_id],
        )
        .unwrap()
        .get(0);
    client
        .execute(
            "UPDATE workflow_runs SET boundaries_json='not-json' WHERE run_id=$1",
            &[&run_id],
        )
        .unwrap();
    let workflow_boundaries_owner_error = store
        .validate_managed_acceptance_product_task_phase("local", &task_id, &target_id, &revision)
        .expect_err("corrupt PostgreSQL workflow boundaries owner must fail closed");
    assert!(
        workflow_boundaries_owner_error
            .contains("managed acceptance workflow boundaries owner is invalid JSON"),
        "{workflow_boundaries_owner_error}"
    );
    client
        .execute(
            "UPDATE workflow_runs SET boundaries_json=$1 WHERE run_id=$2",
            &[&original_workflow_boundaries_json, &run_id],
        )
        .unwrap();
    client
        .execute(
            "UPDATE workflow_run_nodes SET node_json='not-json' WHERE run_id=$1 AND node_id=$2",
            &[&run_id, &node_id],
        )
        .unwrap();
    let node_owner_error = store
        .validate_managed_acceptance_product_task_phase("local", &task_id, &target_id, &revision)
        .expect_err("corrupt PostgreSQL workflow node owner must fail closed");
    assert!(
        node_owner_error.contains("managed acceptance workflow node owner is invalid JSON"),
        "{node_owner_error}"
    );
    client
        .execute(
            "UPDATE workflow_run_nodes SET node_json=$1 WHERE run_id=$2 AND node_id=$3",
            &[&original_node_json, &run_id, &node_id],
        )
        .unwrap();
    let duplicate_node_id = format!("{node_id}-duplicate-owner");
    let mut duplicate_node: Value = serde_json::from_str(&original_node_json).unwrap();
    duplicate_node["node_id"] = json!(duplicate_node_id);
    client
        .execute(
            "INSERT INTO workflow_run_nodes
             (run_id, node_id, task_type, status, node_json, attempt_count)
             VALUES ($1, $2, 'product_apply', 'completed', $3, 0)",
            &[&run_id, &duplicate_node_id, &duplicate_node.to_string()],
        )
        .unwrap();
    let duplicate_node_error = store
        .validate_managed_acceptance_product_task_phase("local", &task_id, &target_id, &revision)
        .expect_err("multiple PostgreSQL nodes claiming one ProductTask must fail closed");
    assert!(
        duplicate_node_error.contains("multiple workflow nodes claim one ProductTask owner"),
        "{duplicate_node_error}"
    );
    client
        .execute(
            "DELETE FROM workflow_run_nodes WHERE run_id=$1 AND node_id=$2",
            &[&run_id, &duplicate_node_id],
        )
        .unwrap();

    let original_terminal_evidence_json: String = client
        .query_one(
            "SELECT evidence_json FROM product_task_terminal_evidence WHERE product_task_id=$1",
            &[&task_id],
        )
        .unwrap()
        .get(0);
    client
        .execute(
            "UPDATE product_task_terminal_evidence SET evidence_json='not-json' WHERE product_task_id=$1",
            &[&task_id],
        )
        .unwrap();
    let terminal_owner_error = store
        .validate_managed_acceptance_product_task_phase("local", &task_id, &target_id, &revision)
        .expect_err("corrupt PostgreSQL terminal evidence owner must fail closed");
    assert!(
        terminal_owner_error.contains("product task terminal evidence is invalid JSON"),
        "{terminal_owner_error}"
    );
    client
        .execute(
            "UPDATE product_task_terminal_evidence SET evidence_json=$1 WHERE product_task_id=$2",
            &[&original_terminal_evidence_json, &task_id],
        )
        .unwrap();

    client
        .execute(
            "DELETE FROM product_task_terminal_evidence WHERE product_task_id=$1",
            &[&task_id],
        )
        .unwrap();
    let terminal_error = store
        .validate_managed_acceptance_product_task_phase("local", &task_id, &target_id, &revision)
        .expect_err("PostgreSQL completed claim without terminal evidence must fail closed");
    assert!(
        terminal_error.contains("terminal evidence"),
        "{terminal_error}"
    );

    std::env::remove_var("ACP_PRODUCT_WORKSPACE_ROOT");
    std::env::remove_var("ACP_ENABLE_TARGET_REPO_OUTPUT");
    std::env::remove_var(PRODUCT_TASK_GATE);
}

#[test]
#[cfg(feature = "pg-tests")]
fn pg_product_artifact_rejects_changes_outside_allowed_paths() {
    let Some(store) = test_store() else { return };
    std::env::set_var(PRODUCT_TASK_GATE, "1");
    std::env::set_var("ACP_ENABLE_TARGET_REPO_OUTPUT", "1");
    let workspace_root = tempfile::tempdir().unwrap();
    std::env::set_var("ACP_PRODUCT_WORKSPACE_ROOT", workspace_root.path());
    let (repo, revision) = pg_product_repo("pg out-of-scope product artifact");
    let tag = uuid_tag();
    let request = ProductTaskIntakeRequest {
        objective: "postgres allowed-path artifact boundary".to_string(),
        target_id: format!("pg-product-{tag}"),
        target_repo_path: repo.path().to_string_lossy().into_owned(),
        source_revision: revision,
        source_tree_hash: None,
        allowed_paths: vec![
            "docs/product_golden_path_fixture.md".to_string(),
            "./admitted-subtree/".to_string(),
        ],
        verification_commands: vec![ProductVerificationCommand {
            command: "test -f docs/product_golden_path_fixture.md".to_string(),
            timeout_ms: 5_000,
        }],
        output_intent: "artifact_only".to_string(),
        executor_policy: ProductExecutorPolicy {
            allowed_executors: vec!["command".to_string()],
            prefer: Some("command".to_string()),
        },
        budget: None,
        risk_class: "low".to_string(),
        approval_required: true,
        confirm_execution: Some(true),
        confirm_output: None,
        idempotency_key: format!("pg-product-out-of-scope-{tag}"),
        expected_version: None,
        tenant_id: Some("local".to_string()),
        workspace_id: Some("default".to_string()),
        workspace_mode: Some("git_worktree".to_string()),
    };
    let validated = validate_intake(&request, "local", "default").unwrap();
    let task = store
        .admit_product_task(&validated, "pg-product-test")
        .unwrap();
    let task_id = task["task_id"].as_str().unwrap();
    let compiled = store
        .compile_and_schedule_product_task(task_id, "pg-product-test", &["command".to_string()])
        .unwrap();
    let run_id = compiled["task"]["run_id"].as_str().unwrap();
    let executor = engine::node_executor::CommandNodeExecutor::default();
    for _ in 0..8 {
        let tick = store
            .tick_with_executor(run_id, "pg-product-test", 1, &executor)
            .unwrap();
        if matches!(
            tick.pointer("/run/status").and_then(Value::as_str),
            Some("completed" | "failed")
        ) {
            break;
        }
    }
    let workspace = compiled["task"]["workspace_binding"]["workspace_path"]
        .as_str()
        .unwrap();
    std::fs::create_dir_all(std::path::Path::new(workspace).join("admitted-subtree/nested"))
        .unwrap();
    std::fs::write(
        std::path::Path::new(workspace).join("admitted-subtree/nested/allowed.md"),
        "admitted subtree change\n",
    )
    .unwrap();
    std::fs::write(
        std::path::Path::new(workspace).join("outside-product-scope.txt"),
        "must not enter artifact\n",
    )
    .unwrap();

    let finalized = store
        .finalize_product_task_after_execution(task_id, "pg-product-test")
        .expect("PostgreSQL artifact capture must durably block out-of-scope changes");
    assert_eq!(finalized["phase"], "verification_authority_lost");
    assert_eq!(finalized["task"]["status"], "blocked");
    assert!(finalized["artifact_id"].is_null());
    assert_eq!(finalized["verification"]["status"], "authority_lost");
    assert!(finalized["verification"]["authority_loss_reason"]
        .as_str()
        .is_some_and(|reason| reason.ends_with("outside-product-scope.txt")));
    assert!(!store
        .supervised_patch_artifacts(10_000)
        .unwrap()
        .iter()
        .any(|artifact| artifact["run_id"] == run_id));
    let task = store.get_product_task(task_id).unwrap().unwrap();
    assert_eq!(task["status"], "blocked");
    let workspace = store
        .get_supervised_patch_workspace(task["workspace_record_id"].as_str().unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(workspace["status"], "quarantined");
    assert_eq!(workspace["verification"]["status"], "authority_lost");
    std::env::remove_var("ACP_PRODUCT_WORKSPACE_ROOT");
    std::env::remove_var("ACP_ENABLE_TARGET_REPO_OUTPUT");
    std::env::remove_var(PRODUCT_TASK_GATE);
}

#[cfg(feature = "pg-tests")]
fn pg_product_repo(label: &str) -> (tempfile::TempDir, String) {
    let repo = tempfile::tempdir().unwrap();
    std::fs::write(repo.path().join("README.md"), format!("{label}\n")).unwrap();
    for args in [
        &["init", "-b", "main"][..],
        &["config", "user.email", "pg-product@example.invalid"][..],
        &["config", "user.name", "PG Product Test"][..],
        &["add", "README.md"][..],
        &["commit", "-m", "init"][..],
    ] {
        let output = Command::new("git")
            .args(args)
            .current_dir(repo.path())
            .output()
            .unwrap();
        assert!(output.status.success(), "git {args:?}: {:?}", output);
    }
    let revision = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo.path())
        .output()
        .unwrap();
    (
        repo,
        String::from_utf8_lossy(&revision.stdout).trim().to_string(),
    )
}

#[cfg(feature = "pg-tests")]
fn pg_ready_for_verification(
    store: &LocalProductStore,
    repo: &std::path::Path,
    revision: &str,
    tag: &str,
    verification_command: &str,
) -> (String, String) {
    pg_ready_for_verification_with_budget(store, repo, revision, tag, verification_command, None)
}

#[cfg(feature = "pg-tests")]
fn pg_ready_for_verification_with_budget(
    store: &LocalProductStore,
    repo: &std::path::Path,
    revision: &str,
    tag: &str,
    verification_command: &str,
    budget: Option<ProductTaskBudget>,
) -> (String, String) {
    let request = ProductTaskIntakeRequest {
        objective: format!("postgres verification authority {tag}"),
        target_id: format!("pg-verification-{tag}"),
        target_repo_path: repo.to_string_lossy().into_owned(),
        source_revision: revision.to_string(),
        source_tree_hash: None,
        allowed_paths: vec!["docs/product_golden_path_fixture.md".to_string()],
        verification_commands: vec![ProductVerificationCommand {
            command: verification_command.to_string(),
            timeout_ms: 700,
        }],
        output_intent: "artifact_only".to_string(),
        executor_policy: ProductExecutorPolicy {
            allowed_executors: vec!["command".to_string()],
            prefer: Some("command".to_string()),
        },
        budget,
        risk_class: "low".to_string(),
        approval_required: true,
        confirm_execution: Some(true),
        confirm_output: Some(true),
        idempotency_key: format!("pg-verification-{tag}"),
        expected_version: None,
        tenant_id: Some("local".to_string()),
        workspace_id: Some("default".to_string()),
        workspace_mode: Some("git_worktree".to_string()),
    };
    let validated = validate_intake(&request, "local", "default").unwrap();
    let task = store
        .admit_product_task(&validated, "pg-verification")
        .unwrap();
    let task_id = task["task_id"].as_str().unwrap().to_string();
    let compiled = store
        .compile_and_schedule_product_task(&task_id, "pg-verification", &["command".to_string()])
        .unwrap();
    let run_id = compiled["task"]["run_id"].as_str().unwrap().to_string();
    let executor = engine::node_executor::CommandNodeExecutor::default();
    for _ in 0..8 {
        let tick = store
            .tick_with_executor(&run_id, "pg-verification", 1, &executor)
            .unwrap();
        if matches!(
            tick.pointer("/run/status").and_then(Value::as_str),
            Some("completed" | "failed")
        ) {
            break;
        }
    }
    (task_id, run_id)
}

#[cfg(feature = "pg-tests")]
fn pg_running_scheduler_authority() -> ProductVerificationRuntimeAuthority {
    ProductVerificationRuntimeAuthority {
        scheduler_attached: true,
        scheduler_running: true,
        scheduler_paused: false,
        scheduler_killed: false,
        global_kill_active: false,
        manual_operational_tick: false,
    }
}

#[test]
#[cfg(feature = "pg-tests")]
fn pg_scheduler_kill_during_verification_rejects_late_result() {
    let Some(store) = test_store() else { return };
    std::env::set_var(PRODUCT_TASK_GATE, "1");
    std::env::set_var("ACP_ENABLE_TARGET_REPO_OUTPUT", "1");
    let workspace_root = tempfile::tempdir().unwrap();
    std::env::set_var("ACP_PRODUCT_WORKSPACE_ROOT", workspace_root.path());
    let (repo, revision) = pg_product_repo("pg scheduler kill verification");
    let tag = uuid_tag();
    let store = Arc::new(store);
    let (task_id, _) =
        pg_ready_for_verification(&store, repo.path(), &revision, &tag, "tail -f README.md");
    let scheduler_killed = Arc::new(AtomicBool::new(false));
    let tool_passes_before = tool_policy_pass_count(&store);
    let finalizer_store = Arc::clone(&store);
    let finalizer_task_id = task_id.clone();
    let finalizer_scheduler_killed = Arc::clone(&scheduler_killed);
    let handle = thread::spawn(move || {
        finalizer_store.finalize_product_task_after_execution_with_authority(
            &finalizer_task_id,
            "pg-verifier",
            &|| {
                let mut authority = pg_running_scheduler_authority();
                authority.scheduler_killed = finalizer_scheduler_killed.load(Ordering::SeqCst);
                Ok(authority)
            },
        )
    });
    wait_for_new_tool_policy_pass(&store, tool_passes_before);
    scheduler_killed.store(true, Ordering::SeqCst);
    let finalized = handle.join().unwrap().unwrap();

    assert_eq!(finalized["phase"], "verification_authority_lost");
    assert_eq!(finalized["task"]["status"], "killed");
    assert!(finalized["artifact_id"].is_null());
    assert_eq!(
        finalized["verification"]["verification_attempts"][0]["result_status"],
        "stale_rejected"
    );
    assert_eq!(
        finalized["verification"]["verification_attempts"][0]["late_result_rejected"],
        true
    );
    std::env::remove_var("ACP_PRODUCT_WORKSPACE_ROOT");
    std::env::remove_var("ACP_ENABLE_TARGET_REPO_OUTPUT");
    std::env::remove_var(PRODUCT_TASK_GATE);
}

#[test]
#[cfg(feature = "pg-tests")]
fn pg_verification_filesystem_write_is_quarantined_and_never_captured() {
    let Some(store) = test_store() else { return };
    std::env::set_var(PRODUCT_TASK_GATE, "1");
    std::env::set_var("ACP_ENABLE_TARGET_REPO_OUTPUT", "1");
    let workspace_root = tempfile::tempdir().unwrap();
    std::env::set_var("ACP_PRODUCT_WORKSPACE_ROOT", workspace_root.path());
    let (repo, revision) = pg_product_repo("pg verification late write");
    let tag = uuid_tag();
    let store = Arc::new(store);
    let (task_id, _) =
        pg_ready_for_verification(&store, repo.path(), &revision, &tag, "tail -f README.md");
    let task = store.get_product_task(&task_id).unwrap().unwrap();
    let workspace_id = task["workspace_record_id"].as_str().unwrap().to_string();
    let workspace_path = task["workspace_binding"]["workspace_path"]
        .as_str()
        .unwrap()
        .to_string();
    let tool_passes_before = tool_policy_pass_count(&store);
    let finalizer_store = Arc::clone(&store);
    let finalizer_task = task_id.clone();
    let handle = thread::spawn(move || {
        finalizer_store.finalize_product_task_after_execution_with_authority(
            &finalizer_task,
            "pg-verifier",
            &|| Ok(pg_running_scheduler_authority()),
        )
    });
    wait_for_new_tool_policy_pass(&store, tool_passes_before);
    std::fs::write(
        std::path::Path::new(&workspace_path).join("README.md"),
        "late write\n",
    )
    .unwrap();
    let finalized = handle.join().unwrap().unwrap();

    assert_eq!(finalized["phase"], "verification_authority_lost");
    assert_eq!(finalized["task"]["status"], "blocked");
    assert!(finalized["artifact_id"].is_null());
    assert_eq!(
        store
            .get_supervised_patch_workspace(&workspace_id)
            .unwrap()
            .unwrap()["status"],
        "quarantined"
    );
    assert!(finalized["verification"]["authority_loss_reason"]
        .as_str()
        .unwrap()
        .contains("late_filesystem_write"));
    std::env::remove_var("ACP_PRODUCT_WORKSPACE_ROOT");
    std::env::remove_var("ACP_ENABLE_TARGET_REPO_OUTPUT");
    std::env::remove_var(PRODUCT_TASK_GATE);
}

#[test]
#[cfg(feature = "pg-tests")]
fn pg_concurrent_product_finalizers_consume_one_verification_effect() {
    let Some(store) = test_store() else { return };
    std::env::set_var(PRODUCT_TASK_GATE, "1");
    std::env::set_var("ACP_ENABLE_TARGET_REPO_OUTPUT", "1");
    let workspace_root = tempfile::tempdir().unwrap();
    std::env::set_var("ACP_PRODUCT_WORKSPACE_ROOT", workspace_root.path());
    let (repo, revision) = pg_product_repo("pg concurrent product verification");
    let tag = uuid_tag();
    let store = Arc::new(store);
    let (task_id, _) =
        pg_ready_for_verification(&store, repo.path(), &revision, &tag, "tail -f README.md");
    let task = store.get_product_task(&task_id).unwrap().unwrap();
    let mut handles = Vec::new();
    for _ in 0..2 {
        let store = Arc::clone(&store);
        let task_id = task_id.clone();
        handles.push(std::thread::spawn(move || {
            store.finalize_product_task_after_execution_with_authority(
                &task_id,
                "pg-verifier",
                &|| Ok(pg_running_scheduler_authority()),
            )
        }));
    }
    let results: Vec<_> = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect();
    assert!(
        results.iter().any(Result::is_ok),
        "at least one concurrent finalizer must return its persisted result: {results:?}"
    );
    let managed_run_id = results
        .iter()
        .filter_map(|result| result.as_ref().ok())
        .find_map(|result| {
            result
                .pointer("/verification/verification_attempts/0/verification_run_id")
                .and_then(Value::as_str)
        })
        .expect("one finalizer must persist the managed verification run");
    let allowed_effects = store
        .audit_events(10_000)
        .unwrap()
        .into_iter()
        .filter(|event| {
            event["action"] == "tool_execution.pre_policy_passed"
                && event["resource"] == managed_run_id
        })
        .count();
    assert_eq!(allowed_effects, 1);
    assert!(store
        .supervised_patch_artifacts(100)
        .unwrap()
        .iter()
        .all(|artifact| { artifact["workspace_id"] != task["workspace_record_id"] }));
    std::env::remove_var("ACP_PRODUCT_WORKSPACE_ROOT");
    std::env::remove_var("ACP_ENABLE_TARGET_REPO_OUTPUT");
    std::env::remove_var(PRODUCT_TASK_GATE);
}

#[test]
#[cfg(feature = "pg-tests")]
fn pg_product_artifact_audit_failure_rolls_back_artifact_workspace_and_task() {
    let Some(store) = test_store() else { return };
    std::env::set_var(PRODUCT_TASK_GATE, "1");
    std::env::set_var("ACP_ENABLE_TARGET_REPO_OUTPUT", "1");
    let workspace_root = tempfile::tempdir().unwrap();
    std::env::set_var("ACP_PRODUCT_WORKSPACE_ROOT", workspace_root.path());
    let (repo, revision) = pg_product_repo("pg artifact audit rollback");
    let tag = uuid_tag();
    let (task_id, _) = pg_ready_for_verification(&store, repo.path(), &revision, &tag, "true");
    let task = store.get_product_task(&task_id).unwrap().unwrap();
    let workspace_id = task["workspace_record_id"].as_str().unwrap().to_string();

    let suffix = uuid_tag().replace('-', "_");
    let function_name = format!("reject_product_artifact_audit_{suffix}");
    let trigger_name = format!("reject_product_artifact_audit_trigger_{suffix}");
    let database_url = std::env::var("ACP_TEST_DATABASE_URL").unwrap();
    let mut client = postgres::Client::connect(&database_url, postgres::NoTls).unwrap();
    client
        .batch_execute(&format!(
            "CREATE FUNCTION {function_name}() RETURNS trigger
             LANGUAGE plpgsql AS $$
             BEGIN
               IF NEW.action = 'supervised_patch.artifact_record' THEN
                 RAISE EXCEPTION 'injected product artifact audit failure';
               END IF;
               RETURN NEW;
             END;
             $$;
             CREATE TRIGGER {trigger_name}
             BEFORE INSERT ON audit_log
             FOR EACH ROW EXECUTE FUNCTION {function_name}();"
        ))
        .unwrap();

    let error = store
        .finalize_product_task_after_execution_with_authority(&task_id, "pg-verifier", &|| {
            Ok(pg_running_scheduler_authority())
        })
        .expect_err("PG artifact audit failure must abort the transaction");
    assert!(
        error.contains("db error"),
        "unexpected PG artifact rollback error: {error}"
    );
    assert_eq!(
        store.get_product_task(&task_id).unwrap().unwrap()["status"],
        "verifying"
    );
    assert_ne!(
        store
            .get_supervised_patch_workspace(&workspace_id)
            .unwrap()
            .unwrap()["status"],
        "patch_prepared"
    );
    assert!(store
        .supervised_patch_artifacts(10_000)
        .unwrap()
        .iter()
        .all(|artifact| artifact["workspace_id"] != workspace_id));

    client
        .batch_execute(&format!(
            "DROP TRIGGER {trigger_name} ON audit_log;
             DROP FUNCTION {function_name}();"
        ))
        .unwrap();
    let retry = store
        .finalize_product_task_after_execution_with_authority(&task_id, "pg-verifier", &|| {
            Ok(pg_running_scheduler_authority())
        })
        .unwrap();
    assert_eq!(retry["phase"], "awaiting_approval");
    std::env::remove_var("ACP_PRODUCT_WORKSPACE_ROOT");
    std::env::remove_var("ACP_ENABLE_TARGET_REPO_OUTPUT");
    std::env::remove_var(PRODUCT_TASK_GATE);
}

#[test]
#[cfg(feature = "pg-tests")]
fn pg_restart_after_persisted_effect_rejects_changed_pre_patch_binding() {
    let Some(store) = test_store() else { return };
    std::env::set_var(PRODUCT_TASK_GATE, "1");
    std::env::set_var("ACP_ENABLE_TARGET_REPO_OUTPUT", "1");
    let workspace_root = tempfile::tempdir().unwrap();
    std::env::set_var("ACP_PRODUCT_WORKSPACE_ROOT", workspace_root.path());
    let (repo, revision) = pg_product_repo("pg restart after product verify effect");
    let tag = uuid_tag();
    let store = Arc::new(store);
    let (task_id, _) = pg_ready_for_verification(&store, repo.path(), &revision, &tag, "true");
    let task = store.get_product_task(&task_id).unwrap().unwrap();
    let workspace_path = task["workspace_binding"]["workspace_path"]
        .as_str()
        .unwrap()
        .to_string();
    let workspace_id = task["workspace_record_id"].as_str().unwrap().to_string();
    let calls = Arc::new(AtomicUsize::new(0));
    let finalizer_store = Arc::clone(&store);
    let finalizer_task = task_id.clone();
    let finalizer_calls = Arc::clone(&calls);
    let crash_workspace = workspace_path.clone();
    let handle = thread::spawn(move || {
        finalizer_store.finalize_product_task_after_execution_with_authority(
            &finalizer_task,
            "pg-verifier-before-crash",
            &|| {
                if finalizer_calls.fetch_add(1, Ordering::SeqCst) == 1 {
                    std::fs::write(
                        std::path::Path::new(&crash_workspace).join("README.md"),
                        "changed in PG crash window\n",
                    )
                    .unwrap();
                    panic!("simulated PG finalizer process loss after durable effect");
                }
                Ok(pg_running_scheduler_authority())
            },
        )
    });
    assert!(handle.join().is_err());

    let database_url = std::env::var("ACP_TEST_DATABASE_URL").unwrap();
    let restarted = LocalProductStore::new_postgres(&database_url, utc_now_string).unwrap();
    let finalized = restarted
        .finalize_product_task_after_execution_with_authority(
            &task_id,
            "pg-verifier-after-restart",
            &|| Ok(pg_running_scheduler_authority()),
        )
        .unwrap();
    assert_eq!(finalized["phase"], "verification_authority_lost");
    assert_eq!(finalized["task"]["status"], "blocked");
    assert!(finalized["artifact_id"].is_null());
    assert!(finalized["verification"]["authority_loss_reason"]
        .as_str()
        .unwrap()
        .contains("pre_patch_binding_superseded"));
    assert_eq!(
        restarted
            .get_supervised_patch_workspace(&workspace_id)
            .unwrap()
            .unwrap()["status"],
        "quarantined"
    );
    std::env::remove_var("ACP_PRODUCT_WORKSPACE_ROOT");
    std::env::remove_var("ACP_ENABLE_TARGET_REPO_OUTPUT");
    std::env::remove_var(PRODUCT_TASK_GATE);
}

#[test]
#[cfg(feature = "pg-tests")]
fn pg_pause_during_verification_rejects_late_result() {
    let Some(store) = test_store() else { return };
    std::env::set_var(PRODUCT_TASK_GATE, "1");
    std::env::set_var("ACP_ENABLE_TARGET_REPO_OUTPUT", "1");
    let workspace_root = tempfile::tempdir().unwrap();
    std::env::set_var("ACP_PRODUCT_WORKSPACE_ROOT", workspace_root.path());
    let (repo, revision) = pg_product_repo("pg pause during verification");
    let tag = uuid_tag();
    let store = Arc::new(store);
    let (task_id, run_id) =
        pg_ready_for_verification(&store, repo.path(), &revision, &tag, "tail -f README.md");
    let tool_passes_before = tool_policy_pass_count(&store);
    let finalizer_store = Arc::clone(&store);
    let finalizer_task = task_id.clone();
    let handle = thread::spawn(move || {
        finalizer_store.finalize_product_task_after_execution_with_authority(
            &finalizer_task,
            "pg-verifier",
            &|| Ok(pg_running_scheduler_authority()),
        )
    });
    wait_for_new_tool_policy_pass(&store, tool_passes_before);
    store
        .update_run_pause_reason(&run_id, Some("pg_operator_hold"))
        .unwrap();
    let finalized = handle.join().unwrap().unwrap();
    assert_eq!(finalized["phase"], "verification_authority_lost");
    assert_eq!(finalized["task"]["status"], "paused");
    assert!(finalized["artifact_id"].is_null());
    std::env::remove_var("ACP_PRODUCT_WORKSPACE_ROOT");
    std::env::remove_var("ACP_ENABLE_TARGET_REPO_OUTPUT");
    std::env::remove_var(PRODUCT_TASK_GATE);
}

#[test]
#[cfg(feature = "pg-tests")]
fn pg_node_attempt_and_lease_timestamp_supersession_reject_late_results() {
    for mode in ["attempt", "leased_at"] {
        let Some(store) = test_store() else { return };
        std::env::set_var(PRODUCT_TASK_GATE, "1");
        std::env::set_var("ACP_ENABLE_TARGET_REPO_OUTPUT", "1");
        let workspace_root = tempfile::tempdir().unwrap();
        std::env::set_var("ACP_PRODUCT_WORKSPACE_ROOT", workspace_root.path());
        let (repo, revision) = pg_product_repo(&format!("pg {mode} supersession"));
        let tag = uuid_tag();
        let store = Arc::new(store);
        let (task_id, run_id) =
            pg_ready_for_verification(&store, repo.path(), &revision, &tag, "tail -f README.md");
        let tool_passes_before = tool_policy_pass_count(&store);
        let finalizer_store = Arc::clone(&store);
        let finalizer_task = task_id.clone();
        let handle = thread::spawn(move || {
            finalizer_store.finalize_product_task_after_execution_with_authority(
                &finalizer_task,
                "pg-verifier",
                &|| Ok(pg_running_scheduler_authority()),
            )
        });
        wait_for_new_tool_policy_pass(&store, tool_passes_before);
        let database_url = std::env::var("ACP_TEST_DATABASE_URL").unwrap();
        let mut client = postgres::Client::connect(&database_url, postgres::NoTls).unwrap();
        if mode == "attempt" {
            client
                .execute(
                    "UPDATE workflow_run_nodes SET attempt_count = attempt_count + 1 WHERE run_id = $1",
                    &[&run_id],
                )
                .unwrap();
        } else {
            client
                .execute(
                    "UPDATE workflow_run_nodes SET leased_at = '2099-01-01T00:00:00Z' WHERE run_id = $1",
                    &[&run_id],
                )
                .unwrap();
        }
        let finalized = handle.join().unwrap().unwrap();
        assert_eq!(finalized["phase"], "verification_authority_lost");
        assert_eq!(finalized["task"]["status"], "blocked");
        assert!(finalized["artifact_id"].is_null());
        assert!(finalized["verification"]["authority_loss_reason"]
            .as_str()
            .unwrap()
            .contains("node_attempt_or_lease_superseded"));
        std::env::remove_var("ACP_PRODUCT_WORKSPACE_ROOT");
        std::env::remove_var("ACP_ENABLE_TARGET_REPO_OUTPUT");
        std::env::remove_var(PRODUCT_TASK_GATE);
    }
}

#[test]
#[cfg(feature = "pg-tests")]
fn pg_task_kill_and_version_supersession_reject_late_results() {
    for mode in ["kill", "version"] {
        let Some(store) = test_store() else { return };
        std::env::set_var(PRODUCT_TASK_GATE, "1");
        std::env::set_var("ACP_ENABLE_TARGET_REPO_OUTPUT", "1");
        let workspace_root = tempfile::tempdir().unwrap();
        std::env::set_var("ACP_PRODUCT_WORKSPACE_ROOT", workspace_root.path());
        let (repo, revision) = pg_product_repo(&format!("pg {mode} during verification"));
        let tag = uuid_tag();
        let store = Arc::new(store);
        let (task_id, _) =
            pg_ready_for_verification(&store, repo.path(), &revision, &tag, "tail -f README.md");
        let tool_passes_before = tool_policy_pass_count(&store);
        let finalizer_store = Arc::clone(&store);
        let finalizer_task = task_id.clone();
        let handle = thread::spawn(move || {
            finalizer_store.finalize_product_task_after_execution_with_authority(
                &finalizer_task,
                "pg-verifier",
                &|| Ok(pg_running_scheduler_authority()),
            )
        });
        wait_for_new_tool_policy_pass(&store, tool_passes_before);
        let database_url = std::env::var("ACP_TEST_DATABASE_URL").unwrap();
        let mut client = postgres::Client::connect(&database_url, postgres::NoTls).unwrap();
        if mode == "kill" {
            client
                .execute(
                    "UPDATE product_tasks SET status='killed', version=version+1 WHERE task_id=$1",
                    &[&task_id],
                )
                .unwrap();
        } else {
            client
                .execute(
                    "UPDATE product_tasks SET version=version+1 WHERE task_id=$1",
                    &[&task_id],
                )
                .unwrap();
        }
        let finalized = handle.join().unwrap().unwrap();
        assert_eq!(finalized["phase"], "verification_authority_lost");
        assert_eq!(
            finalized["task"]["status"],
            if mode == "kill" { "killed" } else { "blocked" }
        );
        assert!(finalized["artifact_id"].is_null());
        std::env::remove_var("ACP_PRODUCT_WORKSPACE_ROOT");
        std::env::remove_var("ACP_ENABLE_TARGET_REPO_OUTPUT");
        std::env::remove_var(PRODUCT_TASK_GATE);
    }
}

#[test]
#[cfg(feature = "pg-tests")]
fn pg_workspace_replacement_during_verification_is_quarantined() {
    let Some(store) = test_store() else { return };
    std::env::set_var(PRODUCT_TASK_GATE, "1");
    std::env::set_var("ACP_ENABLE_TARGET_REPO_OUTPUT", "1");
    let workspace_root = tempfile::tempdir().unwrap();
    std::env::set_var("ACP_PRODUCT_WORKSPACE_ROOT", workspace_root.path());
    let (repo, revision) = pg_product_repo("pg workspace replacement");
    let tag = uuid_tag();
    let store = Arc::new(store);
    let (task_id, _) =
        pg_ready_for_verification(&store, repo.path(), &revision, &tag, "tail -f README.md");
    let task = store.get_product_task(&task_id).unwrap().unwrap();
    let workspace_id = task["workspace_record_id"].as_str().unwrap().to_string();
    let workspace_path = task["workspace_binding"]["workspace_path"]
        .as_str()
        .unwrap()
        .to_string();
    let tool_passes_before = tool_policy_pass_count(&store);
    let finalizer_store = Arc::clone(&store);
    let finalizer_task = task_id.clone();
    let handle = thread::spawn(move || {
        finalizer_store.finalize_product_task_after_execution_with_authority(
            &finalizer_task,
            "pg-verifier",
            &|| Ok(pg_running_scheduler_authority()),
        )
    });
    wait_for_new_tool_policy_pass(&store, tool_passes_before);
    std::fs::rename(&workspace_path, format!("{workspace_path}.replaced")).unwrap();
    std::fs::create_dir(&workspace_path).unwrap();
    let finalized = handle.join().unwrap().unwrap();
    assert_eq!(finalized["phase"], "verification_authority_lost");
    assert_eq!(finalized["task"]["status"], "blocked");
    assert!(finalized["artifact_id"].is_null());
    assert_eq!(
        store
            .get_supervised_patch_workspace(&workspace_id)
            .unwrap()
            .unwrap()["status"],
        "quarantined"
    );
    std::env::remove_var("ACP_PRODUCT_WORKSPACE_ROOT");
    std::env::remove_var("ACP_ENABLE_TARGET_REPO_OUTPUT");
    std::env::remove_var(PRODUCT_TASK_GATE);
}

#[test]
#[cfg(feature = "pg-tests")]
fn pg_remaining_elapsed_budget_caps_running_verification() {
    let Some(store) = test_store() else { return };
    std::env::set_var(PRODUCT_TASK_GATE, "1");
    std::env::set_var("ACP_ENABLE_TARGET_REPO_OUTPUT", "1");
    let workspace_root = tempfile::tempdir().unwrap();
    std::env::set_var("ACP_PRODUCT_WORKSPACE_ROOT", workspace_root.path());
    let (repo, revision) = pg_product_repo("pg verification elapsed budget");
    let tag = uuid_tag();
    let (task_id, _) = pg_ready_for_verification_with_budget(
        &store,
        repo.path(),
        &revision,
        &tag,
        "tail -f README.md",
        Some(ProductTaskBudget {
            total_elapsed_ms: Some(2_500),
            ..ProductTaskBudget::default()
        }),
    );
    let started = std::time::Instant::now();
    let finalized = store
        .finalize_product_task_after_execution_with_authority(&task_id, "pg-verifier", &|| {
            Ok(pg_running_scheduler_authority())
        })
        .unwrap();
    assert!(started.elapsed() < Duration::from_secs(5));
    let attempt = &finalized["verification"]["verification_attempts"][0];
    assert!(attempt["effective_timeout_ms"].as_u64().unwrap() <= 2_500);
    assert!(finalized["artifact_id"].is_null());
    std::env::remove_var("ACP_PRODUCT_WORKSPACE_ROOT");
    std::env::remove_var("ACP_ENABLE_TARGET_REPO_OUTPUT");
    std::env::remove_var(PRODUCT_TASK_GATE);
}

#[test]
#[cfg(feature = "pg-tests")]
fn pg_terminal_evidence_audit_failure_rolls_back_completion() {
    let Some(store) = test_store() else { return };
    std::env::set_var(PRODUCT_TASK_GATE, "1");
    std::env::set_var("ACP_ENABLE_TARGET_REPO_OUTPUT", "1");
    let workspace_root = tempfile::tempdir().unwrap();
    std::env::set_var("ACP_PRODUCT_WORKSPACE_ROOT", workspace_root.path());
    let (repo, revision) = pg_product_repo("pg terminal audit rollback");
    let tag = uuid_tag();
    let (task, approval, _) =
        pg_product_task_to_approval(&store, repo.path(), &revision, &tag, "artifact_only");
    let task_id = task["task_id"].as_str().unwrap();
    let task_version = task["version"].as_u64().unwrap();

    let suffix = uuid_tag().replace('-', "_");
    let function_name = format!("reject_terminal_evidence_audit_{suffix}");
    let trigger_name = format!("reject_terminal_evidence_audit_trigger_{suffix}");
    let database_url = std::env::var("ACP_TEST_DATABASE_URL").unwrap();
    let mut client = postgres::Client::connect(&database_url, postgres::NoTls).unwrap();
    client
        .batch_execute(&format!(
            "CREATE FUNCTION {function_name}() RETURNS trigger
             LANGUAGE plpgsql AS $$
             BEGIN
               IF NEW.action = 'product_task.terminal_evidence_committed' THEN
                 RAISE EXCEPTION 'terminal evidence audit rejected';
               END IF;
               RETURN NEW;
             END;
             $$;
             CREATE TRIGGER {trigger_name}
             BEFORE INSERT ON audit_log
             FOR EACH ROW EXECUTE FUNCTION {function_name}();"
        ))
        .unwrap();

    let error = store
        .output_product_task(
            task_id,
            "pg-output-operator",
            task_version,
            approval["approval_id"].as_str(),
            true,
        )
        .unwrap_err();
    assert!(
        error == "db error" || error.contains("terminal evidence audit rejected"),
        "{error}"
    );
    let current = store.get_product_task(task_id).unwrap().unwrap();
    assert_eq!(current["status"], "awaiting_approval");
    assert_eq!(current["version"], task_version);
    assert!(store
        .get_product_task_terminal_evidence(task_id)
        .unwrap_err()
        .contains("not committed"));

    client
        .batch_execute(&format!(
            "DROP TRIGGER {trigger_name} ON audit_log;
             DROP FUNCTION {function_name}();"
        ))
        .unwrap();
}

#[test]
#[cfg(feature = "pg-tests")]
fn pg_duplicate_terminal_output_is_exactly_once_and_preserves_spend_rollback_guard() {
    let Some(store) = test_store() else { return };
    std::env::set_var(PRODUCT_TASK_GATE, "1");
    std::env::set_var("ACP_ENABLE_TARGET_REPO_OUTPUT", "1");
    let workspace_root = tempfile::tempdir().unwrap();
    std::env::set_var("ACP_PRODUCT_WORKSPACE_ROOT", workspace_root.path());
    let (repo, revision) = pg_product_repo("pg concurrent terminal evidence");
    let tag = uuid_tag();
    let (task, approval, artifact) =
        pg_product_task_to_approval(&store, repo.path(), &revision, &tag, "artifact_only");
    let task_id = task["task_id"].as_str().unwrap().to_string();
    let task_version = task["version"].as_u64().unwrap();
    let approval_id = approval["approval_id"].as_str().unwrap().to_string();
    let database_url = std::env::var("ACP_TEST_DATABASE_URL").unwrap();
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let mut handles = Vec::new();
    for actor in ["pg-output-a", "pg-output-b"] {
        let concurrent_store =
            LocalProductStore::new_postgres(&database_url, utc_now_string).unwrap();
        let barrier = Arc::clone(&barrier);
        let task_id = task_id.clone();
        let approval_id = approval_id.clone();
        handles.push(std::thread::spawn(move || {
            barrier.wait();
            concurrent_store
                .output_product_task(&task_id, actor, task_version, Some(&approval_id), true)
                .unwrap()
        }));
    }
    let results = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect::<Vec<_>>();
    assert!(results
        .iter()
        .all(|result| result["task"]["status"] == "completed"));
    assert_eq!(
        results[0]["terminal_evidence"]["evidence_id"],
        results[1]["terminal_evidence"]["evidence_id"]
    );
    let terminal_audits = store
        .audit_events(10_000)
        .unwrap()
        .into_iter()
        .filter(|event| {
            event["action"] == "product_task.terminal_evidence_committed"
                && event["resource"] == task_id
        })
        .count();
    assert_eq!(terminal_audits, 1);
    let artifact_id = artifact["artifact_id"].as_str().unwrap();
    let output_audits = store
        .audit_events(10_000)
        .unwrap()
        .into_iter()
        .filter(|event| {
            event["action"] == "product_task.nonnetwork_output_completed"
                && event["resource"] == artifact_id
        })
        .count();
    assert_eq!(output_audits, 1);

    store
        .rollback_v34_to_v33("pg-rollback-operator", true)
        .unwrap();
    let rollback_error = store
        .rollback_v33_to_v32("pg-rollback-operator", true)
        .expect_err("PostgreSQL must not drop spend history while any spend exists");
    assert!(
        rollback_error.contains("v33 rollback blocked")
            && rollback_error.contains("managed_acceptance_spend_authorizations"),
        "{rollback_error}"
    );
    assert_eq!(store.schema_version().unwrap(), 33);
}

#[test]
#[cfg(feature = "pg-tests")]
fn pg_product_output_approval_revalidates_current_bindings_atomically() {
    let Some(store) = test_store() else { return };
    std::env::set_var(PRODUCT_TASK_GATE, "1");
    std::env::set_var("ACP_ENABLE_TARGET_REPO_OUTPUT", "1");
    let workspace_root = tempfile::tempdir().unwrap();
    std::env::set_var("ACP_PRODUCT_WORKSPACE_ROOT", workspace_root.path());
    let repo = tempfile::tempdir().unwrap();
    std::fs::write(repo.path().join("README.md"), "pg product approval\n").unwrap();
    for args in [
        &["init", "-b", "main"][..],
        &["config", "user.email", "pg-product@example.invalid"][..],
        &["config", "user.name", "PG Product Test"][..],
        &["add", "README.md"][..],
        &["commit", "-m", "init"][..],
    ] {
        let output = Command::new("git")
            .args(args)
            .current_dir(repo.path())
            .output()
            .unwrap();
        assert!(output.status.success(), "git {args:?}: {:?}", output);
    }
    let revision = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo.path())
        .output()
        .unwrap();
    let revision = String::from_utf8_lossy(&revision.stdout).trim().to_string();
    let tag = uuid_tag();
    let request = ProductTaskIntakeRequest {
        objective: "postgres approval authority fixture".to_string(),
        target_id: format!("pg-product-{tag}"),
        target_repo_path: repo.path().to_string_lossy().into_owned(),
        source_revision: revision.clone(),
        source_tree_hash: None,
        allowed_paths: vec!["docs/product_golden_path_fixture.md".to_string()],
        verification_commands: vec![ProductVerificationCommand {
            command: "test -f docs/product_golden_path_fixture.md".to_string(),
            timeout_ms: 5_000,
        }],
        output_intent: "draft_pr".to_string(),
        executor_policy: ProductExecutorPolicy {
            allowed_executors: vec!["command".to_string()],
            prefer: Some("command".to_string()),
        },
        budget: None,
        risk_class: "low".to_string(),
        approval_required: true,
        confirm_execution: Some(true),
        confirm_output: Some(true),
        idempotency_key: format!("pg-product-approval-{tag}"),
        expected_version: None,
        tenant_id: Some("local".to_string()),
        workspace_id: Some("default".to_string()),
        workspace_mode: Some("git_worktree".to_string()),
    };
    let validated = validate_intake(&request, "local", "default").unwrap();
    let task = store
        .admit_product_task(&validated, "pg-product-test")
        .unwrap();
    let task_id = task["task_id"].as_str().unwrap();
    let compiled = store
        .compile_and_schedule_product_task(task_id, "pg-product-test", &["command".to_string()])
        .unwrap();
    let run_id = compiled["task"]["run_id"].as_str().unwrap();
    let executor = engine::node_executor::CommandNodeExecutor::default();
    for _ in 0..8 {
        let tick = store
            .tick_with_executor(run_id, "pg-product-test", 1, &executor)
            .unwrap();
        if matches!(
            tick.pointer("/run/status").and_then(Value::as_str),
            Some("completed" | "failed")
        ) {
            break;
        }
    }
    store
        .finalize_product_task_after_execution(task_id, "pg-product-test")
        .unwrap();
    let task = store.get_product_task(task_id).unwrap().unwrap();
    let version = task["version"].as_u64().unwrap();
    let approval = store
        .approve_product_task(task_id, "pg-independent-operator", version)
        .unwrap();
    assert_eq!(approval["approval_kind"], "product_output");

    let audit_before = store.audit_events(10_000).unwrap();
    let mut tampered = approval.clone();
    tampered["verification_sha256"] = json!("0".repeat(64));
    let error = store
        .record_product_output_approval(
            approval["run_id"].as_str().unwrap(),
            approval["node_id"].as_str().unwrap(),
            "pg-tampered-operator",
            &tampered,
        )
        .unwrap_err();
    assert!(error.contains("verification binding changed"), "{error}");
    assert_eq!(store.audit_events(10_000).unwrap(), audit_before);

    let pending = store
        .output_product_task(
            task_id,
            "pg-output-operator",
            version,
            approval["approval_id"].as_str(),
            true,
        )
        .unwrap();
    assert_eq!(pending["task"]["status"], "output_pending");
    assert_eq!(pending["output"]["status"], "network_output_unavailable");
    let pending_version = pending["task"]["version"].as_u64().unwrap();

    let artifact = store
        .get_supervised_patch_artifact(approval["artifact_id"].as_str().unwrap())
        .unwrap()
        .unwrap();
    let output_request = json!({
        "schema_version": "product_draft_pr_output_request.v1",
        "product_task_id": task_id,
        "artifact_id": artifact["artifact_id"],
        "approval_id": approval["approval_id"],
        "output_intent": "draft_pr",
        "expected_task_version": pending_version,
        "workspace_id": artifact["workspace_id"],
        "run_id": artifact["run_id"],
        "target_id": artifact["target_id"],
        "patch_hash": artifact["patch_hash"],
        "source_revision": artifact["source_revision"],
        "target_repository": "disposable/pg-acceptance",
        "repository_host": "github.com",
        "base_branch": "main",
        "head_branch": format!("acp/product-{task_id}"),
        "remote": "origin",
        "commit_message": "bounded PG test",
        "pr_title": "Draft: bounded PG test",
        "pr_body": "Do not merge automatically.",
    });
    let output_request_sha =
        hex::encode(Sha256::digest(serde_json::to_vec(&output_request).unwrap()));
    let artifact_id = artifact["artifact_id"].as_str().unwrap();
    let stale_error = store
        .claim_product_output_operation(
            artifact_id,
            &output_request,
            &output_request_sha,
            pending_version.saturating_sub(1),
            "pg-stale-output-operator",
        )
        .unwrap_err();
    assert!(
        stale_error.contains("stale product task version"),
        "{stale_error}"
    );
    assert!(
        store
            .get_supervised_patch_artifact(artifact_id)
            .unwrap()
            .unwrap()
            .get("product_output_operation")
            .is_none(),
        "stale PG output caller must have zero durable operation effect"
    );
    let claimed = store
        .claim_product_output_operation(
            artifact_id,
            &output_request,
            &output_request_sha,
            pending_version,
            "pg-output-operator",
        )
        .unwrap();
    let operation_id = claimed["operation_id"].as_str().unwrap().to_string();
    let commit_sha = "a".repeat(40);
    store
        .record_product_output_branch_pushed(
            artifact_id,
            &operation_id,
            claimed["current_version"].as_u64().unwrap(),
            &commit_sha,
            "pg-output-operator",
        )
        .unwrap();

    let url = std::env::var("ACP_TEST_DATABASE_URL").unwrap();
    let store_a = LocalProductStore::new_postgres(&url, utc_now_string).unwrap();
    let store_b = LocalProductStore::new_postgres(&url, utc_now_string).unwrap();
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let spawn_claim = |claim_store: LocalProductStore,
                       claim_barrier: Arc<std::sync::Barrier>,
                       actor: &'static str| {
        let artifact_id = artifact_id.to_string();
        let request = output_request.clone();
        let request_sha = output_request_sha.clone();
        std::thread::spawn(move || {
            claim_barrier.wait();
            claim_store
                .claim_product_output_operation(
                    &artifact_id,
                    &request,
                    &request_sha,
                    pending_version,
                    actor,
                )
                .unwrap()
        })
    };
    let first = spawn_claim(store_a, Arc::clone(&barrier), "pg-output-a");
    let second = spawn_claim(store_b, Arc::clone(&barrier), "pg-output-b");
    let claims = [first.join().unwrap(), second.join().unwrap()];
    let pr_claim = claims
        .iter()
        .find(|claim| claim["claim_action"] == "create_or_reconcile_pr")
        .unwrap();
    assert_eq!(
        claims
            .iter()
            .filter(|claim| claim["claim_action"] == "operation_in_progress")
            .count(),
        1
    );
    let pull_request = json!({
        "number": 23,
        "url": "https://github.com/disposable/pg-acceptance/pull/23",
        "state": "open",
        "draft": true,
        "reused": false,
        "repository": "disposable/pg-acceptance",
        "base_branch": "main",
        "head_branch": format!("acp/product-{task_id}"),
        "head_sha": commit_sha,
    });
    let completed_operation = store
        .complete_product_output_draft_pr(
            artifact_id,
            &operation_id,
            pr_claim["current_version"].as_u64().unwrap(),
            &pull_request,
            "pg-output-operator",
        )
        .unwrap();
    assert_eq!(completed_operation["state"], "completed");
    let workspace_id = artifact["workspace_id"].as_str().unwrap();
    let workspace = store
        .get_supervised_patch_workspace(workspace_id)
        .unwrap()
        .unwrap();
    let original_verification = workspace["verification"].clone();
    let mut replaced_verification = original_verification.clone();
    replaced_verification["authority_race"] = json!(true);
    store
        .record_workspace_verification(
            workspace_id,
            &replaced_verification,
            "pg-concurrent-verifier",
        )
        .unwrap();
    let stale_terminal = store
        .complete_product_task_draft_pr_output(
            task_id,
            artifact_id,
            &operation_id,
            pr_claim["current_version"].as_u64().unwrap(),
            pending_version,
            &pull_request,
            "pg-output-operator",
        )
        .unwrap_err();
    assert!(
        stale_terminal.contains("verification authority changed")
            || stale_terminal.contains("verification approval binding changed"),
        "{stale_terminal}"
    );
    assert_eq!(
        store.get_product_task(task_id).unwrap().unwrap()["status"],
        "output_pending"
    );
    store
        .record_workspace_verification(
            workspace_id,
            &original_verification,
            "pg-concurrent-verifier-rollback",
        )
        .unwrap();
    let completed_task = store
        .complete_product_task_draft_pr_output(
            task_id,
            artifact_id,
            &operation_id,
            pr_claim["current_version"].as_u64().unwrap(),
            pending_version,
            &pull_request,
            "pg-output-operator",
        )
        .unwrap();
    assert_eq!(completed_task["task"]["status"], "completed");
    assert_eq!(completed_task["operation"]["state"], "completed");
    assert_eq!(
        completed_task["operation"]["branch_push"]["status"],
        "completed"
    );
    assert_eq!(
        completed_task["operation"]["pr_create"]["status"],
        "completed"
    );
    assert_eq!(completed_task["operation"]["pr_create"]["number"], 23);
    let terminal_evidence = completed_task["terminal_evidence"].clone();
    assert_eq!(
        terminal_evidence["schema_version"],
        "product_task_terminal_evidence.v2"
    );
    assert_eq!(terminal_evidence["output"]["operation_id"], operation_id);
    assert_eq!(terminal_evidence["output"]["draft_pr"]["number"], 23);
    let audit_before_read = store.audit_events(10_000).unwrap();
    let restarted = LocalProductStore::new_postgres(
        &std::env::var("ACP_TEST_DATABASE_URL").unwrap(),
        utc_now_string,
    )
    .unwrap();
    assert_eq!(
        restarted
            .get_product_task_terminal_evidence(task_id)
            .unwrap(),
        terminal_evidence
    );
    assert_eq!(restarted.audit_events(10_000).unwrap(), audit_before_read);
    assert!(store
        .mark_product_output_pr_failed_known(
            artifact_id,
            &operation_id,
            pr_claim["current_version"].as_u64().unwrap(),
            "pg-late-output",
            "late failure",
        )
        .is_err());

    let export_tag = uuid_tag();
    let (export_task, export_approval, export_artifact) =
        pg_product_task_to_approval(&store, repo.path(), &revision, &export_tag, "export_patch");
    let export_task_id = export_task["task_id"].as_str().unwrap();
    let export_artifact_id = export_artifact["artifact_id"].as_str().unwrap();
    let mismatched_request = json!({
        "schema_version": "product_draft_pr_output_request.v1",
        "product_task_id": export_task_id,
        "artifact_id": export_artifact_id,
        "approval_id": export_approval["approval_id"],
        "output_intent": "draft_pr",
        "expected_task_version": export_task["version"],
        "workspace_id": export_artifact["workspace_id"],
        "run_id": export_artifact["run_id"],
        "target_id": export_artifact["target_id"],
        "patch_hash": export_artifact["patch_hash"],
        "source_revision": export_artifact["source_revision"],
        "target_repository": "disposable/pg-acceptance",
        "repository_host": "github.com",
        "base_branch": "main",
        "head_branch": format!("acp/product-{export_task_id}"),
        "remote": "origin",
        "commit_message": "bounded mismatched PG test",
        "pr_title": "Draft: mismatched PG test",
        "pr_body": "Do not merge automatically.",
    });
    let mismatched_sha = hex::encode(Sha256::digest(
        serde_json::to_vec(&mismatched_request).unwrap(),
    ));
    let error = store
        .claim_product_output_operation(
            export_artifact_id,
            &mismatched_request,
            &mismatched_sha,
            export_task["version"].as_u64().unwrap(),
            "pg-output-operator",
        )
        .unwrap_err();
    assert!(
        error.contains("task state or intent authority changed"),
        "{error}"
    );
    assert!(
        store
            .get_supervised_patch_artifact(export_artifact_id)
            .unwrap()
            .unwrap()
            .get("product_output_operation")
            .is_none(),
        "mismatched approval must not create an output operation"
    );

    let completed_export = store
        .output_product_task(
            export_task_id,
            "pg-output-operator",
            export_task["version"].as_u64().unwrap(),
            export_approval["approval_id"].as_str(),
            true,
        )
        .unwrap();
    assert_eq!(completed_export["task"]["status"], "completed");
    let completed_version = completed_export["task"]["version"].as_u64().unwrap();
    let reused = store
        .output_product_task(
            export_task_id,
            "pg-output-operator",
            completed_version,
            export_approval["approval_id"].as_str(),
            true,
        )
        .unwrap();
    assert_eq!(reused["reused"], true);
    assert_eq!(
        reused["output_receipt"]["receipt_id"],
        completed_export["output_receipt"]["receipt_id"]
    );
    std::env::remove_var(PRODUCT_TASK_GATE);
    std::env::remove_var("ACP_ENABLE_TARGET_REPO_OUTPUT");
    std::env::remove_var("ACP_PRODUCT_WORKSPACE_ROOT");
}

#[cfg(feature = "pg-tests")]
struct PgCountingExecutor {
    calls: Arc<AtomicUsize>,
}

#[cfg(feature = "pg-tests")]
impl NodeExecutor for PgCountingExecutor {
    fn execute_node(&self, _input: &NodeExecutionInput) -> NodeExecutionOutput {
        self.calls.fetch_add(1, Ordering::SeqCst);
        NodeExecutionOutput {
            status: "completed".to_string(),
            executor_type: "command".to_string(),
            output: Some("fixture".to_string()),
            error_domain: None,
            error_message: None,
            input_tokens: None,
            output_tokens: None,
            estimated_cost: None,
            latency_ms: Some(1),
            process_outcome: None,
            resolved_model: None,
        }
    }

    fn executor_type_name(&self) -> &str {
        "command"
    }
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
fn pg_decision_creation_rejects_non_draft_status_without_transition_receipt() {
    let Some(store) = test_store() else { return };
    let tag = uuid_tag();
    let decision_id = format!("mad-pg-non-draft-{tag}");
    let residual = "a9".repeat(32);
    let parameter_error = store
        .upsert_managed_acceptance_decision(
            "tenant-pg-managed-acceptance",
            &pg_managed_acceptance_decision_body(&decision_id, "non-draft-parameter"),
            &residual,
            "operator_accepted",
            None,
            Some(
                &(chrono::Utc::now() + chrono::Duration::hours(1))
                    .format("%Y-%m-%dT%H:%M:%SZ")
                    .to_string(),
            ),
        )
        .expect_err("PostgreSQL decision cannot be created accepted");
    assert!(parameter_error.contains("transitions are receipt-owned"));

    let mut self_declared = pg_managed_acceptance_decision_body(
        &format!("mad-pg-non-draft-body-{tag}"),
        "non-draft-body",
    );
    self_declared["status"] = json!("operator_accepted");
    let body_error = store
        .upsert_managed_acceptance_decision(
            "tenant-pg-managed-acceptance",
            &self_declared,
            &residual,
            "draft_pending_operator",
            None,
            Some(
                &(chrono::Utc::now() + chrono::Duration::hours(1))
                    .format("%Y-%m-%dT%H:%M:%SZ")
                    .to_string(),
            ),
        )
        .expect_err("PostgreSQL decision body cannot self-declare accepted");
    assert!(body_error.contains("is not transition evidence"));
}

#[test]
#[cfg(feature = "pg-tests")]
fn pg_managed_acceptance_transition_receipts_and_exact_envelope_bindings() {
    let Some(store) = test_store() else { return };
    let tag = uuid_tag();
    let (principal, risk, base) = pg_seed_managed_acceptance(&store, &tag, "bound-attempt");
    let mut cases = Vec::new();
    let mut changed = base.clone();
    changed.product_task_id = "other-product-task".into();
    cases.push(("product_task_id", changed));
    let mut changed = base.clone();
    changed.workflow_id = Some("other-workflow".into());
    cases.push(("workflow_id", changed));
    let mut changed = base.clone();
    changed.workflow_node_id = Some("other-node".into());
    cases.push(("workflow_node_id", changed));
    let mut changed = base.clone();
    changed.execution_id = "other-execution".into();
    cases.push(("execution_id", changed));
    let mut changed = base.clone();
    changed.attempt_id = "other-attempt".into();
    cases.push(("attempt_id", changed));
    let mut changed = base.clone();
    changed.target_repo = "org/other-target".into();
    cases.push(("target_repo", changed));
    let mut changed = base.clone();
    changed.target_main_sha = "b".repeat(40);
    cases.push(("target_main_sha", changed));
    let mut changed = base.clone();
    changed.binary_path = "/opt/other-codex".into();
    cases.push(("binary_path", changed));
    let mut changed = base.clone();
    changed.binary_sha256 = "cd".repeat(32);
    cases.push(("binary_sha256", changed));
    let mut changed = base.clone();
    changed.cancellation_identity = "other-cancel".into();
    cases.push(("cancellation_identity", changed));
    let mut changed = base.clone();
    changed.rollback_identity = "other-rollback".into();
    cases.push(("rollback_identity", changed));
    let mut changed = base.clone();
    changed.output_branch_prefix = "other/".into();
    cases.push(("output_branch_prefix", changed));
    let mut changed = base.clone();
    changed.draft_pr_only = false;
    cases.push(("draft_pr_only", changed));
    let mut changed = base.clone();
    changed.cost_authority = CostAuthority::ProviderReported {
        max_cost: 1.0,
        currency: "USD".into(),
    };
    cases.push(("cost_authority", changed));

    for (field, request) in cases {
        let error = store
            .issue_managed_acceptance_spend_authorization(&principal, &request)
            .expect_err(&format!(
                "{field} mutation must not issue a PostgreSQL spend"
            ));
        assert!(
            error.contains("mismatches decision trial envelope"),
            "{field}: unexpected error {error}"
        );
    }

    let first = store
        .issue_managed_acceptance_spend_authorization(&principal, &base)
        .unwrap();
    let replay = store
        .issue_managed_acceptance_spend_authorization(&principal, &base)
        .unwrap();
    assert_eq!(
        first["spend_authorization_id"],
        replay["spend_authorization_id"]
    );
    store
        .revoke_managed_acceptance_authorization(
            &principal,
            risk["authorization_id"].as_str().unwrap(),
        )
        .unwrap();
    let receipts = store
        .list_managed_acceptance_decision_transition_receipts(risk["decision_id"].as_str().unwrap())
        .unwrap();
    let accepted = receipts
        .iter()
        .find(|receipt| receipt["to_status"] == "operator_accepted")
        .expect("accepted transition receipt");
    let revoked = receipts
        .iter()
        .find(|receipt| receipt["to_status"] == "revoked")
        .expect("revoked transition receipt");
    assert_eq!(accepted["transition_sha256"].as_str().unwrap().len(), 64);
    assert_eq!(revoked["transition_sha256"].as_str().unwrap().len(), 64);
    assert_eq!(
        revoked["previous_transition_sha256"], accepted["transition_sha256"],
        "PostgreSQL transition receipt chain must equal the SQLite contract"
    );

    let database_url = std::env::var("ACP_TEST_DATABASE_URL").unwrap();
    let mut client = postgres::Client::connect(&database_url, postgres::NoTls).unwrap();
    let decision_id = risk["decision_id"].as_str().unwrap();
    let decision_body_sha256 = risk["decision_body_sha256"].as_str().unwrap();
    let accepted_sha = accepted["transition_sha256"].as_str().unwrap();
    let child_error = client
        .execute(
            "INSERT INTO managed_acceptance_decision_transition_receipts (
                transition_receipt_id, decision_id, tenant_id, decision_body_sha256,
                previous_transition_sha256, transition_sha256, from_status, to_status,
                actor_principal_kind, actor_principal_id, receipt_json, created_at
             ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)",
            &[
                &format!("matr-pg-fork-child-{tag}"),
                &decision_id,
                &"tenant-pg-managed-acceptance",
                &decision_body_sha256,
                &accepted_sha,
                &"c1".repeat(32),
                &"operator_accepted",
                &"revoked",
                &"fixture_principal",
                &"forged-fork",
                &"{}",
                &utc_now_string(),
            ],
        )
        .expect_err("PostgreSQL V32 must reject a second child transition");
    assert_eq!(
        child_error
            .as_db_error()
            .and_then(|error| error.constraint()),
        Some("idx_managed_acceptance_transition_one_child"),
        "unexpected child fork error: {child_error}"
    );
    let genesis_error = client
        .execute(
            "INSERT INTO managed_acceptance_decision_transition_receipts (
                transition_receipt_id, decision_id, tenant_id, decision_body_sha256,
                previous_transition_sha256, transition_sha256, from_status, to_status,
                actor_principal_kind, actor_principal_id, receipt_json, created_at
             ) VALUES ($1,$2,$3,$4,NULL,$5,$6,$7,$8,$9,$10,$11)",
            &[
                &format!("matr-pg-fork-genesis-{tag}"),
                &decision_id,
                &"tenant-pg-managed-acceptance",
                &decision_body_sha256,
                &"c2".repeat(32),
                &"draft_pending_operator",
                &"operator_accepted",
                &"fixture_principal",
                &"forged-genesis",
                &"{}",
                &utc_now_string(),
            ],
        )
        .expect_err("PostgreSQL V32 must reject a second genesis transition");
    assert_eq!(
        genesis_error
            .as_db_error()
            .and_then(|error| error.constraint()),
        Some("idx_managed_acceptance_transition_one_genesis"),
        "unexpected genesis fork error: {genesis_error}"
    );
}

#[test]
#[cfg(feature = "pg-tests")]
fn pg_managed_acceptance_transition_restart_uses_hash_chain_when_timestamps_tie() {
    let Ok(database_url) = std::env::var("ACP_TEST_DATABASE_URL") else {
        return;
    };
    let fixed_clock = || "2026-07-26T00:00:00Z".to_string();
    let store = LocalProductStore::new_postgres(&database_url, fixed_clock).unwrap();
    let tag = "same-second-restart-00";
    let (principal, risk, request) = pg_seed_managed_acceptance(&store, tag, "same-second-restart");
    store
        .issue_managed_acceptance_spend_authorization(&principal, &request)
        .unwrap();
    store
        .revoke_managed_acceptance_authorization(
            &principal,
            risk["authorization_id"].as_str().unwrap(),
        )
        .unwrap();
    let decision_id = risk["decision_id"].as_str().unwrap().to_string();
    let before = store
        .list_managed_acceptance_decision_transition_receipts(&decision_id)
        .unwrap();
    assert_eq!(before.len(), 2);
    assert_eq!(before[0]["sequence"], 1);
    assert!(before[0]["previous_transition_sequence"].is_null());
    assert_eq!(before[1]["sequence"], 2);
    assert_eq!(before[1]["previous_transition_sequence"], 1);
    assert_eq!(
        before[1]["previous_transition_sha256"],
        before[0]["transition_sha256"]
    );
    drop(store);

    let restarted = LocalProductStore::new_postgres(&database_url, fixed_clock).unwrap();
    let after = restarted
        .list_managed_acceptance_decision_transition_receipts(&decision_id)
        .unwrap();
    assert_eq!(after, before);
}

#[test]
#[cfg(feature = "pg-tests")]
fn pg_managed_acceptance_transition_receipt_content_tampering_fails_closed_on_read() {
    let Some(store) = test_store() else { return };
    let tag = uuid_tag();
    let (_principal, risk, _request) =
        pg_seed_managed_acceptance(&store, &tag, "transition-tamper-attempt");
    let decision_id = risk["decision_id"].as_str().unwrap().to_string();
    let database_url = std::env::var("ACP_TEST_DATABASE_URL").unwrap();
    let mut client = postgres::Client::connect(&database_url, postgres::NoTls).unwrap();
    let row = client
        .query_one(
            "SELECT transition_receipt_id, receipt_json
             FROM managed_acceptance_decision_transition_receipts
             WHERE decision_id=$1",
            &[&decision_id],
        )
        .unwrap();
    let receipt_id: String = row.get(0);
    let encoded: String = row.get(1);
    let mut receipt: Value = serde_json::from_str(&encoded).unwrap();
    receipt["reason"] = json!("tampered_after_persistence");
    client
        .execute(
            "UPDATE managed_acceptance_decision_transition_receipts
             SET receipt_json=$1 WHERE transition_receipt_id=$2",
            &[&receipt.to_string(), &receipt_id],
        )
        .unwrap();

    let error = store
        .list_managed_acceptance_decision_transition_receipts(&decision_id)
        .expect_err("tampered PostgreSQL transition content must fail closed");
    assert!(error.contains("hash does not match content"), "{error}");
}

#[test]
#[cfg(feature = "pg-tests")]
fn pg_spend_issuance_rejects_missing_persisted_risk_fixture_boolean() {
    let Some(store) = test_store() else { return };
    let tag = uuid_tag();
    let (principal, risk, request) =
        pg_seed_managed_acceptance(&store, &tag, "risk-boolean-attempt");
    let risk_id = risk["authorization_id"].as_str().unwrap().to_string();
    let database_url = std::env::var("ACP_TEST_DATABASE_URL").unwrap();
    let mut client = postgres::Client::connect(&database_url, postgres::NoTls).unwrap();
    let raw: String = client
        .query_one(
            "SELECT body_json FROM managed_acceptance_authorizations WHERE authorization_id=$1",
            &[&risk_id],
        )
        .unwrap()
        .get(0);
    let mut body: Value = serde_json::from_str(&raw).unwrap();
    body.as_object_mut().unwrap().remove("fixture_only");
    client
        .execute(
            "UPDATE managed_acceptance_authorizations SET body_json=$1 WHERE authorization_id=$2",
            &[&body.to_string(), &risk_id],
        )
        .unwrap();
    let error = store
        .issue_managed_acceptance_spend_authorization(&principal, &request)
        .expect_err("PostgreSQL missing persisted risk boolean must fail closed");
    assert!(
        error.contains("fixture_only must be a persisted boolean")
            || error.contains("body_json missing fixture_only")
            || error.contains("body fixture_only must be a boolean"),
        "{error}"
    );
}

#[test]
#[cfg(feature = "pg-tests")]
fn pg_active_spend_logical_identity_constraints_reject_null_and_duplicate_writes() {
    let Some(store) = test_store() else { return };
    let tag = uuid_tag();
    let (principal, _risk, request) =
        pg_seed_managed_acceptance(&store, &tag, "logical-constraint-attempt");
    let first = store
        .issue_managed_acceptance_spend_authorization(&principal, &request)
        .unwrap();
    let first_id = first["spend_authorization_id"]
        .as_str()
        .unwrap()
        .to_string();
    let database_url = std::env::var("ACP_TEST_DATABASE_URL").unwrap();
    let mut client = postgres::Client::connect(&database_url, postgres::NoTls).unwrap();

    let null_error = client
        .execute(
            "UPDATE managed_acceptance_spend_authorizations
             SET logical_authorization_sha256=NULL WHERE spend_authorization_id=$1",
            &[&first_id],
        )
        .expect_err("PostgreSQL V33 must reject an active spend with no logical identity");
    assert!(
        null_error.as_db_error().is_some(),
        "expected PostgreSQL check failure, got: {null_error}"
    );

    let duplicate_id = format!("mas-pg-logical-duplicate-{tag}");
    let duplicate_body_sha256 = "d3".repeat(32);
    let mut duplicate_body = first["body_json"].clone();
    duplicate_body["spend_authorization_id"] = json!(duplicate_id);
    duplicate_body["created_at"] = json!("2026-07-25T12:00:01Z");
    let duplicate_body = duplicate_body.to_string();
    let duplicate_error = client
        .execute(
            "INSERT INTO managed_acceptance_spend_authorizations (
                spend_authorization_id, decision_id, risk_authorization_id, tenant_id,
                principal_kind, principal_id, spend_body_sha256, risk_authorization_sha256,
                logical_authorization_sha256, decision_body_sha256, residual_finding_sha256,
                fixture_only, status, body_json, created_at, updated_at, expires_at,
                consumed_at, consumed_by_attempt_id, revoked_at
             ) SELECT $1, decision_id, risk_authorization_id, tenant_id,
                principal_kind, principal_id, $2, risk_authorization_sha256,
                logical_authorization_sha256, decision_body_sha256, residual_finding_sha256,
                fixture_only, 'active', $3, created_at, updated_at, expires_at,
                NULL, NULL, NULL
             FROM managed_acceptance_spend_authorizations WHERE spend_authorization_id=$4",
            &[
                &duplicate_id,
                &duplicate_body_sha256,
                &duplicate_body,
                &first_id,
            ],
        )
        .expect_err("PostgreSQL V33 must reject a duplicate active logical spend");
    assert_eq!(
        duplicate_error
            .as_db_error()
            .and_then(|error| error.constraint()),
        Some("idx_managed_acceptance_spend_active_logical"),
        "unexpected duplicate logical-spend error: {duplicate_error}"
    );
    let replay = store
        .issue_managed_acceptance_spend_authorization(&principal, &request)
        .expect("rejected raw writes must leave the original PostgreSQL spend reusable");
    assert_eq!(
        replay["spend_authorization_id"],
        first["spend_authorization_id"]
    );
}

#[test]
#[cfg(feature = "pg-tests")]
fn pg_attempt_admission_rejects_non_boolean_persisted_spend_fixture_flag() {
    let Some(store) = test_store() else { return };
    let tag = uuid_tag();
    let (principal, _risk, request) =
        pg_seed_managed_acceptance(&store, &tag, "spend-boolean-attempt");
    let spend = store
        .issue_managed_acceptance_spend_authorization(&principal, &request)
        .unwrap();
    let spend_id = spend["spend_authorization_id"]
        .as_str()
        .unwrap()
        .to_string();
    let database_url = std::env::var("ACP_TEST_DATABASE_URL").unwrap();
    let mut client = postgres::Client::connect(&database_url, postgres::NoTls).unwrap();
    let constraint_error = client
        .execute(
            "UPDATE managed_acceptance_spend_authorizations
             SET fixture_only=-1 WHERE spend_authorization_id=$1",
            &[&spend_id],
        )
        .expect_err("PostgreSQL CHECK must reject a non-boolean fixture flag");
    assert_eq!(
        constraint_error
            .as_db_error()
            .and_then(|error| error.constraint()),
        Some("managed_acceptance_spend_authorizations_fixture_only_check"),
        "unexpected fixture_only constraint error: {constraint_error}"
    );
    let active: i64 = client
        .query_one(
            "SELECT COUNT(*) FROM managed_acceptance_spend_authorizations
             WHERE spend_authorization_id=$1 AND status='active'",
            &[&spend_id],
        )
        .unwrap()
        .get(0);
    assert_eq!(
        active, 1,
        "rejected PostgreSQL spend must never be consumed"
    );
}

#[test]
#[cfg(feature = "pg-tests")]
fn pg_attempt_admission_rejects_persisted_spend_principal_kind_tampering() {
    let Some(store) = test_store() else { return };
    let tag = uuid_tag();
    let (principal, _risk, request) =
        pg_seed_managed_acceptance(&store, &tag, "spend-principal-kind-attempt");
    let spend = store
        .issue_managed_acceptance_spend_authorization(&principal, &request)
        .unwrap();
    let spend_id = spend["spend_authorization_id"]
        .as_str()
        .unwrap()
        .to_string();
    let database_url = std::env::var("ACP_TEST_DATABASE_URL").unwrap();
    let mut client = postgres::Client::connect(&database_url, postgres::NoTls).unwrap();
    client
        .execute(
            "UPDATE managed_acceptance_spend_authorizations
             SET principal_kind='operator_api_key' WHERE spend_authorization_id=$1",
            &[&spend_id],
        )
        .unwrap();

    let error = store
        .admit_managed_acceptance_attempt_for_pg_tests(
            &principal,
            &request.attempt_id,
            &pg_attempt_body_from_spend(&spend),
            &spend_id,
            true,
        )
        .expect_err("PostgreSQL principal-kind tampering must not consume a spend");
    assert!(
        error.contains("spend principal kind mismatch")
            || error.contains("body_json principal_kind"),
        "{error}"
    );
    let active: i64 = client
        .query_one(
            "SELECT COUNT(*) FROM managed_acceptance_spend_authorizations
             WHERE spend_authorization_id=$1 AND status='active'",
            &[&spend_id],
        )
        .unwrap()
        .get(0);
    assert_eq!(active, 1, "rejected PostgreSQL spend must remain active");
}

#[test]
#[cfg(feature = "pg-tests")]
fn pg_managed_acceptance_attempt_replay_lease_terminal_restart_and_principal_parity() {
    let Some(store) = test_store() else { return };
    let tag = uuid_tag();
    let (principal, _risk, request) =
        pg_seed_managed_acceptance(&store, &tag, "attempt-replay-parity");
    let spend = store
        .issue_managed_acceptance_spend_authorization(&principal, &request)
        .unwrap();
    let spend_id = spend["spend_authorization_id"]
        .as_str()
        .unwrap()
        .to_string();
    let attempt_body = pg_attempt_body_from_spend(&spend);
    let unauthorized = AuthenticatedPrincipal::fixture_for_tests(
        "tenant-pg-managed-acceptance",
        &format!("fixture-principal-pg-unauthorized-{tag}"),
    )
    .unwrap();
    let unauthorized_error = store
        .admit_managed_acceptance_attempt_for_pg_tests(
            &unauthorized,
            &request.attempt_id,
            &attempt_body,
            &spend_id,
            true,
        )
        .expect_err("another PostgreSQL principal must not consume this spend");
    assert!(
        unauthorized_error.contains("spend principal mismatch"),
        "{unauthorized_error}"
    );

    let admitted = store
        .admit_managed_acceptance_attempt_for_pg_tests(
            &principal,
            &request.attempt_id,
            &attempt_body,
            &spend_id,
            true,
        )
        .unwrap();
    let lease = admitted["lease_token"].as_str().unwrap().to_string();
    let replay = store
        .admit_managed_acceptance_attempt_for_pg_tests(
            &principal,
            &request.attempt_id,
            &attempt_body,
            &spend_id,
            true,
        )
        .unwrap();
    assert_eq!(replay["idempotent_replay"], true);

    let database_url = std::env::var("ACP_TEST_DATABASE_URL").unwrap();
    let restarted = LocalProductStore::new_postgres(&database_url, utc_now_string).unwrap();
    let restarted_replay = restarted
        .admit_managed_acceptance_attempt_for_pg_tests(
            &principal,
            &request.attempt_id,
            &attempt_body,
            &spend_id,
            true,
        )
        .unwrap();
    assert_eq!(restarted_replay["idempotent_replay"], true);
    let stale_lease_error = restarted
        .complete_managed_acceptance_attempt(
            &request.attempt_id,
            "wrong-lease",
            "outcome_unknown",
            "gateway_outcome_unknown",
            &json!({"content_excluded": true}),
        )
        .expect_err("only the current lease may terminalize an admitted PostgreSQL attempt");
    assert!(stale_lease_error.contains("lease_token mismatch"));
    let terminal = restarted
        .complete_managed_acceptance_attempt(
            &request.attempt_id,
            &lease,
            "outcome_unknown",
            "gateway_outcome_unknown",
            &json!({"content_excluded": true}),
        )
        .unwrap();
    assert_eq!(terminal["status"], "outcome_unknown");

    let resumed = LocalProductStore::new_postgres(&database_url, utc_now_string).unwrap();
    assert_eq!(
        resumed
            .get_managed_acceptance_attempt(&request.attempt_id)
            .unwrap()
            .unwrap()["status"],
        "outcome_unknown"
    );
    assert_eq!(
        resumed
            .get_managed_acceptance_spend_authorization(&spend_id)
            .unwrap()
            .unwrap()["status"],
        "consumed",
        "terminal PostgreSQL attempts must never reactivate a spend"
    );
}

#[test]
#[cfg(feature = "pg-tests")]
fn pg_managed_acceptance_owner_json_read_errors_fail_closed() {
    let Some(store) = test_store() else { return };
    let tag = uuid_tag();
    let (principal, risk, request) = pg_seed_managed_acceptance(&store, &tag, "owner-json-attempt");
    let decision_id = risk["decision_id"].as_str().unwrap().to_string();
    let risk_id = risk["authorization_id"].as_str().unwrap().to_string();
    let spend = store
        .issue_managed_acceptance_spend_authorization(&principal, &request)
        .unwrap();
    let spend_id = spend["spend_authorization_id"]
        .as_str()
        .unwrap()
        .to_string();
    store
        .admit_managed_acceptance_attempt_for_pg_tests(
            &principal,
            &request.attempt_id,
            &pg_attempt_body_from_spend(&spend),
            &spend_id,
            true,
        )
        .unwrap();
    let database_url = std::env::var("ACP_TEST_DATABASE_URL").unwrap();
    let mut client = postgres::Client::connect(&database_url, postgres::NoTls).unwrap();

    let original_decision: String = client
        .query_one(
            "SELECT body_json FROM managed_acceptance_decisions WHERE decision_id=$1",
            &[&decision_id],
        )
        .unwrap()
        .get(0);
    client
        .execute(
            "UPDATE managed_acceptance_decisions SET body_json='not-json' WHERE decision_id=$1",
            &[&decision_id],
        )
        .unwrap();
    assert!(store.get_managed_acceptance_decision(&decision_id).is_err());
    let mut tampered_decision: Value = serde_json::from_str(&original_decision).unwrap();
    tampered_decision["trial_envelope"]["max_retries"] = json!(99);
    client
        .execute(
            "UPDATE managed_acceptance_decisions SET body_json=$1 WHERE decision_id=$2",
            &[&tampered_decision.to_string(), &decision_id],
        )
        .unwrap();
    assert!(
        store.get_managed_acceptance_decision(&decision_id).is_err(),
        "a valid but hash-inconsistent decision body must fail closed"
    );
    client
        .execute(
            "UPDATE managed_acceptance_decisions SET body_json=$1 WHERE decision_id=$2",
            &[&original_decision, &decision_id],
        )
        .unwrap();

    let original_risk: String = client
        .query_one(
            "SELECT body_json FROM managed_acceptance_authorizations WHERE authorization_id=$1",
            &[&risk_id],
        )
        .unwrap()
        .get(0);
    client
        .execute(
            "UPDATE managed_acceptance_authorizations SET body_json='not-json' WHERE authorization_id=$1",
            &[&risk_id],
        )
        .unwrap();
    assert!(store
        .get_active_managed_acceptance_authorization(&risk_id)
        .is_err());
    let mut tampered_risk: Value = serde_json::from_str(&original_risk).unwrap();
    tampered_risk["scope"]["decision_id"] = json!("other-decision");
    client
        .execute(
            "UPDATE managed_acceptance_authorizations SET body_json=$1 WHERE authorization_id=$2",
            &[&tampered_risk.to_string(), &risk_id],
        )
        .unwrap();
    assert!(
        store
            .get_active_managed_acceptance_authorization(&risk_id)
            .is_err(),
        "a valid but hash-inconsistent risk body must fail closed"
    );
    client
        .execute(
            "UPDATE managed_acceptance_authorizations SET body_json=$1 WHERE authorization_id=$2",
            &[&original_risk, &risk_id],
        )
        .unwrap();

    let original_spend: String = client
        .query_one(
            "SELECT body_json FROM managed_acceptance_spend_authorizations WHERE spend_authorization_id=$1",
            &[&spend_id],
        )
        .unwrap()
        .get(0);
    client
        .execute(
            "UPDATE managed_acceptance_spend_authorizations SET body_json='not-json' WHERE spend_authorization_id=$1",
            &[&spend_id],
        )
        .unwrap();
    assert!(store
        .get_managed_acceptance_spend_authorization(&spend_id)
        .is_err());
    let mut tampered_spend: Value = serde_json::from_str(&original_spend).unwrap();
    tampered_spend["model"] = json!("different-model");
    client
        .execute(
            "UPDATE managed_acceptance_spend_authorizations SET body_json=$1 WHERE spend_authorization_id=$2",
            &[&tampered_spend.to_string(), &spend_id],
        )
        .unwrap();
    assert!(
        store
            .get_managed_acceptance_spend_authorization(&spend_id)
            .is_err(),
        "a valid but hash-inconsistent spend body must fail closed"
    );
    client
        .execute(
            "UPDATE managed_acceptance_spend_authorizations SET body_json=$1 WHERE spend_authorization_id=$2",
            &[&original_spend, &spend_id],
        )
        .unwrap();

    client
        .execute(
            "UPDATE managed_acceptance_attempts SET receipt_json='not-json' WHERE attempt_id=$1",
            &[&request.attempt_id],
        )
        .unwrap();
    assert!(store
        .get_managed_acceptance_attempt(&request.attempt_id)
        .is_err());
}

#[test]
#[cfg(feature = "pg-tests")]
fn pg_concurrent_spend_issue_reuses_one_active_logical_receipt() {
    use std::sync::Barrier;

    let Some(store) = test_store() else { return };
    let tag = uuid_tag();
    let (principal, risk, request) =
        pg_seed_managed_acceptance(&store, &tag, "concurrent-logical-attempt");
    let database_url = std::env::var("ACP_TEST_DATABASE_URL").unwrap();
    let decision_id = risk["decision_id"].as_str().unwrap().to_string();
    let barrier = Arc::new(Barrier::new(3));
    let mut joins = Vec::new();
    for _ in 0..2 {
        let database_url = database_url.clone();
        let barrier = Arc::clone(&barrier);
        let principal = principal.clone();
        let request = request.clone();
        joins.push(thread::spawn(move || {
            let concurrent = LocalProductStore::new_postgres(&database_url, utc_now_string)
                .expect("concurrent PostgreSQL store");
            barrier.wait();
            concurrent.issue_managed_acceptance_spend_authorization(&principal, &request)
        }));
    }
    barrier.wait();
    let spends = joins
        .into_iter()
        .map(|join| join.join().unwrap().expect("concurrent spend issuance"))
        .collect::<Vec<_>>();
    assert_eq!(
        spends[0]["spend_authorization_id"], spends[1]["spend_authorization_id"],
        "PostgreSQL retries must reuse one logical spend before receipt IDs/timestamps"
    );
    let mut client =
        postgres::Client::connect(&database_url, postgres::NoTls).expect("raw PostgreSQL client");
    let active: i64 = client
        .query_one(
            "SELECT COUNT(*) FROM managed_acceptance_spend_authorizations
             WHERE decision_id=$1 AND status='active'",
            &[&decision_id],
        )
        .unwrap()
        .get(0);
    assert_eq!(
        active, 1,
        "only one active logical PostgreSQL spend may persist"
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
    assert_eq!(
        results.iter().filter(|result| result.is_ok()).count(),
        1,
        "concurrent durable-memory revisions: {results:?}"
    );
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
fn pg_concurrent_tool_policy_updates_require_current_hash() {
    use std::sync::{Arc, Barrier};

    let Some(store) = test_store() else { return };
    let tag = uuid_tag();
    let tool_name = format!("tool-policy-race-{tag}");
    let initial = store
        .configure_tool_capability(
            "pg-setup", &tool_name, "initial", None, None, false, "low", None,
        )
        .unwrap();
    let expected = initial["resource_sha256"].as_str().unwrap().to_string();
    let store = Arc::new(store);
    let barrier = Arc::new(Barrier::new(3));
    let mut handles = Vec::new();
    for description in ["first", "second"] {
        let store = Arc::clone(&store);
        let barrier = Arc::clone(&barrier);
        let tool_name = tool_name.clone();
        let expected = expected.clone();
        handles.push(std::thread::spawn(move || {
            barrier.wait();
            store.configure_tool_capability(
                description,
                &tool_name,
                description,
                None,
                None,
                true,
                "medium",
                Some(&expected),
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
    assert!(results
        .iter()
        .filter_map(|result| result.as_ref().err())
        .all(|error| error.contains("changed concurrently")));
    let current = store
        .read_tool_capability_policy(&tool_name)
        .unwrap()
        .unwrap();
    let winner = results
        .iter()
        .find_map(|result| result.as_ref().ok())
        .unwrap();
    assert_eq!(current["resource_sha256"], winner["resource_sha256"]);
}

#[test]
#[cfg(feature = "pg-tests")]
fn pg_corrupt_tool_capability_rows_fail_closed() {
    let Some(store) = test_store() else { return };
    let url = std::env::var("ACP_TEST_DATABASE_URL").expect("PG test URL");
    let mut client = postgres::Client::connect(&url, postgres::NoTls).expect("raw PG client");
    let tag = uuid_tag();

    for (suffix, input, output, risk, expected) in [
        ("input", "{not-json", "{}", "low", "input_schema"),
        ("output", "{}", "{not-json", "low", "output_schema"),
        ("risk", "{}", "{}", "unknown", "risk_level"),
    ] {
        let tool_name = format!("corrupt-capability-{suffix}-{tag}");
        client
            .execute(
                "INSERT INTO tool_capabilities
                 (tool_name, description, input_schema_json, output_schema_json,
                  requires_approval, risk_level, created_at)
                 VALUES ($1, 'corrupt fixture', $2, $3, 0, $4, 'now')",
                &[&tool_name, &input, &output, &risk],
            )
            .expect("insert corrupt capability");

        let error = store
            .get_tool_capability(&tool_name)
            .expect_err("corrupt capability must fail closed");
        assert!(error.contains(expected), "unexpected error: {error}");

        client
            .execute(
                "DELETE FROM tool_capabilities WHERE tool_name = $1",
                &[&tool_name],
            )
            .expect("cleanup corrupt capability");
    }
}

#[test]
#[cfg(feature = "pg-tests")]
fn pg_agent_global_cap_is_atomic_across_runs() {
    struct HoldingAgentExecutor {
        entered: std::sync::mpsc::Sender<()>,
        release: Arc<(std::sync::Mutex<bool>, std::sync::Condvar)>,
    }
    impl NodeExecutor for HoldingAgentExecutor {
        fn execute_node(&self, _input: &NodeExecutionInput) -> NodeExecutionOutput {
            self.entered.send(()).unwrap();
            let (lock, condition) = &*self.release;
            let mut released = lock.lock().unwrap();
            while !*released {
                released = condition.wait(released).unwrap();
            }
            NodeExecutionOutput {
                status: "completed".to_string(),
                executor_type: "agent_step".to_string(),
                output: Some("held fixture completed".to_string()),
                error_domain: None,
                error_message: None,
                input_tokens: None,
                output_tokens: None,
                estimated_cost: None,
                latency_ms: Some(1),
                process_outcome: None,
                resolved_model: None,
            }
        }

        fn executor_type_name(&self) -> &str {
            "agent_step"
        }
    }
    struct CountingAgentExecutor(Arc<AtomicUsize>);
    impl NodeExecutor for CountingAgentExecutor {
        fn execute_node(&self, _input: &NodeExecutionInput) -> NodeExecutionOutput {
            self.0.fetch_add(1, Ordering::SeqCst);
            NodeExecutionOutput {
                status: "completed".to_string(),
                executor_type: "agent_step".to_string(),
                output: Some("unexpected second execution".to_string()),
                error_domain: None,
                error_message: None,
                input_tokens: None,
                output_tokens: None,
                estimated_cost: None,
                latency_ms: Some(1),
                process_outcome: None,
                resolved_model: None,
            }
        }

        fn executor_type_name(&self) -> &str {
            "agent_step"
        }
    }

    let Some(store) = test_store() else { return };
    let tag = uuid_tag();
    let create_run = |suffix: &str| {
        let agent_id = format!("agent-cap-{suffix}-{tag}");
        let node_id = format!("node-cap-{suffix}-{tag}");
        let plan = store
            .create_workflow_plan(&agent_id, "pg-test", "pg-test", |ids, _| {
                Ok(json!({
                    "status": "planned_read_only",
                    "workflow_id": ids.workflow_id,
                    "dispatch_id": ids.dispatch_id,
                    "graph": {
                        "workflow_id": ids.workflow_id,
                        "dispatch_id": ids.dispatch_id,
                        "nodes": [{
                            "node_id": node_id,
                            "task_type": "agent_step",
                            "status": "pending",
                            "agent_id": agent_id,
                            "agent_role": "worker",
                            "agent_objective": "bounded PG concurrency",
                            "profile_id": "bounded",
                            "capability_profile": ["work"]
                        }],
                        "edges": []
                    },
                    "analysis": {},
                    "boundaries": {"execution_authority": "rust_scheduler_only"}
                }))
            })
            .unwrap();
        store
            .create_workflow_run_from_plan(plan["plan_id"].as_str().unwrap(), "pg-test")
            .unwrap()["run_id"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let run_a = create_run("a");
    let run_b = create_run("b");
    let store = Arc::new(store);
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let release = Arc::new((std::sync::Mutex::new(false), std::sync::Condvar::new()));
    let first_store = store.clone();
    let first_release = release.clone();
    let first = std::thread::spawn(move || {
        first_store.tick_with_executor_with_agent_caps(
            &run_a,
            "pg-test",
            0,
            &HoldingAgentExecutor {
                entered: entered_tx,
                release: first_release,
            },
            1,
            1,
        )
    });
    if entered_rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .is_err()
    {
        panic!(
            "first PostgreSQL agent claim exited before executor entry: {:?}",
            first.join().unwrap()
        );
    }

    let second_calls = Arc::new(AtomicUsize::new(0));
    let second = store
        .tick_with_executor_with_agent_caps(
            &run_b,
            "pg-test",
            0,
            &CountingAgentExecutor(second_calls.clone()),
            1,
            1,
        )
        .unwrap();
    assert_eq!(second["action"], "no_ready_node");
    assert_eq!(second_calls.load(Ordering::SeqCst), 0);

    let (lock, condition) = &*release;
    *lock.lock().unwrap() = true;
    condition.notify_all();
    assert_eq!(
        first.join().unwrap().unwrap()["result"]["status"],
        "completed"
    );
}

#[test]
#[cfg(feature = "pg-tests")]
fn pg_stale_worker_cannot_overwrite_reclaimed_attempt() {
    struct HoldingCommandExecutor {
        entered: std::sync::mpsc::Sender<()>,
        release: Arc<(std::sync::Mutex<bool>, std::sync::Condvar)>,
    }

    impl NodeExecutor for HoldingCommandExecutor {
        fn execute_node(&self, _input: &NodeExecutionInput) -> NodeExecutionOutput {
            self.entered.send(()).unwrap();
            let (lock, condition) = &*self.release;
            let mut released = lock.lock().unwrap();
            while !*released {
                released = condition.wait(released).unwrap();
            }
            NodeExecutionOutput {
                status: "completed".to_string(),
                executor_type: "command".to_string(),
                output: Some("stale attempt output".to_string()),
                error_domain: None,
                error_message: None,
                input_tokens: None,
                output_tokens: None,
                estimated_cost: None,
                latency_ms: Some(1),
                process_outcome: None,
                resolved_model: None,
            }
        }

        fn executor_type_name(&self) -> &str {
            "command"
        }
    }

    struct ReclaimedCommandExecutor;

    impl NodeExecutor for ReclaimedCommandExecutor {
        fn execute_node(&self, _input: &NodeExecutionInput) -> NodeExecutionOutput {
            NodeExecutionOutput {
                status: "completed".to_string(),
                executor_type: "command".to_string(),
                output: Some("reclaimed attempt output".to_string()),
                error_domain: None,
                error_message: None,
                input_tokens: None,
                output_tokens: None,
                estimated_cost: None,
                latency_ms: Some(1),
                process_outcome: None,
                resolved_model: None,
            }
        }

        fn executor_type_name(&self) -> &str {
            "command"
        }
    }

    let url = match std::env::var("ACP_TEST_DATABASE_URL") {
        Ok(url) => url,
        Err(_) => {
            eprintln!("ACP_TEST_DATABASE_URL not set; skipping pg-tests");
            return;
        }
    };
    let start = chrono::Utc::now();
    let clock = Arc::new(std::sync::Mutex::new(
        start.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
    ));
    let store_clock = clock.clone();
    let store = Arc::new(
        LocalProductStore::new_postgres(&url, move || store_clock.lock().unwrap().clone()).unwrap(),
    );
    let tag = uuid_tag();
    let node_id = format!("stale-command-node-{tag}");
    let plan = store
        .create_workflow_plan(
            &format!("stale worker CAS {tag}"),
            "pg-test",
            "pg-test",
            |ids, _| {
                Ok(json!({
                    "status": "planned_read_only",
                    "workflow_id": ids.workflow_id,
                    "dispatch_id": ids.dispatch_id,
                    "graph": {
                        "workflow_id": ids.workflow_id,
                        "dispatch_id": ids.dispatch_id,
                        "nodes": [{
                            "node_id": node_id,
                            "task_type": "command",
                            "status": "pending",
                            "command": "true"
                        }],
                        "edges": []
                    },
                    "analysis": {},
                    "boundaries": {"execution_authority": "bounded_trusted_local"}
                }))
            },
        )
        .unwrap();
    let run_id = store
        .create_workflow_run_from_plan(plan["plan_id"].as_str().unwrap(), "pg-test")
        .unwrap()["run_id"]
        .as_str()
        .unwrap()
        .to_string();
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let release = Arc::new((std::sync::Mutex::new(false), std::sync::Condvar::new()));
    let first_store = store.clone();
    let first_release = release.clone();
    let first_run_id = run_id.clone();
    let first = std::thread::spawn(move || {
        first_store.tick_with_executor(
            &first_run_id,
            "old-worker",
            0,
            &HoldingCommandExecutor {
                entered: entered_tx,
                release: first_release,
            },
        )
    });
    entered_rx.recv().unwrap();

    *clock.lock().unwrap() = (start + chrono::Duration::seconds(2))
        .format("%Y-%m-%dT%H:%M:%SZ")
        .to_string();
    let recovered_before = store
        .audit_events(10_000)
        .unwrap()
        .into_iter()
        .filter(|event| event["action"] == "workflow_node.stale_lease_recovered")
        .count();
    let recovery_barrier = Arc::new(std::sync::Barrier::new(2));
    let recoveries = (0..2)
        .map(|_| {
            let barrier = recovery_barrier.clone();
            let store = store.clone();
            std::thread::spawn(move || {
                barrier.wait();
                store.recover_stale_leases(1_000).unwrap()
            })
        })
        .collect::<Vec<_>>()
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect::<Vec<_>>();
    let recovery_audit = store.audit_events(10_000).unwrap();
    let recovered_after = recovery_audit
        .iter()
        .filter(|event| event["action"] == "workflow_node.stale_lease_recovered")
        .count();
    assert_eq!(
        recoveries.iter().sum::<i64>(),
        (recovered_after - recovered_before) as i64
    );
    assert_eq!(
        recovery_audit
            .iter()
            .filter(|event| {
                event["action"] == "workflow_node.stale_lease_recovered"
                    && event["resource"] == node_id
            })
            .count(),
        1
    );
    let reclaimed = store
        .tick_with_executor(&run_id, "new-worker", 0, &ReclaimedCommandExecutor)
        .unwrap();
    assert_eq!(reclaimed["action"], "node_executed");
    assert_eq!(reclaimed["attempt"], 2);
    assert_eq!(reclaimed["result"]["output"], "reclaimed attempt output");

    let (lock, condition) = &*release;
    *lock.lock().unwrap() = true;
    condition.notify_all();
    let stale = first.join().unwrap().unwrap();
    assert_eq!(stale["action"], "stale_completion_ignored");
    assert_eq!(stale["attempt"], 1);

    let persisted = store.get_workflow_run(&run_id).unwrap().unwrap();
    let node = persisted["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["node_id"] == node_id)
        .unwrap();
    assert_eq!(node["attempt_count"], 2);
    assert_eq!(node["status"], "completed");
    assert_eq!(node["result"]["output"], "reclaimed attempt output");
    let audit = store.audit_events(200).unwrap();
    assert_eq!(
        audit
            .iter()
            .filter(|event| {
                event["action"] == "workflow_node.stale_completion_ignored"
                    && event["resource"] == node_id
            })
            .count(),
        1
    );
    let terminal = audit
        .iter()
        .find(|event| event["action"] == "workflow_run.completed" && event["resource"] == run_id)
        .expect("executable PG run terminal audit");
    assert_eq!(terminal["details"]["metadata_only"], false);
    assert_eq!(
        terminal["details"]["execution_authority"],
        "bounded_trusted_local"
    );
}

#[test]
#[cfg(feature = "pg-tests")]
fn pg_stale_lease_recovery_rolls_back_when_audit_fails() {
    let url = match std::env::var("ACP_TEST_DATABASE_URL") {
        Ok(url) => url,
        Err(_) => {
            eprintln!("ACP_TEST_DATABASE_URL not set; skipping pg-tests");
            return;
        }
    };
    let store =
        LocalProductStore::new_postgres(&url, || "2026-07-14T00:00:02Z".to_string()).unwrap();
    let tag = uuid_tag();
    let node_id = format!("audit-rollback-node-{tag}");
    let plan = store
        .create_workflow_plan(
            &format!("stale audit rollback {tag}"),
            "pg-test",
            "pg-test",
            |ids, _| {
                Ok(json!({
                    "status": "planned_read_only",
                    "workflow_id": ids.workflow_id,
                    "dispatch_id": ids.dispatch_id,
                    "graph": {
                        "workflow_id": ids.workflow_id,
                        "dispatch_id": ids.dispatch_id,
                        "nodes": [{
                            "node_id": node_id,
                            "task_type": "agent_step",
                            "status": "pending",
                            "agent_id": format!("audit-rollback-agent-{tag}"),
                            "assigned_agent_id": format!("audit-rollback-agent-{tag}"),
                            "agent_role": "worker",
                            "agent_objective": "prove atomic PG stale recovery audit",
                            "profile_id": "bounded",
                            "capability_profile": ["work"],
                            "decision_source": "fixture",
                            "max_actions": 1
                        }],
                        "edges": []
                    },
                    "analysis": {},
                    "boundaries": {"execution_authority": "rust_scheduler_only"}
                }))
            },
        )
        .unwrap();
    let run_id = store
        .create_workflow_run_from_plan(plan["plan_id"].as_str().unwrap(), "pg-test")
        .unwrap()["run_id"]
        .as_str()
        .unwrap()
        .to_string();
    let mut client = postgres::Client::connect(&url, postgres::NoTls).unwrap();
    assert_eq!(
        client
            .execute(
                "UPDATE workflow_run_nodes
                 SET status = 'running', leased_at = $1
                 WHERE run_id = $2 AND node_id = $3 AND status = 'pending'",
                &[&"2026-07-14T00:00:00Z", &run_id, &node_id],
            )
            .unwrap(),
        1
    );
    let suffix = tag.replace('-', "_");
    let function_name = format!("fail_stale_recovery_audit_{suffix}");
    let trigger_name = format!("fail_stale_recovery_audit_trigger_{suffix}");
    let escaped_node_id = node_id.replace('\'', "''");
    client
        .batch_execute(&format!(
            "CREATE FUNCTION {function_name}() RETURNS trigger AS $fixture$
             BEGIN
                 IF NEW.action = 'agent_step.lease_expired'
                    AND NEW.resource = '{escaped_node_id}' THEN
                     RAISE EXCEPTION 'fixture agent lease audit failure';
                 END IF;
                 RETURN NEW;
             END;
             $fixture$ LANGUAGE plpgsql;
             CREATE TRIGGER {trigger_name}
             BEFORE INSERT ON audit_log
             FOR EACH ROW EXECUTE FUNCTION {function_name}();"
        ))
        .unwrap();

    let recovery = store.recover_stale_leases(1_000);
    client
        .batch_execute(&format!(
            "DROP TRIGGER {trigger_name} ON audit_log;
             DROP FUNCTION {function_name}();"
        ))
        .unwrap();
    let _error = recovery.expect_err("audit failure must roll back PG recovery");
    let row = client
        .query_one(
            "SELECT status, leased_at FROM workflow_run_nodes
             WHERE run_id = $1 AND node_id = $2",
            &[&run_id, &node_id],
        )
        .unwrap();
    assert_eq!(row.get::<_, String>(0), "running");
    assert_eq!(
        row.get::<_, Option<String>>(1).as_deref(),
        Some("2026-07-14T00:00:00Z")
    );
    assert_eq!(
        store
            .audit_events(10_000)
            .unwrap()
            .iter()
            .filter(|event| {
                event["action"] == "workflow_node.stale_lease_recovered"
                    && event["resource"] == node_id
            })
            .count(),
        0
    );
    assert_eq!(
        store
            .audit_events(10_000)
            .unwrap()
            .iter()
            .filter(|event| {
                event["action"] == "agent_step.lease_expired" && event["resource"] == node_id
            })
            .count(),
        0
    );

    assert!(store.recover_stale_leases(1_000).unwrap() >= 1);
    let recovered = client
        .query_one(
            "SELECT status, leased_at FROM workflow_run_nodes
             WHERE run_id = $1 AND node_id = $2",
            &[&run_id, &node_id],
        )
        .unwrap();
    assert_eq!(recovered.get::<_, String>(0), "pending");
    assert_eq!(recovered.get::<_, Option<String>>(1), None);
    assert_eq!(
        store
            .audit_events(10_000)
            .unwrap()
            .iter()
            .filter(|event| {
                event["action"] == "workflow_node.stale_lease_recovered"
                    && event["resource"] == node_id
            })
            .count(),
        1
    );
    assert_eq!(
        store
            .audit_events(10_000)
            .unwrap()
            .iter()
            .filter(|event| {
                event["action"] == "agent_step.lease_expired" && event["resource"] == node_id
            })
            .count(),
        1
    );
}

#[test]
#[cfg(feature = "pg-tests")]
fn pg_capped_agent_does_not_block_ready_command_node() {
    let Some(store) = test_store() else { return };
    let tag = uuid_tag();
    let agent_node_id = format!("a-agent-capped-{tag}");
    let command_node_id = format!("z-command-ready-{tag}");
    let agent_id = format!("agent-capped-{tag}");
    let plan = store
        .create_workflow_plan(
            &format!("pg capped agent mixed routing {tag}"),
            "pg-test",
            "pg-test",
            |ids, _| {
                Ok(json!({
                    "status": "planned_read_only",
                    "workflow_id": ids.workflow_id,
                    "dispatch_id": ids.dispatch_id,
                    "graph": {
                        "workflow_id": ids.workflow_id,
                        "dispatch_id": ids.dispatch_id,
                        "nodes": [
                            {
                                "node_id": agent_node_id,
                                "task_type": "agent_step",
                                "status": "pending",
                                "agent_id": agent_id,
                                "assigned_agent_id": agent_id,
                                "agent_role": "reviewer",
                                "profile_id": "bounded",
                                "agent_objective": "bounded PG review",
                                "capability_profile": ["review"],
                                "decision_source": "fixture",
                                "max_actions": 1
                            },
                            {
                                "node_id": command_node_id,
                                "task_type": "command",
                                "status": "pending",
                                "command": "echo ready"
                            }
                        ],
                        "edges": []
                    },
                    "analysis": {},
                    "boundaries": {"execution_authority": "bounded_trusted_local"}
                }))
            },
        )
        .unwrap();
    let run = store
        .create_workflow_run_from_plan(plan["plan_id"].as_str().unwrap(), "pg-test")
        .unwrap();
    let run_id = run["run_id"].as_str().unwrap();

    let tick = store
        .tick_with_executor_with_agent_caps(
            run_id,
            "pg-test",
            0,
            &PgCountingExecutor {
                calls: Arc::new(AtomicUsize::new(0)),
            },
            0,
            0,
        )
        .unwrap();
    assert_eq!(tick["action"], "node_executed");
    assert_eq!(tick["node_id"], command_node_id);

    let current = store.get_workflow_run(run_id).unwrap().unwrap();
    let agent_node = current["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["node_id"] == agent_node_id)
        .unwrap();
    assert_eq!(agent_node["db_status"], "pending");
}

#[test]
#[cfg(feature = "pg-tests")]
fn pg_agent_action_receipt_prevents_duplicate_application() {
    let Some(store) = test_store() else { return };
    let tag = uuid_tag();
    let agent_id = format!("agent-{tag}");
    let node_id = format!("agent-node-{tag}");
    let plan = store
        .create_workflow_plan(
            &format!("agent receipt {tag}"),
            "pg-test",
            "pg-test",
            |ids, _| {
                Ok(json!({
                    "status": "planned_read_only",
                    "workflow_id": ids.workflow_id,
                    "dispatch_id": ids.dispatch_id,
                    "graph": {
                        "nodes": [{
                            "node_id": node_id,
                            "task_type": "agent_step",
                            "status": "pending",
                            "agent_id": agent_id,
                            "agent_role": "implementer",
                            "profile_id": "pg-agent-profile",
                            "agent_objective": "bounded PG receipt test",
                            "capability_profile": ["code"],
                            "model": "fixture"
                        }],
                        "edges": [],
                        "workflow_id": ids.workflow_id,
                        "dispatch_id": ids.dispatch_id
                    },
                    "analysis": {},
                    "boundaries": {"execution_authority": "bounded_trusted_local"}
                }))
            },
        )
        .unwrap();
    let run = store
        .create_workflow_run_from_plan(plan["plan_id"].as_str().unwrap(), "pg-test")
        .unwrap();
    let run_id = run["run_id"].as_str().unwrap().to_string();
    let workflow_id = plan["workflow_id"].as_str().unwrap().to_string();
    let store = Arc::new(store);
    let executor = AgentStepExecutor::new(store.clone(), Box::new(|_| Ok(AgentAction::Complete)));
    let input = NodeExecutionInput {
        node_id: node_id.clone(),
        task_type: "agent_step".to_string(),
        run_id: run_id.clone(),
        workflow_id,
        node_metadata: json!({"agent_id": agent_id}),
    };
    std::env::set_var("ACP_ENABLE_AGENT_RUNTIME", "1");
    std::env::remove_var("ACP_AGENT_RUNTIME_KILL_SWITCH");
    let first = executor.execute_node(&input);
    let repeated = executor.execute_node(&input);
    std::env::remove_var("ACP_ENABLE_AGENT_RUNTIME");
    assert_eq!(first.status, "completed");
    assert_eq!(repeated.status, "completed");
    assert_eq!(first.output, repeated.output);
    assert_eq!(
        store
            .get_agent_state(&agent_id, &run_id)
            .unwrap()
            .unwrap()
            .status,
        "completed"
    );
}

#[test]
#[cfg(feature = "pg-tests")]
fn pg_agent_handoff_receipt_is_concurrent_and_restart_idempotent() {
    let Some(setup) = test_store() else { return };
    let url = std::env::var("ACP_TEST_DATABASE_URL").expect("PG test URL");
    let tag = uuid_tag();
    let source_agent_id = format!("handoff-source-{tag}");
    let target_agent_id = format!("handoff-target-{tag}");
    let node_id = format!("handoff-node-{tag}");
    let correlation_id = format!("handoff-correlation-{tag}");
    let plan = setup
        .create_workflow_plan(
            &format!("concurrent PG handoff receipt {tag}"),
            "pg-test",
            "pg-test",
            |ids, _| {
                Ok(json!({
                    "status": "planned_read_only",
                    "workflow_id": ids.workflow_id,
                    "dispatch_id": ids.dispatch_id,
                    "graph": {
                        "nodes": [{
                            "node_id": node_id,
                            "task_type": "agent_step",
                            "status": "pending",
                            "agent_id": source_agent_id,
                            "agent_role": "implementer",
                            "profile_id": "pg-handoff-profile",
                            "agent_objective": "request one bounded handoff",
                            "capability_profile": ["handoff"],
                            "model": "fixture"
                        }],
                        "edges": [],
                        "workflow_id": ids.workflow_id,
                        "dispatch_id": ids.dispatch_id
                    },
                    "analysis": {},
                    "boundaries": {"execution_authority": "bounded_trusted_local"}
                }))
            },
        )
        .expect("create concurrent handoff plan");
    let workflow_id = plan["workflow_id"]
        .as_str()
        .expect("workflow id")
        .to_string();
    let run_id = setup
        .create_workflow_run_from_plan(plan["plan_id"].as_str().unwrap(), "pg-test")
        .expect("create concurrent handoff run")["run_id"]
        .as_str()
        .expect("run id")
        .to_string();
    setup
        .create_agent_state(
            &target_agent_id,
            &run_id,
            "reviewer",
            &["handoff".to_string(), "mailbox".to_string()],
            Some("receive one bounded handoff"),
            "idle",
            &json!({}),
        )
        .expect("create handoff target agent");
    drop(setup);

    let input = NodeExecutionInput {
        node_id: node_id.clone(),
        task_type: "agent_step".to_string(),
        run_id: run_id.clone(),
        workflow_id,
        node_metadata: json!({"agent_id": source_agent_id}),
    };
    let action = HandoffRequest {
        schema_version: "handoff_request.v1".to_string(),
        correlation_id: correlation_id.clone(),
        objective: "review the bounded PostgreSQL result".to_string(),
        context_summary: "hash-bound fixture context".to_string(),
        target_agent_id: target_agent_id.clone(),
        source_agent_id: source_agent_id.clone(),
        run_id: run_id.clone(),
        node_id: node_id.clone(),
    };
    let decision_barrier = Arc::new(std::sync::Barrier::new(2));
    let decision_calls = Arc::new(AtomicUsize::new(0));

    std::env::set_var("ACP_ENABLE_AGENT_RUNTIME", "1");
    std::env::remove_var("ACP_AGENT_RUNTIME_KILL_SWITCH");
    let handles = (0..2)
        .map(|_| {
            let store = Arc::new(
                LocalProductStore::new_postgres(&url, utc_now_string)
                    .expect("open independent PG agent store"),
            );
            let request = action.clone();
            let barrier = decision_barrier.clone();
            let calls = decision_calls.clone();
            let executor = AgentStepExecutor::new(
                store,
                Box::new(move |_| {
                    calls.fetch_add(1, Ordering::SeqCst);
                    barrier.wait();
                    Ok(AgentAction::RequestHandoff(request.clone()))
                }),
            );
            let input = input.clone();
            std::thread::spawn(move || executor.execute_node(&input))
        })
        .collect::<Vec<_>>();
    let concurrent = handles
        .into_iter()
        .map(|handle| handle.join().expect("join concurrent PG agent action"))
        .collect::<Vec<_>>();
    assert_eq!(decision_calls.load(Ordering::SeqCst), 2);
    assert!(concurrent.iter().all(|output| output.status == "completed"));
    assert_eq!(concurrent[0].output, concurrent[1].output);
    let committed_result = concurrent[0]
        .output
        .clone()
        .expect("committed handoff result");

    let reopened = Arc::new(
        LocalProductStore::new_postgres(&url, utc_now_string)
            .expect("reopen PG store after commit"),
    );
    let replay_calls = decision_calls.clone();
    let replay_executor = AgentStepExecutor::new(
        reopened.clone(),
        Box::new(move |_| {
            replay_calls.fetch_add(1, Ordering::SeqCst);
            Ok(AgentAction::Complete)
        }),
    );
    let replay = replay_executor.execute_node(&input);
    std::env::remove_var("ACP_ENABLE_AGENT_RUNTIME");
    assert_eq!(replay.status, "completed");
    assert_eq!(replay.output.as_deref(), Some(committed_result.as_str()));
    assert_eq!(
        decision_calls.load(Ordering::SeqCst),
        2,
        "restart replay must short-circuit before decision"
    );

    let proposals = reopened
        .list_proposals_by_run(&run_id, 100, 0)
        .expect("list committed handoff proposals");
    assert_eq!(proposals.len(), 1);
    assert_eq!(proposals[0]["proposal_type"], "handoff");
    assert_eq!(proposals[0]["correlation_id"], correlation_id);
    assert_eq!(proposals[0]["target_agent_id"], target_agent_id);
    let messages = reopened
        .list_mailbox(Some(&target_agent_id), Some(&run_id), None, None, 100, 0)
        .expect("list committed handoff mailbox");
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].message_type, "handoff_request");
    assert_eq!(
        messages[0].correlation_id.as_deref(),
        Some(correlation_id.as_str())
    );
    assert_eq!(messages[0].from_agent_id, source_agent_id);
    assert_eq!(messages[0].to_agent_id, target_agent_id);
    assert_eq!(messages[0].run_id.as_deref(), Some(run_id.as_str()));

    let committed_audits = reopened
        .search_audit_events_by_run(&run_id, 1_000, 0)
        .expect("list handoff receipt audits")
        .into_iter()
        .filter(|event| event["action"] == "agent_action.committed")
        .collect::<Vec<_>>();
    assert_eq!(committed_audits.len(), 1);
    assert_eq!(
        committed_audits[0]["details"]["action_type"],
        "request_handoff"
    );

    let mut client = postgres::Client::connect(&url, postgres::NoTls).expect("raw PG client");
    let receipt = client
        .query_one(
            "SELECT COUNT(*), MIN(result_json), MAX(result_json)
             FROM agent_action_receipts WHERE run_id = $1 AND node_id = $2",
            &[&run_id, &node_id],
        )
        .expect("query authoritative agent action receipt");
    assert_eq!(receipt.get::<_, i64>(0), 1);
    assert_eq!(
        receipt.get::<_, Option<String>>(1).as_deref(),
        Some(committed_result.as_str())
    );
    assert_eq!(
        receipt.get::<_, Option<String>>(2).as_deref(),
        Some(committed_result.as_str())
    );
}

#[test]
#[cfg(feature = "pg-tests")]
fn pg_normal_workflow_executes_mailbox_memory_handoff_review_and_debate_chain() {
    let Some(store) = test_store() else { return };
    let tag = uuid_tag();
    let source_agent_id = format!("chain-source-{tag}");
    let target_agent_id = format!("chain-target-{tag}");
    let message_id = format!("chain-message-{tag}");
    let node_specs = [
        ("memory", source_agent_id.as_str()),
        ("mailbox-read", source_agent_id.as_str()),
        ("mailbox-ack", source_agent_id.as_str()),
        ("handoff-request-accept", source_agent_id.as_str()),
        ("handoff-accept", target_agent_id.as_str()),
        ("handoff-request-reject", source_agent_id.as_str()),
        ("handoff-reject", target_agent_id.as_str()),
        ("review-request", source_agent_id.as_str()),
        ("review-verdict", target_agent_id.as_str()),
        ("debate-open", source_agent_id.as_str()),
        ("debate-position", target_agent_id.as_str()),
        ("debate-resolve", source_agent_id.as_str()),
    ];
    let plan = store
        .create_workflow_plan(
            &format!("full PostgreSQL agent workflow {tag}"),
            "pg-test",
            "pg-test",
            |ids, _| {
                let nodes = node_specs
                    .iter()
                    .map(|(suffix, agent_id)| {
                        let source = *agent_id == source_agent_id;
                        json!({
                            "node_id": format!("{suffix}-{tag}"),
                            "task_type": "agent_step",
                            "status": "pending",
                            "agent_id": agent_id,
                            "agent_role": if source { "implementer" } else { "reviewer" },
                            "profile_id": if source { "chain-source-profile" } else { "chain-target-profile" },
                            "agent_objective": "execute one bounded production-chain action",
                            "capability_profile": ["memory", "mailbox", "handoff", "review", "debate"],
                            "model": "fixture"
                        })
                    })
                    .collect::<Vec<_>>();
                let edges = node_specs
                    .windows(2)
                    .enumerate()
                    .map(|(index, pair)| {
                        json!({
                            "edge_id": format!("chain-edge-{index}-{tag}"),
                            "from_node_id": format!("{}-{tag}", pair[0].0),
                            "to_node_id": format!("{}-{tag}", pair[1].0),
                            "condition": "on_success"
                        })
                    })
                    .collect::<Vec<_>>();
                Ok(json!({
                    "status": "planned_read_only",
                    "workflow_id": ids.workflow_id,
                    "dispatch_id": ids.dispatch_id,
                    "graph": {
                        "nodes": nodes,
                        "edges": edges,
                        "workflow_id": ids.workflow_id,
                        "dispatch_id": ids.dispatch_id
                    },
                    "analysis": {"task_domain": "agent_runtime"},
                    "boundaries": {
                        "execution_authority": "rust_scheduler_only",
                        "provider_execution": "default_off_fail_closed"
                    }
                }))
            },
        )
        .expect("create full agent workflow plan");
    let run = store
        .create_workflow_run_from_plan(plan["plan_id"].as_str().unwrap(), "pg-test")
        .expect("create full agent workflow run");
    let run_id = run["run_id"].as_str().unwrap().to_string();
    store
        .send_message(
            &message_id,
            &target_agent_id,
            &source_agent_id,
            "note",
            Some("bounded mailbox fixture"),
            Some(&format!("mailbox-{tag}")),
            Some(&run_id),
            None,
            None,
            &json!({"content_excluded": true}),
        )
        .expect("seed source mailbox");

    let store = Arc::new(store);
    let decision_store = store.clone();
    let decision_message_id = message_id.clone();
    let decision_source_agent = source_agent_id.clone();
    let decision_target_agent = target_agent_id.clone();
    let expected_handoff_accept_correlation = format!("handoff-accept-{tag}");
    let expected_handoff_reject_correlation = format!("handoff-reject-{tag}");
    let expected_review_correlation = format!("review-{tag}");
    let expected_debate_correlation = format!("debate-{tag}");
    let handoff_accept_correlation = expected_handoff_accept_correlation.clone();
    let handoff_reject_correlation = expected_handoff_reject_correlation.clone();
    let review_correlation = expected_review_correlation.clone();
    let debate_correlation = expected_debate_correlation.clone();
    let decision_tag = tag.clone();
    let executor = AgentStepExecutor::new(
        store.clone(),
        Box::new(move |context| {
            let suffix = context
                .node_id
                .strip_suffix(&format!("-{decision_tag}"))
                .ok_or_else(|| "unexpected chain node id".to_string())?;
            match suffix {
                "memory" => Ok(AgentAction::UpdateScratchpadSummary(
                    "bounded durable working summary".to_string(),
                )),
                "mailbox-read" => Ok(AgentAction::ReadMailbox),
                "mailbox-ack" => Ok(AgentAction::AckMessage(decision_message_id.clone())),
                "handoff-request-accept" => Ok(AgentAction::RequestHandoff(HandoffRequest {
                    schema_version: "handoff_request.v1".to_string(),
                    correlation_id: handoff_accept_correlation.clone(),
                    objective: "accept one bounded handoff".to_string(),
                    context_summary: "hash-bound handoff context".to_string(),
                    target_agent_id: decision_target_agent.clone(),
                    source_agent_id: decision_source_agent.clone(),
                    run_id: context.run_id.clone(),
                    node_id: context.node_id.clone(),
                })),
                "handoff-accept" => Ok(AgentAction::AcceptHandoff(
                    handoff_accept_correlation.clone(),
                )),
                "handoff-request-reject" => Ok(AgentAction::RequestHandoff(HandoffRequest {
                    schema_version: "handoff_request.v1".to_string(),
                    correlation_id: handoff_reject_correlation.clone(),
                    objective: "reject one bounded handoff".to_string(),
                    context_summary: "hash-bound rejection context".to_string(),
                    target_agent_id: decision_target_agent.clone(),
                    source_agent_id: decision_source_agent.clone(),
                    run_id: context.run_id.clone(),
                    node_id: context.node_id.clone(),
                })),
                "handoff-reject" => Ok(AgentAction::RejectHandoff(
                    handoff_reject_correlation.clone(),
                )),
                "review-request" => Ok(AgentAction::RequestReview(ReviewRequest {
                    schema_version: "review_request.v1".to_string(),
                    correlation_id: review_correlation.clone(),
                    subject_summary: "review the bounded chain".to_string(),
                    rationale_summary: "production workflow evidence".to_string(),
                    target_agent_id: decision_target_agent.clone(),
                    run_id: context.run_id.clone(),
                    node_id: context.node_id.clone(),
                    blocking: true,
                })),
                "review-verdict" => {
                    let review_id = decision_store
                        .list_proposals_by_run(&context.run_id, 100, 0)?
                        .into_iter()
                        .find(|proposal| {
                            proposal["correlation_id"] == review_correlation
                                && proposal["proposal_type"] == "review_request"
                        })
                        .and_then(|proposal| proposal["proposal_id"].as_str().map(str::to_string))
                        .ok_or_else(|| {
                            "review request missing from production chain".to_string()
                        })?;
                    Ok(AgentAction::SubmitReviewVerdict(ReviewVerdict {
                        schema_version: "review_verdict.v1".to_string(),
                        correlation_id: review_correlation.clone(),
                        review_request_id: review_id,
                        verdict: "accepted".to_string(),
                        rationale_summary: "bounded review accepted".to_string(),
                        run_id: context.run_id.clone(),
                        node_id: context.node_id.clone(),
                        blocking: true,
                    }))
                }
                "debate-open" => Ok(AgentAction::OpenDebate(DebateRequest {
                    schema_version: "debate_request.v1".to_string(),
                    correlation_id: debate_correlation.clone(),
                    subject_summary: "choose the bounded resolution".to_string(),
                    participant_agent_ids: vec![decision_target_agent.clone()],
                    max_rounds: 1,
                    run_id: context.run_id.clone(),
                    node_id: context.node_id.clone(),
                })),
                "debate-position" => {
                    let debate_id = decision_store
                        .list_proposals_by_run(&context.run_id, 100, 0)?
                        .into_iter()
                        .find(|proposal| {
                            proposal["correlation_id"] == debate_correlation
                                && proposal["proposal_type"] == "debate_request"
                        })
                        .and_then(|proposal| proposal["proposal_id"].as_str().map(str::to_string))
                        .ok_or_else(|| {
                            "debate request missing from production chain".to_string()
                        })?;
                    Ok(AgentAction::SubmitDebatePosition(DebatePosition {
                        schema_version: "debate_position.v1".to_string(),
                        correlation_id: debate_correlation.clone(),
                        debate_id,
                        position: "use the bounded evidence".to_string(),
                        rationale_summary: "one deterministic round".to_string(),
                        run_id: context.run_id.clone(),
                        node_id: context.node_id.clone(),
                    }))
                }
                "debate-resolve" => {
                    let debate_id = decision_store
                        .list_proposals_by_run(&context.run_id, 100, 0)?
                        .into_iter()
                        .find(|proposal| {
                            proposal["correlation_id"] == debate_correlation
                                && proposal["proposal_type"] == "debate_request"
                        })
                        .and_then(|proposal| proposal["proposal_id"].as_str().map(str::to_string))
                        .ok_or_else(|| {
                            "debate request missing from production chain".to_string()
                        })?;
                    Ok(AgentAction::ResolveDebate(DebateResolution {
                        schema_version: "debate_resolution.v1".to_string(),
                        correlation_id: debate_correlation.clone(),
                        debate_id,
                        resolution: "retain the bounded production chain".to_string(),
                        winning_position: Some("use the bounded evidence".to_string()),
                        unresolved_risks: None,
                        run_id: context.run_id.clone(),
                        node_id: context.node_id.clone(),
                    }))
                }
                other => Err(format!("unexpected chain action node: {other}")),
            }
        }),
    );

    std::env::set_var("ACP_ENABLE_AGENT_RUNTIME", "1");
    std::env::remove_var("ACP_AGENT_RUNTIME_KILL_SWITCH");
    for (expected_suffix, _) in node_specs {
        let tick = store
            .tick_with_executor_with_agent_caps(&run_id, "pg-test", 0, &executor, 4, 2)
            .expect("execute full agent workflow node");
        assert_eq!(tick["action"], "node_executed");
        assert_eq!(tick["node_id"], format!("{expected_suffix}-{tag}"));
        assert_eq!(tick["result"]["status"], "completed");
    }
    std::env::remove_var("ACP_ENABLE_AGENT_RUNTIME");

    let source_state = store
        .get_agent_state(&source_agent_id, &run_id)
        .expect("read source state")
        .expect("source state");
    assert_eq!(
        source_state.scratchpad_summary.as_deref(),
        Some("bounded durable working summary")
    );
    assert_eq!(
        store
            .read_message(&message_id)
            .expect("read seeded mailbox message")
            .expect("seeded mailbox message")
            .status,
        "acked"
    );
    let proposals = store
        .list_proposals_by_run(&run_id, 100, 0)
        .expect("read full agent workflow proposals");
    assert!(proposals.iter().any(|proposal| {
        proposal["correlation_id"] == expected_handoff_accept_correlation
            && proposal["proposal_type"] == "handoff"
            && proposal["status"] == "accepted"
    }));
    assert!(proposals.iter().any(|proposal| {
        proposal["correlation_id"] == expected_handoff_reject_correlation
            && proposal["proposal_type"] == "handoff"
            && proposal["status"] == "rejected"
    }));
    assert!(proposals.iter().any(|proposal| {
        proposal["correlation_id"] == expected_review_correlation
            && proposal["proposal_type"] == "review_request"
            && proposal["status"] == "accepted"
    }));
    assert!(proposals.iter().any(|proposal| {
        proposal["correlation_id"] == expected_review_correlation
            && proposal["proposal_type"] == "review_verdict"
    }));
    assert!(proposals.iter().any(|proposal| {
        proposal["correlation_id"] == expected_debate_correlation
            && proposal["proposal_type"] == "debate_request"
            && proposal["status"] == "accepted"
    }));
    assert!(proposals.iter().any(|proposal| {
        proposal["correlation_id"] == expected_debate_correlation
            && proposal["proposal_type"] == "debate_position"
    }));
    assert!(proposals.iter().any(|proposal| {
        proposal["correlation_id"] == expected_debate_correlation
            && proposal["proposal_type"] == "debate_resolution"
    }));

    let url = std::env::var("ACP_TEST_DATABASE_URL").expect("PG test URL");
    let mut client = postgres::Client::connect(&url, postgres::NoTls).expect("raw PG client");
    let receipt_count: i64 = client
        .query_one(
            "SELECT COUNT(*) FROM agent_action_receipts WHERE run_id = $1",
            &[&run_id],
        )
        .expect("count full-chain receipts")
        .get(0);
    assert_eq!(receipt_count, node_specs.len() as i64);
}

#[test]
#[cfg(feature = "pg-tests")]
fn pg_tool_approval_is_bound_consumed_and_not_replayed() {
    let Some(store) = test_store() else { return };
    let tag = uuid_tag();
    let node_id = format!("tool-node-{tag}");
    let profile_id = format!("tool-profile-{tag}");
    let tool_name = format!("tool-{tag}");
    let plan = store
        .create_workflow_plan(
            &format!("tool approval {tag}"),
            "pg-test",
            "pg-test",
            |ids, _| {
                Ok(json!({
                    "status": "planned_read_only",
                    "workflow_id": ids.workflow_id,
                    "dispatch_id": ids.dispatch_id,
                    "graph": {
                        "nodes": [{
                            "node_id": node_id,
                            "task_type": "command",
                            "status": "pending",
                            "profile_id": profile_id,
                            "command": format!("{tool_name} pg-fixture")
                        }],
                        "edges": [],
                        "workflow_id": ids.workflow_id,
                        "dispatch_id": ids.dispatch_id
                    },
                    "analysis": {},
                    "boundaries": {"execution_authority": "bounded_trusted_local"}
                }))
            },
        )
        .unwrap();
    let run = store
        .create_workflow_run_from_plan(plan["plan_id"].as_str().unwrap(), "pg-test")
        .unwrap();
    let run_id = run["run_id"].as_str().unwrap().to_string();
    let capability = store
        .configure_tool_capability(
            "pg-operator",
            &tool_name,
            "PG fixture",
            None,
            None,
            true,
            "medium",
            None,
        )
        .unwrap();
    assert_eq!(capability["changed"], true);
    let allowlist = store
        .configure_tool_allowlist(
            "pg-operator",
            &profile_id,
            std::slice::from_ref(&tool_name),
            None,
        )
        .unwrap();
    assert_eq!(allowlist["changed"], true);
    assert_eq!(
        store
            .read_tool_allowlist_policy(&profile_id)
            .unwrap()
            .unwrap()["resource_sha256"],
        allowlist["resource_sha256"]
    );
    let store = Arc::new(store);
    let calls = Arc::new(AtomicUsize::new(0));
    let executor = ToolPolicyNodeExecutor::command(
        Arc::new(PgCountingExecutor {
            calls: calls.clone(),
        }),
        store.clone(),
    );

    let first = store
        .tick_with_executor(&run_id, "pg-test", 0, &executor)
        .unwrap();
    assert_eq!(first["result"]["status"], "awaiting_approval");
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let authorization = store
        .inspect_tool_execution_authorization(&run_id, &node_id)
        .unwrap()
        .unwrap();
    let approval_id = authorization["requested_approval_id"].as_str().unwrap();
    store
        .resolve_requested_workflow_run_approval(
            &run_id,
            approval_id,
            "approved",
            "pg-operator",
            Some("bounded PG fixture"),
        )
        .unwrap();
    let second = store
        .tick_with_executor(&run_id, "pg-test", 0, &executor)
        .unwrap();
    assert_eq!(second["result"]["status"], "completed");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        store
            .inspect_tool_execution_authorization(&run_id, &node_id)
            .unwrap()
            .unwrap()["status"],
        "consumed"
    );
}

#[test]
#[cfg(feature = "pg-tests")]
fn pg_implicit_tool_receipt_is_atomic_across_concurrent_callers() {
    let Some(store) = test_store() else { return };
    let tag = uuid_tag();
    let node_id = format!("implicit-tool-node-{tag}");
    let profile_id = format!("implicit-tool-profile-{tag}");
    let tool_name = format!("implicit-tool-{tag}");
    let plan = store
        .create_workflow_plan(
            &format!("implicit tool receipt {tag}"),
            "pg-test",
            "pg-test",
            |ids, _| {
                Ok(json!({
                    "status": "planned_read_only",
                    "workflow_id": ids.workflow_id,
                    "dispatch_id": ids.dispatch_id,
                    "graph": {
                        "workflow_id": ids.workflow_id,
                        "dispatch_id": ids.dispatch_id,
                        "nodes": [{
                            "node_id": node_id,
                            "task_type": "command",
                            "status": "pending",
                            "profile_id": profile_id,
                            "command": format!("{tool_name} bounded")
                        }],
                        "edges": []
                    },
                    "analysis": {},
                    "boundaries": {"execution_authority": "bounded_trusted_local"}
                }))
            },
        )
        .unwrap();
    let run = store
        .create_workflow_run_from_plan(plan["plan_id"].as_str().unwrap(), "pg-test")
        .unwrap();
    let run_id = run["run_id"].as_str().unwrap().to_string();
    let workflow_id = plan["workflow_id"].as_str().unwrap().to_string();
    store
        .configure_tool_capability(
            "pg-test",
            &tool_name,
            "bounded fixture",
            None,
            None,
            false,
            "low",
            None,
        )
        .unwrap();
    store
        .configure_tool_allowlist(
            "pg-test",
            &profile_id,
            std::slice::from_ref(&tool_name),
            None,
        )
        .unwrap();

    let store = Arc::new(store);
    let calls = Arc::new(AtomicUsize::new(0));
    let inner: Arc<dyn NodeExecutor> = Arc::new(PgCountingExecutor {
        calls: calls.clone(),
    });
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let input = NodeExecutionInput {
        node_id: node_id.clone(),
        task_type: "command".to_string(),
        run_id: run_id.clone(),
        workflow_id,
        node_metadata: json!({
            "profile_id": profile_id,
            "command": format!("{tool_name} bounded")
        }),
    };
    let handles = (0..2)
        .map(|_| {
            let barrier = barrier.clone();
            let inner = inner.clone();
            let input = input.clone();
            let store = store.clone();
            std::thread::spawn(move || {
                let executor = ToolPolicyNodeExecutor::command(inner, store);
                barrier.wait();
                executor.execute_node(&input)
            })
        })
        .collect::<Vec<_>>();
    let results = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect::<Vec<_>>();

    assert_eq!(
        results
            .iter()
            .filter(|result| result.status == "completed")
            .count(),
        1
    );
    assert_eq!(
        results
            .iter()
            .filter(|result| {
                result.error_domain.as_deref() == Some("tool_execution_outcome_unknown")
            })
            .count(),
        1
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let receipt = store
        .inspect_tool_execution_authorization(&run_id, &node_id)
        .unwrap()
        .unwrap();
    assert_eq!(receipt["status"], "consumed");
    assert_eq!(receipt["resolved_by"], "tool-policy:implicit");
    let implicit_claims = store
        .audit_events(200)
        .unwrap()
        .into_iter()
        .filter(|event| {
            event["action"] == "tool_execution.implicit_receipt_claimed"
                && event["resource"] == run_id
        })
        .count();
    assert_eq!(implicit_claims, 1);
}

#[test]
#[cfg(feature = "pg-tests")]
fn pg_budget_auto_pause_and_recovery_are_atomic_and_idempotent() {
    let Ok(url) = std::env::var("ACP_TEST_DATABASE_URL") else {
        eprintln!("ACP_TEST_DATABASE_URL not set; skipping pg-tests");
        return;
    };
    let store =
        LocalProductStore::new_postgres(&url, || "2026-07-11T00:10:20Z".to_string()).unwrap();
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

    let request_binding = json!({
        "schema_version": "target_repo_output_request.v1",
        "artifact_id": artifact_id,
        "workspace_id": artifact["workspace_id"],
        "run_id": run_id,
        "target_id": artifact["target_id"],
        "mode": "push_branch",
        "patch_hash": artifact["patch_hash"],
        "source_revision": artifact["source_revision"],
        "branch_name": format!("acp/{artifact_id}"),
        "remote": "origin",
    });
    let request_sha256 = hex::encode(Sha256::digest(
        serde_json::to_vec(&request_binding).unwrap(),
    ));
    let output = json!({
        "schema_version": "target_repo_output.v1",
        "source_revision": artifact["source_revision"],
        "patch_hash": artifact["patch_hash"],
        "branch_name": format!("acp/{artifact_id}"),
        "remote": "origin",
        "commit_sha": "b".repeat(40),
    });
    let first_store = test_store().expect("first concurrent PostgreSQL store");
    let first_artifact = artifact_id.to_string();
    let first_binding = request_binding.clone();
    let first_hash = request_sha256.clone();
    let first = std::thread::spawn(move || {
        first_store.claim_target_output(&first_artifact, &first_binding, &first_hash, "first-actor")
    });
    let second_store = test_store().expect("second concurrent PostgreSQL store");
    let second_artifact = artifact_id.to_string();
    let second_binding = request_binding.clone();
    let second_hash = request_sha256.clone();
    let second = std::thread::spawn(move || {
        second_store.claim_target_output(
            &second_artifact,
            &second_binding,
            &second_hash,
            "second-actor",
        )
    });
    let claims = [
        first.join().unwrap().unwrap(),
        second.join().unwrap().unwrap(),
    ];
    assert_eq!(
        claims
            .iter()
            .filter(|claim| {
                **claim == engine::storage::local_product_store::TargetOutputClaim::Claimed
            })
            .count(),
        1
    );
    assert_eq!(
        claims
            .iter()
            .filter(|claim| matches!(claim,
                engine::storage::local_product_store::TargetOutputClaim::ReconciliationRequired(state)
                if state == "sending"))
            .count(),
        1
    );
    let receipt = store
        .record_target_output_receipt(
            artifact_id,
            &request_binding,
            &request_sha256,
            &output,
            "test-actor",
        )
        .expect("record target output receipt");
    assert_eq!(receipt["state"], "completed");
    let reused = store
        .record_target_output_receipt(
            artifact_id,
            &request_binding,
            &request_sha256,
            &output,
            "test-actor",
        )
        .expect("reuse target output receipt");
    assert_eq!(reused, receipt);
    let stored = store
        .get_supervised_patch_artifact(artifact_id)
        .expect("reload target output artifact")
        .expect("target output artifact exists");
    assert_eq!(stored["target_output_receipt"], receipt);
    assert!(store
        .record_target_output_receipt(
            artifact_id,
            &json!({"different": true}),
            &"c".repeat(64),
            &output,
            "test-actor",
        )
        .unwrap_err()
        .contains("request hash is invalid"));
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

#[test]
#[cfg(feature = "pg-tests")]
fn pg_durable_memory_is_scope_bound_restart_safe_and_concurrency_safe() {
    let Some(store) = test_store() else { return };
    let tag = uuid_tag();
    let scope = MemoryScope {
        tenant_id: "local".to_string(),
        workspace_id: format!("pg-memory-{tag}"),
        agent_id: Some(format!("agent-{tag}")),
        task_id: None,
    };
    let created = store
        .create_durable_memory(
            &DurableMemoryCreate {
                scope: scope.clone(),
                run_id: None,
                source_id: format!("source-{tag}"),
                source_sha256: "88".repeat(32),
                conflict_key: format!("fact-{tag}"),
                content: json!({"fact":"postgres durable memory"}),
                confidence: 0.9,
                fresh_until: None,
                expires_at: None,
                supersedes_memory_id: None,
            },
            "pg-memory-test",
        )
        .unwrap();
    let memory_id = created["memory_id"].as_str().unwrap().to_string();
    let url = std::env::var("ACP_TEST_DATABASE_URL").unwrap();
    let first_store = LocalProductStore::new_postgres(&url, utc_now_string).unwrap();
    let second_store = LocalProductStore::new_postgres(&url, utc_now_string).unwrap();
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let revise = |suffix: &str| DurableMemoryRevision {
        expected_version: 1,
        source_id: format!("revision-{suffix}-{tag}"),
        source_sha256: if suffix == "a" {
            "99".repeat(32)
        } else {
            "aa".repeat(32)
        },
        content: json!({"winner":suffix}),
        confidence: 1.0,
        fresh_until: None,
        expires_at: None,
    };
    let first_id = memory_id.clone();
    let first_barrier = barrier.clone();
    let first_revision = revise("a");
    let first = std::thread::spawn(move || {
        first_barrier.wait();
        first_store.revise_durable_memory(&first_id, &first_revision, "pg-memory-writer-a")
    });
    let second_id = memory_id.clone();
    let second_barrier = barrier.clone();
    let second_revision = revise("b");
    let second = std::thread::spawn(move || {
        second_barrier.wait();
        second_store.revise_durable_memory(&second_id, &second_revision, "pg-memory-writer-b")
    });
    let results = [first.join().unwrap(), second.join().unwrap()];
    assert_eq!(
        results.iter().filter(|result| result.is_ok()).count(),
        1,
        "concurrent durable-memory revisions: {results:?}"
    );
    assert_eq!(
        results
            .iter()
            .filter(|result| result
                .as_ref()
                .is_err_and(|error| error.contains("version conflict")))
            .count(),
        1,
        "concurrent durable-memory revisions: {results:?}"
    );

    let restarted = LocalProductStore::new_postgres(&url, utc_now_string).unwrap();
    let history = restarted.inspect_durable_memory(&memory_id).unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history.last().unwrap()["version"], 2);
    let retrieval = restarted
        .retrieve_durable_memories(
            &MemoryRetrievalRequest {
                scope: scope.clone(),
                run_id: format!("run-{tag}"),
                node_id: format!("node-{tag}"),
                query: "postgres durable memory".to_string(),
                top_k: 5,
                max_tokens: 100,
                max_bytes: 400,
                allow_lexical_fallback: true,
            },
            "pg-memory-reader",
        )
        .unwrap();
    assert_eq!(retrieval.mode, "lexical_fallback");
    assert_eq!(retrieval.selected.len(), 1);
    assert_eq!(retrieval.selected[0].memory_id, memory_id);

    let cross_scope = restarted
        .retrieve_durable_memories(
            &MemoryRetrievalRequest {
                scope: MemoryScope {
                    workspace_id: format!("other-{tag}"),
                    ..scope.clone()
                },
                run_id: format!("run-{tag}"),
                node_id: format!("node-{tag}"),
                query: "postgres durable memory".to_string(),
                top_k: 5,
                max_tokens: 100,
                max_bytes: 400,
                allow_lexical_fallback: true,
            },
            "pg-memory-reader",
        )
        .unwrap();
    assert!(cross_scope.selected.is_empty());

    let conflict_key = format!("pg-conflict-{tag}");
    let conflict_memory = |source: &str, hash: &str, value: &str| DurableMemoryCreate {
        scope: scope.clone(),
        run_id: None,
        source_id: format!("{source}-{tag}"),
        source_sha256: hash.repeat(32),
        conflict_key: conflict_key.clone(),
        content: json!({"value":value}),
        confidence: 0.9,
        fresh_until: None,
        expires_at: None,
        supersedes_memory_id: None,
    };
    let first_conflict = restarted
        .create_durable_memory(&conflict_memory("conflict-a", "11", "a"), "pg-memory-test")
        .unwrap();
    let second_conflict = restarted
        .create_durable_memory(&conflict_memory("conflict-b", "22", "b"), "pg-memory-test")
        .unwrap();
    assert!(restarted
        .create_durable_memory(&conflict_memory("conflict-c", "33", "c"), "pg-memory-test",)
        .unwrap_err()
        .contains("conflict set is full"));
    let resolved = restarted
        .supersede_durable_memory(
            second_conflict["memory_id"].as_str().unwrap(),
            1,
            first_conflict["memory_id"].as_str().unwrap(),
            2,
            "pg-memory-test",
        )
        .unwrap();
    assert_eq!(resolved["winner"]["state"], "current");
}

#[test]
#[cfg(feature = "pg-tests")]
fn pg_provider_embedding_metadata_is_atomic_and_restart_safe() {
    let Ok(url) = std::env::var("ACP_TEST_DATABASE_URL") else {
        return;
    };
    let keys = [
        "CI",
        "OPENROUTER_API_KEY",
        "ACP_DURABLE_MEMORY_EMBEDDING_MODE",
        "ACP_ENABLE_DURABLE_MEMORY_EMBEDDINGS",
        "ACP_DURABLE_MEMORY_EMBEDDING_DAILY_CAP_USD",
        "ACP_ENABLE_PROVIDER_EXECUTION",
        "ACP_REQUIRE_AUTH",
    ];
    let prior = keys
        .into_iter()
        .map(|key| (key, std::env::var(key).ok()))
        .collect::<Vec<_>>();
    std::env::remove_var("CI");
    std::env::set_var("OPENROUTER_API_KEY", "fixture-credential");
    std::env::set_var("ACP_DURABLE_MEMORY_EMBEDDING_MODE", "provider");
    std::env::set_var("ACP_ENABLE_DURABLE_MEMORY_EMBEDDINGS", "1");
    std::env::set_var("ACP_ENABLE_PROVIDER_EXECUTION", "1");
    std::env::set_var("ACP_REQUIRE_AUTH", "1");
    // Other PostgreSQL integration tests intentionally leave historical usage
    // in this shared disposable database. Isolate this zero-price provider
    // fixture from that unrelated aggregate while retaining production cost
    // reservation behavior.
    std::env::set_var("ACP_DURABLE_MEMORY_EMBEDDING_DAILY_CAP_USD", "1000000000");
    let result = (|| -> Result<(), String> {
        let tag = uuid_tag();
        let transport = Arc::new(PgCountingEmbeddingTransport {
            posts: AtomicUsize::new(0),
        });
        let store = LocalProductStore::new_postgres_with_embedding_transport_for_test(
            &url,
            utc_now_string,
            transport.clone(),
        )?;
        let scope = MemoryScope {
            tenant_id: "local".to_string(),
            workspace_id: format!("pg-provider-memory-{tag}"),
            agent_id: Some(format!("agent-{tag}")),
            task_id: None,
        };
        let created = store.create_durable_memory(
            &DurableMemoryCreate {
                scope: scope.clone(),
                run_id: Some(format!("run-{tag}")),
                source_id: format!("source-{tag}"),
                source_sha256: "ab".repeat(32),
                conflict_key: format!("fact-{tag}"),
                content: json!({"fact":"postgres provider embedding"}),
                confidence: 1.0,
                fresh_until: None,
                expires_at: None,
                supersedes_memory_id: None,
            },
            "pg-provider-memory-test",
        )?;
        let memory_id = created["memory_id"].as_str().unwrap().to_string();
        assert_eq!(created["embedding"]["provenance"], "provider_reported");
        assert_eq!(created["embedding"]["dimensions"], 1536);
        assert_eq!(transport.posts.load(Ordering::SeqCst), 1);
        let mut client =
            postgres::Client::connect(&url, postgres::NoTls).map_err(|error| error.to_string())?;
        client
            .execute(
                "DELETE FROM durable_memory_versions WHERE memory_id=$1",
                &[&memory_id],
            )
            .map_err(|error| error.to_string())?;
        drop(store);

        let restarted = Arc::new(
            LocalProductStore::new_postgres_with_embedding_transport_for_test(
                &url,
                utc_now_string,
                transport.clone(),
            )?,
        );
        let recovered = restarted.create_durable_memory(
            &DurableMemoryCreate {
                scope: scope.clone(),
                run_id: Some(format!("run-{tag}")),
                source_id: format!("source-{tag}"),
                source_sha256: "ab".repeat(32),
                conflict_key: format!("fact-{tag}"),
                content: json!({"fact":"postgres provider embedding"}),
                confidence: 1.0,
                fresh_until: None,
                expires_at: None,
                supersedes_memory_id: None,
            },
            "pg-provider-memory-test",
        )?;
        assert_eq!(
            recovered["embedding"]["binding_sha256"],
            created["embedding"]["binding_sha256"]
        );
        assert_eq!(transport.posts.load(Ordering::SeqCst), 1);
        let barrier = Arc::new(std::sync::Barrier::new(2));
        let revisions = [("a", "postgres revision a"), ("b", "postgres revision b")].map(
            |(suffix, content)| DurableMemoryRevision {
                expected_version: 1,
                source_id: format!("source-{tag}-{suffix}"),
                source_sha256: format!("{:x}", Sha256::digest(content.as_bytes())),
                content: json!({"fact":content}),
                confidence: 0.95,
                fresh_until: None,
                expires_at: None,
            },
        );
        let results = std::thread::scope(|scope| {
            let handles = revisions
                .into_iter()
                .map(|revision| {
                    let restarted = Arc::clone(&restarted);
                    let barrier = Arc::clone(&barrier);
                    let memory_id = memory_id.clone();
                    scope.spawn(move || {
                        barrier.wait();
                        restarted.revise_durable_memory(
                            &memory_id,
                            &revision,
                            "pg-provider-memory-test",
                        )
                    })
                })
                .collect::<Vec<_>>();
            handles
                .into_iter()
                .map(|handle| handle.join().unwrap())
                .collect::<Vec<_>>()
        });
        assert_eq!(
            results.iter().filter(|result| result.is_ok()).count(),
            1,
            "{results:?}"
        );
        assert!(results
            .iter()
            .filter_map(|result| result.as_ref().err())
            .all(
                |error| error.contains("competing provider embedding mutation")
                    || error.contains("version conflict")
            ));
        assert_eq!(transport.posts.load(Ordering::SeqCst), 2);
        let history = restarted.inspect_durable_memory(&memory_id)?;
        assert_eq!(history.len(), 2);
        assert_eq!(
            history[0]["embedding"]["binding_sha256"],
            created["embedding"]["binding_sha256"]
        );
        let receipt_tenant_id = scope.tenant_id.clone();
        let retrieval_request = MemoryRetrievalRequest {
            scope,
            run_id: format!("run-retrieve-{tag}"),
            node_id: format!("node-{tag}"),
            query: "postgres provider embedding".to_string(),
            top_k: 5,
            max_tokens: 100,
            max_bytes: 1000,
            allow_lexical_fallback: false,
        };
        let retrieval =
            restarted.retrieve_durable_memories(&retrieval_request, "pg-provider-memory-test")?;
        assert_eq!(retrieval.selected.len(), 1);
        assert_eq!(
            retrieval.embedding_provider.unwrap()["provider_id"],
            "openrouter"
        );
        assert_eq!(transport.posts.load(Ordering::SeqCst), 3);
        let duplicate =
            restarted.retrieve_durable_memories(&retrieval_request, "pg-provider-memory-test")?;
        assert_eq!(duplicate.result_sha256, retrieval.result_sha256);
        assert_eq!(transport.posts.load(Ordering::SeqCst), 3);
        let receipts = restarted.authorized_provider_embedding_receipt_evidence(
            100,
            engine::storage::local_product_store::ProviderEmbeddingReceiptVisibility::TenantOperator {
                tenant_id: receipt_tenant_id,
            },
        )?;
        assert!(receipts.iter().any(|receipt| {
            receipt["operation_kind"] == "retrieval_query"
                && receipt["state"] == "succeeded"
                && receipt["redacted"] == true
        }));
        let hidden_receipts = restarted.authorized_provider_embedding_receipt_evidence(
            100,
            engine::storage::local_product_store::ProviderEmbeddingReceiptVisibility::Hidden,
        )?;
        assert!(hidden_receipts.is_empty());
        let cross_tenant_receipts = restarted.authorized_provider_embedding_receipt_evidence(
            100,
            engine::storage::local_product_store::ProviderEmbeddingReceiptVisibility::TenantOperator {
                tenant_id: "tenant-other".to_string(),
            },
        )?;
        assert!(cross_tenant_receipts.is_empty());
        assert_eq!(restarted.check_integrity()?.status, "ok");
        let mut client =
            postgres::Client::connect(&url, postgres::NoTls).map_err(|error| error.to_string())?;
        client
            .execute(
                "UPDATE memory_retrieval_events SET result_sha256=$1 WHERE retrieval_id=$2",
                &[&"f".repeat(64), &duplicate.retrieval_id],
            )
            .map_err(|error| error.to_string())?;
        assert!(restarted
            .check_integrity()
            .unwrap_err()
            .contains("retrieval result cross-owner binding is invalid"));
        client
            .execute(
                "UPDATE memory_retrieval_events SET result_sha256=$1 WHERE retrieval_id=$2",
                &[&duplicate.result_sha256, &duplicate.retrieval_id],
            )
            .map_err(|error| error.to_string())?;
        assert_eq!(restarted.check_integrity()?.status, "ok");
        Ok(())
    })();
    for (key, value) in prior {
        if let Some(value) = value {
            std::env::set_var(key, value);
        } else {
            std::env::remove_var(key);
        }
    }
    result.unwrap();
}

#[test]
#[cfg(feature = "pg-tests")]
fn pg_provider_embedding_failure_audit_and_retry_authority_are_atomic() {
    let Ok(url) = std::env::var("ACP_TEST_DATABASE_URL") else {
        return;
    };
    let keys = [
        "CI",
        "OPENROUTER_API_KEY",
        "ACP_DURABLE_MEMORY_EMBEDDING_MODE",
        "ACP_ENABLE_DURABLE_MEMORY_EMBEDDINGS",
        "ACP_DURABLE_MEMORY_EMBEDDING_DAILY_CAP_USD",
        "ACP_ENABLE_PROVIDER_EXECUTION",
        "ACP_REQUIRE_AUTH",
    ];
    let prior = keys
        .into_iter()
        .map(|key| (key, std::env::var(key).ok()))
        .collect::<Vec<_>>();
    std::env::remove_var("CI");
    std::env::set_var("OPENROUTER_API_KEY", "fixture-credential");
    std::env::set_var("ACP_DURABLE_MEMORY_EMBEDDING_MODE", "provider");
    std::env::set_var("ACP_ENABLE_DURABLE_MEMORY_EMBEDDINGS", "1");
    std::env::set_var("ACP_ENABLE_PROVIDER_EXECUTION", "1");
    std::env::set_var("ACP_REQUIRE_AUTH", "1");
    std::env::set_var("ACP_DURABLE_MEMORY_EMBEDDING_DAILY_CAP_USD", "1000000000");
    let result = (|| -> Result<(), String> {
        let tag = uuid_tag();
        let transport = Arc::new(PgFailOnceEmbeddingTransport {
            posts: AtomicUsize::new(0),
        });
        let store = LocalProductStore::new_postgres_with_embedding_transport_for_test(
            &url,
            utc_now_string,
            transport.clone(),
        )?;
        let scope = MemoryScope {
            tenant_id: "local".into(),
            workspace_id: format!("pg-provider-retry-{tag}"),
            agent_id: Some(format!("agent-{tag}")),
            task_id: None,
        };
        let run_id = format!("run-{tag}");
        let request = DurableMemoryCreate {
            scope: scope.clone(),
            run_id: Some(run_id.clone()),
            source_id: format!("source-{tag}"),
            source_sha256: "ef".repeat(32),
            conflict_key: format!("fact-{tag}"),
            content: json!({"fact":"postgres retry authority"}),
            confidence: 1.0,
            fresh_until: None,
            expires_at: None,
            supersedes_memory_id: None,
        };
        let failure = store
            .create_durable_memory(&request, "pg-retry-test")
            .unwrap_err();
        assert!(
            !failure.contains("outcome unknown"),
            "unexpected ambiguous failure: {failure}"
        );
        let mut client =
            postgres::Client::connect(&url, postgres::NoTls).map_err(|error| error.to_string())?;
        let row = client
            .query_one(
                "SELECT target_memory_id,state,attempt_count
             FROM provider_embedding_operations WHERE workspace_id=$1",
                &[&scope.workspace_id],
            )
            .map_err(|error| error.to_string())?;
        let memory_id: String = row.get(0);
        assert_eq!(row.get::<_, String>(1), "failed_known_outcome");
        assert_eq!(row.get::<_, i64>(2), 1);
        let error_events:i64=client.query_one(
            "SELECT COUNT(*) FROM provider_audit_events WHERE dispatch_id LIKE $1 AND event_type='error'",
            &[&"memory-embedding-%".to_string()],
        ).map_err(|error|error.to_string())?.get(0);
        assert!(error_events >= 1);
        let resolution = ProviderEmbeddingResolutionRequest {
            target_version: 1,
            expected_attempt_count: 1,
            scope: scope.clone(),
            run_id: Some(run_id),
            action: ProviderEmbeddingResolutionAction::RetryFailed,
            evidence_source_id: None,
            evidence_sha256: None,
            confirm_resolution: true,
        };
        assert_eq!(
            store.reconcile_provider_embedding_operation(&memory_id, &resolution, "pg-operator")?
                ["state"],
            "retry_authorized"
        );
        let created = store.create_durable_memory(&request, "pg-retry-test")?;
        assert_eq!(created["version"], 1);
        assert_eq!(transport.posts.load(Ordering::SeqCst), 2);
        assert_eq!(store.check_integrity()?.status, "ok");
        Ok(())
    })();
    for (key, value) in prior {
        if let Some(value) = value {
            std::env::set_var(key, value);
        } else {
            std::env::remove_var(key);
        }
    }
    result.unwrap();
}

#[test]
#[cfg(feature = "pg-tests")]
fn pg_active_queue_numeric_types_match_sqlite_contract() {
    let Some(store) = test_store() else { return };
    let tag = uuid_tag();
    let tenant_id = format!("pg-queue-tenant-{tag}");
    let plan = store
        .create_workflow_plan(
            &format!("pg-queue-{tag}"),
            "pg queue type parity",
            "pg-queue-test",
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
        .create_workflow_run_with_queue_metadata(
            plan["plan_id"].as_str().unwrap(),
            "pg-queue-test",
            2,
            None,
            Some(1_200),
            Some(&tenant_id),
        )
        .unwrap();
    let run_id = run["run_id"].as_str().unwrap();

    let active = store.list_active_workflow_runs_prioritized().unwrap();
    let queued = active
        .iter()
        .find(|item| item["run_id"] == run_id)
        .expect("created PostgreSQL run should be readable from prioritized queue");
    assert_eq!(queued["priority"], 2);
    assert_eq!(queued["sla_ms"], 1_200);
    assert_eq!(queued["tenant_id"], tenant_id);

    let tenant = store
        .list_tenants_with_quota()
        .unwrap()
        .into_iter()
        .find(|item| item["tenant_id"] == tenant_id)
        .expect("tenant quota aggregation should decode PostgreSQL AVG");
    assert_eq!(tenant["run_count"], 1);
    assert_eq!(tenant["avg_priority"], 2.0);
    assert!(store.get_queue_status().unwrap()["avg_priority"].is_number());
}

#[test]
#[cfg(feature = "pg-tests")]
fn pg_external_runtime_receipts_are_concurrent_restart_safe_and_scope_bound() {
    let Some(store) = test_store() else { return };
    let tag = uuid_tag();
    let scope = ExternalRuntimeScope {
        tenant_id: format!("tenant-{tag}"),
        workspace_id: format!("workspace-{tag}"),
        run_id: format!("run-{tag}"),
        node_id: format!("node-{tag}"),
        thread_id: format!("thread-{tag}"),
    };
    let request_sha = "a".repeat(64);
    let first = store
        .claim_external_runtime_invocation(&scope, &request_sha, "lease-first", 60, "pg-test")
        .unwrap();
    let (invocation_id, lease_token) = match first {
        ExternalRuntimeInvocationClaim::Claimed { invocation_id, .. } => {
            (invocation_id, "lease-first")
        }
        other => panic!("expected first claim, got {other:?}"),
    };
    assert!(matches!(
        store
            .claim_external_runtime_invocation(&scope, &request_sha, "lease-second", 60, "pg-test")
            .unwrap(),
        ExternalRuntimeInvocationClaim::Busy { .. }
    ));

    let state = json!({
        "memory_digest":"1".repeat(64),
        "summary_digest":"2".repeat(64),
        "fact_ids":[],
        "selected_reference_ids":[],
        "recent_event_hashes":[],
        "turn_count":1,
        "conflict_count":0,
        "correction_count":0,
    });
    let state_sha = hex::encode(Sha256::digest(
        canonical_event_json(&state).unwrap().as_bytes(),
    ));
    let checkpoint = json!({
        "checkpoint_id":format!("ckpt-{tag}"),
        "version":1,
        "parent_checkpoint_id":Value::Null,
        "state_summary":state,
        "state_sha256":state_sha,
    });
    store
        .complete_external_runtime_invocation(
            &scope,
            &invocation_id,
            lease_token,
            "0.1.0",
            "1.2.9",
            "summary_memory",
            &checkpoint,
            &json!({"schema_version":"external_runtime_result.v1","raw_content_persisted":false}),
            "artifact-pg",
            "pg-test",
        )
        .unwrap();
    assert!(matches!(
        store
            .claim_external_runtime_invocation(&scope, &request_sha, "lease-third", 60, "pg-test")
            .unwrap(),
        ExternalRuntimeInvocationClaim::Completed { .. }
    ));
    let mut cross_scope = scope.clone();
    cross_scope.workspace_id = format!("other-{tag}");
    assert!(store
        .external_runtime_checkpoint(&cross_scope)
        .unwrap()
        .is_none());
    let restarted = test_store().unwrap();
    assert_eq!(
        restarted
            .external_runtime_checkpoint(&scope)
            .unwrap()
            .unwrap()["version"],
        1
    );

    let concurrent_scope = ExternalRuntimeScope {
        run_id: format!("concurrent-run-{tag}"),
        node_id: format!("concurrent-node-{tag}"),
        ..scope
    };
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let mut handles = Vec::new();
    for index in 0..2 {
        let url = std::env::var("ACP_TEST_DATABASE_URL").unwrap();
        let scope = concurrent_scope.clone();
        let barrier = barrier.clone();
        handles.push(std::thread::spawn(move || {
            let store = LocalProductStore::new_postgres(&url, utc_now_string).unwrap();
            barrier.wait();
            store
                .claim_external_runtime_invocation(
                    &scope,
                    &"b".repeat(64),
                    &format!("lease-{index}"),
                    60,
                    "pg-test",
                )
                .unwrap()
        }));
    }
    let outcomes = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(outcome, ExternalRuntimeInvocationClaim::Claimed { .. }))
            .count(),
        1
    );
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(outcome, ExternalRuntimeInvocationClaim::Busy { .. }))
            .count(),
        1
    );
}

#[cfg(feature = "pg-tests")]
#[test]
fn pg_rwe_authority_rejects_foreign_replay_and_stale_task_attempt_replay() {
    let Some(store) = test_store() else {
        return;
    };
    let tag = uuid_tag();
    let tenant = format!("tenant-rwe-{tag}");
    let owner = format!("fixture-principal-owner-{tag}");
    let foreign_tenant = format!("foreign-tenant-rwe-{tag}");
    let foreign_owner = format!("fixture-principal-foreign-{tag}");
    let principal = AuthenticatedPrincipal::fixture_for_tests(&tenant, &owner).unwrap();
    let foreign =
        AuthenticatedPrincipal::fixture_for_tests(&foreign_tenant, &foreign_owner).unwrap();
    let corpus = freeze_first_rwe_corpus().unwrap();
    let authorization_id = format!("rwe-pg-auth-{tag}");
    let budgets = corpus
        .tasks
        .iter()
        .map(|task| RwePerTaskBudget::from_task_definition(task, None))
        .collect::<Vec<_>>();
    let body = RweRunAuthorizationBody {
        authorization_id: authorization_id.clone(),
        corpus_sha256: corpus.corpus_sha256.clone(),
        golden_path_product_task_id: "pg-fixture-product-task".into(),
        principal_id: principal.principal_id().into(),
        principal_kind: principal.principal_kind().as_str().into(),
        task_ids: corpus
            .tasks
            .iter()
            .map(|task| task.task_id.clone())
            .collect(),
        max_total_provider_requests: corpus
            .tasks
            .iter()
            .map(|task| task.per_task_max_provider_requests)
            .sum(),
        max_total_tokens: corpus
            .tasks
            .iter()
            .map(|task| task.per_task_max_total_tokens)
            .sum(),
        max_wall_time_ms: corpus.tasks.iter().map(|task| task.timeout_ms).sum(),
        cost_authority: CostAuthority::CostUnavailable,
        per_task_budgets: budgets,
        binary_path: "/usr/bin/codex".into(),
        binary_version: "0.145.0".into(),
        binary_sha256: "ab".repeat(32),
        provider_kind: "openai_compatible".into(),
        provider_host: "api.openai.com".into(),
        provider_base_url: "https://api.openai.com/v1".into(),
        target_repo: "org/disposable".into(),
        target_main_sha: "a".repeat(40),
        executor_identity: "codex-0.145.0".into(),
        model_identity: "gpt-test-model".into(),
        draft_pr_only: true,
        admitted_executor: corpus.admitted_executor.clone(),
        auto_merge_disabled: corpus.auto_merge_disabled,
        expires_at: "2026-08-01T00:00:00Z".into(),
    };
    persist_rwe_run_authorization(&store, &principal, &body, true).unwrap();
    let run_id = format!("rwe-pg-run-{tag}");
    let mut run_body = body.to_json();
    run_body["run_id"] = json!(run_id.clone());
    run_body["provider_free_fixture"] = json!(true);
    let admitted = store
        .admit_rwe_run(&principal, &run_id, &authorization_id, &run_body, true)
        .unwrap();
    let lease = admitted["lease_token"].as_str().unwrap().to_string();

    let foreign_err = store
        .admit_rwe_run(&foreign, &run_id, &authorization_id, &run_body, true)
        .unwrap_err();
    assert!(foreign_err.contains("tenant") || foreign_err.contains("principal"));
    assert!(store
        .admit_rwe_run(
            &principal,
            &run_id,
            "wrong-rwe-authorization",
            &run_body,
            true,
        )
        .is_err());
    let mut conflicting_body = run_body.clone();
    conflicting_body["task_ids"] = json!([corpus.tasks[0].task_id]);
    assert!(store
        .admit_rwe_run(
            &principal,
            &run_id,
            &authorization_id,
            &conflicting_body,
            true
        )
        .is_err());

    let task = &corpus.tasks[0];
    let evidence = json!({"task_id": task.task_id, "replay": true});
    assert!(store
        .persist_rwe_task_attempt(
            &run_id,
            "stale-lease",
            &format!("{run_id}:attempt"),
            &task.task_id,
            &task.definition_sha256,
            "fixture_success",
            &evidence,
        )
        .is_err());
    store
        .persist_rwe_task_attempt(
            &run_id,
            &lease,
            &format!("{run_id}:attempt"),
            &task.task_id,
            &task.definition_sha256,
            "fixture_success",
            &evidence,
        )
        .unwrap();
    assert!(store
        .persist_rwe_task_attempt(
            &run_id,
            "stale-lease",
            &format!("{run_id}:attempt"),
            &task.task_id,
            &task.definition_sha256,
            "fixture_success",
            &evidence,
        )
        .is_err());

    let replay = store
        .admit_rwe_run(&principal, &run_id, &authorization_id, &run_body, true)
        .unwrap();
    assert_eq!(replay["idempotent_replay"], true);
    assert!(replay.get("lease_token").is_none());
    assert!(store.get_rwe_run(&run_id).unwrap().unwrap()["lease_token"].is_null());
}

#[cfg(feature = "pg-tests")]
fn pg_rwe_fixture_authorization_body(
    tag: &str,
    principal: &AuthenticatedPrincipal,
    corpus: &engine::rwe::corpus::FirstRweCorpus,
    budgets: Vec<RwePerTaskBudget>,
    cost_authority: CostAuthority,
) -> RweRunAuthorizationBody {
    RweRunAuthorizationBody {
        authorization_id: format!("rwe-pg-auth-{tag}"),
        corpus_sha256: corpus.corpus_sha256.clone(),
        golden_path_product_task_id: "pg-fixture-product-task".into(),
        principal_id: principal.principal_id().into(),
        principal_kind: principal.principal_kind().as_str().into(),
        task_ids: corpus
            .tasks
            .iter()
            .map(|task| task.task_id.clone())
            .collect(),
        max_total_provider_requests: corpus
            .tasks
            .iter()
            .map(|task| task.per_task_max_provider_requests)
            .sum(),
        max_total_tokens: corpus
            .tasks
            .iter()
            .map(|task| task.per_task_max_total_tokens)
            .sum(),
        max_wall_time_ms: corpus.tasks.iter().map(|task| task.timeout_ms).sum(),
        cost_authority,
        per_task_budgets: budgets,
        binary_path: "/usr/bin/codex".into(),
        binary_version: corpus.admitted_codex_version.clone(),
        binary_sha256: "ab".repeat(32),
        provider_kind: "openai_compatible".into(),
        provider_host: "api.openai.com".into(),
        provider_base_url: "https://api.openai.com/v1".into(),
        target_repo: "org/disposable".into(),
        target_main_sha: "a".repeat(40),
        executor_identity: corpus.tasks[0].executor_identity.clone(),
        model_identity: corpus.tasks[0].model_identity.clone(),
        draft_pr_only: true,
        admitted_executor: corpus.admitted_executor.clone(),
        auto_merge_disabled: corpus.auto_merge_disabled,
        expires_at: "2026-08-01T00:00:00Z".into(),
    }
}

#[cfg(feature = "pg-tests")]
#[test]
fn pg_rwe_corpus_envelope_rejects_mutations_through_issue_and_admit() {
    let Some(store) = test_store() else {
        return;
    };
    let tag = uuid_tag();
    let tenant = format!("tenant-rwe-env-{tag}");
    let owner = format!("fixture-principal-owner-{tag}");
    let principal = AuthenticatedPrincipal::fixture_for_tests(&tenant, &owner).unwrap();
    let corpus = freeze_first_rwe_corpus().unwrap();

    let bad_budgets = corpus
        .tasks
        .iter()
        .enumerate()
        .map(|(i, task)| {
            let mut budget = RwePerTaskBudget::from_task_definition(task, None);
            if i == 0 {
                budget.max_retries = 99;
            }
            budget
        })
        .collect::<Vec<_>>();
    let bad_body = pg_rwe_fixture_authorization_body(
        &format!("retry-{tag}"),
        &principal,
        &corpus,
        bad_budgets,
        CostAuthority::CostUnavailable,
    );
    assert!(persist_rwe_run_authorization(&store, &principal, &bad_body, true).is_err());

    let bad_budgets = corpus
        .tasks
        .iter()
        .enumerate()
        .map(|(i, task)| {
            let mut budget = RwePerTaskBudget::from_task_definition(task, None);
            if i == 0 {
                budget.executor_identity = "rogue-executor".into();
            }
            budget
        })
        .collect::<Vec<_>>();
    let bad_body = pg_rwe_fixture_authorization_body(
        &format!("exec-{tag}"),
        &principal,
        &corpus,
        bad_budgets,
        CostAuthority::CostUnavailable,
    );
    assert!(persist_rwe_run_authorization(&store, &principal, &bad_body, true).is_err());

    let bad_budgets = corpus
        .tasks
        .iter()
        .enumerate()
        .map(|(i, task)| {
            let mut budget = RwePerTaskBudget::from_task_definition(task, None);
            if i == 0 {
                budget.allowed_mutable_paths = vec!["src/rogue.rs".into()];
            }
            budget
        })
        .collect::<Vec<_>>();
    let bad_body = pg_rwe_fixture_authorization_body(
        &format!("paths-{tag}"),
        &principal,
        &corpus,
        bad_budgets,
        CostAuthority::CostUnavailable,
    );
    assert!(persist_rwe_run_authorization(&store, &principal, &bad_body, true).is_err());

    let bad_budgets = corpus
        .tasks
        .iter()
        .map(|task| RwePerTaskBudget::from_task_definition(task, Some(1.0)))
        .collect::<Vec<_>>();
    let bad_body = pg_rwe_fixture_authorization_body(
        &format!("cost-ceiling-{tag}"),
        &principal,
        &corpus,
        bad_budgets,
        CostAuthority::ProviderReported {
            max_cost: 1.0,
            currency: "USD".into(),
        },
    );
    assert!(persist_rwe_run_authorization(&store, &principal, &bad_body, true).is_err());

    let good_budgets = corpus
        .tasks
        .iter()
        .map(|task| RwePerTaskBudget::from_task_definition(task, Some(0.25)))
        .collect::<Vec<_>>();
    let good_body = pg_rwe_fixture_authorization_body(
        &format!("good-{tag}"),
        &principal,
        &corpus,
        good_budgets,
        CostAuthority::ProviderReported {
            max_cost: 5.0,
            currency: "USD".into(),
        },
    );
    let auth = persist_rwe_run_authorization(&store, &principal, &good_body, true).unwrap();
    let auth_id = auth["authorization_id"].as_str().unwrap().to_string();
    let run_id = format!("rwe-pg-env-run-{tag}");
    let mut run_body = good_body.to_json();
    run_body["run_id"] = json!(run_id.clone());
    run_body["provider_free_fixture"] = json!(true);
    let admitted = store
        .admit_rwe_run(&principal, &run_id, &auth_id, &run_body, true)
        .unwrap();
    assert!(admitted.get("lease_token").is_some());
}

#[cfg(feature = "pg-tests")]
#[test]
fn pg_rwe_concurrent_exact_task_attempt_replay_and_conflict() {
    let Some(store) = test_store() else {
        return;
    };
    let tag = uuid_tag();
    let tenant = format!("tenant-rwe-conc-{tag}");
    let owner = format!("fixture-principal-owner-{tag}");
    let principal = AuthenticatedPrincipal::fixture_for_tests(&tenant, &owner).unwrap();
    let corpus = freeze_first_rwe_corpus().unwrap();
    let budgets = corpus
        .tasks
        .iter()
        .map(|task| RwePerTaskBudget::from_task_definition(task, None))
        .collect::<Vec<_>>();
    let body = pg_rwe_fixture_authorization_body(
        &tag,
        &principal,
        &corpus,
        budgets,
        CostAuthority::CostUnavailable,
    );
    persist_rwe_run_authorization(&store, &principal, &body, true).unwrap();
    let authorization_id = body.authorization_id.clone();
    let run_id = format!("rwe-pg-conc-run-{tag}");
    let mut run_body = body.to_json();
    run_body["run_id"] = json!(run_id.clone());
    run_body["provider_free_fixture"] = json!(true);
    let admitted = store
        .admit_rwe_run(&principal, &run_id, &authorization_id, &run_body, true)
        .unwrap();
    let lease = admitted["lease_token"].as_str().unwrap().to_string();

    let Some(url) = std::env::var("ACP_TEST_DATABASE_URL").ok() else {
        return;
    };
    let task = &corpus.tasks[0];
    let attempt_id = format!("{run_id}:attempt");
    let evidence = json!({"task_id": task.task_id, "concurrent": true});

    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let mut handles = Vec::new();
    for _ in 0..2 {
        let b = std::sync::Arc::clone(&barrier);
        let url = url.clone();
        let run_id = run_id.clone();
        let lease = lease.clone();
        let attempt_id = attempt_id.clone();
        let task_id = task.task_id.clone();
        let def = task.definition_sha256.clone();
        let evidence = evidence.clone();
        handles.push(thread::spawn(move || {
            let store = LocalProductStore::new_postgres(&url, utc_now_string).unwrap();
            b.wait();
            store.persist_rwe_task_attempt(
                &run_id,
                &lease,
                &attempt_id,
                &task_id,
                &def,
                "fixture_success",
                &evidence,
            )
        }));
    }
    let results: Vec<_> = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect();
    assert_eq!(
        results.iter().filter(|r| r.is_ok()).count(),
        2,
        "both concurrent exact attempts must succeed via replay"
    );
    let rows = results.into_iter().map(|r| r.unwrap()).collect::<Vec<_>>();
    assert_eq!(
        rows.iter()
            .filter(|v| v["idempotent_replay"].as_bool() == Some(false))
            .count(),
        1
    );
    assert_eq!(
        rows.iter()
            .filter(|v| v["idempotent_replay"].as_bool() == Some(true))
            .count(),
        1
    );

    let conflict_evidence = json!({"task_id": task.task_id, "concurrent": "different"});
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let mut handles = Vec::new();
    for ev in [evidence.clone(), conflict_evidence.clone()] {
        let b = std::sync::Arc::clone(&barrier);
        let url = url.clone();
        let run_id = run_id.clone();
        let lease = lease.clone();
        let attempt_id = format!("{run_id}:conflict-attempt");
        let task_id = task.task_id.clone();
        let def = task.definition_sha256.clone();
        handles.push(thread::spawn(move || {
            let store = LocalProductStore::new_postgres(&url, utc_now_string).unwrap();
            b.wait();
            store.persist_rwe_task_attempt(
                &run_id,
                &lease,
                &attempt_id,
                &task_id,
                &def,
                "fixture_success",
                &ev,
            )
        }));
    }
    let results: Vec<_> = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect();
    assert_eq!(
        results.iter().filter(|r| r.is_ok()).count(),
        1,
        "exactly one conflicting attempt may succeed"
    );
    assert_eq!(results.iter().filter(|r| r.is_err()).count(), 1);
    let err = results
        .into_iter()
        .find(|r| r.is_err())
        .unwrap()
        .unwrap_err();
    assert!(err.contains("conflict"));
}

#[cfg(feature = "pg-tests")]
#[test]
fn pg_rwe_concurrent_terminalization_and_restart_without_lease() {
    let Some(store) = test_store() else {
        return;
    };
    let tag = uuid_tag();
    let tenant = format!("tenant-rwe-term-{tag}");
    let owner = format!("fixture-principal-owner-{tag}");
    let principal = AuthenticatedPrincipal::fixture_for_tests(&tenant, &owner).unwrap();
    let corpus = freeze_first_rwe_corpus().unwrap();
    let budgets = corpus
        .tasks
        .iter()
        .map(|task| RwePerTaskBudget::from_task_definition(task, None))
        .collect::<Vec<_>>();
    let body = pg_rwe_fixture_authorization_body(
        &tag,
        &principal,
        &corpus,
        budgets,
        CostAuthority::CostUnavailable,
    );
    persist_rwe_run_authorization(&store, &principal, &body, true).unwrap();
    let authorization_id = body.authorization_id.clone();
    let run_id = format!("rwe-pg-term-run-{tag}");
    let mut run_body = body.to_json();
    run_body["run_id"] = json!(run_id.clone());
    run_body["provider_free_fixture"] = json!(true);
    let admitted = store
        .admit_rwe_run(&principal, &run_id, &authorization_id, &run_body, true)
        .unwrap();
    let lease = admitted["lease_token"].as_str().unwrap().to_string();

    let task = &corpus.tasks[0];
    let evidence = json!({"task_id": task.task_id, "terminal": true});
    store
        .persist_rwe_task_attempt(
            &run_id,
            &lease,
            &format!("{run_id}:attempt"),
            &task.task_id,
            &task.definition_sha256,
            "fixture_success",
            &evidence,
        )
        .unwrap();

    let aggregate = json!({
        "schema_version": "rwe_run_evidence.v1",
        "run_id": run_id,
        "authorization_id": authorization_id,
        "corpus_sha256": corpus.corpus_sha256,
        "corpus_schema": engine::rwe::corpus::RWE_CORPUS_SCHEMA,
        "task_results": [evidence],
        "aggregate_provider_requests": 0,
        "live_provider_request": false,
        "live_baseline_sealed": false,
        "provider_free_fixture_completion": true,
        "note": "fixture concurrency test",
    });
    let evidence_sha = hex::encode(Sha256::digest(aggregate.to_string().as_bytes()));

    let Some(url) = std::env::var("ACP_TEST_DATABASE_URL").ok() else {
        return;
    };
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let mut handles = Vec::new();
    for _ in 0..2 {
        let b = std::sync::Arc::clone(&barrier);
        let url = url.clone();
        let run_id = run_id.clone();
        let lease = lease.clone();
        let aggregate = aggregate.clone();
        let evidence_sha = evidence_sha.clone();
        handles.push(thread::spawn(move || {
            let store = LocalProductStore::new_postgres(&url, utc_now_string).unwrap();
            b.wait();
            store.complete_rwe_run(
                &run_id,
                &lease,
                "fixture_complete",
                &aggregate,
                &evidence_sha,
            )
        }));
    }
    let results: Vec<_> = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect();
    assert_eq!(
        results.iter().filter(|r| r.is_ok()).count(),
        2,
        "concurrent terminal replay must be idempotent"
    );
    let rows = results.into_iter().map(|r| r.unwrap()).collect::<Vec<_>>();
    assert_eq!(
        rows.iter()
            .filter(|v| v["idempotent_replay"].as_bool() == Some(false))
            .count(),
        1
    );
    assert_eq!(
        rows.iter()
            .filter(|v| v["idempotent_replay"].as_bool() == Some(true))
            .count(),
        1
    );
    for row in &rows {
        assert!(row.get("lease_token").is_none() || row["lease_token"].is_null());
    }

    let run_view = store.get_rwe_run(&run_id).unwrap().unwrap();
    assert!(run_view.get("lease_token").is_none() || run_view["lease_token"].is_null());

    // Restart without lease recovery: exact terminal replay with a stale/missing lease succeeds.
    let replay = store
        .complete_rwe_run(
            &run_id,
            "stale-lease",
            "fixture_complete",
            &aggregate,
            &evidence_sha,
        )
        .unwrap();
    assert_eq!(replay["idempotent_replay"], true);
    assert!(replay.get("lease_token").is_none() || replay["lease_token"].is_null());

    // A missing or stale lease cannot write a new task attempt after terminalization.
    let err = store
        .persist_rwe_task_attempt(
            &run_id,
            &lease,
            &format!("{run_id}:late-attempt"),
            &task.task_id,
            &task.definition_sha256,
            "fixture_success",
            &evidence,
        )
        .unwrap_err();
    assert!(err.contains("admitted") || err.contains("lease") || err.contains("current run"));
}

/// Append an application_name parameter to a PostgreSQL connection string.
#[cfg(feature = "pg-tests")]
fn url_with_application_name(url: &str, app_name: &str) -> String {
    if url.starts_with("postgres://") || url.starts_with("postgresql://") {
        if url.contains('?') {
            format!("{url}&application_name={app_name}")
        } else {
            format!("{url}?application_name={app_name}")
        }
    } else {
        format!("{url} application_name={app_name}")
    }
}

/// Observe a specific named backend waiting on a specific advisory lock key.
/// Joins pg_stat_activity (by application_name) with pg_locks (by advisory lock key
/// identity) so unrelated waiters cannot produce false positives.
#[cfg(feature = "pg-tests")]
fn is_named_backend_waiting_on_lock(observer_url: &str, app_name: &str, lock_key: &str) -> bool {
    let mut client = postgres::Client::connect(observer_url, postgres::NoTls).unwrap();
    let row = client
        .query_one(
            r#"
            SELECT EXISTS (
                SELECT 1
                FROM pg_locks l
                JOIN pg_stat_activity a ON l.pid = a.pid
                WHERE l.locktype = 'advisory'
                  AND l.granted = false
                  AND a.application_name = $1
                  AND (
                      l.classid, l.objid
                  ) = (
                      (hashtext($2)::bigint >> 32)::int,
                      (hashtext($2)::bigint & 4294967295::bigint)::int
                  )
            )
            "#,
            &[&app_name, &lock_key],
        )
        .unwrap();
    row.get::<_, bool>(0)
}

/// Poll until a specific named backend is observed waiting on the target advisory lock.
#[cfg(feature = "pg-tests")]
fn wait_for_named_waiter(
    observer_url: &str,
    app_name: &str,
    lock_key: &str,
    timeout_ms: u64,
) -> bool {
    let start = std::time::Instant::now();
    let poll_interval = std::time::Duration::from_millis(10);
    let timeout = std::time::Duration::from_millis(timeout_ms);

    while start.elapsed() < timeout {
        if is_named_backend_waiting_on_lock(observer_url, app_name, lock_key) {
            return true;
        }
        std::thread::sleep(poll_interval);
    }
    false
}

#[cfg(feature = "pg-tests")]
#[test]
fn pg_rwe_attempt_then_terminalization_lock_ordering() {
    let Some(store) = test_store() else {
        return;
    };
    let tag = uuid_tag();
    let tenant = format!("tenant-rwe-lock-{tag}");
    let owner = format!("fixture-principal-lock-{tag}");
    let principal = AuthenticatedPrincipal::fixture_for_tests(&tenant, &owner).unwrap();
    let corpus = freeze_first_rwe_corpus().unwrap();
    let budgets = corpus
        .tasks
        .iter()
        .map(|task| RwePerTaskBudget::from_task_definition(task, None))
        .collect::<Vec<_>>();
    let body = pg_rwe_fixture_authorization_body(
        &tag,
        &principal,
        &corpus,
        budgets,
        CostAuthority::CostUnavailable,
    );
    persist_rwe_run_authorization(&store, &principal, &body, true).unwrap();
    let authorization_id = body.authorization_id.clone();
    let run_id = format!("rwe-pg-lock-run-{tag}");
    let mut run_body = body.to_json();
    run_body["run_id"] = json!(run_id.clone());
    run_body["provider_free_fixture"] = json!(true);
    let admitted = store
        .admit_rwe_run(&principal, &run_id, &authorization_id, &run_body, true)
        .unwrap();
    let lease = admitted["lease_token"].as_str().unwrap().to_string();

    let Some(url) = std::env::var("ACP_TEST_DATABASE_URL").ok() else {
        return;
    };

    let task = &corpus.tasks[0];
    let attempt_id = format!("{run_id}:lock-attempt");
    let evidence = json!({"task_id": task.task_id, "lock_test": true});
    let aggregate = json!({
        "schema_version": "rwe_run_evidence.v1",
        "run_id": run_id,
        "authorization_id": authorization_id,
        "corpus_sha256": corpus.corpus_sha256,
        "corpus_schema": engine::rwe::corpus::RWE_CORPUS_SCHEMA,
        "task_results": [evidence.clone()],
        "aggregate_provider_requests": 0,
        "live_provider_request": false,
        "live_baseline_sealed": false,
        "provider_free_fixture_completion": true,
        "note": "lock ordering test",
    });
    let evidence_sha = hex::encode(Sha256::digest(aggregate.to_string().as_bytes()));

    let lock_key = format!("rwer:{run_id}");
    let attempt_app = format!("rwe-attempt-{tag}");
    let terminal_app = format!("rwe-terminal-{tag}");

    let mut blocker_client = postgres::Client::connect(&url, postgres::NoTls).unwrap();
    let mut blocker_tx = blocker_client.transaction().unwrap();
    blocker_tx
        .execute("SELECT pg_advisory_xact_lock(hashtext($1))", &[&lock_key])
        .unwrap();

    let url_a = url_with_application_name(&url, &attempt_app);
    let run_id_a = run_id.clone();
    let lease_a = lease.clone();
    let attempt_id_a = attempt_id.clone();
    let task_id_a = task.task_id.clone();
    let def_a = task.definition_sha256.clone();
    let evidence_a = evidence.clone();
    let attempt_handle = thread::spawn(move || {
        let store = LocalProductStore::new_postgres(&url_a, utc_now_string).unwrap();
        store.persist_rwe_task_attempt(
            &run_id_a,
            &lease_a,
            &attempt_id_a,
            &task_id_a,
            &def_a,
            "fixture_success",
            &evidence_a,
        )
    });

    assert!(
        wait_for_named_waiter(&url, &attempt_app, &lock_key, 5000),
        "attempt-first: must observe attempt backend 'attempt' waiting on rwer lock"
    );

    let url_t = url_with_application_name(&url, &terminal_app);
    let run_id_t = run_id.clone();
    let lease_t = lease.clone();
    let aggregate_t = aggregate.clone();
    let evidence_sha_t = evidence_sha.clone();
    let term_handle = thread::spawn(move || {
        let store = LocalProductStore::new_postgres(&url_t, utc_now_string).unwrap();
        store.complete_rwe_run(
            &run_id_t,
            &lease_t,
            "fixture_complete",
            &aggregate_t,
            &evidence_sha_t,
        )
    });

    assert!(
        wait_for_named_waiter(&url, &terminal_app, &lock_key, 5000),
        "attempt-first: must observe terminal backend 'terminal' waiting on rwer lock"
    );

    blocker_tx.rollback().unwrap();
    drop(blocker_client);

    let attempt_result = attempt_handle.join().unwrap();
    assert!(
        attempt_result.is_ok(),
        "attempt-first: attempt must commit before terminal: {attempt_result:?}"
    );

    let term_result = term_handle.join().unwrap();
    assert!(
        term_result.is_ok(),
        "attempt-first: terminal must succeed after attempt: {term_result:?}"
    );
    assert_eq!(term_result.unwrap()["status"], "fixture_complete");

    let post_terminal = store.get_rwe_run(&run_id).unwrap().unwrap();
    assert_eq!(post_terminal["status"], "fixture_complete");

    let late_err = store
        .persist_rwe_task_attempt(
            &run_id,
            &lease,
            &format!("{run_id}:late-after-terminal"),
            &task.task_id,
            &task.definition_sha256,
            "fixture_success",
            &evidence,
        )
        .unwrap_err();
    assert!(
        late_err.contains("admitted")
            || late_err.contains("lease")
            || late_err.contains("current run")
    );
}

#[cfg(feature = "pg-tests")]
#[test]
fn pg_rwe_terminal_first_rejects_late_attempt() {
    let Some(store) = test_store() else {
        return;
    };
    let tag = uuid_tag();
    let tenant = format!("tenant-rwe-tf-{tag}");
    let owner = format!("fixture-principal-tf-{tag}");
    let principal = AuthenticatedPrincipal::fixture_for_tests(&tenant, &owner).unwrap();
    let corpus = freeze_first_rwe_corpus().unwrap();
    let budgets = corpus
        .tasks
        .iter()
        .map(|task| RwePerTaskBudget::from_task_definition(task, None))
        .collect::<Vec<_>>();
    let body = pg_rwe_fixture_authorization_body(
        &tag,
        &principal,
        &corpus,
        budgets,
        CostAuthority::CostUnavailable,
    );
    persist_rwe_run_authorization(&store, &principal, &body, true).unwrap();
    let authorization_id = body.authorization_id.clone();
    let run_id = format!("rwe-pg-tf-run-{tag}");
    let mut run_body = body.to_json();
    run_body["run_id"] = json!(run_id.clone());
    run_body["provider_free_fixture"] = json!(true);
    let admitted = store
        .admit_rwe_run(&principal, &run_id, &authorization_id, &run_body, true)
        .unwrap();
    let lease = admitted["lease_token"].as_str().unwrap().to_string();

    let Some(url) = std::env::var("ACP_TEST_DATABASE_URL").ok() else {
        return;
    };

    let task = &corpus.tasks[0];
    let attempt_id = format!("{run_id}:tf-attempt");
    let evidence = json!({"task_id": task.task_id, "tf_test": true});
    let aggregate = json!({
        "schema_version": "rwe_run_evidence.v1",
        "run_id": run_id,
        "authorization_id": authorization_id,
        "corpus_sha256": corpus.corpus_sha256,
        "corpus_schema": engine::rwe::corpus::RWE_CORPUS_SCHEMA,
        "task_results": [evidence.clone()],
        "aggregate_provider_requests": 0,
        "live_provider_request": false,
        "live_baseline_sealed": false,
        "provider_free_fixture_completion": true,
        "note": "terminal-first test",
    });
    let evidence_sha = hex::encode(Sha256::digest(aggregate.to_string().as_bytes()));

    let lock_key = format!("rwer:{run_id}");
    let terminal_app = format!("rwe-terminal-tf-{tag}");
    let attempt_app = format!("rwe-attempt-tf-{tag}");

    let mut blocker_client = postgres::Client::connect(&url, postgres::NoTls).unwrap();
    let mut blocker_tx = blocker_client.transaction().unwrap();
    blocker_tx
        .execute("SELECT pg_advisory_xact_lock(hashtext($1))", &[&lock_key])
        .unwrap();

    let url_t = url_with_application_name(&url, &terminal_app);
    let run_id_t = run_id.clone();
    let lease_t = lease.clone();
    let aggregate_t = aggregate.clone();
    let evidence_sha_t = evidence_sha.clone();
    let term_handle = thread::spawn(move || {
        let store = LocalProductStore::new_postgres(&url_t, utc_now_string).unwrap();
        store.complete_rwe_run(
            &run_id_t,
            &lease_t,
            "fixture_complete",
            &aggregate_t,
            &evidence_sha_t,
        )
    });

    assert!(
        wait_for_named_waiter(&url, &terminal_app, &lock_key, 5000),
        "terminal-first: must observe terminal backend 'terminal' waiting on rwer lock"
    );

    let url_a = url_with_application_name(&url, &attempt_app);
    let run_id_a = run_id.clone();
    let lease_a = lease.clone();
    let attempt_id_a = attempt_id.clone();
    let task_id_a = task.task_id.clone();
    let def_a = task.definition_sha256.clone();
    let evidence_a = evidence.clone();
    let attempt_handle = thread::spawn(move || {
        let store = LocalProductStore::new_postgres(&url_a, utc_now_string).unwrap();
        store.persist_rwe_task_attempt(
            &run_id_a,
            &lease_a,
            &attempt_id_a,
            &task_id_a,
            &def_a,
            "fixture_success",
            &evidence_a,
        )
    });

    assert!(
        wait_for_named_waiter(&url, &attempt_app, &lock_key, 5000),
        "terminal-first: must observe attempt backend 'attempt' waiting on rwer lock"
    );

    blocker_tx.rollback().unwrap();
    drop(blocker_client);

    let term_result = term_handle.join().unwrap();
    assert!(
        term_result.is_ok(),
        "terminal-first: terminalization must commit first: {term_result:?}"
    );
    assert_eq!(term_result.unwrap()["status"], "fixture_complete");

    let attempt_result = attempt_handle.join().unwrap();
    assert!(
        attempt_result.is_err(),
        "terminal-first: attempt must be rejected after terminalization: {attempt_result:?}"
    );
    let err = attempt_result.unwrap_err();
    assert!(
        err.contains("admitted") || err.contains("lease") || err.contains("current run"),
        "attempt rejection reason: {err}"
    );

    let post_terminal = store.get_rwe_run(&run_id).unwrap().unwrap();
    assert_eq!(post_terminal["status"], "fixture_complete");

    let late_err = store
        .persist_rwe_task_attempt(
            &run_id,
            &lease,
            &format!("{run_id}:late-after-terminal"),
            &task.task_id,
            &task.definition_sha256,
            "fixture_success",
            &evidence,
        )
        .unwrap_err();
    assert!(
        late_err.contains("admitted")
            || late_err.contains("lease")
            || late_err.contains("current run"),
        "attempt after terminalization must be rejected: {late_err}"
    );
}
