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

export interface PlanCreateRequest {
  raw_request: string;
  request_source?: RequestSource;
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
}

export interface WorkflowRunCreateRequest {
  plan_id: string;
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
  schema_version: "local_dashboard.v1";
  status: string;
  counts: { dispatches: number; plans?: number; workflow_runs?: number; team_members: number; api_keys: number; audit_events: number };
  dispatches: DispatchItem[];
  team: LocalTeamSnapshot;
  config: Record<string, unknown>;
  costs: LocalCostSummary;
  boundaries: Boundaries;
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
}

export interface SupervisedPatchWorkspaceCreateResponse {
  schema_version: "axum_api.v1";
  workspace: SupervisedPatchWorkspace;
}

export interface SupervisedPatchWorkspaceActionResponse {
  schema_version: "axum_api.v1";
  workspace: SupervisedPatchWorkspace;
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
    enabled: boolean;
    running: boolean;
    interval_ms: number;
    max_concurrent: number;
    lease_timeout_ms: number;
    active_runs: number;
  };
}

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
