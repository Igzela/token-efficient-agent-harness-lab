//! Store-owned RWE run authorization, admission, and evidence persistence.
//!
//! Authorization creation is authenticated-principal only. The raw body/hash upsert is
//! private; the owner recomputes the canonical body hash. Run admission revalidates the
//! complete authorization envelope. Task attempts and terminal receipts are
//! exact-replay-or-conflict (no UPSERT mutation). Terminalization requires the current
//! lease token.

use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::{append_audit_locked, AuthenticatedPrincipal, DatabaseConnection, LocalProductStore};
use crate::rwe::corpus::{freeze_first_rwe_corpus, RweTaskDefinition};

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
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

fn canonical_json(value: &Value) -> Result<String, String> {
    Ok(sort_value(value).to_string())
}

fn required_string_field<'a>(value: &'a Value, field: &str) -> Result<&'a str, String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("RWE corpus authorization missing non-empty {field}"))
}

fn required_u64_field(value: &Value, field: &str) -> Result<u64, String> {
    value
        .get(field)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("RWE corpus authorization missing numeric {field}"))
}

fn required_string_array_field(value: &Value, field: &str) -> Result<Vec<String>, String> {
    value
        .get(field)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .map(|v| {
                    v.as_str().map(str::to_string).ok_or_else(|| {
                        format!("RWE corpus authorization {field} must be a string array")
                    })
                })
                .collect::<Result<Vec<_>, _>>()
        })
        .unwrap_or_else(|| Err(format!("RWE corpus authorization missing array {field}")))
}

/// The frozen corpus is the authority for task order and per-task budgets. This is checked
/// at issue and admission so a caller cannot authorize a subset while the runner executes the
/// complete corpus.
pub(crate) fn validate_rwe_corpus_envelope(body: &Value) -> Result<(), String> {
    // Fixture-only authorization contract (v1). Production real-RWE must use
    // rwe_run_authorization.v2; a v2 body is never accepted by this envelope and
    // there is no silent v1<->v2 conversion.
    if body.get("schema_version").and_then(Value::as_str) != Some("rwe_run_authorization.v1") {
        return Err("RWE authorization must be rwe_run_authorization.v1".into());
    }
    let corpus = freeze_first_rwe_corpus()?;
    let corpus_sha256 = required_string_field(body, "corpus_sha256")?;
    if corpus_sha256 != corpus.corpus_sha256 {
        return Err("RWE authorization corpus_sha256 does not match frozen corpus".into());
    }
    let auth_admitted_executor = required_string_field(body, "admitted_executor")?;
    if auth_admitted_executor != corpus.admitted_executor {
        return Err("RWE authorization admitted_executor does not match frozen corpus".into());
    }
    let auth_auto_merge_disabled = body
        .get("auto_merge_disabled")
        .and_then(Value::as_bool)
        .ok_or("RWE authorization auto_merge_disabled required")?;
    if auth_auto_merge_disabled != corpus.auto_merge_disabled || !auth_auto_merge_disabled {
        return Err(
            "RWE authorization auto_merge_disabled must be true and match frozen corpus".into(),
        );
    }
    let task_ids = body
        .get("task_ids")
        .and_then(Value::as_array)
        .ok_or("RWE authorization task_ids array required")?;
    let canonical_task_ids = corpus
        .tasks
        .iter()
        .map(|task| task.task_id.as_str())
        .collect::<Vec<_>>();
    let observed_task_ids = task_ids
        .iter()
        .map(|id| {
            id.as_str()
                .ok_or("RWE authorization task_ids must contain strings")
        })
        .collect::<Result<Vec<_>, _>>()?;
    if observed_task_ids != canonical_task_ids {
        return Err("RWE authorization task_ids must exactly match frozen corpus order".into());
    }

    // Bind authorization executor/model/version to the frozen corpus identity.
    let auth_executor = required_string_field(body, "executor_identity")?;
    let auth_model = required_string_field(body, "model_identity")?;
    let auth_binary_version = required_string_field(body, "binary_version")?;
    if auth_binary_version != corpus.admitted_codex_version {
        return Err(
            "RWE authorization binary_version does not match frozen corpus admitted version".into(),
        );
    }
    for task in &corpus.tasks {
        if auth_executor != task.executor_identity || auth_model != task.model_identity {
            return Err(
                "RWE authorization executor/model identity does not match frozen corpus task identity"
                    .into(),
            );
        }
    }

    let cost_authority_value = body
        .get("cost_authority")
        .ok_or("RWE authorization cost_authority required")?;
    let cost_authority = super::CostAuthority::from_json(cost_authority_value)?;
    let cost_ceiling = match &cost_authority {
        super::CostAuthority::ProviderReported { max_cost, .. }
        | super::CostAuthority::LocalEstimate { max_cost, .. } => Some(*max_cost),
        super::CostAuthority::CostUnavailable => None,
    };

    let budgets = body
        .get("per_task_budgets")
        .and_then(Value::as_array)
        .ok_or("RWE authorization per_task_budgets array required")?;
    if budgets.len() != corpus.tasks.len() {
        return Err("RWE authorization must contain exactly one budget per corpus task".into());
    }
    let mut aggregate_requests = 0_u64;
    let mut aggregate_tokens = 0_u64;
    let mut aggregate_wall_ms = 0_u64;
    let mut aggregate_cost = 0.0_f64;
    for (budget, task) in budgets.iter().zip(&corpus.tasks) {
        if required_string_field(budget, "task_id")? != task.task_id {
            return Err("RWE per-task budgets must follow frozen corpus order".into());
        }
        let requests = required_u64_field(budget, "max_provider_requests")?;
        let input_tokens = required_u64_field(budget, "max_input_tokens")?;
        let output_tokens = required_u64_field(budget, "max_output_tokens")?;
        let total_tokens = required_u64_field(budget, "max_total_tokens")?;
        let wall_ms = required_u64_field(budget, "max_wall_time_ms")?;
        let retries = required_u64_field(budget, "max_retries")?;
        if requests != task.per_task_max_provider_requests
            || total_tokens != task.per_task_max_total_tokens
            || wall_ms != task.timeout_ms
            || input_tokens != task.per_task_max_input_tokens
            || output_tokens != task.per_task_max_output_tokens
            || input_tokens.saturating_add(output_tokens) != total_tokens
            || retries != task.per_task_max_retries
        {
            return Err(format!(
                "RWE budget does not exactly match frozen corpus task {}",
                task.task_id
            ));
        }
        if required_string_field(budget, "source_repository")? != task.source_repository
            || required_string_field(budget, "source_commit")? != task.source_commit
            || required_string_field(budget, "source_tree_hash")? != task.source_tree_hash
            || required_string_field(budget, "expected_outcome_class")?
                != task.expected_outcome_class
            || required_u64_field(budget, "patch_max_files")? != task.patch_max_files
            || required_u64_field(budget, "patch_max_lines")? != task.patch_max_lines
            || required_string_field(budget, "cancel_behavior")? != task.cancel_behavior
            || required_string_field(budget, "executor_identity")? != task.executor_identity
            || required_string_field(budget, "model_identity")? != task.model_identity
            || required_u64_field(budget, "deterministic_seed")? != task.deterministic_seed
        {
            return Err(format!(
                "RWE budget task-definition boundaries do not match frozen corpus task {}",
                task.task_id
            ));
        }
        if required_string_array_field(budget, "allowed_mutable_paths")?
            != task.allowed_mutable_paths
            || required_string_array_field(budget, "expected_verification_commands")?
                != task.expected_verification_commands
            || required_string_array_field(budget, "cleanup_rules")? != task.cleanup_rules
        {
            return Err(format!(
                "RWE budget task-definition arrays do not match frozen corpus task {}",
                task.task_id
            ));
        }
        match budget
            .get("max_cost")
            .and_then(|v| if v.is_null() { None } else { v.as_f64() })
        {
            Some(max_cost) => {
                if max_cost <= 0.0 {
                    return Err(format!(
                        "RWE budget max_cost must be positive for task {}",
                        task.task_id
                    ));
                }
                if cost_ceiling.is_none() {
                    return Err(format!(
                        "RWE budget max_cost present but cost_authority has no ceiling for task {}",
                        task.task_id
                    ));
                }
                aggregate_cost += max_cost;
            }
            None => {
                if cost_ceiling.is_some() {
                    return Err(format!(
                        "RWE budget missing max_cost for task {} but cost_authority has ceiling",
                        task.task_id
                    ));
                }
            }
        }
        aggregate_requests = aggregate_requests
            .checked_add(requests)
            .ok_or("RWE aggregate provider-request budget overflow")?;
        aggregate_tokens = aggregate_tokens
            .checked_add(total_tokens)
            .ok_or("RWE aggregate token budget overflow")?;
        aggregate_wall_ms = aggregate_wall_ms
            .checked_add(wall_ms)
            .ok_or("RWE aggregate wall-time budget overflow")?;
    }
    if required_u64_field(body, "max_total_provider_requests")? != aggregate_requests
        || required_u64_field(body, "max_total_tokens")? != aggregate_tokens
        || required_u64_field(body, "max_wall_time_ms")? != aggregate_wall_ms
    {
        return Err("RWE aggregate budgets do not match frozen corpus budgets".into());
    }
    if let Some(ceiling) = cost_ceiling {
        if aggregate_cost > ceiling {
            return Err(
                "RWE aggregate max_cost exceeds cost_authority ceiling or aggregate cost envelope"
                    .into(),
            );
        }
    }
    Ok(())
}

/// Frozen per-task task-definition and budget bound into the RWE authorization envelope.
#[derive(Debug, Clone, PartialEq)]
pub struct RwePerTaskBudget {
    pub task_id: String,
    pub max_provider_requests: u64,
    pub max_input_tokens: u64,
    pub max_output_tokens: u64,
    pub max_total_tokens: u64,
    pub max_wall_time_ms: u64,
    pub max_retries: u64,
    pub source_repository: String,
    pub source_commit: String,
    pub source_tree_hash: String,
    pub allowed_mutable_paths: Vec<String>,
    pub expected_verification_commands: Vec<String>,
    pub expected_outcome_class: String,
    pub patch_max_files: u64,
    pub patch_max_lines: u64,
    pub cancel_behavior: String,
    pub executor_identity: String,
    pub model_identity: String,
    pub deterministic_seed: u64,
    pub cleanup_rules: Vec<String>,
    pub max_cost: Option<f64>,
}

impl RwePerTaskBudget {
    pub fn from_task_definition(task: &RweTaskDefinition, max_cost: Option<f64>) -> Self {
        Self {
            task_id: task.task_id.clone(),
            max_provider_requests: task.per_task_max_provider_requests,
            max_input_tokens: task.per_task_max_input_tokens,
            max_output_tokens: task.per_task_max_output_tokens,
            max_total_tokens: task.per_task_max_total_tokens,
            max_wall_time_ms: task.timeout_ms,
            max_retries: task.per_task_max_retries,
            source_repository: task.source_repository.clone(),
            source_commit: task.source_commit.clone(),
            source_tree_hash: task.source_tree_hash.clone(),
            allowed_mutable_paths: task.allowed_mutable_paths.clone(),
            expected_verification_commands: task.expected_verification_commands.clone(),
            expected_outcome_class: task.expected_outcome_class.clone(),
            patch_max_files: task.patch_max_files,
            patch_max_lines: task.patch_max_lines,
            cancel_behavior: task.cancel_behavior.clone(),
            executor_identity: task.executor_identity.clone(),
            model_identity: task.model_identity.clone(),
            deterministic_seed: task.deterministic_seed,
            cleanup_rules: task.cleanup_rules.clone(),
            max_cost,
        }
    }

    pub fn to_json(&self) -> Value {
        json!({
            "task_id": self.task_id,
            "max_provider_requests": self.max_provider_requests,
            "max_input_tokens": self.max_input_tokens,
            "max_output_tokens": self.max_output_tokens,
            "max_total_tokens": self.max_total_tokens,
            "max_wall_time_ms": self.max_wall_time_ms,
            "max_retries": self.max_retries,
            "source_repository": self.source_repository,
            "source_commit": self.source_commit,
            "source_tree_hash": self.source_tree_hash,
            "allowed_mutable_paths": self.allowed_mutable_paths,
            "expected_verification_commands": self.expected_verification_commands,
            "expected_outcome_class": self.expected_outcome_class,
            "patch_max_files": self.patch_max_files,
            "patch_max_lines": self.patch_max_lines,
            "cancel_behavior": self.cancel_behavior,
            "executor_identity": self.executor_identity,
            "model_identity": self.model_identity,
            "deterministic_seed": self.deterministic_seed,
            "cleanup_rules": self.cleanup_rules,
            "max_cost": self.max_cost,
        })
    }
}

/// Request to issue a one-use RWE run authorization (owner recomputes body/hash).
#[derive(Debug, Clone, PartialEq)]
pub struct RweAuthorizationIssueRequest {
    pub authorization_id: String,
    pub corpus_sha256: String,
    /// Canonical ProductTask ID owned by product_task_terminal_evidence.
    /// This is intentionally not an evidence_id alias.
    pub golden_path_product_task_id: String,
    pub task_ids: Vec<String>,
    pub max_total_provider_requests: u64,
    pub max_total_tokens: u64,
    pub max_wall_time_ms: u64,
    pub cost_authority: super::CostAuthority,
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
    pub fixture_only: bool,
}

