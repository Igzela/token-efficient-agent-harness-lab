// Hand-maintained local API/dashboard response types. Wire types stay generated.

import type {
  DispatchBundle,
  ExecutorType,
  FinalStatus,
  ModelTier,
  RequestSource,
  RiskLevel,
} from "./generated-wire-types.js";

export interface ApiStatus {
  schema_version: "axum_api.v1";
  status: string;
  tenant_id?: string;
}

export interface AdaptiveCompletionRequest {
  prompt: string;
  task_class?: string;
  objective?: "efficient" | "quality";
  risk_level?: "low" | "medium" | "high" | "critical";
  metadata?: Record<string, unknown>;
  include_routing_metadata?: boolean;
}

export interface AdaptiveCompletionUsage {
  input_tokens: number;
  output_tokens: number;
  estimated_cost_usd: number;
  latency_ms: number;
}

export interface AdaptiveCompletionRoutingMetadata {
  candidate_id: string;
  candidate_hash: string;
  candidate_kind: "single" | "ordered_fallback" | "fusion";
  policy_hash: string | null;
  policy_rollout_percentage: number | null;
  observation_id: string | null;
  experiment_assigned: boolean;
}

export interface AdaptiveCompletionResponse {
  schema_version: "adaptive_completion.v1";
  output: string | null;
  usage: AdaptiveCompletionUsage;
  routing_metadata?: AdaptiveCompletionRoutingMetadata;
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

export interface LocalCostSummary {
  schema_version: "local_cost_summary.v2";
  currency: string;
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

export interface ApiKeyMeta {
  key_id: string;
  user_id: string;
  role: string;
  scopes: string[];
  created_at: string;
  created_by: string;
  revoked_at: string | null;
  last_used_at: string | null;
  expires_at: string | null;
}

export interface TeamMember {
  user_id: string;
  display_name: string;
  role: string;
  created_at: string;
  updated_at: string;
}

export interface LocalTeamSnapshot {
  schema_version: "local_team.v1";
  members: TeamMember[];
  api_keys: ApiKeyMeta[];
}

export interface DispatchItem {
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
  input_tokens: number | null;
  output_tokens: number | null;
  estimated_cost_usd: number | null;
  executor_type: ExecutorType;
  latency_ms: number | null;
}

export interface ReadOnlyPlan {
  schema_version: "read_only_plan.v1";
  plan_sequence: number;
  plan_id: string;
  created_at: string;
  updated_at: string;
  raw_request: string;
  request_source: RequestSource;
  status: "planned_read_only" | "blocked_invalid_graph";
  workflow_id: string;
  dispatch_id: string;
  analysis: Record<string, unknown>;
  graph: Record<string, unknown>;
  validation?: Record<string, unknown>;
  execution_order?: string[][];
  advisory?: PlanAdvisory;
  boundaries: Record<string, unknown>;
}

export interface PlanAdvisory {
  schema_version: "plan_advisory.v1";
  mode: "recommendation_only";
  status: "recommendation_ready" | "blocked_for_human_review";
  blockers: Record<string, unknown>[];
  recommendations: Record<string, unknown>[];
  quality: Record<string, unknown>;
  routing: Record<string, unknown>;
  retry: Record<string, unknown>;
  observability: Record<string, unknown>;
  decision: Record<string, unknown>;
}

export interface AgentStepPlanRequest {
  agent_id: string;
  role: string;
  capability_profile: string[];
  profile_id: string;
  model?: string;
}

export interface PlanCreateRequest {
  raw_request: string;
  request_source?: RequestSource;
  agent_steps?: AgentStepPlanRequest[];
  confirm_agent_runtime_plan?: boolean;
}

export interface ToolCapabilityPolicyValue {
  tool_name: string;
  description: string;
  input_schema: Record<string, unknown> | null;
  output_schema: Record<string, unknown> | null;
  requires_approval: boolean;
  risk_level: "low" | "medium" | "high";
}

export interface ToolAllowlistPolicyValue {
  profile_id: string;
  tool_names: string[];
}

export interface ToolHookPolicyValue {
  hook_id: string;
  hook_type: "pre_execution" | "post_execution";
  tool_name: string | null;
  condition: Record<string, unknown> | null;
  action: "log" | "block" | "enrich" | "request_approval";
  action_config: Record<string, unknown> | null;
  enabled: boolean;
}

export interface ToolPolicyResource<T> {
  schema_version: "tool_policy_resource.v1";
  resource_kind: "capability" | "allowlist" | "hook";
  resource_id: string;
  resource_sha256: string;
  changed: boolean;
  value: T;
}

export interface ToolPolicyResponse<T> {
  schema_version: "axum_api.v1";
  resource: ToolPolicyResource<T>;
}

export interface ToolCapabilityPolicyRequest {
  description: string;
  input_schema?: Record<string, unknown>;
  output_schema?: Record<string, unknown>;
  requires_approval: boolean;
  risk_level: "low" | "medium" | "high";
  expected_current_sha256?: string;
  confirm_tool_policy: true;
}

export interface ToolAllowlistPolicyRequest {
  tool_names: string[];
  expected_current_sha256?: string;
  confirm_tool_policy: true;
}

export interface ToolHookPolicyRequest {
  hook_type: "pre_execution" | "post_execution";
  tool_name?: string;
  condition?: Record<string, unknown>;
  action: "log" | "block" | "enrich" | "request_approval";
  action_config?: Record<string, unknown>;
  enabled: boolean;
  expected_current_sha256?: string;
  confirm_tool_policy: true;
}

export interface WorkflowRun {
  schema_version: "workflow_run.v1";
  run_sequence: number;
  run_id: string;
  plan_id: string | null;
  created_at: string;
  updated_at: string;
  status: string;
  workflow_id: string;
  dispatch_id: string | null;
  started_at: string | null;
  completed_at: string | null;
  result: unknown;
  graph?: Record<string, unknown>;
  nodes?: Record<string, unknown>[];
  edges?: Record<string, unknown>[];
  events?: WorkflowRunEvent[];
  approvals?: WorkflowRunApproval[];
  boundaries: Record<string, unknown>;
}

export interface WorkflowRunEvent {
  event_sequence: number;
  event_id: string;
  run_id: string;
  node_id: string | null;
  event_type: string;
  actor: string;
  created_at: string;
  details: unknown;
  metadata_only?: boolean;
}

export interface WorkflowRunApproval {
  approval_sequence: number;
  approval_id: string;
  run_id: string;
  node_id: string;
  decision: "requested" | "approved" | "rejected";
  actor: string;
  reason: string | null;
  created_at: string;
  metadata_only?: boolean;
  execution_authority?: string;
  approval_kind?: "tool_execution";
  tool_name?: string;
  profile_id?: string;
  action_sha256?: string;
  resolved_request_id?: string;
}

export interface WorkflowRunCreateRequest {
  plan_id: string;
  confirm_execution?: boolean;
  workspace_id?: string;
}

export interface WorkflowRunEventRequest {
  node_id?: string;
  event_type: string;
  details?: unknown;
}

export interface WorkflowRunApprovalRequest {
  node_id: string;
  decision: "requested" | "approved" | "rejected";
  reason?: string;
}

export interface WorkflowRunActionRequest {
  reason?: string;
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
  git?: {
    default_branch?: string;
    source_revision?: string;
  } | null;
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

export interface Boundaries {
  provider_transport: string;
  target_repository_writes: string;
  sandbox_process_execution: string;
  runtime_workers: string;
  deployment: string;
  docker_required: boolean;
}

export interface AuditEvent {
  audit_id: number;
  created_at: string;
  actor: string;
  action: string;
  resource: string;
  details?: unknown;
}

export interface BackupRecord {
  backup_id: string;
  created_at: string;
  size_bytes: number;
  label: string;
  source_path: string;
  backup_path: string;
  checksum: string;
}

export interface RestoreResult {
  success: boolean;
  records_restored: number;
  errors: string[];
  duration_ms: number;
}

export interface TableIntegrity {
  name: string;
  row_count: number;
  status: string;
}

export interface ProviderHealthStatus {
  schema_version: "axum_api.v1";
  status: string;
  provider_id?: string;
  enabled?: boolean;
  message?: string;
}

export interface ProviderAuditEvent {
  event_id: string;
  dispatch_id: string;
  provider_id: string;
  event_type: string;
  input_token_count: number | null;
  output_token_count: number | null;
  cost: number | null;
  currency: string | null;
  latency_ms: number | null;
  error_domain: string | null;
  redaction_status: string;
  created_at: string;
}

export interface LocalDashboardState {
  readonly schema_version: "local_dashboard.v1";
  readonly status: string;
  readonly counts: Readonly<{ dispatches: number; plans?: number; workflow_runs?: number; team_members: number; api_keys: number; audit_events: number }>;
  readonly dispatches: readonly DispatchItem[];
  readonly team: LocalTeamSnapshot;
  readonly config: Readonly<Record<string, unknown>>;
  readonly costs: LocalCostSummary;
  readonly boundaries: Boundaries;
  readonly cli: LocalCliCapability;
  readonly adaptive_fusion?: AdaptiveFusionOperatorStatus;
  readonly provider_embedding_receipts: readonly ProviderEmbeddingReceiptEvidence[];
}

export interface ProviderEmbeddingReceiptEvidence {
  readonly operation_id: string;
  readonly operation_kind: "memory_version" | "retrieval_query";
  readonly tenant_id: string;
  readonly workspace_id: string;
  readonly run_id: string | null;
  readonly node_id: string | null;
  readonly provider_id: string;
  readonly requested_model_id: string;
  readonly resolved_model_id: string;
  readonly dimensions: number;
  readonly state: string;
  readonly attempt_count: number;
  readonly receipt_sha256: string;
  readonly request_identity_sha256: string;
  readonly reservation_event_id: string;
  readonly send_event_id: string | null;
  readonly outcome_event_id: string | null;
  readonly result_kind: "memory_version" | "retrieval_event" | null;
  readonly result_id: string | null;
  readonly result_sha256: string | null;
  readonly error_domain: string | null;
  readonly created_at: string;
  readonly updated_at: string;
  readonly redacted: true;
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

export interface DispatchListResponse {
  schema_version: "axum_api.v1";
  dispatches: DispatchItem[];
}

export interface DispatchDetailResponse {
  schema_version: "axum_api.v1";
  dispatch: DispatchItem;
}

export interface PlanListResponse {
  schema_version: "axum_api.v1";
  plans: ReadOnlyPlan[];
}

export interface PlanResponse {
  schema_version: "axum_api.v1";
  plan: ReadOnlyPlan;
}

export interface WorkflowRunListResponse {
  schema_version: "axum_api.v1";
  runs: WorkflowRun[];
}

export interface WorkflowRunResponse {
  schema_version: "axum_api.v1";
  run: WorkflowRun;
}

export interface WorkflowRunEventListResponse {
  schema_version: "axum_api.v1";
  events: WorkflowRunEvent[];
}

export interface WorkflowRunEventResponse {
  schema_version: "axum_api.v1";
  event: WorkflowRunEvent;
}

export interface WorkflowRunApprovalListResponse {
  schema_version: "axum_api.v1";
  approvals: WorkflowRunApproval[];
}

export interface WorkflowRunApprovalResponse {
  schema_version: "axum_api.v1";
  approval: WorkflowRunApproval;
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

export interface ConfigResponse {
  schema_version: "axum_api.v1";
  config: Record<string, unknown>;
  boundaries: Boundaries;
}

export interface TeamResponse {
  schema_version: "local_team.v1";
  members: TeamMember[];
  api_keys: ApiKeyMeta[];
}

export interface ExportResponse {
  schema_version: "local_team_export.v1";
  generated_at: number;
  dispatches: DispatchItem[];
  plans?: ReadOnlyPlan[];
  workflow_runs?: WorkflowRun[];
  config: Record<string, unknown>;
  team: LocalTeamSnapshot;
  costs: LocalCostSummary;
  audit: AuditEvent[];
  boundaries: Boundaries;
}

export interface AuditResponse {
  schema_version: "axum_api.v1";
  redacted?: boolean;
  events: AuditEvent[];
}

export interface ProviderAuditResponse {
  schema_version: "axum_api.v1";
  events: ProviderAuditEvent[];
}

export interface KeyListResponse {
  schema_version: "axum_api.v1";
  keys: ApiKeyMeta[];
}

export interface KeyCreateResponse {
  schema_version: "axum_api.v1";
  key_id: string;
  raw_key: string;
  user_id: string;
  role: string;
  scopes: string[];
  created_at: string;
}

export interface KeyRotateResponse {
  schema_version: "axum_api.v1";
  key_id: string;
  raw_key: string;
  user_id: string;
  role: string;
  scopes: string[];
  created_at: string;
  rotated_from: string;
}

export interface OkResponse {
  schema_version: "axum_api.v1";
  ok: boolean;
}

export interface KeyScopesResponse {
  schema_version: "axum_api.v1";
  ok: boolean;
  key_id: string;
  scopes: string[];
}

export interface MemberCreateResponse {
  schema_version: "axum_api.v1";
  ok: boolean;
  user_id: string;
  display_name: string;
  role: string;
}

export interface MemberUpdateResponse {
  schema_version: "axum_api.v1";
  ok: boolean;
  user_id: string;
  role: string;
}

export interface MemberDeleteResponse {
  schema_version: "axum_api.v1";
  ok: boolean;
  user_id: string;
}

export interface BackupListResponse {
  schema_version: "axum_api.v1";
  backups: BackupRecord[];
}

export interface BackupCreateResponse {
  schema_version: "axum_api.v1";
  backup: BackupRecord;
}

export interface BackupDeleteResponse {
  schema_version: "axum_api.v1";
  ok: boolean;
  backup_id: string;
}

export interface BackupRestoreResponse {
  schema_version: "axum_api.v1";
  restore: RestoreResult;
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

export interface BackupVerifyResponse {
  schema_version: "axum_api.v1";
  verification: BackupVerification;
}

export interface BackupRestoreDryRunResponse {
  schema_version: "axum_api.v1";
  restore_dry_run: BackupVerification;
}

export interface OperationsMetricsResponse {
  schema_version: "axum_api.v1";
  executor_type: string;
  auth_required: boolean;
  provider_enabled: boolean;
  local_store: boolean;
  dispatch_count: number;
  plan_count?: number;
  workflow_run_count?: number;
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
  boundaries: Boundaries;
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
  executor_type?: ExecutorType | string | null;
  success?: boolean | null;
  execution_status?: string | null;
  execution_terminal?: boolean | null;
  execution_succeeded?: boolean | null;
  evaluation_status?: string | null;
  evaluation_completed?: boolean | null;
  evaluation_passed?: boolean | null;
  overall_success?: boolean | null;
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
  task_class: string;
  tier: string;
  pattern_type: string;
  occurrences: number;
  avg_cost_usd: number;
  avg_latency_ms: number;
  pass_rate: number;
  suggestion?: string | null;
  [key: string]: unknown;
}

export interface FeedbackPatternListResponse {
  schema_version: "axum_api.v1";
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

export interface PolicySimulationReportOptions {
  limit?: number;
  policy?: string;
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

export interface ProposalCreateRequest {
  title?: string;
  summary?: string;
  task_class?: string;
  task_domain?: string;
  task_intent?: string;
  tier?: string;
  target_tier?: string;
  payload: Record<string, unknown>;
  evidence?: Record<string, unknown>;
}

export interface ProposalActionRequest {
  actor?: string;
  reason?: string;
  confirm_policy_override?: boolean;
}

export type AdaptiveFusionObjective = "efficient" | "quality" | string;

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

export interface StorageIntegrityResponse {
  schema_version: "axum_api.v1";
  integrity: {
    status: string;
    schema_version: number;
    tables: TableIntegrity[];
  };
}

export interface ImportResponse {
  schema_version: "axum_api.v1";
  imported: {
    dispatches: number;
    plans: number;
    workflow_runs: number;
    config: number;
    team: number;
    audit: number;
  };
  errors: string[];
}

export interface SupervisedPatchWorkspaceCreateRequest {
  run_id: string;
  target_id: string;
  target_repo_path: string;
  source_revision: string;
  plan_id?: string;
  source_tree_hash?: string;
  workspace_mode?: "copy" | "git_worktree";
}

export interface SupervisedPatchWorkspaceCreateResponse {
  schema_version: "axum_api.v1";
  workspace: SupervisedPatchWorkspace;
}

export interface SupervisedPatchWorkspaceActionResponse {
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

export interface SupervisedPatchCaptureResponse {
  schema_version: "axum_api.v1";
  artifact: SupervisedPatchArtifact;
}

export interface SupervisedPatchExportRequest {
  run_id: string;
}

export interface SupervisedPatchExportDetail {
  artifact_id: string;
  artifact: SupervisedPatchArtifact;
  approval_binding: Record<string, unknown>;
  integrity: Record<string, unknown>;
  exported_by: string;
  exported_at: string;
}

export interface SupervisedPatchExportResponse {
  schema_version: "axum_api.v1";
  export: SupervisedPatchExportDetail;
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

export interface TargetRepoPatchOutput {
  schema_version: "target_repo_output.v1";
  source_revision: string;
  patch_hash: string;
  patch: string;
}

export interface TargetRepoBranchOutput {
  schema_version: "target_repo_output.v1";
  source_revision: string;
  branch_name: string;
  remote: string;
  commit_sha: string;
  patch_hash: string;
  pr_title: string;
  pr_body: string;
  pull_request?: {
    number: number;
    url: string;
    state: string;
    reused: boolean;
  };
}

export interface TargetRepoOutputResponse {
  schema_version: "axum_api.v1";
  output: TargetRepoPatchOutput | TargetRepoBranchOutput;
  approval_binding: Record<string, unknown>;
  integrity: Record<string, unknown>;
}

export interface WorkflowRunTickRequest {
  actor?: string;
  max_retries?: number;
  executor?: string;
  timeout_ms?: number;
  command?: string;
}

export interface WorkflowRunTickResponse {
  schema_version: "axum_api.v1";
  tick: Record<string, unknown>;
}

export interface SchedulerStatus {
  schema_version: "axum_api.v1";
  scheduler: {
    enabled?: boolean;
    running: boolean;
    supervised_workers_enabled?: boolean;
    worker_count?: number;
    paused?: boolean;
    kill_requested?: boolean;
    workers?: Array<{
      worker_id: string;
      state: string;
      last_heartbeat_at: string;
      tick_count: number;
      error_count: number;
    }>;
    config?: {
      interval_ms: number;
      max_concurrent: number;
      max_queued: number;
      lease_timeout_ms: number;
      executor_type: string;
      heartbeat_interval_sec: number;
    };
    interval_ms?: number;
    max_concurrent?: number;
    lease_timeout_ms?: number;
    active_runs?: number;
  };
}

export type SchedulerControlAction = "pause" | "resume" | "kill";

export interface ExecutorPoolCapabilities {
  supported_task_types: string[];
  supported_task_domains: string[];
  requires_auth: boolean;
  requires_cli: boolean;
  max_timeout_ms: number;
}

export interface ExecutorPoolEntry {
  executor_type: string;
  capabilities: ExecutorPoolCapabilities;
  available: boolean;
  active_count: number;
  concurrency_limit: number;
  cooldown_until: string | null;
  failure_score: number;
  cost_per_execution_usd: number | null;
  daily_cost_usd: number;
  daily_cost_limit_usd: number | null;
  total_executions: number;
  success_rate: number;
  avg_latency_ms: number;
  last_executed_at: string | null;
}

export interface ExecutorPoolStatus {
  schema_version: string;
  executors: ExecutorPoolEntry[];
  total_active: number;
  total_capacity: number;
}

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

export interface QueueTenantListResponse {
  schema_version: "axum_api.v1";
  tenants: TenantQueueInfo[];
}

export interface UpdatePriorityRequest {
  priority: number;
}

export interface PauseRunRequest {
  reason: string | null;
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

export interface RegressionArtifactEnvelope {
  schema_version: "token_efficiency_regression_artifact.v1";
  artifact_id: string;
  artifact_kind: "token_efficiency_regression_report" | "token_efficiency_regression_batch";
  report_schema_version: string;
  content_sha256: string;
  registry_id: string;
  registry_sha256: string;
  scenario_id: string | null;
  created_at: string;
  read_only: true;
  metadata_only: true;
  report: Record<string, unknown>;
}

export interface RegressionTrend {
  schema_version: "token_efficiency_regression_trend.v1";
  scenario_id: string;
  read_only: true;
  report_only: true;
  point_count: number;
  points: Array<Record<string, unknown>>;
  transitions: Array<Record<string, unknown>>;
  latest: Record<string, unknown> | null;
  trend_sha256: string;
}

export interface RegressionArtifactListResponse {
  metadata_only: true;
  read_only: true;
  report_only: true;
  provider_calls: "disabled";
  mutation_authority: "none";
  artifacts: RegressionArtifactEnvelope[];
}

export interface RegressionArtifactResponse extends Omit<RegressionArtifactListResponse, "artifacts"> {
  artifact: RegressionArtifactEnvelope;
}

export interface RegressionTrendResponse extends Omit<RegressionArtifactListResponse, "artifacts"> {
  trend: RegressionTrend;
}

export interface BudgetEvidenceArtifactEnvelope {
  schema_version: "budget_evidence_artifact.v1";
  artifact_id: string;
  artifact_kind: "forecast" | "anomaly";
  evidence_id: string;
  evidence_sha256: string;
  created_at: string;
  read_only: true;
  metadata_only: true;
  evidence: Record<string, unknown>;
}

export interface BudgetEvidenceArtifactListResponse {
  metadata_only: true;
  read_only: true;
  report_only: true;
  provider_calls: "disabled";
  mutation_authority: "none";
  artifacts: BudgetEvidenceArtifactEnvelope[];
  kind: "forecast" | "anomaly" | null;
  limit: number;
  offset: number;
}

export interface BudgetEvidenceArtifactResponse extends Omit<BudgetEvidenceArtifactListResponse, "artifacts" | "kind" | "limit" | "offset"> {
  artifact: BudgetEvidenceArtifactEnvelope;
}

export interface MemoryScope {
  tenant_id: string;
  workspace_id: string;
  agent_id?: string | null;
  task_id?: string | null;
}
export interface DurableMemoryCreateRequest {
  scope: MemoryScope;
  run_id: string;
  source_id: string;
  source_sha256: string;
  conflict_key: string;
  content: unknown;
  confidence: number;
  fresh_until?: string | null;
  expires_at?: string | null;
  supersedes_memory_id?: string | null;
}
export interface DurableMemoryRevisionRequest {
  run_id: string;
  scope: MemoryScope;
  expected_version: number;
  source_id: string;
  source_sha256: string;
  content: unknown;
  confidence: number;
  fresh_until?: string | null;
  expires_at?: string | null;
}
export interface MemorySupersedeRequest {
  run_id: string;
  scope: MemoryScope;
  winner_expected_version: number;
  loser_memory_id: string;
  loser_expected_version: number;
  confirm_supersede: true;
}
export interface MemoryPruneRequest {
  scope: MemoryScope;
  run_id: string;
  confirm_prune: true;
}
export interface MemoryVersionTransitionRequest {
  expected_version: number;
  run_id: string;
  scope: MemoryScope;
}
export interface MemoryReembedRequest extends MemoryVersionTransitionRequest {
  confirm_reembed: true;
}
export interface ProviderEmbeddingResolutionRequest {
  target_version: number;
  expected_attempt_count: number;
  scope: MemoryScope;
  run_id: string;
  action: "retry_failed" | "acknowledge_unknown";
  evidence_source_id?: string | null;
  evidence_sha256?: string | null;
  confirm_resolution: true;
}
export interface ProviderEmbeddingResolutionResponse { resolution: Record<string, unknown>; }
export interface MemoryRetrievalRequest {
  scope: MemoryScope;
  run_id: string;
  node_id: string;
  query: string;
  top_k: number;
  max_tokens: number;
  max_bytes: number;
  allow_lexical_fallback?: boolean;
}
export interface DurableMemoryResponse { memory: Record<string, unknown>; }
export interface DurableMemoryHistoryResponse { versions: Array<Record<string, unknown>>; }
export interface MemoryRetrievalResponse { retrieval: Record<string, unknown>; }
export interface MemorySupersedeResponse { winner: Record<string, unknown>; superseded: Record<string, unknown>; evidence: Record<string, unknown>; }
export interface MemoryPruneResponse { schema_version: "durable_memory_prune.v1"; pruned_count: number; pruned: Array<Record<string, unknown>>; bounded_limit: number; }
export interface UsageObservationListResponse {
  schema_version: "normalized_usage_read.v1";
  run_id: string;
  observations: Array<Record<string, unknown>>;
  count: number;
  limit: number;
  read_only: true;
  metadata_only: true;
  raw_provider_content: "excluded";
}
export interface BudgetRecomputeRequest { run_id: string; confirm_recompute: boolean; }
export interface BudgetRecomputeResponse { producer: Record<string, unknown>; usage_observations: Array<Record<string, unknown>>; }
export interface OfflineReplayGenerateRequest { replay: Record<string, unknown>; confirm_generation: boolean; }
export interface OfflineReplayGenerateResponse { producer: Record<string, unknown>; }
export interface ReplayProductionProfileRequest { profile: Record<string, unknown>; confirm_profile: boolean; }
export interface ReplayProductionProfileResponse {
  schema_version: "offline_replay_production_profile_read.v1";
  configured: boolean;
  profile: Record<string, unknown> | null;
  provider_calls: "disabled";
  mutation_authority: "none";
}
export interface ReplayProductionProfileUpdateResponse { configured: Record<string, unknown>; }
export interface EvidenceChainPromotionRequest {
  replay_artifact_id: string;
  promotion: Record<string, unknown>;
  canary: Record<string, unknown>;
  rollout_scope: string;
  rollback_target: string;
  confirm_promotion: boolean;
}

export type OfflineReplayStatus =
  | "sufficient"
  | "insufficient_evidence"
  | "incompatible_cohort"
  | "stale_evidence"
  | "tampered_evidence"
  | "uncalibrated_evidence"
  | "out_of_distribution"
  | string;

export interface OfflineReplayArtifactEnvelope {
  schema_version: "offline_replay_artifact.v1";
  artifact_id: string;
  report_schema_version: "offline_policy_replay.v1" | "offline_policy_replay.v2";
  status: OfflineReplayStatus;
  eligibility_content_sha256: string;
  content_sha256: string;
  created_at: string;
  read_only: true;
  metadata_only: true;
  provider_calls: "disabled";
  mutation_authority: "none";
  target_repository_writes: "disabled";
  historical_only?: boolean;
  authorization?: "none";
  report: Record<string, unknown>;
}

export interface OfflineReplayListOptions {
  status?: OfflineReplayStatus;
  limit?: number;
  offset?: number;
}

export interface OfflineReplayArtifactListResponse {
  schema_version: "offline_replay_read.v1";
  artifacts: OfflineReplayArtifactEnvelope[];
  status?: OfflineReplayStatus | null;
  limit: number;
  offset: number;
  empty: boolean;
  read_only: true;
  metadata_only: true;
  mutation_authority: "none";
}

export interface OfflineReplayArtifactResponse {
  schema_version: "offline_replay_read.v1";
  artifact: OfflineReplayArtifactEnvelope;
  read_only: true;
  metadata_only: true;
  mutation_authority: "none";
}

export type OperatorDecisionOutcome = "ready" | "conflict" | "expired" | "insufficient_evidence" | "resolved";
export type OperatorDecisionAction = "approve" | "reject" | "pause" | "resume" | "retry" | "rollback" | "inspect" | "acknowledge";
export interface OperatorDecisionEvidenceReference { evidence_type: string; evidence_id: string; content_sha256: string | null; }
export interface OperatorDecisionItem {
  schema_version: "operator_decision_item.v1";
  decision_id: string; conflict_key: string; resource_id: string; outcome: OperatorDecisionOutcome;
  recommended_action: OperatorDecisionAction | null; severity: "info" | "warning" | "critical";
  confidence: number; generated_at: string; freshness_seconds: number; expires_at: string | null;
  reason_codes: string[]; selected_source: OperatorDecisionEvidenceReference | null;
  evidence_references: OperatorDecisionEvidenceReference[]; content_sha256: string;
}
export interface OperatorDecisionQueue {
  schema_version: "operator_decision_queue.v1"; generated_at: string; maximum_freshness_seconds: number;
  total: number; limit: number; offset: number; source_counts: Record<string, number>;
  items: OperatorDecisionItem[]; queue_sha256: string;
}
export interface OperatorDecisionQueueResponse {
  read_only: true; metadata_only: true; mutation_authority: "none"; provider_calls: "disabled";
  target_repository_writes: "disabled"; queue: OperatorDecisionQueue;
}
export interface OperatorDecisionQueueOptions { generated_at?: string; maximum_freshness_seconds?: number; limit?: number; offset?: number; }
export interface OperatorDecisionBudgetPausePolicy {
  schema_version: "budget_auto_pause_policy.v1";
  enabled: boolean;
  minimum_confidence_score: number;
  maximum_freshness_seconds: number;
  require_critical_severity: boolean;
}
export interface OperatorDecisionActionRequest {
  queue_sha256: string;
  generated_at: string;
  maximum_freshness_seconds: number;
  limit: number;
  offset: number;
  action: OperatorDecisionAction;
  confirm_action: boolean;
  reason?: string;
  budget_policy?: OperatorDecisionBudgetPausePolicy;
}
export interface OperatorDecisionActionResponse {
  schema_version: "operator_decision_action_result.v1";
  decision_id: string;
  queue_sha256: string;
  action: OperatorDecisionAction;
  owner_result: unknown;
}
