//! Provider-free RWE runner: store-owned authorization, admit, fixture dispatch, evidence.

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::corpus::{freeze_first_rwe_corpus, FirstRweCorpus, RWE_CORPUS_SCHEMA};
use crate::storage::local_product_store::validate_rwe_corpus_envelope;
use crate::storage::local_product_store::{
    AuthenticatedPrincipal, CostAuthority, LocalProductStore, RweAuthorizationIssueRequest,
    RwePerTaskBudget, SCOPE_SPEND_AUTHORIZE,
};

pub const RWE_RUN_AUTH_SCHEMA: &str = "rwe_run_authorization.v1";
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
            expires_at: body.expires_at.clone(),
            fixture_only,
        },
    )
}

/// Provider-free runner: admit run, dispatch fixture tasks, persist evidence.
/// Never labels fixture completion as a live RWE baseline.
pub fn run_provider_free_rwe(
    store: &LocalProductStore,
    principal: &AuthenticatedPrincipal,
    run_id: &str,
    authorization_id: &str,
    allow_fixture: bool,
) -> Result<Value, String> {
    let corpus = freeze_first_rwe_corpus()?;
    let now = store.now();
    let auth = store
        .get_rwe_run_authorization(authorization_id)?
        .ok_or_else(|| "RWE authorization not found".to_string())?;
    let auth_body = auth.get("body_json").cloned().unwrap_or(Value::Null);
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
        "provider_free_fixture": allow_fixture,
    }));

    // Exact run replay compares complete run body via store admit.
    if allow_fixture {
        if auth.get("corpus_sha256").and_then(Value::as_str) != Some(corpus.corpus_sha256.as_str())
        {
            return Err("corpus mismatch".into());
        }
    } else {
        match evaluate_rwe_live_gate_from_store(&corpus, Some(&auth), &now) {
            RweLiveGateResult::ReadyAuthorized => {}
            other => return Err(format!("live gate blocked: {}", other.as_str())),
        }
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
    use crate::storage::local_product_store::ALL_MANAGED_ACCEPTANCE_SCOPES;
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
}
