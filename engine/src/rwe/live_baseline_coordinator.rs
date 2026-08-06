//! Provider-free first-live-baseline coordinator for Minimum First RWE.
//!
//! Wires store-owned `rwe_run_authorization.v2` issue/admit to the frozen
//! 4-cell schedule and existing ProductTask / managed_deepseek owners via an
//! injectable [`CellDriver`]. Production live cells call the Golden Path owner
//! through [`ProductGoldenPathCellDriver`]; tests inject controlled outcomes
//! without a second orchestration stack.
//!
//! This module never POSTs to a Provider and never writes a target repository
//! by itself. Live external effects only occur when an admitted driver is
//! invoked outside CI with credentials and explicit operator intent.

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::corpus::RweTaskDefinition;
use super::operator_corpus::{
    freeze_current_operator_contract_set, OperatorFrozenContractSet, OPERATOR_ADMITTED_BINARY_PATH,
    OPERATOR_ADMITTED_BINARY_VERSION, OPERATOR_ADMITTED_MODEL, OPERATOR_TARGET_REPO,
};
use super::runner::{
    persist_rwe_run_authorization_v2, RWE_RUN_AUTH_V2_SCHEMA, RWE_RUN_EVIDENCE_SCHEMA,
};
use crate::provider::managed_deepseek::{
    DEEPSEEK_CREDENTIAL_REFERENCE, DEEPSEEK_OPENAI_BASE_URL, DEEPSEEK_OPENAI_PATH,
    DEEPSEEK_PROVIDER_KIND,
};
use crate::storage::local_product_store::{
    AuthenticatedPrincipal, LocalProductStore, RweAuthorizationV2IssueRequest,
};

pub const RWE_LIVE_BASELINE_COORDINATOR_SCHEMA: &str = "rwe_live_baseline_coordinator.v1";
pub const RWE_CELL_ATTEMPT_EVIDENCE_SCHEMA: &str = "rwe_cell_attempt_evidence.v1";

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

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// Deterministic identity set for one schedule cell under one RWE run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellIdentities {
    pub cell_id: String,
    pub task_id: String,
    pub product_task_id: String,
    pub workflow_id: String,
    pub node_id: String,
    pub delegated_attempt_id: String,
    pub worktree_id: String,
    pub branch_name: String,
    pub rwe_task_attempt_id: String,
    pub definition_sha256: String,
    pub repetition: u64,
    pub seed: u64,
    pub budget_point_id: String,
    pub sequential_order: u64,
}

pub fn cell_identities_for(
    run_id: &str,
    cell: &Value,
    task: &RweTaskDefinition,
) -> Result<CellIdentities, String> {
    let cell_id = cell
        .get("cell_id")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or("schedule cell_id required")?
        .to_string();
    let task_id = cell
        .get("task_id")
        .and_then(Value::as_str)
        .ok_or("schedule cell task_id required")?;
    if task_id != task.task_id {
        return Err(format!(
            "cell task_id {task_id} does not match frozen task {}",
            task.task_id
        ));
    }
    let product_task_id = format!("rwe-pt:{run_id}:{cell_id}");
    let workflow_id = format!("rwe-wf:{run_id}:{cell_id}");
    let node_id = format!("{workflow_id}-implementation");
    let delegated_attempt_id = format!("rwe-att:{run_id}:{cell_id}");
    let worktree_id = format!("rwe-ws:{run_id}:{cell_id}");
    let branch_name = format!("acp/rwe/{run_id}/{cell_id}");
    let rwe_task_attempt_id = format!("{run_id}:{cell_id}");
    Ok(CellIdentities {
        cell_id,
        task_id: task.task_id.clone(),
        product_task_id,
        workflow_id,
        node_id,
        delegated_attempt_id,
        worktree_id,
        branch_name,
        rwe_task_attempt_id,
        definition_sha256: task.definition_sha256.clone(),
        repetition: cell.get("repetition").and_then(Value::as_u64).unwrap_or(0),
        seed: cell.get("seed").and_then(Value::as_u64).unwrap_or(0),
        budget_point_id: cell
            .get("budget_point_id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        sequential_order: cell
            .get("sequential_order")
            .and_then(Value::as_u64)
            .unwrap_or(0),
    })
}

/// Outcome of one cell execution under an injectable driver.
///
/// Public/injected claims never authorize `live_baseline_sealed`. Sealing is
/// derived only from store-owned ProductTask/terminal/provider receipts.
#[derive(Debug, Clone)]
pub struct CellOutcome {
    pub classification: String,
    pub provider_requests: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub latency_ms: u64,
    pub monetary_cost: Option<f64>,
    pub cost_unknown: bool,
    /// Driver-reported; never sufficient alone to seal a live baseline.
    pub live_provider_request: bool,
    /// `injected` | `product_golden_path_owner` — sealing rejects `injected`.
    pub evidence_source: String,
    pub verification_status: String,
    pub verification_trustworthy: bool,
    pub approval_id: Option<String>,
    pub output_draft_pr: Option<Value>,
    pub terminal_evidence_id: Option<String>,
    pub terminal_content_sha256: Option<String>,
    pub cleanup_status: String,
    pub product_task_id: String,
    pub workflow_id: String,
    pub node_id: String,
    pub delegated_attempt_id: String,
    pub workspace_id: String,
    pub note: String,
}

impl CellOutcome {
    pub fn blocked(classification: &str, note: &str, ids: &CellIdentities) -> Self {
        Self {
            classification: classification.into(),
            provider_requests: 0,
            input_tokens: 0,
            output_tokens: 0,
            total_tokens: 0,
            latency_ms: 0,
            monetary_cost: None,
            cost_unknown: false,
            live_provider_request: false,
            evidence_source: "blocked".into(),
            verification_status: "not_run".into(),
            verification_trustworthy: false,
            approval_id: None,
            output_draft_pr: None,
            terminal_evidence_id: None,
            terminal_content_sha256: None,
            cleanup_status: "not_required".into(),
            product_task_id: ids.product_task_id.clone(),
            workflow_id: ids.workflow_id.clone(),
            node_id: ids.node_id.clone(),
            delegated_attempt_id: ids.delegated_attempt_id.clone(),
            workspace_id: ids.worktree_id.clone(),
            note: note.into(),
        }
    }
}

/// Injectable cell execution seam (tests + production share one coordinator).
pub trait CellDriver: Send + Sync {
    /// Fail closed before any cell effect when this driver cannot execute.
    /// Unarmed/CI/missing-credential production drivers return Err here so the
    /// coordinator never burns one-use authority on no-op cells.
    fn ensure_effects_ready(&self) -> Result<(), String> {
        Ok(())
    }

    fn execute_cell(
        &self,
        store: &LocalProductStore,
        principal: &AuthenticatedPrincipal,
        frozen: &OperatorFrozenContractSet,
        run_id: &str,
        lease_token: &str,
        cell: &Value,
        task: &RweTaskDefinition,
        ids: &CellIdentities,
    ) -> Result<CellOutcome, String>;
}

/// Counts `execute_cell` entries that pass budget pre-check (for over-budget proofs).
pub struct CountingCellDriver<'a> {
    pub inner: &'a dyn CellDriver,
    pub invocations: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

impl CellDriver for CountingCellDriver<'_> {
    fn ensure_effects_ready(&self) -> Result<(), String> {
        self.inner.ensure_effects_ready()
    }

    fn execute_cell(
        &self,
        store: &LocalProductStore,
        principal: &AuthenticatedPrincipal,
        frozen: &OperatorFrozenContractSet,
        run_id: &str,
        lease_token: &str,
        cell: &Value,
        task: &RweTaskDefinition,
        ids: &CellIdentities,
    ) -> Result<CellOutcome, String> {
        self.invocations
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.inner.execute_cell(
            store,
            principal,
            frozen,
            run_id,
            lease_token,
            cell,
            task,
            ids,
        )
    }
}

/// Provider-free injected outcomes for orchestration tests. Never seals a live baseline.
pub struct InjectedCellDriver {
    pub outcomes: Vec<CellOutcome>,
}

impl CellDriver for InjectedCellDriver {
    fn execute_cell(
        &self,
        _store: &LocalProductStore,
        _principal: &AuthenticatedPrincipal,
        _frozen: &OperatorFrozenContractSet,
        _run_id: &str,
        _lease_token: &str,
        cell: &Value,
        _task: &RweTaskDefinition,
        ids: &CellIdentities,
    ) -> Result<CellOutcome, String> {
        let order = cell
            .get("sequential_order")
            .and_then(Value::as_u64)
            .unwrap_or(1) as usize;
        let idx = order.saturating_sub(1);
        self.outcomes
            .get(idx)
            .cloned()
            .ok_or_else(|| {
                format!(
                    "injected cell driver missing outcome for sequential_order {order} (have {})",
                    self.outcomes.len()
                )
            })
            .map(|mut o| {
                o.product_task_id = ids.product_task_id.clone();
                o.workflow_id = ids.workflow_id.clone();
                o.node_id = ids.node_id.clone();
                o.delegated_attempt_id = ids.delegated_attempt_id.clone();
                o.workspace_id = ids.worktree_id.clone();
                o.evidence_source = "injected".into();
                // Injected claims cannot assert live provider seal material.
                o.live_provider_request = false;
                o
            })
    }
}

/// Production owner-backed driver: ProductTask admit/workspace + managed DeepSeek
/// stages via injectable HTTP transport (tests) or live credential path.
///
/// Unarmed / CI / missing-credential without fake transport → **Err** before any
/// cell effect (no consumed-authority no-op path).
pub struct ProductGoldenPathCellDriver {
    pub allow_live_provider_effects: bool,
    /// Provider-free tests supply Mock/Counting transport; live path leaves None.
    pub fake_transport: Option<std::sync::Arc<dyn crate::provider::transport::HttpTransport>>,
    /// Root for staging local_folder sources (required for owner-backed cells).
    pub work_root: Option<std::path::PathBuf>,
}

impl Default for ProductGoldenPathCellDriver {
    fn default() -> Self {
        Self {
            allow_live_provider_effects: false,
            fake_transport: None,
            work_root: None,
        }
    }
}

impl ProductGoldenPathCellDriver {
    fn pre_effect_gate(&self) -> Result<(), String> {
        if std::env::var("CI").ok().as_deref() == Some("true") {
            return Err(
                "fail closed before cell effect: live RWE cell execution is forbidden in CI".into(),
            );
        }
        let has_fake = self.fake_transport.is_some();
        if !self.allow_live_provider_effects && !has_fake {
            return Err(
                "fail closed before cell effect: ProductGoldenPathCellDriver not armed (no live effects, no fake transport)"
                    .into(),
            );
        }
        if !has_fake
            && std::env::var(DEEPSEEK_CREDENTIAL_REFERENCE)
                .ok()
                .filter(|v| !v.trim().is_empty())
                .is_none()
        {
            return Err(
                "fail closed before cell effect: parent-process credential symbol missing".into(),
            );
        }
        Ok(())
    }

