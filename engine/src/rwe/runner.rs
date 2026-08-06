//! Provider-free RWE runner: store-owned authorization, admit, fixture dispatch, evidence.

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::corpus::{freeze_first_rwe_corpus, FirstRweCorpus, RWE_CORPUS_SCHEMA};
use super::operator_corpus::freeze_current_operator_contract_set;
use crate::storage::local_product_store::validate_rwe_corpus_envelope;
use crate::storage::local_product_store::{
    AuthenticatedPrincipal, CostAuthority, LocalProductStore, RweAuthorizationIssueRequest,
    RweAuthorizationV2IssueRequest, RwePerTaskBudget, SCOPE_SPEND_AUTHORIZE,
};

pub const RWE_RUN_AUTH_SCHEMA: &str = "rwe_run_authorization.v1";
pub const RWE_RUN_AUTH_V2_SCHEMA: &str = "rwe_run_authorization.v2";
pub const RWE_RUN_EVIDENCE_SCHEMA: &str = "rwe_run_evidence.v1";

/// Canonical spend envelope identity for a multi-task RWE run (must be store-persisted).
#[derive(Debug, Clone, PartialEq)]
pub struct RweRunAuthorizationBody {
    pub authorization_id: String,
    pub corpus_sha256: String,
    pub golden_path_product_task_id: String,
    pub principal_id: String,
    pub principal_kind: String,
    pub task_ids: Vec<String>,
    pub max_total_provider_requests: u64,
    pub max_total_tokens: u64,
    pub max_wall_time_ms: u64,
    pub cost_authority: CostAuthority,
    pub per_task_budgets: Vec<RwePerTaskBudget>,
    pub binary_path: String,
    pub binary_version: String,
    pub binary_sha256: String,
    pub provider_kind: String,
    pub provider_host: String,
    pub provider_base_url: String,
    pub target_repo: String,
    pub target_main_sha: String,
    pub executor_identity: String,
    pub model_identity: String,
    pub draft_pr_only: bool,
    pub admitted_executor: String,
    pub auto_merge_disabled: bool,
    pub expires_at: String,
}

impl RweRunAuthorizationBody {
    pub fn to_json(&self) -> Value {
        json!({
            "schema_version": RWE_RUN_AUTH_SCHEMA,
            "authorization_id": self.authorization_id,
            "corpus_sha256": self.corpus_sha256,
            "golden_path_product_task_id": self.golden_path_product_task_id,
            "principal_id": self.principal_id,
            "principal_kind": self.principal_kind,
            "task_ids": self.task_ids,
            "max_total_provider_requests": self.max_total_provider_requests,
            "max_total_tokens": self.max_total_tokens,
            "max_wall_time_ms": self.max_wall_time_ms,
            "cost_authority": self.cost_authority.to_json(),
            "per_task_budgets": self.per_task_budgets.iter().map(RwePerTaskBudget::to_json).collect::<Vec<_>>(),
            "binary_path": self.binary_path,
            "binary_version": self.binary_version,
            "binary_sha256": self.binary_sha256,
            "provider_kind": self.provider_kind,
            "provider_host": self.provider_host,
            "provider_base_url": self.provider_base_url,
            "target_repo": self.target_repo,
            "target_main_sha": self.target_main_sha,
            "executor_identity": self.executor_identity,
            "model_identity": self.model_identity,
            "draft_pr_only": self.draft_pr_only,
            "admitted_executor": self.admitted_executor,
            "auto_merge_disabled": self.auto_merge_disabled,
            "one_use": true,
            "expires_at": self.expires_at,
        })
    }

