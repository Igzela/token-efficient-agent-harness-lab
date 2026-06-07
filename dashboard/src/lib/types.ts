import type {
  CheckStatus,
  ConfidenceLabel,
  DecisionStatus,
  EvaluationStatus,
  EvidencePolarity,
  EvidenceSource,
  ExecutionStatus,
  ExecutorType,
  ExpectedQualityBand,
  FinalStatus,
  GateSeverity,
  ModelTier,
  QualityRequirement,
  RequestSource,
  RiskFlag,
  RiskLevel,
  TaskDomain,
  TaskIntent,
} from "../../../sdk/typescript/src/generated-wire-types";

export type { ExecutorType } from "../../../sdk/typescript/src/generated-wire-types";

export type { RequestSource } from "../../../sdk/typescript/src/generated-wire-types";

export interface DispatchRequest {
  raw_request: string;
  request_source: RequestSource;
}

export interface ApiStatus {
  schema_version: "axum_api.v1";
  status: string;
  tenant_id?: string;
}

export interface LocalDashboardState {
  schema_version: "local_dashboard.v1";
  status: string;
  counts: LocalCounts;
  dispatches: LocalDispatchHistory[];
  team: LocalTeamState;
  config: Record<string, string | number | boolean | null>;
  costs: LocalCostSummary;
  boundaries: LocalBoundaries;
}

export interface LocalCounts {
  dispatches: number;
  team_members: number;
  api_keys: number;
  audit_events: number;
}

export interface LocalDispatchHistory {
  history_id: number;
  dispatch_id: string;
  created_at: string;
  raw_request: string;
  request_source: RequestSource;
  final_status: FinalStatus;
  selected_tier: ModelTier;
  risk_level: RiskLevel;
  reserved_cost: number;
  bundle: DispatchBundle;
}

export interface DispatchListResponse {
  schema_version: "axum_api.v1";
  dispatches: LocalDispatchHistory[];
}

export interface AuditListResponse {
  schema_version: "axum_api.v1";
  redacted?: boolean;
  events: LocalAuditEvent[];
}

export interface LocalAuditEvent {
  audit_id: number | string;
  created_at: string;
  actor: string;
  action: string;
  resource: string;
  details: Record<string, unknown> | null;
}

export interface LocalTeamState {
  schema_version: "local_team.v1";
  members: LocalTeamMember[];
  api_keys: LocalApiKeyMetadata[];
}

export interface LocalTeamMember {
  user_id: string;
  display_name: string;
  role: "admin" | "readonly" | string;
  created_at: string;
  updated_at: string;
}

export interface LocalApiKeyMetadata {
  key_id: string;
  user_id: string;
  role: "admin" | "readonly" | string;
  scopes: string[];
  created_at: string;
  created_by: string;
  revoked_at: string | null;
  last_used_at: string | null;
  expires_at: string | null;
}

export interface LocalCostSummary {
  schema_version: "local_cost_summary.v2";
  currency: "USD" | string;
  dispatch_count: number;
  total_reserved_cost: number;
  total_estimated_cost_usd: number;
  total_input_tokens: number;
  total_output_tokens: number;
  estimated_cost_available: boolean;
  pricing_configured?: boolean;
  cost_utilization: number;
  by_tier: LocalTierCost[];
  daily: LocalDailyCost[];
}

export interface LocalTierCost {
  selected_tier: ModelTier;
  dispatch_count: number;
  reserved_cost: number;
  estimated_cost_usd: number;
  input_tokens: number;
  output_tokens: number;
}

export interface LocalDailyCost {
  date: string;
  dispatch_count: number;
  reserved_cost: number;
  estimated_cost_usd: number;
}

export interface LocalDispatchCostDetail {
  schema_version: "local_dispatch_cost_detail.v1";
  dispatches: LocalDispatchCostRow[];
}

export interface LocalDispatchCostRow {
  history_id: number;
  dispatch_id: string;
  created_at: string;
  selected_tier: ModelTier;
  reserved_cost: number;
  input_tokens: number;
  output_tokens: number;
  estimated_cost_usd: number;
  executor_type: ExecutorType;
  latency_ms: number | null;
}