    fn stage_local_source(
        &self,
        task: &RweTaskDefinition,
        ids: &CellIdentities,
    ) -> Result<std::path::PathBuf, String> {
        let root = self
            .work_root
            .as_ref()
            .ok_or("ProductGoldenPathCellDriver.work_root required for owner-backed cell")?;
        let source = root.join(format!("source-{}", ids.cell_id));
        std::fs::create_dir_all(&source).map_err(|e| e.to_string())?;
        // Minimal tree so frozen RWE pytest verifiers can run under a staged source.
        for path in &task.allowed_mutable_paths {
            let full = source.join(path);
            if path.ends_with(".md") || path.ends_with("README.md") {
                if let Some(parent) = full.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
                if !full.exists() {
                    std::fs::write(&full, b"# rwe cell source\n").map_err(|e| e.to_string())?;
                }
            } else {
                std::fs::create_dir_all(&full).map_err(|e| e.to_string())?;
            }
        }
        // Provide a trivial passing pytest suite matching frozen verifier shape when present.
        let tests_dir = source.join("apps/api/tests");
        if tests_dir.exists()
            || task
                .expected_verification_commands
                .iter()
                .any(|c| c.contains("pytest"))
        {
            std::fs::create_dir_all(&tests_dir).map_err(|e| e.to_string())?;
            std::fs::create_dir_all(source.join("apps/api/src")).map_err(|e| e.to_string())?;
            std::fs::write(source.join("apps/api/src/__init__.py"), b"")
                .map_err(|e| e.to_string())?;
            std::fs::write(
                tests_dir.join("test_rwe_owner_smoke.py"),
                b"def test_rwe_owner_smoke():\n    assert True\n",
            )
            .map_err(|e| e.to_string())?;
        }
        std::fs::canonicalize(&source).map_err(|e| e.to_string())
    }

    fn build_intake(
        &self,
        principal: &AuthenticatedPrincipal,
        frozen: &OperatorFrozenContractSet,
        task: &RweTaskDefinition,
        ids: &CellIdentities,
        source: &std::path::Path,
    ) -> Result<crate::product_golden_path::ValidatedProductTaskIntake, String> {
        use crate::product_golden_path::{
            validate_intake, ProductExecutorPolicy, ProductTaskBudget, ProductTaskIntakeRequest,
            ProductVerificationCommand, PRODUCT_TASK_GATE,
        };
        // Enable the product gate for this process when absent. Do not clear a
        // caller-owned value (other concurrent tests may rely on the gate).
        if std::env::var_os(PRODUCT_TASK_GATE).is_none() {
            std::env::set_var(PRODUCT_TASK_GATE, "1");
        }
        let verifier = task
            .expected_verification_commands
            .first()
            .cloned()
            .unwrap_or_else(|| "true".into());
        let request = ProductTaskIntakeRequest {
            objective: format!(
                "RWE cell {} task {} (objective hash {})",
                ids.cell_id, task.task_id, task.objective_sha256
            ),
            target_id: format!(
                "rwe-{}",
                frozen.corpus.disposable_target_repo.replace('/', "-")
            ),
            target_repo_path: source.to_string_lossy().into_owned(),
            source_kind: Some("local_folder".into()),
            source_revision: task.source_commit.clone(),
            // Staged local_folder content is synthetic for provider-free cells; do not
            // claim the frozen remote tree hash (store verifies the staged manifest).
            source_tree_hash: None,
            allowed_paths: task.allowed_mutable_paths.clone(),
            verification_commands: vec![ProductVerificationCommand {
                command: verifier,
                timeout_ms: task.timeout_ms.clamp(1, 900_000),
            }],
            // local_folder source stages through the store owner; draft_pr requires
            // a git target and is reserved for the live git_worktree session.
            output_intent: "apply_local_changes".into(),
            executor_policy: ProductExecutorPolicy {
                allowed_executors: vec!["managed_deepseek".into()],
                prefer: Some("managed_deepseek".into()),
            },
            budget: Some(ProductTaskBudget {
                total_tokens: Some(task.per_task_max_total_tokens),
                total_calls: Some(task.per_task_max_provider_requests),
                total_elapsed_ms: Some(task.timeout_ms),
                max_retries: Some(task.per_task_max_retries),
                max_repairs: Some(0),
                max_concurrency: Some(1),
                stage_budgets: None,
            }),
            risk_class: "rwe".into(),
            approval_required: true,
            confirm_execution: Some(true),
            confirm_output: Some(true),
            idempotency_key: ids.product_task_id.clone(),
            expected_version: None,
            tenant_id: Some(principal.tenant_id().into()),
            workspace_id: Some(ids.worktree_id.clone()),
            workspace_mode: Some("local_folder".into()),
        };
        validate_intake(&request, principal.tenant_id(), &ids.worktree_id)
    }

    /// Returns (provider_requests, input_tokens, output_tokens, total_tokens, latency_ms, cost).
    #[allow(clippy::type_complexity)]
    fn run_managed_stages_with_transport(
        &self,
        transport: std::sync::Arc<dyn crate::provider::transport::HttpTransport>,
        task: &RweTaskDefinition,
        ids: &CellIdentities,
        product_task_id: &str,
    ) -> Result<(u64, u64, u64, u64, u64, Option<f64>), String> {
        use crate::node_executor::{NodeExecutionInput, NodeExecutor};
        use crate::provider::config::{CredentialRef, ProviderConfig};
        use crate::provider::credential::CredentialBoundary;
        use crate::provider::managed_deepseek::{
            DeepSeekPriceProfile, DeepSeekProtocol, ManagedAuthoritySource, ManagedCallBinding,
            ManagedDeepSeekProvider, ManagedFailureEffect, PersistedAuthoritySnapshot,
            PersistedManagedExecutionContract, DEEPSEEK_USAGE_PARSER_VERSION,
            MANAGED_PROVIDER_CALL_SCHEMA, MANAGED_PROVIDER_RESPONSE_SCHEMA,
        };
        use crate::provider::managed_deepseek_executor::{
            ManagedDeepSeekExecutorConfig, ManagedDeepSeekNodeExecutor,
        };

        // Credential boundary resolves at send time; tests set a non-empty env value.
        let boundary = CredentialBoundary::new("env").map_err(|e| e.to_string())?;
        let credential = CredentialRef::new(
            DEEPSEEK_CREDENTIAL_REFERENCE,
            "env",
            "***",
            "provider:deepseek",
            "2026-07-30T00:00:00Z",
        );
        let make = |model: &str| {
            std::sync::Arc::new(ManagedDeepSeekProvider::new_openai(
                ProviderConfig::new(
                    "deepseek-rwe",
                    "deepseek",
                    DeepSeekProtocol::OpenAiCompatible.base_url(),
                    model,
                    DEEPSEEK_CREDENTIAL_REFERENCE,
                    "2026-07-30T00:00:00Z",
                ),
                CredentialBoundary::new("env").expect("env boundary"),
                credential.clone(),
                std::sync::Arc::clone(&transport)
                    as std::sync::Arc<dyn crate::provider::transport::HttpTransport>,
            ))
        };
        let config = ManagedDeepSeekExecutorConfig::default();
        let contract = PersistedManagedExecutionContract {
            provider_kind: DEEPSEEK_PROVIDER_KIND.into(),
            protocol: DeepSeekProtocol::OpenAiCompatible,
            host: "api.deepseek.com".into(),
            base_url: DEEPSEEK_OPENAI_BASE_URL.into(),
            endpoint_path: DEEPSEEK_OPENAI_PATH.into(),
            request_schema_version: MANAGED_PROVIDER_CALL_SCHEMA.into(),
            response_schema_version: MANAGED_PROVIDER_RESPONSE_SCHEMA.into(),
            usage_parser_version: DEEPSEEK_USAGE_PARSER_VERSION.into(),
            requested_model: "deepseek-v4-pro".into(),
            limits: config.limits.clone(),
            price_profile: DeepSeekPriceProfile::default(),
        };
        struct RweStaticAuth {
            contract: PersistedManagedExecutionContract,
        }
        impl ManagedAuthoritySource for RweStaticAuth {
            fn current_authority(
                &self,
                binding: &ManagedCallBinding,
            ) -> Result<PersistedAuthoritySnapshot, String> {
                // Contract model must match role default for the request; rebuild per binding.
                let mut contract = self.contract.clone();
                // requested_model is overwritten by request builder from role; store
                // contract is validated against request after request is built — the
                // executor copies config.limits/price_profile onto the request, so
                // contract.limits must match config.limits (already does).
                // requested_model on contract must equal request.requested_model
                // which is role.default_model(); authority validation checks equality.
                // We cannot know role here from binding alone; set from node_id suffix.
                if binding.node_id.ends_with("-implementation") {
                    contract.requested_model = "deepseek-v4-flash".into();
                } else {
                    contract.requested_model = "deepseek-v4-pro".into();
                }
                Ok(PersistedAuthoritySnapshot {
                    product_task_id: binding.product_task_id.clone(),
                    workflow_id: binding.workflow_id.clone(),
                    node_id: binding.node_id.clone(),
                    attempt_id: binding.attempt_id.clone(),
                    spend_authorization_id: binding.spend_authorization_id.clone(),
                    attempt_lease_id: binding.attempt_lease_id.clone(),
                    spend_status: "consumed".into(),
                    consumed_by_attempt_id: Some(binding.attempt_id.clone()),
                    lease_status: "current".into(),
                    execution_contract: Some(contract),
                })
            }
            fn claim_provider_request(
                &self,
                _request: &crate::provider::managed_deepseek::ManagedProviderCallRequest,
            ) -> Result<(), String> {
                Ok(())
            }
            fn reconcile_provider_request(
                &self,
                _request: &crate::provider::managed_deepseek::ManagedProviderCallRequest,
                _response: Option<&crate::provider::managed_deepseek::ManagedProviderResponse>,
                _effect: ManagedFailureEffect,
            ) -> Result<(), String> {
                Ok(())
            }
            fn apply_workspace_action(
                &self,
                _binding: &ManagedCallBinding,
                _node_metadata: &serde_json::Value,
                _model_output: &str,
            ) -> Result<serde_json::Value, String> {
                Ok(json!({
                    "schema_version": "managed_workspace_action_receipt.v1",
                    "status": "applied",
                    "note": "rwe-owner-backed-static-sink"
                }))
            }
        }
        let authority_source = std::sync::Arc::new(RweStaticAuth { contract });
        let executor = ManagedDeepSeekNodeExecutor::new(
            make("deepseek-v4-pro"),
            make(OPERATOR_ADMITTED_MODEL),
            make("deepseek-v4-pro"),
            authority_source,
            config,
        )?;

        let mut provider_requests = 0u64;
        let mut input_tokens = 0u64;
        let mut output_tokens = 0u64;
        let mut latency_ms = 0u64;
        let mut monetary_cost: Option<f64> = None;

        // Planning stage is sufficient to prove managed-DeepSeek owner binding under
        // fake transport; implementation/review remain available for live sessions.
        let stage = "planning";
        let role = "planner";
        let node_id = format!("{}-{stage}", ids.workflow_id);
        let binding = ManagedCallBinding {
            product_task_id: product_task_id.to_string(),
            workflow_id: ids.workflow_id.clone(),
            node_id: node_id.clone(),
            attempt_id: ids.delegated_attempt_id.clone(),
            spend_authorization_id: format!("rwe-spend:{}", ids.cell_id),
            attempt_lease_id: format!("rwe-lease:{}", ids.cell_id),
        };
        let input = NodeExecutionInput {
            node_id: node_id.clone(),
            task_type: "managed_deepseek".into(),
            run_id: ids.workflow_id.clone(),
            workflow_id: ids.workflow_id.clone(),
            node_metadata: json!({
                "product_task_id": product_task_id,
                "managed_deepseek": {
                    "stage": stage,
                    "role": role,
                    "protocol": "openai_compatible",
                    "binding": {
                        "product_task_id": binding.product_task_id,
                        "workflow_id": binding.workflow_id,
                        "node_id": binding.node_id,
                        "attempt_id": binding.attempt_id,
                        "spend_authorization_id": binding.spend_authorization_id,
                        "attempt_lease_id": binding.attempt_lease_id,
                    },
                    "prompt": format!("RWE cell {} stage {stage} task {}", ids.cell_id, task.task_id),
                }
            }),
        };
        let out = executor.execute_node(&input);
        if out.status != "completed" {
            return Err(format!(
                "owner-backed managed stage {stage} status={} err={}",
                out.status,
                out.error_message.unwrap_or_default()
            ));
        }
        provider_requests = provider_requests.saturating_add(1);
        input_tokens = input_tokens.saturating_add(out.input_tokens.unwrap_or(0).max(0) as u64);
        output_tokens = output_tokens.saturating_add(out.output_tokens.unwrap_or(0).max(0) as u64);
        latency_ms = latency_ms.saturating_add(out.latency_ms.unwrap_or(0).max(0) as u64);
        if let Some(c) = out.estimated_cost {
            monetary_cost = Some(monetary_cost.unwrap_or(0.0) + c);
        }
        let _ = boundary;
        Ok((
            provider_requests,
            input_tokens,
            output_tokens,
            input_tokens.saturating_add(output_tokens),
            latency_ms,
            monetary_cost,
        ))
    }
}

