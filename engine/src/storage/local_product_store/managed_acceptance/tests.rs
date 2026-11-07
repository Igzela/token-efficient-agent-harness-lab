use super::*;
use crate::cli::codex_partial_mediation_authority_decision::OPERATOR_RISK_ACCEPTANCE_PHRASE;
use crate::product_golden_path::{
    validate_intake, ProductExecutorPolicy, ProductTaskIntakeRequest, ProductVerificationCommand,
    PRODUCT_TASK_GATE,
};
use crate::provider::config::{CredentialRef, ProviderConfig};
use crate::provider::credential::CredentialBoundary;
use crate::provider::managed_deepseek::{
    DeepSeekPriceProfile, DeepSeekProtocol, ManagedCallLimits, ManagedDeepSeekProvider,
    DEEPSEEK_CREDENTIAL_REFERENCE,
};
use crate::provider::managed_deepseek_executor::{
    ManagedDeepSeekExecutorConfig, ManagedDeepSeekNodeExecutor,
};
use crate::provider::transport::{HttpError, HttpRequest, HttpResponse, HttpTransport};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier, Mutex};
use std::thread;
use tempfile::tempdir;

fn store() -> (tempfile::TempDir, LocalProductStore) {
    let dir = tempdir().unwrap();
    let path = dir.path().join("ma.db");
    let store =
        LocalProductStore::new_with_clock(&path, || "2026-07-25T12:00:00Z".to_string()).unwrap();
    (dir, store)
}

struct DelegatedRouteMockTransport {
    responses: Mutex<Vec<HttpResponse>>,
    sends: AtomicUsize,
}

#[async_trait::async_trait]
impl HttpTransport for DelegatedRouteMockTransport {
    async fn send(&self, _request: &HttpRequest) -> Result<HttpResponse, HttpError> {
        self.sends.fetch_add(1, Ordering::SeqCst);
        let mut responses = self
            .responses
            .lock()
            .map_err(|_| HttpError::Connection("mock transport poisoned".into()))?;
        if responses.is_empty() {
            Err(HttpError::Connection(
                "unexpected fourth provider request".into(),
            ))
        } else {
            Ok(responses.remove(0))
        }
    }
}

struct DelegatedOutcomeUnknownTransport {
    sends: AtomicUsize,
}

#[async_trait::async_trait]
impl HttpTransport for DelegatedOutcomeUnknownTransport {
    async fn send(&self, _request: &HttpRequest) -> Result<HttpResponse, HttpError> {
        self.sends.fetch_add(1, Ordering::SeqCst);
        Err(HttpError::Connection(
            "mock post-send connection outcome is unknown".into(),
        ))
    }
}

struct DelegatedBlockedImplementationTransport {
    responses: Mutex<Vec<HttpResponse>>,
    sends: AtomicUsize,
    implementation_entered: Arc<Barrier>,
    implementation_release: Arc<Barrier>,
}

#[async_trait::async_trait]
impl HttpTransport for DelegatedBlockedImplementationTransport {
    async fn send(&self, _request: &HttpRequest) -> Result<HttpResponse, HttpError> {
        let ordinal = self.sends.fetch_add(1, Ordering::SeqCst);
        if ordinal == 1 {
            self.implementation_entered.wait();
            self.implementation_release.wait();
        }
        let mut responses = self
            .responses
            .lock()
            .map_err(|_| HttpError::Connection("blocked transport poisoned".into()))?;
        if responses.is_empty() {
            Err(HttpError::Connection(
                "unexpected blocked transport provider request".into(),
            ))
        } else {
            Ok(responses.remove(0))
        }
    }
}

fn delegated_openai_response(id: &str, model: &str, content: Value) -> HttpResponse {
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

fn delegated_mock_provider(
    model: &str,
    transport: Arc<dyn HttpTransport>,
) -> Arc<ManagedDeepSeekProvider> {
    let config = ProviderConfig::new(
        "deepseek-managed-test",
        "openai_compatible",
        "https://api.deepseek.com",
        model,
        DEEPSEEK_CREDENTIAL_REFERENCE,
        "2026-07-30T00:00:00Z",
    );
    let credential = CredentialRef::new(
        DEEPSEEK_CREDENTIAL_REFERENCE,
        "env",
        "***",
        "provider:deepseek",
        "2026-07-30T00:00:00Z",
    );
    Arc::new(ManagedDeepSeekProvider::new_openai(
        config,
        CredentialBoundary::for_test(),
        credential,
        transport,
    ))
}

fn decision_body(decision_id: &str) -> Value {
    decision_body_for_attempt(decision_id, "attempt-1")
}

fn decision_body_for_attempt(decision_id: &str, attempt_id: &str) -> Value {
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
            "admitted_endpoint_paths": vec!["/v1/responses"],
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

fn seed_key(store: &LocalProductStore, tenant_id: &str, key_id: &str) {
    store
        .record_api_key_metadata_for_tenant(
            tenant_id,
            key_id,
            "user-operator-1",
            "operator",
            &ALL_MANAGED_ACCEPTANCE_SCOPES
                .iter()
                .map(|s| (*s).to_string())
                .collect::<Vec<_>>(),
            "test-setup",
        )
        .unwrap();
}

fn spend_req(risk_id: &str, attempt_id: &str) -> SpendAuthorizationRequest {
    SpendAuthorizationRequest {
        risk_authorization_id: risk_id.into(),
        product_task_id: "ptask-1".into(),
        workflow_id: Some("wf-1".into()),
        workflow_node_id: Some("node-1".into()),
        execution_id: format!("codex-attempt-{attempt_id}"),
        attempt_id: attempt_id.into(),
        binary_path: "/usr/bin/codex".into(),
        binary_version: "0.145.0".into(),
        binary_sha256: "ab".repeat(32),
        runtime_profile_sha256: None,
        capability_probe_sha256: None,
        provider_kind: "openai_compatible".into(),
        provider_host: "api.openai.com".into(),
        provider_base_url: "https://api.openai.com/v1".into(),
        admitted_endpoint_paths: vec!["/v1/responses".into()],
        model: "gpt-5.6-luna".into(),
        target_repo: "org/disposable-trial".into(),
        target_main_sha: "a".repeat(40),
        output_branch_prefix: "acp/".into(),
        draft_pr_only: true,
        max_provider_requests: 1,
        max_retries: 0,
        max_input_tokens: 8000,
        max_output_tokens: 4000,
        max_total_tokens: 12000,
        max_wall_time_ms: 300000,
        cost_authority: CostAuthority::CostUnavailable,
        cancellation_identity: "cancel-1".into(),
        rollback_identity: "rollback-1".into(),
    }
}

fn attempt_body_for(req: &SpendAuthorizationRequest) -> Value {
    // Mirror spend body fields used by build_attempt_authority_manifest.
    let spend_like = json!({
        "product_task_id": req.product_task_id,
        "workflow_id": req.workflow_id,
        "workflow_node_id": req.workflow_node_id,
        "execution_id": req.execution_id,
        "attempt_id": req.attempt_id,
        "binary_path": req.binary_path,
        "binary_version": req.binary_version,
        "binary_sha256": req.binary_sha256,
        "runtime_profile_sha256": req.runtime_profile_sha256,
        "capability_probe_sha256": req.capability_probe_sha256,
        "provider_kind": req.provider_kind,
        "provider_host": req.provider_host,
        "provider_base_url": req.provider_base_url,
        "admitted_endpoint_paths": req.admitted_endpoint_paths,
        "model": req.model,
        "target_repo": req.target_repo,
        "target_main_sha": req.target_main_sha,
        "output_branch_prefix": req.output_branch_prefix,
        "draft_pr_only": req.draft_pr_only,
        "max_provider_requests": req.max_provider_requests,
        "max_retries": req.max_retries,
        "max_input_tokens": req.max_input_tokens,
        "max_output_tokens": req.max_output_tokens,
        "max_total_tokens": req.max_total_tokens,
        "max_wall_time_ms": req.max_wall_time_ms,
        "cost_authority": req.cost_authority.to_json(),
        "cancellation_identity": req.cancellation_identity,
        "rollback_identity": req.rollback_identity,
        "decision_body_sha256": Value::Null,
        "spend_authorization_id": Value::Null,
    });
    let manifest = build_attempt_authority_manifest(&spend_like).unwrap();
    let mut body = spend_like;
    if let Value::Object(ref mut map) = body {
        map.insert(
            "manifest_sha256".into(),
            manifest
                .get("manifest_sha256")
                .cloned()
                .unwrap_or(Value::Null),
        );
        map.insert("manifest".into(), manifest);
    }
    body
}

#[test]
fn free_form_from_api_key_removed_store_auth_required() {
    let (_dir, store) = store();
    let err = store
        .authenticate_managed_acceptance_principal("t1", "agent", None)
        .unwrap_err();
    assert!(
        err.contains("not admitted") || err.contains("not found"),
        "{err}"
    );
    seed_key(&store, "tenant-a", "key_operator_ok");
    let p = store
        .authenticate_managed_acceptance_principal("tenant-a", "key_operator_ok", Some(1.0))
        .unwrap();
    assert_eq!(p.principal_id(), "key_operator_ok");
    assert!(p.has_scope(SCOPE_RISK_ACKNOWLEDGE));
}

#[test]
fn managed_principal_auth_rejects_unbound_metadata() {
    let (_dir, store) = store();
    store
        .record_api_key_metadata(
            "unbound-operator",
            "operator-user",
            "operator",
            &[SCOPE_RISK_ACKNOWLEDGE.to_string()],
            "test-setup",
        )
        .unwrap();
    let error = store
        .authenticate_managed_acceptance_principal("tenant-a", "unbound-operator", Some(1.0))
        .unwrap_err();
    assert!(error.contains("tenant binding"), "{error}");
}

#[test]
fn persisted_key_metadata_reissues_principal_after_restart_without_secret() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("operator-key-reissue.db");
    let scopes = vec![
        SCOPE_RISK_ACKNOWLEDGE.to_string(),
        SCOPE_DELEGATED_EXECUTE.to_string(),
    ];
    let store =
        LocalProductStore::new_with_clock(&path, || "2026-07-25T12:00:00Z".to_string()).unwrap();
    store
        .record_api_key_metadata_for_tenant(
            "tenant-a",
            "restart-operator-key",
            "operator-user",
            "operator",
            &scopes,
            "canonical-key-issuer",
        )
        .unwrap();
    let metadata = store
        .get_api_key_metadata("restart-operator-key")
        .unwrap()
        .unwrap();
    assert_eq!(metadata["scopes"], json!(scopes));
    assert!(metadata.get("raw_key").is_none());
    drop(store);

    let restarted =
        LocalProductStore::new_with_clock(&path, || "2026-07-25T12:00:01Z".to_string()).unwrap();
    let principal = restarted
        .authenticate_managed_acceptance_principal("tenant-a", "restart-operator-key", Some(1.0))
        .unwrap();
    assert_eq!(principal.principal_id(), "restart-operator-key");
    assert!(principal.has_scope(SCOPE_RISK_ACKNOWLEDGE));
    assert!(principal.has_scope(SCOPE_DELEGATED_EXECUTE));
    assert!(!principal.has_scope(SCOPE_DELEGATED_ARTIFACT_CONFIRM));
}

#[test]
fn expired_api_key_principal_is_rejected_against_wall_clock() {
    let (_dir, store) = store();
    // Key expired at unix 100. A mistaken cost-cap "now" of 0.50 never exceeds
    // real production expiry timestamps and would keep keys alive forever.
    store
        .record_api_key_metadata_with_expiry_for_tenant(
            "tenant-a",
            "key_expired",
            "operator",
            "operator",
            &ALL_MANAGED_ACCEPTANCE_SCOPES
                .iter()
                .map(|scope| (*scope).to_string())
                .collect::<Vec<_>>(),
            Some(100.0),
            "test",
        )
        .unwrap();
    // Mistaken cost-as-clock still sees the key as live (0.50 < 100).
    store
        .authenticate_managed_acceptance_principal("tenant-a", "key_expired", Some(0.50))
        .expect("cost-as-clock incorrectly keeps the key live");
    // Production callers must use wall clock (None) and reject after expiry.
    let wall_clock = store
        .authenticate_managed_acceptance_principal("tenant-a", "key_expired", None)
        .unwrap_err();
    assert!(wall_clock.contains("expired"), "{wall_clock}");
    // Explicit frozen "now" after expiry fails; before expiry still works.
    let after = store
        .authenticate_managed_acceptance_principal("tenant-a", "key_expired", Some(100.5))
        .unwrap_err();
    assert!(after.contains("expired"), "{after}");
    store
        .authenticate_managed_acceptance_principal("tenant-a", "key_expired", Some(50.0))
        .unwrap();
}

#[test]
fn decision_creation_rejects_non_draft_status_without_transition_receipt() {
    let (_dir, store) = store();
    let residual = "a9".repeat(32);
    let parameter_error = store
        .upsert_managed_acceptance_decision(
            "tenant-a",
            &decision_body("mad-non-draft-parameter"),
            &residual,
            "operator_accepted",
            None,
            Some("2026-07-26T00:00:00Z"),
        )
        .expect_err("a decision cannot be created in an accepted state");
    assert!(parameter_error.contains("transitions are receipt-owned"));

    let mut self_declared = decision_body("mad-non-draft-body");
    self_declared["status"] = json!("operator_accepted");
    let body_error = store
        .upsert_managed_acceptance_decision(
            "tenant-a",
            &self_declared,
            &residual,
            "draft_pending_operator",
            None,
            Some("2026-07-26T00:00:00Z"),
        )
        .expect_err("a decision body cannot self-declare accepted status");
    assert!(body_error.contains("is not transition evidence"));
}

#[test]
fn risk_ack_store_derived_spend_admit_idempotent_and_conflict() {
    let (dir, store) = store();
    let principal =
        AuthenticatedPrincipal::fixture_for_tests("tenant-a", "fixture-principal-alice").unwrap();
    let residual = "ab".repeat(32);
    let body = decision_body("mad-test-1");
    let decision = store
        .upsert_managed_acceptance_decision(
            "tenant-a",
            &body,
            &residual,
            "draft_pending_operator",
            None,
            Some("2026-07-26T00:00:00Z"),
        )
        .unwrap();
    let dsha = decision["decision_body_sha256"]
        .as_str()
        .unwrap()
        .to_string();
    let auth = store
        .accept_managed_acceptance_decision(
            &principal,
            &RiskAcknowledgementRequest {
                decision_id: "mad-test-1".into(),
                expected_decision_body_sha256: dsha.clone(),
                expected_residual_finding_sha256: residual.clone(),
                submitted_phrase: OPERATOR_RISK_ACCEPTANCE_PHRASE.into(),
                explicit_go: true,
            },
        )
        .unwrap();
    assert_eq!(auth["execution_granted"], false);
    assert!(auth["fixture_only"].as_bool().unwrap_or(false));
    let auth_id = auth["authorization_id"].as_str().unwrap().to_string();

    // exact risk replay
    let auth2 = store
        .accept_managed_acceptance_decision(
            &principal,
            &RiskAcknowledgementRequest {
                decision_id: "mad-test-1".into(),
                expected_decision_body_sha256: dsha.clone(),
                expected_residual_finding_sha256: residual.clone(),
                submitted_phrase: OPERATOR_RISK_ACCEPTANCE_PHRASE.into(),
                explicit_go: true,
            },
        )
        .unwrap();
    assert_eq!(auth2["authorization_id"], auth_id);

    let spend_request = spend_req(&auth_id, "attempt-1");
    let spend = store
        .issue_managed_acceptance_spend_authorization(&principal, &spend_request)
        .unwrap();
    let spend_id = spend["spend_authorization_id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(spend["status"], "active");
    let attempt_body = attempt_body_for(&spend_request);
    let unauthorized =
        AuthenticatedPrincipal::fixture_for_tests("tenant-a", "fixture-principal-mallory").unwrap();
    let unauthorized_error = store
        .admit_managed_acceptance_attempt_for_test(
            &unauthorized,
            "attempt-1",
            &attempt_body,
            &spend_id,
            true,
        )
        .expect_err("another principal must not consume this spend");
    assert!(
        unauthorized_error.contains("spend principal mismatch"),
        "{unauthorized_error}"
    );
    assert_eq!(
        store
            .get_managed_acceptance_spend_authorization(&spend_id)
            .unwrap()
            .unwrap()["status"],
        "active"
    );
    let production_fixture_error = store
        .admit_managed_acceptance_attempt(&principal, "attempt-1", &attempt_body, &spend_id)
        .expect_err("production admission must reject fixture authority");
    assert!(production_fixture_error.contains("fixture principal"));
    assert_eq!(
        store
            .get_managed_acceptance_spend_authorization(&spend_id)
            .unwrap()
            .unwrap()["status"],
        "active"
    );
    let a1 = store
        .admit_managed_acceptance_attempt_for_test(
            &principal,
            "attempt-1",
            &attempt_body,
            &spend_id,
            true,
        )
        .unwrap();
    assert_eq!(a1["status"], "admitted");
    assert!(a1["lease_token"].as_str().unwrap().starts_with("lease-"));
    let lease = a1["lease_token"].as_str().unwrap().to_string();

    let a2 = store
        .admit_managed_acceptance_attempt_for_test(
            &principal,
            "attempt-1",
            &attempt_body,
            &spend_id,
            true,
        )
        .unwrap();
    assert_eq!(a2["idempotent_replay"], true);
    let restarted = LocalProductStore::new_with_clock(dir.path().join("ma.db"), || {
        "2026-07-25T12:00:00Z".to_string()
    })
    .unwrap();
    let restarted_replay = restarted
        .admit_managed_acceptance_attempt_for_test(
            &principal,
            "attempt-1",
            &attempt_body,
            &spend_id,
            true,
        )
        .unwrap();
    assert_eq!(restarted_replay["idempotent_replay"], true);

    // spend consumed — cannot re-admit different attempt
    let spend_row = store
        .get_managed_acceptance_spend_authorization(&spend_id)
        .unwrap()
        .unwrap();
    assert_eq!(spend_row["status"], "consumed");

    let mut conflict_body = attempt_body.clone();
    conflict_body["model"] = json!("conflict-model");
    if let Some(manifest) = conflict_body.get_mut("manifest") {
        manifest["model"] = json!("conflict-model");
    }
    let conflict = store.admit_managed_acceptance_attempt_for_test(
        &principal,
        "attempt-1",
        &conflict_body,
        &spend_id,
        true,
    );
    assert!(conflict.unwrap_err().contains("conflict"));

    store
        .complete_managed_acceptance_attempt(
            "attempt-1",
            &lease,
            "succeeded",
            "succeeded_fixture",
            &json!({"ok": true}),
        )
        .unwrap();
    let late = store.complete_managed_acceptance_attempt(
        "attempt-1",
        &lease,
        "failed",
        "failed_provider",
        &json!({"ok": false}),
    );
    assert!(late.unwrap_err().contains("late terminal"));
    // Exact terminal replay requires the current lease; a reconstructed
    // receipt with an arbitrary lease is not an authorized replay.
    let exact = store
        .complete_managed_acceptance_attempt(
            "attempt-1",
            &lease,
            "succeeded",
            "succeeded_fixture",
            &json!({"ok": true}),
        )
        .unwrap();
    assert_eq!(exact["status"], "succeeded");
    let conflict_receipt = store.complete_managed_acceptance_attempt(
        "attempt-1",
        "wrong-lease",
        "succeeded",
        "succeeded_fixture",
        &json!({"ok": true, "extra": 1}),
    );
    assert!(conflict_receipt
        .unwrap_err()
        .contains("lease_token mismatch"));
    let restarted_terminal = restarted
        .get_managed_acceptance_attempt("attempt-1")
        .unwrap()
        .expect("terminal attempt must survive a separate store restart");
    assert_eq!(restarted_terminal["status"], "succeeded");
    assert_eq!(
        restarted
            .get_managed_acceptance_spend_authorization(&spend_id)
            .unwrap()
            .unwrap()["status"],
        "consumed",
        "terminalization must never reactivate a consumed spend"
    );
}

#[test]
fn spend_issue_reuses_one_active_receipt_for_the_same_logical_attempt() {
    let (_dir, store) = store();
    let principal =
        AuthenticatedPrincipal::fixture_for_tests("tenant-a", "fixture-principal-spend-idempotent")
            .unwrap();
    let residual = "77".repeat(32);
    let decision = store
        .upsert_managed_acceptance_decision(
            "tenant-a",
            &decision_body_for_attempt("mad-spend-idempotent", "same-attempt"),
            &residual,
            "draft_pending_operator",
            None,
            Some("2026-07-26T00:00:00Z"),
        )
        .unwrap();
    let risk = store
        .accept_managed_acceptance_decision(
            &principal,
            &RiskAcknowledgementRequest {
                decision_id: "mad-spend-idempotent".into(),
                expected_decision_body_sha256: decision["decision_body_sha256"]
                    .as_str()
                    .unwrap()
                    .into(),
                expected_residual_finding_sha256: residual,
                submitted_phrase: OPERATOR_RISK_ACCEPTANCE_PHRASE.into(),
                explicit_go: true,
            },
        )
        .unwrap();
    let request = spend_req(risk["authorization_id"].as_str().unwrap(), "same-attempt");

    let first = store
        .issue_managed_acceptance_spend_authorization(&principal, &request)
        .unwrap();
    let second = store
        .issue_managed_acceptance_spend_authorization(&principal, &request)
        .unwrap();

    assert_eq!(first["status"], "active");
    assert_eq!(
        first["spend_authorization_id"], second["spend_authorization_id"],
        "replay must reuse the one active logical spend receipt"
    );
    assert_eq!(
        first["logical_authorization_sha256"],
        stable_spend_authorization_identity(&first["body_json"]).unwrap(),
        "persisted logical spend identity must match its canonical body"
    );
}

#[test]
fn logical_spend_identity_is_stable_before_receipt_id_and_timestamp_generation() {
    let principal =
        AuthenticatedPrincipal::fixture_for_tests("tenant-a", "fixture-principal-logical").unwrap();
    let request = spend_req("maa-logical", "logical-attempt");
    let risk = json!({
        "authorization_sha256": "a1".repeat(32),
        "decision_body_sha256": "b2".repeat(32),
        "residual_finding_sha256": "c3".repeat(32),
        "expires_at": "2026-07-26T00:00:00Z",
    });
    let first = build_spend_body(
        "mas-first-receipt",
        &principal,
        &request,
        &risk,
        "mad-logical",
        true,
        "2026-07-25T12:00:00Z",
        None,
    )
    .unwrap();
    let second = build_spend_body(
        "mas-second-receipt",
        &principal,
        &request,
        &risk,
        "mad-logical",
        true,
        "2026-07-25T12:01:00Z",
        None,
    )
    .unwrap();
    assert_eq!(
        stable_spend_authorization_identity(&first).unwrap(),
        stable_spend_authorization_identity(&second).unwrap(),
        "logical identity must be derived before UUID/timestamp receipt generation"
    );
    let mut different_expiry = second;
    different_expiry["expires_at"] = json!("2026-07-26T01:00:00Z");
    assert_ne!(
        stable_spend_authorization_identity(&first).unwrap(),
        stable_spend_authorization_identity(&different_expiry).unwrap(),
        "expiry is authorization scope and must remain in the logical identity"
    );
}

#[test]
fn attempt_admission_rejects_non_boolean_persisted_spend_fixture_flag() {
    let (_dir, store) = store();
    let principal =
        AuthenticatedPrincipal::fixture_for_tests("tenant-a", "fixture-principal-spend-boolean")
            .unwrap();
    let decision_id = "mad-spend-boolean";
    let residual = "7d".repeat(32);
    let decision = store
        .upsert_managed_acceptance_decision(
            "tenant-a",
            &decision_body_for_attempt(decision_id, "spend-boolean-attempt"),
            &residual,
            "draft_pending_operator",
            None,
            Some("2026-07-26T00:00:00Z"),
        )
        .unwrap();
    let risk = store
        .accept_managed_acceptance_decision(
            &principal,
            &RiskAcknowledgementRequest {
                decision_id: decision_id.into(),
                expected_decision_body_sha256: decision["decision_body_sha256"]
                    .as_str()
                    .unwrap()
                    .into(),
                expected_residual_finding_sha256: residual,
                submitted_phrase: OPERATOR_RISK_ACCEPTANCE_PHRASE.into(),
                explicit_go: true,
            },
        )
        .unwrap();
    let request = spend_req(
        risk["authorization_id"].as_str().unwrap(),
        "spend-boolean-attempt",
    );
    let spend = store
        .issue_managed_acceptance_spend_authorization(&principal, &request)
        .unwrap();
    let spend_id = spend["spend_authorization_id"]
        .as_str()
        .unwrap()
        .to_string();
    store
        .with_conn(|conn| {
            // The production schema already constrains this column. Use
            // SQLite's isolated test-only check bypass to prove the
            // loader remains fail-closed even if storage is corrupted
            // underneath that first line of defense.
            conn.execute_batch("PRAGMA ignore_check_constraints=ON")
                .map_err(|error| error.to_string())?;
            conn.execute(
                "UPDATE managed_acceptance_spend_authorizations
                     SET fixture_only=-1 WHERE spend_authorization_id=?1",
                [&spend_id],
            )
            .map_err(|error| error.to_string())?;
            conn.execute_batch("PRAGMA ignore_check_constraints=OFF")
                .map_err(|error| error.to_string())?;
            Ok(())
        })
        .unwrap();

    let error = store
        .admit_managed_acceptance_attempt_for_test(
            &principal,
            "spend-boolean-attempt",
            &attempt_body_for(&request),
            &spend_id,
            true,
        )
        .expect_err("non-boolean fixture flag must not consume a spend");
    assert!(
        error.contains("spend fixture_only is not a persisted boolean"),
        "{error}"
    );
    let active: i64 = store
        .with_conn(|conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM managed_acceptance_spend_authorizations
                     WHERE spend_authorization_id=?1 AND status='active'",
                [&spend_id],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())
        })
        .unwrap();
    assert_eq!(active, 1, "rejected spend must never be consumed");
}

