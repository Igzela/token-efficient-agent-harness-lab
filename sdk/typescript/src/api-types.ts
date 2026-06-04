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
  audit_event_count: number;
  api_key_count: number;
  backup_count: number;
  latest_backup_created_at: string | null;
  total_reserved_cost: number;
  total_estimated_cost_usd: number;
  total_input_tokens: number;
  total_output_tokens: number;
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
    config: number;
    team: number;
    audit: number;
  };
  errors: string[];
}