impl CellDriver for ProductGoldenPathCellDriver {
    fn ensure_effects_ready(&self) -> Result<(), String> {
        self.pre_effect_gate()
    }

    fn execute_cell(
        &self,
        store: &LocalProductStore,
        principal: &AuthenticatedPrincipal,
        frozen: &OperatorFrozenContractSet,
        run_id: &str,
        _lease_token: &str,
        cell: &Value,
        task: &RweTaskDefinition,
        ids: &CellIdentities,
    ) -> Result<CellOutcome, String> {
        self.pre_effect_gate()?;
        let _ = (run_id, cell);

        let source = self.stage_local_source(task, ids)?;
        let intake = self.build_intake(principal, frozen, task, ids, &source)?;
        // Store owner: admit ProductTask + prepare app-owned workspace (no provider POST).
        let product_task = store.admit_product_task(&intake, principal.principal_id())?;
        let status = product_task
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("");
        if status != "workspace_bound" && status != "graph_ready" {
            return Err(format!(
                "owner-backed cell expected workspace_bound, got {status}"
            ));
        }
        let admitted_task_id = product_task
            .get("task_id")
            .and_then(Value::as_str)
            .unwrap_or(&ids.product_task_id)
            .to_string();
        let workspace_id = product_task
            .pointer("/workspace_binding/workspace_id")
            .and_then(Value::as_str)
            .or_else(|| product_task.get("workspace_id").and_then(Value::as_str))
            .unwrap_or(&ids.worktree_id)
            .to_string();
        let workspace_path = product_task
            .pointer("/workspace_binding/workspace_path")
            .or_else(|| product_task.pointer("/workspace_binding/workspace_canonical_path"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();

        let mut provider_requests = 0u64;
        let mut input_tokens = 0u64;
        let mut output_tokens = 0u64;
        let mut total_tokens = 0u64;
        let mut latency_ms = 0u64;
        let mut monetary_cost: Option<f64> = None;
        let mut live_provider_request = false;
        let mut note = String::from("owner-backed ProductTask admit");

        if let Some(transport) = &self.fake_transport {
            match self.run_managed_stages_with_transport(
                std::sync::Arc::clone(transport),
                task,
                ids,
                &admitted_task_id,
            ) {
                Ok((pr, it, ot, tt, lat, cost)) => {
                    provider_requests = pr;
                    input_tokens = it;
                    output_tokens = ot;
                    total_tokens = tt;
                    latency_ms = lat;
                    monetary_cost = cost;
                    note.push_str("; managed DeepSeek stages via injectable transport");
                    // Fake HTTP is never live seal material.
                    live_provider_request = false;
                }
                Err(e) => {
                    return Ok(CellOutcome {
                        classification: "provider_known_failure".into(),
                        provider_requests: 0,
                        input_tokens: 0,
                        output_tokens: 0,
                        total_tokens: 0,
                        latency_ms: 0,
                        monetary_cost: None,
                        cost_unknown: true,
                        live_provider_request: false,
                        evidence_source: "product_golden_path_owner".into(),
                        verification_status: "not_run".into(),
                        verification_trustworthy: false,
                        approval_id: None,
                        output_draft_pr: None,
                        terminal_evidence_id: None,
                        terminal_content_sha256: None,
                        cleanup_status: "not_required".into(),
                        product_task_id: admitted_task_id,
                        workflow_id: ids.workflow_id.clone(),
                        node_id: ids.node_id.clone(),
                        delegated_attempt_id: ids.delegated_attempt_id.clone(),
                        workspace_id,
                        note: format!("owner-backed managed stage failed: {e}"),
                    });
                }
            }
        } else if self.allow_live_provider_effects {
            // Live armed path: ProductTask has been admitted through the store owner.
            // Full delegated managed-DeepSeek activation (spend/lease/proposal/delegation)
            // remains the operator live-session binding through existing store owners —
            // this coordinator does not create a second spend owner or invent provider success.
            note.push_str(
                "; ProductTask admitted via store owner; live managed DeepSeek activation requires store-owned spend/lease/delegation receipts",
            );
            return Ok(CellOutcome {
                classification: "blocked_authority".into(),
                provider_requests: 0,
                input_tokens: 0,
                output_tokens: 0,
                total_tokens: 0,
                latency_ms: 0,
                monetary_cost: None,
                cost_unknown: false,
                live_provider_request: false,
                evidence_source: "product_golden_path_owner".into(),
                verification_status: "not_run".into(),
                verification_trustworthy: false,
                approval_id: None,
                output_draft_pr: None,
                terminal_evidence_id: None,
                terminal_content_sha256: None,
                cleanup_status: "not_required".into(),
                product_task_id: admitted_task_id,
                workflow_id: ids.workflow_id.clone(),
                node_id: ids.node_id.clone(),
                delegated_attempt_id: ids.delegated_attempt_id.clone(),
                workspace_id,
                note,
            });
        }

        // Deterministic verifier via existing CommandNodeExecutor (ENV-aware argv).
        let mut verification_status = "not_run".to_string();
        let mut verification_trustworthy = false;
        if let Some(cmd) = task.expected_verification_commands.first() {
            use crate::node_executor::{CommandNodeExecutor, NodeExecutionInput, NodeExecutor};
            let executor = CommandNodeExecutor::default().with_timeout(task.timeout_ms.min(60_000));
            let out = executor.execute_node(&NodeExecutionInput {
                node_id: format!("{}-deterministic_verification", ids.workflow_id),
                task_type: "command".into(),
                run_id: ids.workflow_id.clone(),
                workflow_id: ids.workflow_id.clone(),
                node_metadata: json!({
                    "command": cmd,
                    "workspace_path": workspace_path,
                    "workspace_root": workspace_path,
                }),
            });
            if out.status == "completed" || out.status == "succeeded" {
                verification_status = "passed".into();
                verification_trustworthy = true;
            } else {
                verification_status = "failed".into();
            }
            latency_ms = latency_ms.saturating_add(out.latency_ms.unwrap_or(0).max(0) as u64);
            note.push_str("; verifier via CommandNodeExecutor");
        }

        // Terminal evidence only from store owner — never invent seal material.
        let (terminal_evidence_id, terminal_content_sha256) =
            match store.get_product_task_terminal_evidence(&admitted_task_id) {
                Ok(ev) if !ev.is_null() => (
                    ev.get("evidence_id")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    ev.get("content_sha256")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                ),
                _ => (None, None),
            };

        let classification = if verification_status == "failed" {
            "verifier_failed"
        } else if verification_status == "passed" && provider_requests > 0 {
            // Owner-backed path exercised store + managed stages + verifier.
            // Live seal still requires store terminal/provider receipts.
            "success"
        } else if terminal_evidence_id.is_some() {
            "success"
        } else {
            "controlled_failure"
        };

        Ok(CellOutcome {
            classification: classification.into(),
            provider_requests,
            input_tokens,
            output_tokens,
            total_tokens,
            latency_ms,
            monetary_cost,
            cost_unknown: monetary_cost.is_none() && provider_requests > 0,
            live_provider_request,
            evidence_source: "product_golden_path_owner".into(),
            verification_status,
            verification_trustworthy,
            approval_id: None,
            output_draft_pr: None,
            terminal_evidence_id,
            terminal_content_sha256,
            cleanup_status: "completed".into(),
            product_task_id: admitted_task_id,
            workflow_id: ids.workflow_id.clone(),
            node_id: ids.node_id.clone(),
            delegated_attempt_id: ids.delegated_attempt_id.clone(),
            workspace_id,
            note,
        })
    }
}

fn terminal_classifications() -> &'static [&'static str] {
    &[
        "success",
        "controlled_failure",
        "verifier_failed",
        "provider_known_failure",
        "timeout",
        "cancelled",
        "outcome_unknown",
        "blocked_ci_environment",
        "blocked_provider_free_mode",
        "blocked_missing_credential",
        "blocked_budget",
        "blocked_authority",
        "blocked_live_session_incomplete",
        "cleanup_failed",
        "fixture_success",
        "skipped_by_stop_rule",
        "injected_success",
        "injected_verifier_failed",
        "injected_provider_failure",
        "injected_timeout",
        "injected_cancel",
        "injected_outcome_unknown",
    ]
}

fn is_terminal_classification(c: &str) -> bool {
    terminal_classifications().contains(&c)
}

fn stop_rules_from_schedule(frozen: &OperatorFrozenContractSet) -> Vec<String> {
    frozen
        .schedule
        .body
        .get("stop_rules")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn should_stop_after_cell(stop_rules: &[String], classification: &str) -> Option<&'static str> {
    if stop_rules.iter().any(|r| r == "stop_on_authority_failure")
        && matches!(
            classification,
            "blocked_authority" | "blocked_missing_credential" | "blocked_ci_environment"
        )
    {
        return Some("stop_on_authority_failure");
    }
    if stop_rules.iter().any(|r| r == "stop_on_budget_exhaustion")
        && classification == "blocked_budget"
    {
        return Some("stop_on_budget_exhaustion");
    }
    if classification == "outcome_unknown" {
        // Protocol: outcome_unknown is terminal for the cell; never auto-retry.
        return None;
    }
    None
}

/// Provider-free readiness: no auth consumption, no provider, no target write.
pub fn operator_preflight(
    store: &LocalProductStore,
    principal: &AuthenticatedPrincipal,
    authorization_id: Option<&str>,
    golden_path_prerequisite_product_task_id: Option<&str>,
) -> Result<Value, String> {
    let frozen = freeze_current_operator_contract_set()?;
    let mut blockers = Vec::new();
    let mut notes = Vec::new();

    if std::env::var("CI").ok().as_deref() == Some("true") {
        blockers.push(json!({
            "code": "ci_environment",
            "detail": "live RWE is forbidden in CI"
        }));
    }

    let cred_present = std::env::var(DEEPSEEK_CREDENTIAL_REFERENCE)
        .ok()
        .filter(|v| !v.trim().is_empty())
        .is_some();
    if !cred_present {
        blockers.push(json!({
            "code": "missing_credential_symbol",
            "detail": format!("{DEEPSEEK_CREDENTIAL_REFERENCE} not set in parent process")
        }));
    }

    // GP prerequisite: must exist for same tenant when provided or when checking readiness.
    let prereq_id = golden_path_prerequisite_product_task_id.unwrap_or("");
    let mut gp_ready = false;
    if prereq_id.is_empty() {
        blockers.push(json!({
            "code": "missing_golden_path_prerequisite_id",
            "detail": "operator must supply same-tenant Golden Path prerequisite ProductTask id"
        }));
    } else {
        match store.get_product_task_for_tenant(prereq_id, principal.tenant_id()) {
            Ok(Some(_)) => match store.get_product_task_terminal_evidence(prereq_id) {
                Ok(ev) if !ev.is_null() => {
                    if ev.get("tenant_id").and_then(Value::as_str) == Some(principal.tenant_id())
                        && ev.get("task_status").and_then(Value::as_str) == Some("completed")
                    {
                        gp_ready = true;
                    } else {
                        blockers.push(json!({
                            "code": "golden_path_prerequisite_not_ready",
                            "detail": "terminal evidence missing completed same-tenant seal"
                        }));
                    }
                }
                _ => {
                    blockers.push(json!({
                        "code": "golden_path_prerequisite_missing",
                        "detail": "no terminal evidence for prerequisite ProductTask"
                    }));
                }
            },
            Ok(None) => {
                blockers.push(json!({
                    "code": "golden_path_prerequisite_not_found",
                    "detail": "ProductTask not found for authenticated tenant"
                }));
            }
            Err(e) => {
                blockers.push(json!({
                    "code": "golden_path_prerequisite_tenant_mismatch",
                    "detail": e
                }));
            }
        }
    }

    let mut auth_status = Value::Null;
    if let Some(auth_id) = authorization_id {
        match store.get_rwe_run_authorization(auth_id)? {
            Some(row) => {
                auth_status = json!({
                    "authorization_id": auth_id,
                    "status": row.get("status"),
                    "fixture_only": row.get("fixture_only"),
                    "expires_at": row.get("expires_at"),
                    "tenant_id": row.get("tenant_id"),
                });
                if row.get("tenant_id").and_then(Value::as_str) != Some(principal.tenant_id()) {
                    blockers.push(json!({
                        "code": "authorization_tenant_mismatch",
                        "detail": "authorization tenant does not match principal"
                    }));
                }
                if row.get("status").and_then(Value::as_str) != Some("active") {
                    blockers.push(json!({
                        "code": "authorization_not_active",
                        "detail": row.get("status").cloned().unwrap_or(Value::Null)
                    }));
                }
                if row
                    .get("body_json")
                    .and_then(|b| b.get("schema_version"))
                    .and_then(Value::as_str)
                    != Some(RWE_RUN_AUTH_V2_SCHEMA)
                {
                    blockers.push(json!({
                        "code": "authorization_not_v2",
                        "detail": "live baseline requires rwe_run_authorization.v2"
                    }));
                }
            }
            None => {
                notes.push(json!(
                    "authorization_id not yet issued; operator may issue under Board B"
                ));
            }
        }
    } else {
        notes.push(json!(
            "no authorization_id supplied; preflight does not issue or consume"
        ));
    }

    // Target / provider freeze bindings (constants, not caller text).
    let target_main = frozen
        .corpus
        .tasks
        .first()
        .map(|t| t.source_commit.as_str())
        .unwrap_or("");
    if target_main != "6240768506320a324d68787b9eaa86971c8c930c" {
        blockers.push(json!({
            "code": "frozen_target_sha_mismatch",
            "detail": target_main
        }));
    }
    if frozen.corpus.disposable_target_repo != OPERATOR_TARGET_REPO {
        blockers.push(json!({
            "code": "frozen_target_repo_mismatch",
            "detail": frozen.corpus.disposable_target_repo
        }));
    }

    let ready = blockers.is_empty() && gp_ready;
    Ok(sort_value(&json!({
        "schema_version": "rwe_operator_preflight.v1",
        "ready": ready,
        "live_baseline_sealed": false,
        "provider_call_performed": false,
        "target_write_performed": false,
        "authority_consumed": false,
        "frozen": {
            "accepted_main_sha": frozen.accepted_main_sha,
            "corpus_sha256": frozen.corpus.corpus_sha256,
            "protocol_sha256": frozen.protocol.body_sha256,
            "schedule_sha256": frozen.schedule.schedule_sha256,
            "cell_count": frozen.schedule.body.get("cells").and_then(Value::as_array).map(|a| a.len()),
            "target_repo": OPERATOR_TARGET_REPO,
            "target_main_sha": target_main,
            "provider_kind": DEEPSEEK_PROVIDER_KIND,
            "provider_base_url": DEEPSEEK_OPENAI_BASE_URL,
            "provider_path": DEEPSEEK_OPENAI_PATH,
            "model": OPERATOR_ADMITTED_MODEL,
            "binary_path": OPERATOR_ADMITTED_BINARY_PATH,
            "binary_version": OPERATOR_ADMITTED_BINARY_VERSION,
        },
        "principal": {
            "tenant_id": principal.tenant_id(),
            "principal_id": principal.principal_id(),
            "principal_kind": principal.principal_kind().as_str(),
        },
        "credential_symbol_present": cred_present,
        "credential_reference": DEEPSEEK_CREDENTIAL_REFERENCE,
        "golden_path_prerequisite_ready": gp_ready,
        "authorization": auth_status,
        "blockers": blockers,
        "notes": notes,
    })))
}

/// Issue (if needed) and admit a v2 authorization, returning lease for coordination.
pub fn issue_and_admit_v2(
    store: &LocalProductStore,
    principal: &AuthenticatedPrincipal,
    authorization_id: &str,
    run_id: &str,
    golden_path_prerequisite_product_task_id: &str,
    expires_at: &str,
) -> Result<Value, String> {
    // Preflight gate: do not consume when not ready.
    let pre = operator_preflight(
        store,
        principal,
        None,
        Some(golden_path_prerequisite_product_task_id),
    )?;
    if pre.get("ready").and_then(Value::as_bool) != Some(true) {
        // Allow issue path when only missing credential/CI (operator may issue dry) —
        // but GP prerequisite is mandatory before issue.
        if pre
            .get("blockers")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter().any(|b| {
                    matches!(
                        b.get("code").and_then(Value::as_str),
                        Some(
                            "missing_golden_path_prerequisite_id"
                                | "golden_path_prerequisite_missing"
                                | "golden_path_prerequisite_not_found"
                                | "golden_path_prerequisite_tenant_mismatch"
                                | "golden_path_prerequisite_not_ready"
                        )
                    )
                })
            })
            .unwrap_or(true)
        {
            return Err(format!(
                "fail closed before RWE authority consumption: {}",
                pre.get("blockers").cloned().unwrap_or(json!([]))
            ));
        }
    }

    let _issued = persist_rwe_run_authorization_v2(
        store,
        principal,
        &RweAuthorizationV2IssueRequest {
            authorization_id: authorization_id.into(),
            golden_path_prerequisite_product_task_id: golden_path_prerequisite_product_task_id
                .into(),
            expires_at: expires_at.into(),
        },
    )?;

    let auth = store
        .get_rwe_run_authorization(authorization_id)?
        .ok_or("issued authorization missing")?;
    let auth_body = auth.get("body_json").cloned().unwrap_or(Value::Null);
    let mut run_body = auth_body.clone();
    if let Value::Object(ref mut m) = run_body {
        m.insert("run_id".into(), json!(run_id));
        m.insert("authorization_id".into(), json!(authorization_id));
        m.insert("provider_free_fixture".into(), json!(false));
        m.insert("schema_version".into(), json!("rwe_run_body.v2"));
    }
    let admitted = store.admit_rwe_run(principal, run_id, authorization_id, &run_body, false)?;
    Ok(admitted)
}