#[test]
fn attempt_admission_rejects_persisted_spend_principal_kind_tampering() {
    let (_dir, store) = store();
    let principal = AuthenticatedPrincipal::fixture_for_tests(
        "tenant-a",
        "fixture-principal-spend-principal-kind",
    )
    .unwrap();
    let decision_id = "mad-spend-principal-kind";
    let residual = "7f".repeat(32);
    let decision = store
        .upsert_managed_acceptance_decision(
            "tenant-a",
            &decision_body_for_attempt(decision_id, "spend-principal-kind-attempt"),
            &residual,
            "draft_pending_operator",
            None,
            Some("2026-07-26T00:00:00Z"),
        )
        .unwrap();
    let risk = store
        .accept_managed_acceptance_decision(
            &principal,
            &RiskAcknowledgementRequest {
                decision_id: decision_id.into(),
                expected_decision_body_sha256: decision["decision_body_sha256"]
                    .as_str()
                    .unwrap()
                    .into(),
                expected_residual_finding_sha256: residual,
                submitted_phrase: OPERATOR_RISK_ACCEPTANCE_PHRASE.into(),
                explicit_go: true,
            },
        )
        .unwrap();
    let request = spend_req(
        risk["authorization_id"].as_str().unwrap(),
        "spend-principal-kind-attempt",
    );
    let spend = store
        .issue_managed_acceptance_spend_authorization(&principal, &request)
        .unwrap();
    let spend_id = spend["spend_authorization_id"]
        .as_str()
        .unwrap()
        .to_string();
    store
        .with_conn(|conn| {
            conn.execute(
                "UPDATE managed_acceptance_spend_authorizations
                     SET principal_kind='operator_api_key' WHERE spend_authorization_id=?1",
                [&spend_id],
            )
            .map_err(|error| error.to_string())?;
            Ok(())
        })
        .unwrap();

    let error = store
        .admit_managed_acceptance_attempt_for_test(
            &principal,
            "spend-principal-kind-attempt",
            &attempt_body_for(&request),
            &spend_id,
            true,
        )
        .expect_err("principal-kind tampering must not consume a spend");
    assert!(
        error.contains("spend principal kind mismatch")
            || error.contains("body_json principal_kind"),
        "{error}"
    );
    assert!(
        store
            .get_managed_acceptance_spend_authorization(&spend_id)
            .is_err(),
        "tampered spend owner reads must fail closed"
    );
}

#[test]
fn managed_acceptance_owner_json_read_errors_fail_closed() {
    let (_dir, store) = store();
    let principal =
        AuthenticatedPrincipal::fixture_for_tests("tenant-a", "fixture-principal-owner-json")
            .unwrap();
    let decision_id = "mad-owner-json";
    let residual = "7e".repeat(32);
    let decision = store
        .upsert_managed_acceptance_decision(
            "tenant-a",
            &decision_body_for_attempt(decision_id, "owner-json-attempt"),
            &residual,
            "draft_pending_operator",
            None,
            Some("2026-07-26T00:00:00Z"),
        )
        .unwrap();
    let risk = store
        .accept_managed_acceptance_decision(
            &principal,
            &RiskAcknowledgementRequest {
                decision_id: decision_id.into(),
                expected_decision_body_sha256: decision["decision_body_sha256"]
                    .as_str()
                    .unwrap()
                    .into(),
                expected_residual_finding_sha256: residual,
                submitted_phrase: OPERATOR_RISK_ACCEPTANCE_PHRASE.into(),
                explicit_go: true,
            },
        )
        .unwrap();
    let risk_id = risk["authorization_id"].as_str().unwrap().to_string();
    let request = spend_req(&risk_id, "owner-json-attempt");
    let spend = store
        .issue_managed_acceptance_spend_authorization(&principal, &request)
        .unwrap();
    let spend_id = spend["spend_authorization_id"]
        .as_str()
        .unwrap()
        .to_string();
    store
        .admit_managed_acceptance_attempt_for_test(
            &principal,
            "owner-json-attempt",
            &attempt_body_for(&request),
            &spend_id,
            true,
        )
        .unwrap();

    let original_decision: String = store
        .with_conn(|conn| {
            conn.query_row(
                "SELECT body_json FROM managed_acceptance_decisions WHERE decision_id=?1",
                [decision_id],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())
        })
        .unwrap();
    store
        .with_conn(|conn| {
            conn.execute(
                "UPDATE managed_acceptance_decisions SET body_json='not-json' WHERE decision_id=?1",
                [decision_id],
            )
            .map_err(|error| error.to_string())?;
            Ok(())
        })
        .unwrap();
    assert!(store.get_managed_acceptance_decision(decision_id).is_err());
    let mut tampered_decision: Value = serde_json::from_str(&original_decision).unwrap();
    tampered_decision["trial_envelope"]["max_retries"] = json!(99);
    store
        .with_conn(|conn| {
            conn.execute(
                "UPDATE managed_acceptance_decisions SET body_json=?1 WHERE decision_id=?2",
                params![tampered_decision.to_string(), decision_id],
            )
            .map_err(|error| error.to_string())?;
            Ok(())
        })
        .unwrap();
    assert!(
        store.get_managed_acceptance_decision(decision_id).is_err(),
        "a valid but hash-inconsistent decision body must fail closed"
    );
    store
        .with_conn(|conn| {
            conn.execute(
                "UPDATE managed_acceptance_decisions SET body_json=?1 WHERE decision_id=?2",
                params![original_decision, decision_id],
            )
            .map_err(|error| error.to_string())?;
            Ok(())
        })
        .unwrap();

    let original_risk: String = store
        .with_conn(|conn| {
            conn.query_row(
                "SELECT body_json FROM managed_acceptance_authorizations WHERE authorization_id=?1",
                [&risk_id],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())
        })
        .unwrap();
    store
            .with_conn(|conn| {
                conn.execute(
                    "UPDATE managed_acceptance_authorizations SET body_json='not-json' WHERE authorization_id=?1",
                    [&risk_id],
                )
                .map_err(|error| error.to_string())?;
                Ok(())
            })
            .unwrap();
    assert!(store
        .get_active_managed_acceptance_authorization(&risk_id)
        .is_err());
    let mut tampered_risk: Value = serde_json::from_str(&original_risk).unwrap();
    tampered_risk["scope"]["decision_id"] = json!("other-decision");
    store
            .with_conn(|conn| {
                conn.execute(
                    "UPDATE managed_acceptance_authorizations SET body_json=?1 WHERE authorization_id=?2",
                    params![tampered_risk.to_string(), risk_id],
                )
                .map_err(|error| error.to_string())?;
                Ok(())
            })
            .unwrap();
    assert!(
        store
            .get_active_managed_acceptance_authorization(&risk_id)
            .is_err(),
        "a valid but hash-inconsistent risk body must fail closed"
    );
    store
            .with_conn(|conn| {
                conn.execute(
                    "UPDATE managed_acceptance_authorizations SET body_json=?1 WHERE authorization_id=?2",
                    params![original_risk, risk_id],
                )
                .map_err(|error| error.to_string())?;
                Ok(())
            })
            .unwrap();

    let original_spend: String = store
            .with_conn(|conn| {
                conn.query_row(
                    "SELECT body_json FROM managed_acceptance_spend_authorizations WHERE spend_authorization_id=?1",
                    [&spend_id],
                    |row| row.get(0),
                )
                .map_err(|error| error.to_string())
            })
            .unwrap();
    store
            .with_conn(|conn| {
                conn.execute(
                    "UPDATE managed_acceptance_spend_authorizations SET body_json='not-json' WHERE spend_authorization_id=?1",
                    [&spend_id],
                )
                .map_err(|error| error.to_string())?;
                Ok(())
            })
            .unwrap();
    assert!(store
        .get_managed_acceptance_spend_authorization(&spend_id)
        .is_err());
    let mut tampered_spend: Value = serde_json::from_str(&original_spend).unwrap();
    tampered_spend["model"] = json!("different-model");
    store
            .with_conn(|conn| {
                conn.execute(
                    "UPDATE managed_acceptance_spend_authorizations SET body_json=?1 WHERE spend_authorization_id=?2",
                    params![tampered_spend.to_string(), spend_id],
                )
                .map_err(|error| error.to_string())?;
                Ok(())
            })
            .unwrap();
    assert!(
        store
            .get_managed_acceptance_spend_authorization(&spend_id)
            .is_err(),
        "a valid but hash-inconsistent spend body must fail closed"
    );
    store
            .with_conn(|conn| {
                conn.execute(
                    "UPDATE managed_acceptance_spend_authorizations SET body_json=?1 WHERE spend_authorization_id=?2",
                    params![original_spend, spend_id],
                )
                .map_err(|error| error.to_string())?;
                Ok(())
            })
            .unwrap();

    store
            .with_conn(|conn| {
                conn.execute(
                    "UPDATE managed_acceptance_attempts SET receipt_json='not-json' WHERE attempt_id='owner-json-attempt'",
                    [],
                )
                .map_err(|error| error.to_string())?;
                Ok(())
            })
            .unwrap();
    assert!(store
        .get_managed_acceptance_attempt("owner-json-attempt")
        .is_err());
}

#[test]
fn concurrent_spend_issue_reuses_one_active_logical_receipt() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("spend-race.db");
    let store =
        LocalProductStore::new_with_clock(&path, || "2026-07-25T12:00:00Z".to_string()).unwrap();
    let principal =
        AuthenticatedPrincipal::fixture_for_tests("tenant-a", "fixture-principal-spend-race")
            .unwrap();
    let residual = "79".repeat(32);
    let decision = store
        .upsert_managed_acceptance_decision(
            "tenant-a",
            &decision_body_for_attempt("mad-spend-race", "same-logical-attempt"),
            &residual,
            "draft_pending_operator",
            None,
            Some("2026-07-26T00:00:00Z"),
        )
        .unwrap();
    let risk = store
        .accept_managed_acceptance_decision(
            &principal,
            &RiskAcknowledgementRequest {
                decision_id: "mad-spend-race".into(),
                expected_decision_body_sha256: decision["decision_body_sha256"]
                    .as_str()
                    .unwrap()
                    .into(),
                expected_residual_finding_sha256: residual,
                submitted_phrase: OPERATOR_RISK_ACCEPTANCE_PHRASE.into(),
                explicit_go: true,
            },
        )
        .unwrap();
    let risk_id = risk["authorization_id"].as_str().unwrap().to_string();
    let request = spend_req(&risk_id, "same-logical-attempt");
    let barrier = Arc::new(Barrier::new(3));
    let mut joins = Vec::new();
    for _ in 0..2 {
        let path = path.clone();
        let barrier = Arc::clone(&barrier);
        let request = request.clone();
        joins.push(thread::spawn(move || {
            let concurrent =
                LocalProductStore::new_with_clock(&path, || "2026-07-25T12:00:00Z".to_string())
                    .unwrap();
            let principal = AuthenticatedPrincipal::fixture_for_tests(
                "tenant-a",
                "fixture-principal-spend-race",
            )
            .unwrap();
            barrier.wait();
            concurrent.issue_managed_acceptance_spend_authorization(&principal, &request)
        }));
    }
    barrier.wait();
    let spends = joins
        .into_iter()
        .map(|join| {
            join.join()
                .unwrap()
                .expect("concurrent logical spend issuance")
        })
        .collect::<Vec<_>>();
    assert_eq!(
        spends[0]["spend_authorization_id"], spends[1]["spend_authorization_id"],
        "concurrent retries must collapse before UUID/timestamp receipt creation"
    );
    let connection = rusqlite::Connection::open(&path).unwrap();
    let active: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM managed_acceptance_spend_authorizations
                 WHERE tenant_id='tenant-a' AND status='active'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(active, 1, "only one active logical spend may persist");
}

#[test]
fn active_spend_logical_identity_constraints_reject_null_and_duplicate_writes() {
    let (_dir, store) = store();
    let principal = AuthenticatedPrincipal::fixture_for_tests(
        "tenant-a",
        "fixture-principal-logical-constraint",
    )
    .unwrap();
    let residual = "7c".repeat(32);
    let decision = store
        .upsert_managed_acceptance_decision(
            "tenant-a",
            &decision_body_for_attempt("mad-logical-constraint", "logical-attempt"),
            &residual,
            "draft_pending_operator",
            None,
            Some("2026-07-26T00:00:00Z"),
        )
        .unwrap();
    let risk = store
        .accept_managed_acceptance_decision(
            &principal,
            &RiskAcknowledgementRequest {
                decision_id: "mad-logical-constraint".into(),
                expected_decision_body_sha256: decision["decision_body_sha256"]
                    .as_str()
                    .unwrap()
                    .into(),
                expected_residual_finding_sha256: residual,
                submitted_phrase: OPERATOR_RISK_ACCEPTANCE_PHRASE.into(),
                explicit_go: true,
            },
        )
        .unwrap();
    let request = spend_req(
        risk["authorization_id"].as_str().unwrap(),
        "logical-attempt",
    );
    let first = store
        .issue_managed_acceptance_spend_authorization(&principal, &request)
        .unwrap();
    let first_id = first["spend_authorization_id"]
        .as_str()
        .unwrap()
        .to_string();
    let mut second_body = first["body_json"].clone();
    second_body["spend_authorization_id"] = json!("mas-logical-duplicate-second");
    second_body["created_at"] = json!("2026-07-25T12:00:01Z");
    let second_sha = sha256_hex(canonical_json(&second_body).unwrap().as_bytes());
    let null_error = store
        .with_conn(|conn| {
            conn.execute(
                "UPDATE managed_acceptance_spend_authorizations
                     SET logical_authorization_sha256=NULL WHERE spend_authorization_id=?1",
                [&first_id],
            )
            .map_err(|error| error.to_string())?;
            Ok(())
        })
        .expect_err("V33 must reject an active spend with no logical identity");
    assert!(
        null_error.contains("CHECK constraint failed"),
        "unexpected active-null error: {null_error}"
    );
    let duplicate_error = store
        .with_conn(|conn| {
            conn.execute(
                "INSERT INTO managed_acceptance_spend_authorizations (
                        spend_authorization_id, decision_id, risk_authorization_id, tenant_id,
                        principal_kind, principal_id, spend_body_sha256, risk_authorization_sha256,
                        logical_authorization_sha256, decision_body_sha256, residual_finding_sha256,
                        fixture_only, status, body_json, created_at, updated_at, expires_at,
                        consumed_at, consumed_by_attempt_id, revoked_at
                     ) SELECT ?1, decision_id, risk_authorization_id, tenant_id,
                        principal_kind, principal_id, ?2, risk_authorization_sha256,
                        logical_authorization_sha256, decision_body_sha256, residual_finding_sha256,
                        fixture_only, 'active', ?3, created_at, updated_at, expires_at,
                        NULL, NULL, NULL
                     FROM managed_acceptance_spend_authorizations WHERE spend_authorization_id=?4",
                rusqlite::params![
                    "mas-logical-duplicate-second",
                    second_sha,
                    second_body.to_string(),
                    first_id,
                ],
            )
            .map_err(|error| error.to_string())?;
            Ok(())
        })
        .expect_err("V33 must reject a second active spend for the same logical identity");
    assert!(
        duplicate_error.contains("UNIQUE constraint failed"),
        "unexpected active-duplicate error: {duplicate_error}"
    );
    let replay = store
        .issue_managed_acceptance_spend_authorization(&principal, &request)
        .expect("the rejected raw writes must leave the original spend reusable");
    assert_eq!(
        replay["spend_authorization_id"],
        first["spend_authorization_id"]
    );
}

#[test]
fn spend_envelope_requires_exact_attempt_target_binary_and_recovery_bindings() {
    let (_dir, store) = store();
    let principal = AuthenticatedPrincipal::fixture_for_tests(
        "tenant-a",
        "fixture-principal-envelope-bindings",
    )
    .unwrap();
    let residual = "7a".repeat(32);
    let decision = store
        .upsert_managed_acceptance_decision(
            "tenant-a",
            &decision_body_for_attempt("mad-envelope-bindings", "bound-attempt"),
            &residual,
            "draft_pending_operator",
            None,
            Some("2026-07-26T00:00:00Z"),
        )
        .unwrap();
    let risk = store
        .accept_managed_acceptance_decision(
            &principal,
            &RiskAcknowledgementRequest {
                decision_id: "mad-envelope-bindings".into(),
                expected_decision_body_sha256: decision["decision_body_sha256"]
                    .as_str()
                    .unwrap()
                    .into(),
                expected_residual_finding_sha256: residual,
                submitted_phrase: OPERATOR_RISK_ACCEPTANCE_PHRASE.into(),
                explicit_go: true,
            },
        )
        .unwrap();
    let base = spend_req(risk["authorization_id"].as_str().unwrap(), "bound-attempt");
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
            .expect_err(&format!("{field} mutation must not issue spend"));
        assert!(
            error.contains("mismatches decision trial envelope"),
            "{field}: unexpected error {error}"
        );
    }
}

#[test]
fn spend_issuance_rejects_missing_persisted_risk_fixture_boolean() {
    let (_dir, store) = store();
    let principal =
        AuthenticatedPrincipal::fixture_for_tests("tenant-a", "fixture-principal-risk-boolean")
            .unwrap();
    let residual = "7d".repeat(32);
    let decision = store
        .upsert_managed_acceptance_decision(
            "tenant-a",
            &decision_body_for_attempt("mad-risk-boolean", "risk-boolean-attempt"),
            &residual,
            "draft_pending_operator",
            None,
            Some("2026-07-26T00:00:00Z"),
        )
        .unwrap();
    let risk = store
        .accept_managed_acceptance_decision(
            &principal,
            &RiskAcknowledgementRequest {
                decision_id: "mad-risk-boolean".into(),
                expected_decision_body_sha256: decision["decision_body_sha256"]
                    .as_str()
                    .unwrap()
                    .into(),
                expected_residual_finding_sha256: residual,
                submitted_phrase: OPERATOR_RISK_ACCEPTANCE_PHRASE.into(),
                explicit_go: true,
            },
        )
        .unwrap();
    let risk_id = risk["authorization_id"].as_str().unwrap().to_string();
    store
        .with_conn(|conn| {
            let raw: String = conn
                .query_row(
                    "SELECT body_json FROM managed_acceptance_authorizations
                         WHERE authorization_id=?1",
                    [&risk_id],
                    |row| row.get(0),
                )
                .map_err(|error| error.to_string())?;
            let mut body: Value = serde_json::from_str(&raw).map_err(|error| error.to_string())?;
            body.as_object_mut()
                .ok_or("risk authorization body must be an object")?
                .remove("fixture_only");
            conn.execute(
                "UPDATE managed_acceptance_authorizations SET body_json=?1
                     WHERE authorization_id=?2",
                params![body.to_string(), risk_id],
            )
            .map_err(|error| error.to_string())?;
            Ok(())
        })
        .unwrap();
    let error = store
        .issue_managed_acceptance_spend_authorization(
            &principal,
            &spend_req(&risk_id, "risk-boolean-attempt"),
        )
        .expect_err("missing persisted risk boolean must fail closed");
    assert!(
        error.contains("fixture_only must be a persisted boolean")
            || error.contains("body fixture_only must be a boolean")
            || error.contains("body_json missing fixture_only"),
        "{error}"
    );
}