fn parse_rfc3339_utc(field: &str, value: &str) -> Result<chrono::DateTime<chrono::Utc>, String> {
    chrono::DateTime::parse_from_rfc3339(value.trim())
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .map_err(|_| format!("{field} must be canonical RFC3339/UTC"))
}

fn is_at_or_before(expires_at: &str, now: &str) -> Result<bool, String> {
    Ok(parse_rfc3339_utc("expires_at", expires_at)? <= parse_rfc3339_utc("now", now)?)
}

fn require_finite_rwe_expiry(expires_at: &str) -> Result<String, String> {
    let raw = expires_at.trim();
    if raw.is_empty() {
        return Err("finite expires_at required".into());
    }
    let dt = parse_rfc3339_utc("expires_at", raw)?;
    let year = dt.format("%Y").to_string();
    if year == "2099" || year == "9999" || year == "2100" {
        return Err("finite expires_at required (far-future placeholder rejected)".into());
    }
    Ok(dt.format("%Y-%m-%dT%H:%M:%SZ").to_string())
}

/// Validate Golden Path terminal evidence as successful, live, independently accepted,
/// and identity-matched — not schema-only.
fn validate_golden_path_terminal_evidence(
    ev: &Value,
    request: &RweAuthorizationIssueRequest,
) -> Result<(), String> {
    if ev.get("schema_version").and_then(Value::as_str) != Some("product_task_terminal_evidence.v2")
    {
        return Err(
            "golden_path_terminal_evidence is not product_task_terminal_evidence.v2".into(),
        );
    }
    if ev.get("task_status").and_then(Value::as_str) != Some("completed") {
        return Err(
            "golden_path_terminal_evidence is not successful (task_status!=completed)".into(),
        );
    }
    let executor_class = ev
        .pointer("/node/executor_class")
        .and_then(Value::as_str)
        .unwrap_or("");
    if executor_class == "fixture_deterministic" || executor_class.is_empty() {
        return Err(
            "golden_path_terminal_evidence must be live (non-fixture) executor_class".into(),
        );
    }
    let trustworthy = ev
        .pointer("/verification/trustworthy")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let verification_status = ev
        .pointer("/verification/status")
        .and_then(Value::as_str)
        .unwrap_or("");
    if !trustworthy || !matches!(verification_status, "accepted" | "passed") {
        return Err(
            "golden_path_terminal_evidence requires trustworthy accepted/passed verification"
                .into(),
        );
    }
    let approval_id = ev
        .pointer("/approval/approval_id")
        .and_then(Value::as_str)
        .unwrap_or("");
    if approval_id.is_empty() {
        return Err("golden_path_terminal_evidence missing independent approval identity".into());
    }
    // The owner lookup contract is ProductTask ID; evidence_id remains a separate
    // canonical receipt identity and is never accepted as an alias.
    let evidence_id = ev.get("evidence_id").and_then(Value::as_str).unwrap_or("");
    if evidence_id.is_empty() {
        return Err("golden_path_terminal_evidence missing evidence_id".into());
    }
    let ev_product_task_id = ev
        .get("product_task_id")
        .and_then(Value::as_str)
        .unwrap_or("");
    let requested_product_task_id = request.golden_path_product_task_id.trim();
    if requested_product_task_id != ev_product_task_id {
        return Err("golden_path_product_task_id does not match ProductTask identity".into());
    }
    let source_revision = ev
        .get("source_revision")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or("golden_path_terminal_evidence source_revision required")?;
    if source_revision != request.target_main_sha {
        return Err(
            "golden_path_terminal_evidence source_revision mismatches target_main_sha".into(),
        );
    }
    let identity = ev
        .pointer("/node/managed_executor_identity")
        .and_then(Value::as_object)
        .ok_or("golden_path_terminal_evidence managed_executor_identity object required")?;
    if identity.get("schema_version").and_then(Value::as_str)
        != Some("managed_executor_identity.v1")
        || identity.get("executor_type").and_then(Value::as_str)
            != Some(request.executor_identity.as_str())
        || identity.get("model").and_then(Value::as_str) != Some(request.model_identity.as_str())
        || identity.get("binary_path").and_then(Value::as_str) != Some(request.binary_path.as_str())
        || identity.get("binary_version").and_then(Value::as_str)
            != Some(request.binary_version.as_str())
        || identity.get("binary_sha256").and_then(Value::as_str)
            != Some(request.binary_sha256.as_str())
        || identity.get("provider_kind").and_then(Value::as_str)
            != Some(request.provider_kind.as_str())
        || identity.get("provider_host").and_then(Value::as_str)
            != Some(request.provider_host.as_str())
        || identity.get("provider_base_url").and_then(Value::as_str)
            != Some(request.provider_base_url.as_str())
    {
        return Err(
            "golden_path_terminal_evidence executor/provider/binary/model identity mismatch".into(),
        );
    }
    if ev.pointer("/output/intent").and_then(Value::as_str) != Some("draft_pr") {
        return Err("golden_path_terminal_evidence output intent is not draft_pr".into());
    }
    let draft_pr = ev
        .pointer("/output/draft_pr")
        .and_then(Value::as_object)
        .ok_or("golden_path_terminal_evidence draft_pr output target required")?;
    if draft_pr.get("repository").and_then(Value::as_str) != Some(request.target_repo.as_str())
        || draft_pr.get("base_branch").and_then(Value::as_str) != Some("main")
        || draft_pr.get("draft").and_then(Value::as_bool) != Some(true)
        || draft_pr
            .get("head_sha")
            .and_then(Value::as_str)
            .is_none_or(str::is_empty)
    {
        return Err("golden_path_terminal_evidence draft_pr target mismatch".into());
    }
    Ok(())
}

/// Strip lease capability tokens from a general-read run projection.
fn redact_lease_from_run_view(mut row: Value) -> Value {
    if let Value::Object(ref mut m) = row {
        m.remove("lease_token");
        if let Some(Value::Object(ev)) = m.get_mut("evidence_json") {
            if let Some(Value::Object(admit)) = ev.get_mut("admit_state") {
                admit.remove("lease_token");
            }
        }
    }
    row
}

impl LocalProductStore {
    /// Authenticated-only RWE authorization creation. Recomputes canonical body/hash inside
    /// the owner; caller-supplied body_sha is never trusted.
    pub fn issue_rwe_run_authorization(
        &self,
        principal: &AuthenticatedPrincipal,
        request: &RweAuthorizationIssueRequest,
    ) -> Result<Value, String> {
        principal.require_scope(super::SCOPE_SPEND_AUTHORIZE)?;
        let fixture_only = request.fixture_only;
        if fixture_only
            != matches!(
                principal.principal_kind(),
                super::PrincipalKind::FixturePrincipal
            )
        {
            return Err("fixture_only mismatch with principal kind".into());
        }
        if !fixture_only && !principal.may_authorize_production_live_start() {
            return Err("principal cannot authorize production RWE spend".into());
        }
        if request.corpus_sha256.len() != 64
            || !request.corpus_sha256.chars().all(|c| c.is_ascii_hexdigit())
        {
            return Err("corpus_sha256 must be 64 hex chars".into());
        }
        let expires_at = require_finite_rwe_expiry(&request.expires_at)?;
        if request.task_ids.is_empty() {
            return Err("task_ids required".into());
        }
        if request.target_repo.trim().is_empty() {
            return Err("target_repo required".into());
        }
        if request.target_main_sha.len() != 40 && request.target_main_sha.len() != 64 {
            return Err("target_main_sha invalid".into());
        }
        if !request.draft_pr_only {
            return Err("draft_pr_only required".into());
        }
        if request.admitted_executor.trim().is_empty() {
            return Err("admitted_executor required".into());
        }
        if !request.auto_merge_disabled {
            return Err("auto_merge_disabled required".into());
        }
        if request.max_total_provider_requests == 0
            || request.max_total_tokens == 0
            || request.max_wall_time_ms == 0
        {
            return Err("aggregate budgets must be positive".into());
        }
        if request.per_task_budgets.is_empty() {
            return Err("per_task_budgets required".into());
        }
        for budget in &request.per_task_budgets {
            if budget.task_id.trim().is_empty()
                || budget.max_provider_requests == 0
                || budget.max_total_tokens == 0
                || budget.max_wall_time_ms == 0
            {
                return Err(format!(
                    "per-task budget incomplete or zero for task {}",
                    budget.task_id
                ));
            }
            if !request.task_ids.iter().any(|t| t == &budget.task_id) {
                return Err(format!(
                    "per-task budget task_id {} not in task_ids",
                    budget.task_id
                ));
            }
        }
        for task_id in &request.task_ids {
            if !request
                .per_task_budgets
                .iter()
                .any(|b| &b.task_id == task_id)
            {
                return Err(format!("missing per-task budget for task_id {task_id}"));
            }
        }
        if request.binary_path.trim().is_empty()
            || request.binary_version.trim().is_empty()
            || request.binary_sha256.len() != 64
            || !request.binary_sha256.chars().all(|c| c.is_ascii_hexdigit())
        {
            return Err("exact binary path/version/sha256 required".into());
        }
        if request.provider_kind.trim().is_empty()
            || request.provider_host.trim().is_empty()
            || request.provider_base_url.trim().is_empty()
        {
            return Err("exact provider kind/host/base_url required".into());
        }
        let _ = super::CostAuthority::from_json(&request.cost_authority.to_json())?;
        // Bind / verify Golden Path terminal evidence (live requires full acceptance proof).
        if request.golden_path_product_task_id.trim().is_empty() {
            return Err("golden_path_product_task_id required".into());
        }
        if !fixture_only {
            let te =
                self.get_product_task_terminal_evidence(request.golden_path_product_task_id.trim());
            match te {
                Ok(ev) if !ev.is_null() => {
                    validate_golden_path_terminal_evidence(&ev, request)?;
                }
                _ => {
                    return Err(
                        "golden_path_product_task_id not found in terminal-evidence owner".into(),
                    );
                }
            }
        }

        let per_task = request
            .per_task_budgets
            .iter()
            .map(RwePerTaskBudget::to_json)
            .collect::<Vec<_>>();
        let body_json = sort_value(&json!({
            "schema_version": "rwe_run_authorization.v1",
            "authorization_id": request.authorization_id,
            "tenant_id": principal.tenant_id(),
            "corpus_sha256": request.corpus_sha256,
            "golden_path_product_task_id": request.golden_path_product_task_id,
            "principal_id": principal.principal_id(),
            "principal_kind": principal.principal_kind().as_str(),
            "task_ids": request.task_ids,
            "max_total_provider_requests": request.max_total_provider_requests,
            "max_total_tokens": request.max_total_tokens,
            "max_wall_time_ms": request.max_wall_time_ms,
            "cost_authority": request.cost_authority.to_json(),
            "per_task_budgets": per_task,
            "binary_path": request.binary_path,
            "binary_version": request.binary_version,
            "binary_sha256": request.binary_sha256,
            "provider_kind": request.provider_kind,
            "provider_host": request.provider_host,
            "provider_base_url": request.provider_base_url,
            "target_repo": request.target_repo,
            "target_main_sha": request.target_main_sha,
            "executor_identity": request.executor_identity,
            "model_identity": request.model_identity,
            "draft_pr_only": request.draft_pr_only,
            "admitted_executor": request.admitted_executor,
            "auto_merge_disabled": request.auto_merge_disabled,
            "one_use": true,
            "fixture_only": fixture_only,
            "expires_at": expires_at,
        }));
        validate_rwe_corpus_envelope(&body_json)?;
        let body_sha256 = sha256_hex(canonical_json(&body_json)?.as_bytes());
        self.insert_rwe_run_authorization_owned(
            principal.tenant_id(),
            &request.authorization_id,
            principal.principal_id(),
            principal.principal_kind().as_str(),
            &request.corpus_sha256,
            &body_sha256,
            &body_json,
            &expires_at,
            fixture_only,
        )
    }

    /// Test-only wrapper: persist a gate-eligible RWE authorization row exactly as
    /// `issue_rwe_run_authorization` would, bypassing the issue-time terminal-evidence
    /// binding. On current main no real terminal evidence can satisfy that binding
    /// (the frozen corpus identity can never equal a graph-compiled
    /// `managed_executor_identity`), so a live-eligible row can only be constructed
    /// through this owner insert for runner fail-closed regression coverage.
    /// Never compiled into production builds.
    #[cfg(test)]
    #[doc(hidden)]
    pub fn insert_rwe_run_authorization_for_tests(
        &self,
        tenant_id: &str,
        authorization_id: &str,
        principal_id: &str,
        principal_kind: &str,
        body_json: &Value,
        expires_at: &str,
        fixture_only: bool,
    ) -> Result<Value, String> {
        let body_json = sort_value(body_json);
        let body_sha256 = sha256_hex(canonical_json(&body_json)?.as_bytes());
        let corpus_sha256 = body_json
            .get("corpus_sha256")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        self.insert_rwe_run_authorization_owned(
            tenant_id,
            authorization_id,
            principal_id,
            principal_kind,
            &corpus_sha256,
            &body_sha256,
            &body_json,
            expires_at,
            fixture_only,
        )
    }

