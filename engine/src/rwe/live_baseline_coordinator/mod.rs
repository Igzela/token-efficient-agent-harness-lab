//! Provider-free first-live-baseline coordinator for Minimum First RWE.
//!
//! Wires store-owned `rwe_run_authorization.v2` issue/admit to the frozen 4-cell
//! schedule under existing Product Golden Path and `LocalProductStore` owners for
//! the exact frozen RWE bindings. Cell dispatch is fenced by store-owned atomic
//! claim (run↔authorization binding + full next-cell budget envelope).
//!
//! Production composition uses [`ProductGoldenPathCellDriver`]. Injectable
//! drivers are test-only and cannot seal. Sealing requires immutable store-owned
//! ProductTask/terminal/provider-journal evidence. Provider POSTs and target
//! writes remain off until controller live authorization after merge; tests use
//! fake transports only.

#![allow(unused_imports)]

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::cli::codex_budget_authority::{
    CodexBudgetAuthority, CodexBudgetGateway, CodexExecutableIdentity, CodexProviderIdentity,
    CodexUpstreamAuth, CODEX_BUDGET_AUTHORITY_SCHEMA,
};
use crate::harness_evolution::{
    ControllerAction, EffectReceiptDisposition, LedgerController, LedgerOrchestrator,
    LedgerOrchestratorConfig, LedgerTaskRecord, LedgerTaskStatus, LedgerTerminalDisposition,
    LedgerTerminalRecord, LedgerVerifier, LedgerWorker, NewTaskSpec, NextTaskDecision,
    OrchestrationError, OrchestrationUsageEnvelope, StoreEffectReceipt, VerificationOutcome,
    VerificationReport, WorkerContext, WorkerOutcomeStatus, WorkerResult, WorkingLedger,
    LEDGER_ORCHESTRATED_HARNESS_ID,
};
use crate::product_golden_path::{validate_matrix_binding, ProductHarnessMatrixBinding};
use crate::provider::config::{CredentialRef, ProviderConfig};
use crate::provider::credential::CredentialBoundary;
use crate::provider::managed_deepseek::{
    DeepSeekProtocol, ManagedAuthoritySource, ManagedCallBinding, ManagedCallLimits,
    ManagedDeepSeekProvider, ManagedFailureEffect, ManagedFunctionCall, ManagedMessage,
    ManagedModelRole, ManagedProviderCallAuthority, ManagedProviderCallError,
    ManagedProviderCallRequest, ManagedProviderResponse, ManagedToolCall, ManagedUsage,
    DEEPSEEK_CREDENTIAL_REFERENCE, DEEPSEEK_OPENAI_BASE_URL, DEEPSEEK_OPENAI_PATH,
    DEEPSEEK_PROVIDER_KIND,
};
use crate::provider::managed_deepseek_executor::{
    ManagedDeepSeekExecutorConfig, ManagedDeepSeekNodeExecutor, ManagedNodeProvider,
};
use crate::provider::transport::{HttpError, HttpRequest, HttpTransport};
use crate::rwe::corpus::RweTaskDefinition;
use crate::rwe::frozen_rwe_bindings::{
    current_comparison_manifest, rwe_composition_seam_ready, RweCellBudgetEnvelope,
    FROZEN_RWE_RISK_CLASS, FROZEN_RWE_TARGET_MAIN_SHA,
};
use crate::rwe::operator_corpus::{
    freeze_current_operator_contract_set, OperatorFrozenContractSet, OPERATOR_ADMITTED_BINARY_PATH,
    OPERATOR_ADMITTED_BINARY_VERSION, OPERATOR_ADMITTED_MODEL,
    OPERATOR_ADMITTED_PLANNER_REVIEWER_MODEL, OPERATOR_TARGET_REPO,
};
use crate::rwe::runner::{
    persist_rwe_run_authorization_v2, RWE_RUN_AUTH_V2_SCHEMA, RWE_RUN_EVIDENCE_SCHEMA,
};
use crate::storage::local_product_store::{
    compute_attempt_manifest_sha256, AuthenticatedPrincipal, DelegationContract, LocalProductStore,
    RweAuthorizationV2IssueRequest, DELEGATION_SCHEMA_VERSION,
};

pub const RWE_LIVE_BASELINE_COORDINATOR_SCHEMA: &str = "rwe_live_baseline_coordinator.v1";

pub const RWE_CELL_ATTEMPT_EVIDENCE_SCHEMA: &str = "rwe_cell_attempt_evidence.v1";

/// Composition seam is callable for exact frozen RWE bindings under existing owners.
pub const RWE_LIVE_CELL_COMPOSITION_SEAM: &str =
    "rwe_cell_composition:product_golden_path+local_product_store:frozen_rwe_bindings.v1";

/// Composition seam for ledger-orchestrated matrix cells. The cell executes
/// the provider-independent `LedgerOrchestrator` manager/worker/verifier loop
/// inside the store-owned ProductTask envelope instead of the default managed
/// 3-node path; no second scheduler, store, provider authority, evaluator,
/// budget, or approval owner is introduced.
pub const RWE_LEDGER_CELL_COMPOSITION_SEAM: &str =
    "rwe_cell_composition:ledger_orchestrated+product_golden_path+local_product_store.v1";

/// Operator live-run token: live provider POSTs and target writes require the
/// explicit `=1` symbol in the parent process (parity with the armed fixture's
/// `ACP_RWE_ARMED_LIVE_RUN` gate; CI never sets either).
pub const RWE_OPERATOR_LIVE_RUN_TOKEN: &str = "ACP_RWE_OPERATOR_LIVE_RUN";

const CODEX_PREREQUISITE_PRIMARY_TASK_ID: &str = "rwe-minimum-t1-fix_flow_linkage";

const CODEX_PREREQUISITE_FALLBACK_TASK_ID: &str = "rwe-minimum-t2-draft_contract_tests";

// Finite operator recovery remains bounded even when prior immutable attempts
// are preserved after an upstream outcome-unknown response.
pub const RWE_CODEX_PREREQUISITE_MAX_RECOVERY_GENERATIONS_TOKEN: &str =
    "ACP_CODEX_PREREQUISITE_MAX_RECOVERY_GENERATIONS";

const CODEX_PREREQUISITE_DEFAULT_MAX_RECOVERY_GENERATIONS: u8 = 10;

/// Live ledger-orchestrated cell composition marker. Rounds execute real
/// delegated implementer calls through the store-owned provider journal;
/// provenance and seal eligibility still derive from store rows only.
pub const RWE_LIVE_LEDGER_CELL_COMPOSITION: &str =
    "rwe_cell_composition:ledger_orchestrated_live_delegated_implementer_rounds.v1";

/// Explicitly records that the ledger manager is policy-backed and provider
/// independent. Its call count is lifecycle accounting only; provider effects
/// can originate solely from worker rounds.
pub const RWE_LEDGER_MANAGER_SEMANTICS: &str =
    "ledger-manager:provider-independent:policy-backed-deterministic:v1";

/// Upper bound for one live ledger worker prompt rendering. The managed call
/// envelope reserves `max_input_tokens` (12k frozen) against byte-counted
/// input, so the prompt stays well inside the pre-send reservation.
const LIVE_LEDGER_MAX_PROMPT_BYTES: usize = 8_000;

/// Upper bound for one frozen verifier output capture per live ledger round.
const LIVE_LEDGER_MAX_VERIFIER_OUTPUT_BYTES: usize = 65_536;

mod cell;
mod provider;
mod schedule;
mod worker;

pub use cell::*;
pub use provider::*;
pub use schedule::*;
pub use worker::*;

#[cfg(test)]
mod tests;