#[test]
fn decision_status_changes_persist_hash_linked_transition_receipts() {
    let (_dir, store) = store();
    let principal =
        AuthenticatedPrincipal::fixture_for_tests("tenant-a", "fixture-principal-transition")
            .unwrap();
    let residual = "88".repeat(32);
    let decision = store
        .upsert_managed_acceptance_decision(
            "tenant-a",
            &decision_body("mad-transition-receipts"),
            &residual,
            "draft_pending_operator",
            None,
            Some("2026-07-26T00:00:00Z"),
        )
        .unwrap();
    let decision_sha = decision["decision_body_sha256"]
        .as_str()
        .unwrap()
        .to_string();
    let authorization = store
        .accept_managed_acceptance_decision(
            &principal,
            &RiskAcknowledgementRequest {
                decision_id: "mad-transition-receipts".into(),
                expected_decision_body_sha256: decision_sha.clone(),
                expected_residual_finding_sha256: residual,
                submitted_phrase: OPERATOR_RISK_ACCEPTANCE_PHRASE.into(),
                explicit_go: true,
            },
        )
        .unwrap();
    store
        .revoke_managed_acceptance_authorization(
            &principal,
            authorization["authorization_id"].as_str().unwrap(),
        )
        .unwrap();

    let receipts = store
        .list_managed_acceptance_decision_transition_receipts("mad-transition-receipts")
        .unwrap();
    assert_eq!(receipts.len(), 2);
    let accepted = receipts
        .iter()
        .find(|receipt| receipt["to_status"] == "operator_accepted")
        .unwrap();
    let revoked = receipts
        .iter()
        .find(|receipt| receipt["to_status"] == "revoked")
        .unwrap();
    assert_eq!(accepted["decision_body_sha256"], decision_sha);
    assert_eq!(accepted["from_status"], "draft_pending_operator");
    assert_eq!(revoked["from_status"], "operator_accepted");
    assert_eq!(
        revoked["previous_transition_sha256"],
        accepted["transition_sha256"]
    );
    assert_eq!(accepted["transition_sha256"].as_str().unwrap().len(), 64);
    assert_eq!(revoked["transition_sha256"].as_str().unwrap().len(), 64);
}

#[test]
fn decision_transition_receipts_order_by_sequence_for_equal_timestamps() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("transition-order.db");
    let store =
        LocalProductStore::new_with_clock(&path, || "2026-07-26T00:00:00Z".to_string()).unwrap();
    let principal =
        AuthenticatedPrincipal::fixture_for_tests("tenant-a", "fixture-principal-transition-order")
            .unwrap();
    let decision_id = "mad-transition-sequence-order-02";
    let residual = "8b".repeat(32);
    let decision = store
        .upsert_managed_acceptance_decision(
            "tenant-a",
            &decision_body(decision_id),
            &residual,
            "draft_pending_operator",
            None,
            Some("2026-07-27T00:00:00Z"),
        )
        .unwrap();
    let authorization = store
        .accept_managed_acceptance_decision(
            &principal,
            &RiskAcknowledgementRequest {
                decision_id: decision_id.to_string(),
                expected_decision_body_sha256: decision["decision_body_sha256"]
                    .as_str()
                    .unwrap()
                    .into(),
                expected_residual_finding_sha256: residual,
                submitted_phrase: OPERATOR_RISK_ACCEPTANCE_PHRASE.into(),
                explicit_go: true,
            },
        )
        .unwrap();
    store
        .revoke_managed_acceptance_authorization(
            &principal,
            authorization["authorization_id"].as_str().unwrap(),
        )
        .unwrap();

    let before = store
        .list_managed_acceptance_decision_transition_receipts(decision_id)
        .unwrap();
    assert_eq!(before.len(), 2);
    assert_eq!(before[0]["sequence"], 1);
    assert_eq!(before[1]["sequence"], 2);
    assert_eq!(before[1]["previous_transition_sequence"], 1);
}

#[test]
fn decision_transition_receipt_content_tampering_fails_closed_on_read() {
    let (_dir, store) = store();
    let principal = AuthenticatedPrincipal::fixture_for_tests(
        "tenant-a",
        "fixture-principal-transition-tamper",
    )
    .unwrap();
    let decision_id = "mad-transition-tamper";
    let residual = "89".repeat(32);
    let decision = store
        .upsert_managed_acceptance_decision(
            "tenant-a",
            &decision_body(decision_id),
            &residual,
            "draft_pending_operator",
            None,
            Some("2026-07-26T00:00:00Z"),
        )
        .unwrap();
    store
        .accept_managed_acceptance_decision(
            &principal,
            &RiskAcknowledgementRequest {
                decision_id: decision_id.into(),
                expected_decision_body_sha256: decision["decision_body_sha256"]
                    .as_str()
                    .unwrap()
                    .into(),
                expected_residual_finding_sha256: residual,
                submitted_phrase: OPERATOR_RISK_ACCEPTANCE_PHRASE.into(),
                explicit_go: true,
            },
        )
        .unwrap();
    store
        .with_conn(|conn| {
            let (receipt_id, encoded): (String, String) = conn
                .query_row(
                    "SELECT transition_receipt_id, receipt_json
                         FROM managed_acceptance_decision_transition_receipts
                         WHERE decision_id=?1",
                    [decision_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .map_err(|error| error.to_string())?;
            let mut receipt: Value =
                serde_json::from_str(&encoded).map_err(|error| error.to_string())?;
            receipt["reason"] = json!("tampered_after_persistence");
            conn.execute(
                "UPDATE managed_acceptance_decision_transition_receipts
                     SET receipt_json=?1 WHERE transition_receipt_id=?2",
                params![receipt.to_string(), receipt_id],
            )
            .map_err(|error| error.to_string())?;
            Ok(())
        })
        .unwrap();

    let error = store
        .list_managed_acceptance_decision_transition_receipts(decision_id)
        .expect_err("tampered transition content must not be readable as evidence");
    assert!(error.contains("hash does not match content"), "{error}");
}

#[test]
fn decision_transition_receipt_owner_constraints_reject_forked_writes() {
    let (_dir, store) = store();
    let principal =
        AuthenticatedPrincipal::fixture_for_tests("tenant-a", "fixture-principal-transition-fork")
            .unwrap();
    let decision_id = "mad-transition-fork";
    let residual = "8a".repeat(32);
    let decision = store
        .upsert_managed_acceptance_decision(
            "tenant-a",
            &decision_body(decision_id),
            &residual,
            "draft_pending_operator",
            None,
            Some("2026-07-26T00:00:00Z"),
        )
        .unwrap();
    let authorization = store
        .accept_managed_acceptance_decision(
            &principal,
            &RiskAcknowledgementRequest {
                decision_id: decision_id.into(),
                expected_decision_body_sha256: decision["decision_body_sha256"]
                    .as_str()
                    .unwrap()
                    .into(),
                expected_residual_finding_sha256: residual.clone(),
                submitted_phrase: OPERATOR_RISK_ACCEPTANCE_PHRASE.into(),
                explicit_go: true,
            },
        )
        .unwrap();
    store
        .revoke_managed_acceptance_authorization(
            &principal,
            authorization["authorization_id"].as_str().unwrap(),
        )
        .unwrap();
    let accepted_sha = store
        .list_managed_acceptance_decision_transition_receipts(decision_id)
        .unwrap()
        .into_iter()
        .find(|receipt| receipt["to_status"] == "operator_accepted")
        .unwrap()["transition_sha256"]
        .as_str()
        .unwrap()
        .to_string();
    let forged = decision_status_transition_receipt(
        decision_id,
        "tenant-a",
        decision["decision_body_sha256"].as_str().unwrap(),
        &residual,
        2,
        Some(1),
        Some(&accepted_sha),
        "operator_accepted",
        "revoked",
        &principal,
        "2026-07-25T12:00:01Z",
        "forged_parallel_revocation",
    )
    .unwrap();
    let child_error = store
        .with_conn(|conn| {
            conn.execute(
                "INSERT INTO managed_acceptance_decision_transition_receipts (
                        transition_receipt_id, decision_id, tenant_id, decision_body_sha256,
                        previous_transition_sha256, transition_sha256, from_status, to_status,
                        actor_principal_kind, actor_principal_id, receipt_json, created_at
                     ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
                params![
                    forged["transition_receipt_id"].as_str().unwrap(),
                    decision_id,
                    "tenant-a",
                    decision["decision_body_sha256"].as_str().unwrap(),
                    forged["previous_transition_sha256"].as_str().unwrap(),
                    forged["transition_sha256"].as_str().unwrap(),
                    forged["from_status"].as_str().unwrap(),
                    forged["to_status"].as_str().unwrap(),
                    forged["actor_principal_kind"].as_str().unwrap(),
                    forged["actor_principal_id"].as_str().unwrap(),
                    forged.to_string(),
                    forged["at"].as_str().unwrap(),
                ],
            )
            .map_err(|error| error.to_string())?;
            Ok(())
        })
        .expect_err("V32 must reject a second child transition from one predecessor");
    assert!(
        child_error.contains("UNIQUE constraint failed"),
        "unexpected child fork error: {child_error}"
    );

    let forged_genesis = decision_status_transition_receipt(
        decision_id,
        "tenant-a",
        decision["decision_body_sha256"].as_str().unwrap(),
        &residual,
        1,
        None,
        None,
        "draft_pending_operator",
        "operator_accepted",
        &principal,
        "2026-07-25T12:00:02Z",
        "forged_second_genesis",
    )
    .unwrap();
    let genesis_error = store
        .with_conn(|conn| {
            conn.execute(
                "INSERT INTO managed_acceptance_decision_transition_receipts (
                        transition_receipt_id, decision_id, tenant_id, decision_body_sha256,
                        previous_transition_sha256, transition_sha256, from_status, to_status,
                        actor_principal_kind, actor_principal_id, receipt_json, created_at
                     ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
                params![
                    forged_genesis["transition_receipt_id"].as_str().unwrap(),
                    decision_id,
                    "tenant-a",
                    decision["decision_body_sha256"].as_str().unwrap(),
                    Option::<String>::None,
                    forged_genesis["transition_sha256"].as_str().unwrap(),
                    forged_genesis["from_status"].as_str().unwrap(),
                    forged_genesis["to_status"].as_str().unwrap(),
                    forged_genesis["actor_principal_kind"].as_str().unwrap(),
                    forged_genesis["actor_principal_id"].as_str().unwrap(),
                    forged_genesis.to_string(),
                    forged_genesis["at"].as_str().unwrap(),
                ],
            )
            .map_err(|error| error.to_string())?;
            Ok(())
        })
        .expect_err("V32 must reject a second genesis transition");
    assert!(
        genesis_error.contains("UNIQUE constraint failed"),
        "unexpected genesis fork error: {genesis_error}"
    );

    let receipts = store
        .list_managed_acceptance_decision_transition_receipts(decision_id)
        .expect("rejected writes must leave the persisted transition chain valid");
    assert_eq!(receipts.len(), 2);
}

#[test]
fn concurrent_admission_single_winner() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("race.db");
    let store =
        LocalProductStore::new_with_clock(&path, || "2026-07-25T12:00:00Z".to_string()).unwrap();
    let principal =
        AuthenticatedPrincipal::fixture_for_tests("tenant-a", "fixture-principal-race").unwrap();
    let residual = "11".repeat(32);
    let body = decision_body_for_attempt("mad-race", "race-1");
    let decision = store
        .upsert_managed_acceptance_decision(
            "tenant-a",
            &body,
            &residual,
            "draft_pending_operator",
            None,
            Some("2026-07-26T00:00:00Z"),
        )
        .unwrap();
    let dsha = decision["decision_body_sha256"]
        .as_str()
        .unwrap()
        .to_string();
    let auth = store
        .accept_managed_acceptance_decision(
            &principal,
            &RiskAcknowledgementRequest {
                decision_id: "mad-race".into(),
                expected_decision_body_sha256: dsha,
                expected_residual_finding_sha256: residual,
                submitted_phrase: OPERATOR_RISK_ACCEPTANCE_PHRASE.into(),
                explicit_go: true,
            },
        )
        .unwrap();
    let auth_id = auth["authorization_id"].as_str().unwrap().to_string();
    let spend_request = spend_req(&auth_id, "race-1");
    let spend = store
        .issue_managed_acceptance_spend_authorization(&principal, &spend_request)
        .unwrap();
    let spend_id = spend["spend_authorization_id"]
        .as_str()
        .unwrap()
        .to_string();
    let attempt_body = attempt_body_for(&spend_request);
    let barrier = Arc::new(Barrier::new(2));
    let path2 = path.clone();
    let spend_a = spend_id.clone();
    let spend_b = spend_id.clone();
    let body_a = attempt_body.clone();
    let body_b = attempt_body.clone();
    let b1 = Arc::clone(&barrier);
    let b2 = Arc::clone(&barrier);
    let h1 = thread::spawn(move || {
        let s = LocalProductStore::new_with_clock(&path2, || "2026-07-25T12:00:00Z".to_string())
            .unwrap();
        let p = AuthenticatedPrincipal::fixture_for_tests("tenant-a", "fixture-principal-race")
            .unwrap();
        b1.wait();
        s.admit_managed_acceptance_attempt_for_test(&p, "race-1", &body_a, &spend_a, true)
    });
    let path3 = path.clone();
    let h2 = thread::spawn(move || {
        let s = LocalProductStore::new_with_clock(&path3, || "2026-07-25T12:00:00Z".to_string())
            .unwrap();
        let p = AuthenticatedPrincipal::fixture_for_tests("tenant-a", "fixture-principal-race")
            .unwrap();
        b2.wait();
        s.admit_managed_acceptance_attempt_for_test(&p, "race-1", &body_b, &spend_b, true)
    });
    let r1 = h1.join().unwrap();
    let r2 = h2.join().unwrap();
    let ok = [r1.is_ok(), r2.is_ok()].iter().filter(|x| **x).count();
    // One admits, the other may idempotent-replay or fail on spend consumed.
    assert!(ok >= 1, "at least one admission must succeed");
    let final_spend = store
        .get_managed_acceptance_spend_authorization(&spend_id)
        .unwrap()
        .unwrap();
    assert_eq!(final_spend["status"], "consumed");
    let attempt = store
        .get_managed_acceptance_attempt("race-1")
        .unwrap()
        .unwrap();
    assert_eq!(attempt["status"], "admitted");
}

#[test]
fn revocation_blocks_spend() {
    let (_dir, store) = store();
    let principal =
        AuthenticatedPrincipal::fixture_for_tests("tenant-a", "fixture-principal-bob").unwrap();
    let residual = "33".repeat(32);
    let body = decision_body("mad-rev");
    let decision = store
        .upsert_managed_acceptance_decision(
            "tenant-a",
            &body,
            &residual,
            "draft_pending_operator",
            None,
            Some("2026-07-26T00:00:00Z"),
        )
        .unwrap();
    let dsha = decision["decision_body_sha256"]
        .as_str()
        .unwrap()
        .to_string();
    let auth = store
        .accept_managed_acceptance_decision(
            &principal,
            &RiskAcknowledgementRequest {
                decision_id: "mad-rev".into(),
                expected_decision_body_sha256: dsha,
                expected_residual_finding_sha256: residual,
                submitted_phrase: OPERATOR_RISK_ACCEPTANCE_PHRASE.into(),
                explicit_go: true,
            },
        )
        .unwrap();
    let auth_id = auth["authorization_id"].as_str().unwrap().to_string();
    store
        .revoke_managed_acceptance_authorization(&principal, &auth_id)
        .unwrap();
    let err = store
        .issue_managed_acceptance_spend_authorization(&principal, &spend_req(&auth_id, "x"))
        .unwrap_err();
    assert!(err.contains("not active") || err.contains("revok"), "{err}");
}

#[test]
fn finite_expiry_mandatory_and_bound_into_decision_hash() {
    let (_dir, store) = store();
    let body = decision_body("mad-exp");
    let residual = "44".repeat(32);
    let err = store
        .upsert_managed_acceptance_decision(
            "tenant-a",
            &body,
            &residual,
            "draft_pending_operator",
            None,
            None,
        )
        .unwrap_err();
    assert!(err.contains("finite expires_at"), "{err}");
    let err_far = store
        .upsert_managed_acceptance_decision(
            "tenant-a",
            &body,
            &residual,
            "draft_pending_operator",
            None,
            Some("2099-01-01T00:00:00Z"),
        )
        .unwrap_err();
    assert!(
        err_far.contains("far-future") || err_far.contains("finite"),
        "{err_far}"
    );
    let d1 = store
        .upsert_managed_acceptance_decision(
            "tenant-a",
            &body,
            &residual,
            "draft_pending_operator",
            None,
            Some("2026-07-26T00:00:00Z"),
        )
        .unwrap();
    let d2 = store
        .upsert_managed_acceptance_decision(
            "tenant-a",
            &decision_body("mad-exp-2"),
            &residual,
            "draft_pending_operator",
            None,
            Some("2026-07-27T00:00:00Z"),
        )
        .unwrap();
    assert_ne!(
        d1["decision_body_sha256"], d2["decision_body_sha256"],
        "different expiry must change canonical authority hash"
    );
}

#[test]
fn spend_budget_mismatch_against_decision_rejects() {
    let (_dir, store) = store();
    let principal =
        AuthenticatedPrincipal::fixture_for_tests("tenant-a", "fixture-principal-budget").unwrap();
    let residual = "55".repeat(32);
    let decision = store
        .upsert_managed_acceptance_decision(
            "tenant-a",
            &decision_body_for_attempt("mad-budget", "bad-budget"),
            &residual,
            "draft_pending_operator",
            None,
            Some("2026-07-26T00:00:00Z"),
        )
        .unwrap();
    let dsha = decision["decision_body_sha256"]
        .as_str()
        .unwrap()
        .to_string();
    let auth = store
        .accept_managed_acceptance_decision(
            &principal,
            &RiskAcknowledgementRequest {
                decision_id: "mad-budget".into(),
                expected_decision_body_sha256: dsha,
                expected_residual_finding_sha256: residual,
                submitted_phrase: OPERATOR_RISK_ACCEPTANCE_PHRASE.into(),
                explicit_go: true,
            },
        )
        .unwrap();
    let auth_id = auth["authorization_id"].as_str().unwrap().to_string();
    let mut bad = spend_req(&auth_id, "bad-budget");
    bad.max_provider_requests = 99;
    let err = store
        .issue_managed_acceptance_spend_authorization(&principal, &bad)
        .unwrap_err();
    assert!(err.contains("max_provider_requests"), "{err}");
    bad = spend_req(&auth_id, "bad-budget");
    bad.model = "mutated-model".into();
    let err = store
        .issue_managed_acceptance_spend_authorization(&principal, &bad)
        .unwrap_err();
    assert!(err.contains("model"), "{err}");
}

#[test]
fn attempt_body_mismatch_rejects_before_spend_consumption() {
    let (_dir, store) = store();
    let principal =
        AuthenticatedPrincipal::fixture_for_tests("tenant-a", "fixture-principal-mismatch")
            .unwrap();
    let residual = "66".repeat(32);
    let decision = store
        .upsert_managed_acceptance_decision(
            "tenant-a",
            &decision_body_for_attempt("mad-mismatch", "mismatch-1"),
            &residual,
            "draft_pending_operator",
            None,
            Some("2026-07-26T00:00:00Z"),
        )
        .unwrap();
    let dsha = decision["decision_body_sha256"]
        .as_str()
        .unwrap()
        .to_string();
    let auth = store
        .accept_managed_acceptance_decision(
            &principal,
            &RiskAcknowledgementRequest {
                decision_id: "mad-mismatch".into(),
                expected_decision_body_sha256: dsha,
                expected_residual_finding_sha256: residual,
                submitted_phrase: OPERATOR_RISK_ACCEPTANCE_PHRASE.into(),
                explicit_go: true,
            },
        )
        .unwrap();
    let auth_id = auth["authorization_id"].as_str().unwrap().to_string();
    let req = spend_req(&auth_id, "mismatch-1");
    let spend = store
        .issue_managed_acceptance_spend_authorization(&principal, &req)
        .unwrap();
    let spend_id = spend["spend_authorization_id"]
        .as_str()
        .unwrap()
        .to_string();
    let mut body = attempt_body_for(&req);
    body["model"] = json!("wrong-model");
    let err = store
        .admit_managed_acceptance_attempt_for_test(&principal, "mismatch-1", &body, &spend_id, true)
        .unwrap_err();
    assert!(err.contains("mismatch") || err.contains("model"), "{err}");
    // Spend must remain active (not consumed on mismatch).
    let spend_row = store
        .get_managed_acceptance_spend_authorization(&spend_id)
        .unwrap()
        .unwrap();
    assert_eq!(spend_row["status"], "active");
}

#[test]
fn decision_body_hash_stable_across_accept_status_transition() {
    let (_dir, store) = store();
    let principal =
        AuthenticatedPrincipal::fixture_for_tests("tenant-a", "fixture-principal-stable").unwrap();
    let residual = "88".repeat(32);
    let decision = store
        .upsert_managed_acceptance_decision(
            "tenant-a",
            &decision_body("mad-stable"),
            &residual,
            "draft_pending_operator",
            None,
            Some("2026-07-26T00:00:00Z"),
        )
        .unwrap();
    let dsha = decision["decision_body_sha256"]
        .as_str()
        .unwrap()
        .to_string();
    store
        .accept_managed_acceptance_decision(
            &principal,
            &RiskAcknowledgementRequest {
                decision_id: "mad-stable".into(),
                expected_decision_body_sha256: dsha.clone(),
                expected_residual_finding_sha256: residual,
                submitted_phrase: OPERATOR_RISK_ACCEPTANCE_PHRASE.into(),
                explicit_go: true,
            },
        )
        .unwrap();
    let after = store
        .get_managed_acceptance_decision("mad-stable")
        .unwrap()
        .unwrap();
    assert_eq!(after["status"], "operator_accepted");
    assert_eq!(
        after["decision_body_sha256"].as_str().unwrap(),
        dsha.as_str(),
        "mutable status must not rewrite the immutable decision authority hash"
    );
}

#[test]
fn rfc3339_offset_expiry_compared_by_instant_not_lexically() {
    // "2026-07-25T12:00:00+02:00" is 10:00Z — before store now 12:00Z — must be expired.
    let dir = tempdir().unwrap();
    let path = dir.path().join("offset.db");
    let store =
        LocalProductStore::new_with_clock(&path, || "2026-07-25T12:00:00Z".to_string()).unwrap();
    let principal =
        AuthenticatedPrincipal::fixture_for_tests("tenant-a", "fixture-principal-offset").unwrap();
    let residual = "99".repeat(32);
    // Create with far enough expiry first.
    let decision = store.upsert_managed_acceptance_decision(
        "tenant-a",
        &decision_body("mad-offset"),
        &residual,
        "draft_pending_operator",
        None,
        Some("2026-07-25T14:00:00+02:00"), // 12:00Z == now boundary after normalize?
    );
    // 14:00+02:00 == 12:00Z; require strictly after now → should fail.
    assert!(decision.is_err(), "expiry at exact now must fail");
    let decision = store
        .upsert_managed_acceptance_decision(
            "tenant-a",
            &decision_body("mad-offset"),
            &residual,
            "draft_pending_operator",
            None,
            Some("2026-07-25T15:00:00+02:00"), // 13:00Z > 12:00Z
        )
        .unwrap();
    let dsha = decision["decision_body_sha256"]
        .as_str()
        .unwrap()
        .to_string();
    // Advance to after expiry (15:00+02 = 13:00Z; set now to 14:00Z).
    let store_late =
        LocalProductStore::new_with_clock(&path, || "2026-07-25T14:00:00Z".to_string()).unwrap();
    let err = store_late
        .accept_managed_acceptance_decision(
            &principal,
            &RiskAcknowledgementRequest {
                decision_id: "mad-offset".into(),
                expected_decision_body_sha256: dsha,
                expected_residual_finding_sha256: residual,
                submitted_phrase: OPERATOR_RISK_ACCEPTANCE_PHRASE.into(),
                explicit_go: true,
            },
        )
        .unwrap_err();
    assert!(err.contains("expir"), "{err}");
}

#[test]
fn expired_decision_rejects_accept_and_spend() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("exp.db");
    // Create decision while clock is early.
    let store_early =
        LocalProductStore::new_with_clock(&path, || "2026-07-25T12:00:00Z".to_string()).unwrap();
    let principal =
        AuthenticatedPrincipal::fixture_for_tests("tenant-a", "fixture-principal-exp").unwrap();
    let residual = "77".repeat(32);
    let decision = store_early
        .upsert_managed_acceptance_decision(
            "tenant-a",
            &decision_body("mad-time"),
            &residual,
            "draft_pending_operator",
            None,
            Some("2026-07-25T13:00:00Z"),
        )
        .unwrap();
    let dsha = decision["decision_body_sha256"]
        .as_str()
        .unwrap()
        .to_string();
    // Advance clock past expiry.
    let store_late =
        LocalProductStore::new_with_clock(&path, || "2026-07-25T14:00:00Z".to_string()).unwrap();
    let err = store_late
        .accept_managed_acceptance_decision(
            &principal,
            &RiskAcknowledgementRequest {
                decision_id: "mad-time".into(),
                expected_decision_body_sha256: dsha,
                expected_residual_finding_sha256: residual,
                submitted_phrase: OPERATOR_RISK_ACCEPTANCE_PHRASE.into(),
                explicit_go: true,
            },
        )
        .unwrap_err();
    assert!(err.contains("expir"), "{err}");
}

