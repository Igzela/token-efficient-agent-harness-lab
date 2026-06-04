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
