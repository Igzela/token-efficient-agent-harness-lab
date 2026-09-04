use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub const LEDGER_ORCHESTRATED_SCHEMA_VERSION: &str = "harness_evolution_ledger_orchestration.v1";
pub const DEFAULT_MAX_LEDGER_TASKS: usize = 32;
pub const DEFAULT_MAX_LEDGER_FINDINGS: usize = 64;
pub const DEFAULT_MAX_LEDGER_OBSERVATIONS: usize = 64;
pub const DEFAULT_MAX_SUMMARY_BYTES: usize = 4096;
pub const DEFAULT_MAX_LEDGER_BYTES: usize = 128 * 1024;
pub const DEFAULT_MAX_ORCHESTRATION_ROUNDS: u32 = 30;
pub const DEFAULT_MAX_TASK_ATTEMPTS: u32 = 3;
pub const DEFAULT_MAX_NO_PROGRESS_ROUNDS: u32 = 2;
pub const DEFAULT_MAX_TRUNCATION_RECOVERIES: u32 = 3;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LedgerOrchestratorConfig {
    pub max_tasks: usize,
    pub max_findings: usize,
    pub max_observations: usize,
    pub max_summary_bytes: usize,
    pub max_ledger_bytes: usize,
    pub max_orchestration_rounds: u32,
    pub max_task_attempts: u32,
    pub max_no_progress_rounds: u32,
    pub max_truncations: u32,
}