fn delegated_contract() -> DelegationContract {
    DelegationContract {
        schema_version: DELEGATION_SCHEMA_VERSION.into(),
        delegation_id: "delegation-golden-path-1".into(),
        created_at: "2026-07-25T12:00:00Z".into(),
        expires_at: "2026-07-26T12:00:00Z".into(),
        executions: 1,
        repositories: vec!["Igzela/alters-lab".into()],
        task_classes: vec!["documentation".into()],
        allowed_paths: vec!["docs/USER_GUIDE.md".into()],
        max_changed_files: 1,
        max_changed_lines: 100,
        max_cost_usd_per_run: 0.50,
        max_total_cost_usd: 0.50,
        protocol: "openai_compatible".into(),
        models: json!({
            "planner": "deepseek-v4-pro",
            "implementer": "deepseek-v4-flash",
            "reviewer": "deepseek-v4-pro"
        }),
        output: json!({
            "draft_pr_only": true,
            "target_main_write": false,
            "merge": false,
            "auto_merge": false
        }),
        forbidden: vec![
            "credential changes".into(),
            "authentication or permission changes".into(),
            "schema or database migrations".into(),
            "dependency changes".into(),
            "executable or workflow changes".into(),
            "destructive operations".into(),
            "release".into(),
            "deployment".into(),
        ],
    }
}

fn delegated_proposal() -> Value {
    let mut proposal = json!({
        "schema_version": "managed_proposal_manifest.v1",
        "target_repository": "Igzela/alters-lab",
        "target_main_sha": "6".repeat(40),
        "mutable_paths": ["docs/USER_GUIDE.md"],
        "max_cost_usd": null,
        "verifier": "deterministic_docs_health_check_v1",
        "provider_execution_binding": crate::rwe::campaign_package::canonical_deepseek_provider_binding().to_json()
    });
    proposal["manifest_sha256"] = json!(compute_attempt_manifest_sha256(&proposal).unwrap());
    proposal
}

fn delegated_execution() -> Value {
    json!({
        "observed_at": "2026-07-25T12:00:00Z",
        "target_repository": "Igzela/alters-lab",
        "target_main_sha": "6".repeat(40),
        "tenant_id": "tenant-a",
        "product_task_id": "product-task-golden-path-1",
        "workflow_id": "workflow-golden-path-1",
        "workflow_node_ids": ["planning", "implementation", "verification", "review"],
        "attempt_id": "attempt-golden-path-1",
        "verifier": "deterministic_docs_health_check_v1",
        "mutable_paths": ["docs/USER_GUIDE.md"],
        "cancellation_identity": "cancel-golden-path-1",
        "rollback_identity": "rollback-golden-path-1"
    })
}

fn delegated_test_principal(principal_id: &str) -> AuthenticatedPrincipal {
    AuthenticatedPrincipal {
        tenant_id: "tenant-a".into(),
        principal_id: principal_id.into(),
        principal_kind: PrincipalKind::OperatorApiKey,
        scopes: ALL_MANAGED_ACCEPTANCE_SCOPES
            .iter()
            .map(|scope| (*scope).to_string())
            .collect(),
        user_id: format!("user-{principal_id}"),
        role: "owner".into(),
    }
}

fn effect_envelope_for(
    decision_id: &str,
    envelope_id: &str,
    max_total_cost_usd: f64,
    max_child_authorizations: u64,
    expires_at: &str,
) -> EffectEnvelopeContract {
    EffectEnvelopeContract {
        schema_version: EFFECT_ENVELOPE_SCHEMA_VERSION.into(),
        decision_id: decision_id.into(),
        envelope_id: envelope_id.into(),
        owner_principal_id: "effect-owner".into(),
        effect_kind: "repository_maintenance".into(),
        target_repository: "Igzela/token-efficient-agent-harness-lab".into(),
        target_main_sha: "a".repeat(40),
        max_total_cost_usd,
        max_child_authorizations,
        created_at: "2026-07-25T12:00:00Z".into(),
        expires_at: expires_at.into(),
    }
}

fn effect_child_for(
    child_authorization_id: &str,
    operation_id: &str,
    max_cost_usd: f64,
    expires_at: &str,
) -> EffectChildAuthorizationRequest {
    EffectChildAuthorizationRequest {
        child_authorization_id: child_authorization_id.into(),
        operation_id: operation_id.into(),
        effect_kind: "repository_maintenance".into(),
        target_repository: "Igzela/token-efficient-agent-harness-lab".into(),
        target_main_sha: "a".repeat(40),
        max_cost_usd,
        expires_at: expires_at.into(),
    }
}

fn effect_evidence(child_authorization_id: &str, effect_executed: bool) -> Value {
    let payload = json!({
        "kind": "provider_free_canary",
        "child_authorization_id": child_authorization_id,
        "effect_executed": effect_executed,
    });
    let evidence_sha256 = sha256_hex(canonical_json(&payload).unwrap().as_bytes());
    json!({
        "evidence": payload,
        "evidence_sha256": evidence_sha256,
    })
}

#[test]
fn effect_envelope_validation_is_immutable_and_bounded() {
    let mut envelope = effect_envelope_for(
        "effect-validation-decision",
        "effect-validation-envelope",
        0.75,
        2,
        "2026-07-26T12:00:00Z",
    );
    envelope.validate("2026-07-25T12:00:00Z").unwrap();

    envelope.target_main_sha = "not-a-commit".into();
    assert!(envelope.validate("2026-07-25T12:00:00Z").is_err());
    envelope.target_main_sha = "a".repeat(40);
    envelope.expires_at = "2026-07-25T11:59:59Z".into();
    assert!(envelope.validate("2026-07-25T12:00:00Z").is_err());
    envelope.expires_at = "2026-07-26T12:00:00Z".into();
    envelope.max_total_cost_usd = f64::NAN;
    assert!(envelope.validate("2026-07-25T12:00:00Z").is_err());
}

#[test]
fn effect_parent_children_are_one_use_bounded_revocable_and_unknown_is_not_retryable() {
    let (dir, store) = store();
    let principal = delegated_test_principal("effect-owner");
    let residual = "b".repeat(64);
    let envelope = effect_envelope_for(
        "effect-parent-decision",
        "effect-parent-envelope",
        0.75,
        2,
        "2026-07-26T12:00:00Z",
    );
    let approved = store
        .approve_effect_envelope(&principal, &envelope, &residual)
        .unwrap();
    let parent_authorization_id = approved["authorization_id"].as_str().unwrap().to_string();

    let child_one = effect_child_for(
        "effect-child-one",
        "effect-operation-one",
        0.50,
        "2026-07-26T12:00:00Z",
    );
    let other_principal = delegated_test_principal("other-effect-owner");
    assert!(store
        .derive_effect_child_authorization(&other_principal, &parent_authorization_id, &child_one,)
        .is_err());
    let first = store
        .derive_effect_child_authorization(&principal, &parent_authorization_id, &child_one)
        .unwrap();
    assert_eq!(first["status"], "active");
    assert_eq!(first["one_use"], true);
    let replay = store
        .derive_effect_child_authorization(&principal, &parent_authorization_id, &child_one)
        .unwrap();
    assert_eq!(replay["replayed"], true);

    let mut mismatched_target = child_one.clone();
    mismatched_target.child_authorization_id = "effect-child-target-mismatch".into();
    mismatched_target.target_repository = "another/repository".into();
    assert!(
        store
            .derive_effect_child_authorization(
                &principal,
                &parent_authorization_id,
                &mismatched_target,
            )
            .is_err()
    );
    let mut late_child = child_one.clone();
    late_child.child_authorization_id = "effect-child-late".into();
    late_child.operation_id = "effect-operation-late".into();
    late_child.expires_at = "2026-07-27T00:00:00Z".into();
    assert!(store
        .derive_effect_child_authorization(&principal, &parent_authorization_id, &late_child)
        .is_err());

    let child_two = effect_child_for(
        "effect-child-two",
        "effect-operation-two",
        0.25,
        "2026-07-26T12:00:00Z",
    );
    store
        .derive_effect_child_authorization(&principal, &parent_authorization_id, &child_two)
        .unwrap();
    let child_three = effect_child_for(
        "effect-child-three",
        "effect-operation-three",
        0.01,
        "2026-07-26T12:00:00Z",
    );
    assert!(store
        .derive_effect_child_authorization(&principal, &parent_authorization_id, &child_three,)
        .is_err());

    let revoked = store
        .revoke_managed_acceptance_authorization(&principal, &parent_authorization_id)
        .unwrap();
    assert_eq!(revoked["status"], "revoked");
    assert_eq!(
        store
            .get_managed_acceptance_spend_authorization("effect-child-one")
            .unwrap()
            .unwrap()["status"],
        "revoked"
    );
    assert!(store
        .derive_effect_child_authorization(&principal, &parent_authorization_id, &child_three)
        .is_err());

    let budget_envelope = effect_envelope_for(
        "effect-budget-decision",
        "effect-budget-envelope",
        0.60,
        3,
        "2026-07-26T12:00:00Z",
    );
    let budget_approved = store
        .approve_effect_envelope(&principal, &budget_envelope, &residual)
        .unwrap();
    let budget_parent = budget_approved["authorization_id"].as_str().unwrap();
    store
        .derive_effect_child_authorization(
            &principal,
            budget_parent,
            &effect_child_for(
                "effect-budget-child-one",
                "effect-budget-operation-one",
                0.50,
                "2026-07-26T12:00:00Z",
            ),
        )
        .unwrap();
    assert!(store
        .derive_effect_child_authorization(
            &principal,
            budget_parent,
            &effect_child_for(
                "effect-budget-child-two",
                "effect-budget-operation-two",
                0.20,
                "2026-07-26T12:00:00Z",
            ),
        )
        .is_err());

    let unknown_envelope = effect_envelope_for(
        "effect-unknown-decision",
        "effect-unknown-envelope",
        0.10,
        2,
        "2026-07-26T12:00:00Z",
    );
    let unknown_approved = store
        .approve_effect_envelope(&principal, &unknown_envelope, &residual)
        .unwrap();
    let unknown_parent = unknown_approved["authorization_id"].as_str().unwrap();
    let unknown_child = effect_child_for(
        "effect-unknown-child",
        "effect-unknown-operation",
        0.10,
        "2026-07-26T12:00:00Z",
    );
    store
        .derive_effect_child_authorization(&principal, unknown_parent, &unknown_child)
        .unwrap();
    let evidence = effect_evidence("effect-unknown-child", false);
    let settled = store
        .settle_effect_child_authorization(
            &principal,
            "effect-unknown-child",
            "effect-unknown-attempt",
            EFFECT_OUTCOME_UNKNOWN,
            &evidence,
        )
        .unwrap();
    assert_eq!(settled["status"], EFFECT_OUTCOME_UNKNOWN);
    assert_eq!(settled["terminal_class"], "effect_outcome_unknown");
    assert_eq!(settled["receipt_json"]["outcome_unknown_no_retry"], true);
    assert!(store
        .derive_effect_child_authorization(
            &principal,
            unknown_parent,
            &effect_child_for(
                "effect-unknown-child-two",
                "effect-unknown-operation-two",
                0.01,
                "2026-07-26T12:00:00Z",
            ),
        )
        .is_err());
    let settlement_replay = store
        .settle_effect_child_authorization(
            &principal,
            "effect-unknown-child",
            "effect-unknown-attempt",
            EFFECT_OUTCOME_UNKNOWN,
            &evidence,
        )
        .unwrap();
    assert_eq!(settlement_replay["idempotent_replay"], true);
    let conflicting_evidence = effect_evidence("effect-unknown-child", true);
    assert!(store
        .settle_effect_child_authorization(
            &principal,
            "effect-unknown-child",
            "effect-unknown-attempt",
            EFFECT_OUTCOME_UNKNOWN,
            &conflicting_evidence,
        )
        .is_err());

    let expiry_envelope = effect_envelope_for(
        "effect-expiry-decision",
        "effect-expiry-envelope",
        0.10,
        1,
        "2026-07-26T12:00:00Z",
    );
    assert!(expiry_envelope.validate("2026-07-26T12:00:01Z").is_err());
    drop(dir);
}

#[test]
fn effect_outcome_evidence_is_strict_bounded_and_digest_bound() {
    let valid = effect_evidence("effect-evidence-child", false);
    assert!(validate_effect_outcome_evidence(&valid).is_ok());

    let nested = json!({
        "evidence": {
            "kind": "provider_free_canary",
            "child_authorization_id": "effect-evidence-child",
            "effect_executed": false,
            "metadata": {"output": "must not persist"}
        },
        "evidence_sha256": "a".repeat(64)
    });
    assert!(validate_effect_outcome_evidence(&nested).is_err());

    let oversized = json!({
        "evidence": {
            "kind": "provider_free_canary",
            "child_authorization_id": "x".repeat(257),
            "effect_executed": false
        },
        "evidence_sha256": "b".repeat(64)
    });
    assert!(validate_effect_outcome_evidence(&oversized).is_err());

    let sensitive_child_id = ["api_", "key=secret-value"].concat();
    let sensitive = json!({
        "evidence": {
            "kind": "provider_free_canary",
            "child_authorization_id": sensitive_child_id,
            "effect_executed": false
        },
        "evidence_sha256": "c".repeat(64)
    });
    assert!(validate_effect_outcome_evidence(&sensitive).is_err());
}

#[test]
fn effect_child_settlement_retains_terminal_evidence_after_expiry() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("effect-expiry.db");
    let early =
        LocalProductStore::new_with_clock(&path, || "2026-07-25T12:00:00Z".to_string()).unwrap();
    let principal = delegated_test_principal("effect-owner");
    let envelope = effect_envelope_for(
        "effect-expiry-settlement-decision",
        "effect-expiry-settlement-envelope",
        0.10,
        1,
        "2026-07-25T13:00:00Z",
    );
    let residual = "d".repeat(64);
    let approved = early
        .approve_effect_envelope(&principal, &envelope, &residual)
        .unwrap();
    let parent_id = approved["authorization_id"].as_str().unwrap();
    let child = effect_child_for(
        "effect-expiry-settlement-child",
        "effect-expiry-settlement-operation",
        0.10,
        "2026-07-25T13:00:00Z",
    );
    early
        .derive_effect_child_authorization(&principal, parent_id, &child)
        .unwrap();
    drop(early);

    let late =
        LocalProductStore::new_with_clock(&path, || "2026-07-25T14:00:00Z".to_string()).unwrap();
    assert!(late
        .derive_effect_child_authorization(
            &principal,
            parent_id,
            &effect_child_for(
                "effect-expiry-settlement-new-child",
                "effect-expiry-settlement-new-operation",
                0.01,
                "2026-07-25T14:30:00Z",
            ),
        )
        .is_err());
    let settled = late
        .settle_effect_child_authorization(
            &principal,
            "effect-expiry-settlement-child",
            "effect-expiry-settlement-attempt",
            EFFECT_OUTCOME_UNKNOWN,
            &effect_evidence("effect-expiry-settlement-child", false),
        )
        .unwrap();
    assert_eq!(settled["status"], EFFECT_OUTCOME_UNKNOWN);
    assert_eq!(settled["terminal_class"], "effect_outcome_unknown");
}

#[cfg(feature = "pg-tests")]
#[test]
fn pg_effect_parent_child_and_unknown_outcome_have_sqlite_parity() {
    let Ok(database_url) = std::env::var("ACP_TEST_DATABASE_URL") else {
        eprintln!("ACP_TEST_DATABASE_URL not set; skipping effect PostgreSQL parity test");
        return;
    };
    let suffix = format!(
        "{}-{}",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    );
    let store =
        LocalProductStore::new_postgres(&database_url, || "2026-07-25T12:00:00Z".to_string())
            .unwrap();
    let principal = delegated_test_principal("effect-owner");
    let envelope = effect_envelope_for(
        &format!("pg-effect-decision-{suffix}"),
        &format!("pg-effect-envelope-{suffix}"),
        0.20,
        2,
        "2026-07-26T12:00:00Z",
    );
    let residual = "e".repeat(64);
    let approved = store
        .approve_effect_envelope(&principal, &envelope, &residual)
        .unwrap();
    let parent_authorization_id = approved["authorization_id"].as_str().unwrap();
    let child_id = format!("pg-effect-child-{suffix}");
    store
        .derive_effect_child_authorization(
            &principal,
            parent_authorization_id,
            &effect_child_for(
                &child_id,
                &format!("pg-effect-operation-{suffix}"),
                0.20,
                "2026-07-26T12:00:00Z",
            ),
        )
        .unwrap();
    let attempt_id = format!("pg-effect-attempt-{suffix}");
    let outcome = store
        .settle_effect_child_authorization(
            &principal,
            &child_id,
            &attempt_id,
            EFFECT_OUTCOME_UNKNOWN,
            &effect_evidence(&child_id, false),
        )
        .unwrap();
    assert_eq!(outcome["status"], EFFECT_OUTCOME_UNKNOWN);
    assert!(store
        .derive_effect_child_authorization(
            &principal,
            parent_authorization_id,
            &effect_child_for(
                &format!("pg-effect-child-two-{suffix}"),
                &format!("pg-effect-operation-two-{suffix}"),
                0.01,
                "2026-07-26T12:00:00Z",
            ),
        )
        .is_err());
}