    pub fn body_sha256(&self) -> String {
        let sorted = sort_value(&self.to_json());
        hex::encode(Sha256::digest(sorted.to_string().as_bytes()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RweLiveGateResult {
    ReadyAuthorized,
    BlockedMissingRweSpendAuthorization,
    BlockedCorpusMismatch,
    BlockedPrincipal,
    BlockedExpired,
    BlockedConsumedOrRevoked,
}

impl RweLiveGateResult {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ReadyAuthorized => "ready_authorized",
            Self::BlockedMissingRweSpendAuthorization => "blocked_missing_rwe_spend_authorization",
            Self::BlockedCorpusMismatch => "blocked_corpus_mismatch",
            Self::BlockedPrincipal => "blocked_principal",
            Self::BlockedExpired => "blocked_expired",
            Self::BlockedConsumedOrRevoked => "blocked_consumed_or_revoked",
        }
    }
}

fn sort_value(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<_> = map.keys().cloned().collect();
            keys.sort();
            let mut out = serde_json::Map::new();
            for k in keys {
                if let Some(v) = map.get(&k) {
                    out.insert(k, sort_value(v));
                }
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(sort_value).collect()),
        other => other.clone(),
    }
}

/// Evaluate live gate against a store-loaded authorization row (not caller `active=true`).
pub fn evaluate_rwe_live_gate_from_store(
    corpus: &FirstRweCorpus,
    auth_row: Option<&Value>,
    now: &str,
) -> RweLiveGateResult {
    let Some(auth) = auth_row else {
        return RweLiveGateResult::BlockedMissingRweSpendAuthorization;
    };
    if auth.get("corpus_sha256").and_then(Value::as_str) != Some(corpus.corpus_sha256.as_str()) {
        return RweLiveGateResult::BlockedCorpusMismatch;
    }
    let status = auth.get("status").and_then(Value::as_str).unwrap_or("");
    if status == "consumed" || status == "revoked" {
        return RweLiveGateResult::BlockedConsumedOrRevoked;
    }
    if status != "active" {
        return RweLiveGateResult::BlockedMissingRweSpendAuthorization;
    }
    if let Some(exp) = auth.get("expires_at").and_then(Value::as_str) {
        let expired = chrono::DateTime::parse_from_rfc3339(exp)
            .ok()
            .zip(chrono::DateTime::parse_from_rfc3339(now).ok())
            .map(|(e, n)| e <= n)
            .unwrap_or(true);
        if expired {
            return RweLiveGateResult::BlockedExpired;
        }
    } else {
        return RweLiveGateResult::BlockedExpired;
    }
    let kind = auth
        .get("principal_kind")
        .and_then(Value::as_str)
        .unwrap_or("");
    if kind == "fixture_principal" {
        return RweLiveGateResult::BlockedPrincipal;
    }
    if kind != "operator_api_key" {
        return RweLiveGateResult::BlockedPrincipal;
    }
    RweLiveGateResult::ReadyAuthorized
}

/// Persist a one-use RWE run authorization via authenticated store owner.
/// Fixture principals may persist fixture-only rows that never pass the production live gate.
pub fn persist_rwe_run_authorization(
    store: &LocalProductStore,
    principal: &AuthenticatedPrincipal,
    body: &RweRunAuthorizationBody,
    fixture_only: bool,
) -> Result<Value, String> {
    if !principal.has_scope(SCOPE_SPEND_AUTHORIZE) {
        return Err("principal missing spend authorize scope".into());
    }
    store.issue_rwe_run_authorization(
        principal,
        &RweAuthorizationIssueRequest {
            authorization_id: body.authorization_id.clone(),
            corpus_sha256: body.corpus_sha256.clone(),
            golden_path_product_task_id: body.golden_path_product_task_id.clone(),
            task_ids: body.task_ids.clone(),
            max_total_provider_requests: body.max_total_provider_requests,
            max_total_tokens: body.max_total_tokens,
            max_wall_time_ms: body.max_wall_time_ms,
            cost_authority: body.cost_authority.clone(),
            per_task_budgets: body.per_task_budgets.clone(),
            binary_path: body.binary_path.clone(),
            binary_version: body.binary_version.clone(),
            binary_sha256: body.binary_sha256.clone(),
            provider_kind: body.provider_kind.clone(),
            provider_host: body.provider_host.clone(),
            provider_base_url: body.provider_base_url.clone(),
            target_repo: body.target_repo.clone(),
            target_main_sha: body.target_main_sha.clone(),
            executor_identity: body.executor_identity.clone(),
            model_identity: body.model_identity.clone(),
            draft_pr_only: body.draft_pr_only,
            admitted_executor: body.admitted_executor.clone(),
            auto_merge_disabled: body.auto_merge_disabled,
            expires_at: body.expires_at.clone(),
            fixture_only,
        },
    )
}

/// Issue production `rwe_run_authorization.v2` through the authenticated store owner.
/// Bindings are store-derived; callers supply only identity, GP prerequisite, and expiry.
pub fn persist_rwe_run_authorization_v2(
    store: &LocalProductStore,
    principal: &AuthenticatedPrincipal,
    request: &RweAuthorizationV2IssueRequest,
) -> Result<Value, String> {
    if !principal.has_scope(SCOPE_SPEND_AUTHORIZE) {
        return Err("principal missing spend authorize scope".into());
    }
    store.issue_rwe_run_authorization_v2(principal, request)
}

/// Canonical production real-RWE spend envelope body (`rwe_run_authorization.v2`).
/// Binds the frozen operator corpus/protocol/schedule artifacts and the
/// accepted-main SHA they were frozen at. `golden_path_prerequisite_product_task_id`
/// names the accepted live Golden Path prerequisite (a single accepted live-seal
/// receipt), never a per-cell terminal receipt.
#[derive(Debug, Clone, PartialEq)]
pub struct RweRunAuthorizationV2Body {
    pub authorization_id: String,
    pub accepted_main_sha: String,
    pub corpus_artifact_path: String,
    pub corpus_sha256: String,
    pub protocol_sha256: String,
    pub schedule_sha256: String,
    pub golden_path_prerequisite_product_task_id: String,
    pub principal_id: String,
    pub principal_kind: String,
    pub task_ids: Vec<String>,
    pub max_total_provider_requests: u64,
    pub max_total_tokens: u64,
    pub max_wall_time_ms: u64,
    pub cost_authority: CostAuthority,
    pub per_task_budgets: Vec<RwePerTaskBudget>,
    pub binary_path: String,
    pub binary_version: String,
    pub binary_sha256: String,
    pub provider_kind: String,
    pub provider_host: String,
    pub provider_base_url: String,
    pub provider_path: String,
    pub budget_point_ids: Vec<String>,
    pub target_repo: String,
    pub target_main_sha: String,
    pub executor_identity: String,
    pub model_identity: String,
    pub draft_pr_only: bool,
    pub admitted_executor: String,
    pub auto_merge_disabled: bool,
    pub expires_at: String,
}

impl RweRunAuthorizationV2Body {
    pub fn to_json(&self) -> Value {
        json!({
            "schema_version": RWE_RUN_AUTH_V2_SCHEMA,
            "authorization_id": self.authorization_id,
            "accepted_main_sha": self.accepted_main_sha,
            "corpus_artifact_path": self.corpus_artifact_path,
            "corpus_sha256": self.corpus_sha256,
            "protocol_sha256": self.protocol_sha256,
            "schedule_sha256": self.schedule_sha256,
            "golden_path_prerequisite_product_task_id": self.golden_path_prerequisite_product_task_id,
            "principal_id": self.principal_id,
            "principal_kind": self.principal_kind,
            "task_ids": self.task_ids,
            "max_total_provider_requests": self.max_total_provider_requests,
            "max_total_tokens": self.max_total_tokens,
            "max_wall_time_ms": self.max_wall_time_ms,
            "cost_authority": self.cost_authority.to_json(),
            "per_task_budgets": self.per_task_budgets.iter().map(RwePerTaskBudget::to_json).collect::<Vec<_>>(),
            "binary_path": self.binary_path,
            "binary_version": self.binary_version,
            "binary_sha256": self.binary_sha256,
            "provider_kind": self.provider_kind,
            "provider_host": self.provider_host,
            "provider_base_url": self.provider_base_url,
            "provider_path": self.provider_path,
            "budget_point_ids": self.budget_point_ids,
            "target_repo": self.target_repo,
            "target_main_sha": self.target_main_sha,
            "executor_identity": self.executor_identity,
            "model_identity": self.model_identity,
            "draft_pr_only": self.draft_pr_only,
            "admitted_executor": self.admitted_executor,
            "auto_merge_disabled": self.auto_merge_disabled,
            "one_use": true,
            "expires_at": self.expires_at,
        })
    }

