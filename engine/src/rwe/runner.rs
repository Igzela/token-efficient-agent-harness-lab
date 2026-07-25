//! Provider-free RWE runner: store-owned authorization, admit, fixture dispatch, evidence.

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::corpus::{freeze_first_rwe_corpus, FirstRweCorpus, RWE_CORPUS_SCHEMA};
use crate::storage::local_product_store::{
    AuthenticatedPrincipal, CostAuthority, LocalProductStore, SCOPE_SPEND_AUTHORIZE,
};

pub const RWE_RUN_AUTH_SCHEMA: &str = "rwe_run_authorization.v1";
pub const RWE_RUN_EVIDENCE_SCHEMA: &str = "rwe_run_evidence.v1";

/// Canonical spend envelope identity for a multi-task RWE run (must be store-persisted).
#[derive(Debug, Clone, PartialEq)]
pub struct RweRunAuthorizationBody {
    pub authorization_id: String,
    pub corpus_sha256: String,
    pub golden_path_terminal_evidence_id: String,
    pub principal_id: String,
    pub principal_kind: String,
    pub task_ids: Vec<String>,
    pub max_total_provider_requests: u64,
    pub max_total_tokens: u64,
    pub max_wall_time_ms: u64,
    pub cost_authority: CostAuthority,
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
            "golden_path_terminal_evidence_id": self.golden_path_terminal_evidence_id,
            "principal_id": self.principal_id,
            "principal_kind": self.principal_kind,
            "task_ids": self.task_ids,
            "max_total_provider_requests": self.max_total_provider_requests,
            "max_total_tokens": self.max_total_tokens,
            "max_wall_time_ms": self.max_wall_time_ms,
            "cost_authority": self.cost_authority.to_json(),
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
        if exp < now {
            return RweLiveGateResult::BlockedExpired;
        }
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

/// Persist a one-use RWE run authorization. Fixture principals may persist fixture-only rows
/// that can never pass the production live gate.
pub fn persist_rwe_run_authorization(
    store: &LocalProductStore,
    principal: &AuthenticatedPrincipal,
    body: &RweRunAuthorizationBody,
    fixture_only: bool,
) -> Result<Value, String> {
    if !principal.has_scope(SCOPE_SPEND_AUTHORIZE) {
        return Err("principal missing spend authorize scope".into());
    }
    if fixture_only
        != matches!(
            principal.principal_kind(),
            crate::storage::local_product_store::PrincipalKind::FixturePrincipal
        )
    {
        return Err("fixture_only mismatch with principal kind".into());
    }
    if !fixture_only && !principal.may_authorize_production_live_start() {
        return Err("principal cannot authorize production RWE spend".into());
    }
    let body_json = sort_value(&body.to_json());
    let body_sha = body.body_sha256();
    store.upsert_rwe_run_authorization(
        principal.tenant_id(),
        &body.authorization_id,
        principal.principal_id(),
        principal.principal_kind().as_str(),
        &body.corpus_sha256,
        &body_sha,
        &body_json,
        &body.expires_at,
        fixture_only,
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
    // Exact run replay must not require still-active (unconsumed) authorization.
    if let Some(existing) = store.get_rwe_run(run_id)? {
        let mut row = existing;
        if let Value::Object(ref mut m) = row {
            m.insert("idempotent_replay".into(), json!(true));
        }
        return Ok(row);
    }
    let auth = store
        .get_rwe_run_authorization(authorization_id)?
        .ok_or_else(|| "RWE authorization not found".to_string())?;
    let now = "2026-07-25T12:00:00Z";
    if allow_fixture {
        // Fixture path: still require active auth + corpus match, but fixture principal ok.
        if auth.get("corpus_sha256").and_then(Value::as_str) != Some(corpus.corpus_sha256.as_str())
        {
            return Err("corpus mismatch".into());
        }
        if auth.get("status").and_then(Value::as_str) != Some("active") {
            return Err("authorization not active".into());
        }
        if !auth
            .get("fixture_only")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            return Err("fixture runner requires fixture_only authorization".into());
        }
    } else {
        match evaluate_rwe_live_gate_from_store(&corpus, Some(&auth), now) {
            RweLiveGateResult::ReadyAuthorized => {}
            other => return Err(format!("live gate blocked: {}", other.as_str())),
        }
    }

    // Atomic admit + consume
    let admitted = store.admit_rwe_run(
        principal.tenant_id(),
        run_id,
        authorization_id,
        &corpus.corpus_sha256,
        principal.principal_id(),
    )?;
    if admitted
        .get("idempotent_replay")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Ok(admitted);
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
    store.complete_rwe_run(run_id, "fixture_complete", &aggregate, &evidence_sha)
}

/// Provider-free gate dossier for handoff (no secrets).
pub fn provider_free_rwe_readiness_dossier() -> Value {
    let corpus = freeze_first_rwe_corpus().expect("corpus fixtures present");
    json!({
        "schema_version": "rwe_provider_free_readiness.v1",
        "corpus_id": corpus.corpus_id,
        "corpus_sha256": corpus.corpus_sha256,
        "task_count": corpus.tasks.len(),
        "live_gate": evaluate_rwe_live_gate_from_store(&corpus, None, "2026-07-25T12:00:00Z").as_str(),
        "manual_gate": [
            "independent Board A (#299) approval",
            "live Golden Path terminal evidence",
            "store-owned non-fixture RweRunAuthorization with spend envelope",
            "parent-only provider credential",
            "exact disposable target + target-main SHA",
        ],
        "live_calls_performed": false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::local_product_store::ALL_MANAGED_ACCEPTANCE_SCOPES;
    use tempfile::tempdir;
    use uuid::Uuid;

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
        let body = RweRunAuthorizationBody {
            authorization_id: auth_id.clone(),
            corpus_sha256: corpus.corpus_sha256.clone(),
            golden_path_terminal_evidence_id: "gp-terminal-fixture".into(),
            principal_id: principal.principal_id().into(),
            principal_kind: principal.principal_kind().as_str().into(),
            task_ids: corpus.tasks.iter().map(|t| t.task_id.clone()).collect(),
            max_total_provider_requests: 5,
            max_total_tokens: 60_000,
            max_wall_time_ms: 900_000,
            cost_authority: CostAuthority::CostUnavailable,
            target_repo: "org/disposable".into(),
            target_main_sha: "a".repeat(40),
            executor_identity: "codex-0.145.0".into(),
            model_identity: "gpt-test-model".into(),
            draft_pr_only: true,
            expires_at: "2026-08-01T00:00:00Z".into(),
        };
        let _ = ALL_MANAGED_ACCEPTANCE_SCOPES;
        persist_rwe_run_authorization(&store, &principal, &body, true).unwrap();
        let run = run_provider_free_rwe(&store, &principal, "rwe-run-1", &auth_id, true).unwrap();
        assert_eq!(run["status"], "fixture_complete");
        assert_eq!(run["live_baseline_sealed"], false);
        assert_eq!(run["provider_free_fixture_completion"], true);
        // one-use consumed
        let auth = store.get_rwe_run_authorization(&auth_id).unwrap().unwrap();
        assert_eq!(auth["status"], "consumed");
        // exact replay
        let replay =
            run_provider_free_rwe(&store, &principal, "rwe-run-1", &auth_id, true).unwrap();
        assert_eq!(replay["idempotent_replay"], true);
    }
}