    /// Private owner insert/exact-replay. Not a public caller-supplied body/hash bypass.
    fn insert_rwe_run_authorization_owned(
        &self,
        tenant_id: &str,
        authorization_id: &str,
        principal_id: &str,
        principal_kind: &str,
        corpus_sha256: &str,
        body_sha256: &str,
        body_json: &Value,
        expires_at: &str,
        fixture_only: bool,
    ) -> Result<Value, String> {
        let now = self.now();
        if is_at_or_before(expires_at, &now)? {
            return Err("RWE authorization already expired at issue time".into());
        }
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = rusqlite::Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                    .map_err(|e| e.to_string())?;
                if let Some((existing_sha, _status)) = tx
                    .query_row(
                        "SELECT body_sha256, status FROM rwe_run_authorizations WHERE authorization_id=?1",
                        params![authorization_id],
                        |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
                    )
                    .optional()
                    .map_err(|e| e.to_string())?
                {
                    if existing_sha != body_sha256 {
                        return Err("conflicting RWE authorization body".into());
                    }
                    return load_rwe_auth_sqlite(&tx, authorization_id);
                }
                tx.execute(
                    "INSERT INTO rwe_run_authorizations (
                        authorization_id, tenant_id, principal_id, principal_kind, corpus_sha256,
                        body_sha256, body_json, fixture_only, status, created_at, updated_at,
                        expires_at, consumed_at, consumed_by_run_id, revoked_at
                     ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,'active',?9,?9,?10,NULL,NULL,NULL)",
                    params![
                        authorization_id,
                        tenant_id,
                        principal_id,
                        principal_kind,
                        corpus_sha256,
                        body_sha256,
                        body_json.to_string(),
                        if fixture_only { 1 } else { 0 },
                        now,
                        expires_at,
                    ],
                )
                .map_err(|e| e.to_string())?;
                append_audit_locked(
                    &tx,
                    &now,
                    principal_id,
                    "rwe.authorization_issued",
                    authorization_id,
                    &json!({"body_sha256": body_sha256, "fixture_only": fixture_only}),
                )?;
                let row = load_rwe_auth_sqlite(&tx, authorization_id)?;
                tx.commit().map_err(|e| e.to_string())?;
                Ok(row)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|e| e.to_string())?;
                tx.execute(
                    "SELECT pg_advisory_xact_lock(hashtext($1))",
                    &[&format!("rwea:{authorization_id}")],
                )
                .map_err(|e| e.to_string())?;
                if let Some(row) = tx
                    .query_opt(
                        "SELECT body_sha256 FROM rwe_run_authorizations WHERE authorization_id=$1 FOR UPDATE",
                        &[&authorization_id],
                    )
                    .map_err(|e| e.to_string())?
                {
                    let existing: String = row.get(0);
                    if existing != body_sha256 {
                        return Err("conflicting RWE authorization body".into());
                    }
                    return load_rwe_auth_pg(&mut tx, authorization_id);
                }
                let fixture_i: i32 = if fixture_only { 1 } else { 0 };
                let active = "active";
                tx.execute(
                    "INSERT INTO rwe_run_authorizations (
                        authorization_id, tenant_id, principal_id, principal_kind, corpus_sha256,
                        body_sha256, body_json, fixture_only, status, created_at, updated_at,
                        expires_at, consumed_at, consumed_by_run_id, revoked_at
                     ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$10,$11,NULL,NULL,NULL)",
                    &[
                        &authorization_id,
                        &tenant_id,
                        &principal_id,
                        &principal_kind,
                        &corpus_sha256,
                        &body_sha256,
                        &body_json.to_string(),
                        &fixture_i,
                        &active,
                        &now,
                        &expires_at,
                    ],
                )
                .map_err(|e| e.to_string())?;
                let row = load_rwe_auth_pg(&mut tx, authorization_id)?;
                tx.commit().map_err(|e| e.to_string())?;
                Ok(row)
            }),
        }
    }

    /// General run read. Lease tokens are capability secrets and are never returned here;
    /// they are only returned from successful admission / current-owner context.
    pub fn get_rwe_run(&self, run_id: &str) -> Result<Option<Value>, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let exists = conn
                    .query_row(
                        "SELECT run_id FROM rwe_runs WHERE run_id=?1",
                        params![run_id],
                        |_| Ok(()),
                    )
                    .optional()
                    .map_err(|e| e.to_string())?;
                if exists.is_none() {
                    return Ok(None);
                }
                Ok(Some(redact_lease_from_run_view(load_rwe_run_sqlite(
                    conn, run_id,
                )?)))
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                if client
                    .query_opt("SELECT run_id FROM rwe_runs WHERE run_id=$1", &[&run_id])
                    .map_err(|e| e.to_string())?
                    .is_some()
                {
                    Ok(Some(redact_lease_from_run_view(load_rwe_run_pg(
                        client, run_id,
                    )?)))
                } else {
                    Ok(None)
                }
            }),
        }
    }

    pub fn get_rwe_run_authorization(
        &self,
        authorization_id: &str,
    ) -> Result<Option<Value>, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                conn.query_row(
                    "SELECT authorization_id FROM rwe_run_authorizations WHERE authorization_id=?1",
                    params![authorization_id],
                    |_| Ok(()),
                )
                .optional()
                .map_err(|e| e.to_string())?
                .map(|_| load_rwe_auth_sqlite(conn, authorization_id))
                .transpose()
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                if client
                    .query_opt(
                        "SELECT authorization_id FROM rwe_run_authorizations WHERE authorization_id=$1",
                        &[&authorization_id],
                    )
                    .map_err(|e| e.to_string())?
                    .is_some()
                {
                    Ok(Some(load_rwe_auth_pg(client, authorization_id)?))
                } else {
                    Ok(None)
                }
            }),
        }
    }

    pub fn revoke_rwe_run_authorization(
        &self,
        principal: &AuthenticatedPrincipal,
        authorization_id: &str,
    ) -> Result<Value, String> {
        principal.require_scope(super::SCOPE_REVOKE)?;
        let now = self.now();
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = rusqlite::Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                    .map_err(|e| e.to_string())?;
                let auth = load_rwe_auth_sqlite(&tx, authorization_id)?;
                if auth.get("principal_id").and_then(Value::as_str)
                    != Some(principal.principal_id())
                    && !principal.has_scope("team:admin")
                {
                    return Err("principal cannot revoke this RWE authorization".into());
                }
                tx.execute(
                    "UPDATE rwe_run_authorizations SET status='revoked', revoked_at=?1, updated_at=?1 WHERE authorization_id=?2 AND status='active'",
                    params![now, authorization_id],
                )
                .map_err(|e| e.to_string())?;
                let row = load_rwe_auth_sqlite(&tx, authorization_id)?;
                tx.commit().map_err(|e| e.to_string())?;
                Ok(row)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|e| e.to_string())?;
                let auth = load_rwe_auth_pg(&mut tx, authorization_id)?;
                if auth.get("principal_id").and_then(Value::as_str)
                    != Some(principal.principal_id())
                    && !principal.has_scope("team:admin")
                {
                    return Err("principal cannot revoke this RWE authorization".into());
                }
                tx.execute(
                    "UPDATE rwe_run_authorizations SET status='revoked', revoked_at=$1, updated_at=$1 WHERE authorization_id=$2 AND status='active'",
                    &[&now, &authorization_id],
                )
                .map_err(|e| e.to_string())?;
                let row = load_rwe_auth_pg(&mut tx, authorization_id)?;
                tx.commit().map_err(|e| e.to_string())?;
                Ok(row)
            }),
        }
    }

    /// Admit a run: revalidate complete authorization envelope, compare run body, consume one-use.
    pub fn admit_rwe_run(
        &self,
        principal: &AuthenticatedPrincipal,
        run_id: &str,
        authorization_id: &str,
        run_body: &Value,
        allow_fixture: bool,
    ) -> Result<Value, String> {
        let now = self.now();
        let body = sort_value(run_body);
        let run_body_sha256 = sha256_hex(canonical_json(&body)?.as_bytes());
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = rusqlite::Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                    .map_err(|e| e.to_string())?;
                if let Some(_status) = tx
                    .query_row(
                        "SELECT status FROM rwe_runs WHERE run_id=?1",
                        params![run_id],
                        |r| r.get::<_, String>(0),
                    )
                    .optional()
                    .map_err(|e| e.to_string())?
                {
                    let existing = load_rwe_run_sqlite(&tx, run_id)?;
                    let auth = load_rwe_auth_sqlite(&tx, authorization_id)?;
                    validate_existing_rwe_run_replay(
                        &existing,
                        &auth,
                        principal,
                        authorization_id,
                        allow_fixture,
                        &body,
                        &run_body_sha256,
                    )?;
                    return exact_run_replay_or_conflict(&existing, &run_body_sha256);
                }
                let auth = load_rwe_auth_sqlite(&tx, authorization_id)?;
                validate_rwe_auth_for_admit(
                    &auth,
                    principal,
                    &body,
                    allow_fixture,
                    &now,
                )?;
                let updated = tx
                    .execute(
                        "UPDATE rwe_run_authorizations SET status='consumed', consumed_at=?1, consumed_by_run_id=?2, updated_at=?1 WHERE authorization_id=?3 AND status='active'",
                        params![now, run_id, authorization_id],
                    )
                    .map_err(|e| e.to_string())?;
                if updated != 1 {
                    return Err("RWE authorization already consumed".into());
                }
                let lease_token = format!("rwe-lease-{}", Uuid::new_v4());
                let admit_envelope = sort_value(&json!({
                    "admit_state": {
                        "lease_token": lease_token,
                        "run_body_sha256": run_body_sha256,
                        "run_body": body,
                    },
                    "live_baseline_sealed": false,
                    "provider_free_fixture_completion": false,
                    "live_provider_request": false,
                }));
                let corpus_sha256 = auth
                    .get("corpus_sha256")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                tx.execute(
                    "INSERT INTO rwe_runs (
                        run_id, tenant_id, authorization_id, corpus_sha256, principal_id,
                        status, evidence_json, evidence_sha256, created_at, updated_at
                     ) VALUES (?1,?2,?3,?4,?5,'admitted',?6,NULL,?7,?7)",
                    params![
                        run_id,
                        principal.tenant_id(),
                        authorization_id,
                        corpus_sha256,
                        principal.principal_id(),
                        admit_envelope.to_string(),
                        now
                    ],
                )
                .map_err(|e| e.to_string())?;
                append_audit_locked(
                    &tx,
                    &now,
                    principal.principal_id(),
                    "rwe.run_admitted",
                    run_id,
                    &json!({
                        "authorization_id": authorization_id,
                        "run_body_sha256": run_body_sha256,
                    }),
                )?;
                let mut row = load_rwe_run_sqlite(&tx, run_id)?;
                if let Value::Object(ref mut m) = row {
                    m.insert("lease_token".into(), json!(lease_token));
                }
                tx.commit().map_err(|e| e.to_string())?;
                Ok(row)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|e| e.to_string())?;
                tx.execute(
                    "SELECT pg_advisory_xact_lock(hashtext($1))",
                    &[&format!("rwer:{run_id}")],
                )
                .map_err(|e| e.to_string())?;
                if tx
                    .query_opt(
                        "SELECT status FROM rwe_runs WHERE run_id=$1 FOR UPDATE",
                        &[&run_id],
                    )
                    .map_err(|e| e.to_string())?
                    .is_some()
                {
                    let existing = load_rwe_run_pg(&mut tx, run_id)?;
                    let auth = load_rwe_auth_pg(&mut tx, authorization_id)?;
                    validate_existing_rwe_run_replay(
                        &existing,
                        &auth,
                        principal,
                        authorization_id,
                        allow_fixture,
                        &body,
                        &run_body_sha256,
                    )?;
                    return exact_run_replay_or_conflict(&existing, &run_body_sha256);
                }
                let auth = load_rwe_auth_pg(&mut tx, authorization_id)?;
                validate_rwe_auth_for_admit(
                    &auth,
                    principal,
                    &body,
                    allow_fixture,
                    &now,
                )?;
                let updated = tx
                    .execute(
                        "UPDATE rwe_run_authorizations SET status='consumed', consumed_at=$1, consumed_by_run_id=$2, updated_at=$1 WHERE authorization_id=$3 AND status='active'",
                        &[&now, &run_id, &authorization_id],
                    )
                    .map_err(|e| e.to_string())?;
                if updated != 1 {
                    return Err("RWE authorization already consumed".into());
                }
                let lease_token = format!("rwe-lease-{}", Uuid::new_v4());
                let admit_envelope = sort_value(&json!({
                    "admit_state": {
                        "lease_token": lease_token,
                        "run_body_sha256": run_body_sha256,
                        "run_body": body,
                    },
                    "live_baseline_sealed": false,
                    "provider_free_fixture_completion": false,
                    "live_provider_request": false,
                }));
                let corpus_sha256 = auth
                    .get("corpus_sha256")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let status = "admitted";
                let tenant = principal.tenant_id();
                let pid = principal.principal_id();
                tx.execute(
                    "INSERT INTO rwe_runs (
                        run_id, tenant_id, authorization_id, corpus_sha256, principal_id,
                        status, evidence_json, evidence_sha256, created_at, updated_at
                     ) VALUES ($1,$2,$3,$4,$5,$6,$7,NULL,$8,$8)",
                    &[
                        &run_id,
                        &tenant,
                        &authorization_id,
                        &corpus_sha256,
                        &pid,
                        &status,
                        &admit_envelope.to_string(),
                        &now,
                    ],
                )
                .map_err(|e| e.to_string())?;
                let mut row = load_rwe_run_pg(&mut tx, run_id)?;
                if let Value::Object(ref mut m) = row {
                    m.insert("lease_token".into(), json!(lease_token));
                }
                tx.commit().map_err(|e| e.to_string())?;
                Ok(row)
            }),
        }
    }

    /// Immutable exact-replay-or-conflict task-attempt persistence (no UPSERT mutation).
    /// Requires the current run lease / owner for every new write.
    pub fn persist_rwe_task_attempt(
        &self,
        run_id: &str,
        lease_token: &str,
        task_attempt_id: &str,
        task_id: &str,
        definition_sha256: &str,
        classification: &str,
        evidence: &Value,
    ) -> Result<Value, String> {
        let now = self.now();
        let evidence_sorted = sort_value(evidence);
        let evidence_s = canonical_json(&evidence_sorted)?;
        let evidence_sha = sha256_hex(evidence_s.as_bytes());
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = rusqlite::Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                    .map_err(|e| e.to_string())?;
                let run = load_rwe_run_sqlite(&tx, run_id)?;
                validate_current_rwe_run_lease(&run, run_id, lease_token)?;
                if let Some((
                    existing_run,
                    existing_task,
                    existing_def,
                    existing_class,
                    existing_sha,
                )) = tx
                    .query_row(
                        "SELECT run_id, task_id, definition_sha256, classification, evidence_sha256
                         FROM rwe_task_attempts WHERE task_attempt_id=?1",
                        params![task_attempt_id],
                        |r| {
                            Ok((
                                r.get::<_, String>(0)?,
                                r.get::<_, String>(1)?,
                                r.get::<_, String>(2)?,
                                r.get::<_, String>(3)?,
                                r.get::<_, String>(4)?,
                            ))
                        },
                    )
                    .optional()
                    .map_err(|e| e.to_string())?
                {
                    if existing_run != run_id
                        || existing_task != task_id
                        || existing_def != definition_sha256
                        || existing_class != classification
                        || existing_sha != evidence_sha
                    {
                        return Err("conflicting RWE task-attempt identity/evidence".into());
                    }
                    return Ok(json!({
                        "task_attempt_id": task_attempt_id,
                        "run_id": run_id,
                        "task_id": task_id,
                        "definition_sha256": definition_sha256,
                        "evidence_sha256": evidence_sha,
                        "classification": classification,
                        "idempotent_replay": true,
                    }));
                }
                tx.execute(
                    "INSERT INTO rwe_task_attempts (
                        task_attempt_id, run_id, task_id, definition_sha256, classification,
                        evidence_json, evidence_sha256, created_at
                     ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
                    params![
                        task_attempt_id,
                        run_id,
                        task_id,
                        definition_sha256,
                        classification,
                        evidence_s,
                        evidence_sha,
                        now
                    ],
                )
                .map_err(|e| e.to_string())?;
                tx.commit().map_err(|e| e.to_string())?;
                Ok(json!({
                    "task_attempt_id": task_attempt_id,
                    "run_id": run_id,
                    "task_id": task_id,
                    "definition_sha256": definition_sha256,
                    "evidence_sha256": evidence_sha,
                    "classification": classification,
                    "idempotent_replay": false,
                }))
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|e| e.to_string())?;
                tx.execute(
                    "SELECT pg_advisory_xact_lock(hashtext($1))",
                    &[&format!("rwer:{run_id}")],
                )
                .map_err(|e| e.to_string())?;
                tx.execute(
                    "SELECT pg_advisory_xact_lock(hashtext($1))",
                    &[&format!("rweta:{task_attempt_id}")],
                )
                .map_err(|e| e.to_string())?;
                let run = load_rwe_run_pg(&mut tx, run_id)?;
                validate_current_rwe_run_lease(&run, run_id, lease_token)?;
                if let Some(row) = tx
                    .query_opt(
                        "SELECT run_id, task_id, definition_sha256, classification, evidence_sha256
                         FROM rwe_task_attempts WHERE task_attempt_id=$1 FOR UPDATE",
                        &[&task_attempt_id],
                    )
                    .map_err(|e| e.to_string())?
                {
                    let existing_run: String = row.get(0);
                    let existing_task: String = row.get(1);
                    let existing_def: String = row.get(2);
                    let existing_class: String = row.get(3);
                    let existing_sha: String = row.get(4);
                    if existing_run != run_id
                        || existing_task != task_id
                        || existing_def != definition_sha256
                        || existing_class != classification
                        || existing_sha != evidence_sha
                    {
                        return Err("conflicting RWE task-attempt identity/evidence".into());
                    }
                    return Ok(json!({
                        "task_attempt_id": task_attempt_id,
                        "run_id": run_id,
                        "task_id": task_id,
                        "definition_sha256": definition_sha256,
                        "evidence_sha256": evidence_sha,
                        "classification": classification,
                        "idempotent_replay": true,
                    }));
                }
                tx.execute(
                    "INSERT INTO rwe_task_attempts (
                        task_attempt_id, run_id, task_id, definition_sha256, classification,
                        evidence_json, evidence_sha256, created_at
                     ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
                    &[
                        &task_attempt_id,
                        &run_id,
                        &task_id,
                        &definition_sha256,
                        &classification,
                        &evidence_s,
                        &evidence_sha,
                        &now,
                    ],
                )
                .map_err(|e| e.to_string())?;
                tx.commit().map_err(|e| e.to_string())?;
                Ok(json!({
                    "task_attempt_id": task_attempt_id,
                    "run_id": run_id,
                    "task_id": task_id,
                    "definition_sha256": definition_sha256,
                    "evidence_sha256": evidence_sha,
                    "classification": classification,
                    "idempotent_replay": false,
                }))
            }),
        }
    }

    /// Terminalize under current lease. Stores one canonical terminal receipt body/hash so
    /// direct exact replay succeeds and conflicting replay rejects.
    pub fn complete_rwe_run(
        &self,
        run_id: &str,
        lease_token: &str,
        status: &str,
        evidence: &Value,
        evidence_sha256: &str,
    ) -> Result<Value, String> {
        if !matches!(
            status,
            "fixture_complete" | "succeeded" | "failed" | "cancelled" | "outcome_unknown"
        ) {
            return Err(format!("invalid RWE terminal status {status}"));
        }
        let now = self.now();
        let evidence_sorted = sort_value(evidence);
        // Caller evidence body must be self-consistent before owner wraps the receipt.
        let caller_evidence_sha = sha256_hex(canonical_json(&evidence_sorted)?.as_bytes());
        if caller_evidence_sha != evidence_sha256 {
            return Err("evidence_sha256 mismatch vs evidence body".into());
        }
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = rusqlite::Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                    .map_err(|e| e.to_string())?;
                let existing = load_rwe_run_sqlite(&tx, run_id)?;
                let cur_status = existing
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let admit = existing
                    .get("evidence_json")
                    .cloned()
                    .unwrap_or(Value::Null);
                let admit_body_sha = admit
                    .pointer("/admit_state/run_body_sha256")
                    .or_else(|| admit.pointer("/admit_run_body_sha256"))
                    .cloned()
                    .unwrap_or(Value::Null);
                let receipt = build_canonical_terminal_receipt(
                    run_id,
                    status,
                    &evidence_sorted,
                    &admit_body_sha,
                )?;
                let receipt_sha = sha256_hex(canonical_json(&receipt)?.as_bytes());
                if cur_status != "admitted" {
                    // Exact terminal replay: stored hash must match rebuilt canonical receipt.
                    if cur_status == status
                        && existing.get("evidence_sha256").and_then(Value::as_str)
                            == Some(receipt_sha.as_str())
                    {
                        let mut row = redact_lease_from_run_view(existing);
                        if let Value::Object(ref mut m) = row {
                            m.insert("idempotent_replay".into(), json!(true));
                        }
                        return Ok(row);
                    }
                    return Err("late RWE terminal write rejected".into());
                }
                let expected_lease = admit
                    .pointer("/admit_state/lease_token")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                if expected_lease != lease_token {
                    return Err("RWE lease_token mismatch".into());
                }
                let terminal_s = canonical_json(&receipt)?;
                tx.execute(
                    "UPDATE rwe_runs SET status=?1, evidence_json=?2, evidence_sha256=?3, updated_at=?4 WHERE run_id=?5 AND status='admitted'",
                    params![status, terminal_s, receipt_sha, now, run_id],
                )
                .map_err(|e| e.to_string())?;
                let row = redact_lease_from_run_view(load_rwe_run_sqlite(&tx, run_id)?);
                tx.commit().map_err(|e| e.to_string())?;
                Ok(row)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|e| e.to_string())?;
                tx.execute(
                    "SELECT pg_advisory_xact_lock(hashtext($1))",
                    &[&format!("rwer:{run_id}")],
                )
                .map_err(|e| e.to_string())?;
                let existing = load_rwe_run_pg(&mut tx, run_id)?;
                let cur_status = existing
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let admit = existing
                    .get("evidence_json")
                    .cloned()
                    .unwrap_or(Value::Null);
                let admit_body_sha = admit
                    .pointer("/admit_state/run_body_sha256")
                    .or_else(|| admit.pointer("/admit_run_body_sha256"))
                    .cloned()
                    .unwrap_or(Value::Null);
                let receipt = build_canonical_terminal_receipt(
                    run_id,
                    status,
                    &evidence_sorted,
                    &admit_body_sha,
                )?;
                let receipt_sha = sha256_hex(canonical_json(&receipt)?.as_bytes());
                if cur_status != "admitted" {
                    if cur_status == status
                        && existing.get("evidence_sha256").and_then(Value::as_str)
                            == Some(receipt_sha.as_str())
                    {
                        let mut row = redact_lease_from_run_view(existing);
                        if let Value::Object(ref mut m) = row {
                            m.insert("idempotent_replay".into(), json!(true));
                        }
                        return Ok(row);
                    }
                    return Err("late RWE terminal write rejected".into());
                }
                let expected_lease = admit
                    .pointer("/admit_state/lease_token")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                if expected_lease != lease_token {
                    return Err("RWE lease_token mismatch".into());
                }
                let terminal_s = canonical_json(&receipt)?;
                let updated = tx
                    .execute(
                        "UPDATE rwe_runs SET status=$1, evidence_json=$2, evidence_sha256=$3, updated_at=$4 WHERE run_id=$5 AND status='admitted'",
                        &[&status, &terminal_s, &receipt_sha, &now, &run_id],
                    )
                    .map_err(|e| e.to_string())?;
                if updated != 1 {
                    return Err("late RWE terminal write rejected".into());
                }
                let row = redact_lease_from_run_view(load_rwe_run_pg(&mut tx, run_id)?);
                tx.commit().map_err(|e| e.to_string())?;
                Ok(row)
            }),
        }
    }
}