    pub fn body_sha256(&self) -> String {
        let sorted = sort_value(&self.to_json());
        hex::encode(Sha256::digest(sorted.to_string().as_bytes()))
    }
}

/// Provider-free runner: admit run, dispatch fixture tasks, persist evidence.
/// Never labels fixture completion as a live RWE baseline.
///
/// Live path (`allow_fixture=false`) admits only store-owned `rwe_run_authorization.v2`
/// and returns after atomic one-use spend consumption. It does **not** dispatch the
/// 4-cell schedule, call a Provider, or write the target repository (Board B scope).
pub fn run_provider_free_rwe(
    store: &LocalProductStore,
    principal: &AuthenticatedPrincipal,
    run_id: &str,
    authorization_id: &str,
    allow_fixture: bool,
) -> Result<Value, String> {
    let auth = store
        .get_rwe_run_authorization(authorization_id)?
        .ok_or_else(|| "RWE authorization not found".to_string())?;
    let auth_body = auth.get("body_json").cloned().unwrap_or(Value::Null);
    let schema = auth_body
        .get("schema_version")
        .and_then(Value::as_str)
        .unwrap_or("");

    if !allow_fixture {
        // Production live path: require store-owned v2 spend envelope; admit only.
        if schema != RWE_RUN_AUTH_V2_SCHEMA {
            return Err(
                "fail closed: live RWE requires store-owned rwe_run_authorization.v2".into(),
            );
        }
        let frozen = freeze_current_operator_contract_set()?;
        if auth.get("corpus_sha256").and_then(Value::as_str)
            != Some(frozen.corpus.corpus_sha256.as_str())
        {
            return Err("corpus mismatch vs store-owned operator freeze".into());
        }
        // Run body is projected from the store-owned authorization body (not caller text).
        let mut run_body = auth_body.clone();
        if let Value::Object(ref mut m) = run_body {
            m.insert("run_id".into(), json!(run_id));
            m.insert("authorization_id".into(), json!(authorization_id));
            m.insert("provider_free_fixture".into(), json!(false));
            m.insert("schema_version".into(), json!("rwe_run_body.v2"));
        }
        let run_body = sort_value(&run_body);
        let mut admitted =
            store.admit_rwe_run(principal, run_id, authorization_id, &run_body, false)?;
        // Board B stops after admit: no cell dispatch, provider call, workspace, or target write.
        if let Value::Object(ref mut m) = admitted {
            m.insert("board_b_admit_only".into(), json!(true));
            m.insert("live_provider_request".into(), json!(false));
            m.insert("live_baseline_sealed".into(), json!(false));
            m.insert(
                "note".into(),
                json!("Board B production admit consumes one-use RWE spend; cell coordinator and live baseline are later packets"),
            );
        }
        return Ok(admitted);
    }

    // Fixture path (v1 only).
    let corpus = freeze_first_rwe_corpus()?;
    validate_rwe_corpus_envelope(&auth_body)?;

    let run_body = sort_value(&json!({
        "schema_version": "rwe_run_body.v1",
        "run_id": run_id,
        "authorization_id": authorization_id,
        "corpus_sha256": corpus.corpus_sha256,
        "task_ids": auth_body.get("task_ids"),
        "cost_authority": auth_body.get("cost_authority"),
        "per_task_budgets": auth_body.get("per_task_budgets"),
        "binary_path": auth_body.get("binary_path"),
        "binary_version": auth_body.get("binary_version"),
        "binary_sha256": auth_body.get("binary_sha256"),
        "provider_kind": auth_body.get("provider_kind"),
        "provider_host": auth_body.get("provider_host"),
        "provider_base_url": auth_body.get("provider_base_url"),
        "target_repo": auth_body.get("target_repo"),
        "target_main_sha": auth_body.get("target_main_sha"),
        "executor_identity": auth_body.get("executor_identity"),
        "model_identity": auth_body.get("model_identity"),
        "max_total_provider_requests": auth_body.get("max_total_provider_requests"),
        "max_total_tokens": auth_body.get("max_total_tokens"),
        "max_wall_time_ms": auth_body.get("max_wall_time_ms"),
        "golden_path_product_task_id": auth_body.get("golden_path_product_task_id"),
        "draft_pr_only": auth_body.get("draft_pr_only"),
        "admitted_executor": auth_body.get("admitted_executor"),
        "auto_merge_disabled": auth_body.get("auto_merge_disabled"),
        "provider_free_fixture": allow_fixture,
    }));

    if auth.get("corpus_sha256").and_then(Value::as_str) != Some(corpus.corpus_sha256.as_str()) {
        return Err("corpus mismatch".into());
    }

    let admitted = store.admit_rwe_run(
        principal,
        run_id,
        authorization_id,
        &run_body,
        allow_fixture,
    )?;
    if admitted
        .get("idempotent_replay")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Ok(admitted);
    }
    let lease_token = admitted
        .get("lease_token")
        .and_then(Value::as_str)
        .ok_or("admit missing lease_token")?
        .to_string();

