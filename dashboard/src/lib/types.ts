export type RequestSource = "cli" | "api" | "dashboard" | "agent" | "workflow" | "test_fixture";

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
  request_source: string;
  final_status: string;
  selected_tier: string;
  risk_level: string;
  reserved_cost: number;
  bundle: DispatchBundle;
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
}

export interface LocalCostSummary {
  schema_version: "local_cost_summary.v1";
  currency: "USD" | string;
  dispatch_count: number;
  total_reserved_cost: number;
  by_tier: LocalTierCost[];
}

export interface LocalTierCost {
  selected_tier: string;
  dispatch_count: number;
  reserved_cost: number;
}

export interface LocalBoundaries {
  provider_transport: string;
  target_repository_writes: string;
  sandbox_process_execution: string;
  runtime_workers: string;
  deployment: string;
  docker_required: boolean;
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
  final_status: string;
  created_at: string;
  updated_at: string;
}

export interface TaskAnalysis {
  schema_version: "task_analysis.v1";
  analysis_id: string;
  raw_request_snapshot: string;
  request_source: RequestSource;
  primary_task_type: string;
  task_domain: string;
  task_intent: string;
  risk_flags: string[];
  complexity_score: number;
  cognitive_complexity: number;
  context_complexity: number;
  execution_risk: number;
  ambiguity_score: number;
  required_capabilities: string[];
  context_budget_estimate: number;
  execution_budget_estimate: number;
  quality_requirement: string;
  risk_level: string;
  confidence: number;
  confidence_label: string;
  uncertainty_reason: string[];
  safe_default: string;
  escalation_trigger: string | null;
  positive_evidence: Evidence[];
  negative_evidence: Evidence[];
  features_detected: Record<string, unknown>;
  analysis_method: string;
  created_at: string;
}

export interface Evidence {
  feature: string;
  text: string;
  span: [number, number];
  polarity: string;
  source: string;
  rule_id: string | null;
  confidence: number;
  negation_scope: string | null;
}

export interface DispatchDecision {
  schema_version: "dispatch_decision.v1";
  decision_id: string;
  analysis_id: string;
  analysis_snapshot: Record<string, unknown>;
  selected_tier: string;
  selected_profile_id: string | null;
  fallback_tier: string;
  fallback_profile_id: string | null;
  shadow_routes: ShadowRoute[];
  hard_constraints: string[];
  rejected_candidates: RejectedCandidate[];
  no_shadow_route_reason: string | null;
  max_input_tokens: number;
  max_output_tokens: number;
  routing_reason: string;
  quality_requirement: string;
  expected_quality_band: string;
  confidence: number;
  confidence_label: string;
  budget_reservation: BudgetReservation;
  execution_policy: Record<string, unknown>;
  execution_gates: ExecutionGate[];
  routing_mode: string;
  routing_experiment_id: string | null;
  decision_status: string;
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
  severity: string;
  reason: string;
  evidence_refs: string[];
  clearance_required: string;
  cleared: boolean;
  cleared_by: string | null;
  cleared_at: string | null;
}

export interface ShadowRoute {
  tier: string;
  profile_id: string | null;
  reason: string;
  admission_scope: string;
  estimated_cost: number | null;
  expected_tradeoff: string;
}

export interface RejectedCandidate {
  tier: string;
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
  executor_type: string;
  status: string;
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
  status: string;
  checks: EvaluationCheck[];
  quality_score: number | null;
  requires_retry: boolean;
  retry_reason: string | null;
  created_at: string;
}

export interface EvaluationCheck {
  check_id: string;
  name: string;
  status: string;
  reason: string;
}