fn build_canonical_terminal_receipt(
    run_id: &str,
    status: &str,
    evidence: &Value,
    admit_run_body_sha256: &Value,
) -> Result<Value, String> {
    Ok(sort_value(&json!({
        "schema_version": "rwe_terminal_receipt.v1",
        "run_id": run_id,
        "status": status,
        "admit_run_body_sha256": admit_run_body_sha256,
        "evidence": evidence,
        "live_baseline_sealed": evidence
            .get("live_baseline_sealed")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        "provider_free_fixture_completion": evidence
            .get("provider_free_fixture_completion")
            .and_then(Value::as_bool)
            .unwrap_or(status == "fixture_complete"),
        "live_provider_request": evidence
            .get("live_provider_request")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })))
}

fn exact_run_replay_or_conflict(existing: &Value, run_body_sha256: &str) -> Result<Value, String> {
    let existing_sha = existing
        .pointer("/evidence_json/admit_state/run_body_sha256")
        .and_then(Value::as_str)
        .or_else(|| {
            existing
                .pointer("/evidence_json/admit_run_body_sha256")
                .and_then(Value::as_str)
        })
        .or_else(|| {
            existing
                .pointer("/evidence_json/run_body_sha256")
                .and_then(Value::as_str)
        });
    if existing_sha != Some(run_body_sha256) {
        return Err("conflicting RWE run body reuse or missing body identity".into());
    }
    let mut row = redact_lease_from_run_view(existing.clone());
    if let Value::Object(ref mut m) = row {
        m.insert("idempotent_replay".into(), json!(true));
    }
    Ok(row)
}