#[test]
fn managed_workspace_descriptor_traversal_rejects_symlinks_and_directory_swap_races() {
    use std::os::unix::fs::symlink;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    let dir = tempdir().unwrap();
    let workspace = dir.path().join("workspace");
    let outside = dir.path().join("outside");
    std::fs::create_dir_all(workspace.join("docs")).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(workspace.join("docs/USER_GUIDE.md"), "safe").unwrap();
    std::fs::write(outside.join("USER_GUIDE.md"), "outside").unwrap();

    let final_link = workspace.join("docs/FINAL_LINK.md");
    symlink(outside.join("USER_GUIDE.md"), &final_link).unwrap();
    assert!(open_absolute_path_without_symlinks(&final_link, libc::O_RDONLY).is_err());
    std::fs::remove_file(final_link).unwrap();

    let running = Arc::new(AtomicBool::new(true));
    let running_swapper = running.clone();
    let workspace_swapper = workspace.clone();
    let outside_swapper = outside.clone();
    let swapper = std::thread::spawn(move || {
        let docs = workspace_swapper.join("docs");
        let real = workspace_swapper.join("docs.real");
        while running_swapper.load(Ordering::SeqCst) {
            if std::fs::rename(&docs, &real).is_ok() {
                let _ = symlink(&outside_swapper, &docs);
                let _ = std::fs::remove_file(&docs);
                let _ = std::fs::rename(&real, &docs);
            }
        }
        let _ = std::fs::remove_file(&docs);
        let _ = std::fs::rename(&real, &docs);
    });
    for _ in 0..1_000 {
        if let Ok(mut file) = open_absolute_path_without_symlinks(
            &workspace.join("docs/USER_GUIDE.md"),
            libc::O_RDONLY,
        ) {
            let mut content = String::new();
            file.read_to_string(&mut content).unwrap();
            assert_eq!(content, "safe");
        }
    }
    running.store(false, Ordering::SeqCst);
    swapper.join().unwrap();
}

#[test]
fn delegated_manifest_spend_lease_and_terminal_are_restart_safe() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("delegated.db");
    let store = LocalProductStore::new_with_clock(&path, || "2026-07-25T12:00:00Z".into()).unwrap();
    seed_key(&store, "tenant-a", "delegated-operator");
    let principal = store
        .authenticate_managed_acceptance_principal("tenant-a", "delegated-operator", Some(1.0))
        .unwrap();
    let delegation = delegated_contract();
    let persisted = store.persist_delegation(&principal, &delegation).unwrap();
    store
        .with_conn(|connection| {
            connection
                .execute(
                    "INSERT INTO product_tasks (
                            task_id, schema_version, tenant_id, workspace_id, idempotency_key,
                            status, version, objective_fingerprint, target_id, target_repo_path,
                            source_revision, source_tree_hash, output_intent, risk_class,
                            approval_required, confirm_execution, confirm_output,
                            intake_contract_sha256, intake_json, workspace_binding_json,
                            plan_id, run_id, workspace_record_id, failure_code, failure_detail,
                            created_at, updated_at, created_by
                         ) VALUES (
                            ?1, 'product_task.v1', 'tenant-a', 'default',
                            'provider-journal-restart', 'graph_ready', 1, ?2,
                            'alters-lab-docs', '/redacted/target', ?3, NULL, 'draft_pr', 'low',
                            1, 1, 1, ?4, '{}', ?5, NULL, 'run-provider-journal-1',
                            NULL, NULL, NULL, ?6, ?6, 'test'
                         )",
                    params![
                        "product-task-golden-path-1",
                        "a".repeat(64),
                        "b".repeat(40),
                        "c".repeat(64),
                        json!({
                            "allowed_paths": ["docs/USER_GUIDE.md"],
                            "workspace_path": "/redacted/not-used"
                        })
                        .to_string(),
                        "2026-07-25T12:00:00Z"
                    ],
                )
                .map(|_| ())
                .map_err(|error| error.to_string())
        })
        .unwrap();
    store
        .bind_delegation_to_product_task(
            &principal,
            "product-task-golden-path-1",
            &delegation.delegation_id,
        )
        .unwrap();
    let proposal = delegated_proposal();
    store
        .persist_approved_delegated_proposal(
            &delegation.delegation_id,
            &proposal,
            proposal["manifest_sha256"].as_str().unwrap(),
        )
        .unwrap();
    let mut mutated_proposal = proposal.clone();
    mutated_proposal["target_main_sha"] = json!("7".repeat(40));
    mutated_proposal["manifest_sha256"] = Value::Null;
    mutated_proposal["manifest_sha256"] =
        json!(compute_attempt_manifest_sha256(&mutated_proposal).unwrap());
    let mut mutated_execution = delegated_execution();
    mutated_execution["target_main_sha"] = json!("7".repeat(40));
    let mutated_manifest =
        derive_final_execution_manifest(&mutated_proposal, &delegation, &mutated_execution)
            .unwrap();
    assert!(store
        .approve_delegated_manifest(&principal, &delegation.delegation_id, &mutated_manifest,)
        .unwrap_err()
        .contains("approved proposal"));
    let manifest =
        derive_final_execution_manifest(&proposal, &delegation, &delegated_execution()).unwrap();
    let approval = store
        .approve_delegated_manifest(&principal, &delegation.delegation_id, &manifest)
        .unwrap();
    let replayed_approval = store
        .approve_delegated_manifest(&principal, &delegation.delegation_id, &manifest)
        .unwrap();
    assert_eq!(approval, replayed_approval);
    let spend = store
        .issue_delegated_spend(
            &principal,
            &delegation.delegation_id,
            approval["approval_receipt_sha256"].as_str().unwrap(),
            &manifest,
        )
        .unwrap();
    assert_eq!(persisted["status"], "active");
    assert_eq!(spend["status"], "active");
    assert!(store
        .admit_delegated_attempt(
            &principal,
            &delegation.delegation_id,
            "attempt-golden-path-1",
            &manifest,
        )
        .unwrap_err()
        .contains("stale or mismatched"));
    assert_eq!(
        store
            .issue_delegated_spend(
                &principal,
                &delegation.delegation_id,
                approval["approval_receipt_sha256"].as_str().unwrap(),
                &manifest,
            )
            .unwrap(),
        spend
    );
    let activator = delegated_test_principal("delegated-activator");
    let lease = store
        .admit_delegated_attempt(
            &activator,
            &delegation.delegation_id,
            "attempt-golden-path-1",
            &manifest,
        )
        .unwrap();
    assert_eq!(lease["status"], "admitted");
    assert!(store
        .admit_delegated_attempt(
            &delegated_test_principal("delegated-activator"),
            &delegation.delegation_id,
            "attempt-golden-path-2",
            &manifest,
        )
        .is_err());
    let delegated_binding = crate::provider::managed_deepseek::ManagedCallBinding {
        product_task_id: "product-task-golden-path-1".into(),
        workflow_id: "workflow-golden-path-1".into(),
        node_id: "planning".into(),
        attempt_id: "attempt-golden-path-1".into(),
        spend_authorization_id: spend["spend_authorization_id"].as_str().unwrap().into(),
        attempt_lease_id: lease["attempt_lease_id"].as_str().unwrap().into(),
    };
    let authority = <LocalProductStore as crate::provider::managed_deepseek::ManagedAuthoritySource>::current_authority(
            &store,
            &delegated_binding,
        )
        .unwrap();
    assert_eq!(authority.lease_status, "current");
    assert_eq!(authority.spend_status, "consumed");
    let gateway_permit = store
        .codex_gateway_start_permit_for_delegated_attempt(&delegated_binding)
        .unwrap();
    assert!(gateway_permit.execution_id().starts_with("codex-attempt-"));
    let contract = authority.execution_contract.clone().unwrap();
    let mut request = crate::provider::managed_deepseek::ManagedProviderCallRequest::for_role(
        crate::provider::managed_deepseek::ManagedModelRole::Planner,
        contract.protocol,
        delegated_binding.clone(),
    );
    request.limits = contract.limits;
    request.price_profile = contract.price_profile;
    request.max_output_tokens = request.limits.max_output_tokens;
    request.messages = vec![crate::provider::managed_deepseek::ManagedMessage::text(
        "user",
        "DO_NOT_PERSIST_PROVIDER_PROMPT_CONTENT",
    )];
    store
        .with_conn(|connection| {
            connection
                .execute(
                    "INSERT INTO workflow_runs (
                            run_sequence, run_id, plan_id, created_at, updated_at, status,
                            workflow_id, dispatch_id, started_at, completed_at, result_json,
                            boundaries_json, run_json, priority, tenant_id
                         ) VALUES (
                            1, 'run-provider-journal-1', NULL, ?1, ?1, 'running',
                            ?2, NULL, ?1, NULL, NULL, '{}', '{}', 5, 'tenant-a'
                         )",
                    params!["2026-07-25T12:00:00Z", delegated_binding.workflow_id],
                )
                .map_err(|error| error.to_string())?;
            connection
                .execute(
                    "INSERT INTO workflow_run_nodes (
                            run_id, node_id, task_type, status, node_json, started_at,
                            attempt_count, leased_at
                         ) VALUES (
                            'run-provider-journal-1', ?1, 'managed_deepseek', 'running',
                            '{}', ?2, 1, ?2
                         )",
                    params![delegated_binding.node_id, "2026-07-25T12:00:00Z"],
                )
                .map(|_| ())
                .map_err(|error| error.to_string())
        })
        .unwrap();
    store.claim_delegated_provider_request(&request).unwrap();
    // The durable claim records the request's execution-path transport
    // provenance (for_role defaults to the fail-closed injected value).
    let claimed_journal: String = store
        .with_conn(|connection| {
            connection
                .query_row(
                    "SELECT provider_request_journal_json
                         FROM managed_acceptance_delegations WHERE delegation_id=?1",
                    params![delegation.delegation_id],
                    |row| row.get(0),
                )
                .map_err(|error| error.to_string())
        })
        .unwrap();
    assert!(claimed_journal.contains("\"transport_provenance\":\"injected\""));
    assert!(claimed_journal.contains("\"status\":\"sending\""));
    assert!(store
        .complete_delegated_attempt(
            &delegation.delegation_id,
            "attempt-golden-path-1",
            lease["attempt_lease_token"].as_str().unwrap(),
            "in_flight",
            &json!({"provider_requests": 0}),
            0.0,
        )
        .is_err());
    let restarted =
        LocalProductStore::new_with_clock(&path, || "2026-07-25T12:00:00Z".into()).unwrap();
    let replay_error = restarted
        .claim_delegated_provider_request(&request)
        .unwrap_err();
    assert!(replay_error.contains("replay is forbidden"));
    restarted
        .reconcile_delegated_provider_request(
            &request,
            None,
            crate::provider::managed_deepseek::ManagedFailureEffect::OutcomeUnknown,
        )
        .unwrap();
    assert!(restarted
        .claim_delegated_provider_request(&request)
        .unwrap_err()
        .contains("replay is forbidden"));
    let journal: String = restarted
        .with_conn(|connection| {
            connection
                .query_row(
                    "SELECT provider_request_journal_json
                         FROM managed_acceptance_delegations WHERE delegation_id=?1",
                    params![delegation.delegation_id],
                    |row| row.get(0),
                )
                .map_err(|error| error.to_string())
        })
        .unwrap();
    assert!(!journal.contains("DO_NOT_PERSIST_PROVIDER_PROMPT_CONTENT"));
    assert!(journal.contains("\"status\":\"outcome_unknown\""));
    // Provenance survives restart and reconcile unchanged; an injected
    // claim can never be upgraded to external.
    assert!(journal.contains("\"transport_provenance\":\"injected\""));
    assert!(!journal.contains("\"transport_provenance\":\"external\""));
    let replay = restarted
        .admit_delegated_attempt(
            &delegated_test_principal("delegated-activator"),
            &delegation.delegation_id,
            "attempt-golden-path-1",
            &manifest,
        )
        .unwrap();
    assert_eq!(replay["replayed"], true);
    assert!(restarted
        .complete_delegated_attempt(
            &delegation.delegation_id,
            "attempt-golden-path-1",
            lease["attempt_lease_token"].as_str().unwrap(),
            "failed",
            &json!({"provider_requests": 1, "raw_output": "[redacted]"}),
            0.51,
        )
        .is_err());
    let terminal = restarted
        .complete_delegated_attempt(
            &delegation.delegation_id,
            "attempt-golden-path-1",
            lease["attempt_lease_token"].as_str().unwrap(),
            "outcome_unknown",
            &json!({"provider_requests": 1, "raw_output": "[redacted]"}),
            0.10,
        )
        .unwrap();
    assert_eq!(terminal["status"], "closed");
    assert_eq!(terminal["spend_authorization_state"], "expired");
    assert_eq!(terminal["attempt_lease_state"], "closed");
    assert_eq!(terminal["delegation_state"], "expired");
    assert!(restarted
        .complete_delegated_attempt(
            &delegation.delegation_id,
            "attempt-golden-path-1",
            lease["attempt_lease_token"].as_str().unwrap(),
            "failed",
            &json!({"provider_requests": 1, "raw_output": "[redacted]"}),
            0.10,
        )
        .is_err());
    let retry = restarted.issue_delegated_spend(
        &principal,
        &delegation.delegation_id,
        approval["approval_receipt_sha256"].as_str().unwrap(),
        &manifest,
    );
    assert!(retry.is_err(), "outcome_unknown must not permit a retry");
}

#[test]
fn provider_journal_claim_records_injected_provenance_and_reconcile_preserves_it() {
    use crate::provider::managed_deepseek::{
        DeepSeekProtocol, ManagedCallBinding, ManagedCallLimits, ManagedModelRole,
        ManagedProviderCallRequest,
    };
    use crate::provider::transport::ProviderTransportProvenance;
    let binding = ManagedCallBinding {
        product_task_id: "pt-journal".into(),
        workflow_id: "wf-journal".into(),
        node_id: "planning".into(),
        attempt_id: "attempt-journal".into(),
        spend_authorization_id: "spend-journal".into(),
        attempt_lease_id: "lease-journal".into(),
    };
    let mut request = ManagedProviderCallRequest::for_role(
        ManagedModelRole::Planner,
        DeepSeekProtocol::OpenAiCompatible,
        binding,
    );
    request.limits = ManagedCallLimits {
        max_requests: 3,
        max_retries: 0,
        max_input_tokens: 8_000,
        max_output_tokens: 4_000,
        max_cumulative_tokens: 24_000,
        timeout_ms: 30_000,
        max_cost_usd: None,
    };
    // for_role defaults to the fail-closed injected provenance; only the
    // executor stamping from the actual transport can set external.
    assert_eq!(request.transport_provenance.as_str(), "injected");
    let scheduler = ManagedSchedulerLeaseAuthority {
        run_id: "run-journal".into(),
        attempt_count: 1,
        lease_owner_token_sha256: "lease-token-sha".into(),
        allowed_paths: json!([]),
        workspace_path: "/redacted/workspace".into(),
    };
    let claimed =
        claim_provider_journal_entry("[]", &request, &scheduler, "2026-07-25T12:00:00Z").unwrap();
    let parsed: Value = serde_json::from_str(&claimed).unwrap();
    assert_eq!(
        parsed[0]["schema_version"],
        "managed_provider_request_claim.v1"
    );
    assert_eq!(parsed[0]["transport_provenance"], "injected");
    assert_eq!(parsed[0]["status"], "sending");
    // Reconcile preserves provenance and never upgrades it.
    let reconciled = reconcile_provider_journal_entry(
        &claimed,
        &request,
        None,
        crate::provider::managed_deepseek::ManagedFailureEffect::PreSend,
        "2026-07-25T12:00:00Z",
    )
    .unwrap();
    let parsed2: Value = serde_json::from_str(&reconciled).unwrap();
    assert_eq!(parsed2[0]["transport_provenance"], "injected");
    assert_eq!(parsed2[0]["status"], "failed_before_send");
    // A request that forges external provenance cannot reconcile or claim
    // the same node: the claim hash and node identity bind the provenance.
    let mut forged = request.clone();
    forged.transport_provenance = ProviderTransportProvenance::External;
    assert!(reconcile_provider_journal_entry(
        &reconciled,
        &forged,
        None,
        crate::provider::managed_deepseek::ManagedFailureEffect::PreSend,
        "2026-07-25T12:00:00Z",
    )
    .is_err());
    assert!(
        claim_provider_journal_entry(&reconciled, &forged, &scheduler, "2026-07-25T12:00:00Z")
            .is_err()
    );
    // An external-provenance claim on a fresh node records external.
    let mut external = request.clone();
    external.binding.node_id = "review".into();
    external.transport_provenance = ProviderTransportProvenance::External;
    let external_claimed =
        claim_provider_journal_entry("[]", &external, &scheduler, "2026-07-25T12:00:00Z").unwrap();
    let parsed3: Value = serde_json::from_str(&external_claimed).unwrap();
    assert_eq!(parsed3[0]["transport_provenance"], "external");
}

#[test]
fn delegated_authority_rechecks_revocation_before_provider_or_workspace_effect() {
    let (_dir, store) = store();
    let now = "2026-07-25T12:00:00Z";
    let delegation = delegated_contract();
    let principal = AuthenticatedPrincipal {
        tenant_id: "tenant-a".into(),
        principal_id: "delegated-operator".into(),
        principal_kind: PrincipalKind::OperatorApiKey,
        scopes: ALL_MANAGED_ACCEPTANCE_SCOPES
            .iter()
            .map(|scope| (*scope).to_string())
            .collect(),
        user_id: "operator".into(),
        role: "owner".into(),
    };
    // The test store clock is fixed before the delegation expiry and this
    // direct authenticated principal is deliberately non-fixture.
    assert!(delegation.validate(now).is_ok());
    let target_repo = _dir.path().join("revocation-target");
    let workspace_path = _dir.path().join("revocation-workspace");
    std::fs::create_dir_all(&target_repo).unwrap();
    std::fs::create_dir_all(&workspace_path).unwrap();
    let workspace = store
        .record_supervised_patch_workspace(
            &json!({
                "run_id": "run-revocation-recheck",
                "target_id": "alters-lab-docs",
                "target_repo_path": target_repo,
                "workspace_path": workspace_path,
                "source_revision": "source-revocation-recheck",
                "workspace_mode": "copy",
                "status": "workspace_created"
            }),
            "test",
        )
        .unwrap();
    let workspace_id = workspace["workspace_id"].as_str().unwrap();
    store
        .with_conn(|connection| {
            connection
                .execute(
                    "INSERT INTO product_tasks (
                            task_id, schema_version, tenant_id, workspace_id, idempotency_key,
                            status, version, objective_fingerprint, target_id, target_repo_path,
                            source_revision, source_tree_hash, output_intent, risk_class,
                            approval_required, confirm_execution, confirm_output,
                            intake_contract_sha256, intake_json, workspace_binding_json,
                            plan_id, run_id, workspace_record_id, failure_code, failure_detail,
                            created_at, updated_at, created_by
                         ) VALUES (
                            ?1, 'product_task.v1', 'tenant-a', 'default',
                            'revocation-recheck', 'graph_ready', 1, ?2,
                            'alters-lab-docs', '/redacted/target', ?3, NULL, 'draft_pr', 'low',
                            1, 1, 1, ?4, '{}', ?5, NULL, NULL,
                            ?6, NULL, NULL, ?7, ?7, 'test'
                         )",
                    params![
                        "product-task-golden-path-1",
                        "a".repeat(64),
                        "b".repeat(40),
                        "c".repeat(64),
                        json!({
                            "allowed_paths": ["docs/USER_GUIDE.md"],
                            "workspace_path": "/redacted/not-used"
                        })
                        .to_string(),
                        workspace_id,
                        now
                    ],
                )
                .map(|_| ())
                .map_err(|error| error.to_string())
        })
        .unwrap();
    store
        .persist_delegation_for_product_task(&principal, "product-task-golden-path-1", &delegation)
        .unwrap();
    let proposal = delegated_proposal();
    store
        .persist_approved_delegated_proposal(
            &delegation.delegation_id,
            &proposal,
            proposal["manifest_sha256"].as_str().unwrap(),
        )
        .unwrap();
    let manifest =
        derive_final_execution_manifest(&proposal, &delegation, &delegated_execution()).unwrap();
    let approval = store
        .approve_delegated_manifest(&principal, &delegation.delegation_id, &manifest)
        .unwrap();
    let spend = store
        .issue_delegated_spend(
            &principal,
            &delegation.delegation_id,
            approval["approval_receipt_sha256"].as_str().unwrap(),
            &manifest,
        )
        .unwrap();
    let activator = delegated_test_principal("delegated-activator");
    let lease = store
        .admit_delegated_attempt(
            &activator,
            &delegation.delegation_id,
            "attempt-golden-path-1",
            &manifest,
        )
        .unwrap();
    let binding = crate::provider::managed_deepseek::ManagedCallBinding {
        product_task_id: "product-task-golden-path-1".into(),
        workflow_id: "workflow-golden-path-1".into(),
        node_id: "planning".into(),
        attempt_id: "attempt-golden-path-1".into(),
        spend_authorization_id: spend["spend_authorization_id"].as_str().unwrap().into(),
        attempt_lease_id: lease["attempt_lease_id"].as_str().unwrap().into(),
    };
    store
        .revoke_delegation(&principal, &delegation.delegation_id)
        .unwrap();
    let recoverable: (String, String, String, Option<String>) = store
        .with_conn(|connection| {
            connection
                .query_row(
                    "SELECT status, spend_status, attempt_status, terminal_receipt_json
                         FROM managed_acceptance_delegations WHERE delegation_id=?1",
                    params![delegation.delegation_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )
                .map_err(|error| error.to_string())
        })
        .unwrap();
    assert_eq!(recoverable.0, "revoked");
    assert_eq!(recoverable.1, "revoked");
    assert_eq!(recoverable.2, "closed");
    assert!(recoverable.3.is_some());
    assert!(<LocalProductStore as crate::provider::managed_deepseek::ManagedAuthoritySource>::current_authority(&store, &binding).is_err());
}

