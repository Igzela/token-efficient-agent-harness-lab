type RequestSource = "cli" | "api" | "dashboard" | "agent" | "workflow" | "test_fixture";

type ModelTier = "cheap_executor" | "balanced_worker" | "strong_planner" | "verifier" | "advisor";
type TaskDomain = "code" | "docs" | "config" | "infra" | "math" | "architecture" | "repo_ops" | "governance" | "other";
type TaskIntent = "generate" | "review" | "debug" | "summarize" | "audit" | "plan" | "refactor" | "compare" | "explain" | "classify";
type RiskFlag = "target_write" | "provider_call" | "sandbox_execution" | "deployment" | "secret_handling" | "destructive_operation" | "long_context" | "high_uncertainty";
type QualityRequirement = "draft" | "standard" | "high" | "critical";
type RiskLevel = "low" | "medium" | "high" | "critical";
type ConfidenceLabel = "low" | "medium" | "high";
type EvidencePolarity = "positive" | "negative";
type EvidenceSource = "raw_request" | "repo_context" | "user_constraints" | "target_metadata";
type ExpectedQualityBand = "low" | "medium" | "high" | "unknown";
type DecisionStatus = "decided" | "needs_approval" | "blocked" | "diagnostic_only";
type GateSeverity = "info" | "warning" | "block" | "critical";
type ExecutorType = "noop" | "mock" | "manual" | "provider" | "claude_code_cli" | "codex_cli";
type ExecutionStatus = "not_executed" | "preview_generated" | "mock_completed" | "manual_pending" | "manual_completed" | "failed" | "cli_completed" | "provider_completed";
type EvaluationStatus = "pass" | "fail" | "needs_human_review" | "not_evaluated";
type CheckStatus = "pass" | "fail" | "warning" | "skipped";
type FinalStatus = "dispatched" | "executing" | "completed" | "failed" | "escalated" | "cancelled" | "not_executed" | "manual_pending";

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

export interface EvaluationCheck {
  check_id: string;
  name: string;
  status: CheckStatus;
  reason: string;
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

export interface DispatchBundle {
  record: DispatchRecord;
  analysis: TaskAnalysis;
  decision: DispatchDecision;
  execution_result: ExecutionResult;
  evaluation_result: EvaluationResult;
}

export interface DispatchRequest {
  schema_version?: "dispatch_request.v1";
  raw_request: string;
  request_source: RequestSource;
}

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
  cost_utilization: number;
  by_tier: LocalTierCost[];
  daily: LocalDailyCost[];
}

export interface LocalTierCost {
  selected_tier: string;
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
  selected_tier: string;
  reserved_cost: number;
  input_tokens: number;
  output_tokens: number;
  estimated_cost_usd: number;
  executor_type: string;
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
  request_source: string;
  final_status: string;
  selected_tier: string;
  risk_level: string;
  reserved_cost: number;
  bundle: DispatchBundle;
  input_tokens: number | null;
  output_tokens: number | null;
  estimated_cost_usd: number | null;
  executor_type: string;
  latency_ms: number | null;
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
  counts: { dispatches: number; team_members: number; api_keys: number; audit_events: number };
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
  config: Record<string, unknown>;
  team: LocalTeamSnapshot;
  costs: LocalCostSummary;
  audit: AuditEvent[];
  boundaries: Boundaries;
}

export interface AuditResponse {
  schema_version: "axum_api.v1";
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
    config: number;
    team: number;
    audit: number;
  };
  errors: string[];
}