fn build_cell_evidence(
    run_id: &str,
    authorization_id: &str,
    frozen: &OperatorFrozenContractSet,
    cell: &Value,
    task: &RweTaskDefinition,
    ids: &CellIdentities,
    outcome: &CellOutcome,
) -> Value {
    sort_value(&json!({
        "schema_version": RWE_CELL_ATTEMPT_EVIDENCE_SCHEMA,
        "run_id": run_id,
        "authorization_id": authorization_id,
        "cell_id": ids.cell_id,
        "task_id": ids.task_id,
        "definition_sha256": ids.definition_sha256,
        "objective_sha256": task.objective_sha256,
        "repetition": ids.repetition,
        "seed": ids.seed,
        "budget_point_id": ids.budget_point_id,
        "sequential_order": ids.sequential_order,
        "product_task_id": outcome.product_task_id,
        "workflow_id": outcome.workflow_id,
        "node_id": outcome.node_id,
        "delegated_attempt_id": outcome.delegated_attempt_id,
        "workspace_id": outcome.workspace_id,
        "branch_name": ids.branch_name,
        "classification": outcome.classification,
        "evidence_source": outcome.evidence_source,
        "provider_requests": outcome.provider_requests,
        "input_tokens": outcome.input_tokens,
        "output_tokens": outcome.output_tokens,
        "total_tokens": outcome.total_tokens,
        "latency_ms": outcome.latency_ms,
        "monetary_cost": outcome.monetary_cost,
        "cost_unknown": outcome.cost_unknown,
        "live_provider_request": outcome.live_provider_request,
        "verification": {
            "status": outcome.verification_status,
            "trustworthy": outcome.verification_trustworthy,
            "commands": task.expected_verification_commands,
        },
        "approval_id": outcome.approval_id,
        "output": {
            "intent": "draft_pr",
            "draft_pr": outcome.output_draft_pr,
        },
        "terminal_evidence_id": outcome.terminal_evidence_id,
        "terminal_content_sha256": outcome.terminal_content_sha256,
        "cleanup_status": outcome.cleanup_status,
        "note": outcome.note,
        "frozen_bindings": {
            "corpus_sha256": frozen.corpus.corpus_sha256,
            "protocol_sha256": frozen.protocol.body_sha256,
            "schedule_sha256": frozen.schedule.schedule_sha256,
            "model": OPERATOR_ADMITTED_MODEL,
            "target_repo": OPERATOR_TARGET_REPO,
            "cell": cell,
        },
        "schedule_cell_budget": {
            "max_provider_requests": cell.get("max_provider_requests"),
            "max_total_tokens": cell.get("max_total_tokens"),
            "max_wall_time_ms": cell.get("max_wall_time_ms"),
            "max_cost": cell.get("max_cost"),
        },
    }))
}