fn validate_existing_rwe_run_replay(
    existing: &Value,
    auth: &Value,
    principal: &AuthenticatedPrincipal,
    authorization_id: &str,
    allow_fixture: bool,
    run_body: &Value,
    run_body_sha256: &str,
) -> Result<(), String> {
    let run_id = existing
        .get("run_id")
        .and_then(Value::as_str)
        .ok_or("existing RWE run missing run_id")?;
    if auth.get("authorization_id").and_then(Value::as_str) != Some(authorization_id)
        || existing.get("authorization_id").and_then(Value::as_str) != Some(authorization_id)
    {
        return Err("RWE existing-run replay authorization identity mismatch".into());
    }
    if auth.get("tenant_id").and_then(Value::as_str) != Some(principal.tenant_id())
        || existing.get("tenant_id").and_then(Value::as_str) != Some(principal.tenant_id())
    {
        return Err("RWE existing-run replay tenant mismatch".into());
    }
    if auth.get("principal_id").and_then(Value::as_str) != Some(principal.principal_id())
        || existing.get("principal_id").and_then(Value::as_str) != Some(principal.principal_id())
        || auth.get("principal_kind").and_then(Value::as_str)
            != Some(principal.principal_kind().as_str())
    {
        return Err("RWE existing-run replay principal/owner mismatch".into());
    }
    if auth.get("status").and_then(Value::as_str) != Some("consumed")
        || auth.get("consumed_by_run_id").and_then(Value::as_str) != Some(run_id)
    {
        return Err("RWE existing-run replay authorization is not consumed by this run".into());
    }
    if auth.get("fixture_only").and_then(Value::as_bool) != Some(allow_fixture) {
        return Err("RWE existing-run replay fixture/live state mismatch".into());
    }
    if !matches!(
        existing.get("status").and_then(Value::as_str),
        Some("admitted")
            | Some("fixture_complete")
            | Some("succeeded")
            | Some("failed")
            | Some("cancelled")
            | Some("outcome_unknown")
    ) {
        return Err("RWE existing-run replay current state is not replayable".into());
    }
    let auth_body = auth
        .get("body_json")
        .ok_or("RWE existing-run replay authorization body missing")?;
    validate_rwe_corpus_envelope(auth_body)?;
    if run_body.get("authorization_id").and_then(Value::as_str) != Some(authorization_id) {
        return Err("RWE existing-run replay body authorization mismatch".into());
    }
    if sha256_hex(canonical_json(run_body)?.as_bytes()) != run_body_sha256 {
        return Err("RWE existing-run replay body hash mismatch".into());
    }
    Ok(())
}

fn validate_current_rwe_run_lease(
    run: &Value,
    run_id: &str,
    lease_token: &str,
) -> Result<(), String> {
    if run.get("run_id").and_then(Value::as_str) != Some(run_id)
        || run.get("status").and_then(Value::as_str) != Some("admitted")
    {
        return Err("RWE task-attempt write requires an admitted current run".into());
    }
    let expected_lease = run
        .pointer("/evidence_json/admit_state/lease_token")
        .and_then(Value::as_str)
        .unwrap_or("");
    if expected_lease.is_empty() || expected_lease != lease_token {
        return Err("RWE task-attempt write requires current run lease/owner".into());
    }
    Ok(())
}

fn validate_rwe_auth_for_admit(
    auth: &Value,
    principal: &AuthenticatedPrincipal,
    run_body: &Value,
    allow_fixture: bool,
    now: &str,
) -> Result<(), String> {
    if auth.get("tenant_id").and_then(Value::as_str) != Some(principal.tenant_id()) {
        return Err("RWE authorization tenant mismatch".into());
    }
    if auth.get("principal_id").and_then(Value::as_str) != Some(principal.principal_id()) {
        return Err("RWE authorization principal mismatch".into());
    }
    if auth.get("status").and_then(Value::as_str) != Some("active") {
        return Err("RWE authorization not active".into());
    }
    if let Some(exp) = auth.get("expires_at").and_then(Value::as_str) {
        if is_at_or_before(exp, now)? {
            return Err("RWE authorization expired".into());
        }
    } else {
        return Err("RWE authorization missing expires_at".into());
    }
    if auth.get("revoked_at").and_then(Value::as_str).is_some() {
        return Err("RWE authorization revoked".into());
    }
    let fixture_only = auth
        .get("fixture_only")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if allow_fixture {
        if !fixture_only {
            return Err("fixture runner requires fixture_only authorization".into());
        }
        if !matches!(
            principal.principal_kind(),
            super::PrincipalKind::FixturePrincipal
        ) {
            return Err("fixture admit requires fixture principal".into());
        }
    } else {
        if fixture_only {
            return Err("fixture_only authorization cannot admit live RWE".into());
        }
        if !principal.may_authorize_production_live_start() {
            return Err("principal cannot admit live RWE".into());
        }
    }
    let body = auth.get("body_json").cloned().unwrap_or(Value::Null);
    validate_rwe_corpus_envelope(&body)?;
    // Complete envelope vs run body — every authority field is mandatory.
    for field in [
        "corpus_sha256",
        "target_repo",
        "target_main_sha",
        "executor_identity",
        "model_identity",
        "max_total_provider_requests",
        "max_total_tokens",
        "max_wall_time_ms",
        "golden_path_product_task_id",
        "draft_pr_only",
        "admitted_executor",
        "auto_merge_disabled",
        "task_ids",
        "cost_authority",
        "per_task_budgets",
        "binary_path",
        "binary_version",
        "binary_sha256",
        "provider_kind",
        "provider_host",
        "provider_base_url",
    ] {
        let expected = body
            .get(field)
            .cloned()
            .or_else(|| auth.get(field).cloned())
            .ok_or_else(|| format!("authorization missing field {field}"))?;
        let observed = run_body
            .get(field)
            .cloned()
            .ok_or_else(|| format!("run body missing required field {field}"))?;
        if expected != observed {
            return Err(format!("run body {field} mismatch vs RWE authorization"));
        }
    }
    if run_body.get("corpus_sha256").and_then(Value::as_str)
        != auth.get("corpus_sha256").and_then(Value::as_str)
    {
        return Err("corpus_sha256 mismatch".into());
    }
    Ok(())
}

fn load_rwe_auth_sqlite(conn: &rusqlite::Connection, id: &str) -> Result<Value, String> {
    conn.query_row(
        "SELECT authorization_id, tenant_id, principal_id, principal_kind, corpus_sha256,
                body_sha256, body_json, fixture_only, status, created_at, updated_at,
                expires_at, consumed_at, consumed_by_run_id, revoked_at
         FROM rwe_run_authorizations WHERE authorization_id=?1",
        params![id],
        |row| {
            let body_s: String = row.get(6)?;
            Ok(json!({
                "schema_version": "rwe_run_authorization.v1",
                "authorization_id": row.get::<_, String>(0)?,
                "tenant_id": row.get::<_, String>(1)?,
                "principal_id": row.get::<_, String>(2)?,
                "principal_kind": row.get::<_, String>(3)?,
                "corpus_sha256": row.get::<_, String>(4)?,
                "body_sha256": row.get::<_, String>(5)?,
                "body_json": serde_json::from_str::<Value>(&body_s).unwrap_or(Value::Null),
                "fixture_only": row.get::<_, i64>(7)? != 0,
                "status": row.get::<_, String>(8)?,
                "created_at": row.get::<_, String>(9)?,
                "updated_at": row.get::<_, String>(10)?,
                "expires_at": row.get::<_, String>(11)?,
                "consumed_at": row.get::<_, Option<String>>(12)?,
                "consumed_by_run_id": row.get::<_, Option<String>>(13)?,
                "revoked_at": row.get::<_, Option<String>>(14)?,
            }))
        },
    )
    .map_err(|e| e.to_string())
}

#[cfg(feature = "pg")]
fn load_rwe_auth_pg(client: &mut impl postgres::GenericClient, id: &str) -> Result<Value, String> {
    let row = client
        .query_one(
            "SELECT authorization_id, tenant_id, principal_id, principal_kind, corpus_sha256,
                    body_sha256, body_json, fixture_only, status, created_at, updated_at,
                    expires_at, consumed_at, consumed_by_run_id, revoked_at
             FROM rwe_run_authorizations WHERE authorization_id=$1",
            &[&id],
        )
        .map_err(|e| e.to_string())?;
    let body_s: String = row.get(6);
    let fixture: i32 = row.get(7);
    Ok(json!({
        "schema_version": "rwe_run_authorization.v1",
        "authorization_id": row.get::<_, String>(0),
        "tenant_id": row.get::<_, String>(1),
        "principal_id": row.get::<_, String>(2),
        "principal_kind": row.get::<_, String>(3),
        "corpus_sha256": row.get::<_, String>(4),
        "body_sha256": row.get::<_, String>(5),
        "body_json": serde_json::from_str::<Value>(&body_s).unwrap_or(Value::Null),
        "fixture_only": fixture != 0,
        "status": row.get::<_, String>(8),
        "created_at": row.get::<_, String>(9),
        "updated_at": row.get::<_, String>(10),
        "expires_at": row.get::<_, String>(11),
        "consumed_at": row.get::<_, Option<String>>(12),
        "consumed_by_run_id": row.get::<_, Option<String>>(13),
        "revoked_at": row.get::<_, Option<String>>(14),
    }))
}

fn load_rwe_run_sqlite(conn: &rusqlite::Connection, run_id: &str) -> Result<Value, String> {
    conn.query_row(
        "SELECT run_id, tenant_id, authorization_id, corpus_sha256, principal_id,
                status, evidence_json, evidence_sha256, created_at, updated_at
         FROM rwe_runs WHERE run_id=?1",
        params![run_id],
        |row| {
            let ev: Option<String> = row.get(6)?;
            let evidence: Value = ev
                .as_deref()
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or(Value::Null);
            let mut out = json!({
                "schema_version": "rwe_run.v1",
                "run_id": row.get::<_, String>(0)?,
                "tenant_id": row.get::<_, String>(1)?,
                "authorization_id": row.get::<_, String>(2)?,
                "corpus_sha256": row.get::<_, String>(3)?,
                "principal_id": row.get::<_, String>(4)?,
                "status": row.get::<_, String>(5)?,
                "evidence_json": evidence.clone(),
                "evidence_sha256": row.get::<_, Option<String>>(7)?,
                "created_at": row.get::<_, String>(8)?,
                "updated_at": row.get::<_, String>(9)?,
                "idempotent_replay": false,
            });
            if let (Value::Object(ref mut m), Value::Object(ev_map)) = (&mut out, &evidence) {
                for key in [
                    "live_baseline_sealed",
                    "provider_free_fixture_completion",
                    "live_provider_request",
                ] {
                    if let Some(v) = ev_map.get(key) {
                        m.insert(key.into(), v.clone());
                    }
                }
                if let Some(admit) = ev_map.get("admit_state") {
                    if let Some(lease) = admit.get("lease_token") {
                        m.insert("lease_token".into(), lease.clone());
                    }
                }
            }
            Ok(out)
        },
    )
    .map_err(|e| e.to_string())
}