export interface LocalBoundaries {
  provider_transport: string;
  target_repository_writes: string;
  sandbox_process_execution: string;
  runtime_workers: string;
  deployment: string;
  docker_required: boolean;
}

export interface OperationsMetrics {
  schema_version: "axum_api.v1";
  executor_type: string;
  auth_required: boolean;
  provider_enabled: boolean;
  local_store: boolean;
  dispatch_count: number;
  audit_event_count: number;
  api_key_count: number;
  backup_count: number;
  latest_backup_created_at: string | null;
  total_reserved_cost: number;
  total_estimated_cost_usd: number;
  total_input_tokens: number;
  total_output_tokens: number;
  estimated_cost_available: boolean;
  pricing_configured: boolean;
  boundaries: LocalBoundaries;
  plan_count?: number;
  workflow_run_count?: number;
  artifact_count?: number;
  approval_count?: number;
  executor_latency_avg_ms?: number;
  scheduler_active_runs?: number;
}

export interface BackupVerification {
  backup_id: string;
  success: boolean;
  checksum_ok: boolean;
  integrity_ok: boolean;
  records_checked: number;
  size_bytes: number;
  backup_path: string;
  target_path: string | null;
  restore_would_overwrite: boolean;
  dry_run: boolean;
  errors: string[];
}

export interface SupervisedPatchWorkspace {
  schema_version: "supervised_patch_workspace.v1";
  workspace_sequence: number;
  workspace_id: string;
  plan_id: string | null;
  run_id: string;
  target_id: string;
  target_repo_path: string;
  target_repo_canonical_path: string;
  workspace_path: string;
  workspace_canonical_path: string;
  source_revision: string;
  source_tree_hash: string | null;
  status: string;
  created_at: string;
  updated_at: string;
  boundary: Record<string, unknown>;
  metadata_only: true;
  execution_authority: "disabled";
}

export interface SupervisedPatchArtifact {
  schema_version: "supervised_patch_artifact.v1";
  artifact_sequence: number;
  artifact_id: string;
  workspace_id: string;
  run_id: string;
  plan_id: string | null;
  target_id: string;
  source_revision: string;
  artifact_type: "patch_diff";
  patch_hash: string;
  changed_files: string[];
  redaction_status: "pending" | "redacted" | "failed";
  storage_refs?: Record<string, unknown>;
  retention_expires_at?: string | null;
  created_at: string;
  metadata_only: true;
  execution_authority: "disabled";
  patch_apply_authority: "disabled";
  artifact_file_created?: false;
}

export interface SupervisedPatchWorkspaceListResponse {
  schema_version: "axum_api.v1";
  metadata_only: true;
  execution_authority: "disabled";
  workspaces: SupervisedPatchWorkspace[];
}

export interface SupervisedPatchWorkspaceResponse {
  schema_version: "axum_api.v1";
  metadata_only: true;
  execution_authority: "disabled";
  workspace: SupervisedPatchWorkspace;
}

export interface SupervisedPatchArtifactListResponse {
  schema_version: "axum_api.v1";
  metadata_only: true;
  execution_authority: "disabled";
  artifacts: SupervisedPatchArtifact[];
}

export interface SupervisedPatchArtifactResponse {
  schema_version: "axum_api.v1";
  metadata_only: true;
  execution_authority: "disabled";
  artifact: SupervisedPatchArtifact;
}

export interface SupervisedPatchWorkspaceCreateResponse {
  schema_version: "axum_api.v1";
  workspace: SupervisedPatchWorkspace;
}

export interface SupervisedPatchArtifactCaptureResponse {
  schema_version: "axum_api.v1";
  artifact: SupervisedPatchArtifact;
}

export interface SupervisedPatchExportResponse {
  schema_version: "axum_api.v1";
  export: {
    artifact_id: string;
    artifact: SupervisedPatchArtifact;
    approval_binding: Record<string, unknown>;
    integrity: Record<string, unknown>;
    exported_by: string;
    exported_at: string;
  };
}

// Workflow run types

export interface WorkflowRunNode {
  node_id: string;
  task_type: string;
  status: string;
  input_refs: string[];
  output_ref: string | null;
  cost_incurred: number;
  error_domain: string | null;
  error_message: string | null;
  executor_type: string | null;
  latency_ms: number | null;
  attempt: number;
  lease_expires_at: string | null;
  created_at: string;
  updated_at: string;
}