#[test]
fn delegated_authority_rejects_mutation_revocation_expiry_and_output_escape() {
    let delegation = delegated_contract();
    let mut proposal = delegated_proposal();
    proposal["max_cost_usd"] = json!(0.50);
    assert!(
        derive_final_execution_manifest(&proposal, &delegation, &delegated_execution()).is_err()
    );
    let mut expired = delegation.clone();
    expired.expires_at = "2026-07-25T11:59:59Z".into();
    assert!(expired.validate("2026-07-25T12:00:00Z").is_err());
    let manifest =
        derive_final_execution_manifest(&delegated_proposal(), &delegation, &delegated_execution())
            .unwrap();
    let bad_artifact = json!({
        "artifact_sha256": "a".repeat(64),
        "changed_files": ["../outside"],
        "changed_lines": 1
    });
    let verification = json!({"status": "succeeded", "verification_sha256": "b".repeat(64)});
    let review = json!({
        "schema_version": "managed_deepseek_review_receipt.v1",
        "status": "accepted",
        "material_objection_count": 0,
        "resolved_model": "deepseek-v4-pro"
    });
    let provider_execution = json!({
        "schema_version": "managed_deepseek_execution_evidence.v1",
        "provider_request_count": 3,
        "cumulative_tokens": 18,
        "realized_cost_usd": 0.01,
        "requests": [
            {"stage":"planning","role":"planner","protocol":"openai_compatible","requested_model":"deepseek-v4-pro","resolved_model":"deepseek-v4-pro","request_id":"request-1","usage":{"input_tokens":4,"output_tokens":2,"cache_read_tokens":0,"cache_creation_tokens":0,"reasoning_output_tokens":0,"fresh_input_tokens":4,"cumulative_tokens":6,"model":"deepseek-v4-pro","request_id":"request-1"},"realized_cost_usd":0.004},
            {"stage":"implementation","role":"implementer","protocol":"openai_compatible","requested_model":"deepseek-v4-flash","resolved_model":"deepseek-v4-flash","request_id":"request-2","usage":{"input_tokens":4,"output_tokens":2,"cache_read_tokens":0,"cache_creation_tokens":0,"reasoning_output_tokens":0,"fresh_input_tokens":4,"cumulative_tokens":6,"model":"deepseek-v4-flash","request_id":"request-2"},"realized_cost_usd":0.002},
            {"stage":"review","role":"reviewer","protocol":"openai_compatible","requested_model":"deepseek-v4-pro","resolved_model":"deepseek-v4-pro","request_id":"request-3","usage":{"input_tokens":4,"output_tokens":2,"cache_read_tokens":0,"cache_creation_tokens":0,"reasoning_output_tokens":0,"fresh_input_tokens":4,"cumulative_tokens":6,"model":"deepseek-v4-pro","request_id":"request-3"},"realized_cost_usd":0.004}
        ]
    });
    let err = confirm_delegated_artifact_output(
        &delegation,
        &manifest,
        &bad_artifact,
        &verification,
        &review,
        &provider_execution,
        "6".repeat(40).as_str(),
        0.01,
    )
    .unwrap_err();
    assert!(err.contains("path"));

    // Strict containment: the parent of the allowed file entry, pseudo
    // children of it, and prefix-collision names are never admitted.
    for escaped_path in [
        "docs",
        "docs/USER_GUIDE.md/child",
        "README.md.bak",
        "README.md/child",
        "docs/USER_GUIDE.md/../../outside",
    ] {
        let escaping = json!({
            "artifact_sha256": "a".repeat(64),
            "changed_files": [escaped_path],
            "changed_lines": 1
        });
        let err = confirm_delegated_artifact_output(
            &delegation,
            &manifest,
            &escaping,
            &verification,
            &review,
            &provider_execution,
            "6".repeat(40).as_str(),
            0.01,
        )
        .unwrap_err();
        assert!(
            err.contains("path"),
            "{escaped_path} must fail closed: {err}"
        );
    }

    let artifact = json!({
        "artifact_sha256": "a".repeat(64),
        "changed_files": ["docs/USER_GUIDE.md"],
        "changed_lines": 1
    });
    let mut failed_verification = verification.clone();
    failed_verification["status"] = json!("failed");
    assert!(confirm_delegated_artifact_output(
        &delegation,
        &manifest,
        &artifact,
        &failed_verification,
        &review,
        &provider_execution,
        "6".repeat(40).as_str(),
        0.01,
    )
    .unwrap_err()
    .contains("verification"));
    assert!(confirm_delegated_artifact_output(
        &delegation,
        &manifest,
        &artifact,
        &verification,
        &review,
        &provider_execution,
        "7".repeat(40).as_str(),
        0.01,
    )
    .unwrap_err()
    .contains("target"));
    let mut mismatched_usage = provider_execution.clone();
    mismatched_usage["requests"][1]["usage"]["model"] = json!("deepseek-v4-pro");
    assert!(confirm_delegated_artifact_output(
        &delegation,
        &manifest,
        &artifact,
        &verification,
        &review,
        &mismatched_usage,
        "6".repeat(40).as_str(),
        0.01,
    )
    .unwrap_err()
    .contains("provider request"));
    let mut mismatched_cost = provider_execution.clone();
    mismatched_cost["requests"][0]["realized_cost_usd"] = json!(0.005);
    assert!(confirm_delegated_artifact_output(
        &delegation,
        &manifest,
        &artifact,
        &verification,
        &review,
        &mismatched_cost,
        "6".repeat(40).as_str(),
        0.01,
    )
    .unwrap_err()
    .contains("provider request"));

    // The persisted manifest owns the envelope: a valid RWE package may
    // exceed the legacy DeepSeek 16k ceiling (Codex uses 20,192).
    let mut rwe_delegation = delegation.clone();
    rwe_delegation.task_classes = vec!["rwe".into()];
    rwe_delegation.allowed_paths =
        crate::rwe::frozen_rwe_bindings::frozen_rwe_union_allowed_paths().unwrap();
    let (max_changed_files, max_changed_lines) =
        crate::rwe::frozen_rwe_bindings::frozen_rwe_max_patch_limits().unwrap();
    rwe_delegation.max_changed_files = max_changed_files;
    rwe_delegation.max_changed_lines = max_changed_lines;
    let frozen = crate::rwe::operator_corpus::freeze_current_operator_contract_set().unwrap();
    let cell_cost = frozen.schedule.body["cells"]
        .as_array()
        .and_then(|cells| cells.first())
        .and_then(|cell| {
            crate::rwe::frozen_rwe_bindings::frozen_schedule_cell_max_cost(cell).unwrap()
        })
        .unwrap();
    rwe_delegation.max_cost_usd_per_run = cell_cost;
    rwe_delegation.max_total_cost_usd = cell_cost;
    let mut rwe_manifest = manifest.clone();
    rwe_manifest["limits"]["max_input_tokens"] = json!(12_000);
    rwe_manifest["limits"]["max_output_tokens"] = json!(8_192);
    rwe_manifest["limits"]["max_cumulative_tokens"] = json!(20_000);
    rwe_manifest["manifest_sha256"] =
        json!(compute_attempt_manifest_sha256(&rwe_manifest).unwrap());
    let mut rwe_artifact = artifact.clone();
    rwe_artifact["changed_files"] = json!([rwe_delegation.allowed_paths[0].clone()]);
    let mut high_usage = provider_execution.clone();
    for request in high_usage["requests"].as_array_mut().unwrap() {
        request["usage"]["cumulative_tokens"] = json!(6_000);
    }
    high_usage["cumulative_tokens"] = json!(18_000);
    let accepted = confirm_delegated_artifact_output(
        &rwe_delegation,
        &rwe_manifest,
        &rwe_artifact,
        &verification,
        &review,
        &high_usage,
        "6".repeat(40).as_str(),
        0.01,
    )
    .unwrap();
    assert_eq!(accepted["provider_execution"]["cumulative_tokens"], 18_000);
}

#[test]
fn managed_workspace_action_parser_allows_only_one_json_fence() {
    let action = r#"{"schema_version":"managed_workspace_action.v1","action":"replace_text"}"#;
    let fenced_json = format!("```json\n{action}\n```");
    assert_eq!(
        LocalProductStore::parse_managed_workspace_action(&fenced_json).unwrap()["schema_version"],
        "managed_workspace_action.v1"
    );
    let fenced_plain = format!("```\n{action}\n```");
    assert_eq!(
        LocalProductStore::parse_managed_workspace_action(&fenced_plain).unwrap()["schema_version"],
        "managed_workspace_action.v1"
    );
    let fenced_caps = format!("```JSON\r\n{action}\r\n```");
    assert_eq!(
        LocalProductStore::parse_managed_workspace_action(&fenced_caps).unwrap()["schema_version"],
        "managed_workspace_action.v1"
    );
    let single_line_json = format!("```json {action} ```");
    assert_eq!(
        LocalProductStore::parse_managed_workspace_action(&single_line_json).unwrap()
            ["schema_version"],
        "managed_workspace_action.v1"
    );
    let single_line_plain = format!("``` {action} ```");
    assert_eq!(
        LocalProductStore::parse_managed_workspace_action(&single_line_plain).unwrap()
            ["schema_version"],
        "managed_workspace_action.v1"
    );
    let bare = action;
    assert_eq!(
        LocalProductStore::parse_managed_workspace_action(bare).unwrap()["schema_version"],
        "managed_workspace_action.v1"
    );
    let with_prose = format!("Here is the action:\n```json\n{action}\n```\nAll done.");
    assert_eq!(
        LocalProductStore::parse_managed_workspace_action(&with_prose).unwrap()["schema_version"],
        "managed_workspace_action.v1"
    );
    let embedded_bare = format!("Applying update: {action}. Please verify.");
    assert_eq!(
        LocalProductStore::parse_managed_workspace_action(&embedded_bare).unwrap()
            ["schema_version"],
        "managed_workspace_action.v1"
    );
    assert!(LocalProductStore::parse_managed_workspace_action(
        "```json\n{}\n```\n```json\n{}\n```"
    )
    .is_err());
    assert!(LocalProductStore::parse_managed_workspace_action("```python\n{}\n```").is_err());
}

#[test]
fn workspace_provider_journal_uses_manifest_provider_identity() {
    let binding = crate::provider::managed_deepseek::ManagedCallBinding {
        product_task_id: "pt-codex-workspace".into(),
        workflow_id: "wf-codex-workspace".into(),
        node_id: "implementation".into(),
        attempt_id: "attempt-codex-workspace".into(),
        spend_authorization_id: "spend-codex-workspace".into(),
        attempt_lease_id: "lease-codex-workspace".into(),
    };
    let scheduler = ManagedSchedulerLeaseAuthority {
        run_id: "run-codex-workspace".into(),
        attempt_count: 1,
        lease_owner_token_sha256: "scheduler-lease-sha".into(),
        allowed_paths: json!([]),
        workspace_path: "/redacted/workspace".into(),
    };
    let manifest = json!({
        "provider": {
            "provider_identity": "chatgpt-codex-subscription",
            "provider_kind": "chatgpt_subscription",
            "admitted_model": "gpt-5.6-luna"
        }
    });
    let journal = json!([{
        "node_id": binding.node_id,
        "status": "succeeded",
        "role": "implementer",
        "provider_identity": "chatgpt-codex-subscription",
        "provider_kind": "chatgpt_subscription",
        "requested_model": "gpt-5.6-luna",
        "scheduler_lease": {
            "product_task_id": binding.product_task_id,
            "run_id": scheduler.run_id,
            "workflow_id": binding.workflow_id,
            "node_id": binding.node_id,
            "attempt_count": scheduler.attempt_count,
            "lease_owner_token_sha256": scheduler.lease_owner_token_sha256
        }
    }]);
    assert!(validate_workspace_provider_journal(
        &journal.to_string(),
        &binding,
        &scheduler,
        &manifest,
    )
    .is_ok());

    let mut wrong_model = manifest.clone();
    wrong_model["provider"]["admitted_model"] = json!("gpt-5.6-terra");
    assert!(validate_workspace_provider_journal(
        &journal.to_string(),
        &binding,
        &scheduler,
        &wrong_model,
    )
    .is_err());

    let mut wrong_provider = manifest.clone();
    wrong_provider["provider"]["provider_identity"] = json!("different-provider");
    assert!(validate_workspace_provider_journal(
        &journal.to_string(),
        &binding,
        &scheduler,
        &wrong_provider,
    )
    .is_err());

    let wrong_kind = json!({
        "provider": {
            "provider_identity": "chatgpt-codex-subscription",
            "provider_kind": "different-kind",
            "admitted_model": "gpt-5.6-luna"
        }
    });
    assert!(validate_workspace_provider_journal(
        &journal.to_string(),
        &binding,
        &scheduler,
        &wrong_kind,
    )
    .is_err());
}