#[cfg(feature = "pg")]
fn load_rwe_run_pg(
    client: &mut impl postgres::GenericClient,
    run_id: &str,
) -> Result<Value, String> {
    let row = client
        .query_one(
            "SELECT run_id, tenant_id, authorization_id, corpus_sha256, principal_id,
                    status, evidence_json, evidence_sha256, created_at, updated_at
             FROM rwe_runs WHERE run_id=$1",
            &[&run_id],
        )
        .map_err(|e| e.to_string())?;
    let ev: Option<String> = row.get(6);
    let evidence: Value = ev
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or(Value::Null);
    let mut out = json!({
        "schema_version": "rwe_run.v1",
        "run_id": row.get::<_, String>(0),
        "tenant_id": row.get::<_, String>(1),
        "authorization_id": row.get::<_, String>(2),
        "corpus_sha256": row.get::<_, String>(3),
        "principal_id": row.get::<_, String>(4),
        "status": row.get::<_, String>(5),
        "evidence_json": evidence.clone(),
        "evidence_sha256": row.get::<_, Option<String>>(7),
        "created_at": row.get::<_, String>(8),
        "updated_at": row.get::<_, String>(9),
        "idempotent_replay": false,
    });
    if let Value::Object(ref mut m) = out {
        if let Some(obj) = evidence.as_object() {
            for key in [
                "live_baseline_sealed",
                "provider_free_fixture_completion",
                "live_provider_request",
            ] {
                if let Some(v) = obj.get(key) {
                    m.insert(key.into(), v.clone());
                }
            }
            if let Some(admit) = obj.get("admit_state") {
                if let Some(lease) = admit.get("lease_token") {
                    m.insert("lease_token".into(), lease.clone());
                }
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod authority_regression_tests {
    use super::*;

    pub(super) fn valid_corpus_body() -> Value {
        let corpus = freeze_first_rwe_corpus().unwrap();
        let task_ids = corpus
            .tasks
            .iter()
            .map(|task| json!(task.task_id))
            .collect::<Vec<_>>();
        let budgets = corpus
            .tasks
            .iter()
            .map(|task| RwePerTaskBudget::from_task_definition(task, None).to_json())
            .collect::<Vec<_>>();
        json!({
            "schema_version": "rwe_run_authorization.v1",
            "corpus_sha256": corpus.corpus_sha256,
            "task_ids": task_ids,
            "per_task_budgets": budgets,
            "max_total_provider_requests": corpus.tasks.iter().map(|t| t.per_task_max_provider_requests).sum::<u64>(),
            "max_total_tokens": corpus.tasks.iter().map(|t| t.per_task_max_total_tokens).sum::<u64>(),
            "max_wall_time_ms": corpus.tasks.iter().map(|t| t.timeout_ms).sum::<u64>(),
            "cost_authority": super::super::CostAuthority::CostUnavailable.to_json(),
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
            "golden_path_product_task_id": "gp-task",
        })
    }

    fn request() -> RweAuthorizationIssueRequest {
        RweAuthorizationIssueRequest {
            authorization_id: "auth-1".into(),
            corpus_sha256: "a".repeat(64),
            golden_path_product_task_id: "product-task-1".into(),
            task_ids: vec!["task-1".into()],
            max_total_provider_requests: 1,
            max_total_tokens: 12_000,
            max_wall_time_ms: 180_000,
            cost_authority: super::super::CostAuthority::CostUnavailable,
            per_task_budgets: Vec::new(),
            binary_path: "/usr/bin/codex".into(),
            binary_version: "0.145.0".into(),
            binary_sha256: "b".repeat(64),
            provider_kind: "openai_compatible".into(),
            provider_host: "api.openai.com".into(),
            provider_base_url: "https://api.openai.com/v1".into(),
            target_repo: "org/disposable".into(),
            target_main_sha: "c".repeat(40),
            executor_identity: "codex_cli".into(),
            model_identity: "gpt-test-model".into(),
            draft_pr_only: true,
            admitted_executor: "codex-cli-api-key-mediated".into(),
            auto_merge_disabled: true,
            expires_at: "2026-08-01T00:00:00Z".into(),
            fixture_only: false,
        }
    }

    fn evidence() -> Value {
        json!({
            "schema_version": "product_task_terminal_evidence.v2",
            "evidence_id": "evidence-1",
            "product_task_id": "product-task-1",
            "task_status": "completed",
            "node": {
                "executor_class": "managed_coding",
                "managed_executor_identity": {
                    "schema_version": "managed_executor_identity.v1",
                    "executor_type": "codex_cli",
                    "binary_path": "/usr/bin/codex",
                    "binary_version": "0.145.0",
                    "binary_sha256": "b".repeat(64),
                    "model": "gpt-test-model",
                    "provider_kind": "openai_compatible",
                    "provider_host": "api.openai.com",
                    "provider_base_url": "https://api.openai.com/v1"
                }
            },
            "source_revision": "c".repeat(40),
            "verification": {"trustworthy": true, "status": "passed"},
            "approval": {"approval_id": "approval-1"},
            "output": {
                "intent": "draft_pr",
                "draft_pr": {
                    "number": 1,
                    "repository": "org/disposable",
                    "base_branch": "main",
                    "head_branch": "acp/product-task-1",
                    "head_sha": "d".repeat(40),
                    "draft": true
                }
            }
        })
    }

    #[test]
    fn terminal_evidence_requires_trustworthy_and_accepted_status() {
        let mut ev = evidence();
        ev["verification"]["trustworthy"] = json!(false);
        assert!(validate_golden_path_terminal_evidence(&ev, &request()).is_err());
        ev["verification"]["trustworthy"] = json!(true);
        ev["verification"]["status"] = json!("failed");
        assert!(validate_golden_path_terminal_evidence(&ev, &request()).is_err());
    }

    #[test]
    fn terminal_evidence_requires_object_identity_and_full_binding() {
        let mut ev = evidence();
        ev["node"]["managed_executor_identity"]["executor_type"] = json!("wrong");
        assert!(validate_golden_path_terminal_evidence(&ev, &request()).is_err());
        let mut ev = evidence();
        ev["node"]["managed_executor_identity"]["provider_base_url"] =
            json!("https://wrong.invalid/v1");
        assert!(validate_golden_path_terminal_evidence(&ev, &request()).is_err());
        let mut ev = evidence();
        ev["output"]["draft_pr"]["repository"] = json!("org/other");
        assert!(validate_golden_path_terminal_evidence(&ev, &request()).is_err());
    }

    #[test]
    fn live_issue_rejects_compiler_shaped_terminal_evidence() {
        // A terminal evidence carrying the managed_executor_identity shape that
        // compile_product_executable_graph actually emits for codex_cli
        // (engine/src/product_golden_path.rs) can never satisfy the live issue-time
        // binding: the compiled identity has no provider_kind/provider_host/
        // provider_base_url fields and its executor_type ("codex_cli") can never equal
        // the frozen corpus identity ("codex-0.145.0") required by
        // validate_rwe_corpus_envelope. This is why on current main
        // issue_rwe_run_authorization(fixture_only=false) always fails closed, and why
        // the B0 runner regression constructs its gate-eligible row through the store
        // authorization owner insert instead of issue.
        let mut ev = evidence();
        ev["node"]["managed_executor_identity"] = json!({
            "schema_version": "managed_executor_identity.v1",
            "executor_type": "codex_cli",
            "executor_class": "managed_coding",
            "runtime_profile_schema_version": "codex_runtime_profile.v1",
            "runtime_profile_id": "codex-evidence-fixture.v1",
            "capability_probe_sha256": "c".repeat(64),
            "binary_path": "/usr/bin/codex",
            "binary_version": "0.145.0",
            "binary_sha256": "b".repeat(64),
            "executor_kind": "codex-cli-api-key-mediated",
            "protocol_kind": "openai_compatible",
            "requested_model": "gpt-test-model",
            "resolved_model": "gpt-test-model",
            "model": "gpt-test-model",
            "provider_identity": "provider-identity",
            "credential_reference": "credential-reference",
            "endpoint_allowlist": ["api.openai.com"],
        });
        let err = validate_golden_path_terminal_evidence(&ev, &request()).unwrap_err();
        assert!(
            err.contains("identity mismatch") || err.contains("managed_executor_identity"),
            "{err}"
        );
        // Even the corpus-required identity variant fails: no provider_* fields exist
        // in any compiled shape, so request provider fields can never be matched.
        let mut corpus_matching = ev.clone();
        corpus_matching["node"]["managed_executor_identity"]["executor_type"] =
            json!("codex-0.145.0");
        assert!(validate_golden_path_terminal_evidence(&corpus_matching, &request()).is_err());
    }

    #[test]
    fn terminal_evidence_id_contract_is_product_task_id_only() {
        let mut ev = evidence();
        ev["product_task_id"] = json!("different-task");
        assert!(validate_golden_path_terminal_evidence(&ev, &request()).is_err());
    }

    #[test]
    fn frozen_corpus_rejects_subset_extra_duplicate_and_reordered_authority() {
        let valid = valid_corpus_body();
        validate_rwe_corpus_envelope(&valid).unwrap();

        let mut subset = valid.clone();
        subset["task_ids"] = json!([valid["task_ids"][0]]);
        assert!(validate_rwe_corpus_envelope(&subset).is_err());

        let mut extra = valid.clone();
        extra["task_ids"]
            .as_array_mut()
            .unwrap()
            .push(json!("extra"));
        assert!(validate_rwe_corpus_envelope(&extra).is_err());

        let mut duplicate = valid.clone();
        let first = duplicate["task_ids"][0].clone();
        duplicate["task_ids"].as_array_mut().unwrap()[1] = first;
        assert!(validate_rwe_corpus_envelope(&duplicate).is_err());

        let mut reordered = valid.clone();
        reordered["task_ids"].as_array_mut().unwrap().swap(0, 1);
        assert!(validate_rwe_corpus_envelope(&reordered).is_err());
        let mut reordered_budgets = valid.clone();
        reordered_budgets["per_task_budgets"]
            .as_array_mut()
            .unwrap()
            .swap(0, 1);
        assert!(validate_rwe_corpus_envelope(&reordered_budgets).is_err());
    }

    #[test]
    fn frozen_corpus_rejects_duplicate_or_missing_budgets_and_wrong_aggregate() {
        let valid = valid_corpus_body();
        let mut duplicate = valid.clone();
        let first = duplicate["per_task_budgets"][0].clone();
        duplicate["per_task_budgets"].as_array_mut().unwrap()[1] = first;
        assert!(validate_rwe_corpus_envelope(&duplicate).is_err());

        let mut missing = valid.clone();
        missing["per_task_budgets"].as_array_mut().unwrap().pop();
        assert!(validate_rwe_corpus_envelope(&missing).is_err());

        let mut wrong = valid;
        wrong["max_total_tokens"] = json!(1);
        assert!(validate_rwe_corpus_envelope(&wrong).is_err());
    }

    #[test]
    fn frozen_corpus_rejects_retry_and_identity_mutations() {
        let valid = valid_corpus_body();
        validate_rwe_corpus_envelope(&valid).unwrap();

        let mut wrong_retries = valid.clone();
        wrong_retries["per_task_budgets"][0]["max_retries"] = json!(99);
        assert!(validate_rwe_corpus_envelope(&wrong_retries).is_err());

        let mut wrong_executor = valid.clone();
        wrong_executor["per_task_budgets"][1]["executor_identity"] = json!("rogue-executor");
        assert!(validate_rwe_corpus_envelope(&wrong_executor).is_err());

        let mut wrong_model = valid.clone();
        wrong_model["per_task_budgets"][2]["model_identity"] = json!("rogue-model");
        assert!(validate_rwe_corpus_envelope(&wrong_model).is_err());

        let mut wrong_top_executor = valid.clone();
        wrong_top_executor["executor_identity"] = json!("rogue-executor");
        assert!(validate_rwe_corpus_envelope(&wrong_top_executor).is_err());

        let mut wrong_version = valid.clone();
        wrong_version["binary_version"] = json!("0.999.0");
        assert!(validate_rwe_corpus_envelope(&wrong_version).is_err());
    }

    #[test]
    fn frozen_corpus_rejects_source_and_task_definition_boundary_mutations() {
        let valid = valid_corpus_body();
        validate_rwe_corpus_envelope(&valid).unwrap();

        let mut wrong_source = valid.clone();
        wrong_source["per_task_budgets"][0]["source_repository"] =
            json!("https://rogue.example/repo");
        assert!(validate_rwe_corpus_envelope(&wrong_source).is_err());

        let mut wrong_commit = valid.clone();
        wrong_commit["per_task_budgets"][0]["source_commit"] = json!("00".repeat(20));
        assert!(validate_rwe_corpus_envelope(&wrong_commit).is_err());

        let mut wrong_paths = valid.clone();
        wrong_paths["per_task_budgets"][0]["allowed_mutable_paths"] = json!(["src/rogue.rs"]);
        assert!(validate_rwe_corpus_envelope(&wrong_paths).is_err());

        let mut wrong_verification = valid.clone();
        wrong_verification["per_task_budgets"][0]["expected_verification_commands"] =
            json!(["rm -rf /"]);
        assert!(validate_rwe_corpus_envelope(&wrong_verification).is_err());

        let mut wrong_outcome = valid.clone();
        wrong_outcome["per_task_budgets"][0]["expected_outcome_class"] = json!("unbounded_success");
        assert!(validate_rwe_corpus_envelope(&wrong_outcome).is_err());

        let mut wrong_patch = valid.clone();
        wrong_patch["per_task_budgets"][0]["patch_max_files"] = json!(999);
        assert!(validate_rwe_corpus_envelope(&wrong_patch).is_err());

        let mut wrong_cancel = valid.clone();
        wrong_cancel["per_task_budgets"][0]["cancel_behavior"] = json!("ignore");
        assert!(validate_rwe_corpus_envelope(&wrong_cancel).is_err());

        let mut wrong_cleanup = valid.clone();
        wrong_cleanup["per_task_budgets"][0]["cleanup_rules"] = json!(["preserve_everything"]);
        assert!(validate_rwe_corpus_envelope(&wrong_cleanup).is_err());
    }

    #[test]
    fn frozen_corpus_rejects_cost_authority_inconsistency() {
        let valid = valid_corpus_body();
        validate_rwe_corpus_envelope(&valid).unwrap();

        // max_cost present without a cost ceiling.
        let mut cost_without_ceiling = valid.clone();
        cost_without_ceiling["per_task_budgets"][0]["max_cost"] = json!(1.0);
        assert!(validate_rwe_corpus_envelope(&cost_without_ceiling).is_err());

        // ceiling present without per-task max_cost.
        let mut ceiling_without_cost = valid.clone();
        ceiling_without_cost["cost_authority"] = json!({
            "kind": "provider_reported",
            "max_cost": 10.0,
            "currency": "USD",
            "monetary_ceiling_enforced": true,
        });
        assert!(validate_rwe_corpus_envelope(&ceiling_without_cost).is_err());

        // per-task costs exceed the aggregate ceiling.
        let mut over_budget = valid.clone();
        over_budget["cost_authority"] = json!({
            "kind": "provider_reported",
            "max_cost": 4.0,
            "currency": "USD",
            "monetary_ceiling_enforced": true,
        });
        for budget in over_budget["per_task_budgets"].as_array_mut().unwrap() {
            budget["max_cost"] = json!(1.0);
        }
        assert!(validate_rwe_corpus_envelope(&over_budget).is_err());

        // consistent ceiling and per-task costs are accepted.
        let mut consistent = valid.clone();
        consistent["cost_authority"] = json!({
            "kind": "provider_reported",
            "max_cost": 10.0,
            "currency": "USD",
            "monetary_ceiling_enforced": true,
        });
        for budget in consistent["per_task_budgets"].as_array_mut().unwrap() {
            budget["max_cost"] = json!(1.0);
        }
        // 5 tasks * 1.0 = 5.0 <= 10.0
        assert!(validate_rwe_corpus_envelope(&consistent).is_ok());
    }

    #[test]
    fn frozen_corpus_rejects_input_output_swap_with_same_total() {
        let valid = valid_corpus_body();
        validate_rwe_corpus_envelope(&valid).unwrap();

        let mut input_up_output_down = valid.clone();
        let b0 = &mut input_up_output_down["per_task_budgets"][0];
        let orig_input = b0["max_input_tokens"].as_u64().unwrap();
        let orig_output = b0["max_output_tokens"].as_u64().unwrap();
        let delta = 1000_u64;
        b0["max_input_tokens"] = json!(orig_input + delta);
        b0["max_output_tokens"] = json!(orig_output - delta);
        assert!(validate_rwe_corpus_envelope(&input_up_output_down).is_err());

        let mut output_up_input_down = valid;
        let b0 = &mut output_up_input_down["per_task_budgets"][0];
        let orig_input = b0["max_input_tokens"].as_u64().unwrap();
        let orig_output = b0["max_output_tokens"].as_u64().unwrap();
        b0["max_input_tokens"] = json!(orig_input - delta);
        b0["max_output_tokens"] = json!(orig_output + delta);
        assert!(validate_rwe_corpus_envelope(&output_up_input_down).is_err());
    }

    #[test]
    fn corpus_fixture_rejects_input_output_total_inconsistency() {
        use crate::rwe::corpus::freeze_first_rwe_corpus_from_root;
        let dir = tempfile::tempdir().unwrap();
        let tasks_dir = dir.path().join("tasks");
        std::fs::create_dir_all(&tasks_dir).unwrap();
        let mut bad = serde_json::from_str::<Value>(
            &std::fs::read_to_string(
                crate::rwe::corpus::default_corpus_fixture_root()
                    .join("tasks/bounded_source_edit.json"),
            )
            .unwrap(),
        )
        .unwrap();
        bad["per_task_max_input_tokens"] = json!(9999);
        bad["per_task_max_output_tokens"] = json!(6000);
        std::fs::write(tasks_dir.join("bounded_source_edit.json"), bad.to_string()).unwrap();
        for name in [
            "controlled_cancellation",
            "docs_code_sync",
            "focused_bug_repair",
            "small_test_addition",
        ] {
            std::fs::copy(
                crate::rwe::corpus::default_corpus_fixture_root()
                    .join(format!("tasks/{name}.json")),
                tasks_dir.join(format!("{name}.json")),
            )
            .unwrap();
        }
        let err = freeze_first_rwe_corpus_from_root(dir.path()).unwrap_err();
        assert!(
            err.contains("per_task_max_input_tokens") || err.contains("per_task_max_total_tokens")
        );
    }

    #[test]
    fn envelope_rejects_admitted_executor_mismatch() {
        let mut body = valid_corpus_body();
        body["admitted_executor"] = json!("rogue-executor");
        let err = validate_rwe_corpus_envelope(&body).unwrap_err();
        assert!(err.contains("admitted_executor"), "{err}");
    }

    #[test]
    fn envelope_rejects_auto_merge_disabled_false() {
        let mut body = valid_corpus_body();
        body["auto_merge_disabled"] = json!(false);
        let err = validate_rwe_corpus_envelope(&body).unwrap_err();
        assert!(err.contains("auto_merge_disabled"), "{err}");
    }
}

/// Validate a production real-RWE authorization body (`rwe_run_authorization.v2`)
/// against the frozen operator corpus, protocol, execution schedule, and the
/// accepted-main SHA at which the artifacts were frozen. Every binding is exact;
/// the embedded snapshot never falls back to the mutable checkout. No table is
/// added: the one-use authorization row remains the sole spend owner.
/// Production wiring (issue/admit) lands in the Golden Path binding PR (Board B);
/// until then this contract is exercised by provider-free tests only.
#[cfg(test)]
pub(crate) fn validate_rwe_run_authorization_v2(
    body: &Value,
    frozen: &crate::rwe::operator_corpus::OperatorFrozenContractSet,
) -> Result<(), String> {
    let corpus = &frozen.corpus;
    let protocol = &frozen.protocol;
    let schedule = &frozen.schedule;
    if body.get("schema_version").and_then(Value::as_str) != Some("rwe_run_authorization.v2") {
        return Err("production RWE authorization must be rwe_run_authorization.v2".into());
    }
    let accepted = required_string_field(body, "accepted_main_sha")?;
    if accepted != frozen.accepted_main_sha {
        return Err("accepted_main_sha does not match the frozen harness main".into());
    }
    let artifact_path = required_string_field(body, "corpus_artifact_path")?;
    if artifact_path != frozen.corpus_artifact_path {
        return Err("corpus_artifact_path does not match the frozen artifact root".into());
    }
    if required_string_field(body, "corpus_sha256")? != corpus.corpus_sha256 {
        return Err("RWE authorization v2 corpus_sha256 does not match frozen corpus".into());
    }
    if required_string_field(body, "protocol_sha256")? != protocol.body_sha256 {
        return Err("RWE authorization v2 protocol_sha256 does not match frozen protocol".into());
    }
    if required_string_field(body, "schedule_sha256")? != schedule.schedule_sha256 {
        return Err("RWE authorization v2 schedule_sha256 does not match frozen schedule".into());
    }
    if body
        .get("golden_path_prerequisite_product_task_id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .is_empty()
    {
        return Err("golden_path_prerequisite_product_task_id required".into());
    }
    if body.get("draft_pr_only").and_then(Value::as_bool) != Some(true)
        || body.get("auto_merge_disabled").and_then(Value::as_bool) != Some(true)
    {
        return Err("draft_pr_only and auto_merge_disabled must be true".into());
    }
    if required_string_field(body, "admitted_executor")? != corpus.admitted_executor {
        return Err("v2 admitted_executor does not match frozen corpus".into());
    }
    if required_string_field(body, "binary_version")? != corpus.admitted_codex_version {
        return Err("v2 binary_version does not match frozen corpus admitted version".into());
    }
    let task_ids = body
        .get("task_ids")
        .and_then(Value::as_array)
        .ok_or("RWE authorization v2 task_ids array required")?;
    let canonical_task_ids = corpus
        .tasks
        .iter()
        .map(|task| task.task_id.as_str())
        .collect::<Vec<_>>();
    let observed_task_ids = task_ids
        .iter()
        .map(|id| id.as_str().ok_or("v2 task_ids must contain strings"))
        .collect::<Result<Vec<_>, _>>()?;
    if observed_task_ids != canonical_task_ids {
        return Err("v2 task_ids must exactly match frozen corpus order".into());
    }
    let auth_executor = required_string_field(body, "executor_identity")?;
    let auth_model = required_string_field(body, "model_identity")?;
    for task in &corpus.tasks {
        if auth_executor != task.executor_identity || auth_model != task.model_identity {
            return Err(
                "v2 executor/model identity does not match frozen corpus task identity".into(),
            );
        }
    }
    if body.get("one_use").and_then(Value::as_bool) != Some(true) {
        return Err("v2 spend envelope must be one_use".into());
    }
    if required_string_field(body, "target_repo")? != corpus.disposable_target_repo {
        return Err("v2 target_repo does not match the frozen operator target repository".into());
    }
    let target_main = required_string_field(body, "target_main_sha")?;
    for task in &corpus.tasks {
        if target_main != task.source_commit {
            return Err("v2 target_main_sha does not match the frozen corpus target main".into());
        }
    }
    let principal_id = required_string_field(body, "principal_id")?;
    let principal_kind = required_string_field(body, "principal_kind")?;
    if principal_id.is_empty() || principal_kind.is_empty() {
        return Err("v2 principal_id and principal_kind are required".into());
    }
    let expires_at = body
        .get("expires_at")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or("v2 expires_at required")?;
    if chrono::DateTime::parse_from_rfc3339(expires_at).is_err() {
        return Err("v2 expires_at must be an RFC3339 instant".into());
    }
    let binary_path = required_string_field(body, "binary_path")?;
    let binary_sha256 = required_string_field(body, "binary_sha256")?;
    if binary_path.is_empty() || binary_sha256.len() != 64 {
        return Err("v2 binary_path/binary_sha256 must bind the admitted executor binary".into());
    }
    let budget_point_ids = body
        .get("budget_point_ids")
        .and_then(Value::as_array)
        .ok_or("v2 budget_point_ids array required")?;
    let observed_budget_points = budget_point_ids
        .iter()
        .map(|id| {
            id.as_str()
                .ok_or("v2 budget_point_ids must contain strings")
        })
        .collect::<Result<Vec<_>, _>>()?;
    let canonical_budget_points = protocol
        .body
        .get("budget_points")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|p| p.get("budget_point_id").and_then(Value::as_str))
                .collect::<Vec<_>>()
        })
        .ok_or("frozen protocol budget_points missing")?;
    if observed_budget_points != canonical_budget_points {
        return Err("v2 budget_point_ids must exactly match frozen protocol budget points".into());
    }
    // Provider route pinned to the frozen managed-deepseek route constants in
    // the accepted codebase (provider::managed_deepseek).
    if required_string_field(body, "provider_kind")? != "deepseek"
        || required_string_field(body, "provider_host")? != "api.deepseek.com"
        || required_string_field(body, "provider_base_url")? != "https://api.deepseek.com"
        || required_string_field(body, "provider_path")? != "/chat/completions"
    {
        return Err("v2 provider route does not match the frozen operator route".into());
    }
    let cost_authority_value = body
        .get("cost_authority")
        .ok_or("v2 cost_authority required")?;
    let cost_authority = super::CostAuthority::from_json(cost_authority_value)?;
    let cost_ceiling = match &cost_authority {
        super::CostAuthority::ProviderReported { max_cost, .. }
        | super::CostAuthority::LocalEstimate { max_cost, .. } => Some(*max_cost),
        super::CostAuthority::CostUnavailable => None,
    };
    let budgets = body
        .get("per_task_budgets")
        .and_then(Value::as_array)
        .ok_or("v2 per_task_budgets array required")?;
    if budgets.len() != corpus.tasks.len() {
        return Err("v2 must contain exactly one budget per corpus task".into());
    }
    let mut aggregate_requests = 0_u64;
    let mut aggregate_tokens = 0_u64;
    let mut aggregate_wall_ms = 0_u64;
    let mut aggregate_cost = 0.0_f64;
    for (budget, task) in budgets.iter().zip(&corpus.tasks) {
        if required_string_field(budget, "task_id")? != task.task_id {
            return Err("v2 per-task budgets must follow frozen corpus order".into());
        }
        let requests = required_u64_field(budget, "max_provider_requests")?;
        let input_tokens = required_u64_field(budget, "max_input_tokens")?;
        let output_tokens = required_u64_field(budget, "max_output_tokens")?;
        let total_tokens = required_u64_field(budget, "max_total_tokens")?;
        let wall_ms = required_u64_field(budget, "max_wall_time_ms")?;
        let retries = required_u64_field(budget, "max_retries")?;
        if requests != task.per_task_max_provider_requests
            || total_tokens != task.per_task_max_total_tokens
            || wall_ms != task.timeout_ms
            || input_tokens != task.per_task_max_input_tokens
            || output_tokens != task.per_task_max_output_tokens
            || input_tokens.saturating_add(output_tokens) != total_tokens
            || retries != task.per_task_max_retries
        {
            return Err(format!(
                "v2 budget does not exactly match frozen corpus task {}",
                task.task_id
            ));
        }
        if required_string_field(budget, "source_repository")? != task.source_repository
            || required_string_field(budget, "source_commit")? != task.source_commit
            || required_string_field(budget, "source_tree_hash")? != task.source_tree_hash
            || required_string_field(budget, "expected_outcome_class")?
                != task.expected_outcome_class
            || required_u64_field(budget, "patch_max_files")? != task.patch_max_files
            || required_u64_field(budget, "patch_max_lines")? != task.patch_max_lines
            || required_string_field(budget, "cancel_behavior")? != task.cancel_behavior
            || required_string_field(budget, "executor_identity")? != task.executor_identity
            || required_string_field(budget, "model_identity")? != task.model_identity
            || required_u64_field(budget, "deterministic_seed")? != task.deterministic_seed
        {
            return Err(format!(
                "v2 budget task-definition boundaries do not match frozen corpus task {}",
                task.task_id
            ));
        }
        if required_string_array_field(budget, "allowed_mutable_paths")?
            != task.allowed_mutable_paths
            || required_string_array_field(budget, "expected_verification_commands")?
                != task.expected_verification_commands
            || required_string_array_field(budget, "cleanup_rules")? != task.cleanup_rules
        {
            return Err(format!(
                "v2 budget task-definition arrays do not match frozen corpus task {}",
                task.task_id
            ));
        }
        match budget
            .get("max_cost")
            .and_then(|v| if v.is_null() { None } else { v.as_f64() })
        {
            Some(max_cost) => {
                if max_cost <= 0.0 {
                    return Err(format!(
                        "v2 budget max_cost must be positive for task {}",
                        task.task_id
                    ));
                }
                if cost_ceiling.is_none() {
                    return Err(format!(
                        "v2 budget max_cost present but cost_authority has no ceiling for task {}",
                        task.task_id
                    ));
                }
                aggregate_cost += max_cost;
            }
            None => {
                if cost_ceiling.is_some() {
                    return Err(format!(
                        "v2 budget missing max_cost for task {} but cost_authority has ceiling",
                        task.task_id
                    ));
                }
            }
        }
        aggregate_requests = aggregate_requests
            .checked_add(requests)
            .ok_or("v2 aggregate provider-request budget overflow")?;
        aggregate_tokens = aggregate_tokens
            .checked_add(total_tokens)
            .ok_or("v2 aggregate token budget overflow")?;
        aggregate_wall_ms = aggregate_wall_ms
            .checked_add(wall_ms)
            .ok_or("v2 aggregate wall-time budget overflow")?;
    }
    // Aggregate totals are bound by the frozen schedule's run-level budget below;
    // the per-task sums are not directly comparable because each task executes
    // once per schedule cell (repetitions x budget points).
    let _ = (aggregate_requests, aggregate_tokens, aggregate_wall_ms);
    if let Some(ceiling) = cost_ceiling {
        if (aggregate_cost - ceiling).abs() > f64::EPSILON {
            return Err("v2 aggregate max_cost does not match cost_authority ceiling".into());
        }
    }
    // The authorization totals must equal the frozen schedule's cell sums (each
    // corpus task executes once per schedule cell); the schedule freeze already
    // proved the cell sums equal the run-level budget.
    let mut cell_requests = 0_u64;
    let mut cell_tokens = 0_u64;
    let mut cell_wall_ms = 0_u64;
    for cell in schedule
        .body
        .get("cells")
        .and_then(Value::as_array)
        .ok_or("frozen schedule cells missing")?
    {
        cell_requests = cell_requests
            .checked_add(
                cell.get("max_provider_requests")
                    .and_then(Value::as_u64)
                    .ok_or("cell max_provider_requests missing")?,
            )
            .ok_or("cell request sum overflow")?;
        cell_tokens = cell_tokens
            .checked_add(
                cell.get("max_total_tokens")
                    .and_then(Value::as_u64)
                    .ok_or("cell max_total_tokens missing")?,
            )
            .ok_or("cell token sum overflow")?;
        cell_wall_ms = cell_wall_ms
            .checked_add(
                cell.get("max_wall_time_ms")
                    .and_then(Value::as_u64)
                    .ok_or("cell max_wall_time_ms missing")?,
            )
            .ok_or("cell wall sum overflow")?;
    }
    if required_u64_field(body, "max_total_provider_requests")? != cell_requests
        || required_u64_field(body, "max_total_tokens")? != cell_tokens
        || required_u64_field(body, "max_wall_time_ms")? != cell_wall_ms
    {
        return Err("v2 totals must equal the frozen schedule cell sums".into());
    }
    let run_level = schedule
        .body
        .get("run_level_budget")
        .and_then(Value::as_object)
        .ok_or("frozen schedule run_level_budget missing")?;
    if run_level
        .get("max_total_provider_requests")
        .and_then(Value::as_u64)
        != Some(cell_requests)
        || run_level.get("max_total_tokens").and_then(Value::as_u64) != Some(cell_tokens)
        || run_level.get("max_wall_time_ms").and_then(Value::as_u64) != Some(cell_wall_ms)
    {
        return Err("v2 totals must equal the frozen schedule run-level budget".into());
    }
    Ok(())
}

#[cfg(test)]
mod operator_v2_authority_tests {
    use super::authority_regression_tests;
    use super::validate_rwe_corpus_envelope;
    use super::validate_rwe_run_authorization_v2;
    use crate::rwe::operator_corpus::{freeze_operator_contract_set, OperatorFrozenContractSet};
    use crate::storage::local_product_store::{CostAuthority, RwePerTaskBudget};
    use serde_json::json;
    use serde_json::Value;

    // Accepted-main SHA the Board-A artifacts are frozen at (updated at merge).
    const ACCEPTED_MAIN_SHA: &str = "3c6cd00f68f4db2a9eef99598deebc42f95ab62b";

    fn frozen() -> OperatorFrozenContractSet {
        freeze_operator_contract_set(ACCEPTED_MAIN_SHA).unwrap()
    }

    fn valid_v2_body(frozen: &OperatorFrozenContractSet) -> Value {
        let task_ids: Vec<Value> = frozen
            .corpus
            .tasks
            .iter()
            .map(|t| json!(t.task_id))
            .collect();
        let budgets: Vec<Value> = frozen
            .corpus
            .tasks
            .iter()
            .map(|t| RwePerTaskBudget::from_task_definition(t, None).to_json())
            .collect();
        let budget_points = frozen
            .protocol
            .body
            .get("budget_points")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let budget_point_ids: Vec<Value> = budget_points
            .iter()
            .map(|p| p.get("budget_point_id").cloned().unwrap_or_default())
            .collect();
        json!({
            "schema_version": "rwe_run_authorization.v2",
            "authorization_id": "v2-live-auth",
            "accepted_main_sha": frozen.accepted_main_sha,
            "corpus_artifact_path": frozen.corpus_artifact_path,
            "corpus_sha256": frozen.corpus.corpus_sha256,
            "protocol_sha256": frozen.protocol.body_sha256,
            "schedule_sha256": frozen.schedule.schedule_sha256,
            "golden_path_prerequisite_product_task_id": "ptask-live-seal-accepted",
            "principal_id": "operator-rwe",
            "principal_kind": "operator_api_key",
            "task_ids": task_ids,
            "max_total_provider_requests": frozen.schedule.body["run_level_budget"]["max_total_provider_requests"].as_u64().unwrap(),
            "max_total_tokens": frozen.schedule.body["run_level_budget"]["max_total_tokens"].as_u64().unwrap(),
            "max_wall_time_ms": frozen.schedule.body["run_level_budget"]["max_wall_time_ms"].as_u64().unwrap(),
            "cost_authority": CostAuthority::CostUnavailable.to_json(),
            "per_task_budgets": budgets,
            "binary_path": "in-process:managed_deepseek",
            "binary_version": frozen.corpus.admitted_codex_version,
            "binary_sha256": "0".repeat(64),
            "provider_kind": "deepseek",
            "provider_host": "api.deepseek.com",
            "provider_base_url": "https://api.deepseek.com",
            "provider_path": "/chat/completions",
            "budget_point_ids": budget_point_ids,
            "target_repo": frozen.corpus.disposable_target_repo,
            "target_main_sha": frozen.corpus.tasks[0].source_commit,
            "executor_identity": frozen.corpus.tasks[0].executor_identity,
            "model_identity": frozen.corpus.tasks[0].model_identity,
            "draft_pr_only": true,
            "admitted_executor": frozen.corpus.admitted_executor,
            "auto_merge_disabled": true,
            "one_use": true,
            "expires_at": "2026-08-07T00:00:00Z",
        })
    }

    #[test]
    fn v2_body_passes_all_frozen_bindings() {
        let frozen = frozen();
        validate_rwe_run_authorization_v2(&valid_v2_body(&frozen), &frozen).unwrap();
    }

    #[test]
    fn v2_body_passes_with_schedule_cost_ceiling() {
        let frozen = frozen();
        let mut body = valid_v2_body(&frozen);
        let run_level = &frozen.schedule.body["run_level_budget"];
        body["cost_authority"] = run_level["cost_authority"].clone();
        let ceiling = run_level["cost_authority"]["max_cost"].as_f64().unwrap();
        let per_task_ceiling = ceiling / frozen.corpus.tasks.len() as f64;
        body["per_task_budgets"] = json!(frozen
            .corpus
            .tasks
            .iter()
            .map(|t| RwePerTaskBudget::from_task_definition(t, Some(per_task_ceiling)).to_json())
            .collect::<Vec<_>>());
        validate_rwe_run_authorization_v2(&body, &frozen).unwrap();
    }

    #[test]
    fn v2_rejects_every_binding_mutation() {
        let frozen = frozen();
        let valid = valid_v2_body(&frozen);

        let mut wrong_schema = valid.clone();
        wrong_schema["schema_version"] = json!("rwe_run_authorization.v1");
        assert!(validate_rwe_run_authorization_v2(&wrong_schema, &frozen).is_err());

        let mut wrong_accepted = valid.clone();
        wrong_accepted["accepted_main_sha"] = json!("a".repeat(40));
        assert!(validate_rwe_run_authorization_v2(&wrong_accepted, &frozen).is_err());

        let mut wrong_path = valid.clone();
        wrong_path["corpus_artifact_path"] = json!("engine/fixtures/rwe/first_corpus/v1");
        assert!(validate_rwe_run_authorization_v2(&wrong_path, &frozen).is_err());

        let mut wrong_corpus = valid.clone();
        wrong_corpus["corpus_sha256"] = json!("0".repeat(64));
        assert!(validate_rwe_run_authorization_v2(&wrong_corpus, &frozen).is_err());

        let mut wrong_protocol = valid.clone();
        wrong_protocol["protocol_sha256"] = json!("0".repeat(64));
        assert!(validate_rwe_run_authorization_v2(&wrong_protocol, &frozen).is_err());

        let mut wrong_schedule = valid.clone();
        wrong_schedule["schedule_sha256"] = json!("0".repeat(64));
        assert!(validate_rwe_run_authorization_v2(&wrong_schedule, &frozen).is_err());

        let mut no_prereq = valid.clone();
        no_prereq["golden_path_prerequisite_product_task_id"] = json!("");
        assert!(validate_rwe_run_authorization_v2(&no_prereq, &frozen).is_err());

        let mut wrong_identity = valid.clone();
        wrong_identity["executor_identity"] = json!("codex_cli");
        assert!(validate_rwe_run_authorization_v2(&wrong_identity, &frozen).is_err());

        let mut wrong_provider = valid.clone();
        wrong_provider["provider_kind"] = json!("openai_compatible");
        assert!(validate_rwe_run_authorization_v2(&wrong_provider, &frozen).is_err());

        let mut wrong_version = valid.clone();
        wrong_version["binary_version"] = json!("0.145.0");
        assert!(validate_rwe_run_authorization_v2(&wrong_version, &frozen).is_err());

        let mut reordered = valid.clone();
        reordered["task_ids"].as_array_mut().unwrap().swap(0, 1);
        assert!(validate_rwe_run_authorization_v2(&reordered, &frozen).is_err());

        let mut wrong_budget = valid.clone();
        wrong_budget["per_task_budgets"][0]["max_retries"] = json!(99);
        assert!(validate_rwe_run_authorization_v2(&wrong_budget, &frozen).is_err());

        let mut wrong_aggregate = valid.clone();
        wrong_aggregate["max_total_tokens"] = json!(1);
        assert!(validate_rwe_run_authorization_v2(&wrong_aggregate, &frozen).is_err());

        let mut auto_merge = valid.clone();
        auto_merge["auto_merge_disabled"] = json!(false);
        assert!(validate_rwe_run_authorization_v2(&auto_merge, &frozen).is_err());

        let mut reuseable = valid.clone();
        reuseable["one_use"] = json!(false);
        assert!(validate_rwe_run_authorization_v2(&reuseable, &frozen).is_err());

        let mut wrong_target_repo = valid.clone();
        wrong_target_repo["target_repo"] = json!("Igzela/some-other-repo");
        assert!(validate_rwe_run_authorization_v2(&wrong_target_repo, &frozen).is_err());

        let mut wrong_target_main = valid.clone();
        wrong_target_main["target_main_sha"] = json!("a".repeat(40));
        assert!(validate_rwe_run_authorization_v2(&wrong_target_main, &frozen).is_err());

        let mut no_budget_points = valid.clone();
        no_budget_points["budget_point_ids"] = json!([]);
        assert!(validate_rwe_run_authorization_v2(&no_budget_points, &frozen).is_err());

        let mut extra_budget_point = valid.clone();
        extra_budget_point["budget_point_ids"] = json!(["bp-standard", "bp-extra"]);
        assert!(validate_rwe_run_authorization_v2(&extra_budget_point, &frozen).is_err());

        let mut wrong_path = valid.clone();
        wrong_path["provider_path"] = json!("/v1/chat/completions");
        assert!(validate_rwe_run_authorization_v2(&wrong_path, &frozen).is_err());

        let mut wrong_base = valid.clone();
        wrong_base["provider_base_url"] = json!("https://api.deepseek.com/v1");
        assert!(validate_rwe_run_authorization_v2(&wrong_base, &frozen).is_err());

        let mut wrong_binary_sha = valid.clone();
        wrong_binary_sha["binary_sha256"] = json!("abc");
        assert!(validate_rwe_run_authorization_v2(&wrong_binary_sha, &frozen).is_err());

        let mut bad_expiry = valid.clone();
        bad_expiry["expires_at"] = json!("not-a-time");
        assert!(validate_rwe_run_authorization_v2(&bad_expiry, &frozen).is_err());

        let mut no_principal = valid.clone();
        no_principal["principal_id"] = json!("");
        assert!(validate_rwe_run_authorization_v2(&no_principal, &frozen).is_err());
    }

    #[test]
    fn v1_envelope_rejects_v2_schema_and_vice_versa() {
        let frozen = frozen();
        // A v2 body can never pass the fixture v1 envelope (strict separation).
        let v2 = valid_v2_body(&frozen);
        let err = validate_rwe_corpus_envelope(&v2).unwrap_err();
        assert!(err.contains("rwe_run_authorization.v1"), "{err}");
        // A v1 fixture body can never pass the v2 validator.
        let v1_body = authority_regression_tests::valid_corpus_body();
        let err2 = validate_rwe_run_authorization_v2(&v1_body, &frozen).unwrap_err();
        assert!(err2.contains("rwe_run_authorization.v2"), "{err2}");
        // Fixture v1 envelope still accepts its own schema.
        validate_rwe_corpus_envelope(&authority_regression_tests::valid_corpus_body()).unwrap();
    }
}