export interface WorkflowRunEdge {
  edge_id: string;
  from_node_id: string;
  to_node_id: string;
  edge_type: string;
  created_at: string;
}

export interface WorkflowRun {
  run_id: string;
  plan_id: string | null;
  workflow_id: string;
  status: string;
  initiated_by: string;
  nodes: WorkflowRunNode[];
  edges: WorkflowRunEdge[];
  boundaries: Record<string, unknown>;
  created_at: string;
  updated_at: string;
}

export interface WorkflowRunEvent {
  event_id: string;
  run_id: string;
  node_id: string | null;
  event_type: string;
  details: Record<string, unknown> | null;
  actor: string;
  created_at: string;
}

export interface WorkflowRunApproval {
  approval_id: string;
  run_id: string;
  node_id: string;
  decision: string;
  decided_by: string;
  reason: string | null;
  bound_patch_hash: string | null;
  bound_source_revision: string | null;
  bound_changed_files: string[] | null;
  expires_at: string | null;
  created_at: string;
}

export interface WorkflowRunListResponse {
  schema_version: "axum_api.v1";
  runs: WorkflowRun[];
}

export interface WorkflowRunDetailResponse {
  schema_version: "axum_api.v1";
  run: WorkflowRun;
}

export interface WorkflowRunEventListResponse {
  schema_version: "axum_api.v1";
  events: WorkflowRunEvent[];
}

export interface WorkflowRunApprovalListResponse {
  schema_version: "axum_api.v1";
  approvals: WorkflowRunApproval[];
}

export interface WorkflowRunApprovalResponse {
  schema_version: "axum_api.v1";
  approval: WorkflowRunApproval;
}

export interface WorkflowRunTickResponse {
  schema_version: "axum_api.v1";
  tick: Record<string, unknown>;
}

export interface WorkflowRunActionResponse {
  schema_version: "axum_api.v1";
  run: WorkflowRun;
}

// Plan types

export interface WorkflowPlan {
  plan_id: string;
  workflow_id: string;
  dispatch_id: string | null;
  raw_request: string;
  request_source: string;
  status: string;
  initiated_by: string;
  graph: Record<string, unknown>;
  advisory: Record<string, unknown> | null;
  boundaries: Record<string, unknown>;
  created_at: string;
  updated_at: string;
}

export interface WorkflowPlanListResponse {
  schema_version: "axum_api.v1";
  plans: WorkflowPlan[];
}

export interface WorkflowPlanDetailResponse {
  schema_version: "axum_api.v1";
  plan: WorkflowPlan;
}

// Executor pool types

export interface ExecutorPoolCapabilities {
  max_concurrent: number;
  supported_executor_types: string[];
  supports_cooldown: boolean;
  supports_failure_tracking: boolean;
}

export interface ExecutorPoolEntry {
  executor_type: string;
  status: "available" | "unavailable" | "cooldown";
  active_count: number;
  capacity: number;
  failure_score: number;
  success_rate: number;
  avg_latency_ms: number;
  cost_per_execution: number;
  daily_cost: number;
  cooldown_until: string | null;
  last_failure_at: string | null;
  total_executions: number;
}

export interface ExecutorPoolStatus {
  schema_version: string;
  capabilities: ExecutorPoolCapabilities;
  entries: ExecutorPoolEntry[];
  total_active: number;
  total_capacity: number;
  updated_at: string;
}

// Scheduler types

export interface SchedulerConfig {
  interval_ms: number;
  max_concurrent: number;
  lease_timeout_ms: number;
  executor_type: string;
}

export interface SchedulerStatus {
  schema_version: string;
  running: boolean;
  enabled?: boolean;
  message?: string;
  started_at?: string | null;
  config?: SchedulerConfig;
  tick_count?: number;
  error_count?: number;
  retry_count?: number;
  total_execution_time_ns?: number;
  last_tick_at?: string | null;
  last_error?: string | null;
  active_runs?: number;
}

