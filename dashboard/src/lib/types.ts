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
  cli: LocalCliCapability;
  adaptive_fusion?: AdaptiveFusionOperatorStatus;
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

export interface LocalCliCapability {
  enabled: boolean;
  claude_code: boolean;
  codex: boolean;
}

export interface AdaptiveFusionOperatorStatus {
  schema_version: "adaptive_fusion_operator_status.v1";
  trusted_local_profile: {
    schema_version: "trusted_local_profile.v1";
    requested: boolean;
    ready: boolean;
    blockers: string[];
    capabilities: {
      provider_execution: boolean;
      adaptive_execution: boolean;
      default_routing: boolean;
      experiments: boolean;
      auto_promotion: boolean;
    };
  };
  trusted_local_task_advancement: {
    schema_version: "trusted_local_task_advancement.v1";
    requested: boolean;
    ready: boolean;
    blockers: string[];
    executor_type: string;
    worker_count: number;
    max_concurrent: number;
  };
  completion_api: {
    available: boolean;
    ready_for_live_completion: boolean;
    executor_configured: boolean;
    registry_configured: boolean;
    storage_configured: boolean;
    default_routing_enabled: boolean;
  };
  gates: {
    provider_execution: boolean;
    adaptive_execution: boolean;
    auth: boolean;
    fusion_kill_switch: boolean;
    experiments_enabled: boolean;
    experiments_active: boolean;
    experiments_paused: boolean;
    experiments_kill_switch: boolean;
    auto_promotion_enabled: boolean;
    auto_promotion_active: boolean;
    auto_promotion_kill_switch: boolean;
  };
  policy: {
    active_policy_count: number;
    snapshot_count: number;
    active_snapshot_count: number;
    live_execution_authority: false;
    requires_explicit_adaptive_plan: true;
  };
  authority: {
    provider_execution_active: boolean;
    adaptive_execution_active: boolean;
    default_routing_active: boolean;
    experiments_active: boolean;
    auto_promotion_active: boolean;
    task_advancement_active: boolean;
  };
  bounds: {
    per_dispatch_cost_cap_usd: number | null;
    daily_cost_cap_usd: number | null;
    today_cost_usd: number;
    daily_cost_remaining_usd: number | null;
    experiment_traffic_rate: number;
    experiment_max_cost_usd: number;
    experiment_max_total_tokens: number;
    experiment_max_calls: number;
    experiment_max_elapsed_ms: number;
    experiment_max_concurrency: number;
    experiment_policy_valid: boolean;
    experiment_policy_blockers: string[];
    auto_promotion_rollout_percentage: number;
    auto_promotion_policy_valid: boolean;
    auto_promotion_policy_blockers: string[];
    worker_count: number;
    worker_max_concurrent: number;
  };
  observations: {
    count: number;
    success_count: number;
    failure_count: number;
    total_cost_usd: number;
    latest_at: string | null;
  };
  scheduler: {
    enabled: boolean;
    running: boolean;
    supervised_workers_enabled: boolean;
    paused: boolean;
    kill_requested: boolean;
    worker_count: number;
    max_concurrent: number;
    executor_type: string | null;
    active_runs: number;
    tick_count: number;
    error_count: number;
    last_tick_at: string | null;
  };
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

export interface DispatchMetricBucket {
  dispatch_count: number;
  success_count: number;
  failure_count: number;
  success_rate: number;
  total_reserved_cost: number;
  total_estimated_cost_usd: number;
  estimated_cost_available: boolean;
  estimated_cost_rows: number;
  total_input_tokens: number;
  total_output_tokens: number;
  [key: string]: unknown;
}

export interface DispatchMetrics {
  schema_version: "dispatch_metrics.v1";
  limit: number;
  totals: DispatchMetricBucket;
  by_tier: DispatchMetricBucket[];
  by_task_class: DispatchMetricBucket[];
  by_final_status: DispatchMetricBucket[];
  by_evaluation_status: DispatchMetricBucket[];
}

export interface DispatchMetricsResponse {
  schema_version: "axum_api.v1";
  metrics: DispatchMetrics;
  limit?: number;
}

export interface FeedbackTrace {
  trace_id: string;
  created_at: string;
  task_class: string;
  tier: string;
  status: string;
  dispatch_id?: string | null;
  run_id?: string | null;
  node_id?: string | null;
  executor_type?: string | null;
  success?: boolean | null;
  latency_ms?: number | null;
  cost_usd?: number | null;
  quality_score?: number | null;
  retry_count?: number | null;
  error_domain?: string | null;
  metadata?: Record<string, unknown> | null;
  [key: string]: unknown;
}

export interface FeedbackTraceListResponse {
  schema_version: "axum_api.v1";
  traces: FeedbackTrace[];
  total?: number;
  limit?: number;
  offset?: number;
}

export interface FeedbackCostOfPass {
  task_class: string;
  tier: string;
  pass_count: number;
  total_count: number;
  pass_rate: number;
  average_cost_usd: number;
  median_cost_usd?: number | null;
  p95_cost_usd?: number | null;
  [key: string]: unknown;
}

export interface FeedbackCostOfPassResponse {
  schema_version: "axum_api.v1";
  rows: FeedbackCostOfPass[];
}

export interface FeedbackPattern {
  schema_version: string;
  pattern_id: string;
  pattern_type: string;
  affected_tier: string | null;
  affected_task_class: string | null;
  count: number;
  denominator: number;
  rate: number;
  evidence_trace_ids: string[];
  severity: 'low' | 'medium' | 'high';
  recommendation_hint: string;
}

export interface FeedbackPatternListResponse {
  schema_version: string;
  patterns: FeedbackPattern[];
}

export interface SimulationReportItem {
  scenario_id: string;
  created_at?: string | null;
  task_class?: string | null;
  tier?: string | null;
  status: string;
  baseline_cost_usd?: number | null;
  simulated_cost_usd?: number | null;
  cost_delta_usd?: number | null;
  pass_rate_delta?: number | null;
  recommendation?: string | null;
  [key: string]: unknown;
}

export interface SimulationReportResponse {
  schema_version: "axum_api.v1";
  report: SimulationReportItem[];
  limit?: number;
  summary?: Record<string, unknown>;
}

export interface PolicySimulationResult {
  schema_version: string;
  scenario_id: string;
  candidate_policy_id: string;
  input_trace_count: number;
  actual_success_rate: number;
  simulated_success_rate: number;
  success_rate_delta: number;
  actual_average_cost: number;
  simulated_average_cost: number;
  cost_delta: number;
  actual_average_latency_ms: number;
  simulated_average_latency_ms: number;
  latency_delta: number;
  actual_human_review_rate: number;
  simulated_human_review_rate: number;
  human_review_rate_delta: number;
  assumptions: string[];
  evidence_trace_ids: string[];
  safety: string;
}

export type ProposalStatus =
  | "pending"
  | "approved"
  | "rejected"
  | "active"
  | "inactive"
  | "rolled_back"
  | "superseded"
  | string;

export interface ControlledLoopProposal {
  proposal_id: string;
  created_at: string;
  updated_at?: string | null;
  title?: string | null;
  summary?: string | null;
  status: ProposalStatus;
  task_class?: string | null;
  tier?: string | null;
  target_tier?: string | null;
  policy_key?: string | null;
  proposed_by?: string | null;
  requires_human_approval?: boolean;
  evidence?: Record<string, unknown> | null;
  payload?: Record<string, unknown> | null;
  approval?: Record<string, unknown> | null;
  rollback?: Record<string, unknown> | null;
  [key: string]: unknown;
}

export interface ProposalListResponse {
  schema_version: "axum_api.v1";
  proposals: ControlledLoopProposal[];
  total?: number;
  limit?: number;
  offset?: number;
}

export interface ProposalResponse {
  schema_version: "axum_api.v1";
  proposal: ControlledLoopProposal;
}

export type AdaptiveFusionObjective = "efficient" | "quality" | string;

export interface AdaptiveCompletionRequest {
  prompt: string;
  task_class?: string;
  objective?: "efficient" | "quality";
  risk_level?: "low" | "medium" | "high" | "critical";
  metadata?: Record<string, unknown>;
  include_routing_metadata?: boolean;
}

export interface AdaptiveCompletionResponse {
  schema_version: "adaptive_completion.v1";
  output: string | null;
  usage: {
    input_tokens: number;
    output_tokens: number;
    estimated_cost_usd: number;
    latency_ms: number;
  };
  routing_metadata?: {
    candidate_id: string;
    candidate_hash: string;
    candidate_kind: "single" | "ordered_fallback" | "fusion";
    policy_hash: string | null;
    policy_rollout_percentage: number | null;
    observation_id: string | null;
    experiment_assigned: boolean;
  };
}

export type ProviderEndpointType = "stub" | "openai_compatible" | "anthropic";

export interface ProviderEndpointConfig {
  endpoint_id: string;
  provider_type: ProviderEndpointType;
  base_url?: string | null;
  model: string;
  credential_env?: string | null;
  timeout_ms?: number;
  input_cost_per_1k_usd?: number | null;
  output_cost_per_1k_usd?: number | null;
}

export interface ProviderEndpointConfigResponse {
  schema_version: "axum_api.v1";
  source: "none" | "environment" | "local_config" | string;
  endpoints: ProviderEndpointConfig[];
  runtime: {
    executor_configured: boolean;
    registry_configured: boolean;
    workflow_executor_configured?: boolean;
    workflow_registry_configured?: boolean;
    completion_executor_configured?: boolean;
    completion_registry_configured?: boolean;
    local_config_apply_requires_restart: boolean;
    local_config_applies_to_completion_api?: boolean;
    local_config_error_code?: string | null;
  };
  safety: {
    raw_secrets_allowed: false;
    credential_storage: "env_reference_only" | string;
    supported_provider_types: ProviderEndpointType[];
  };
}

export interface ProviderEndpointConfigRequest {
  endpoints: ProviderEndpointConfig[];
  confirm_provider_endpoint_config: true;
}

export interface AdaptivePolicyPromotion {
  task_class: string;
  objective: AdaptiveFusionObjective;
  candidate_id: string;
  baseline_candidate_id: string;
  sample_count: number;
  confidence: number;
  mean_quality_delta: number;
  mean_cost_reduction: number;
  failure_rate_delta: number;
  evidence_run_ids: string[];
  risk_level: string;
  confirm_adaptive_policy_promotion: boolean;
}

export interface PromotedAdaptivePolicy {
  schema_version: string;
  policy_key: string;
  task_class: string;
  objective: AdaptiveFusionObjective;
  candidate_id: string;
  baseline_candidate_id: string;
  sample_count: number;
  confidence: number;
  mean_quality_delta: number;
  mean_cost_reduction: number;
  failure_rate_delta: number;
  evidence_run_ids: string[];
  policy_hash: string;
  shadow_first: boolean;
  live_execution_authority: boolean;
  requires_explicit_adaptive_plan: boolean;
}

export interface AdaptivePolicySnapshot {
  schema_version: string;
  adjustment_id: string;
  snapshot_id: string;
  created_at: string;
  updated_at: string;
  status: string;
  actor: string;
  policy_key: string;
  candidate_id: string;
  active_policy_before: PromotedAdaptivePolicy | null;
  promoted_policy: PromotedAdaptivePolicy;
  evidence_run_ids: string[];
  safety_hash: string;
}

export interface AdaptiveFusionPoliciesResponse {
  schema_version: "axum_api.v1";
  policies: PromotedAdaptivePolicy[];
  snapshots: AdaptivePolicySnapshot[];
  live_execution_authority: false;
  requires_explicit_adaptive_plan: true;
}

export interface AdaptivePolicyPromotionVerdict {
  schema_version: string;
  eligible: boolean;
  blocked_reasons: string[];
  policy: PromotedAdaptivePolicy | null;
}

export interface AdaptivePolicyPromotionRequest {
  actor?: string;
  promotion: AdaptivePolicyPromotion;
}

export interface AdaptivePolicyPromotionResponse {
  schema_version: "axum_api.v1";
  decision: AdaptivePolicyPromotionVerdict;
  result: Record<string, unknown>;
}

export interface AdaptivePolicyRollbackRequest {
  actor?: string;
  reason?: string;
  confirm_adaptive_policy_rollback?: boolean;
}

export interface AdaptivePolicyRollbackResponse {
  schema_version: string;
  [key: string]: unknown;
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

export interface WorkspaceVerificationAttempt {
  executor_type: string;
  status: string;
  output?: string | null;
  error_domain?: string | null;
  error_message?: string | null;
  latency_ms?: number | null;
  attempt: number;
}

export interface WorkspaceVerification {
  schema_version: "workspace_verification.v1";
  status: "evidence_recorded" | "verification_failed";
  command: string[];
  result_status: string;
  executor_type: string;
  output?: string | null;
  error_domain?: string | null;
  error_message?: string | null;
  latency_ms?: number | null;
  timeout_ms: number;
  attempt: number;
  verification_attempts: WorkspaceVerificationAttempt[];
  repair_attempts: WorkspaceVerificationAttempt[];
  recorded_at: string;
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
  workspace_mode?: "copy" | "git_worktree";
  git?: Record<string, unknown> | null;
  target_output_authority?: "disabled" | "approval_bound";
  verification_execution_authority?: "allowlisted_commands";
  verification?: WorkspaceVerification | null;
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
  secret_scan_status?: "pending" | "passed" | "blocked";
  review_diff?: string;
  evidence_bundle?: Record<string, unknown>;
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

export interface SupervisedPatchVerificationRequest {
  command: string;
  confirm_verification: true;
  timeout_ms?: number;
  attempt?: number;
  repair_executor?: "codex_cli" | "claude_code_cli";
  max_repair_attempts?: number;
}

export interface SupervisedPatchVerificationResponse {
  schema_version: "axum_api.v1";
  verification: WorkspaceVerification;
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

export interface TargetRepoOutputRequest {
  run_id: string;
  mode: "export_patch" | "push_branch";
  confirm_target_output: true;
  branch_name?: string;
  remote?: string;
  commit_message?: string;
  pr_title?: string;
  create_pull_request?: boolean;
}

export interface TargetRepoOutputResponse {
  schema_version: "axum_api.v1";
  output: {
    schema_version: "target_repo_output.v1";
    source_revision: string;
    patch_hash: string;
    patch?: string;
    branch_name?: string;
    remote?: string;
    commit_sha?: string;
    pr_title?: string;
    pr_body?: string;
    pull_request?: {
      number: number;
      url: string;
      state: string;
      reused: boolean;
    };
  };
  approval_binding: Record<string, unknown>;
  integrity: Record<string, unknown>;
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

export interface WorkflowPlanCreateResponse {
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
  max_queued?: number;
  lease_timeout_ms: number;
  executor_type: string;
  heartbeat_interval_sec?: number;
}

export interface SchedulerWorkerStatus {
  worker_id: string;
  state: string;
  last_heartbeat_at: string;
  tick_count: number;
  error_count: number;
}

export interface SchedulerStatus {
  schema_version: string;
  running: boolean;
  enabled?: boolean;
  message?: string;
  started_at?: string | null;
  supervised_workers_enabled?: boolean;
  worker_count?: number;
  paused?: boolean;
  kill_requested?: boolean;
  workers?: SchedulerWorkerStatus[];
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

export interface AutoAdjustmentsReport {
  schema_version: string;
  mode: string;
  env_gate: boolean;
  dry_run: boolean;
  no_live_mutation: boolean;
  active_apply_available: boolean;
  rollback_endpoint_available: boolean;
  guard: Record<string, unknown>;
  decisions: unknown[];
  snapshot_previews: unknown[];
  active_auto_adjustments: unknown[];
  blocked_reasons: string[];
}

export interface AutoAdjustmentApplyResult {
  schema_version: string;
  adjustment_id: string;
  snapshot_id: string;
  proposal_id: string;
  candidate_id: string;
  policy_key: string;
  target_tier: string;
  status: string;
  applied: boolean;
  blocked_reasons: string[];
  rollback_endpoint?: string;
}

export interface RegulatorStateResponse {
  schema_version: string;
  regulator: {
    mode: "disabled" | "dry_run" | "active";
    env_gate_enabled: boolean;
    dry_run_enabled: boolean;
    active_gate_enabled: boolean;
    pg_database_url_configured: boolean;
  };
  active_routing_policy: Record<string, unknown> | null;
  proposals: {
    pending_count: number;
    active_count: number;
  };
  auto_adjustments: {
    active_count: number;
    report: Record<string, unknown>;
  };
  warnings: string[];
}