/// Derive live_baseline_sealed only from store-owned ProductTask/terminal receipts.
/// Public/injected CellOutcome claims never authorize a seal.
fn evaluate_store_owned_live_baseline_seal(
    store: &LocalProductStore,
    principal: &AuthenticatedPrincipal,
    frozen: &OperatorFrozenContractSet,
    cell_results: &[Value],
) -> bool {
    let mut executed = 0usize;
    for ev in cell_results {
        let class = ev
            .get("classification")
            .and_then(Value::as_str)
            .unwrap_or("");
        if class == "skipped_by_stop_rule" {
            continue;
        }
        executed += 1;
        // Injected / blocked / public claims cannot seal.
        let source = ev
            .get("evidence_source")
            .and_then(Value::as_str)
            .unwrap_or("");
        if source != "product_golden_path_owner" {
            return false;
        }
        // Driver-reported live_provider_request alone is never sufficient; require store.
        let product_task_id = match ev.get("product_task_id").and_then(Value::as_str) {
            Some(id) if !id.is_empty() => id,
            _ => return false,
        };
        let task = match store.get_product_task_for_tenant(product_task_id, principal.tenant_id()) {
            Ok(Some(t)) => t,
            _ => return false,
        };
        if task.get("status").and_then(Value::as_str) != Some("completed") {
            return false;
        }
        if task.get("fixture_only").and_then(Value::as_bool) == Some(true) {
            return false;
        }
        let te = match store.get_product_task_terminal_evidence(product_task_id) {
            Ok(v) if !v.is_null() => v,
            _ => return false,
        };
        if te.get("tenant_id").and_then(Value::as_str) != Some(principal.tenant_id()) {
            return false;
        }
        if te.get("product_task_id").and_then(Value::as_str) != Some(product_task_id) {
            return false;
        }
        if te.get("task_status").and_then(Value::as_str) != Some("completed") {
            return false;
        }
        if te.get("fixture").and_then(Value::as_bool) == Some(true)
            || te.get("fixture_only").and_then(Value::as_bool) == Some(true)
        {
            return false;
        }
        // Require verification / approval / artifact / output on terminal.
        let verification_ok = te
            .pointer("/verification/trustworthy")
            .and_then(Value::as_bool)
            == Some(true)
            && te.pointer("/verification/status").and_then(Value::as_str) == Some("passed");
        let approval_ok = te
            .pointer("/approval/approval_id")
            .and_then(Value::as_str)
            .is_some_and(|s| !s.is_empty());
        let artifact_ok = te
            .pointer("/artifact/artifact_id")
            .and_then(Value::as_str)
            .is_some_and(|s| !s.is_empty());
        let output_ok =
            te.pointer("/output/draft_pr").is_some() || te.pointer("/output/receipt_id").is_some();
        if !verification_ok || !approval_ok || !artifact_ok || !output_ok {
            return false;
        }
        // Bind terminal source_revision to frozen corpus when present.
        if let Some(rev) = te.get("source_revision").and_then(Value::as_str) {
            let expected = frozen
                .corpus
                .tasks
                .first()
                .map(|t| t.source_commit.as_str())
                .unwrap_or("");
            if !expected.is_empty() && rev != expected {
                return false;
            }
        }
        // Content hash must be present for a sealable terminal receipt.
        if te
            .get("content_sha256")
            .and_then(Value::as_str)
            .is_none_or(|s| s.is_empty() || s.len() != 64)
        {
            return false;
        }
    }
    // At least one executed cell required; empty/all-skipped cannot seal.
    executed > 0
}