export interface SchedulerStatusResponse {
  schema_version: "axum_api.v1";
  tenant_id?: string;
  request_id?: string;
  scheduler: SchedulerStatus;
}

export interface ExecutorPoolStatusResponse {
  schema_version: "axum_api.v1";
  pool: ExecutorPoolStatus;
}

// Queue types

export interface QueueConfig {
  max_concurrent: number;
  max_queued: number;
  backpressure_enabled: boolean;
  backpressure_activation: number;
}

export interface TenantQueueInfo {
  tenant_id: string;
  run_count: number;
  avg_priority: number;
}

export interface QueueRunSummary {
  run_id: string;
  run_sequence: number;
  workflow_id: string;
  status: string;
  priority: number;
  deadline_at: string | null;
  sla_ms: number | null;
  tenant_id: string;
  queue_position: number | null;
  pause_reason: string | null;
  degrade_mode: string | null;
  created_at: string;
  started_at: string | null;
}

export interface QueueStatus {
  schema_version: string;
  total_queued: number;
  total_running: number;
  total_paused: number;
  total_completed: number;
  total_failed: number;
  avg_priority: number;
  overdue_count: number;
  capacity_utilization: number;
  queue_depth_ratio: number;
  backpressure_active: boolean;
  effective_concurrency: number;
  queue_config: QueueConfig;
  tenant_counts: TenantQueueInfo[];
}

export interface QueueStatusResponse {
  schema_version: "axum_api.v1";
  queue: QueueStatus;
}

export interface QueueRunListResponse {
  schema_version: "axum_api.v1";
  runs: QueueRunSummary[];
}

export interface QueueRunResponse {
  schema_version: "axum_api.v1";
  run: QueueRunSummary;
}

export interface DecisionRecord {
  decision_id: string;
  run_id: string | null;
  node_id: string | null;
  action: string;
  reason: string;
  action_reason?: string;
  executor: string | null;
  selected_executor?: string;
  blocked_reason: string | null;
  confidence: number;
  confidence_score: number;
  confidence_label: string;
  input_signals: Record<string, unknown>;
  created_at: string;
  quality_signal?: Record<string, unknown> | null;
  routing_signal?: Record<string, unknown> | null;
  cost_signal?: Record<string, unknown> | null;
  approval_signal?: Record<string, unknown> | null;
  queue_signal?: Record<string, unknown> | null;
  executor_pool_signal?: Record<string, unknown> | null;
  candidate_executors?: string[] | null;
  degraded_reason?: string | null;
}

export interface DecisionLogStats {
  total_decisions: number;
  by_action: Record<string, number>;
  avg_confidence: number;
}

export interface DecisionListResponse {
  schema_version: "axum_api.v1";
  tenant_id?: string;
  request_id?: string;
  decisions: DecisionRecord[];
  total: number;
  limit: number;
  offset: number;
}

export interface DecisionDetailResponse {
  schema_version: "axum_api.v1";
  tenant_id?: string;
  request_id?: string;
  decision: DecisionRecord;
}

export interface DecisionStatsResponse {
  schema_version: "axum_api.v1";
  tenant_id?: string;
  request_id?: string;
  stats: DecisionLogStats;
}

export interface QueueTenantListResponse {
  schema_version: "axum_api.v1";
  tenants: TenantQueueInfo[];
}

export interface DispatchBundle {
  record: DispatchRecord;
  analysis: TaskAnalysis;
  decision: DispatchDecision;
  execution_result: ExecutionResult;
  evaluation_result: EvaluationResult;
}

export interface DispatchRecord {
  schema_version: "dispatch_record.v1";
  dispatch_id: string;
  request_snapshot: string;
  task_analysis_id: string;
  decision_id: string;
  execution_result_id: string | null;
  evaluation_result_id: string | null;
  usage_ledger_row_id: string | null;
  budget_reservation_id: string | null;
  final_status: FinalStatus;
  created_at: string;
  updated_at: string;
}