impl Default for LedgerOrchestratorConfig {
    fn default() -> Self {
        Self {
            max_tasks: DEFAULT_MAX_LEDGER_TASKS,
            max_findings: DEFAULT_MAX_LEDGER_FINDINGS,
            max_observations: DEFAULT_MAX_LEDGER_OBSERVATIONS,
            max_summary_bytes: DEFAULT_MAX_SUMMARY_BYTES,
            max_ledger_bytes: DEFAULT_MAX_LEDGER_BYTES,
            max_orchestration_rounds: DEFAULT_MAX_ORCHESTRATION_ROUNDS,
            max_task_attempts: DEFAULT_MAX_TASK_ATTEMPTS,
            max_no_progress_rounds: DEFAULT_MAX_NO_PROGRESS_ROUNDS,
            max_truncations: DEFAULT_MAX_TRUNCATION_RECOVERIES,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LedgerTaskStatus {
    Pending,
    Selected,
    Running,
    Verified,
    Failed,
    Blocked,
    Superseded,
}

impl LedgerTaskStatus {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Verified | Self::Failed | Self::Superseded)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LedgerTaskRecord {
    pub id: String,
    pub description: String,
    pub status: LedgerTaskStatus,
    pub result_digest: Option<String>,
    pub evidence_refs: Vec<String>,
    pub attempt_count: u32,
    pub failure_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LedgerFinding {
    pub id: String,
    pub summary: String,
    pub source: String,
    pub related_task_id: Option<String>,
    pub evidence_digest: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationOutcome {
    Pass,
    Fail,
    Inconclusive,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationObservation {
    pub round: u32,
    pub task_id: String,
    pub outcome: VerificationOutcome,
    pub observation_summary: String,
    pub evidence_digest: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkingLedger {
    pub schema_version: String,
    pub contract_digest: String,
    pub current_plan: String,
    pub tasks: Vec<LedgerTaskRecord>,
    pub findings: Vec<LedgerFinding>,
    pub artifact_refs: Vec<String>,
    pub observations: Vec<VerificationObservation>,
    pub current_selected_task_id: Option<String>,
    pub progress_fingerprint: String,
    pub round_count: u32,
    pub replan_count: u32,
    pub truncation_count: u32,
    pub no_progress_count: u32,
}

impl WorkingLedger {
    pub fn new(contract_digest: impl Into<String>, initial_plan: impl Into<String>) -> Self {
        let contract = contract_digest.into();
        let plan = initial_plan.into();
        let fingerprint = compute_progress_fingerprint(None, None, None, &contract);
        Self {
            schema_version: LEDGER_ORCHESTRATED_SCHEMA_VERSION.to_string(),
            contract_digest: contract,
            current_plan: plan,
            tasks: Vec::new(),
            findings: Vec::new(),
            artifact_refs: Vec::new(),
            observations: Vec::new(),
            current_selected_task_id: None,
            progress_fingerprint: fingerprint,
            round_count: 0,
            replan_count: 0,
            truncation_count: 0,
            no_progress_count: 0,
        }
    }

    pub fn add_task(
        &mut self,
        id: &str,
        description: &str,
        config: &LedgerOrchestratorConfig,
    ) -> Result<(), OrchestrationError> {
        validate_task_id(id)?;
        if self.tasks.iter().any(|t| t.id == id) {
            return Err(OrchestrationError::DuplicateTaskId(id.to_string()));
        }
        if self.tasks.len() >= config.max_tasks {
            return Err(OrchestrationError::OversizedField {
                field: "tasks".to_string(),
                current: self.tasks.len() + 1,
                max: config.max_tasks,
            });
        }
        if description.len() > config.max_summary_bytes {
            return Err(OrchestrationError::OversizedField {
                field: "task_description".to_string(),
                current: description.len(),
                max: config.max_summary_bytes,
            });
        }
        let sanitized = crate::provider::redaction::redact_sensitive_patterns(description);
        self.tasks.push(LedgerTaskRecord {
            id: id.to_string(),
            description: sanitized,
            status: LedgerTaskStatus::Pending,
            result_digest: None,
            evidence_refs: Vec::new(),
            attempt_count: 0,
            failure_reason: None,
        });
        self.validate_bounds(config)?;
        Ok(())
    }

    pub fn add_finding(
        &mut self,
        mut finding: LedgerFinding,
        config: &LedgerOrchestratorConfig,
    ) -> Result<(), OrchestrationError> {
        if self.findings.len() >= config.max_findings {
            return Err(OrchestrationError::OversizedField {
                field: "findings".to_string(),
                current: self.findings.len() + 1,
                max: config.max_findings,
            });
        }
        if finding.summary.len() > config.max_summary_bytes {
            return Err(OrchestrationError::OversizedField {
                field: "finding_summary".to_string(),
                current: finding.summary.len(),
                max: config.max_summary_bytes,
            });
        }
        finding.summary = crate::provider::redaction::redact_sensitive_patterns(&finding.summary);
        self.findings.push(finding);
        self.validate_bounds(config)?;
        Ok(())
    }

    pub fn add_observation(
        &mut self,
        mut observation: VerificationObservation,
        config: &LedgerOrchestratorConfig,
    ) -> Result<(), OrchestrationError> {
        if self.observations.len() >= config.max_observations {
            return Err(OrchestrationError::OversizedField {
                field: "observations".to_string(),
                current: self.observations.len() + 1,
                max: config.max_observations,
            });
        }
        if observation.observation_summary.len() > config.max_summary_bytes {
            return Err(OrchestrationError::OversizedField {
                field: "observation_summary".to_string(),
                current: observation.observation_summary.len(),
                max: config.max_summary_bytes,
            });
        }
        observation.observation_summary =
            crate::provider::redaction::redact_sensitive_patterns(&observation.observation_summary);
        self.observations.push(observation);
        self.validate_bounds(config)?;
        Ok(())
    }

    pub fn get_task(&self, id: &str) -> Option<&LedgerTaskRecord> {
        self.tasks.iter().find(|t| t.id == id)
    }

    pub fn get_task_mut(&mut self, id: &str) -> Option<&mut LedgerTaskRecord> {
        self.tasks.iter_mut().find(|t| t.id == id)
    }

    pub fn pending_task_ids(&self) -> Vec<String> {
        self.tasks
            .iter()
            .filter(|t| t.status == LedgerTaskStatus::Pending)
            .map(|t| t.id.clone())
            .collect()
    }

    pub fn all_verified(&self) -> bool {
        !self.tasks.is_empty()
            && self.tasks.iter().all(|t| {
                t.status == LedgerTaskStatus::Verified || t.status == LedgerTaskStatus::Superseded
            })
            && self
                .tasks
                .iter()
                .any(|t| t.status == LedgerTaskStatus::Verified)
    }

    pub fn validate_bounds(
        &self,
        config: &LedgerOrchestratorConfig,
    ) -> Result<(), OrchestrationError> {
        let bytes = self.estimate_bytes();
        if bytes > config.max_ledger_bytes {
            return Err(OrchestrationError::OversizedLedger {
                current: bytes,
                max: config.max_ledger_bytes,
            });
        }
        Ok(())
    }

    pub fn estimate_bytes(&self) -> usize {
        serde_json::to_vec(self).map(|v| v.len()).unwrap_or(0)
    }

    pub fn state_digest(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.contract_digest.as_bytes());
        for task in &self.tasks {
            hasher.update(task.id.as_bytes());
            hasher.update(format!("{:?}", task.status).as_bytes());
            if let Some(res) = &task.result_digest {
                hasher.update(res.as_bytes());
            }
            for art in &task.evidence_refs {
                hasher.update(art.as_bytes());
            }
        }
        for art in &self.artifact_refs {
            hasher.update(art.as_bytes());
        }
        hex::encode(hasher.finalize())
    }
}

pub fn validate_task_id(id: &str) -> Result<(), OrchestrationError> {
    if id.is_empty()
        || id.len() > 64
        || !id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
    {
        return Err(OrchestrationError::InvalidTaskId(id.to_string()));
    }
    Ok(())
}

pub fn compute_progress_fingerprint(
    task_id: Option<&str>,
    result_digest: Option<&str>,
    verif_digest: Option<&str>,
    ledger_state_digest: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(task_id.unwrap_or("none").as_bytes());
    hasher.update(b"|");
    hasher.update(result_digest.unwrap_or("none").as_bytes());
    hasher.update(b"|");
    hasher.update(verif_digest.unwrap_or("none").as_bytes());
    hasher.update(b"|");
    hasher.update(ledger_state_digest.as_bytes());
    hex::encode(hasher.finalize())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerContext {
    pub contract_digest: String,
    pub plan_summary: String,
    pub selected_task: LedgerTaskRecord,
    pub relevant_findings: Vec<LedgerFinding>,
    pub relevant_artifact_refs: Vec<String>,
    pub expected_evidence: String,
    pub execution_metadata: BTreeMap<String, String>,
}

pub fn project_worker_context(
    ledger: &WorkingLedger,
    task_id: &str,
    expected_evidence: &str,
) -> Result<WorkerContext, OrchestrationError> {
    let selected_task = ledger
        .get_task(task_id)
        .ok_or_else(|| OrchestrationError::TaskNotFound(task_id.to_string()))?
        .clone();

    // Deterministic fresh-context projection: worker sees only findings that are
    // either unbound (global discovered facts) or explicitly bound to this task.
    // Unrelated tasks and their private findings are strictly omitted.
    let relevant_findings: Vec<LedgerFinding> = ledger
        .findings
        .iter()
        .filter(|f| match &f.related_task_id {
            None => true,
            Some(rel) => rel == task_id,
        })
        .cloned()
        .collect();

    // Collect relevant artifacts
    let mut relevant_artifact_refs: Vec<String> = selected_task.evidence_refs.clone();
    for art in &ledger.artifact_refs {
        if !relevant_artifact_refs.contains(art) {
            relevant_artifact_refs.push(art.clone());
        }
    }

    let mut execution_metadata = BTreeMap::new();
    execution_metadata.insert("round".to_string(), ledger.round_count.to_string());
    execution_metadata.insert(
        "attempt".to_string(),
        selected_task.attempt_count.to_string(),
    );
    execution_metadata.insert("task_id".to_string(), task_id.to_string());

    Ok(WorkerContext {
        contract_digest: ledger.contract_digest.clone(),
        plan_summary: ledger.current_plan.clone(),
        selected_task,
        relevant_findings,
        relevant_artifact_refs,
        expected_evidence: expected_evidence.to_string(),
        execution_metadata,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerOutcomeStatus {
    Completed,
    Failed,
    Truncated,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerResult {
    pub status: WorkerOutcomeStatus,
    pub output_digest: Option<String>,
    pub partial_summary: Option<String>,
    pub artifact_refs: Vec<String>,
    pub findings: Vec<LedgerFinding>,
    pub failure_reason: Option<String>,
}

pub trait LedgerWorker {
    fn execute_task(&mut self, context: &WorkerContext)
        -> Result<WorkerResult, OrchestrationError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationReport {
    pub outcome: VerificationOutcome,
    pub observation_summary: String,
    pub evidence_digest: Option<String>,
}

pub trait LedgerVerifier {
    fn verify_task(
        &mut self,
        ledger: &WorkingLedger,
        task: &LedgerTaskRecord,
        result: &WorkerResult,
    ) -> Result<VerificationReport, OrchestrationError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewTaskSpec {
    pub id: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum ControllerAction {
    ExecuteTask {
        task_id: String,
    },
    Replan {
        new_plan_summary: Option<String>,
        new_tasks: Vec<NewTaskSpec>,
        supersede_task_ids: Vec<String>,
    },
    DeclareComplete {
        deliverable_digest: String,
    },
    DeclareFailed {
        reason: String,
    },
    DeclareNoProgress {
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NextTaskDecision {
    pub action: ControllerAction,
    pub reason_summary: String,
    pub expected_evidence: String,
}

pub trait LedgerController {
    fn decide_next_action(
        &mut self,
        ledger: &WorkingLedger,
    ) -> Result<NextTaskDecision, OrchestrationError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrchestrationLifecycleState {
    Initialized,
    SelectingAction,
    ExecutingWorker,
    Verifying,
    Replanning,
    Completed,
    Failed,
    NoProgress,
    Cancelled,
}

impl OrchestrationLifecycleState {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::NoProgress | Self::Cancelled
        )
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrchestrationMetrics {
    pub round_count: u32,
    pub manager_call_count: u32,
    pub worker_call_count: u32,
    pub verification_count: u32,
    pub replan_count: u32,
    pub failed_worker_attempts: u32,
    pub truncation_count: u32,
    pub no_progress_count: u32,
    pub verification_recovery_count: u32,
    pub context_input_bytes: u64,
    pub ledger_peak_bytes: usize,
    pub task_count: usize,
    pub prompt_tokens: Option<u64>,
    pub completion_tokens: Option<u64>,
    pub cost_usd_micros: Option<u64>,
    pub duration_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrchestrationSummary {
    pub terminal_state: OrchestrationLifecycleState,
    pub deliverable_digest: Option<String>,
    pub terminal_reason: String,
    pub rounds_executed: u32,
    pub metrics: OrchestrationMetrics,
    pub final_ledger_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrchestrationError {
    InvalidTaskId(String),
    DuplicateTaskId(String),
    TaskNotFound(String),
    OversizedLedger {
        current: usize,
        max: usize,
    },
    OversizedField {
        field: String,
        current: usize,
        max: usize,
    },
    MaxRoundsExceeded(u32),
    NoProgressLimitExceeded(u32),
    InvalidControllerDecision(String),
    WorkerExecutionError(String),
    VerificationError(String),
    SecretDetected(String),
    Cancelled,
}

impl std::fmt::Display for OrchestrationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidTaskId(id) => write!(f, "invalid task id: {id}"),
            Self::DuplicateTaskId(id) => write!(f, "duplicate task id: {id}"),
            Self::TaskNotFound(id) => write!(f, "task not found: {id}"),
            Self::OversizedLedger { current, max } => {
                write!(f, "oversized ledger: {current} bytes exceeds max {max}")
            }
            Self::OversizedField {
                field,
                current,
                max,
            } => {
                write!(f, "field {field} size {current} exceeds max {max}")
            }
            Self::MaxRoundsExceeded(r) => write!(f, "max orchestration rounds {r} exceeded"),
            Self::NoProgressLimitExceeded(r) => {
                write!(f, "no progress limit {r} consecutive rounds exceeded")
            }
            Self::InvalidControllerDecision(s) => write!(f, "invalid controller decision: {s}"),
            Self::WorkerExecutionError(s) => write!(f, "worker execution error: {s}"),
            Self::VerificationError(s) => write!(f, "verification error: {s}"),
            Self::SecretDetected(s) => write!(f, "credential or secret shape detected: {s}"),
            Self::Cancelled => write!(f, "orchestration cancelled"),
        }
    }
}

impl std::error::Error for OrchestrationError {}

pub struct LedgerOrchestrator<C: LedgerController, W: LedgerWorker, V: LedgerVerifier> {
    pub config: LedgerOrchestratorConfig,
    pub state: OrchestrationLifecycleState,
    pub ledger: WorkingLedger,
    pub controller: C,
    pub worker: W,
    pub verifier: V,
    pub metrics: OrchestrationMetrics,
    consecutive_no_progress: u32,
    last_fingerprint: Option<String>,
    pending_decision: Option<NextTaskDecision>,
    pub terminal_summary: Option<OrchestrationSummary>,
    cancelled: bool,
}

impl<C: LedgerController, W: LedgerWorker, V: LedgerVerifier> LedgerOrchestrator<C, W, V> {
    pub fn new(
        config: LedgerOrchestratorConfig,
        ledger: WorkingLedger,
        controller: C,
        worker: W,
        verifier: V,
    ) -> Self {
        let metrics = OrchestrationMetrics {
            task_count: ledger.tasks.len(),
            ledger_peak_bytes: ledger.estimate_bytes(),
            ..Default::default()
        };
        Self {
            config,
            state: OrchestrationLifecycleState::Initialized,
            ledger,
            controller,
            worker,
            verifier,
            metrics,
            consecutive_no_progress: 0,
            last_fingerprint: None,
            pending_decision: None,
            terminal_summary: None,
            cancelled: false,
        }
    }

    pub fn cancel(&mut self) {
        self.cancelled = true;
        self.state = OrchestrationLifecycleState::Cancelled;
    }

    pub fn step(&mut self) -> Result<OrchestrationLifecycleState, OrchestrationError> {
        if self.cancelled {
            self.state = OrchestrationLifecycleState::Cancelled;
            return Ok(self.state);
        }
        if self.state.is_terminal() {
            return Ok(self.state);
        }

        if self.ledger.round_count >= self.config.max_orchestration_rounds {
            self.state = OrchestrationLifecycleState::Failed;
            return Err(OrchestrationError::MaxRoundsExceeded(
                self.config.max_orchestration_rounds,
            ));
        }

        match self.state {
            OrchestrationLifecycleState::Initialized => {
                self.state = OrchestrationLifecycleState::SelectingAction;
                Ok(self.state)
            }
            OrchestrationLifecycleState::SelectingAction => {
                self.metrics.manager_call_count += 1;
                let decision = self.controller.decide_next_action(&self.ledger)?;
                match &decision.action {
                    ControllerAction::DeclareComplete {
                        deliverable_digest: _,
                    } => {
                        self.state = OrchestrationLifecycleState::Completed;
                        self.pending_decision = Some(decision);
                        Ok(self.state)
                    }
                    ControllerAction::DeclareFailed { reason: _ } => {
                        self.state = OrchestrationLifecycleState::Failed;
                        self.pending_decision = Some(decision);
                        Ok(self.state)
                    }
                    ControllerAction::DeclareNoProgress { reason: _ } => {
                        self.metrics.no_progress_count += 1;
                        self.ledger.no_progress_count += 1;
                        self.state = OrchestrationLifecycleState::NoProgress;
                        self.pending_decision = Some(decision);
                        Ok(self.state)
                    }
                    ControllerAction::Replan {
                        new_plan_summary,
                        new_tasks,
                        supersede_task_ids,
                    } => {
                        self.metrics.replan_count += 1;
                        self.ledger.replan_count += 1;
                        if let Some(plan) = new_plan_summary {
                            self.ledger.current_plan =
                                crate::provider::redaction::redact_sensitive_patterns(plan);
                        }
                        for id in supersede_task_ids {
                            if let Some(task) = self.ledger.get_task_mut(id) {
                                task.status = LedgerTaskStatus::Superseded;
                            }
                        }
                        for new_t in new_tasks {
                            self.ledger
                                .add_task(&new_t.id, &new_t.description, &self.config)?;
                        }
                        self.metrics.task_count = self.ledger.tasks.len();
                        self.state = OrchestrationLifecycleState::SelectingAction;
                        Ok(self.state)
                    }
                    ControllerAction::ExecuteTask { task_id } => {
                        let task = self
                            .ledger
                            .get_task_mut(task_id)
                            .ok_or_else(|| OrchestrationError::TaskNotFound(task_id.clone()))?;
                        if task.status.is_terminal() {
                            return Err(OrchestrationError::InvalidControllerDecision(format!(
                                "cannot execute task {task_id} with terminal status {:?}",
                                task.status
                            )));
                        }
                        task.status = LedgerTaskStatus::Selected;
                        self.ledger.current_selected_task_id = Some(task_id.clone());
                        self.pending_decision = Some(decision);
                        self.state = OrchestrationLifecycleState::ExecutingWorker;
                        Ok(self.state)
                    }
                }
            }
            OrchestrationLifecycleState::ExecutingWorker => {
                let task_id = self
                    .ledger
                    .current_selected_task_id
                    .clone()
                    .ok_or_else(|| {
                        OrchestrationError::InvalidControllerDecision(
                            "missing selected task in ExecutingWorker state".into(),
                        )
                    })?;
                let decision = self.pending_decision.take().ok_or_else(|| {
                    OrchestrationError::InvalidControllerDecision(
                        "missing controller decision in ExecutingWorker state".into(),
                    )
                })?;

                self.ledger.round_count += 1;
                self.metrics.round_count += 1;

                let context =
                    project_worker_context(&self.ledger, &task_id, &decision.expected_evidence)?;
                let context_bytes = serde_json::to_vec(&context).map(|v| v.len()).unwrap_or(0);
                self.metrics.context_input_bytes += context_bytes as u64;

                // Set status to Running and increment attempt count
                if let Some(task) = self.ledger.get_task_mut(&task_id) {
                    task.status = LedgerTaskStatus::Running;
                    task.attempt_count += 1;
                }

                self.metrics.worker_call_count += 1;
                let worker_result = self.worker.execute_task(&context)?;

                // Handle worker outcomes
                match worker_result.status {
                    WorkerOutcomeStatus::Completed => {
                        // Update task with output
                        if let Some(task) = self.ledger.get_task_mut(&task_id) {
                            task.result_digest = worker_result.output_digest.clone();
                            for art in &worker_result.artifact_refs {
                                if !task.evidence_refs.contains(art) {
                                    task.evidence_refs.push(art.clone());
                                }
                            }
                        }
                        for art in &worker_result.artifact_refs {
                            if !self.ledger.artifact_refs.contains(art) {
                                self.ledger.artifact_refs.push(art.clone());
                            }
                        }
                        for f in &worker_result.findings {
                            self.ledger.add_finding(f.clone(), &self.config)?;
                        }

                        self.state = OrchestrationLifecycleState::Verifying;
                        self.pending_decision = Some(NextTaskDecision {
                            action: ControllerAction::ExecuteTask {
                                task_id: task_id.clone(),
                            },
                            reason_summary: decision.reason_summary,
                            expected_evidence: decision.expected_evidence,
                        });
                        // Pass worker result along via state
                        self.verify_worker_result(&task_id, &worker_result)?;
                        Ok(self.state)
                    }
                    WorkerOutcomeStatus::Truncated => {
                        self.metrics.truncation_count += 1;
                        self.ledger.truncation_count += 1;
                        // Recover bounded partial information
                        if let Some(summary) = &worker_result.partial_summary {
                            self.ledger.add_finding(
                                LedgerFinding {
                                    id: format!(
                                        "truncation-recovery-{task_id}-{}",
                                        self.ledger.round_count
                                    ),
                                    summary: summary.clone(),
                                    source: "worker_truncation_recovery".to_string(),
                                    related_task_id: Some(task_id.clone()),
                                    evidence_digest: worker_result.output_digest.clone(),
                                },
                                &self.config,
                            )?;
                        }
                        for f in &worker_result.findings {
                            self.ledger.add_finding(f.clone(), &self.config)?;
                        }

                        let attempt = self
                            .ledger
                            .get_task(&task_id)
                            .map(|t| t.attempt_count)
                            .unwrap_or(0);
                        if attempt >= self.config.max_task_attempts {
                            if let Some(task) = self.ledger.get_task_mut(&task_id) {
                                task.status = LedgerTaskStatus::Failed;
                                task.failure_reason =
                                    Some("worker truncation attempt limit reached".into());
                            }
                        } else {
                            if let Some(task) = self.ledger.get_task_mut(&task_id) {
                                task.status = LedgerTaskStatus::Pending;
                            }
                        }

                        self.state = OrchestrationLifecycleState::SelectingAction;
                        Ok(self.state)
                    }
                    WorkerOutcomeStatus::Failed => {
                        self.metrics.failed_worker_attempts += 1;
                        let attempt = self
                            .ledger
                            .get_task(&task_id)
                            .map(|t| t.attempt_count)
                            .unwrap_or(0);
                        if attempt >= self.config.max_task_attempts {
                            if let Some(task) = self.ledger.get_task_mut(&task_id) {
                                task.status = LedgerTaskStatus::Failed;
                                task.failure_reason = worker_result.failure_reason.clone();
                            }
                        } else {
                            if let Some(task) = self.ledger.get_task_mut(&task_id) {
                                task.status = LedgerTaskStatus::Pending;
                                task.failure_reason = worker_result.failure_reason.clone();
                            }
                        }
                        self.state = OrchestrationLifecycleState::SelectingAction;
                        Ok(self.state)
                    }
                    WorkerOutcomeStatus::Blocked => {
                        if let Some(task) = self.ledger.get_task_mut(&task_id) {
                            task.status = LedgerTaskStatus::Blocked;
                            task.failure_reason = worker_result.failure_reason.clone();
                        }
                        self.state = OrchestrationLifecycleState::SelectingAction;
                        Ok(self.state)
                    }
                }
            }
            OrchestrationLifecycleState::Verifying => {
                // Handled in verify_worker_result
                self.state = OrchestrationLifecycleState::SelectingAction;
                Ok(self.state)
            }
            OrchestrationLifecycleState::Replanning => {
                self.state = OrchestrationLifecycleState::SelectingAction;
                Ok(self.state)
            }
            OrchestrationLifecycleState::Completed
            | OrchestrationLifecycleState::Failed
            | OrchestrationLifecycleState::NoProgress
            | OrchestrationLifecycleState::Cancelled => Ok(self.state),
        }
    }

    fn verify_worker_result(
        &mut self,
        task_id: &str,
        worker_result: &WorkerResult,
    ) -> Result<(), OrchestrationError> {
        self.metrics.verification_count += 1;
        let task_record = self
            .ledger
            .get_task(task_id)
            .ok_or_else(|| OrchestrationError::TaskNotFound(task_id.to_string()))?
            .clone();

        let report = self
            .verifier
            .verify_task(&self.ledger, &task_record, worker_result)?;

        let observation = VerificationObservation {
            round: self.ledger.round_count,
            task_id: task_id.to_string(),
            outcome: report.outcome,
            observation_summary: report.observation_summary,
            evidence_digest: report.evidence_digest.clone(),
        };
        self.ledger.add_observation(observation, &self.config)?;

        // Progress fingerprint and no-progress loop detection
        let new_fp = compute_progress_fingerprint(
            Some(task_id),
            worker_result.output_digest.as_deref(),
            report.evidence_digest.as_deref(),
            &self.ledger.state_digest(),
        );
        if self.last_fingerprint.as_deref() == Some(&new_fp) {
            self.consecutive_no_progress += 1;
            self.metrics.no_progress_count += 1;
            self.ledger.no_progress_count += 1;
        } else {
            self.consecutive_no_progress = 0;
            self.last_fingerprint = Some(new_fp.clone());
        }
        self.ledger.progress_fingerprint = new_fp;

        if self.consecutive_no_progress >= self.config.max_no_progress_rounds {
            self.state = OrchestrationLifecycleState::NoProgress;
            return Ok(());
        }

        match report.outcome {
            VerificationOutcome::Pass => {
                if let Some(t) = self.ledger.get_task_mut(task_id) {
                    t.status = LedgerTaskStatus::Verified;
                }
                self.state = OrchestrationLifecycleState::SelectingAction;
            }
            VerificationOutcome::Fail => {
                self.metrics.failed_worker_attempts += 1;
                let attempt = self
                    .ledger
                    .get_task(task_id)
                    .map(|t| t.attempt_count)
                    .unwrap_or(0);
                if attempt >= self.config.max_task_attempts {
                    if let Some(t) = self.ledger.get_task_mut(task_id) {
                        t.status = LedgerTaskStatus::Failed;
                        t.failure_reason = Some("verification failed repeatedly".to_string());
                    }
                } else {
                    if let Some(t) = self.ledger.get_task_mut(task_id) {
                        t.status = LedgerTaskStatus::Pending;
                    }
                }
                self.state = OrchestrationLifecycleState::SelectingAction;
            }
            VerificationOutcome::Inconclusive => {
                if let Some(t) = self.ledger.get_task_mut(task_id) {
                    if t.attempt_count >= self.config.max_task_attempts {
                        t.status = LedgerTaskStatus::Blocked;
                        t.failure_reason = Some("verification inconclusive".to_string());
                    } else {
                        t.status = LedgerTaskStatus::Pending;
                    }
                }
                self.state = OrchestrationLifecycleState::SelectingAction;
            }
        }

        // Update ledger size peak metric
        let peak = self.ledger.estimate_bytes();
        if peak > self.metrics.ledger_peak_bytes {
            self.metrics.ledger_peak_bytes = peak;
        }

        Ok(())
    }

    pub fn run_to_completion(&mut self) -> Result<OrchestrationSummary, OrchestrationError> {
        while !self.state.is_terminal() {
            self.step()?;
        }

        let deliverable_digest = match &self.pending_decision {
            Some(NextTaskDecision {
                action: ControllerAction::DeclareComplete { deliverable_digest },
                ..
            }) => Some(deliverable_digest.clone()),
            _ => {
                // If all tasks verified, compute digest from all verified deliverables
                if self.ledger.all_verified() {
                    let mut hasher = Sha256::new();
                    for t in &self.ledger.tasks {
                        if let Some(d) = &t.result_digest {
                            hasher.update(d.as_bytes());
                        }
                    }
                    Some(hex::encode(hasher.finalize()))
                } else {
                    None
                }
            }
        };

        let terminal_reason = match self.state {
            OrchestrationLifecycleState::Completed => {
                "orchestration completed successfully".to_string()
            }
            OrchestrationLifecycleState::Failed => "orchestration failed".to_string(),
            OrchestrationLifecycleState::NoProgress => {
                "no progress detected across rounds".to_string()
            }
            OrchestrationLifecycleState::Cancelled => "orchestration cancelled".to_string(),
            _ => "unknown terminal state".to_string(),
        };

        let final_ledger_hash = self.ledger.state_digest();
        let summary = OrchestrationSummary {
            terminal_state: self.state,
            deliverable_digest,
            terminal_reason,
            rounds_executed: self.ledger.round_count,
            metrics: self.metrics.clone(),
            final_ledger_hash,
        };
        self.terminal_summary = Some(summary.clone());
        Ok(summary)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    struct ScriptedController {
        decisions: Vec<NextTaskDecision>,
        index: usize,
    }

    impl ScriptedController {
        fn new(decisions: Vec<NextTaskDecision>) -> Self {
            Self {
                decisions,
                index: 0,
            }
        }
    }

    impl LedgerController for ScriptedController {
        fn decide_next_action(
            &mut self,
            _ledger: &WorkingLedger,
        ) -> Result<NextTaskDecision, OrchestrationError> {
            if self.index < self.decisions.len() {
                let d = self.decisions[self.index].clone();
                self.index += 1;
                Ok(d)
            } else {
                Ok(NextTaskDecision {
                    action: ControllerAction::DeclareNoProgress {
                        reason: "no more scripted decisions".to_string(),
                    },
                    reason_summary: "exhausted script".to_string(),
                    expected_evidence: "".to_string(),
                })
            }
        }
    }

    struct AdaptiveController;

    impl LedgerController for AdaptiveController {
        fn decide_next_action(
            &mut self,
            ledger: &WorkingLedger,
        ) -> Result<NextTaskDecision, OrchestrationError> {
            if ledger.all_verified() {
                return Ok(NextTaskDecision {
                    action: ControllerAction::DeclareComplete {
                        deliverable_digest: "all_tasks_verified_digest".to_string(),
                    },
                    reason_summary: "all tasks verified".to_string(),
                    expected_evidence: "final_deliverable".to_string(),
                });
            }

            if let Some(pending_id) = ledger.pending_task_ids().into_iter().next() {
                return Ok(NextTaskDecision {
                    action: ControllerAction::ExecuteTask {
                        task_id: pending_id.clone(),
                    },
                    reason_summary: format!("executing pending task {pending_id}"),
                    expected_evidence: format!("evidence_for_{pending_id}"),
                });
            }

            Ok(NextTaskDecision {
                action: ControllerAction::DeclareFailed {
                    reason: "no pending or verifiable tasks remain".to_string(),
                },
                reason_summary: "terminal failure".to_string(),
                expected_evidence: "".to_string(),
            })
        }
    }

    type WorkerHandler =
        Box<dyn Fn(&WorkerContext) -> Result<WorkerResult, OrchestrationError> + Send + Sync>;

    struct MockWorker {
        handler: WorkerHandler,
    }

    impl MockWorker {
        fn new(
            f: impl Fn(&WorkerContext) -> Result<WorkerResult, OrchestrationError>
                + Send
                + Sync
                + 'static,
        ) -> Self {
            Self {
                handler: Box::new(f),
            }
        }
    }

    impl LedgerWorker for MockWorker {
        fn execute_task(
            &mut self,
            context: &WorkerContext,
        ) -> Result<WorkerResult, OrchestrationError> {
            (self.handler)(context)
        }
    }

    type VerifierHandler = Box<
        dyn Fn(
                &WorkingLedger,
                &LedgerTaskRecord,
                &WorkerResult,
            ) -> Result<VerificationReport, OrchestrationError>
            + Send
            + Sync,
    >;

    struct MockVerifier {
        handler: VerifierHandler,
    }

    impl MockVerifier {
        fn new(
            f: impl Fn(
                    &WorkingLedger,
                    &LedgerTaskRecord,
                    &WorkerResult,
                ) -> Result<VerificationReport, OrchestrationError>
                + Send
                + Sync
                + 'static,
        ) -> Self {
            Self {
                handler: Box::new(f),
            }
        }

        fn pass_all() -> Self {
            Self::new(|_, _, _| {
                Ok(VerificationReport {
                    outcome: VerificationOutcome::Pass,
                    observation_summary: "checks passed".to_string(),
                    evidence_digest: Some("pass_digest".to_string()),
                })
            })
        }
    }

    impl LedgerVerifier for MockVerifier {
        fn verify_task(
            &mut self,
            ledger: &WorkingLedger,
            task: &LedgerTaskRecord,
            result: &WorkerResult,
        ) -> Result<VerificationReport, OrchestrationError> {
            (self.handler)(ledger, task, result)
        }
    }

    #[test]
    fn scenario_a_successful_decomposition() {
        let config = LedgerOrchestratorConfig::default();
        let mut ledger = WorkingLedger::new("contract:decomp-v1", "decompose and implement");
        ledger
            .add_task("task-a", "implement component a", &config)
            .unwrap();
        ledger
            .add_task("task-b", "implement component b", &config)
            .unwrap();

        let controller = AdaptiveController;
        let worker = MockWorker::new(|ctx| {
            let task_id = &ctx.selected_task.id;
            Ok(WorkerResult {
                status: WorkerOutcomeStatus::Completed,
                output_digest: Some(format!("output_digest_{task_id}")),
                partial_summary: None,
                artifact_refs: vec![format!("artifact:{task_id}")],
                findings: vec![LedgerFinding {
                    id: format!("finding-{task_id}"),
                    summary: format!("successfully built {task_id}"),
                    source: "worker".to_string(),
                    related_task_id: Some(task_id.clone()),
                    evidence_digest: Some(format!("ev-{task_id}")),
                }],
                failure_reason: None,
            })
        });
        let verifier = MockVerifier::pass_all();

        let mut orchestrator =
            LedgerOrchestrator::new(config, ledger, controller, worker, verifier);
        let summary = orchestrator.run_to_completion().unwrap();

        assert_eq!(
            summary.terminal_state,
            OrchestrationLifecycleState::Completed
        );
        assert_eq!(
            summary.deliverable_digest,
            Some("all_tasks_verified_digest".to_string())
        );
        assert_eq!(summary.metrics.round_count, 2);
        assert_eq!(summary.metrics.worker_call_count, 2);
        assert_eq!(summary.metrics.verification_count, 2);
        assert_eq!(summary.metrics.replan_count, 0);
        assert!(orchestrator.ledger.all_verified());
        assert_eq!(
            orchestrator.ledger.get_task("task-a").unwrap().status,
            LedgerTaskStatus::Verified
        );
        assert_eq!(
            orchestrator.ledger.get_task("task-b").unwrap().status,
            LedgerTaskStatus::Verified
        );
    }

    #[test]
    fn scenario_b_verification_repair() {
        let config = LedgerOrchestratorConfig::default();
        let mut ledger = WorkingLedger::new("contract:repair-v1", "implement with self-repair");
        ledger
            .add_task("task-bugfix", "fix memory leak", &config)
            .unwrap();

        let controller = AdaptiveController;
        let attempts = Arc::new(AtomicU32::new(0));
        let attempts_clone = attempts.clone();

        let worker = MockWorker::new(move |_ctx| {
            let cur = attempts_clone.fetch_add(1, Ordering::SeqCst);
            if cur == 0 {
                Ok(WorkerResult {
                    status: WorkerOutcomeStatus::Completed,
                    output_digest: Some("buggy_output_attempt_1".to_string()),
                    partial_summary: None,
                    artifact_refs: vec!["patch:leak_incomplete".to_string()],
                    findings: vec![],
                    failure_reason: None,
                })
            } else {
                Ok(WorkerResult {
                    status: WorkerOutcomeStatus::Completed,
                    output_digest: Some("fixed_output_attempt_2".to_string()),
                    partial_summary: None,
                    artifact_refs: vec!["patch:leak_repaired".to_string()],
                    findings: vec![LedgerFinding {
                        id: "finding-repaired".to_string(),
                        summary: "fixed memory free in cleanup".to_string(),
                        source: "worker_repair".to_string(),
                        related_task_id: Some("task-bugfix".to_string()),
                        evidence_digest: Some("ev:repaired".to_string()),
                    }],
                    failure_reason: None,
                })
            }
        });

        let verifier = MockVerifier::new(|_ledger, _task, result| {
            if result.output_digest.as_deref() == Some("buggy_output_attempt_1") {
                Ok(VerificationReport {
                    outcome: VerificationOutcome::Fail,
                    observation_summary: "leak still detected on stress test".to_string(),
                    evidence_digest: Some("test_fail_leak_persists".to_string()),
                })
            } else {
                Ok(VerificationReport {
                    outcome: VerificationOutcome::Pass,
                    observation_summary: "leak resolved; valgrind clean".to_string(),
                    evidence_digest: Some("test_pass_leak_clean".to_string()),
                })
            }
        });

        let mut orchestrator =
            LedgerOrchestrator::new(config, ledger, controller, worker, verifier);
        let summary = orchestrator.run_to_completion().unwrap();

        assert_eq!(
            summary.terminal_state,
            OrchestrationLifecycleState::Completed
        );
        assert_eq!(summary.metrics.round_count, 2);
        assert_eq!(summary.metrics.failed_worker_attempts, 1);
        assert_eq!(summary.metrics.verification_count, 2);
        assert_eq!(orchestrator.ledger.observations.len(), 2);
        assert_eq!(
            orchestrator.ledger.observations[0].outcome,
            VerificationOutcome::Fail
        );
        assert_eq!(
            orchestrator.ledger.observations[1].outcome,
            VerificationOutcome::Pass
        );
        assert_eq!(
            orchestrator.ledger.get_task("task-bugfix").unwrap().status,
            LedgerTaskStatus::Verified
        );
    }

    #[test]
    fn scenario_c_no_progress_guard() {
        let config = LedgerOrchestratorConfig {
            max_no_progress_rounds: 2,
            ..Default::default()
        };
        let mut ledger = WorkingLedger::new("contract:loop-v1", "test no progress");
        ledger
            .add_task("task-stuck", "cannot make progress", &config)
            .unwrap();

        let controller = ScriptedController::new(vec![
            NextTaskDecision {
                action: ControllerAction::ExecuteTask {
                    task_id: "task-stuck".to_string(),
                },
                reason_summary: "attempt 1".to_string(),
                expected_evidence: "ev".to_string(),
            },
            NextTaskDecision {
                action: ControllerAction::ExecuteTask {
                    task_id: "task-stuck".to_string(),
                },
                reason_summary: "attempt 2 same".to_string(),
                expected_evidence: "ev".to_string(),
            },
            NextTaskDecision {
                action: ControllerAction::ExecuteTask {
                    task_id: "task-stuck".to_string(),
                },
                reason_summary: "attempt 3 same".to_string(),
                expected_evidence: "ev".to_string(),
            },
        ]);

        let worker = MockWorker::new(|_| {
            Ok(WorkerResult {
                status: WorkerOutcomeStatus::Completed,
                output_digest: Some("identical_stuck_output".to_string()),
                partial_summary: None,
                artifact_refs: vec![],
                findings: vec![],
                failure_reason: None,
            })
        });

        let verifier = MockVerifier::new(|_, _, _| {
            Ok(VerificationReport {
                outcome: VerificationOutcome::Fail,
                observation_summary: "stuck output fails verification identically".to_string(),
                evidence_digest: Some("fail_digest_identical".to_string()),
            })
        });
        let mut orchestrator =
            LedgerOrchestrator::new(config, ledger, controller, worker, verifier);
        let summary = orchestrator.run_to_completion().unwrap();

        assert_eq!(
            summary.terminal_state,
            OrchestrationLifecycleState::NoProgress
        );
        assert!(summary.metrics.no_progress_count >= 2);
    }

    #[test]
    fn scenario_d_worker_truncation_recovery() {
        let config = LedgerOrchestratorConfig::default();
        let mut ledger = WorkingLedger::new("contract:trunc-v1", "test truncation recovery");
        ledger
            .add_task("task-large", "produce large code change", &config)
            .unwrap();

        let controller = AdaptiveController;
        let attempts = Arc::new(AtomicU32::new(0));
        let attempts_clone = attempts.clone();

        let worker = MockWorker::new(move |_| {
            let cur = attempts_clone.fetch_add(1, Ordering::SeqCst);
            if cur == 0 {
                Ok(WorkerResult {
                    status: WorkerOutcomeStatus::Truncated,
                    output_digest: Some("partial_out_sha".to_string()),
                    partial_summary: Some("Discovered root cause in token buffer".to_string()),
                    artifact_refs: vec!["partial_patch.diff".to_string()],
                    findings: vec![LedgerFinding {
                        id: "f-partial".to_string(),
                        summary: "token buffer boundary is 4096 bytes".to_string(),
                        source: "worker_truncation".to_string(),
                        related_task_id: Some("task-large".to_string()),
                        evidence_digest: Some("ev:buffer".to_string()),
                    }],
                    failure_reason: None,
                })
            } else {
                Ok(WorkerResult {
                    status: WorkerOutcomeStatus::Completed,
                    output_digest: Some("full_completed_patch".to_string()),
                    partial_summary: None,
                    artifact_refs: vec!["complete_patch.diff".to_string()],
                    findings: vec![],
                    failure_reason: None,
                })
            }
        });

        let verifier = MockVerifier::pass_all();
        let mut orchestrator =
            LedgerOrchestrator::new(config, ledger, controller, worker, verifier);
        let summary = orchestrator.run_to_completion().unwrap();

        assert_eq!(
            summary.terminal_state,
            OrchestrationLifecycleState::Completed
        );
        assert_eq!(summary.metrics.truncation_count, 1);
        assert_eq!(orchestrator.ledger.findings.len(), 2); // 1 partial summary finding + 1 finding from result
        assert_eq!(
            orchestrator.ledger.get_task("task-large").unwrap().status,
            LedgerTaskStatus::Verified
        );
    }

    #[test]
    fn scenario_e_state_bounds_enforced() {
        let config = LedgerOrchestratorConfig {
            max_tasks: 2,
            max_findings: 2,
            max_observations: 2,
            max_summary_bytes: 50,
            max_ledger_bytes: 4096,
            max_orchestration_rounds: 5,
            max_task_attempts: 2,
            max_no_progress_rounds: 2,
            max_truncations: 2,
        };
        let mut ledger = WorkingLedger::new("contract:bounds", "bounds testing");

        // 1. Task bounds
        ledger.add_task("task-1", "description 1", &config).unwrap();
        ledger.add_task("task-2", "description 2", &config).unwrap();
        let err = ledger
            .add_task("task-3", "description 3", &config)
            .unwrap_err();
        assert!(
            matches!(err, OrchestrationError::OversizedField { ref field, .. } if field == "tasks")
        );

        // 2. Summary bytes bound
        let mut ledger2 = WorkingLedger::new("contract:bounds2", "bounds testing 2");
        let long_desc = "a".repeat(100);
        let err2 = ledger2
            .add_task("task-long", &long_desc, &config)
            .unwrap_err();
        assert!(
            matches!(err2, OrchestrationError::OversizedField { ref field, .. } if field == "task_description")
        );

        // 3. Finding bounds
        ledger2
            .add_finding(
                LedgerFinding {
                    id: "f1".into(),
                    summary: "short 1".into(),
                    source: "s".into(),
                    related_task_id: None,
                    evidence_digest: None,
                },
                &config,
            )
            .unwrap();
        ledger2
            .add_finding(
                LedgerFinding {
                    id: "f2".into(),
                    summary: "short 2".into(),
                    source: "s".into(),
                    related_task_id: None,
                    evidence_digest: None,
                },
                &config,
            )
            .unwrap();
        let err3 = ledger2
            .add_finding(
                LedgerFinding {
                    id: "f3".into(),
                    summary: "short 3".into(),
                    source: "s".into(),
                    related_task_id: None,
                    evidence_digest: None,
                },
                &config,
            )
            .unwrap_err();
        assert!(
            matches!(err3, OrchestrationError::OversizedField { ref field, .. } if field == "findings")
        );
    }

    #[test]
    fn scenario_f_strict_isolation_between_cells() {
        let config = LedgerOrchestratorConfig::default();

        // Cell 1
        let mut ledger_cell_1 = WorkingLedger::new("contract:cell_1", "plan 1");
        ledger_cell_1
            .add_task("task-cell-1", "task 1", &config)
            .unwrap();
        ledger_cell_1
            .add_finding(
                LedgerFinding {
                    id: "cell-1-secret-finding".to_string(),
                    summary: "private finding in cell 1".to_string(),
                    source: "cell_1".to_string(),
                    related_task_id: Some("task-cell-1".to_string()),
                    evidence_digest: Some("ev:cell1".to_string()),
                },
                &config,
            )
            .unwrap();

        // Cell 2
        let mut ledger_cell_2 = WorkingLedger::new("contract:cell_2", "plan 2");
        ledger_cell_2
            .add_task("task-cell-2", "task 2", &config)
            .unwrap();

        // Check Cell 2 cannot see Cell 1's findings, tasks, or digests
        assert!(ledger_cell_2.get_task("task-cell-1").is_none());
        assert!(ledger_cell_2.findings.is_empty());
        assert_ne!(ledger_cell_1.state_digest(), ledger_cell_2.state_digest());
        assert_ne!(
            ledger_cell_1.progress_fingerprint,
            ledger_cell_2.progress_fingerprint
        );
    }

    #[test]
    fn scenario_g_fresh_context_filtering() {
        let config = LedgerOrchestratorConfig::default();
        let mut ledger = WorkingLedger::new("contract:fresh-context", "multi-task plan");
        ledger.add_task("task-1", "task one", &config).unwrap();
        ledger.add_task("task-2", "task two", &config).unwrap();
        ledger.add_task("task-3", "task three", &config).unwrap();

        // Finding 1 bound to task-1
        ledger
            .add_finding(
                LedgerFinding {
                    id: "f-task-1".to_string(),
                    summary: "finding for task 1".to_string(),
                    source: "worker".to_string(),
                    related_task_id: Some("task-1".to_string()),
                    evidence_digest: None,
                },
                &config,
            )
            .unwrap();

        // Finding 2 bound to task-2 (unrelated to task-1)
        ledger
            .add_finding(
                LedgerFinding {
                    id: "f-task-2".to_string(),
                    summary: "private finding for task 2".to_string(),
                    source: "worker".to_string(),
                    related_task_id: Some("task-2".to_string()),
                    evidence_digest: None,
                },
                &config,
            )
            .unwrap();

        // Finding 3 is global (related_task_id is None)
        ledger
            .add_finding(
                LedgerFinding {
                    id: "f-global".to_string(),
                    summary: "global project invariant".to_string(),
                    source: "system".to_string(),
                    related_task_id: None,
                    evidence_digest: None,
                },
                &config,
            )
            .unwrap();

        let ctx = project_worker_context(&ledger, "task-1", "expected").unwrap();

        assert_eq!(ctx.selected_task.id, "task-1");
        // Must contain task-1 finding and global finding
        assert!(ctx.relevant_findings.iter().any(|f| f.id == "f-task-1"));
        assert!(ctx.relevant_findings.iter().any(|f| f.id == "f-global"));
        // Must NOT contain task-2 finding
        assert!(!ctx.relevant_findings.iter().any(|f| f.id == "f-task-2"));
        assert_eq!(ctx.relevant_findings.len(), 2);
    }

    #[test]
    fn security_and_authority_boundaries_isolated() {
        let config = LedgerOrchestratorConfig::default();
        let mut ledger = WorkingLedger::new("contract:security", "testing boundaries");

        // Secrets in task description are sanitized/redacted
        let test_key = format!("{}_{}", "sk-test", "1234567890abcdef");
        let task_desc = format!("API key: DEEPSEEK_API_KEY={test_key}");
        ledger.add_task("task-sec", &task_desc, &config).unwrap();
        let task = ledger.get_task("task-sec").unwrap();
        assert!(!task.description.contains(&test_key));

        // Secrets in findings are sanitized/redacted
        let sensitive_token = format!("{}_{}", "ghp", "1234567890abcdef");
        let finding_summary = format!("Found Bearer {sensitive_token} in header");
        ledger
            .add_finding(
                LedgerFinding {
                    id: "f-sec".to_string(),
                    summary: finding_summary,
                    source: "scanner".to_string(),
                    related_task_id: Some("task-sec".to_string()),
                    evidence_digest: None,
                },
                &config,
            )
            .unwrap();
        assert!(!ledger.findings[0].summary.contains(&sensitive_token));

        // H_ledger is purely an in-memory/ephemeral state machine without authority
        // to invoke ProductStore transactions or schedule external processes.
    }

    #[test]
    fn failure_handling_cases() {
        let config = LedgerOrchestratorConfig::default();
        let mut ledger = WorkingLedger::new("contract:fail", "failure testing");

        // Invalid task id
        let err1 = ledger
            .add_task("bad task id with spaces!", "desc", &config)
            .unwrap_err();
        assert!(matches!(err1, OrchestrationError::InvalidTaskId(_)));

        // Duplicate task id
        ledger.add_task("task-dup", "desc 1", &config).unwrap();
        let err2 = ledger.add_task("task-dup", "desc 2", &config).unwrap_err();
        assert!(matches!(err2, OrchestrationError::DuplicateTaskId(_)));

        // Task not found in context projection
        let err3 = project_worker_context(&ledger, "non-existent-task", "ev").unwrap_err();
        assert!(matches!(err3, OrchestrationError::TaskNotFound(_)));

        // Cancellation handling
        let controller = AdaptiveController;
        let worker = MockWorker::new(|_| {
            Ok(WorkerResult {
                status: WorkerOutcomeStatus::Completed,
                output_digest: Some("out".to_string()),
                partial_summary: None,
                artifact_refs: vec![],
                findings: vec![],
                failure_reason: None,
            })
        });
        let verifier = MockVerifier::pass_all();
        let mut orchestrator =
            LedgerOrchestrator::new(config.clone(), ledger, controller, worker, verifier);
        orchestrator.cancel();
        let summary = orchestrator.run_to_completion().unwrap();
        assert_eq!(
            summary.terminal_state,
            OrchestrationLifecycleState::Cancelled
        );

        // Max rounds exhaustion
        let small_rounds_config = LedgerOrchestratorConfig {
            max_orchestration_rounds: 1,
            ..Default::default()
        };
        let mut ledger_rounds = WorkingLedger::new("contract:rounds", "plan");
        ledger_rounds
            .add_task("task-r", "desc", &small_rounds_config)
            .unwrap();
        let controller2 = AdaptiveController;
        let worker2 = MockWorker::new(|_| {
            Ok(WorkerResult {
                status: WorkerOutcomeStatus::Failed,
                output_digest: None,
                partial_summary: None,
                artifact_refs: vec![],
                findings: vec![],
                failure_reason: Some("retry needed".to_string()),
            })
        });
        let verifier2 = MockVerifier::pass_all();
        let mut orchestrator2 = LedgerOrchestrator::new(
            small_rounds_config,
            ledger_rounds,
            controller2,
            worker2,
            verifier2,
        );
        let res = orchestrator2.run_to_completion();
        assert!(res.is_err());
        assert!(matches!(
            res.unwrap_err(),
            OrchestrationError::MaxRoundsExceeded(1)
        ));
    }

    #[test]
    fn matrix_planning_with_ledger_orchestrated_harness() {
        use crate::harness_evolution::{
            build_mx1_matrix_plan, sample_mx1_descriptor_manifest_with_ledger_harness,
            validate_mx1_descriptor_manifest, Mx1MatrixRung, LEDGER_ORCHESTRATED_HARNESS_ID,
            MX1_ARM_ZERO_HARNESS_ID,
        };

        let manifest = sample_mx1_descriptor_manifest_with_ledger_harness();
        validate_mx1_descriptor_manifest(&manifest).unwrap();

        // Check the ledger-orchestrated harness descriptor
        let ledger_harness = manifest
            .harnesses
            .iter()
            .find(|h| h.descriptor_id == LEDGER_ORCHESTRATED_HARNESS_ID)
            .expect("ledger-orchestrated harness must be present in manifest");
        assert!(ledger_harness.default_off);
        assert_eq!(ledger_harness.source_owner, "rust-engine-harness-evolution");

        // Build 2x2x3 matrix plan: 2 Harnesses * 2 Models * 3 Strategies = 12 cells
        let plan = build_mx1_matrix_plan(
            &manifest,
            Mx1MatrixRung::TwoByTwoByThree,
            "task-pe7-test-matrix",
            1,
            &"a".repeat(64),
        )
        .unwrap();

        assert_eq!(plan.cells.len(), 12);
        let arm_zero_cells = plan
            .cells
            .iter()
            .filter(|c| c.identity.harness_id == MX1_ARM_ZERO_HARNESS_ID)
            .count();
        let ledger_cells = plan
            .cells
            .iter()
            .filter(|c| c.identity.harness_id == LEDGER_ORCHESTRATED_HARNESS_ID)
            .count();

        assert_eq!(arm_zero_cells, 6);
        assert_eq!(ledger_cells, 6);

        for cell in &plan.cells {
            assert_eq!(
                cell.disposition,
                crate::harness_evolution::Mx1MatrixCellDisposition::Admitted
            );
        }
    }

    #[test]
    fn adapter_normalization_ledger_orchestrated() {
        use crate::harness_evolution::{
            build_mx1_matrix_plan, sample_mx1_descriptor_manifest_with_ledger_harness,
            Mx1HarnessRunAdapter, Mx1LedgerOrchestratedHarnessAdapter, Mx1MatrixRung,
            LEDGER_ORCHESTRATED_HARNESS_ID,
        };
        use crate::product_golden_path::ProductTaskStatus;

        let manifest = sample_mx1_descriptor_manifest_with_ledger_harness();
        let descriptor = manifest
            .harnesses
            .iter()
            .find(|h| h.descriptor_id == LEDGER_ORCHESTRATED_HARNESS_ID)
            .unwrap()
            .clone();

        let adapter = Mx1LedgerOrchestratedHarnessAdapter::new(descriptor.clone()).unwrap();
        assert_eq!(
            adapter.descriptor().descriptor_id,
            LEDGER_ORCHESTRATED_HARNESS_ID
        );

        let plan = build_mx1_matrix_plan(
            &manifest,
            Mx1MatrixRung::TwoByTwoByThree,
            "task-pe7-test-norm",
            1,
            &"b".repeat(64),
        )
        .unwrap();

        let cell = plan
            .cells
            .iter()
            .find(|c| c.identity.harness_id == LEDGER_ORCHESTRATED_HARNESS_ID)
            .unwrap();

        let terminal = serde_json::json!({
            "schema_version": "product_task_terminal_evidence.v2",
            "product_task_id": "task-pe7-test-norm",
            "task_status": "completed",
            "workspace_scope_id": "norm-workspace-scope",
            "source_revision": "0123456789abcdef0123456789abcdef01234567",
            "verification": {"trustworthy": true, "status": "evidence_recorded"},
            "usage": {"tokens": 17},
            "cost": {"microunits": 19},
            "cleanup": {"status": "complete"},
            "restart": {"status": "not_observed"},
            "recovery": {"status": "reconciled"},
            "raw_output": "never-projected",
            "content_sha256": null,
        });
        let mut terminal_val = terminal.clone();
        let digest = hex::encode(sha2::Sha256::digest(
            serde_json::to_vec(&terminal_val).unwrap(),
        ));
        terminal_val["content_sha256"] = serde_json::Value::String(digest);

        let evidence = crate::product_golden_path::project_product_harness_run(
            &serde_json::json!({
                "task_id": "task-pe7-test-norm",
                "status": "completed",
                "workspace_id": "norm-workspace-scope",
                "workspace_binding": {
                    "workspace_id": "norm-workspace",
                    "workspace_path": "/private/norm-workspace",
                    "source_revision": "0123456789abcdef0123456789abcdef01234567",
                    "allowed_paths": ["engine/src/harness_evolution.rs"]
                },
                "failure_code": serde_json::Value::Null,
                "failure_detail": serde_json::Value::Null,
            }),
            Some(&terminal_val),
        )
        .unwrap();

        let normalized = adapter.normalize_run(&plan, cell, &evidence).unwrap();
        assert_eq!(normalized.cell_id, cell.cell_id);
        assert_eq!(normalized.harness_id, LEDGER_ORCHESTRATED_HARNESS_ID);
        assert_eq!(normalized.terminal_outcome, ProductTaskStatus::Completed);
    }
}