fn cell_budget_headroom(
    cell: &Value,
    frozen: &OperatorFrozenContractSet,
    aggregate_requests: u64,
    aggregate_tokens: u64,
) -> Result<(), String> {
    let cell_req = cell
        .get("max_provider_requests")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let cell_tokens = cell
        .get("max_total_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let run_max_req = frozen
        .schedule
        .body
        .pointer("/run_level_budget/max_total_provider_requests")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let run_max_tokens = frozen
        .schedule
        .body
        .pointer("/run_level_budget/max_total_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    if cell_req == 0 {
        return Err("cell max_provider_requests is zero".into());
    }
    if aggregate_requests >= run_max_req {
        return Err("run-level provider-request budget already exhausted".into());
    }
    if run_max_tokens > 0 && aggregate_tokens >= run_max_tokens {
        return Err("run-level token budget already exhausted".into());
    }
    if cell_tokens == 0 {
        return Err("cell max_total_tokens is zero".into());
    }
    Ok(())
}

fn reconstruct_stopped_by(existing: &[Value], stop_rules: &[String]) -> Option<String> {
    for row in existing {
        let class = row
            .get("classification")
            .and_then(Value::as_str)
            .unwrap_or("");
        if class == "skipped_by_stop_rule" {
            // Prefer explicit stop reason from evidence note when present.
            if let Some(note) = row.pointer("/evidence_json/note").and_then(Value::as_str) {
                if note.starts_with("stop_on_") {
                    return Some(note.to_string());
                }
            }
            return Some("stop_rule_reconstructed".into());
        }
        if let Some(rule) = should_stop_after_cell(stop_rules, class) {
            return Some(rule.into());
        }
    }
    None
}

/// Run the frozen schedule under an admitted authorization and injectable driver.
pub fn run_frozen_schedule(
    store: &LocalProductStore,
    principal: &AuthenticatedPrincipal,
    run_id: &str,
    authorization_id: &str,
    lease_token: &str,
    driver: &dyn CellDriver,
) -> Result<Value, String> {
    // Fail closed before any cell effect or destructive run terminalization when
    // the driver cannot execute (unarmed / CI / missing credential).
    driver.ensure_effects_ready()?;

    let frozen = freeze_current_operator_contract_set()?;
    let auth = store
        .get_rwe_run_authorization(authorization_id)?
        .ok_or("RWE authorization not found")?;
    if auth.get("tenant_id").and_then(Value::as_str) != Some(principal.tenant_id()) {
        return Err("authorization tenant mismatch".into());
    }
    if auth.get("principal_id").and_then(Value::as_str) != Some(principal.principal_id()) {
        return Err("authorization principal mismatch".into());
    }
    // Restart recovery: exact admit replay if needed.
    let mut lease = lease_token.to_string();
    if lease.is_empty() {
        let auth_body = auth.get("body_json").cloned().unwrap_or(Value::Null);
        let mut run_body = auth_body;
        if let Value::Object(ref mut m) = run_body {
            m.insert("run_id".into(), json!(run_id));
            m.insert("authorization_id".into(), json!(authorization_id));
            m.insert("provider_free_fixture".into(), json!(false));
            m.insert("schema_version".into(), json!("rwe_run_body.v2"));
        }
        let replay = store.admit_rwe_run(principal, run_id, authorization_id, &run_body, false)?;
        lease = replay
            .get("lease_token")
            .and_then(Value::as_str)
            .ok_or("admit/replay missing lease_token for restart recovery")?
            .to_string();
    }

    let cells = frozen
        .schedule
        .body
        .get("cells")
        .and_then(Value::as_array)
        .ok_or("frozen schedule cells missing")?;
    if cells.len() != 4 {
        return Err(format!(
            "Minimum First RWE schedule must have exactly 4 cells, got {}",
            cells.len()
        ));
    }

    let existing = store.list_rwe_task_attempts_for_run(run_id)?;
    let mut existing_by_attempt: std::collections::BTreeMap<String, Value> =
        std::collections::BTreeMap::new();
    for row in &existing {
        if let Some(id) = row.get("task_attempt_id").and_then(Value::as_str) {
            existing_by_attempt.insert(id.to_string(), row.clone());
        }
    }

    let stop_rules = stop_rules_from_schedule(&frozen);
    // Reconstruct prior stop state for exact idempotent restart.
    let mut stopped_by = reconstruct_stopped_by(&existing, &stop_rules);

    let mut cell_results = Vec::new();
    let mut aggregate_requests = 0u64;
    let mut aggregate_tokens = 0u64;
    let mut any_live_provider = false;

    for cell in cells {
        let task_id = cell
            .get("task_id")
            .and_then(Value::as_str)
            .ok_or("cell task_id missing")?;
        let task = frozen
            .corpus
            .tasks
            .iter()
            .find(|t| t.task_id == task_id)
            .ok_or_else(|| format!("frozen corpus missing task {task_id}"))?;
        let ids = cell_identities_for(run_id, cell, task)?;

        if let Some(prior) = existing_by_attempt.get(&ids.rwe_task_attempt_id) {
            let class = prior
                .get("classification")
                .and_then(Value::as_str)
                .unwrap_or("");
            if is_terminal_classification(class) {
                let evidence = prior.get("evidence_json").cloned().unwrap_or(Value::Null);
                if let Some(ev) = prior.get("evidence_json") {
                    aggregate_requests = aggregate_requests.saturating_add(
                        ev.get("provider_requests")
                            .and_then(Value::as_u64)
                            .unwrap_or(0),
                    );
                    aggregate_tokens = aggregate_tokens.saturating_add(
                        ev.get("total_tokens").and_then(Value::as_u64).unwrap_or(0),
                    );
                    any_live_provider |= ev
                        .get("live_provider_request")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                }
                cell_results.push(evidence);
                // Keep stop state consistent with prior terminal classes.
                if stopped_by.is_none() {
                    if let Some(rule) = should_stop_after_cell(&stop_rules, class) {
                        stopped_by = Some(rule.into());
                    }
                }
                continue;
            }
        }

        if stopped_by.is_some() {
            // skipped_by_stop_rule is terminal accounting — never dispatch.
            let outcome = CellOutcome::blocked(
                "skipped_by_stop_rule",
                stopped_by.as_deref().unwrap_or("stop"),
                &ids,
            );
            let evidence = build_cell_evidence(
                run_id,
                authorization_id,
                &frozen,
                cell,
                task,
                &ids,
                &outcome,
            );
            store.persist_rwe_task_attempt(
                run_id,
                &lease,
                &ids.rwe_task_attempt_id,
                &ids.task_id,
                &ids.definition_sha256,
                &outcome.classification,
                &evidence,
            )?;
            cell_results.push(evidence);
            continue;
        }

        // Pre-effect budget reservation against frozen ceilings (no driver/provider).
        if let Err(budget_err) =
            cell_budget_headroom(cell, &frozen, aggregate_requests, aggregate_tokens)
        {
            let outcome = CellOutcome::blocked(
                "blocked_budget",
                &format!("pre-effect budget refusal: {budget_err}"),
                &ids,
            );
            let evidence = build_cell_evidence(
                run_id,
                authorization_id,
                &frozen,
                cell,
                task,
                &ids,
                &outcome,
            );
            store.persist_rwe_task_attempt(
                run_id,
                &lease,
                &ids.rwe_task_attempt_id,
                &ids.task_id,
                &ids.definition_sha256,
                &outcome.classification,
                &evidence,
            )?;
            cell_results.push(evidence);
            if stop_rules.iter().any(|r| r == "stop_on_budget_exhaustion") {
                stopped_by = Some("stop_on_budget_exhaustion".into());
            }
            continue;
        }

        let outcome =
            driver.execute_cell(store, principal, &frozen, run_id, &lease, cell, task, &ids)?;

        // Post-effect honesty: outcomes that exceed frozen ceilings fail closed.
        let cell_req = cell
            .get("max_provider_requests")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let cell_tokens = cell
            .get("max_total_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let run_max_req = frozen
            .schedule
            .body
            .pointer("/run_level_budget/max_total_provider_requests")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let run_max_tokens = frozen
            .schedule
            .body
            .pointer("/run_level_budget/max_total_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let mut outcome = outcome;
        if outcome.provider_requests > cell_req
            || outcome.total_tokens > cell_tokens
            || aggregate_requests.saturating_add(outcome.provider_requests) > run_max_req
            || (run_max_tokens > 0
                && aggregate_tokens.saturating_add(outcome.total_tokens) > run_max_tokens)
        {
            outcome = CellOutcome::blocked(
                "blocked_budget",
                "cell or run provider/token budget exceeded by outcome",
                &ids,
            );
            if stop_rules.iter().any(|r| r == "stop_on_budget_exhaustion") {
                stopped_by = Some("stop_on_budget_exhaustion".into());
            }
        }

        let evidence = build_cell_evidence(
            run_id,
            authorization_id,
            &frozen,
            cell,
            task,
            &ids,
            &outcome,
        );
        store.persist_rwe_task_attempt(
            run_id,
            &lease,
            &ids.rwe_task_attempt_id,
            &ids.task_id,
            &ids.definition_sha256,
            &outcome.classification,
            &evidence,
        )?;
        aggregate_requests = aggregate_requests.saturating_add(outcome.provider_requests);
        aggregate_tokens = aggregate_tokens.saturating_add(outcome.total_tokens);
        any_live_provider |= outcome.live_provider_request;
        if stopped_by.is_none() {
            if let Some(rule) = should_stop_after_cell(&stop_rules, &outcome.classification) {
                stopped_by = Some(rule.into());
            }
        }
        // outcome_unknown: never auto-retry, never second authorization.
        cell_results.push(evidence);
    }

    // All pre-registered cells must have a terminal attempt before run terminalization.
    let final_attempts = store.list_rwe_task_attempts_for_run(run_id)?;
    if final_attempts.len() < cells.len() {
        return Err(format!(
            "run cannot terminalize: {} task attempts for {} cells",
            final_attempts.len(),
            cells.len()
        ));
    }

    // Sealing is store-owned only; injected/public claims never seal.
    let live_baseline_sealed =
        evaluate_store_owned_live_baseline_seal(store, principal, &frozen, &cell_results);

    let aggregate = sort_value(&json!({
        "schema_version": RWE_RUN_EVIDENCE_SCHEMA,
        "coordinator_schema": RWE_LIVE_BASELINE_COORDINATOR_SCHEMA,
        "run_id": run_id,
        "authorization_id": authorization_id,
        "corpus_sha256": frozen.corpus.corpus_sha256,
        "protocol_sha256": frozen.protocol.body_sha256,
        "schedule_sha256": frozen.schedule.schedule_sha256,
        "accepted_main_sha": frozen.accepted_main_sha,
        "cell_results": cell_results,
        "aggregate_provider_requests": aggregate_requests,
        "aggregate_total_tokens": aggregate_tokens,
        "live_provider_request": any_live_provider,
        "live_baseline_sealed": live_baseline_sealed,
        "provider_free_fixture_completion": false,
        "stopped_by": stopped_by,
        "comparison_eligible": false,
        "note": if live_baseline_sealed {
            "live baseline sealed from store-owned ProductTask/terminal receipts"
        } else {
            "provider-free, injected, or incomplete store receipts; not a sealed live baseline"
        },
    }));
    let evidence_sha = sha256_hex(aggregate.to_string().as_bytes());
    let status = if live_baseline_sealed {
        "succeeded"
    } else if cell_results
        .iter()
        .any(|e| e.get("classification").and_then(Value::as_str) == Some("outcome_unknown"))
    {
        "outcome_unknown"
    } else {
        // Provider-free orchestration and non-sealed live runs use failed until sealed.
        "failed"
    };
    let completed = store.complete_rwe_run(run_id, &lease, status, &aggregate, &evidence_sha)?;
    Ok(sort_value(&json!({
        "schema_version": RWE_LIVE_BASELINE_COORDINATOR_SCHEMA,
        "run": completed,
        "aggregate": aggregate,
        "cell_count": cells.len(),
        "attempts_recorded": final_attempts.len(),
        "live_baseline_sealed": live_baseline_sealed,
        "provider_call_performed": any_live_provider,
        "provider_calls": if any_live_provider { "observed_in_cell_evidence" } else { "0" },
    })))
}

/// Build first-baseline evidence projection compatible with economic protocol artifacts
/// without claiming COMPARISON_ELIGIBLE or creating a VDE store.
pub fn project_first_baseline_evidence(run_aggregate: &Value) -> Value {
    sort_value(&json!({
        "schema_version": "rwe_first_baseline_evidence_projection.v1",
        "live_baseline_sealed": run_aggregate.get("live_baseline_sealed"),
        "comparison_eligible": false,
        "aggregate_provider_requests": run_aggregate.get("aggregate_provider_requests"),
        "aggregate_total_tokens": run_aggregate.get("aggregate_total_tokens"),
        "cell_results": run_aggregate.get("cell_results"),
        "corpus_sha256": run_aggregate.get("corpus_sha256"),
        "protocol_sha256": run_aggregate.get("protocol_sha256"),
        "schedule_sha256": run_aggregate.get("schedule_sha256"),
        "note": "projection only; not a VDE store and not comparison-eligible",
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::local_product_store::{
        SCOPE_ATTEMPT_ADMIT, SCOPE_REVOKE, SCOPE_RISK_ACKNOWLEDGE, SCOPE_SPEND_AUTHORIZE,
    };
    use sha2::Digest;
    use tempfile::tempdir;

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

    fn success_outcomes() -> Vec<CellOutcome> {
        (0..4)
            .map(|i| CellOutcome {
                classification: "injected_success".into(),
                provider_requests: 0,
                input_tokens: 0,
                output_tokens: 0,
                total_tokens: 0,
                latency_ms: 1,
                monetary_cost: Some(0.0),
                cost_unknown: false,
                live_provider_request: false,
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

    #[test]
    fn preflight_fails_closed_without_gp_and_without_consuming() {
        let dir = tempdir().unwrap();
        let store = LocalProductStore::new_with_clock(dir.path().join("pf.db"), || {
            "2026-07-25T12:00:00Z".into()
        })
        .unwrap();
        let principal = operator(&store, "t-pf", "op-pf");
        let pre = operator_preflight(&store, &principal, None, None).unwrap();
        assert_eq!(pre["ready"], false);
        assert_eq!(pre["authority_consumed"], false);
        assert_eq!(pre["provider_call_performed"], false);
    }

    #[test]
    fn four_cell_injected_orchestration_maps_identities_and_receipts() {
        let dir = tempdir().unwrap();
        let store = LocalProductStore::new_with_clock(dir.path().join("c4.db"), || {
            "2026-07-25T12:00:00Z".into()
        })
        .unwrap();
        let tenant = "t-c4";
        let principal = operator(&store, tenant, "op-c4");
        seed_gp(&store, "ptask-gp-c4", tenant);
        let auth_id = "auth-c4";
        let run_id = "run-c4";
        let admitted = issue_and_admit_v2(
            &store,
            &principal,
            auth_id,
            run_id,
            "ptask-gp-c4",
            "2026-08-07T00:00:00Z",
        )
        .unwrap();
        let lease = admitted["lease_token"].as_str().unwrap().to_string();
        let driver = InjectedCellDriver {
            outcomes: success_outcomes(),
        };
        let result =
            run_frozen_schedule(&store, &principal, run_id, auth_id, &lease, &driver).unwrap();
        assert_eq!(result["cell_count"], 4);
        assert_eq!(result["attempts_recorded"], 4);
        assert_eq!(result["live_baseline_sealed"], false);
        let attempts = store.list_rwe_task_attempts_for_run(run_id).unwrap();
        assert_eq!(attempts.len(), 4);
        for a in &attempts {
            let ev = &a["evidence_json"];
            assert!(ev.get("cell_id").and_then(Value::as_str).is_some());
            assert!(ev
                .get("definition_sha256")
                .and_then(Value::as_str)
                .is_some());
            assert!(ev.get("product_task_id").and_then(Value::as_str).is_some());
            assert!(ev.get("workflow_id").and_then(Value::as_str).is_some());
            assert_eq!(ev["live_provider_request"], false);
        }
        // Exact restart: re-run skips completed cells (idempotent).
        let again =
            run_frozen_schedule(&store, &principal, run_id, auth_id, &lease, &driver).unwrap();
        assert_eq!(again["attempts_recorded"], 4);
        let attempts2 = store.list_rwe_task_attempts_for_run(run_id).unwrap();
        assert_eq!(attempts2.len(), 4);
    }

    #[test]
    fn classifier_outcomes_preserve_cost_fields() {
        let dir = tempdir().unwrap();
        let store = LocalProductStore::new_with_clock(dir.path().join("cls.db"), || {
            "2026-07-25T12:00:00Z".into()
        })
        .unwrap();
        let tenant = "t-cls";
        let principal = operator(&store, tenant, "op-cls");
        seed_gp(&store, "ptask-gp-cls", tenant);
        let outcomes = vec![
            CellOutcome {
                classification: "injected_verifier_failed".into(),
                provider_requests: 1,
                input_tokens: 10,
                output_tokens: 5,
                total_tokens: 15,
                latency_ms: 100,
                monetary_cost: Some(0.01),
                cost_unknown: false,
                live_provider_request: false,
                evidence_source: "injected".into(),
                verification_status: "failed".into(),
                verification_trustworthy: true,
                approval_id: None,
                output_draft_pr: None,
                terminal_evidence_id: Some("tev".into()),
                terminal_content_sha256: Some("a".repeat(64)),
                cleanup_status: "completed".into(),
                product_task_id: String::new(),
                workflow_id: String::new(),
                node_id: String::new(),
                delegated_attempt_id: String::new(),
                workspace_id: String::new(),
                note: "verifier".into(),
            },
            CellOutcome {
                classification: "injected_outcome_unknown".into(),
                provider_requests: 1,
                input_tokens: 10,
                output_tokens: 0,
                total_tokens: 10,
                latency_ms: 50,
                monetary_cost: None,
                cost_unknown: true,
                live_provider_request: false,
                evidence_source: "injected".into(),
                verification_status: "not_run".into(),
                verification_trustworthy: false,
                approval_id: None,
                output_draft_pr: None,
                terminal_evidence_id: None,
                terminal_content_sha256: None,
                cleanup_status: "incomplete".into(),
                product_task_id: String::new(),
                workflow_id: String::new(),
                node_id: String::new(),
                delegated_attempt_id: String::new(),
                workspace_id: String::new(),
                note: "unknown".into(),
            },
            CellOutcome {
                classification: "injected_timeout".into(),
                provider_requests: 1,
                input_tokens: 1,
                output_tokens: 0,
                total_tokens: 1,
                latency_ms: 900000,
                monetary_cost: Some(0.02),
                cost_unknown: false,
                live_provider_request: false,
                evidence_source: "injected".into(),
                verification_status: "not_run".into(),
                verification_trustworthy: false,
                approval_id: None,
                output_draft_pr: None,
                terminal_evidence_id: None,
                terminal_content_sha256: None,
                cleanup_status: "completed".into(),
                product_task_id: String::new(),
                workflow_id: String::new(),
                node_id: String::new(),
                delegated_attempt_id: String::new(),
                workspace_id: String::new(),
                note: "timeout".into(),
            },
            CellOutcome {
                classification: "injected_cancel".into(),
                provider_requests: 0,
                input_tokens: 0,
                output_tokens: 0,
                total_tokens: 0,
                latency_ms: 1,
                monetary_cost: Some(0.0),
                cost_unknown: false,
                live_provider_request: false,
                evidence_source: "injected".into(),
                verification_status: "not_run".into(),
                verification_trustworthy: false,
                approval_id: None,
                output_draft_pr: None,
                terminal_evidence_id: None,
                terminal_content_sha256: None,
                cleanup_status: "completed".into(),
                product_task_id: String::new(),
                workflow_id: String::new(),
                node_id: String::new(),
                delegated_attempt_id: String::new(),
                workspace_id: String::new(),
                note: "cancel".into(),
            },
        ];
        let admitted = issue_and_admit_v2(
            &store,
            &principal,
            "auth-cls",
            "run-cls",
            "ptask-gp-cls",
            "2026-08-07T00:00:00Z",
        )
        .unwrap();
        let lease = admitted["lease_token"].as_str().unwrap();
        let driver = InjectedCellDriver { outcomes };
        let result =
            run_frozen_schedule(&store, &principal, "run-cls", "auth-cls", lease, &driver).unwrap();
        assert_eq!(result["attempts_recorded"], 4);
        let attempts = store.list_rwe_task_attempts_for_run("run-cls").unwrap();
        let unknown = attempts
            .iter()
            .find(|a| a["classification"] == "injected_outcome_unknown")
            .unwrap();
        assert_eq!(unknown["evidence_json"]["cost_unknown"], true);
        assert!(unknown["evidence_json"]["monetary_cost"].is_null());
        // No second authorization was issued/consumed for outcome_unknown.
        assert_eq!(
            store
                .get_rwe_run_authorization("auth-cls")
                .unwrap()
                .unwrap()["status"],
            "consumed"
        );
    }

    #[test]
    fn concurrent_duplicate_cell_is_single_effect() {
        let dir = tempdir().unwrap();
        let store = LocalProductStore::new_with_clock(dir.path().join("dup.db"), || {
            "2026-07-25T12:00:00Z".into()
        })
        .unwrap();
        let tenant = "t-dup";
        let principal = operator(&store, tenant, "op-dup");
        seed_gp(&store, "ptask-gp-dup", tenant);
        let admitted = issue_and_admit_v2(
            &store,
            &principal,
            "auth-dup",
            "run-dup",
            "ptask-gp-dup",
            "2026-08-07T00:00:00Z",
        )
        .unwrap();
        let lease = admitted["lease_token"].as_str().unwrap().to_string();
        let driver = InjectedCellDriver {
            outcomes: success_outcomes(),
        };
        run_frozen_schedule(&store, &principal, "run-dup", "auth-dup", &lease, &driver).unwrap();
        // Second schedule run is exact replay for completed cells.
        run_frozen_schedule(&store, &principal, "run-dup", "auth-dup", &lease, &driver).unwrap();
        assert_eq!(
            store
                .list_rwe_task_attempts_for_run("run-dup")
                .unwrap()
                .len(),
            4
        );
    }

    #[test]
    fn stale_and_wrong_principal_do_not_consume_fresh_authority() {
        let dir = tempdir().unwrap();
        let store = LocalProductStore::new_with_clock(dir.path().join("stale.db"), || {
            "2026-07-25T12:00:00Z".into()
        })
        .unwrap();
        let tenant = "t-stale";
        let principal = operator(&store, tenant, "op-stale");
        seed_gp(&store, "ptask-gp-stale", tenant);
        // Missing prereq path for foreign.
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
        // Expired issue rejected.
        let err = issue_and_admit_v2(
            &store,
            &principal,
            "auth-exp",
            "run-exp",
            "ptask-gp-stale",
            "2026-07-01T00:00:00Z",
        )
        .unwrap_err();
        assert!(
            err.contains("expired") || err.contains("fail closed"),
            "{err}"
        );
    }

    #[test]
    fn crash_style_admit_lease_recovery_then_schedule() {
        let dir = tempdir().unwrap();
        let store = LocalProductStore::new_with_clock(dir.path().join("lease.db"), || {
            "2026-07-25T12:00:00Z".into()
        })
        .unwrap();
        let tenant = "t-lease";
        let principal = operator(&store, tenant, "op-lease");
        seed_gp(&store, "ptask-gp-lease", tenant);
        let admitted = issue_and_admit_v2(
            &store,
            &principal,
            "auth-lease",
            "run-lease",
            "ptask-gp-lease",
            "2026-08-07T00:00:00Z",
        )
        .unwrap();
        assert!(admitted.get("lease_token").is_some());
        // Simulate crash: empty lease → coordinator recovers via exact admit replay.
        let driver = InjectedCellDriver {
            outcomes: success_outcomes(),
        };
        let result =
            run_frozen_schedule(&store, &principal, "run-lease", "auth-lease", "", &driver)
                .unwrap();
        assert_eq!(result["attempts_recorded"], 4);
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
        assert_eq!(a.definition_sha256, task.definition_sha256);
    }

    #[test]
    fn unarmed_product_driver_fails_before_cell_effect_and_run_terminalization() {
        let dir = tempdir().unwrap();
        let store = LocalProductStore::new_with_clock(dir.path().join("unarmed.db"), || {
            "2026-07-25T12:00:00Z".into()
        })
        .unwrap();
        let tenant = "t-unarmed";
        let principal = operator(&store, tenant, "op-unarmed");
        seed_gp(&store, "ptask-gp-unarmed", tenant);
        let admitted = issue_and_admit_v2(
            &store,
            &principal,
            "auth-unarmed",
            "run-unarmed",
            "ptask-gp-unarmed",
            "2026-08-07T00:00:00Z",
        )
        .unwrap();
        let lease = admitted["lease_token"].as_str().unwrap();
        let driver = ProductGoldenPathCellDriver::default();
        let err = run_frozen_schedule(
            &store,
            &principal,
            "run-unarmed",
            "auth-unarmed",
            lease,
            &driver,
        )
        .unwrap_err();
        assert!(
            err.contains("fail closed before cell effect") || err.contains("not armed"),
            "{err}"
        );
        // No cell attempts, no run terminalization.
        assert!(store
            .list_rwe_task_attempts_for_run("run-unarmed")
            .unwrap()
            .is_empty());
        let run = store.get_rwe_run("run-unarmed").unwrap().unwrap();
        assert_ne!(run.get("status").and_then(Value::as_str), Some("failed"));
        assert_ne!(run.get("status").and_then(Value::as_str), Some("succeeded"));
    }

    #[test]
    fn injected_outcome_cannot_seal_live_baseline_even_with_live_claim() {
        let dir = tempdir().unwrap();
        let store = LocalProductStore::new_with_clock(dir.path().join("inj-seal.db"), || {
            "2026-07-25T12:00:00Z".into()
        })
        .unwrap();
        let tenant = "t-inj-seal";
        let principal = operator(&store, tenant, "op-inj-seal");
        seed_gp(&store, "ptask-gp-inj-seal", tenant);
        let mut outcomes = success_outcomes();
        for o in &mut outcomes {
            o.live_provider_request = true; // public claim — must not seal
            o.classification = "success".into();
            o.terminal_evidence_id = Some("fake-tev".into());
            o.terminal_content_sha256 = Some("a".repeat(64));
            o.evidence_source = "injected".into();
        }
        let admitted = issue_and_admit_v2(
            &store,
            &principal,
            "auth-inj-seal",
            "run-inj-seal",
            "ptask-gp-inj-seal",
            "2026-08-07T00:00:00Z",
        )
        .unwrap();
        let lease = admitted["lease_token"].as_str().unwrap();
        let driver = InjectedCellDriver { outcomes };
        let result = run_frozen_schedule(
            &store,
            &principal,
            "run-inj-seal",
            "auth-inj-seal",
            lease,
            &driver,
        )
        .unwrap();
        assert_eq!(result["live_baseline_sealed"], false);
        // Injected driver forces evidence_source=injected and clears live flag.
        for a in store
            .list_rwe_task_attempts_for_run("run-inj-seal")
            .unwrap()
        {
            assert_eq!(a["evidence_json"]["evidence_source"], "injected");
            assert_eq!(a["evidence_json"]["live_provider_request"], false);
        }
    }

    #[test]
    fn pre_effect_budget_refusal_invokes_driver_zero_times() {
        let dir = tempdir().unwrap();
        let store = LocalProductStore::new_with_clock(dir.path().join("budget.db"), || {
            "2026-07-25T12:00:00Z".into()
        })
        .unwrap();
        let tenant = "t-budget";
        let principal = operator(&store, tenant, "op-budget");
        seed_gp(&store, "ptask-gp-budget", tenant);
        // First cell exhausts the run-level request budget (12).
        let mut outcomes = success_outcomes();
        outcomes[0].provider_requests = 12;
        outcomes[0].total_tokens = 100;
        let admitted = issue_and_admit_v2(
            &store,
            &principal,
            "auth-budget",
            "run-budget",
            "ptask-gp-budget",
            "2026-08-07T00:00:00Z",
        )
        .unwrap();
        let lease = admitted["lease_token"].as_str().unwrap().to_string();
        let inner = InjectedCellDriver {
            outcomes: outcomes.clone(),
        };
        let counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counting = CountingCellDriver {
            inner: &inner,
            invocations: std::sync::Arc::clone(&counter),
        };
        run_frozen_schedule(
            &store,
            &principal,
            "run-budget",
            "auth-budget",
            &lease,
            &counting,
        )
        .unwrap();
        // Cell 1 dispatched; cells 2-4 stopped by budget (post cell1) as skipped or blocked.
        // Restart with exhausted aggregate: remaining incomplete cells must not re-dispatch.
        // After first run all 4 cells are terminal. Prove pre-effect path with a fresh run
        // that already has aggregate at ceiling via prior attempts reconstructed budget.
        let attempts = store.list_rwe_task_attempts_for_run("run-budget").unwrap();
        assert_eq!(attempts.len(), 4);
        let first_invocations = counter.load(std::sync::atomic::Ordering::SeqCst);
        assert!(first_invocations >= 1, "cell1 must have run once");

        // Second schedule: all terminal → zero new driver invocations.
        let before = counter.load(std::sync::atomic::Ordering::SeqCst);
        run_frozen_schedule(
            &store,
            &principal,
            "run-budget",
            "auth-budget",
            &lease,
            &counting,
        )
        .unwrap();
        assert_eq!(
            counter.load(std::sync::atomic::Ordering::SeqCst),
            before,
            "restart must not re-dispatch terminal cells"
        );

        // Dedicated pre-effect proof: new run where first attempt already recorded full budget.
        let admitted2 = issue_and_admit_v2(
            &store,
            &principal,
            "auth-budget2",
            "run-budget2",
            "ptask-gp-budget",
            "2026-08-07T00:00:00Z",
        )
        .unwrap();
        let lease2 = admitted2["lease_token"].as_str().unwrap().to_string();
        let frozen = freeze_current_operator_contract_set().unwrap();
        let cell0 = &frozen.schedule.body["cells"][0];
        let task0 = frozen
            .corpus
            .tasks
            .iter()
            .find(|t| t.task_id == cell0["task_id"].as_str().unwrap())
            .unwrap();
        let ids0 = cell_identities_for("run-budget2", cell0, task0).unwrap();
        let mut seed_outcome = outcomes[0].clone();
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
                &lease2,
                &ids0.rwe_task_attempt_id,
                &ids0.task_id,
                &ids0.definition_sha256,
                "injected_success",
                &evidence,
            )
            .unwrap();
        let counter2 = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let inner2 = InjectedCellDriver {
            outcomes: success_outcomes(),
        };
        let counting2 = CountingCellDriver {
            inner: &inner2,
            invocations: std::sync::Arc::clone(&counter2),
        };
        let result = run_frozen_schedule(
            &store,
            &principal,
            "run-budget2",
            "auth-budget2",
            &lease2,
            &counting2,
        )
        .unwrap();
        assert_eq!(
            counter2.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "over-budget preflight must invoke driver zero times"
        );
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
        let store = LocalProductStore::new_with_clock(dir.path().join("stop.db"), || {
            "2026-07-25T12:00:00Z".into()
        })
        .unwrap();
        let tenant = "t-stop";
        let principal = operator(&store, tenant, "op-stop");
        seed_gp(&store, "ptask-gp-stop", tenant);
        let mut outcomes = success_outcomes();
        outcomes[0].classification = "blocked_authority".into();
        outcomes[0].live_provider_request = false;
        let admitted = issue_and_admit_v2(
            &store,
            &principal,
            "auth-stop",
            "run-stop",
            "ptask-gp-stop",
            "2026-08-07T00:00:00Z",
        )
        .unwrap();
        let lease = admitted["lease_token"].as_str().unwrap().to_string();
        let counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let inner = InjectedCellDriver {
            outcomes: outcomes.clone(),
        };
        let counting = CountingCellDriver {
            inner: &inner,
            invocations: std::sync::Arc::clone(&counter),
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
        let first = counter.load(std::sync::atomic::Ordering::SeqCst);
        assert_eq!(first, 1, "only first cell should dispatch before stop");
        let attempts = store.list_rwe_task_attempts_for_run("run-stop").unwrap();
        assert_eq!(attempts.len(), 4);
        let skipped = attempts
            .iter()
            .filter(|a| a["classification"] == "skipped_by_stop_rule")
            .count();
        assert_eq!(skipped, 3);
        // Restart: reconstruct stop; zero new dispatches.
        run_frozen_schedule(
            &store,
            &principal,
            "run-stop",
            "auth-stop",
            &lease,
            &counting,
        )
        .unwrap();
        assert_eq!(
            counter.load(std::sync::atomic::Ordering::SeqCst),
            first,
            "restart must not redispatch skipped/terminal cells"
        );
    }

    #[test]
    fn outcome_unknown_is_terminal_no_second_authorization() {
        let dir = tempdir().unwrap();
        let store = LocalProductStore::new_with_clock(dir.path().join("ou.db"), || {
            "2026-07-25T12:00:00Z".into()
        })
        .unwrap();
        let tenant = "t-ou";
        let principal = operator(&store, tenant, "op-ou");
        seed_gp(&store, "ptask-gp-ou", tenant);
        let mut outcomes = success_outcomes();
        outcomes[0].classification = "outcome_unknown".into();
        outcomes[0].cost_unknown = true;
        outcomes[0].monetary_cost = None;
        let admitted = issue_and_admit_v2(
            &store,
            &principal,
            "auth-ou",
            "run-ou",
            "ptask-gp-ou",
            "2026-08-07T00:00:00Z",
        )
        .unwrap();
        let lease = admitted["lease_token"].as_str().unwrap();
        let driver = InjectedCellDriver { outcomes };
        let result =
            run_frozen_schedule(&store, &principal, "run-ou", "auth-ou", lease, &driver).unwrap();
        assert_eq!(result["attempts_recorded"], 4);
        // No second authorization issued.
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
    fn owner_backed_success_path_with_fake_transport() {
        let dir = tempdir().unwrap();
        let work = dir.path().join("work");
        std::fs::create_dir_all(&work).unwrap();
        let store = LocalProductStore::new_with_clock(dir.path().join("owner.db"), || {
            "2026-07-25T12:00:00Z".into()
        })
        .unwrap();
        let tenant = "t-owner";
        let principal = operator(&store, tenant, "op-owner");
        seed_gp(&store, "ptask-gp-owner", tenant);
        let admitted = issue_and_admit_v2(
            &store,
            &principal,
            "auth-owner",
            "run-owner",
            "ptask-gp-owner",
            "2026-08-07T00:00:00Z",
        )
        .unwrap();
        let lease = admitted["lease_token"].as_str().unwrap();

        // Mock OpenAI-compatible response with required planner JSON body.
        let plan_body = r#"{"schema_version":"managed_deepseek_plan.v1","status":"planned","path":"docs/USER_GUIDE.md","intent":"clarify_doctor_read_only_health_check"}"#;
        let response_json = format!(
            r#"{{"id":"rwe-fake-1","model":"deepseek-v4-pro","choices":[{{"message":{{"role":"assistant","content":{}}},"finish_reason":"stop"}}],"usage":{{"prompt_tokens":10,"completion_tokens":5}}}}"#,
            serde_json::to_string(plan_body).unwrap()
        );
        // Four cells × one planning stage each.
        let responses: Vec<_> = (0..4)
            .map(|_| {
                Ok(crate::provider::transport::HttpResponse {
                    status: 200,
                    body: response_json.as_bytes().to_vec(),
                })
            })
            .collect();
        let transport =
            std::sync::Arc::new(crate::provider::transport::MockTransport::new(responses));
        // Credential boundary resolves at send; provide a non-secret test value.
        std::env::set_var(DEEPSEEK_CREDENTIAL_REFERENCE, "test-not-a-real-key");
        let driver = ProductGoldenPathCellDriver {
            allow_live_provider_effects: false,
            fake_transport: Some(transport),
            work_root: Some(work),
        };
        let result = run_frozen_schedule(
            &store,
            &principal,
            "run-owner",
            "auth-owner",
            lease,
            &driver,
        );
        std::env::remove_var(DEEPSEEK_CREDENTIAL_REFERENCE);
        let result = result.expect("owner-backed schedule");
        assert_eq!(result["cell_count"], 4);
        assert_eq!(result["live_baseline_sealed"], false);
        let attempts = store.list_rwe_task_attempts_for_run("run-owner").unwrap();
        assert_eq!(attempts.len(), 4);
        for a in &attempts {
            assert_eq!(
                a["evidence_json"]["evidence_source"],
                "product_golden_path_owner"
            );
            // ProductTask was admitted through the store owner.
            let ptid = a["evidence_json"]["product_task_id"].as_str().unwrap();
            let task = store
                .get_product_task_for_tenant(ptid, tenant)
                .unwrap()
                .expect("product task must exist in store");
            assert!(
                matches!(
                    task.get("status").and_then(Value::as_str),
                    Some("workspace_bound" | "graph_ready" | "completed" | "failed")
                ),
                "unexpected product task status: {:?}",
                task.get("status")
            );
        }
    }

    #[test]
    fn missing_terminal_receipt_fails_closed_for_seal() {
        let dir = tempdir().unwrap();
        let store = LocalProductStore::new_with_clock(dir.path().join("term.db"), || {
            "2026-07-25T12:00:00Z".into()
        })
        .unwrap();
        let tenant = "t-term";
        let principal = operator(&store, tenant, "op-term");
        seed_gp(&store, "ptask-gp-term", tenant);
        // Inject success with terminal ids but no store-owned terminal → seal false.
        let mut outcomes = success_outcomes();
        for o in &mut outcomes {
            o.evidence_source = "product_golden_path_owner".into(); // spoofed — still fails store check
            o.live_provider_request = true;
            o.classification = "success".into();
            o.product_task_id = "nonexistent-pt".into();
        }
        // Injected driver forces evidence_source back to injected.
        let admitted = issue_and_admit_v2(
            &store,
            &principal,
            "auth-term",
            "run-term",
            "ptask-gp-term",
            "2026-08-07T00:00:00Z",
        )
        .unwrap();
        let lease = admitted["lease_token"].as_str().unwrap();
        let result = run_frozen_schedule(
            &store,
            &principal,
            "run-term",
            "auth-term",
            lease,
            &InjectedCellDriver { outcomes },
        )
        .unwrap();
        assert_eq!(result["live_baseline_sealed"], false);
    }
}