    // The store has admitted the exact frozen corpus envelope; dispatch only that canonical
    // ordered task set, never a caller-declared subset or an independently reconstructed set.
    let authorized_task_ids = auth_body
        .get("task_ids")
        .and_then(Value::as_array)
        .ok_or("admitted RWE authorization missing canonical task_ids")?;
    if authorized_task_ids.len() != corpus.tasks.len()
        || authorized_task_ids
            .iter()
            .zip(&corpus.tasks)
            .any(|(id, task)| id.as_str() != Some(task.task_id.as_str()))
    {
        return Err("admitted RWE authorization does not cover canonical corpus".into());
    }
    let mut task_results = Vec::new();
    let mut total_requests = 0u64;
    for task in &corpus.tasks {
        let task_attempt_id = format!("{run_id}:{}", task.task_id);
        let classification = if task.class.contains("cancellation") {
            "controlled_cancel_fixture"
        } else {
            "fixture_success"
        };
        let evidence = json!({
            "schema_version": "rwe_task_attempt_evidence.v1",
            "task_attempt_id": task_attempt_id,
            "task_id": task.task_id,
            "definition_sha256": task.definition_sha256,
            "objective_sha256": task.objective_sha256,
            "provider_requests": 0,
            "input_tokens": 0,
            "output_tokens": 0,
            "total_tokens": 0,
            "cost_authority": CostAuthority::CostUnavailable.to_json(),
            "latency_ms": 0,
            "classification": classification,
            "live_provider_request": false,
            "fixture_dispatch": true,
        });
        store.persist_rwe_task_attempt(
            run_id,
            &lease_token,
            &task_attempt_id,
            &task.task_id,
            &task.definition_sha256,
            classification,
            &evidence,
        )?;
        total_requests += 0;
        task_results.push(evidence);
    }

    let aggregate = sort_value(&json!({
        "schema_version": RWE_RUN_EVIDENCE_SCHEMA,
        "run_id": run_id,
        "authorization_id": authorization_id,
        "corpus_sha256": corpus.corpus_sha256,
        "corpus_schema": RWE_CORPUS_SCHEMA,
        "task_results": task_results,
        "aggregate_provider_requests": total_requests,
        "live_provider_request": false,
        "live_baseline_sealed": false,
        "provider_free_fixture_completion": true,
        "note": "Fixture completion is not a live RWE baseline",
    }));
    let evidence_sha = hex::encode(Sha256::digest(aggregate.to_string().as_bytes()));
    store.complete_rwe_run(
        run_id,
        &lease_token,
        "fixture_complete",
        &aggregate,
        &evidence_sha,
    )
}