#[test]
fn managed_workspace_action_requires_store_owned_current_authority() {
    let (_dir, store) = store();
    let workspace = tempdir().unwrap();
    let docs = workspace.path().join("docs");
    std::fs::create_dir_all(&docs).unwrap();
    let target = docs.join("USER_GUIDE.md");
    let long_prefix = (0..120)
        .map(|line| format!("guide line {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(
        &target,
        format!("# Guide\n{long_prefix}\nalters-lab doctor checks health.\n"),
    )
    .unwrap();
    let binding = crate::provider::managed_deepseek::ManagedCallBinding {
        product_task_id: "product-task-golden-path-1".into(),
        workflow_id: "workflow-golden-path-1".into(),
        node_id: "implementation".into(),
        attempt_id: "attempt-golden-path-1".into(),
        spend_authorization_id: "spend-golden-path-1".into(),
        attempt_lease_id: "lease-golden-path-1".into(),
    };
    let metadata = json!({
        "workspace_path": workspace.path(),
        "workspace_root": workspace.path(),
        "allowed_paths": ["docs/USER_GUIDE.md"]
    });
    let action = json!({
        "schema_version": "managed_workspace_action.v1",
        "action": "replace_text",
        "path": "docs/USER_GUIDE.md",
        "old_text": "alters-lab doctor checks health.",
        "new_text": "alters-lab doctor performs a read-only health check."
    });
    assert!(store
        .apply_managed_workspace_action(&binding, &metadata, &action.to_string())
        .unwrap_err()
        .contains("current delegated authority"));
    assert!(std::fs::read_to_string(&target)
        .unwrap()
        .contains("checks health"));

    let escape = json!({
        "schema_version": "managed_workspace_action.v1",
        "action": "write_file",
        "path": "../outside.md",
        "content": "unsafe"
    });
    assert!(store
        .apply_managed_workspace_action(&binding, &metadata, &escape.to_string())
        .is_err());
    assert!(store
        .apply_managed_workspace_action(&binding, &metadata, "{malformed")
        .is_err());
    assert!(<LocalProductStore as crate::provider::managed_deepseek::ManagedAuthoritySource>::apply_workspace_action(
            &store,
            &binding,
            &metadata,
            &action.to_string(),
        )
        .is_err());
}

#[test]
fn delegated_product_task_prepare_approve_lease_and_activate_is_provider_free() {
    struct DelegatedGoldenPathEnvGuard {
        values: Vec<(&'static str, Option<std::ffi::OsString>)>,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl DelegatedGoldenPathEnvGuard {
        fn enable() -> Self {
            let lock = crate::cli::config::cli_env_test_lock()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let names = [
                PRODUCT_TASK_GATE,
                "ACP_ENABLE_TARGET_REPO_OUTPUT",
                "ACP_TARGET_REPO_OUTPUT_KILL_SWITCH",
                "ACP_TARGET_REPO_REMOTE_HOST_ALLOWLIST",
            ];
            let values = names
                .into_iter()
                .map(|name| (name, std::env::var_os(name)))
                .collect();
            std::env::set_var(PRODUCT_TASK_GATE, "1");
            std::env::set_var("ACP_ENABLE_TARGET_REPO_OUTPUT", "1");
            std::env::set_var("ACP_TARGET_REPO_OUTPUT_KILL_SWITCH", "0");
            std::env::set_var("ACP_TARGET_REPO_REMOTE_HOST_ALLOWLIST", "github.com");
            Self {
                values,
                _lock: lock,
            }
        }
    }

    impl Drop for DelegatedGoldenPathEnvGuard {
        fn drop(&mut self) {
            for (name, value) in self.values.drain(..) {
                if let Some(value) = value {
                    std::env::set_var(name, value);
                } else {
                    std::env::remove_var(name);
                }
            }
        }
    }

    let _env = DelegatedGoldenPathEnvGuard::enable();

    let (dir, store) = store();
    let store = Arc::new(store);
    let repo = dir.path().join("target");
    std::fs::create_dir_all(repo.join("docs")).unwrap();
    std::fs::write(
        repo.join("docs/USER_GUIDE.md"),
        "# User guide\n\n`alters-lab doctor` checks health.\n",
    )
    .unwrap();
    for args in [
        vec!["init", "-b", "main"],
        vec!["config", "user.email", "delegated@example.invalid"],
        vec!["config", "user.name", "Delegated Test"],
        vec!["add", "docs/USER_GUIDE.md"],
        vec!["commit", "-m", "initial"],
        vec![
            "remote",
            "add",
            "origin",
            "https://github.com/Igzela/alters-lab.git",
        ],
    ] {
        assert!(Command::new("git")
            .args(args)
            .current_dir(&repo)
            .status()
            .unwrap()
            .success());
    }
    let revision = String::from_utf8(
        Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&repo)
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap()
    .trim()
    .to_string();
    let intake = ProductTaskIntakeRequest {
        objective: "Clarify that alters-lab doctor performs a read-only health check.".into(),
        target_id: "alters-lab-docs".into(),
        target_repo_path: repo.to_string_lossy().into_owned(),
        source_kind: None,
        source_revision: revision.clone(),
        source_tree_hash: None,
        allowed_paths: vec!["docs/USER_GUIDE.md".into()],
        verification_commands: vec![ProductVerificationCommand {
            command: "grep -E read-only[[:space:]]health[[:space:]]check docs/USER_GUIDE.md".into(),
            timeout_ms: 5_000,
        }],
        output_intent: "draft_pr".into(),
        executor_policy: ProductExecutorPolicy {
            allowed_executors: vec!["managed_deepseek".into()],
            prefer: Some("managed_deepseek".into()),
        },
        budget: None,
        risk_class: "low".into(),
        approval_required: true,
        confirm_execution: Some(true),
        confirm_output: Some(true),
        idempotency_key: "delegated-product-route-1".into(),
        expected_version: None,
        tenant_id: Some("tenant-a".into()),
        workspace_id: Some("default".into()),
        workspace_mode: Some("git_worktree".into()),
        matrix_binding: None,
    };
    let mut foreign_intake = intake.clone();
    foreign_intake.idempotency_key = "delegated-product-route-foreign-tenant".into();
    foreign_intake.tenant_id = Some("foreign".into());
    let foreign_validated = validate_intake(&foreign_intake, "foreign", "default").unwrap();
    let foreign_task = store
        .admit_product_task(&foreign_validated, "executor")
        .unwrap();
    let validated = validate_intake(&intake, "tenant-a", "default").unwrap();
    let task = store.admit_product_task(&validated, "executor").unwrap();
    let task_id = task["task_id"].as_str().unwrap();

    seed_key(&store, "tenant-a", "delegated-operator");
    let principal = store
        .authenticate_managed_acceptance_principal("tenant-a", "delegated-operator", Some(1.0))
        .unwrap();
    let delegation = delegated_contract();
    store.persist_delegation(&principal, &delegation).unwrap();
    store
        .bind_delegation_to_product_task(&principal, task_id, &delegation.delegation_id)
        .unwrap();
    let mut proposal = json!({
        "schema_version": "pe7_product_golden_path_live_seal_manifest.v1",
        "target": {
            "repository": "Igzela/alters-lab",
            "default_branch": "main",
            "default_branch_sha": revision,
            "allowed_mutable_paths": ["docs/USER_GUIDE.md"],
            "target_main_must_remain_unchanged": true,
            "output": "one_unmerged_acp_draft_pr_only"
        },
        "provider": {
            "protocol": "openai_compatible",
            "execution_binding": crate::rwe::campaign_package::canonical_deepseek_provider_binding().to_json()
        },
        "role_models": {
            "planner": "deepseek-v4-pro",
            "implementer": "deepseek-v4-flash",
            "reviewer": "deepseek-v4-pro"
        },
        "limits": {
            "max_cost_usd": null
        }
    });
    proposal["manifest_sha256"] = json!(compute_attempt_manifest_sha256(&proposal).unwrap());
    store
        .persist_approved_delegated_proposal(
            &delegation.delegation_id,
            &proposal,
            proposal["manifest_sha256"].as_str().unwrap(),
        )
        .unwrap();
    let runner_absence = store
        .prepare_delegated_managed_product_task(
            task_id,
            "executor",
            &[],
            &proposal,
            &delegation,
            "attempt-runner-absent",
        )
        .unwrap_err();
    assert!(runner_absence.contains("available managed_deepseek executor"));
    let foreign_error = store
        .prepare_delegated_managed_product_task(
            foreign_task["task_id"].as_str().unwrap(),
            "executor",
            &["managed_deepseek".into()],
            &proposal,
            &delegation,
            "attempt-foreign-tenant",
        )
        .unwrap_err();
    assert!(foreign_error.contains("tenant"));
    let prepared = store
        .prepare_delegated_managed_product_task(
            task_id,
            "executor",
            &["managed_deepseek".into()],
            &proposal,
            &delegation,
            "attempt-golden-path-1",
        )
        .unwrap();
    assert_eq!(prepared["scheduler_eligible"], false);
    let replayed_prepare = store
        .prepare_delegated_managed_product_task(
            task_id,
            "executor",
            &["managed_deepseek".into()],
            &proposal,
            &delegation,
            "attempt-golden-path-1",
        )
        .unwrap();
    assert_eq!(
        replayed_prepare["plan"]["plan_id"],
        prepared["plan"]["plan_id"]
    );
    assert_eq!(
        replayed_prepare["final_manifest"]["manifest_sha256"],
        prepared["final_manifest"]["manifest_sha256"]
    );
    let orphan_plan_id = prepared["plan"]["plan_id"].as_str().unwrap().to_string();
    store
        .with_conn(|connection| {
            connection
                .execute(
                    "UPDATE product_tasks SET plan_id=NULL WHERE task_id=?1",
                    params![task_id],
                )
                .map(|_| ())
                .map_err(|error| error.to_string())
        })
        .unwrap();
    let recovered_store =
        LocalProductStore::new_with_clock(store.db_path(), || "2026-07-25T12:00:00Z".to_string())
            .unwrap();
    let recovered_prepare = recovered_store
        .prepare_delegated_managed_product_task(
            task_id,
            "recovery-owner",
            &["managed_deepseek".into()],
            &proposal,
            &delegation,
            "attempt-golden-path-1",
        )
        .unwrap();
    assert_eq!(
        recovered_prepare["plan"]["plan_id"].as_str(),
        Some(orphan_plan_id.as_str())
    );
    assert_eq!(
        recovered_prepare["final_manifest"]["manifest_sha256"],
        prepared["final_manifest"]["manifest_sha256"]
    );
    assert_eq!(
        recovered_store
            .list_workflow_plans_with_offset(10_000, 0)
            .unwrap()
            .iter()
            .filter(|plan| {
                plan.pointer("/advisory/product_task_id")
                    .and_then(Value::as_str)
                    == Some(task_id)
            })
            .count(),
        1
    );
    let manifest = prepared["final_manifest"].clone();
    let approval = store
        .approve_delegated_manifest(&principal, &delegation.delegation_id, &manifest)
        .unwrap();
    let spend = store
        .issue_delegated_spend(
            &principal,
            &delegation.delegation_id,
            approval["approval_receipt_sha256"].as_str().unwrap(),
            &manifest,
        )
        .unwrap();
    let activator = delegated_test_principal("delegated-activator");
    let lease = store
        .admit_delegated_attempt(
            &activator,
            &delegation.delegation_id,
            "attempt-golden-path-1",
            &manifest,
        )
        .unwrap();
    let activated = store
        .activate_delegated_managed_product_task(
            task_id,
            "executor",
            &manifest,
            spend["spend_authorization_id"].as_str().unwrap(),
            lease["attempt_lease_id"].as_str().unwrap(),
        )
        .unwrap();
    assert_eq!(activated["task"]["status"], "graph_ready");
    assert_eq!(activated["scheduler_eligible"], true);
    let nodes = activated["run"]["nodes"].as_array().unwrap();
    assert_eq!(nodes.len(), 4);
    assert_eq!(
        nodes[0]["managed_deepseek"]["binding"]["attempt_id"],
        "attempt-golden-path-1"
    );
    let replayed_activation = store
        .activate_delegated_managed_product_task(
            task_id,
            "executor",
            &manifest,
            spend["spend_authorization_id"].as_str().unwrap(),
            lease["attempt_lease_id"].as_str().unwrap(),
        )
        .unwrap();
    assert_eq!(replayed_activation["replayed"], true);
    assert_eq!(
        replayed_activation["run"]["run_id"],
        activated["run"]["run_id"]
    );
    assert_eq!(
        store
            .search_workflow_runs(100, 0, None)
            .unwrap()
            .iter()
            .filter(|run| run.get("plan_id") == activated["run"].get("plan_id"))
            .count(),
        1
    );

    // Fork a crash-safe local database snapshot at the exact activated
    // frontier, then prove an unknown provider effect closes all delegated
    // authority through the ordinary finalizer without a second send.
    let activated_workspace_path = activated["task"]
        .pointer("/workspace_binding/workspace_path")
        .and_then(Value::as_str)
        .unwrap()
        .to_string();
    let revocation_db = dir.path().join("delegated-revocation.db");
    store
        .with_conn(|connection| {
            connection
                .execute(
                    "VACUUM INTO ?1",
                    params![revocation_db.to_string_lossy().as_ref()],
                )
                .map(|_| ())
                .map_err(|error| error.to_string())
        })
        .unwrap();
    let revocation_workspace = dir.path().join("delegated-revocation-workspace");
    std::fs::create_dir_all(revocation_workspace.join("docs")).unwrap();
    std::fs::copy(
        PathBuf::from(&activated_workspace_path).join("docs/USER_GUIDE.md"),
        revocation_workspace.join("docs/USER_GUIDE.md"),
    )
    .unwrap();
    let revocation_workspace = std::fs::canonicalize(revocation_workspace).unwrap();
    let revocation_store =
        LocalProductStore::new_with_clock(&revocation_db, || "2026-07-25T12:00:00Z".to_string())
            .unwrap();
    let revocation_workspace_id = activated["task"]["workspace_record_id"]
        .as_str()
        .unwrap()
        .to_string();
    let mut revocation_binding =
        revocation_store.get_product_task(task_id).unwrap().unwrap()["workspace_binding"].clone();
    revocation_binding["workspace_path"] = json!(revocation_workspace);
    revocation_binding["workspace_root"] = json!(revocation_workspace);
    revocation_store
        .with_conn(|connection| {
            let workspace_json: String = connection
                .query_row(
                    "SELECT workspace_json FROM supervised_patch_workspaces
                         WHERE workspace_id=?1",
                    params![revocation_workspace_id],
                    |row| row.get(0),
                )
                .map_err(|error| error.to_string())?;
            let mut workspace_json: Value =
                serde_json::from_str(&workspace_json).map_err(|error| error.to_string())?;
            workspace_json["workspace_mode"] = json!("copy");
            workspace_json["workspace_path"] = json!(revocation_workspace);
            connection
                .execute(
                    "UPDATE product_tasks SET workspace_binding_json=?1 WHERE task_id=?2",
                    params![revocation_binding.to_string(), task_id],
                )
                .map_err(|error| error.to_string())?;
            connection
                .execute(
                    "UPDATE supervised_patch_workspaces
                         SET workspace_path=?1, workspace_canonical_path=?1, workspace_json=?2
                         WHERE workspace_id=?3",
                    params![
                        revocation_workspace.to_string_lossy().as_ref(),
                        workspace_json.to_string(),
                        revocation_workspace_id
                    ],
                )
                .map_err(|error| error.to_string())?;
            Ok(())
        })
        .unwrap();
    revocation_store
        .revoke_delegation(&principal, &delegation.delegation_id)
        .unwrap();
    let revoked_terminal: (String, String, String, String) = revocation_store
        .with_conn(|connection| {
            connection
                .query_row(
                    "SELECT status, spend_status, attempt_status, terminal_receipt_json
                         FROM managed_acceptance_delegations WHERE delegation_id=?1",
                    params![delegation.delegation_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )
                .map_err(|error| error.to_string())
        })
        .unwrap();
    assert_eq!(
        (
            &revoked_terminal.0[..],
            &revoked_terminal.1[..],
            &revoked_terminal.2[..]
        ),
        ("revoked", "revoked", "closed")
    );
    assert!(revoked_terminal
        .3
        .contains("managed_delegated_revocation_terminal_evidence.v1"));
    assert!(!revocation_workspace.exists());
    let failure_db = dir.path().join("delegated-outcome-unknown.db");
    store
        .with_conn(|connection| {
            connection
                .execute(
                    "VACUUM INTO ?1",
                    params![failure_db.to_string_lossy().as_ref()],
                )
                .map(|_| ())
                .map_err(|error| error.to_string())
        })
        .unwrap();
    let failure_workspace = dir.path().join("delegated-outcome-unknown-workspace");
    std::fs::create_dir_all(failure_workspace.join("docs")).unwrap();
    std::fs::copy(
        PathBuf::from(&activated_workspace_path).join("docs/USER_GUIDE.md"),
        failure_workspace.join("docs/USER_GUIDE.md"),
    )
    .unwrap();
    let failure_workspace = std::fs::canonicalize(failure_workspace).unwrap();
    let failure_store = Arc::new(
        LocalProductStore::new_with_clock(&failure_db, || "2026-07-25T12:00:00Z".to_string())
            .unwrap(),
    );
    let mut failure_binding =
        failure_store.get_product_task(task_id).unwrap().unwrap()["workspace_binding"].clone();
    failure_binding["workspace_path"] = json!(failure_workspace);
    failure_binding["workspace_root"] = json!(failure_workspace);
    let workspace_record_id = activated["task"]["workspace_record_id"]
        .as_str()
        .unwrap()
        .to_string();
    failure_store
        .with_conn(|connection| {
            let workspace_json: String = connection
                .query_row(
                    "SELECT workspace_json FROM supervised_patch_workspaces
                         WHERE workspace_id=?1",
                    params![workspace_record_id],
                    |row| row.get(0),
                )
                .map_err(|error| error.to_string())?;
            let mut workspace_json: Value =
                serde_json::from_str(&workspace_json).map_err(|error| error.to_string())?;
            workspace_json["workspace_mode"] = json!("copy");
            workspace_json["workspace_path"] = json!(failure_workspace);
            connection
                .execute(
                    "UPDATE product_tasks SET workspace_binding_json=?1
                         WHERE task_id=?2",
                    params![failure_binding.to_string(), task_id],
                )
                .map_err(|error| error.to_string())?;
            connection
                .execute(
                    "UPDATE supervised_patch_workspaces
                         SET workspace_path=?1, workspace_canonical_path=?1,
                             workspace_json=?2
                         WHERE workspace_id=?3",
                    params![
                        failure_workspace.to_string_lossy().as_ref(),
                        workspace_json.to_string(),
                        workspace_record_id
                    ],
                )
                .map_err(|error| error.to_string())?;
            Ok(())
        })
        .unwrap();
    let unknown_transport = Arc::new(DelegatedOutcomeUnknownTransport {
        sends: AtomicUsize::new(0),
    });
    let unknown_transport_trait: Arc<dyn HttpTransport> = unknown_transport.clone();
    let unknown_source: Arc<dyn crate::provider::managed_deepseek::ManagedAuthoritySource> =
        failure_store.clone();
    let unknown_executor = ManagedDeepSeekNodeExecutor::new(
        delegated_mock_provider("deepseek-v4-pro", unknown_transport_trait.clone()),
        delegated_mock_provider("deepseek-v4-flash", unknown_transport_trait.clone()),
        delegated_mock_provider("deepseek-v4-pro", unknown_transport_trait),
        unknown_source,
        ManagedDeepSeekExecutorConfig {
            protocol: DeepSeekProtocol::OpenAiCompatible,
            limits: ManagedCallLimits {
                max_requests: 3,
                max_retries: 0,
                max_input_tokens: 8_000,
                max_output_tokens: 4_000,
                max_cumulative_tokens: 24_000,
                timeout_ms: 30_000,
                max_cost_usd: Some(0.50),
            },
            price_profile: DeepSeekPriceProfile::default(),
        },
    )
    .unwrap();
    let failure_run_id = activated["run"]["run_id"].as_str().unwrap();
    let mut failure_tick = Value::Null;
    for _ in 0..12 {
        failure_tick = failure_store
            .tick_with_executor(failure_run_id, "executor", 0, &unknown_executor)
            .unwrap();
        if failure_tick["run"]["status"] == "failed" {
            break;
        }
    }
    assert_eq!(unknown_transport.sends.load(Ordering::SeqCst), 1);
    let failed_run = failure_store
        .get_workflow_run(failure_run_id)
        .unwrap()
        .unwrap();
    assert_eq!(
        failure_tick["run"]["status"], "failed",
        "run={failed_run:#} tick={failure_tick:#}"
    );
    assert_eq!(
        failed_run["nodes"][0]["result"]["error_domain"],
        "provider_outcome_unknown"
    );
    let failure_terminal = failure_store
        .finalize_product_task_after_execution(task_id, "recovery-owner")
        .unwrap();
    assert_eq!(
        failure_terminal["delegated_terminal"]["terminal"]["terminal_class"],
        "outcome_unknown"
    );
    assert_eq!(
        failure_terminal["delegated_terminal"]["terminal"]["spend_authorization_state"],
        "expired"
    );
    assert_eq!(
        failure_terminal["delegated_terminal"]["terminal"]["attempt_lease_state"],
        "closed"
    );
    assert_eq!(
        failure_terminal["delegated_terminal"]["terminal"]["delegation_state"],
        "expired"
    );
    assert!(!failure_workspace.exists());
    let recovered_failure_store =
        LocalProductStore::new_with_clock(&failure_db, || "2026-07-25T12:00:00Z".into()).unwrap();
    let recovered = recovered_failure_store
        .finalize_product_task_after_execution(task_id, "recovery-owner")
        .unwrap();
    assert_eq!(recovered["phase"], "terminal_failure");
    assert!(recovered["delegated_terminal"].is_null());
    // A replayed finalizer returns no new terminal, but recovery must
    // still observe the original uncertainty and conservative reservation.
    let durable = recovered_failure_store
        .delegated_authority_state(&delegation.delegation_id)
        .unwrap();
    assert_eq!(durable["attempt_lease_state"], "closed");
    assert_eq!(durable["spend_authorization_state"], "expired");
    assert_eq!(
        durable["terminal_evidence"]["terminal_class"],
        "outcome_unknown"
    );
    assert_eq!(durable["terminal_evidence"]["provider_request_count"], 1);
    assert_eq!(
        durable["terminal_evidence"]["cost_evidence"],
        "conservative_reservation"
    );
    let replayed = recovered_failure_store
        .finalize_product_task_after_execution(task_id, "recovery-owner")
        .unwrap();
    assert!(replayed["delegated_terminal"].is_null());
    assert_eq!(
        recovered_failure_store
            .delegated_authority_state(&delegation.delegation_id)
            .unwrap(),
        durable
    );
    assert_eq!(unknown_transport.sends.load(Ordering::SeqCst), 1);

    let cancelled_db = dir.path().join("delegated-late-response-cancelled.db");
    store
        .with_conn(|connection| {
            connection
                .execute(
                    "VACUUM INTO ?1",
                    params![cancelled_db.to_string_lossy().as_ref()],
                )
                .map(|_| ())
                .map_err(|error| error.to_string())
        })
        .unwrap();
    let cancelled_workspace = dir.path().join("delegated-late-response-workspace");
    std::fs::create_dir_all(cancelled_workspace.join("docs")).unwrap();
    std::fs::copy(
        PathBuf::from(&activated_workspace_path).join("docs/USER_GUIDE.md"),
        cancelled_workspace.join("docs/USER_GUIDE.md"),
    )
    .unwrap();
    let cancelled_workspace = std::fs::canonicalize(cancelled_workspace).unwrap();
    let cancelled_store = Arc::new(
        LocalProductStore::new_with_clock(&cancelled_db, || "2026-07-25T12:00:00Z".to_string())
            .unwrap(),
    );
    let mut cancelled_binding =
        cancelled_store.get_product_task(task_id).unwrap().unwrap()["workspace_binding"].clone();
    cancelled_binding["workspace_path"] = json!(cancelled_workspace);
    cancelled_binding["workspace_root"] = json!(cancelled_workspace);
    cancelled_store
        .with_conn(|connection| {
            let workspace_json: String = connection
                .query_row(
                    "SELECT workspace_json FROM supervised_patch_workspaces
                         WHERE workspace_id=?1",
                    params![workspace_record_id],
                    |row| row.get(0),
                )
                .map_err(|error| error.to_string())?;
            let mut workspace_json: Value =
                serde_json::from_str(&workspace_json).map_err(|error| error.to_string())?;
            workspace_json["workspace_mode"] = json!("copy");
            workspace_json["workspace_path"] = json!(cancelled_workspace);
            connection
                .execute(
                    "UPDATE product_tasks SET workspace_binding_json=?1
                         WHERE task_id=?2",
                    params![cancelled_binding.to_string(), task_id],
                )
                .map_err(|error| error.to_string())?;
            connection
                .execute(
                    "UPDATE supervised_patch_workspaces
                         SET workspace_path=?1, workspace_canonical_path=?1,
                             workspace_json=?2
                         WHERE workspace_id=?3",
                    params![
                        cancelled_workspace.to_string_lossy().as_ref(),
                        workspace_json.to_string(),
                        workspace_record_id
                    ],
                )
                .map_err(|error| error.to_string())?;
            Ok(())
        })
        .unwrap();
    let cancelled_preimage = std::fs::read(cancelled_workspace.join("docs/USER_GUIDE.md")).unwrap();
    let implementation_entered = Arc::new(Barrier::new(2));
    let implementation_release = Arc::new(Barrier::new(2));
    let blocked_transport = Arc::new(DelegatedBlockedImplementationTransport {
        responses: Mutex::new(vec![
            delegated_openai_response(
                "blocked-plan-1",
                "deepseek-v4-pro",
                json!({
                    "schema_version": "managed_deepseek_plan.v1",
                    "status": "planned",
                    "path": "docs/USER_GUIDE.md",
                    "intent": "clarify_doctor_read_only_health_check"
                }),
            ),
            delegated_openai_response(
                "blocked-implementation-1",
                "deepseek-v4-flash",
                json!({
                    "schema_version": "managed_workspace_action.v1",
                    "action": "replace_text",
                    "path": "docs/USER_GUIDE.md",
                    "old_text": "`alters-lab doctor` checks health.",
                    "new_text": "`alters-lab doctor` performs a read-only health check."
                }),
            ),
        ]),
        sends: AtomicUsize::new(0),
        implementation_entered: Arc::clone(&implementation_entered),
        implementation_release: Arc::clone(&implementation_release),
    });
    let blocked_transport_trait: Arc<dyn HttpTransport> = blocked_transport.clone();
    let blocked_source: Arc<dyn crate::provider::managed_deepseek::ManagedAuthoritySource> =
        cancelled_store.clone();
    let blocked_executor = ManagedDeepSeekNodeExecutor::new(
        delegated_mock_provider("deepseek-v4-pro", blocked_transport_trait.clone()),
        delegated_mock_provider("deepseek-v4-flash", blocked_transport_trait.clone()),
        delegated_mock_provider("deepseek-v4-pro", blocked_transport_trait),
        blocked_source,
        ManagedDeepSeekExecutorConfig {
            protocol: DeepSeekProtocol::OpenAiCompatible,
            limits: ManagedCallLimits {
                max_requests: 3,
                max_retries: 0,
                max_input_tokens: 8_000,
                max_output_tokens: 4_000,
                max_cumulative_tokens: 24_000,
                timeout_ms: 30_000,
                max_cost_usd: Some(0.50),
            },
            price_profile: DeepSeekPriceProfile::default(),
        },
    )
    .unwrap();
    let cancelled_run_id = activated["run"]["run_id"].as_str().unwrap().to_string();
    cancelled_store
        .tick_with_executor(&cancelled_run_id, "executor", 0, &blocked_executor)
        .unwrap();
    assert_eq!(blocked_transport.sends.load(Ordering::SeqCst), 1);
    let execution_store = Arc::clone(&cancelled_store);
    let execution_run_id = cancelled_run_id.clone();
    let late_response = thread::spawn(move || {
        execution_store.tick_with_executor(&execution_run_id, "executor", 0, &blocked_executor)
    });
    implementation_entered.wait();
    let cancelled = cancelled_store
        .request_workflow_run_cancel(
            &cancelled_run_id,
            "cancellation-authority",
            Some("deterministic late-response boundary"),
        )
        .unwrap();
    assert_eq!(cancelled["status"], "cancelled");
    implementation_release.wait();
    let late_result = late_response.join().unwrap().unwrap();
    assert_eq!(late_result["result"]["status"], "failed");
    assert!(late_result["result"]["error_message"]
        .as_str()
        .is_some_and(|message| message.contains("stale") || message.contains("cancelled")));
    assert_eq!(blocked_transport.sends.load(Ordering::SeqCst), 2);
    assert_eq!(
        std::fs::read(cancelled_workspace.join("docs/USER_GUIDE.md")).unwrap(),
        cancelled_preimage,
        "a provider response returned after cancellation must not mutate the workspace"
    );
    let cancelled_journal: Vec<Value> = cancelled_store
        .with_conn(|connection| {
            connection
                .query_row(
                    "SELECT provider_request_journal_json
                         FROM managed_acceptance_delegations WHERE attempt_id=?1",
                    params!["attempt-golden-path-1"],
                    |row| row.get::<_, String>(0),
                )
                .map_err(|error| error.to_string())
        })
        .and_then(|encoded| serde_json::from_str(&encoded).map_err(|error| error.to_string()))
        .unwrap();
    let cancelled_implementation = cancelled_journal
        .iter()
        .find(|entry| entry.get("role").and_then(Value::as_str) == Some("implementer"))
        .unwrap();
    assert_eq!(cancelled_implementation["status"], "succeeded");
    assert_eq!(
        cancelled_store
            .get_workflow_run(&cancelled_run_id)
            .unwrap()
            .unwrap()["status"],
        "failed"
    );

    let transport = Arc::new(DelegatedRouteMockTransport {
        responses: Mutex::new(vec![
            delegated_openai_response(
                "mock-plan-1",
                "deepseek-v4-pro",
                json!({
                    "schema_version": "managed_deepseek_plan.v1",
                    "status": "planned",
                    "path": "docs/USER_GUIDE.md",
                    "intent": "clarify_doctor_read_only_health_check"
                }),
            ),
            delegated_openai_response(
                "mock-implementation-1",
                "deepseek-v4-flash",
                json!({
                    "schema_version": "managed_workspace_action.v1",
                    "action": "replace_text",
                    "path": "docs/USER_GUIDE.md",
                    "old_text": "`alters-lab doctor` checks health.",
                    "new_text": "`alters-lab doctor` performs a read-only health check."
                }),
            ),
            delegated_openai_response(
                "mock-review-1",
                "deepseek-v4-pro",
                json!({
                    "schema_version": "managed_deepseek_review.v1",
                    "status": "accepted",
                    "material_objections": []
                }),
            ),
        ]),
        sends: AtomicUsize::new(0),
    });
    let transport_trait: Arc<dyn HttpTransport> = transport.clone();
    let source: Arc<dyn crate::provider::managed_deepseek::ManagedAuthoritySource> = store.clone();
    let executor = ManagedDeepSeekNodeExecutor::new(
        delegated_mock_provider("deepseek-v4-pro", transport_trait.clone()),
        delegated_mock_provider("deepseek-v4-flash", transport_trait.clone()),
        delegated_mock_provider("deepseek-v4-pro", transport_trait),
        source,
        ManagedDeepSeekExecutorConfig {
            protocol: DeepSeekProtocol::OpenAiCompatible,
            limits: ManagedCallLimits {
                max_requests: 3,
                max_retries: 0,
                max_input_tokens: 8_000,
                max_output_tokens: 4_000,
                max_cumulative_tokens: 24_000,
                timeout_ms: 30_000,
                max_cost_usd: Some(0.50),
            },
            price_profile: DeepSeekPriceProfile::default(),
        },
    )
    .unwrap();
    let run_id = activated["run"]["run_id"].as_str().unwrap();
    let mut tick_results = Vec::new();
    for _ in 0..8 {
        let tick = store
            .tick_with_executor(run_id, "executor", 0, &executor)
            .unwrap();
        tick_results.push(tick.clone());
        if tick
            .pointer("/run/status")
            .and_then(Value::as_str)
            .is_some_and(|status| matches!(status, "completed" | "failed" | "cancelled" | "killed"))
        {
            break;
        }
    }
    let completed_run = store.get_workflow_run(run_id).unwrap().unwrap();
    assert_eq!(
        completed_run["status"], "completed",
        "run={completed_run:#} ticks={tick_results:#?}"
    );
    assert_eq!(transport.sends.load(Ordering::SeqCst), 3);
    let workspace_path = activated_workspace_path.as_str();
    let guide =
        std::fs::read_to_string(PathBuf::from(workspace_path).join("docs/USER_GUIDE.md")).unwrap();
    assert!(guide.contains("performs a read-only health check"));
    let finalized = store
        .finalize_product_task_after_execution(task_id, "executor")
        .unwrap();
    assert_eq!(finalized["task"]["status"], "awaiting_approval");
    let task_version = finalized["task"]["version"].as_u64().unwrap();
    let original_provider_journal = store
        .with_conn(|connection| {
            connection
                .query_row(
                    "SELECT provider_request_journal_json
                         FROM managed_acceptance_delegations WHERE attempt_id=?1",
                    params!["attempt-golden-path-1"],
                    |row| row.get::<_, String>(0),
                )
                .map_err(|error| error.to_string())
        })
        .unwrap();
    assert!(store
        .approve_delegated_product_task(
            &principal,
            task_id,
            "artifact-confirmer",
            task_version,
            &delegation.delegation_id,
            &manifest,
            &revision,
        )
        .unwrap_err()
        .contains("confirmer authority"));
    assert!(store
        .approve_delegated_product_task(
            &activator,
            task_id,
            "artifact-confirmer",
            task_version,
            &delegation.delegation_id,
            &manifest,
            &revision,
        )
        .unwrap_err()
        .contains("confirmer authority"));
    let mut tampered_provider_journal: Value =
        serde_json::from_str(&original_provider_journal).unwrap();
    tampered_provider_journal[0]["request_id"] = json!("tampered-request-id");
    store
        .with_conn(|connection| {
            connection
                .execute(
                    "UPDATE managed_acceptance_delegations
                         SET provider_request_journal_json=?1 WHERE attempt_id=?2",
                    params![
                        tampered_provider_journal.to_string(),
                        "attempt-golden-path-1"
                    ],
                )
                .map(|_| ())
                .map_err(|error| error.to_string())
        })
        .unwrap();
    assert!(store
        .approve_delegated_product_task(
            &delegated_test_principal("artifact-confirmer"),
            task_id,
            "artifact-confirmer",
            task_version,
            &delegation.delegation_id,
            &manifest,
            &revision,
        )
        .unwrap_err()
        .contains("journal"));
    store
        .with_conn(|connection| {
            connection
                .execute(
                    "UPDATE managed_acceptance_delegations
                         SET provider_request_journal_json=?1 WHERE attempt_id=?2",
                    params![original_provider_journal, "attempt-golden-path-1"],
                )
                .map(|_| ())
                .map_err(|error| error.to_string())
        })
        .unwrap();
    let delegated_approval = store
        .approve_delegated_product_task(
            &delegated_test_principal("artifact-confirmer"),
            task_id,
            "artifact-confirmer",
            task_version,
            &delegation.delegation_id,
            &manifest,
            &revision,
        )
        .unwrap();
    assert_eq!(
        delegated_approval["artifact_confirmation"]["schema_version"],
        "managed_delegated_artifact_confirmation.v1"
    );
    assert_eq!(delegated_approval["replayed"], false);
    let replayed_approval = store
        .approve_delegated_product_task(
            &delegated_test_principal("artifact-confirmer"),
            task_id,
            "artifact-confirmer",
            task_version,
            &delegation.delegation_id,
            &manifest,
            &revision,
        )
        .unwrap();
    assert_eq!(replayed_approval["replayed"], true);
    assert_eq!(
        replayed_approval["approval"]["approval_id"],
        delegated_approval["approval"]["approval_id"]
    );
    let output = store
        .output_product_task(
            task_id,
            "output-owner",
            task_version,
            delegated_approval["approval"]["approval_id"].as_str(),
            true,
        )
        .unwrap();
    assert_eq!(output["output"]["mode"], "draft_pr");
    assert_eq!(output["output"]["status"], "planned", "output={output:#}");
    assert_eq!(output["output"]["network_effect"], false);
    assert_eq!(output["output"]["target_repository"], "Igzela/alters-lab");
    assert!(output["output"]["head_branch"]
        .as_str()
        .is_some_and(|branch| branch.starts_with("acp/")));
    assert_eq!(
        output["output"]["operation"]["request"]["source_revision"],
        revision
    );
    assert_eq!(output["task"]["status"], "output_pending");
    assert_eq!(transport.sends.load(Ordering::SeqCst), 3);
    assert!(store
        .complete_delegated_product_task_terminal(
            &delegation.delegation_id,
            "attempt-golden-path-1",
            task_id,
            "cleanup-owner",
        )
        .is_err());
    assert!(PathBuf::from(workspace_path).exists());
    let target_main_after = String::from_utf8(
        Command::new("git")
            .args(["rev-parse", "main"])
            .current_dir(&repo)
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap()
    .trim()
    .to_string();
    assert_eq!(target_main_after, revision);
    let workspace_record_id = finalized["task"]["workspace_record_id"].as_str().unwrap();
    let cleaned = store
        .cleanup_workspace(workspace_record_id, "cleanup-owner")
        .unwrap();
    assert_eq!(cleaned["status"], "cleaned");
    assert!(!PathBuf::from(workspace_path).exists());
    let terminal_receipt = json!({
        "schema_version": "managed_delegated_terminal_evidence.v1",
        "product_task_id": task_id,
        "workflow_run_id": run_id,
        "manifest_sha256": manifest["manifest_sha256"],
        "artifact_confirmation_sha256": delegated_approval["artifact_confirmation"]["artifact_confirmation_sha256"],
        "output_mode": output["output"]["mode"],
        "output_status": output["output"]["status"],
        "provider_requests": transport.sends.load(Ordering::SeqCst),
        "cleanup_status": cleaned["status"],
        "target_main_sha": target_main_after,
    });
    let realized_cost_usd = delegated_approval["realized_cost_usd"].as_f64().unwrap();
    let terminal = store
        .complete_delegated_attempt(
            &delegation.delegation_id,
            "attempt-golden-path-1",
            lease["attempt_lease_token"].as_str().unwrap(),
            "succeeded",
            &terminal_receipt,
            realized_cost_usd,
        )
        .unwrap();
    assert_eq!(terminal["status"], "closed");
    assert_eq!(terminal["terminal_class"], "succeeded");
    assert_eq!(terminal["spend_authorization_state"], "expired");
    assert_eq!(terminal["attempt_lease_state"], "closed");
    assert_eq!(terminal["delegation_state"], "expired");
    assert_eq!(
        store
            .complete_delegated_attempt(
                &delegation.delegation_id,
                "attempt-golden-path-1",
                lease["attempt_lease_token"].as_str().unwrap(),
                "succeeded",
                &terminal_receipt,
                realized_cost_usd,
            )
            .unwrap()["replayed"],
        true
    );
}

#[test]
fn bootstrap_rebinds_only_unadmitted_delegation_and_is_idempotent() {
    struct ProductGateGuard(Option<std::ffi::OsString>);

    impl Drop for ProductGateGuard {
        fn drop(&mut self) {
            match self.0.take() {
                Some(value) => std::env::set_var(PRODUCT_TASK_GATE, value),
                None => std::env::remove_var(PRODUCT_TASK_GATE),
            }
        }
    }

    let _env_lock = crate::cli::config::cli_env_test_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let prior_gate = std::env::var_os(PRODUCT_TASK_GATE);
    std::env::set_var(PRODUCT_TASK_GATE, "1");
    let _gate = ProductGateGuard(prior_gate);

    let (dir, store) = store();
    let target = dir.path().join("target");
    std::fs::create_dir_all(target.join("docs")).unwrap();
    std::fs::write(target.join("docs/USER_GUIDE.md"), "guide\n").unwrap();
    let intake = ProductTaskIntakeRequest {
        objective: "Create a bounded local artifact".into(),
        target_id: "bootstrap-recovery".into(),
        target_repo_path: target.to_string_lossy().into_owned(),
        source_kind: Some("local_folder".into()),
        source_revision: "unused-local-folder-revision".into(),
        source_tree_hash: None,
        allowed_paths: vec!["docs/USER_GUIDE.md".into()],
        verification_commands: vec![ProductVerificationCommand {
            command: "test -f docs/USER_GUIDE.md".into(),
            timeout_ms: 5_000,
        }],
        output_intent: "artifact_only".into(),
        executor_policy: ProductExecutorPolicy {
            allowed_executors: vec!["deterministic".into()],
            prefer: Some("deterministic".into()),
        },
        budget: None,
        risk_class: "low".into(),
        approval_required: true,
        confirm_execution: Some(true),
        confirm_output: Some(true),
        idempotency_key: "bootstrap-recovery-rebind".into(),
        expected_version: None,
        tenant_id: Some("local".into()),
        workspace_id: Some("default".into()),
        workspace_mode: Some("local_folder".into()),
        matrix_binding: None,
    };
    let validated = validate_intake(&intake, "local", "default").unwrap();
    let task = store.admit_product_task(&validated, "operator").unwrap();
    let task_id = task["task_id"].as_str().unwrap();
    let task_version_before = task["version"].as_i64().unwrap();

    seed_key(&store, "local", "bootstrap-reconcile-operator");
    let principal = store
        .authenticate_managed_acceptance_principal(
            "local",
            "bootstrap-reconcile-operator",
            Some(1.0),
        )
        .unwrap();
    let bootstrap_scopes = vec![SCOPE_IDENTITY_DELEGATE.to_string()];
    let over_scoped_bootstrap = vec![
        SCOPE_IDENTITY_DELEGATE.to_string(),
        SCOPE_RISK_ACKNOWLEDGE.to_string(),
    ];
    store
        .record_api_key_metadata_for_tenant(
            "local",
            LOCAL_BOOTSTRAP_API_KEY_ID,
            "local-admin",
            "admin",
            &over_scoped_bootstrap,
            "test-bootstrap",
        )
        .unwrap();
    assert!(store
        .authenticate_bootstrap_identity_delegation_principal("local", Some(1.0))
        .is_err());
    store
        .record_api_key_metadata_for_tenant(
            "local",
            LOCAL_BOOTSTRAP_API_KEY_ID,
            "local-admin",
            "admin",
            &bootstrap_scopes,
            "test-bootstrap",
        )
        .unwrap();
    let bootstrap = store
        .authenticate_bootstrap_identity_delegation_principal("local", Some(1.0))
        .unwrap();
    let mut delegation = delegated_contract();
    delegation.delegation_id = "bootstrap-rebind-delegation".into();
    store
        .persist_delegation_for_product_task(&principal, task_id, &delegation)
        .unwrap();
    let reviewer_scopes = vec![
        SCOPE_RISK_ACKNOWLEDGE.to_string(),
        SCOPE_DELEGATED_AUTONOMY.to_string(),
        SCOPE_DELEGATED_MANIFEST_APPROVE.to_string(),
        SCOPE_SPEND_AUTHORIZE.to_string(),
    ];
    store
        .record_api_key_metadata_for_tenant(
            "local",
            "bootstrap-reconcile-reissued",
            "reviewer-user",
            "reviewer",
            &reviewer_scopes,
            "test-bootstrap",
        )
        .unwrap();
    let reissued_reviewer = store
        .authenticate_managed_acceptance_principal_for_tenant(
            "local",
            "bootstrap-reconcile-reissued",
            Some(1.0),
        )
        .unwrap();

    let rebound = store
        .rebind_unadmitted_delegation_for_bootstrap(
            &bootstrap,
            task_id,
            &delegation.delegation_id,
            &reissued_reviewer,
        )
        .unwrap();
    assert_eq!(rebound["status"], "active");
    assert_eq!(rebound["delegation_state"], "active");
    assert_eq!(rebound["operation_identity"], delegation.delegation_id);
    assert_eq!(rebound["product_task_id"], task_id);
    assert_eq!(
        rebound["reviewer_principal_id"],
        "bootstrap-reconcile-reissued"
    );
    let original_principal: String = store
        .with_conn(|conn| {
            conn.query_row(
                "SELECT principal_id FROM managed_acceptance_delegations WHERE delegation_id=?1",
                params![delegation.delegation_id],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())
        })
        .unwrap();
    assert_eq!(original_principal, "bootstrap-reconcile-operator");
    assert_eq!(rebound["replayed"], false);
    assert_eq!(
        store
            .delegated_authority_state(&delegation.delegation_id)
            .unwrap()["delegation_state"],
        "active"
    );
    assert_eq!(
        store.get_product_task(task_id).unwrap().unwrap()["version"],
        task_version_before
    );

    let audit_count_before_replay = store
            .with_conn(|conn| {
                conn.query_row(
                    "SELECT COUNT(*) FROM audit_log WHERE action='managed_acceptance.delegation_bootstrap_rebound' AND resource=?1",
                    params![delegation.delegation_id],
                    |row| row.get::<_, i64>(0),
                )
                .map_err(|error| error.to_string())
            })
            .unwrap();

    let replay = store
        .rebind_unadmitted_delegation_for_bootstrap(
            &bootstrap,
            task_id,
            &delegation.delegation_id,
            &reissued_reviewer,
        )
        .unwrap();
    assert_eq!(replay["replayed"], true);
    assert_eq!(
        store.get_product_task(task_id).unwrap().unwrap()["version"],
        task_version_before
    );
    let audit_count_after_replay = store
            .with_conn(|conn| {
                conn.query_row(
                    "SELECT COUNT(*) FROM audit_log WHERE action='managed_acceptance.delegation_bootstrap_rebound' AND resource=?1",
                    params![delegation.delegation_id],
                    |row| row.get::<_, i64>(0),
                )
                .map_err(|error| error.to_string())
            })
            .unwrap();
    assert_eq!(audit_count_after_replay, audit_count_before_replay);

    store
        .record_api_key_metadata_for_tenant(
            "local",
            "bootstrap-reconcile-second-reviewer",
            "second-reviewer-user",
            "reviewer",
            &reviewer_scopes,
            "test-bootstrap",
        )
        .unwrap();
    let second_reviewer = store
        .authenticate_managed_acceptance_principal_for_tenant(
            "local",
            "bootstrap-reconcile-second-reviewer",
            Some(1.0),
        )
        .unwrap();
    assert!(store
        .rebind_unadmitted_delegation_for_bootstrap(
            &bootstrap,
            task_id,
            &delegation.delegation_id,
            &second_reviewer,
        )
        .is_err());
    let mut foreign_intake = intake;
    foreign_intake.idempotency_key = "bootstrap-rebind-foreign-task".into();
    let foreign_task = store
        .admit_product_task(
            &validate_intake(&foreign_intake, "local", "default").unwrap(),
            "operator",
        )
        .unwrap();
    let foreign_task_id = foreign_task["task_id"].as_str().unwrap();
    assert_ne!(foreign_task_id, task_id);
    assert!(store
        .rebind_unadmitted_delegation_for_bootstrap(
            &bootstrap,
            foreign_task_id,
            &delegation.delegation_id,
            &reissued_reviewer,
        )
        .is_err());
    assert!(store
        .authenticate_bootstrap_identity_delegation_principal("foreign-tenant", Some(1.0))
        .is_err());

    store
        .record_api_key_metadata_with_expiry_for_tenant(
            "foreign",
            "bootstrap-reconcile-foreign-reviewer",
            "foreign-reviewer-user",
            "reviewer",
            &reviewer_scopes,
            None,
            "test-bootstrap",
        )
        .unwrap();
    let foreign_reviewer = store
        .authenticate_managed_acceptance_principal(
            "foreign",
            "bootstrap-reconcile-foreign-reviewer",
            Some(1.0),
        )
        .unwrap();
    assert!(store
        .rebind_unadmitted_delegation_for_bootstrap(
            &bootstrap,
            task_id,
            &delegation.delegation_id,
            &foreign_reviewer,
        )
        .is_err());

    store
        .record_api_key_metadata_for_tenant(
            "local",
            "bootstrap-reconcile-operator-profile",
            "operator-profile-user",
            "operator",
            &ALL_MANAGED_ACCEPTANCE_SCOPES
                .iter()
                .map(|scope| (*scope).to_string())
                .collect::<Vec<_>>(),
            "test-bootstrap",
        )
        .unwrap();
    let operator_profile = store
        .authenticate_managed_acceptance_principal_for_tenant(
            "local",
            "bootstrap-reconcile-operator-profile",
            Some(1.0),
        )
        .unwrap();
    assert!(store
        .rebind_unadmitted_delegation_for_bootstrap(
            &bootstrap,
            task_id,
            &delegation.delegation_id,
            &operator_profile,
        )
        .is_err());

    let mut unbound = delegated_contract();
    unbound.delegation_id = "bootstrap-rebind-unbound".into();
    store.persist_delegation(&principal, &unbound).unwrap();
    assert!(store
        .rebind_unadmitted_delegation_for_bootstrap(
            &bootstrap,
            task_id,
            &unbound.delegation_id,
            &principal,
        )
        .is_err());

    let mut revoked = delegated_contract();
    revoked.delegation_id = "bootstrap-rebind-revoked".into();
    store
        .persist_delegation_for_product_task(&principal, task_id, &revoked)
        .unwrap();
    store
        .revoke_delegation(&principal, &revoked.delegation_id)
        .unwrap();
    assert!(store
        .rebind_unadmitted_delegation_for_bootstrap(
            &bootstrap,
            task_id,
            &revoked.delegation_id,
            &principal,
        )
        .is_err());
}

#[test]
fn legacy_proposal_without_provider_binding_derives_and_validates_successfully() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("legacy.db");
    let store = LocalProductStore::new_with_clock(&path, || "2026-07-25T12:00:00Z".into()).unwrap();
    seed_key(&store, "tenant-a", "legacy-operator");
    let principal = store
        .authenticate_managed_acceptance_principal("tenant-a", "legacy-operator", Some(1.0))
        .unwrap();
    let delegation = delegated_contract();
    store.persist_delegation(&principal, &delegation).unwrap();
    store
        .with_conn(|connection| {
            connection
                .execute(
                    "INSERT INTO product_tasks (
                            task_id, schema_version, tenant_id, workspace_id, idempotency_key,
                            status, version, objective_fingerprint, target_id, target_repo_path,
                            source_revision, source_tree_hash, output_intent, risk_class,
                            approval_required, confirm_execution, confirm_output,
                            intake_contract_sha256, intake_json, workspace_binding_json,
                            plan_id, run_id, workspace_record_id, failure_code, failure_detail,
                            created_at, updated_at, created_by
                         ) VALUES (
                            ?1, 'product_task.v1', 'tenant-a', 'default',
                            'legacy-proposal-test', 'graph_ready', 1, ?2,
                            'alters-lab-docs', '/redacted/target', ?3, NULL, 'draft_pr', 'low',
                            1, 1, 1, ?4, '{}', ?5, NULL, 'run-legacy-1',
                            NULL, NULL, NULL, ?6, ?6, 'test'
                         )",
                    params![
                        "product-task-golden-path-1",
                        "a".repeat(64),
                        "b".repeat(40),
                        "c".repeat(64),
                        json!({
                            "allowed_paths": ["docs/USER_GUIDE.md"],
                            "workspace_path": "/redacted/not-used"
                        })
                        .to_string(),
                        "2026-07-25T12:00:00Z"
                    ],
                )
                .map(|_| ())
                .map_err(|error| error.to_string())
        })
        .unwrap();
    store
        .bind_delegation_to_product_task(
            &principal,
            "product-task-golden-path-1",
            &delegation.delegation_id,
        )
        .unwrap();
    // Legacy proposal without provider_execution_binding:
    let mut legacy_proposal = json!({
        "schema_version": "managed_proposal_manifest.v1",
        "target_repository": "Igzela/alters-lab",
        "target_main_sha": "6".repeat(40),
        "mutable_paths": ["docs/USER_GUIDE.md"],
        "max_cost_usd": null,
        "verifier": "deterministic_docs_health_check_v1"
    });
    legacy_proposal["manifest_sha256"] =
        json!(compute_attempt_manifest_sha256(&legacy_proposal).unwrap());
    store
        .persist_approved_delegated_proposal(
            &delegation.delegation_id,
            &legacy_proposal,
            legacy_proposal["manifest_sha256"].as_str().unwrap(),
        )
        .unwrap();

    let manifest =
        derive_final_execution_manifest(&legacy_proposal, &delegation, &delegated_execution())
            .unwrap();
    let approval = store
        .approve_delegated_manifest(&principal, &delegation.delegation_id, &manifest)
        .unwrap();
    let spend = store
        .issue_delegated_spend(
            &principal,
            &delegation.delegation_id,
            approval["approval_receipt_sha256"].as_str().unwrap(),
            &manifest,
        )
        .unwrap();
    assert_eq!(spend["status"], "active");

    // Validate contract derivation:
    let contract = delegated_execution_contract(&manifest, "planning").unwrap();
    assert_eq!(contract.requested_model, "deepseek-v4-pro");
    let canonical = crate::rwe::campaign_package::canonical_deepseek_provider_binding();
    assert_eq!(contract.base_url, canonical.base_url);
    assert_eq!(contract.endpoint_path, canonical.endpoint_path);
}
