//! Bounded recursive task-tree policy for PE7.
//!
//! This module owns admission decisions only. Execution remains owned by the
//! existing agent runtime and scheduler; callers persist the returned tree
//! through `LocalProductStore`.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
#[cfg(test)]
use std::sync::{Mutex, OnceLock};

pub const RECURSIVE_SCHEMA_VERSION: &str = "recursive_execution.v1";
pub const MAX_RECURSIVE_DEPTH: u8 = 2;
pub const MAX_ACCEPTED_CHILDREN_PER_NODE: usize = 3;
pub const MAX_RECURSIVE_NODES_PER_ROOT: usize = 12;
pub const MAX_RECURSIVE_LEASES: usize = 3;
pub const MAX_RECURSIVE_RETRIES: u8 = 1;
const MAX_OBJECTIVE_BYTES: usize = 4096;
const MAX_CONTEXT_BYTES: usize = 8192;
pub const MAX_RECURSIVE_TREE_BYTES: usize = 131_072;
pub const MAX_SCOPE_VALUE_BYTES: usize = 1024;
pub const MAX_SCOPE_ITEMS: usize = 64;
pub const MAX_RECURSIVE_DECISION_EVIDENCE_REFS: usize = 8;
pub const MAX_RECURSIVE_EVIDENCE_REF_BYTES: usize = 256;
pub const OBJECTIVE_IDENTITY_VERSION: &str = "recursive-objective.v2";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct RecursiveObjectiveIdentity {
    version: &'static str,
    intent: String,
    subject: String,
    qualifiers: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecursiveExecutionState {
    Running,
    OperatorPaused,
    BudgetExhausted,
    KillStopped,
    TerminalFailed,
}

impl Default for RecursiveExecutionState {
    fn default() -> Self {
        Self::Running
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecursiveFailureReason {
    RecursiveDisabled,
    DepthExceeded,
    ChildLimitExceeded,
    TreeBudgetExhausted,
    DuplicateObjective,
    AncestorCycle,
    CapabilityEscalation,
    ScopeMismatch,
    StaleParent,
    ProposalConflict,
    ReceiptConflict,
    SchedulerCapacityExhausted,
    RecursiveKillSwitchActive,
    OperatorPaused,
    TerminalFailed,
    ExecutionFailure,
    RetryExhausted,
    RecursiveTreeMissing,
    RecursiveNodeMissing,
    RecursiveUsageUnavailable,
    FixtureUsageContractMissing,
}

impl RecursiveFailureReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RecursiveDisabled => "recursive_disabled",
            Self::DepthExceeded => "depth_exceeded",
            Self::ChildLimitExceeded => "child_limit_exceeded",
            Self::TreeBudgetExhausted => "tree_budget_exhausted",
            Self::DuplicateObjective => "duplicate_objective",
            Self::AncestorCycle => "ancestor_cycle",
            Self::CapabilityEscalation => "capability_escalation",
            Self::ScopeMismatch => "scope_mismatch",
            Self::StaleParent => "stale_parent",
            Self::ProposalConflict => "proposal_conflict",
            Self::ReceiptConflict => "receipt_conflict",
            Self::SchedulerCapacityExhausted => "scheduler_capacity_exhausted",
            Self::RecursiveKillSwitchActive => "recursive_kill_switch_active",
            Self::OperatorPaused => "operator_paused",
            Self::TerminalFailed => "terminal_failed",
            Self::ExecutionFailure => "recursive_node_execution_failed",
            Self::RetryExhausted => "recursive_retry_exhausted",
            Self::RecursiveTreeMissing => "recursive_tree_missing",
            Self::RecursiveNodeMissing => "recursive_node_missing",
            Self::RecursiveUsageUnavailable => "recursive_usage_unavailable",
            Self::FixtureUsageContractMissing => "fixture_usage_contract_missing",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RecursiveBudget {
    pub calls_remaining: u64,
    pub tokens_remaining: u64,
    pub cost_micros_remaining: u64,
    pub time_ms_remaining: u64,
}

impl RecursiveBudget {
    pub(crate) fn can_spend(&self, usage: &Self) -> bool {
        self.calls_remaining >= usage.calls_remaining
            && self.tokens_remaining >= usage.tokens_remaining
            && self.cost_micros_remaining >= usage.cost_micros_remaining
            && self.time_ms_remaining >= usage.time_ms_remaining
    }

    fn spend(&mut self, other: &Self) {
        self.calls_remaining = self.calls_remaining.saturating_sub(other.calls_remaining);
        self.tokens_remaining = self.tokens_remaining.saturating_sub(other.tokens_remaining);
        self.cost_micros_remaining = self
            .cost_micros_remaining
            .saturating_sub(other.cost_micros_remaining);
        self.time_ms_remaining = self
            .time_ms_remaining
            .saturating_sub(other.time_ms_remaining);
    }

    pub(crate) fn add(&mut self, other: &Self) {
        self.calls_remaining = self.calls_remaining.saturating_add(other.calls_remaining);
        self.tokens_remaining = self.tokens_remaining.saturating_add(other.tokens_remaining);
        self.cost_micros_remaining = self
            .cost_micros_remaining
            .saturating_add(other.cost_micros_remaining);
        self.time_ms_remaining = self
            .time_ms_remaining
            .saturating_add(other.time_ms_remaining);
    }

    fn bounded_by(&self, other: &Self) -> Self {
        Self {
            calls_remaining: self.calls_remaining.min(other.calls_remaining),
            tokens_remaining: self.tokens_remaining.min(other.tokens_remaining),
            cost_micros_remaining: self.cost_micros_remaining.min(other.cost_micros_remaining),
            time_ms_remaining: self.time_ms_remaining.min(other.time_ms_remaining),
        }
    }

    fn is_nonzero(&self) -> bool {
        self.calls_remaining > 0
            && self.tokens_remaining > 0
            && self.cost_micros_remaining > 0
            && self.time_ms_remaining > 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecursiveScope {
    pub repository: Option<String>,
    pub allowed_paths: BTreeSet<String>,
    pub capabilities: BTreeSet<String>,
}

impl RecursiveScope {
    pub fn is_subset_of(&self, parent: &Self) -> bool {
        self.repository == parent.repository
            && self.allowed_paths.is_subset(&parent.allowed_paths)
            && self.capabilities.is_subset(&parent.capabilities)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecursiveProposal {
    pub proposal_id: String,
    pub parent_node_id: String,
    pub parent_version: u64,
    pub objective: String,
    pub context_summary: String,
    pub requested_scope: RecursiveScope,
    pub requested_capabilities: BTreeSet<String>,
    pub budget: RecursiveBudget,
    pub receipt_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecursiveNode {
    pub node_id: String,
    pub root_run_id: String,
    pub parent_node_id: Option<String>,
    pub proposal_id: Option<String>,
    pub depth: u8,
    pub objective_fingerprint: String,
    pub ancestor_fingerprints: Vec<String>,
    pub scope: RecursiveScope,
    pub capabilities: BTreeSet<String>,
    #[serde(default)]
    pub tenant_id: Option<String>,
    #[serde(default)]
    pub workspace_id: Option<String>,
    pub budget: RecursiveBudget,
    #[serde(default)]
    pub child_budget: RecursiveBudget,
    #[serde(default)]
    pub reservation: RecursiveBudget,
    #[serde(default)]
    pub actual_usage: RecursiveBudget,
    pub status: String,
    pub accepted_children: usize,
    pub retry_count: u8,
    pub version: u64,
    pub lease_id: Option<String>,
    #[serde(default)]
    pub failure_reason: Option<String>,
    #[serde(default)]
    pub evidence_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecursiveDecisionEvidence {
    pub reason_code: String,
    pub evidence_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecursiveTree {
    pub schema_version: String,
    pub root_run_id: String,
    pub workflow_id: String,
    pub root_node_id: String,
    #[serde(default)]
    pub root_agent_id: String,
    #[serde(default)]
    pub root_recursive_marker: String,
    #[serde(default)]
    pub root_creation_receipt_sha256: String,
    pub root_scope: RecursiveScope,
    pub root_capabilities: BTreeSet<String>,
    #[serde(default)]
    pub root_tenant_id: Option<String>,
    #[serde(default)]
    pub root_workspace_id: Option<String>,
    pub root_budget: RecursiveBudget,
    /// Remaining unallocated tree authority. `spent_budget` records actual
    /// usage separately so reservation and execution accounting cannot be
    /// confused.
    #[serde(default)]
    pub root_budget_limit: Option<RecursiveBudget>,
    #[serde(default)]
    pub spent_budget: RecursiveBudget,
    #[serde(default)]
    pub reserved_budget: RecursiveBudget,
    #[serde(default)]
    pub execution_state: RecursiveExecutionState,
    pub version: u64,
    pub nodes: BTreeMap<String, RecursiveNode>,
    pub accepted_proposals: BTreeSet<String>,
    pub receipts: BTreeMap<String, String>,
    pub active_leases: BTreeSet<String>,
    #[serde(default)]
    pub rejected_proposals: BTreeMap<String, RecursiveDecisionEvidence>,
    #[serde(default)]
    pub usage_receipts: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecursiveAdmission {
    pub node: RecursiveNode,
    pub parent_version: u64,
}

pub fn normalize_objective(objective: &str) -> String {
    objective
        .chars()
        .map(|ch| {
            if ch.is_alphanumeric() || ch.is_whitespace() {
                ch.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .filter(|token| !matches!(*token, "a" | "an" | "the" | "to" | "please"))
        .map(|token| match token {
            "inspect" | "examine" => "review",
            "documentation" => "docs",
            other => other,
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn objective_fingerprint(objective: &str) -> String {
    let tokens = normalize_objective(objective)
        .split_whitespace()
        .map(str::to_string)
        .collect::<Vec<_>>();
    let intent = tokens.first().cloned().unwrap_or_default();
    let subject = tokens.get(1).cloned().unwrap_or_default();
    let qualifiers = tokens.into_iter().skip(2).collect::<Vec<_>>();
    let canonical = RecursiveObjectiveIdentity {
        version: OBJECTIVE_IDENTITY_VERSION,
        intent,
        subject,
        qualifiers,
    };
    let mut hasher = Sha256::new();
    hasher.update(
        serde_json::to_vec(&canonical)
            .expect("recursive objective identity serialization is infallible")
            .as_slice(),
    );
    hex::encode(hasher.finalize())
}

fn derived_node_id(root_run_id: &str, proposal_id: &str, fingerprint: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(root_run_id.as_bytes());
    hasher.update([0]);
    hasher.update(proposal_id.as_bytes());
    hasher.update([0]);
    hasher.update(fingerprint.as_bytes());
    format!("recursive-{}", hex::encode(hasher.finalize()))
}

pub(crate) fn derived_node_id_for_persistence(
    root_run_id: &str,
    proposal_id: &str,
    fingerprint: &str,
) -> String {
    derived_node_id(root_run_id, proposal_id, fingerprint)
}

pub(crate) fn recursive_enabled() -> bool {
    std::env::var("ACP_RECURSIVE_EXECUTION_ENABLED")
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

fn kill_switch_active() -> bool {
    std::env::var("ACP_RECURSIVE_EXECUTION_KILL_SWITCH")
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

fn shape_is_valid(proposal: &RecursiveProposal) -> bool {
    !proposal.proposal_id.is_empty()
        && !proposal.parent_node_id.is_empty()
        && !proposal.receipt_sha256.is_empty()
        && !normalize_objective(&proposal.objective).is_empty()
        && proposal.objective.len() <= MAX_OBJECTIVE_BYTES
        && proposal.context_summary.len() <= MAX_CONTEXT_BYTES
}

impl RecursiveTree {
    pub fn new(
        root_run_id: impl Into<String>,
        workflow_id: impl Into<String>,
        objective: &str,
        scope: RecursiveScope,
        capabilities: BTreeSet<String>,
        budget: RecursiveBudget,
    ) -> Self {
        let root_run_id = root_run_id.into();
        let fingerprint = objective_fingerprint(objective);
        let mut root_identity = Sha256::new();
        root_identity.update(root_run_id.as_bytes());
        root_identity.update([0]);
        root_identity.update(fingerprint.as_bytes());
        let node_id = format!(
            "recursive-root-{}",
            &hex::encode(root_identity.finalize())[..24]
        );
        Self::new_with_root_node_id(
            root_run_id,
            workflow_id,
            node_id,
            objective,
            scope,
            capabilities,
            budget,
        )
    }

    pub fn new_with_root_node_id(
        root_run_id: impl Into<String>,
        workflow_id: impl Into<String>,
        node_id: impl Into<String>,
        objective: &str,
        scope: RecursiveScope,
        capabilities: BTreeSet<String>,
        budget: RecursiveBudget,
    ) -> Self {
        let root_run_id = root_run_id.into();
        let node_id = node_id.into();
        let fingerprint = objective_fingerprint(objective);
        let node = RecursiveNode {
            node_id: node_id.clone(),
            root_run_id: root_run_id.clone(),
            parent_node_id: None,
            proposal_id: None,
            depth: 0,
            objective_fingerprint: fingerprint.clone(),
            ancestor_fingerprints: Vec::new(),
            scope: scope.clone(),
            capabilities: capabilities.clone(),
            tenant_id: None,
            workspace_id: None,
            budget: budget.clone(),
            status: "ready".to_string(),
            accepted_children: 0,
            retry_count: 0,
            child_budget: budget.clone(),
            reservation: RecursiveBudget::default(),
            actual_usage: RecursiveBudget::default(),
            version: 1,
            lease_id: None,
            failure_reason: None,
            evidence_refs: Vec::new(),
        };
        let mut nodes = BTreeMap::new();
        nodes.insert(node_id.clone(), node);
        Self {
            schema_version: RECURSIVE_SCHEMA_VERSION.to_string(),
            root_run_id,
            workflow_id: workflow_id.into(),
            root_node_id: node_id,
            root_agent_id: String::new(),
            root_recursive_marker: String::new(),
            root_creation_receipt_sha256: String::new(),
            root_scope: scope,
            root_capabilities: capabilities,
            root_tenant_id: None,
            root_workspace_id: None,
            root_budget_limit: Some(budget.clone()),
            root_budget: budget,
            spent_budget: RecursiveBudget {
                calls_remaining: 0,
                tokens_remaining: 0,
                cost_micros_remaining: 0,
                time_ms_remaining: 0,
            },
            reserved_budget: RecursiveBudget::default(),
            execution_state: RecursiveExecutionState::Running,
            version: 1,
            nodes,
            accepted_proposals: BTreeSet::new(),
            receipts: BTreeMap::new(),
            active_leases: BTreeSet::new(),
            rejected_proposals: BTreeMap::new(),
            usage_receipts: BTreeMap::new(),
        }
    }

    pub fn bind_root_identity(
        &mut self,
        agent_id: &str,
        recursive_marker: &str,
        creation_receipt_sha256: &str,
    ) -> Result<(), RecursiveFailureReason> {
        if agent_id.is_empty() || recursive_marker.is_empty() || creation_receipt_sha256.is_empty()
        {
            return Err(RecursiveFailureReason::ReceiptConflict);
        }
        let matches_or_empty =
            |existing: &str, value: &str| existing.is_empty() || existing == value;
        if !matches_or_empty(&self.root_agent_id, agent_id)
            || !matches_or_empty(&self.root_recursive_marker, recursive_marker)
            || !matches_or_empty(&self.root_creation_receipt_sha256, creation_receipt_sha256)
        {
            return Err(RecursiveFailureReason::ReceiptConflict);
        }
        self.root_agent_id = agent_id.to_string();
        self.root_recursive_marker = recursive_marker.to_string();
        self.root_creation_receipt_sha256 = creation_receipt_sha256.to_string();
        Ok(())
    }

    pub fn bind_root_execution_scope(
        &mut self,
        tenant_id: Option<&str>,
        workspace_id: Option<&str>,
    ) -> Result<(), RecursiveFailureReason> {
        let tenant_id = tenant_id.map(str::to_string);
        let workspace_id = workspace_id.map(str::to_string);
        if self
            .root_tenant_id
            .as_ref()
            .is_some_and(|value| Some(value) != tenant_id.as_ref())
            || self
                .root_workspace_id
                .as_ref()
                .is_some_and(|value| Some(value) != workspace_id.as_ref())
        {
            return Err(RecursiveFailureReason::ReceiptConflict);
        }
        self.root_tenant_id = tenant_id.clone();
        self.root_workspace_id = workspace_id.clone();
        let root = self
            .nodes
            .get_mut(&self.root_node_id)
            .ok_or(RecursiveFailureReason::RecursiveNodeMissing)?;
        root.tenant_id = tenant_id;
        root.workspace_id = workspace_id;
        Ok(())
    }

    pub fn admit_child(
        &mut self,
        proposal: &RecursiveProposal,
    ) -> Result<RecursiveAdmission, RecursiveFailureReason> {
        if !recursive_enabled() {
            return Err(RecursiveFailureReason::RecursiveDisabled);
        }
        if kill_switch_active() {
            return Err(RecursiveFailureReason::RecursiveKillSwitchActive);
        }
        match self.execution_state {
            RecursiveExecutionState::Running => {}
            RecursiveExecutionState::OperatorPaused => {
                return Err(RecursiveFailureReason::OperatorPaused)
            }
            RecursiveExecutionState::BudgetExhausted => {
                return Err(RecursiveFailureReason::TreeBudgetExhausted)
            }
            RecursiveExecutionState::KillStopped => {
                return Err(RecursiveFailureReason::RecursiveKillSwitchActive)
            }
            RecursiveExecutionState::TerminalFailed => {
                return Err(RecursiveFailureReason::TerminalFailed)
            }
        }
        if !shape_is_valid(proposal) {
            return Err(RecursiveFailureReason::ProposalConflict);
        }
        if self
            .receipts
            .get(&proposal.proposal_id)
            .is_some_and(|receipt| receipt != &proposal.receipt_sha256)
        {
            return Err(RecursiveFailureReason::ReceiptConflict);
        }
        if self.accepted_proposals.contains(&proposal.proposal_id) {
            return Err(RecursiveFailureReason::ProposalConflict);
        }
        if self.rejected_proposals.contains_key(&proposal.proposal_id) {
            return Err(RecursiveFailureReason::ProposalConflict);
        }
        let parent = self
            .nodes
            .get(&proposal.parent_node_id)
            .cloned()
            .ok_or(RecursiveFailureReason::StaleParent)?;
        if parent.version != proposal.parent_version {
            return Err(RecursiveFailureReason::StaleParent);
        }
        if parent.depth >= MAX_RECURSIVE_DEPTH {
            return Err(RecursiveFailureReason::DepthExceeded);
        }
        if parent.accepted_children >= MAX_ACCEPTED_CHILDREN_PER_NODE {
            return Err(RecursiveFailureReason::ChildLimitExceeded);
        }
        if self.nodes.len() >= MAX_RECURSIVE_NODES_PER_ROOT {
            return Err(RecursiveFailureReason::TreeBudgetExhausted);
        }
        if !proposal
            .requested_capabilities
            .is_subset(&parent.capabilities)
        {
            return Err(RecursiveFailureReason::CapabilityEscalation);
        }
        if !proposal.requested_scope.is_subset_of(&parent.scope) {
            return Err(RecursiveFailureReason::ScopeMismatch);
        }
        let fingerprint = objective_fingerprint(&proposal.objective);
        if parent.ancestor_fingerprints.contains(&fingerprint)
            || parent.objective_fingerprint == fingerprint
        {
            return Err(RecursiveFailureReason::AncestorCycle);
        }
        if self
            .nodes
            .values()
            .any(|node| node.objective_fingerprint == fingerprint)
        {
            return Err(RecursiveFailureReason::DuplicateObjective);
        }
        // The proposal is only an upper bound. The control plane derives the
        // effective child budget from both remaining authorities, so a model
        // cannot choose a budget that bypasses the parent or whole-tree
        // allowance.
        let effective_budget = proposal
            .budget
            .bounded_by(&parent.child_budget)
            .bounded_by(&self.root_budget);
        if !effective_budget.is_nonzero() {
            return Err(RecursiveFailureReason::TreeBudgetExhausted);
        }
        if let Some(limit) = self.root_budget_limit.as_ref() {
            let mut projected_total = self.spent_budget.clone();
            projected_total.add(&self.reserved_budget);
            projected_total.add(&effective_budget);
            if !limit.can_spend(&projected_total) {
                return Err(RecursiveFailureReason::TreeBudgetExhausted);
            }
        }
        let node = RecursiveNode {
            node_id: derived_node_id(&self.root_run_id, &proposal.proposal_id, &fingerprint),
            root_run_id: self.root_run_id.clone(),
            parent_node_id: Some(parent.node_id.clone()),
            proposal_id: Some(proposal.proposal_id.clone()),
            depth: parent.depth + 1,
            objective_fingerprint: fingerprint.clone(),
            ancestor_fingerprints: parent
                .ancestor_fingerprints
                .iter()
                .cloned()
                .chain(std::iter::once(parent.objective_fingerprint.clone()))
                .collect(),
            scope: proposal.requested_scope.clone(),
            capabilities: proposal.requested_capabilities.clone(),
            tenant_id: parent.tenant_id.clone(),
            workspace_id: parent.workspace_id.clone(),
            budget: effective_budget.clone(),
            child_budget: effective_budget.clone(),
            reservation: effective_budget.clone(),
            actual_usage: RecursiveBudget::default(),
            status: "ready".to_string(),
            accepted_children: 0,
            retry_count: 0,
            version: 1,
            lease_id: None,
            failure_reason: None,
            evidence_refs: vec![format!("agent_proposal/{}", proposal.proposal_id)],
        };
        let parent_entry = self.nodes.get_mut(&parent.node_id).expect("parent checked");
        parent_entry.accepted_children += 1;
        parent_entry.child_budget.spend(&effective_budget);
        parent_entry.version += 1;
        self.root_budget.spend(&effective_budget);
        self.reserved_budget.add(&effective_budget);
        self.nodes.insert(node.node_id.clone(), node.clone());
        self.accepted_proposals.insert(proposal.proposal_id.clone());
        self.receipts.insert(
            proposal.proposal_id.clone(),
            proposal.receipt_sha256.clone(),
        );
        self.version += 1;
        Ok(RecursiveAdmission {
            node,
            parent_version: parent.version + 1,
        })
    }

    pub fn lease_node(
        &mut self,
        node_id: &str,
        lease_id: &str,
    ) -> Result<(), RecursiveFailureReason> {
        if !recursive_enabled() {
            return Err(RecursiveFailureReason::RecursiveDisabled);
        }
        if kill_switch_active() {
            return Err(RecursiveFailureReason::RecursiveKillSwitchActive);
        }
        match self.execution_state {
            RecursiveExecutionState::Running => {}
            RecursiveExecutionState::OperatorPaused => {
                return Err(RecursiveFailureReason::OperatorPaused)
            }
            RecursiveExecutionState::BudgetExhausted => {
                return Err(RecursiveFailureReason::TreeBudgetExhausted)
            }
            RecursiveExecutionState::KillStopped => {
                return Err(RecursiveFailureReason::RecursiveKillSwitchActive)
            }
            RecursiveExecutionState::TerminalFailed => {
                return Err(RecursiveFailureReason::TerminalFailed)
            }
        }
        if self.active_leases.len() >= MAX_RECURSIVE_LEASES {
            return Err(RecursiveFailureReason::SchedulerCapacityExhausted);
        }
        let node = self
            .nodes
            .get_mut(node_id)
            .ok_or(RecursiveFailureReason::StaleParent)?;
        if node.lease_id.is_some() || node.status != "ready" {
            return Err(RecursiveFailureReason::ProposalConflict);
        }
        node.lease_id = Some(lease_id.to_string());
        node.status = "leased".to_string();
        node.version += 1;
        self.active_leases.insert(lease_id.to_string());
        self.version += 1;
        Ok(())
    }

    pub fn complete_node(
        &mut self,
        node_id: &str,
        lease_id: &str,
        success: bool,
    ) -> Result<(), RecursiveFailureReason> {
        let result = self.complete_node_with_usage(
            node_id,
            lease_id,
            success,
            &RecursiveBudget {
                calls_remaining: 0,
                tokens_remaining: 0,
                cost_micros_remaining: 0,
                time_ms_remaining: 0,
            },
        );
        if result.is_ok() {
            self.release_node_reservation_for_persistence(node_id);
        }
        result
    }

    pub fn complete_node_with_usage(
        &mut self,
        node_id: &str,
        lease_id: &str,
        success: bool,
        usage: &RecursiveBudget,
    ) -> Result<(), RecursiveFailureReason> {
        let within_node_budget = {
            let node = self
                .nodes
                .get(node_id)
                .ok_or(RecursiveFailureReason::StaleParent)?;
            if node.lease_id.as_deref() != Some(lease_id) {
                return Err(RecursiveFailureReason::ReceiptConflict);
            }
            node.budget.can_spend(usage)
        };
        let within_tree_budget = self.record_usage(lease_id, usage)?;
        let within_budget = within_node_budget && within_tree_budget;
        let node = self
            .nodes
            .get_mut(node_id)
            .ok_or(RecursiveFailureReason::StaleParent)?;
        node.budget.spend(usage);
        node.actual_usage.add(usage);
        node.reservation.spend(usage);
        node.status = if success && within_budget {
            "completed"
        } else {
            "failed"
        }
        .to_string();
        node.failure_reason = if !within_budget {
            Some(
                RecursiveFailureReason::TreeBudgetExhausted
                    .as_str()
                    .to_string(),
            )
        } else if !success && node.retry_count >= MAX_RECURSIVE_RETRIES {
            Some(RecursiveFailureReason::RetryExhausted.as_str().to_string())
        } else {
            (!success).then(|| {
                RecursiveFailureReason::ExecutionFailure
                    .as_str()
                    .to_string()
            })
        };
        node.lease_id = None;
        node.version += 1;
        self.active_leases.remove(lease_id);
        if !within_budget {
            self.execution_state = RecursiveExecutionState::BudgetExhausted;
        } else if !success {
            self.execution_state = RecursiveExecutionState::TerminalFailed;
        }
        self.version += 1;
        Ok(())
    }

    pub(crate) fn release_node_reservation_for_persistence(&mut self, node_id: &str) {
        let (parent_node_id, remaining) = {
            let Some(node) = self.nodes.get_mut(node_id) else {
                return;
            };
            let remaining = std::mem::take(&mut node.reservation);
            (node.parent_node_id.clone(), remaining)
        };
        self.reserved_budget.spend(&remaining);
        self.root_budget.add(&remaining);
        if let Some(parent_node_id) = parent_node_id {
            if let Some(parent) = self.nodes.get_mut(&parent_node_id) {
                parent.child_budget.add(&remaining);
                parent.version += 1;
            }
        }
        self.version += 1;
    }

    fn record_usage(
        &mut self,
        receipt_id: &str,
        usage: &RecursiveBudget,
    ) -> Result<bool, RecursiveFailureReason> {
        let usage_fingerprint =
            serde_json::to_string(usage).map_err(|_| RecursiveFailureReason::ReceiptConflict)?;
        if let Some(previous) = self.usage_receipts.get(receipt_id) {
            if let Some(previous_usage) = previous.strip_prefix("1:") {
                if previous_usage == usage_fingerprint {
                    return Ok(true);
                }
            } else if let Some(previous_usage) = previous.strip_prefix("0:") {
                if previous_usage == usage_fingerprint {
                    return Ok(false);
                }
            }
            return Err(RecursiveFailureReason::ReceiptConflict);
        }
        let within_tree_budget = self
            .root_budget_limit
            .as_ref()
            .map(|limit| {
                let mut after_spent = self.spent_budget.clone();
                after_spent.add(usage);
                let mut after_reserved = self.reserved_budget.clone();
                after_reserved.spend(usage);
                let mut after_total = after_spent.clone();
                after_total.add(&after_reserved);
                limit.can_spend(&after_total)
            })
            .unwrap_or(true);
        self.spent_budget.add(usage);
        self.reserved_budget.spend(usage);
        self.usage_receipts.insert(
            receipt_id.to_string(),
            format!(
                "{}:{usage_fingerprint}",
                if within_tree_budget { 1 } else { 0 }
            ),
        );
        if !within_tree_budget {
            self.execution_state = RecursiveExecutionState::BudgetExhausted;
            self.version += 1;
        }
        Ok(within_tree_budget)
    }

    /// Account a result that arrived after its workflow lease was replaced or
    /// terminalized. The result cannot mutate node state, but its measured
    /// usage is still charged exactly once to the persisted tree budget.
    pub(crate) fn record_late_usage(
        &mut self,
        node_id: &str,
        attempt_receipt: &str,
        usage: &RecursiveBudget,
    ) -> Result<bool, RecursiveFailureReason> {
        if !self.nodes.contains_key(node_id) {
            return Err(RecursiveFailureReason::StaleParent);
        }
        self.record_usage(attempt_receipt, usage)
    }

    pub fn retry_node(&mut self, node_id: &str) -> Result<(), RecursiveFailureReason> {
        let node = self
            .nodes
            .get_mut(node_id)
            .ok_or(RecursiveFailureReason::StaleParent)?;
        if node.retry_count >= MAX_RECURSIVE_RETRIES {
            // Keep the node terminal when the scheduler asks to recover a
            // second stale lease.  Returning an error here would roll back
            // the surrounding queue transaction and leave the workflow node
            // pending forever.
            node.status = "failed".to_string();
            node.failure_reason = Some(RecursiveFailureReason::RetryExhausted.as_str().to_string());
            self.execution_state = RecursiveExecutionState::TerminalFailed;
            node.version += 1;
            self.version += 1;
            return Ok(());
        }
        node.retry_count += 1;
        node.status = "ready".to_string();
        node.failure_reason = None;
        if self.execution_state == RecursiveExecutionState::TerminalFailed {
            self.execution_state = RecursiveExecutionState::Running;
        }
        node.version += 1;
        self.version += 1;
        Ok(())
    }

    pub(crate) fn retry_allowed(
        &self,
        node_id: &str,
        usage: &RecursiveBudget,
    ) -> Result<bool, RecursiveFailureReason> {
        let node = self
            .nodes
            .get(node_id)
            .ok_or(RecursiveFailureReason::StaleParent)?;
        let mut remaining = node.budget.clone();
        let current_attempt_fits = remaining.can_spend(usage);
        if current_attempt_fits {
            remaining.spend(usage);
        }
        Ok(recursive_enabled()
            && !kill_switch_active()
            && self.execution_state == RecursiveExecutionState::Running
            && node.retry_count < MAX_RECURSIVE_RETRIES
            && current_attempt_fits
            // The retry reuses the existing reservation. The stale attempt
            // has no authoritative provider usage, so it must not consume the
            // node's final execution allowance merely to qualify for retry.
            && remaining.calls_remaining > 0)
    }

    pub fn pause(&mut self) {
        if self.execution_state == RecursiveExecutionState::Running {
            self.execution_state = RecursiveExecutionState::OperatorPaused;
        }
        self.version += 1;
    }

    pub fn resume(&mut self) {
        if self.execution_state == RecursiveExecutionState::OperatorPaused {
            self.execution_state = RecursiveExecutionState::Running;
            self.version += 1;
        }
    }

    pub fn record_rejection(&mut self, proposal_id: &str, reason: RecursiveFailureReason) {
        self.record_rejection_evidence(
            proposal_id,
            RecursiveDecisionEvidence {
                reason_code: reason.as_str().to_string(),
                evidence_refs: vec![format!("agent_proposal/{proposal_id}")],
            },
        );
    }

    pub(crate) fn record_rejection_evidence(
        &mut self,
        proposal_id: &str,
        evidence: RecursiveDecisionEvidence,
    ) {
        self.rejected_proposals
            .insert(proposal_id.to_string(), evidence);
        self.version += 1;
    }

    pub fn redacted_read_model(&self) -> Value {
        let nodes: Vec<Value> = self
            .nodes
            .values()
            .map(|node| {
                json!({
                    "node_id": node.node_id,
                    "parent_node_id": node.parent_node_id,
                    "proposal_id": node.proposal_id,
                    "depth": node.depth,
                    "objective_fingerprint": node.objective_fingerprint,
                    "ancestor_count": node.ancestor_fingerprints.len(),
                    "status": node.status,
                    "accepted_children": node.accepted_children,
                    "retry_count": node.retry_count,
                    "version": node.version,
                    "leased": node.lease_id.is_some(),
                    "failure_reason": node.failure_reason,
                    "evidence_refs": node.evidence_refs,
                    "budget": {
                        "calls_remaining": node.budget.calls_remaining,
                        "tokens_remaining": node.budget.tokens_remaining,
                        "cost_micros_remaining": node.budget.cost_micros_remaining,
                        "time_ms_remaining": node.budget.time_ms_remaining,
                    },
                    "child_budget": {
                        "calls_remaining": node.child_budget.calls_remaining,
                        "tokens_remaining": node.child_budget.tokens_remaining,
                        "cost_micros_remaining": node.child_budget.cost_micros_remaining,
                        "time_ms_remaining": node.child_budget.time_ms_remaining,
                    },
                    "reservation": {
                        "calls_remaining": node.reservation.calls_remaining,
                        "tokens_remaining": node.reservation.tokens_remaining,
                        "cost_micros_remaining": node.reservation.cost_micros_remaining,
                        "time_ms_remaining": node.reservation.time_ms_remaining,
                    },
                    "actual_usage": {
                        "calls": node.actual_usage.calls_remaining,
                        "tokens": node.actual_usage.tokens_remaining,
                        "cost_micros": node.actual_usage.cost_micros_remaining,
                        "time_ms": node.actual_usage.time_ms_remaining,
                    },
                    "tenant_bound": node.tenant_id.is_some(),
                    "workspace_bound": node.workspace_id.is_some(),
                })
            })
            .collect();
        json!({
            "schema_version": self.schema_version,
            "root_run_id": self.root_run_id,
            "workflow_id": self.workflow_id,
            "root_node_id": self.root_node_id,
            "execution_state": self.execution_state,
            "version": self.version,
            "node_count": self.nodes.len(),
            "active_lease_count": self.active_leases.len(),
            "accepted_proposal_count": self.accepted_proposals.len(),
            "rejected_proposal_count": self.rejected_proposals.len(),
            "rejected_proposals": self.rejected_proposals,
            "spent_budget": {
                "calls": self.spent_budget.calls_remaining,
                "tokens": self.spent_budget.tokens_remaining,
                "cost_micros": self.spent_budget.cost_micros_remaining,
                "time_ms": self.spent_budget.time_ms_remaining,
            },
            "reserved_budget": {
                "calls": self.reserved_budget.calls_remaining,
                "tokens": self.reserved_budget.tokens_remaining,
                "cost_micros": self.reserved_budget.cost_micros_remaining,
                "time_ms": self.reserved_budget.time_ms_remaining,
            },
            "nodes": nodes,
        })
    }
}

#[cfg(test)]
pub(crate) fn test_env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope() -> RecursiveScope {
        RecursiveScope {
            repository: Some("fixture".to_string()),
            allowed_paths: ["docs/".to_string()].into_iter().collect(),
            capabilities: ["read".to_string(), "write".to_string()]
                .into_iter()
                .collect(),
        }
    }

    fn budget() -> RecursiveBudget {
        RecursiveBudget {
            calls_remaining: 10,
            tokens_remaining: 100,
            cost_micros_remaining: 100,
            time_ms_remaining: 1000,
        }
    }

    #[test]
    fn root_node_identity_binds_run_and_objective() {
        let first = RecursiveTree::new(
            "root-identity-run-a",
            "root-identity-workflow",
            "review docs",
            scope(),
            BTreeSet::from(["read".to_string()]),
            budget(),
        );
        let second = RecursiveTree::new(
            "root-identity-run-b",
            "root-identity-workflow",
            "review docs",
            scope(),
            BTreeSet::from(["read".to_string()]),
            budget(),
        );
        assert_ne!(first.root_node_id, second.root_node_id);
        assert_eq!(
            first.nodes[&first.root_node_id].objective_fingerprint,
            second.nodes[&second.root_node_id].objective_fingerprint
        );
    }

    fn proposal(tree: &RecursiveTree) -> RecursiveProposal {
        RecursiveProposal {
            proposal_id: "proposal-1".to_string(),
            parent_node_id: tree.root_node_id.clone(),
            parent_version: 1,
            objective: "  child   objective ".to_string(),
            context_summary: "fixture context".to_string(),
            requested_scope: scope(),
            requested_capabilities: ["read".to_string()].into_iter().collect(),
            budget: RecursiveBudget {
                calls_remaining: 1,
                tokens_remaining: 10,
                cost_micros_remaining: 1,
                time_ms_remaining: 10,
            },
            receipt_sha256: "receipt-1".to_string(),
        }
    }

    #[test]
    fn default_off_and_kill_switch_are_fail_closed() {
        let _guard = test_env_lock().lock().unwrap();
        std::env::remove_var("ACP_RECURSIVE_EXECUTION_ENABLED");
        std::env::remove_var("ACP_RECURSIVE_EXECUTION_KILL_SWITCH");
        let mut tree = RecursiveTree::new(
            "run",
            "fixture",
            "root",
            scope(),
            ["read".to_string()].into_iter().collect(),
            budget(),
        );
        assert_eq!(
            tree.admit_child(&proposal(&tree)),
            Err(RecursiveFailureReason::RecursiveDisabled)
        );
        std::env::set_var("ACP_RECURSIVE_EXECUTION_ENABLED", "1");
        std::env::set_var("ACP_RECURSIVE_EXECUTION_KILL_SWITCH", "1");
        assert_eq!(
            tree.admit_child(&proposal(&tree)),
            Err(RecursiveFailureReason::RecursiveKillSwitchActive)
        );
        std::env::remove_var("ACP_RECURSIVE_EXECUTION_ENABLED");
        std::env::remove_var("ACP_RECURSIVE_EXECUTION_KILL_SWITCH");
    }

    #[test]
    fn admission_is_deterministic_and_narrowing() {
        let _guard = test_env_lock().lock().unwrap();
        std::env::set_var("ACP_RECURSIVE_EXECUTION_ENABLED", "1");
        std::env::remove_var("ACP_RECURSIVE_EXECUTION_KILL_SWITCH");
        let mut tree = RecursiveTree::new(
            "run",
            "fixture",
            "root",
            scope(),
            ["read".to_string(), "write".to_string()]
                .into_iter()
                .collect(),
            budget(),
        );
        let p = proposal(&tree);
        let admitted = tree.admit_child(&p).unwrap();
        assert_eq!(admitted.node.depth, 1);
        assert_eq!(admitted.node.scope.repository, Some("fixture".to_string()));
        assert_eq!(
            objective_fingerprint("a  b"),
            objective_fingerprint(" a b ")
        );
        assert_eq!(
            objective_fingerprint("Please inspect the documentation"),
            objective_fingerprint("review docs")
        );
        assert_ne!(
            objective_fingerprint("review docs"),
            objective_fingerprint("review source code")
        );
        assert_eq!(
            tree.admit_child(&p),
            Err(RecursiveFailureReason::ProposalConflict)
        );
        std::env::remove_var("ACP_RECURSIVE_EXECUTION_ENABLED");
    }

    #[test]
    fn limits_and_operator_evidence_are_bounded() {
        let _guard = test_env_lock().lock().unwrap();
        std::env::set_var("ACP_RECURSIVE_EXECUTION_ENABLED", "1");
        let mut tree = RecursiveTree::new(
            "run",
            "fixture",
            "root",
            scope(),
            ["read".to_string()].into_iter().collect(),
            budget(),
        );
        let p = proposal(&tree);
        let admitted = tree.admit_child(&p).unwrap();
        assert_eq!(tree.lease_node(&admitted.node.node_id, "lease-1"), Ok(()));
        assert_eq!(
            tree.complete_node(&admitted.node.node_id, "lease-1", false),
            Ok(())
        );
        assert_eq!(tree.retry_node(&admitted.node.node_id), Ok(()));
        assert_eq!(tree.retry_node(&admitted.node.node_id), Ok(()));
        let node = tree.nodes.get(&admitted.node.node_id).expect("node");
        assert_eq!(node.status, "failed");
        assert_eq!(
            node.failure_reason.as_deref(),
            Some(RecursiveFailureReason::RetryExhausted.as_str())
        );
        let evidence = tree.redacted_read_model();
        assert!(evidence["nodes"]
            .to_string()
            .contains("objective_fingerprint"));
        assert!(!evidence["nodes"].to_string().contains("child objective"));
        std::env::remove_var("ACP_RECURSIVE_EXECUTION_ENABLED");
    }

    #[test]
    fn child_budget_is_derived_from_remaining_authorities() {
        let _guard = test_env_lock().lock().unwrap();
        std::env::set_var("ACP_RECURSIVE_EXECUTION_ENABLED", "1");
        let mut tree = RecursiveTree::new(
            "run-budget",
            "fixture",
            "root",
            scope(),
            ["read".to_string()].into_iter().collect(),
            budget(),
        );
        let mut child_proposal = proposal(&tree);
        child_proposal.budget = RecursiveBudget {
            calls_remaining: 100,
            tokens_remaining: 10_000,
            cost_micros_remaining: 10_000,
            time_ms_remaining: 60_000,
        };
        let admitted = tree
            .admit_child(&child_proposal)
            .expect("bounded admission");
        assert_eq!(admitted.node.budget, budget());
        assert_eq!(
            tree.root_budget,
            RecursiveBudget {
                calls_remaining: 0,
                tokens_remaining: 0,
                cost_micros_remaining: 0,
                time_ms_remaining: 0,
            }
        );
        std::env::remove_var("ACP_RECURSIVE_EXECUTION_ENABLED");
    }

    #[test]
    fn depth_two_tree_separates_reservation_from_actual_usage() {
        let _guard = test_env_lock().lock().unwrap();
        std::env::set_var("ACP_RECURSIVE_EXECUTION_ENABLED", "1");
        let mut tree = RecursiveTree::new(
            "depth-two-run",
            "fixture",
            "root objective",
            scope(),
            ["read".to_string()].into_iter().collect(),
            RecursiveBudget {
                calls_remaining: 10,
                tokens_remaining: 100,
                cost_micros_remaining: 100,
                time_ms_remaining: 1000,
            },
        );
        let child = tree
            .admit_child(&RecursiveProposal {
                proposal_id: "child-proposal".to_string(),
                parent_node_id: tree.root_node_id.clone(),
                parent_version: 1,
                objective: "review child docs".to_string(),
                context_summary: "fixture".to_string(),
                requested_scope: scope(),
                requested_capabilities: ["read".to_string()].into_iter().collect(),
                budget: RecursiveBudget {
                    calls_remaining: 3,
                    tokens_remaining: 30,
                    cost_micros_remaining: 30,
                    time_ms_remaining: 300,
                },
                receipt_sha256: "child-receipt".to_string(),
            })
            .expect("child admitted");
        tree.lease_node(&child.node.node_id, "child-lease")
            .expect("child lease");
        let grandchild = tree
            .admit_child(&RecursiveProposal {
                proposal_id: "grandchild-proposal".to_string(),
                parent_node_id: child.node.node_id.clone(),
                parent_version: child.node.version + 1,
                objective: "review grandchild docs".to_string(),
                context_summary: "fixture".to_string(),
                requested_scope: scope(),
                requested_capabilities: ["read".to_string()].into_iter().collect(),
                budget: RecursiveBudget {
                    calls_remaining: 2,
                    tokens_remaining: 20,
                    cost_micros_remaining: 20,
                    time_ms_remaining: 200,
                },
                receipt_sha256: "grandchild-receipt".to_string(),
            })
            .expect("grandchild admitted");
        tree.complete_node_with_usage(
            &child.node.node_id,
            "child-lease",
            true,
            &RecursiveBudget {
                calls_remaining: 1,
                tokens_remaining: 10,
                cost_micros_remaining: 10,
                time_ms_remaining: 100,
            },
        )
        .expect("child completes");
        tree.release_node_reservation_for_persistence(&child.node.node_id);
        tree.lease_node(&grandchild.node.node_id, "grandchild-lease")
            .expect("grandchild lease");
        tree.complete_node(&grandchild.node.node_id, "grandchild-lease", true)
            .expect("grandchild completes");
        assert_eq!(tree.nodes[&child.node.node_id].status, "completed");
        assert_eq!(tree.nodes[&grandchild.node.node_id].status, "completed");
        assert_eq!(tree.reserved_budget, RecursiveBudget::default());
        assert_eq!(tree.spent_budget.calls_remaining, 1);
        assert_eq!(tree.spent_budget.tokens_remaining, 10);
        std::env::remove_var("ACP_RECURSIVE_EXECUTION_ENABLED");
    }

    #[test]
    fn terminal_states_cannot_be_reopened_by_resume() {
        let mut paused = RecursiveTree::new(
            "state-run",
            "fixture",
            "root",
            scope(),
            ["read".to_string()].into_iter().collect(),
            budget(),
        );
        paused.pause();
        assert_eq!(
            paused.execution_state,
            RecursiveExecutionState::OperatorPaused
        );
        paused.resume();
        assert_eq!(paused.execution_state, RecursiveExecutionState::Running);

        paused.execution_state = RecursiveExecutionState::BudgetExhausted;
        paused.resume();
        assert_eq!(
            paused.execution_state,
            RecursiveExecutionState::BudgetExhausted
        );
        paused.execution_state = RecursiveExecutionState::KillStopped;
        paused.resume();
        assert_eq!(paused.execution_state, RecursiveExecutionState::KillStopped);
        paused.execution_state = RecursiveExecutionState::TerminalFailed;
        paused.resume();
        assert_eq!(
            paused.execution_state,
            RecursiveExecutionState::TerminalFailed
        );
    }

    #[test]
    fn stale_lease_retries_once_and_late_completion_cannot_replace_attempt() {
        let _guard = test_env_lock().lock().unwrap();
        std::env::set_var("ACP_RECURSIVE_EXECUTION_ENABLED", "1");
        let mut tree = RecursiveTree::new(
            "stale-retry-run",
            "fixture",
            "root",
            scope(),
            ["read".to_string()].into_iter().collect(),
            budget(),
        );
        let admission = tree.admit_child(&proposal(&tree)).expect("child");
        tree.lease_node(&admission.node.node_id, "lease-one")
            .expect("lease");
        let zero = RecursiveBudget::default();
        assert!(tree
            .retry_allowed(&admission.node.node_id, &zero)
            .expect("retry allowed"));
        tree.complete_node_with_usage(&admission.node.node_id, "lease-one", false, &zero)
            .expect("stale completion");
        tree.retry_node(&admission.node.node_id).expect("one retry");
        tree.lease_node(&admission.node.node_id, "lease-two")
            .expect("replacement lease");
        assert!(!tree
            .retry_allowed(&admission.node.node_id, &zero)
            .expect("retry exhausted"));
        tree.complete_node_with_usage(&admission.node.node_id, "lease-two", false, &zero)
            .expect("second stale completion");
        tree.retry_node(&admission.node.node_id)
            .expect("terminalize");
        let status = tree.nodes[&admission.node.node_id].status.clone();
        let reason = tree.nodes[&admission.node.node_id].failure_reason.clone();
        tree.record_late_usage(
            &admission.node.node_id,
            "late-replacement-attempt",
            &RecursiveBudget {
                calls_remaining: 1,
                tokens_remaining: 1,
                cost_micros_remaining: 1,
                time_ms_remaining: 1,
            },
        )
        .expect("late usage is accounted");
        assert_eq!(tree.nodes[&admission.node.node_id].status, status);
        assert_eq!(tree.nodes[&admission.node.node_id].failure_reason, reason);
        std::env::remove_var("ACP_RECURSIVE_EXECUTION_ENABLED");
    }
}