/// Provider-free gate dossier for handoff (no secrets).
pub fn provider_free_rwe_readiness_dossier() -> Value {
    let corpus = freeze_first_rwe_corpus().expect("corpus fixtures present");
    json!({
        "schema_version": "rwe_provider_free_readiness.v1",
        "corpus_id": corpus.corpus_id,
        "corpus_sha256": corpus.corpus_sha256,
        "task_count": corpus.tasks.len(),
        "live_gate": evaluate_rwe_live_gate_from_store(&corpus, None, "1970-01-01T00:00:00Z").as_str(),
        "manual_gate": [
            "independent Board A (#299) approval",
            "live Golden Path terminal evidence",
            "store-owned non-fixture RweRunAuthorization with spend envelope",
            "parent-only provider credential",
            "exact disposable target + target-main SHA",
        ],
        "live_calls_performed": false,
        "note": "live_gate timestamp is non-authoritative; production uses store.now()",
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::local_product_store::{
        ALL_MANAGED_ACCEPTANCE_SCOPES, SCOPE_ATTEMPT_ADMIT, SCOPE_RISK_ACKNOWLEDGE,
    };
    use tempfile::tempdir;
    use uuid::Uuid;

    fn fixture_auth_body(
        auth_id: &str,
        principal: &AuthenticatedPrincipal,
        corpus: &FirstRweCorpus,
        expires_at: &str,
    ) -> RweRunAuthorizationBody {
        let task_ids: Vec<String> = corpus.tasks.iter().map(|t| t.task_id.clone()).collect();
        let per_task_budgets = corpus
            .tasks
            .iter()
            .map(|t| RwePerTaskBudget::from_task_definition(t, None))
            .collect();
        RweRunAuthorizationBody {
            authorization_id: auth_id.into(),
            corpus_sha256: corpus.corpus_sha256.clone(),
            golden_path_product_task_id: "gp-terminal-fixture".into(),
            principal_id: principal.principal_id().into(),
            principal_kind: principal.principal_kind().as_str().into(),
            task_ids,
            max_total_provider_requests: 5,
            max_total_tokens: 60_000,
            max_wall_time_ms: 900_000,
            cost_authority: CostAuthority::CostUnavailable,
            per_task_budgets,
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
            expires_at: expires_at.into(),
        }
    }

    #[test]
    fn gate_blocks_without_store_auth_and_rejects_fixture_for_live() {
        let corpus = freeze_first_rwe_corpus().unwrap();
        assert_eq!(
            evaluate_rwe_live_gate_from_store(&corpus, None, "2026-07-25T12:00:00Z"),
            RweLiveGateResult::BlockedMissingRweSpendAuthorization
        );
        let fixture_row = json!({
            "status": "active",
            "corpus_sha256": corpus.corpus_sha256,
            "principal_kind": "fixture_principal",
            "expires_at": "2026-08-01T00:00:00Z",
        });
        assert_eq!(
            evaluate_rwe_live_gate_from_store(&corpus, Some(&fixture_row), "2026-07-25T12:00:00Z"),
            RweLiveGateResult::BlockedPrincipal
        );
    }

    #[test]
    fn provider_free_runner_fixture_path_persists_evidence_not_live_baseline() {
        let dir = tempdir().unwrap();
        let store = LocalProductStore::new_with_clock(dir.path().join("rwe.db"), || {
            "2026-07-25T12:00:00Z".into()
        })
        .unwrap();
        let principal =
            AuthenticatedPrincipal::fixture_for_tests("tenant-rwe", "fixture-principal-rwe")
                .unwrap();
        let corpus = freeze_first_rwe_corpus().unwrap();
        let auth_id = format!("rwe-auth-{}", Uuid::new_v4());
        let body = fixture_auth_body(&auth_id, &principal, &corpus, "2026-08-01T00:00:00Z");
        let _ = ALL_MANAGED_ACCEPTANCE_SCOPES;
        persist_rwe_run_authorization(&store, &principal, &body, true).unwrap();
        let run = run_provider_free_rwe(&store, &principal, "rwe-run-1", &auth_id, true).unwrap();
        assert_eq!(run["status"], "fixture_complete");
        assert_eq!(run["live_baseline_sealed"], false);
        assert_eq!(run["provider_free_fixture_completion"], true);
        // General read must not expose lease capability.
        let general = store.get_rwe_run("rwe-run-1").unwrap().unwrap();
        assert!(general.get("lease_token").is_none());
        // one-use consumed
        let auth = store.get_rwe_run_authorization(&auth_id).unwrap().unwrap();
        assert_eq!(auth["status"], "consumed");
        // exact replay
        let replay =
            run_provider_free_rwe(&store, &principal, "rwe-run-1", &auth_id, true).unwrap();
        assert_eq!(replay["idempotent_replay"], true);
        // conflicting task-attempt mutation rejected (terminal run + missing lease)
        let err = store.persist_rwe_task_attempt(
            "rwe-run-1",
            "not-a-lease",
            "rwe-run-1:small_test_addition",
            "small_test_addition",
            &corpus.tasks[0].definition_sha256,
            "mutated",
            &json!({"different": true}),
        );
        assert!(err.is_err());
    }

    #[test]
    fn rwe_authority_rejects_foreign_replay_and_requires_lease_before_attempt_replay() {
        let dir = tempdir().unwrap();
        let store = LocalProductStore::new_with_clock(dir.path().join("rwe-authority.db"), || {
            "2026-07-25T12:00:00Z".into()
        })
        .unwrap();
        let principal =
            AuthenticatedPrincipal::fixture_for_tests("tenant-rwe-auth", "fixture-principal-owner")
                .unwrap();
        let foreign = AuthenticatedPrincipal::fixture_for_tests(
            "tenant-rwe-foreign",
            "fixture-principal-foreign",
        )
        .unwrap();
        let corpus = freeze_first_rwe_corpus().unwrap();
        let auth_id = "rwe-authority-auth";
        let body = fixture_auth_body(auth_id, &principal, &corpus, "2026-08-01T00:00:00Z");
        persist_rwe_run_authorization(&store, &principal, &body, true).unwrap();
        let mut run_body = body.to_json();
        run_body["run_id"] = json!("rwe-authority-run");
        run_body["provider_free_fixture"] = json!(true);
        let admitted = store
            .admit_rwe_run(&principal, "rwe-authority-run", auth_id, &run_body, true)
            .unwrap();
        let lease = admitted["lease_token"].as_str().unwrap().to_string();

        let foreign_err = store
            .admit_rwe_run(&foreign, "rwe-authority-run", auth_id, &run_body, true)
            .unwrap_err();
        assert!(foreign_err.contains("tenant") || foreign_err.contains("principal"));
        assert!(store
            .admit_rwe_run(
                &principal,
                "rwe-authority-run",
                "wrong-authorization",
                &run_body,
                true,
            )
            .is_err());
        let mut conflicting_body = run_body.clone();
        conflicting_body["task_ids"] = json!([corpus.tasks[0].task_id]);
        assert!(store
            .admit_rwe_run(
                &principal,
                "rwe-authority-run",
                auth_id,
                &conflicting_body,
                true,
            )
            .is_err());

        let evidence = json!({"task_id": corpus.tasks[0].task_id, "replay": true});
        assert!(store
            .persist_rwe_task_attempt(
                "rwe-authority-run",
                "stale-lease",
                "rwe-authority-attempt",
                &corpus.tasks[0].task_id,
                &corpus.tasks[0].definition_sha256,
                "fixture_success",
                &evidence,
            )
            .is_err());
        let first = store
            .persist_rwe_task_attempt(
                "rwe-authority-run",
                &lease,
                "rwe-authority-attempt",
                &corpus.tasks[0].task_id,
                &corpus.tasks[0].definition_sha256,
                "fixture_success",
                &evidence,
            )
            .unwrap();
        assert_eq!(first["idempotent_replay"], false);
        assert!(store
            .persist_rwe_task_attempt(
                "rwe-authority-run",
                "stale-lease",
                "rwe-authority-attempt",
                &corpus.tasks[0].task_id,
                &corpus.tasks[0].definition_sha256,
                "fixture_success",
                &evidence,
            )
            .is_err());
        let replay = store
            .persist_rwe_task_attempt(
                "rwe-authority-run",
                &lease,
                "rwe-authority-attempt",
                &corpus.tasks[0].task_id,
                &corpus.tasks[0].definition_sha256,
                "fixture_success",
                &evidence,
            )
            .unwrap();
        assert_eq!(replay["idempotent_replay"], true);

        let replayed = store
            .admit_rwe_run(&principal, "rwe-authority-run", auth_id, &run_body, true)
            .unwrap();
        assert_eq!(replayed["idempotent_replay"], true);
        assert!(replayed.get("lease_token").is_none());
        assert!(store
            .get_rwe_run("rwe-authority-run")
            .unwrap()
            .unwrap()
            .get("lease_token")
            .is_none());
    }

    #[test]
    fn fixture_authorization_rejected_in_live_mode_without_consumption() {
        // Defense test: the runner rejects allow_fixture=false before admit and before
        // authorization consumption for ANY authorization, including fixture rows.
        // (The operator live-eligible regression is
        // live_eligible_authorization_fail_closed_before_admit_and_consumption.)
        let dir = tempdir().unwrap();
        let store = LocalProductStore::new_with_clock(dir.path().join("rwe-b0.db"), || {
            "2026-07-25T12:00:00Z".into()
        })
        .unwrap();
        let principal =
            AuthenticatedPrincipal::fixture_for_tests("tenant-rwe-b0", "fixture-principal-b0")
                .unwrap();
        let corpus = freeze_first_rwe_corpus().unwrap();
        let auth_id = format!("rwe-auth-{}", Uuid::new_v4());
        let body = fixture_auth_body(&auth_id, &principal, &corpus, "2026-08-01T00:00:00Z");
        persist_rwe_run_authorization(&store, &principal, &body, true).unwrap();

        // allow_fixture=false is rejected before admit and before consumption.
        let err =
            run_provider_free_rwe(&store, &principal, "rwe-run-live", &auth_id, false).unwrap_err();
        assert!(err.contains("fail closed"), "{err}");
        assert!(store.get_rwe_run("rwe-run-live").unwrap().is_none());
        let auth = store.get_rwe_run_authorization(&auth_id).unwrap().unwrap();
        assert_eq!(auth["status"], "active");
        // Repeated live-mode attempts stay fail-closed and never consume the authority.
        assert!(
            run_provider_free_rwe(&store, &principal, "rwe-run-live", &auth_id, false).is_err()
        );
        let auth = store.get_rwe_run_authorization(&auth_id).unwrap().unwrap();
        assert_eq!(auth["status"], "active");
        // No task-attempt row can exist: without an admitted run row the store attempt
        // owner rejects any attempt write (run + lease invariant), so a missing run row
        // provably implies zero attempts.
        assert!(store
            .persist_rwe_task_attempt(
                "rwe-run-live",
                "stale-lease",
                "rwe-run-live:no-attempt",
                &corpus.tasks[0].task_id,
                &corpus.tasks[0].definition_sha256,
                "fixture_success",
                &json!({"b0": true}),
            )
            .is_err());
        // Fixture path unchanged: the same authorization still admits and completes.
        let run =
            run_provider_free_rwe(&store, &principal, "rwe-run-fixture", &auth_id, true).unwrap();
        assert_eq!(run["status"], "fixture_complete");
        let auth = store.get_rwe_run_authorization(&auth_id).unwrap().unwrap();
        assert_eq!(auth["status"], "consumed");
    }

    #[test]
    fn live_eligible_authorization_fail_closed_before_admit_and_consumption() {
        // Real operator authentication through the store-owned API-key metadata owner,
        // not a fabricated principal. A live-eligible *v1* row (pre-Board-B contract)
        // must still fail closed before admit and consumption: live path requires
        // store-owned rwe_run_authorization.v2. Constructed via the store authorization
        // owner insert (test-only wrapper) for gate-eligible regression coverage.
        let dir = tempdir().unwrap();
        let store = LocalProductStore::new_with_clock(dir.path().join("rwe-b0-live.db"), || {
            "2026-07-25T12:00:00Z".into()
        })
        .unwrap();
        let operator_key_id = "operator-rwe-b0";
        let operator_scopes = vec![
            SCOPE_RISK_ACKNOWLEDGE.to_string(),
            SCOPE_SPEND_AUTHORIZE.to_string(),
            SCOPE_ATTEMPT_ADMIT.to_string(),
        ];
        store
            .record_api_key_metadata_for_tenant(
                "tenant-rwe-live",
                operator_key_id,
                "operator-user",
                "operator",
                &operator_scopes,
                "test-operator",
            )
            .unwrap();
        let principal = store
            .authenticate_managed_acceptance_principal("tenant-rwe-live", operator_key_id, None)
            .unwrap();
        assert_eq!(principal.principal_kind().as_str(), "operator_api_key");
        assert!(principal.may_authorize_production_live_start());

        let corpus = freeze_first_rwe_corpus().unwrap();
        let task_ids: Vec<String> = corpus.tasks.iter().map(|t| t.task_id.clone()).collect();
        let per_task_budgets = corpus
            .tasks
            .iter()
            .map(|t| RwePerTaskBudget::from_task_definition(t, None))
            .collect::<Vec<_>>();
        // The frozen corpus envelope bindings the issue path currently allows.
        let body = json!({
            "schema_version": RWE_RUN_AUTH_SCHEMA,
            "authorization_id": "b0-live-auth",
            "tenant_id": "tenant-rwe-live",
            "corpus_sha256": corpus.corpus_sha256,
            "golden_path_product_task_id": "gp-terminal-live",
            "principal_id": operator_key_id,
            "principal_kind": "operator_api_key",
            "task_ids": task_ids,
            "max_total_provider_requests": corpus.tasks.iter().map(|t| t.per_task_max_provider_requests).sum::<u64>(),
            "max_total_tokens": corpus.tasks.iter().map(|t| t.per_task_max_total_tokens).sum::<u64>(),
            "max_wall_time_ms": corpus.tasks.iter().map(|t| t.timeout_ms).sum::<u64>(),
            "cost_authority": CostAuthority::CostUnavailable.to_json(),
            "per_task_budgets": per_task_budgets.iter().map(RwePerTaskBudget::to_json).collect::<Vec<_>>(),
            "binary_path": "/usr/bin/codex",
            "binary_version": corpus.admitted_codex_version,
            "binary_sha256": "ab".repeat(32),
            "provider_kind": "openai_compatible",
            "provider_host": "api.openai.com",
            "provider_base_url": "https://api.openai.com/v1",
            "target_repo": "org/disposable",
            "target_main_sha": "a".repeat(40),
            "executor_identity": corpus.tasks[0].executor_identity,
            "model_identity": corpus.tasks[0].model_identity,
            "draft_pr_only": true,
            "admitted_executor": corpus.admitted_executor,
            "auto_merge_disabled": corpus.auto_merge_disabled,
            "one_use": true,
        });
        store
            .insert_rwe_run_authorization_for_tests(
                "tenant-rwe-live",
                "b0-live-auth",
                operator_key_id,
                "operator_api_key",
                &body,
                "2099-01-01T00:00:00Z",
                false,
            )
            .unwrap();

        // Precondition: this row is live-eligible (would pass the gate on base main).
        let auth = store
            .get_rwe_run_authorization("b0-live-auth")
            .unwrap()
            .unwrap();
        assert_eq!(auth["status"], "active");
        assert_eq!(auth["principal_kind"], "operator_api_key");
        assert_eq!(
            evaluate_rwe_live_gate_from_store(&corpus, Some(&auth), "2026-07-25T12:00:00Z"),
            RweLiveGateResult::ReadyAuthorized
        );

        // B0: v1 live-eligible authorization is rejected before admit/consumption.
        let err = run_provider_free_rwe(&store, &principal, "b0-run-live", "b0-live-auth", false)
            .unwrap_err();
        assert!(
            err.contains("fail closed") || err.contains("rwe_run_authorization.v2"),
            "{err}"
        );
        assert!(store.get_rwe_run("b0-run-live").unwrap().is_none());
        let auth = store
            .get_rwe_run_authorization("b0-live-auth")
            .unwrap()
            .unwrap();
        assert_eq!(auth["status"], "active");
        // No task-attempt row can exist: the store attempt owner rejects any write
        // without an admitted run row and lease (run + lease invariant).
        assert!(store
            .persist_rwe_task_attempt(
                "b0-run-live",
                "stale-lease",
                "b0-run-live:no-attempt",
                &corpus.tasks[0].task_id,
                &corpus.tasks[0].definition_sha256,
                "fixture_success",
                &json!({"b0": true}),
            )
            .is_err());
        // Repeated live attempts never consume the one-use authority.
        assert!(
            run_provider_free_rwe(&store, &principal, "b0-run-live", "b0-live-auth", false)
                .is_err()
        );
        let auth = store
            .get_rwe_run_authorization("b0-live-auth")
            .unwrap()
            .unwrap();
        assert_eq!(auth["status"], "active");
    }

    #[test]
    fn board_b_v2_production_issue_and_runner_admit_without_provider_or_target() {
        use crate::rwe::operator_corpus::OPERATOR_ARTIFACTS_FROZEN_AT_MAIN_SHA;
        use crate::storage::local_product_store::RweAuthorizationV2IssueRequest;
        use sha2::Digest;

        let dir = tempdir().unwrap();
        let store = LocalProductStore::new_with_clock(dir.path().join("rwe-board-b.db"), || {
            "2026-07-25T12:00:00Z".into()
        })
        .unwrap();
        let tenant = "tenant-board-b";
        let key_id = "operator-board-b";
        store
            .record_api_key_metadata_for_tenant(
                tenant,
                key_id,
                "operator-user",
                "operator",
                &[
                    SCOPE_RISK_ACKNOWLEDGE.to_string(),
                    SCOPE_SPEND_AUTHORIZE.to_string(),
                    SCOPE_ATTEMPT_ADMIT.to_string(),
                ],
                "test-operator",
            )
            .unwrap();
        let principal = store
            .authenticate_managed_acceptance_principal(tenant, key_id, None)
            .unwrap();
        let prereq = "ptask-board-b-gp";
        let mut evidence = json!({
            "schema_version": "product_task_terminal_evidence.v2",
            "evidence_id": "ev-board-b-gp",
            "product_task_id": prereq,
            "tenant_id": tenant,
            "workspace_scope_id": "ws-board-b",
            "task_version": 1,
            "task_status": "completed",
            "node": {"executor_class": "managed_coding"},
            "source_revision": "c".repeat(40),
            "verification": {"trustworthy": true, "status": "passed"},
            "approval": {"approval_id": "approval-board-b"},
            "artifact": {"artifact_id": "artifact-board-b"},
            "output": {
                "intent": "draft_pr",
                "result_sha256": "d".repeat(64),
                "operation_id": "op-board-b",
                "receipt_id": "rcpt-board-b",
                "draft_pr": {
                    "number": 5,
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
        let hash = hex::encode(sha2::Sha256::digest(serde_json::to_vec(&evidence).unwrap()));
        evidence["content_sha256"] = json!(hash);
        store
            .insert_product_task_terminal_evidence_for_tests(&evidence)
            .unwrap();

        let auth_id = "board-b-v2-auth";
        let issued = persist_rwe_run_authorization_v2(
            &store,
            &principal,
            &RweAuthorizationV2IssueRequest {
                authorization_id: auth_id.into(),
                golden_path_prerequisite_product_task_id: prereq.into(),
                expires_at: "2026-08-07T00:00:00Z".into(),
            },
        )
        .unwrap();
        assert_eq!(issued["status"], "active");
        assert_eq!(
            issued["body_json"]["accepted_main_sha"],
            OPERATOR_ARTIFACTS_FROZEN_AT_MAIN_SHA
        );

        let admitted =
            run_provider_free_rwe(&store, &principal, "board-b-run", auth_id, false).unwrap();
        assert_eq!(admitted["status"], "admitted");
        assert_eq!(admitted["board_b_admit_only"], true);
        assert_eq!(admitted["live_provider_request"], false);
        assert_eq!(admitted["live_baseline_sealed"], false);
        assert!(admitted.get("lease_token").is_some());
        let auth = store.get_rwe_run_authorization(auth_id).unwrap().unwrap();
        assert_eq!(auth["status"], "consumed");
        // Exact replay.
        let replay =
            run_provider_free_rwe(&store, &principal, "board-b-run", auth_id, false).unwrap();
        assert_eq!(replay["idempotent_replay"], true);
        // No cell/task-attempt rows created by Board B admit-only path.
        let frozen = freeze_current_operator_contract_set().unwrap();
        assert!(store
            .persist_rwe_task_attempt(
                "board-b-run",
                "stale-lease",
                "board-b-run:no-cell",
                &frozen.corpus.tasks[0].task_id,
                &frozen.corpus.tasks[0].definition_sha256,
                "should_fail",
                &json!({"board_b": true}),
            )
            .is_err());
    }

    #[test]
    fn revocation_and_expiry_block_admit() {
        let dir = tempdir().unwrap();
        let store = LocalProductStore::new_with_clock(dir.path().join("rwe2.db"), || {
            "2026-07-25T12:00:00Z".into()
        })
        .unwrap();
        let principal =
            AuthenticatedPrincipal::fixture_for_tests("tenant-rwe", "fixture-principal-rwe2")
                .unwrap();
        let corpus = freeze_first_rwe_corpus().unwrap();
        let auth_id = format!("rwe-auth-{}", Uuid::new_v4());
        let body = fixture_auth_body(&auth_id, &principal, &corpus, "2026-08-01T00:00:00Z");
        persist_rwe_run_authorization(&store, &principal, &body, true).unwrap();
        store
            .revoke_rwe_run_authorization(&principal, &auth_id)
            .unwrap();
        let err =
            run_provider_free_rwe(&store, &principal, "rwe-run-rev", &auth_id, true).unwrap_err();
        assert!(
            err.contains("not active") || err.contains("revok") || err.contains("expired"),
            "{err}"
        );
    }

    #[test]
    fn admit_rejects_admitted_executor_and_auto_merge_mutation() {
        let dir = tempdir().unwrap();
        let store = LocalProductStore::new_with_clock(dir.path().join("rwe-boundary.db"), || {
            "2026-07-25T12:00:00Z".into()
        })
        .unwrap();
        let principal =
            AuthenticatedPrincipal::fixture_for_tests("tenant-rwe-b", "fixture-principal-b")
                .unwrap();
        let corpus = freeze_first_rwe_corpus().unwrap();

        let body_ok = fixture_auth_body(
            "rwe-boundary-auth-ok",
            &principal,
            &corpus,
            "2026-08-01T00:00:00Z",
        );
        persist_rwe_run_authorization(&store, &principal, &body_ok, true).unwrap();
        let mut run_body_ok = body_ok.to_json();
        run_body_ok["run_id"] = json!("rwe-boundary-run-ok");
        run_body_ok["provider_free_fixture"] = json!(true);
        let admitted = store
            .admit_rwe_run(
                &principal,
                "rwe-boundary-run-ok",
                "rwe-boundary-auth-ok",
                &run_body_ok,
                true,
            )
            .unwrap();
        assert!(admitted.get("lease_token").is_some());

        let mut body_exec = fixture_auth_body(
            "rwe-boundary-auth-exec",
            &principal,
            &corpus,
            "2026-08-01T00:00:00Z",
        );
        body_exec.admitted_executor = "rogue-executor".into();
        assert!(persist_rwe_run_authorization(&store, &principal, &body_exec, true).is_err());

        let mut body_merge = fixture_auth_body(
            "rwe-boundary-auth-merge",
            &principal,
            &corpus,
            "2026-08-01T00:00:00Z",
        );
        body_merge.auto_merge_disabled = false;
        assert!(persist_rwe_run_authorization(&store, &principal, &body_merge, true).is_err());
    }
}