export interface TaskAnalysis {
  schema_version: "task_analysis.v1";
  analysis_id: string;
  raw_request_snapshot: string;
  request_source: RequestSource;
  primary_task_type: string;
  task_domain: TaskDomain;
  task_intent: TaskIntent;
  risk_flags: RiskFlag[];
  complexity_score: number;
  cognitive_complexity: number;
  context_complexity: number;
  execution_risk: number;
  ambiguity_score: number;
  required_capabilities: string[];
  context_budget_estimate: number;
  execution_budget_estimate: number;
  quality_requirement: QualityRequirement;
  risk_level: RiskLevel;
  confidence: number;
  confidence_label: ConfidenceLabel;
  uncertainty_reason: string[];
  safe_default: string;
  escalation_trigger: string | null;
  positive_evidence: Evidence[];
  negative_evidence: Evidence[];
  features_detected: Record<string, unknown>;
  analysis_method: "rule_only";
  created_at: string;
}

export interface Evidence {
  feature: string;
  text: string;
  span: [number, number];
  polarity: EvidencePolarity;
  source: EvidenceSource;
  rule_id: string | null;
  confidence: number;
  negation_scope: string | null;
}

export interface DispatchDecision {
  schema_version: "dispatch_decision.v1";
  decision_id: string;
  analysis_id: string;
  analysis_snapshot: Record<string, unknown>;
  selected_tier: ModelTier;
  selected_profile_id: string | null;
  fallback_tier: ModelTier;
  fallback_profile_id: string | null;
  shadow_routes: ShadowRoute[];
  hard_constraints: string[];
  rejected_candidates: RejectedCandidate[];
  no_shadow_route_reason: string | null;
  max_input_tokens: number;
  max_output_tokens: number;
  routing_reason: string;
  quality_requirement: QualityRequirement;
  expected_quality_band: ExpectedQualityBand;
  confidence: number;
  confidence_label: ConfidenceLabel;
  budget_reservation: BudgetReservation;
  execution_policy: Record<string, unknown>;
  execution_gates: ExecutionGate[];
  routing_mode: string;
  routing_experiment_id: string | null;
  decision_status: DecisionStatus;
  created_at: string;
}

export interface BudgetReservation {
  schema_version: "budget_reservation.v1";
  reservation_id: string;
  decision_id: string;
  currency: string;
  pricing_snapshot_id: string | null;
  pre_budget: number;
  reserved_input_tokens: number;
  reserved_output_tokens: number;
  reserved_total_tokens: number;
  reserved_cost: number;
  budget_policy_id: string | null;
  budget_gate: string | null;
  status: string;
  actual_usage_ref: string | null;
  budget_delta: number | null;
  budget_violation: boolean;
  created_at: string;
  updated_at: string;
  expires_at: string | null;
}

export interface ExecutionGate {
  gate_id: string;
  gate_type: string;
  severity: GateSeverity;
  reason: string;
  evidence_refs: string[];
  clearance_required: string;
  cleared: boolean;
  cleared_by: string | null;
  cleared_at: string | null;
}

export interface ShadowRoute {
  tier: ModelTier;
  profile_id: string | null;
  reason: string;
  admission_scope: string;
  estimated_cost: number | null;
  expected_tradeoff: string;
}

export interface RejectedCandidate {
  tier: ModelTier;
  profile_id: string | null;
  reason: string;
  constraint_failed: string | null;
  estimated_cost: number | null;
}

export interface ExecutionResult {
  schema_version: "execution_result.v1";
  result_id: string;
  dispatch_id: string;
  decision_id: string;
  executor_type: ExecutorType;
  status: ExecutionStatus;
  output: string | null;
  prompt_pack: Record<string, unknown> | null;
  input_tokens: number | null;
  output_tokens: number | null;
  estimated_cost: number | null;
  latency_ms: number | null;
  error_domain: string | null;
  error_message: string | null;
  provider_request_id: string | null;
  attempt_number: number | null;
  finish_reason: string | null;
  usage_source: string | null;
  created_at: string;
}

export interface EvaluationResult {
  schema_version: "evaluation_result.v1";
  evaluation_id: string;
  dispatch_id: string;
  decision_id: string;
  execution_result_id: string;
  status: EvaluationStatus;
  checks: EvaluationCheck[];
  quality_score: number | null;
  requires_retry: boolean;
  retry_reason: string | null;
  created_at: string;
}

export interface EvaluationCheck {
  check_id: string;
  name: string;
  status: CheckStatus;
  reason: string;
}
