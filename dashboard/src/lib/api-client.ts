import type {
  AdaptiveCompletionRequest,
  AdaptiveCompletionResponse,
  AdaptiveFusionPoliciesResponse,
  AdaptivePolicyPromotionRequest,
  AdaptivePolicyPromotionResponse,
  AdaptivePolicyRollbackRequest,
  AdaptivePolicyRollbackResponse,
  ApiStatus,
  AuditListResponse,
  AutoAdjustmentsReport,
  AutoAdjustmentApplyResult,
  BackupVerification,
  CircuitBreakerStatusResponse,
  DispatchMetricsResponse,
  DecisionListResponse,
  DecisionStatsResponse,
  DispatchListResponse,
  ExecutorPoolStatusResponse,
  FeedbackCostOfPassResponse,
  FeedbackPatternListResponse,
  FeedbackTraceListResponse,
  LocalDashboardState,
  LocalDispatchCostDetail,
  ObservabilityMetricsResponse,
  OperationsMetrics,
  ProposalListResponse,
  ProposalResponse,
  ProviderEndpointConfigRequest,
  ProviderEndpointConfigResponse,
  QueueRunListResponse,
  QueueRunResponse,
  QueueStatusResponse,
  QueueTenantListResponse,
  RegulatorStateResponse,
  SchedulerStatusResponse,
  OperatorEvidenceResponse,
  OperatorDecisionQueueResponse,
  ScorecardArtifactListResponse,
  BudgetEvidenceArtifactListResponse,
  OfflineReplayArtifactListResponse,
  PolicySimulationResult,
  SimulationReportResponse,
  StorageIntegrityResponse,
  SupervisedPatchArtifactCaptureResponse,
  SupervisedPatchArtifactListResponse,
  SupervisedPatchArtifactResponse,
  SupervisedPatchExportResponse,
  SupervisedPatchWorkspaceCreateResponse,
  SupervisedPatchWorkspaceListResponse,
  SupervisedPatchWorkspaceResponse,
  SupervisedPatchVerificationRequest,
  SupervisedPatchVerificationResponse,
  TargetRepoOutputRequest,
  TargetRepoOutputResponse,
  WorkflowPlanCreateResponse,
  WorkflowRunActionResponse,
  WorkflowRunApprovalListResponse,
  WorkflowRunApprovalResponse,
  WorkflowRunDetailResponse,
  WorkflowRunEventListResponse,
  WorkflowRunListResponse,
  WorkflowRunTickResponse,
} from "./types";

const BASE = "";
const TOKEN_KEY = "acp_local_token";

export class ApiError extends Error {
  body: unknown;
  code?: string;
  status: number;
  constructor(status: number, message: string, code?: string, body?: unknown) {
    super(message);
    this.name = "ApiError";
    this.body = body;
    this.code = code;
    this.status = status;
  }
}

export function isAuthError(err: unknown): boolean {
  return err instanceof ApiError && (err.status === 401 || err.status === 403);
}

export function getStoredToken(): string | null {
  if (typeof window === "undefined") return null;
  return localStorage.getItem(TOKEN_KEY);
}

export function setStoredToken(token: string): void {
  if (typeof window === "undefined") return;
  localStorage.setItem(TOKEN_KEY, token);
}

export function clearStoredToken(): void {
  if (typeof window === "undefined") return;
  localStorage.removeItem(TOKEN_KEY);
}

function authHeaders(): Record<string, string> {
  const token = getStoredToken();
  if (token) return { Authorization: `Bearer ${token}` };
  return {};
}

async function fetchJson<T>(url: string, init?: RequestInit): Promise<T> {
  let res: Response;
  try {
    res = await fetch(url, {
      ...init,
      headers: { ...authHeaders(), ...(init?.headers ?? {}) },
    });
  } catch {
    throw new ApiError(0, "Network error - is the engine running?");
  }
  if (!res.ok) {
    let message = `${res.status} ${res.statusText}`;
    let code: string | undefined;
    let body: unknown;
    try {
      body = await res.json();
      if (body && typeof body === "object") {
        const record = body as Record<string, unknown>;
        if (typeof record.error === "string") message = record.error;
        if (typeof record.code === "string") code = record.code;
      }
    } catch {
      body = undefined;
    }
    throw new ApiError(res.status, message, code, body);
  }
  return res.json();
}

function withQuery(path: string, params: Record<string, string | number | boolean | undefined>): string {
  const query = new URLSearchParams();
  Object.entries(params).forEach(([key, value]) => {
    if (value !== undefined && value !== "") query.set(key, String(value));
  });
  const suffix = query.toString();
  return `${BASE}${path}${suffix ? `?${suffix}` : ""}`;
}

export async function fetchHealth(): Promise<ApiStatus> {
  return fetchJson<ApiStatus>(`${BASE}/api/v1/health`);
}

export async function fetchReady(): Promise<ApiStatus> {
  return fetchJson<ApiStatus>(`${BASE}/api/v1/ready`);
}

export async function createAdaptiveCompletion(
  request: AdaptiveCompletionRequest,
): Promise<AdaptiveCompletionResponse> {
  return fetchJson<AdaptiveCompletionResponse>(
    `${BASE}/api/v1/adaptive-fusion/completions`,
    {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(request),
    },
  );
}

export async function fetchDashboard(): Promise<LocalDashboardState> {
  return fetchJson<LocalDashboardState>(`${BASE}/api/v1/dashboard`);
}

export async function fetchProviderEndpoints(): Promise<ProviderEndpointConfigResponse> {
  return fetchJson<ProviderEndpointConfigResponse>(`${BASE}/api/v1/provider/endpoints`);
}

export async function saveProviderEndpoints(
  request: ProviderEndpointConfigRequest,
): Promise<ProviderEndpointConfigResponse> {
  return fetchJson<ProviderEndpointConfigResponse>(`${BASE}/api/v1/provider/endpoints`, {
    method: "PUT",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(request),
  });
}

export async function fetchDispatches(params: {
  limit?: number;
  offset?: number;
  search?: string;
} = {}): Promise<DispatchListResponse> {
  return fetchJson<DispatchListResponse>(withQuery("/api/v1/dispatches", params));
}

export async function createApiKey(request: { user_id: string; role: string; scopes: string[]; expires_at?: number }): Promise<Record<string, unknown>> {
  return fetchJson<Record<string, unknown>>(`${BASE}/api/v1/keys`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(request),
  });
}

export async function revokeApiKey(keyId: string): Promise<Record<string, unknown>> {
  return fetchJson<Record<string, unknown>>(`${BASE}/api/v1/keys/${encodeURIComponent(keyId)}/revoke`, { method: "POST" });
}

export async function rotateApiKey(keyId: string): Promise<Record<string, unknown>> {
  return fetchJson<Record<string, unknown>>(`${BASE}/api/v1/keys/${encodeURIComponent(keyId)}/rotate`, { method: "POST" });
}

export async function deleteApiKey(keyId: string): Promise<Record<string, unknown>> {
  return fetchJson<Record<string, unknown>>(`${BASE}/api/v1/keys/${encodeURIComponent(keyId)}`, { method: "DELETE" });
}

export async function createTeamMember(request: { user_id: string; display_name: string; role: string }): Promise<Record<string, unknown>> {
  return fetchJson<Record<string, unknown>>(`${BASE}/api/v1/team`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(request),
  });
}

export async function updateMemberRole(userId: string, role: string): Promise<Record<string, unknown>> {
  return fetchJson<Record<string, unknown>>(`${BASE}/api/v1/team/${encodeURIComponent(userId)}`, {
    method: "PUT",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ role }),
  });
}

export async function deleteMember(userId: string): Promise<Record<string, unknown>> {
  return fetchJson<Record<string, unknown>>(`${BASE}/api/v1/team/${encodeURIComponent(userId)}`, { method: "DELETE" });
}

export async function fetchDispatchDetail(dispatchId: string): Promise<Record<string, unknown>> {
  return fetchJson<Record<string, unknown>>(`${BASE}/api/v1/dispatches/${encodeURIComponent(dispatchId)}`);
}

export async function fetchBackups(): Promise<Record<string, unknown>> {
  return fetchJson<Record<string, unknown>>(`${BASE}/api/v1/backups`);
}

export async function createBackup(): Promise<Record<string, unknown>> {
  return fetchJson<Record<string, unknown>>(`${BASE}/api/v1/backups`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ confirm_local_backup: true }),
  });
}

export async function deleteBackup(backupId: string): Promise<Record<string, unknown>> {
  return fetchJson<Record<string, unknown>>(`${BASE}/api/v1/backups/${encodeURIComponent(backupId)}`, { method: "DELETE" });
}

export async function restoreBackup(backupId: string): Promise<Record<string, unknown>> {
  return fetchJson<Record<string, unknown>>(`${BASE}/api/v1/backups/${encodeURIComponent(backupId)}/restore`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ confirm_restore: true }),
  });
}

export async function verifyBackup(backupId: string): Promise<{ schema_version: "axum_api.v1"; verification: BackupVerification }> {
  return fetchJson<{ schema_version: "axum_api.v1"; verification: BackupVerification }>(
    `${BASE}/api/v1/backups/${encodeURIComponent(backupId)}/verify`,
  );
}

export async function restoreBackupDryRun(backupId: string): Promise<{ schema_version: "axum_api.v1"; restore_dry_run: BackupVerification }> {
  return fetchJson<{ schema_version: "axum_api.v1"; restore_dry_run: BackupVerification }>(
    `${BASE}/api/v1/backups/${encodeURIComponent(backupId)}/restore/dry-run`,
    {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ confirm_restore_dry_run: true }),
    },
  );
}

export async function fetchAudit(params: {
  limit?: number;
  offset?: number;
  redact?: boolean;
  search?: string;
} = {}): Promise<AuditListResponse> {
  return fetchJson<AuditListResponse>(withQuery("/api/v1/audit", params));
}

export async function fetchStorageIntegrity(): Promise<StorageIntegrityResponse> {
  return fetchJson<StorageIntegrityResponse>(`${BASE}/api/v1/storage/integrity`);
}

export async function fetchCostDetails(params: { limit?: number } = {}): Promise<LocalDispatchCostDetail> {
  return fetchJson<LocalDispatchCostDetail>(withQuery("/api/v1/costs/dispatches", params));
}

export async function updateKeyScopes(keyId: string, scopes: string[]): Promise<Record<string, unknown>> {
  return fetchJson<Record<string, unknown>>(
    `${BASE}/api/v1/keys/${encodeURIComponent(keyId)}/scopes`,
    {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ scopes }),
    },
  );
}

export async function fetchCircuitBreakerStatus(): Promise<CircuitBreakerStatusResponse> {
  return fetchJson<CircuitBreakerStatusResponse>(`${BASE}/api/v1/circuit-breaker/status`);
}

export async function fetchObservabilityMetrics(): Promise<ObservabilityMetricsResponse> {
  return fetchJson<ObservabilityMetricsResponse>(`${BASE}/api/v1/metrics/observability`);
}

export async function fetchMetrics(): Promise<OperationsMetrics> {
  return fetchJson<OperationsMetrics>(`${BASE}/api/v1/metrics`);
}

export async function fetchDispatchMetrics(params: { limit?: number } = {}): Promise<DispatchMetricsResponse> {
  return fetchJson<DispatchMetricsResponse>(withQuery("/api/v1/dispatch-metrics", params));
}

export async function fetchFeedbackTraces(params: {
  limit?: number;
  offset?: number;
  task_class?: string;
  tier?: string;
  status?: string;
} = {}): Promise<FeedbackTraceListResponse> {
  return fetchJson<FeedbackTraceListResponse>(withQuery("/api/v1/feedback/traces", params));
}

export async function fetchFeedbackPatterns(params: {
  limit?: number;
  offset?: number;
  pattern_type?: string;
  severity?: string;
} = {}): Promise<FeedbackPatternListResponse> {
  return fetchJson<FeedbackPatternListResponse>(withQuery("/api/v1/feedback/patterns", params));
}

export async function fetchFeedbackCostOfPass(params: {
  task_class?: string;
  tier?: string;
} = {}): Promise<FeedbackCostOfPassResponse> {
  return fetchJson<FeedbackCostOfPassResponse>(withQuery("/api/v1/feedback/cost-of-pass", params));
}

export async function fetchSimulationReport(params: { limit?: number } = {}): Promise<SimulationReportResponse> {
  return fetchJson<SimulationReportResponse>(withQuery("/api/v1/simulation/report", params));
}

export async function fetchPolicySimulationReport(params: { limit?: number; policy?: string } = {}): Promise<PolicySimulationResult> {
  return fetchJson<PolicySimulationResult>(withQuery("/api/v1/simulation/policy-delta", params));
}

export async function fetchOfflineReplayArtifacts(params: {
  status?: string;
  limit?: number;
  offset?: number;
} = {}): Promise<OfflineReplayArtifactListResponse> {
  return fetchJson<OfflineReplayArtifactListResponse>(withQuery("/api/v1/offline-replays", params));
}

export async function fetchProposals(params: {
  limit?: number;
  offset?: number;
  status?: string;
} = {}): Promise<ProposalListResponse> {
  return fetchJson<ProposalListResponse>(withQuery("/api/v1/proposals", params));
}

export async function fetchGeneratedProposals(params: { limit?: number } = {}): Promise<{ schema_version: string; total: number; candidates: unknown[] }> {
  return fetchJson<{ schema_version: string; total: number; candidates: unknown[] }>(
    withQuery("/api/v1/proposals/generated", params),
  );
}

export async function approveProposal(proposalId: string, reason?: string): Promise<ProposalResponse> {
  return fetchJson<ProposalResponse>(
    `${BASE}/api/v1/proposals/${encodeURIComponent(proposalId)}/approve`,
    {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ reason, confirm_policy_override: true }),
    },
  );
}

export async function rejectProposal(proposalId: string, reason?: string): Promise<ProposalResponse> {
  return fetchJson<ProposalResponse>(
    `${BASE}/api/v1/proposals/${encodeURIComponent(proposalId)}/reject`,
    {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ reason, confirm_policy_override: true }),
    },
  );
}

export async function rollbackProposal(proposalId: string, reason?: string): Promise<ProposalResponse> {
  return fetchJson<ProposalResponse>(
    `${BASE}/api/v1/proposals/${encodeURIComponent(proposalId)}/rollback`,
    {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ reason, confirm_policy_override: true }),
    },
  );
}

export async function deactivateProposal(proposalId: string, reason?: string): Promise<ProposalResponse> {
  return fetchJson<ProposalResponse>(
    `${BASE}/api/v1/proposals/${encodeURIComponent(proposalId)}/deactivate`,
    {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ reason }),
    },
  );
}

export async function fetchAdaptiveFusionPolicies(): Promise<AdaptiveFusionPoliciesResponse> {
  return fetchJson<AdaptiveFusionPoliciesResponse>(`${BASE}/api/v1/adaptive-fusion/policies`);
}

export async function promoteAdaptiveFusionPolicy(
  request: AdaptivePolicyPromotionRequest,
): Promise<AdaptivePolicyPromotionResponse> {
  return fetchJson<AdaptivePolicyPromotionResponse>(`${BASE}/api/v1/adaptive-fusion/policies/promote`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(request),
  });
}

export async function rollbackAdaptiveFusionPolicy(
  adjustmentId: string,
  request: AdaptivePolicyRollbackRequest = {},
): Promise<AdaptivePolicyRollbackResponse> {
  return fetchJson<AdaptivePolicyRollbackResponse>(
    `${BASE}/api/v1/adaptive-fusion/policies/${encodeURIComponent(adjustmentId)}/rollback`,
    {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        actor: request.actor,
        reason: request.reason,
        confirm_adaptive_policy_rollback:
          request.confirm_adaptive_policy_rollback ?? true,
      }),
    },
  );
}

export async function fetchProviderHealth(): Promise<Record<string, unknown>> {
  return fetchJson<Record<string, unknown>>(`${BASE}/api/v1/provider/health`);
}

export async function fetchSupervisedPatchWorkspaces(params: { limit?: number } = {}): Promise<SupervisedPatchWorkspaceListResponse> {
  return fetchJson<SupervisedPatchWorkspaceListResponse>(withQuery("/api/v1/supervised-patch/workspaces", params));
}

export async function fetchSupervisedPatchWorkspaceDetail(workspaceId: string): Promise<SupervisedPatchWorkspaceResponse> {
  return fetchJson<SupervisedPatchWorkspaceResponse>(
    `${BASE}/api/v1/supervised-patch/workspaces/${encodeURIComponent(workspaceId)}`,
  );
}

export async function fetchSupervisedPatchArtifacts(params: { limit?: number } = {}): Promise<SupervisedPatchArtifactListResponse> {
  return fetchJson<SupervisedPatchArtifactListResponse>(withQuery("/api/v1/supervised-patch/artifacts", params));
}

export async function fetchSupervisedPatchArtifactDetail(artifactId: string): Promise<SupervisedPatchArtifactResponse> {
  return fetchJson<SupervisedPatchArtifactResponse>(
    `${BASE}/api/v1/supervised-patch/artifacts/${encodeURIComponent(artifactId)}`,
  );
}

export async function createSupervisedPatchWorkspace(request: {
  run_id: string;
  target_id: string;
  target_repo_path: string;
  source_revision: string;
  plan_id?: string;
  source_tree_hash?: string;
  workspace_mode?: "copy" | "git_worktree";
}): Promise<SupervisedPatchWorkspaceCreateResponse> {
  return fetchJson<SupervisedPatchWorkspaceCreateResponse>(
    `${BASE}/api/v1/supervised-patch/workspaces`,
    {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(request),
    },
  );
}

export async function cleanupSupervisedPatchWorkspace(workspaceId: string): Promise<SupervisedPatchWorkspaceResponse> {
  return fetchJson<SupervisedPatchWorkspaceResponse>(
    `${BASE}/api/v1/supervised-patch/workspaces/${encodeURIComponent(workspaceId)}/cleanup`,
    { method: "POST" },
  );
}

export async function quarantineSupervisedPatchWorkspace(workspaceId: string): Promise<SupervisedPatchWorkspaceResponse> {
  return fetchJson<SupervisedPatchWorkspaceResponse>(
    `${BASE}/api/v1/supervised-patch/workspaces/${encodeURIComponent(workspaceId)}/quarantine`,
    { method: "POST" },
  );
}

export async function verifySupervisedPatchWorkspace(
  workspaceId: string,
  request: SupervisedPatchVerificationRequest,
): Promise<SupervisedPatchVerificationResponse> {
  return fetchJson<SupervisedPatchVerificationResponse>(
    `${BASE}/api/v1/supervised-patch/workspaces/${encodeURIComponent(workspaceId)}/verify`,
    {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(request),
    },
  );
}

export async function captureSupervisedPatch(workspaceId: string): Promise<SupervisedPatchArtifactCaptureResponse> {
  return fetchJson<SupervisedPatchArtifactCaptureResponse>(
    `${BASE}/api/v1/supervised-patch/workspaces/${encodeURIComponent(workspaceId)}/capture`,
    { method: "POST" },
  );
}

export async function exportSupervisedPatchArtifact(artifactId: string, runId: string): Promise<SupervisedPatchExportResponse> {
  return fetchJson<SupervisedPatchExportResponse>(
    `${BASE}/api/v1/supervised-patch/artifacts/${encodeURIComponent(artifactId)}/export`,
    {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ run_id: runId }),
    },
  );
}

export async function targetRepoOutput(
  artifactId: string,
  request: TargetRepoOutputRequest,
): Promise<TargetRepoOutputResponse> {
  return fetchJson<TargetRepoOutputResponse>(
    `${BASE}/api/v1/supervised-patch/artifacts/${encodeURIComponent(artifactId)}/output`,
    {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(request),
    },
  );
}

export async function recordWorkflowRunApproval(
  runId: string,
  request: {
    node_id: string;
    decision: string;
    reason?: string;
    bound_patch_hash?: string;
    bound_source_revision?: string;
    bound_changed_files?: string[];
    expires_at?: string;
  },
): Promise<WorkflowRunApprovalResponse> {
  return fetchJson<WorkflowRunApprovalResponse>(
    `${BASE}/api/v1/workflow-runs/${encodeURIComponent(runId)}/approvals`,
    {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(request),
    },
  );
}

export async function tickWorkflowRun(runId: string, request?: {
  actor?: string;
  max_retries?: number;
  executor?: string;
  timeout_ms?: number;
}): Promise<WorkflowRunTickResponse> {
  return fetchJson<WorkflowRunTickResponse>(
    `${BASE}/api/v1/workflow-runs/${encodeURIComponent(runId)}/tick`,
    {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(request ?? {}),
    },
  );
}

export async function cancelWorkflowRun(runId: string, reason?: string): Promise<WorkflowRunActionResponse> {
  return fetchJson<WorkflowRunActionResponse>(
    `${BASE}/api/v1/workflow-runs/${encodeURIComponent(runId)}/cancel`,
    {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ reason }),
    },
  );
}


export async function fetchScorecards(params: {
  run_id?: string;
  dispatch_id?: string;
  scenario_id?: string;
  limit?: number;
}): Promise<ScorecardArtifactListResponse> {
  return fetchJson<ScorecardArtifactListResponse>(withQuery("/api/v1/scorecards", params));
}

export async function fetchBudgetEvidence(params: {
  kind?: "forecast" | "anomaly";
  limit?: number;
  offset?: number;
} = {}): Promise<BudgetEvidenceArtifactListResponse> {
  return fetchJson<BudgetEvidenceArtifactListResponse>(withQuery("/api/v1/budget-evidence", params));
}

export async function fetchOperatorEvidence(runId: string): Promise<OperatorEvidenceResponse> {
  return fetchJson<OperatorEvidenceResponse>(
    `${BASE}/api/v1/operator/evidence/${encodeURIComponent(runId)}`,
  );
}

export async function fetchOperatorDecisions(params: { generated_at?: string; maximum_freshness_seconds?: number; limit?: number; offset?: number } = {}): Promise<OperatorDecisionQueueResponse> {
  return fetchJson<OperatorDecisionQueueResponse>(withQuery("/api/v1/operator/decisions", params));
}

export async function fetchWorkflowRuns(params: {
  limit?: number;
  offset?: number;
  search?: string;
} = {}): Promise<WorkflowRunListResponse> {
  return fetchJson<WorkflowRunListResponse>(withQuery("/api/v1/workflow-runs", params));
}

export async function fetchWorkflowRunDetail(runId: string): Promise<WorkflowRunDetailResponse> {
  return fetchJson<WorkflowRunDetailResponse>(
    `${BASE}/api/v1/workflow-runs/${encodeURIComponent(runId)}`,
  );
}

export async function fetchWorkflowRunEvents(runId: string, params: {
  limit?: number;
} = {}): Promise<WorkflowRunEventListResponse> {
  return fetchJson<WorkflowRunEventListResponse>(withQuery(`/api/v1/workflow-runs/${encodeURIComponent(runId)}/events`, params));
}

export async function fetchWorkflowRunApprovals(runId: string, params: {
  limit?: number;
} = {}): Promise<WorkflowRunApprovalListResponse> {
  return fetchJson<WorkflowRunApprovalListResponse>(withQuery(`/api/v1/workflow-runs/${encodeURIComponent(runId)}/approvals`, params));
}

export async function createWorkflowPlan(request: {
  raw_request: string;
  request_source?: string;
}): Promise<WorkflowPlanCreateResponse> {
  return fetchJson<WorkflowPlanCreateResponse>(`${BASE}/api/v1/plans`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(request),
  });
}

export async function createWorkflowRun(planId: string): Promise<WorkflowRunActionResponse> {
  return fetchJson<WorkflowRunActionResponse>(`${BASE}/api/v1/workflow-runs`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ plan_id: planId }),
  });
}

export async function fetchSchedulerStatus(): Promise<SchedulerStatusResponse> {
  return fetchJson<SchedulerStatusResponse>(`${BASE}/api/v1/scheduler/status`);
}

export async function controlScheduler(action: "pause" | "resume" | "kill"): Promise<SchedulerStatusResponse> {
  return fetchJson<SchedulerStatusResponse>(`${BASE}/api/v1/scheduler/control`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ action, confirm_control: true }),
  });
}

export async function fetchExecutorPool(): Promise<ExecutorPoolStatusResponse> {
  return fetchJson<ExecutorPoolStatusResponse>(`${BASE}/api/v1/executor-pool`);
}

export async function fetchQueueStatus(): Promise<QueueStatusResponse> {
  return fetchJson<QueueStatusResponse>(`${BASE}/api/v1/queue/status`);
}

export async function fetchQueueRuns(params: {
  limit?: number;
  offset?: number;
} = {}): Promise<QueueRunListResponse> {
  return fetchJson<QueueRunListResponse>(withQuery("/api/v1/queue/runs", params));
}

export async function updateRunPriority(runId: string, priority: number): Promise<QueueRunResponse> {
  return fetchJson<QueueRunResponse>(
    `${BASE}/api/v1/queue/runs/${encodeURIComponent(runId)}/priority`,
    {
      method: "PUT",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ priority }),
    },
  );
}

export async function pauseRun(runId: string, reason?: string | null): Promise<QueueRunResponse> {
  return fetchJson<QueueRunResponse>(
    `${BASE}/api/v1/queue/runs/${encodeURIComponent(runId)}/pause`,
    {
      method: "PUT",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ reason: reason ?? null }),
    },
  );
}

export async function fetchQueueTenants(): Promise<QueueTenantListResponse> {
  return fetchJson<QueueTenantListResponse>(`${BASE}/api/v1/queue/tenants`);
}

export async function fetchDecisions(params: {
  limit?: number;
  offset?: number;
  search?: string;
  run_id?: string;
} = {}): Promise<DecisionListResponse> {
  return fetchJson<DecisionListResponse>(withQuery("/api/v1/decisions", params));
}

export async function fetchDecisionStats(): Promise<DecisionStatsResponse> {
  return fetchJson<DecisionStatsResponse>(`${BASE}/api/v1/decisions/stats`);
}

export async function fetchAutoAdjustments(params: { limit?: number } = {}): Promise<AutoAdjustmentsReport> {
  return fetchJson<AutoAdjustmentsReport>(withQuery("/api/v1/auto-adjustments", params));
}

export async function applyAutoAdjustment(request: {
  actor?: string;
  candidate_id?: string;
  confirm_auto_adjustment?: boolean;
}): Promise<AutoAdjustmentApplyResult> {
  return fetchJson<AutoAdjustmentApplyResult>(`${BASE}/api/v1/auto-adjustments/apply`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(request),
  });
}

export async function rollbackAutoAdjustment(
  adjustmentId: string,
  request: {
    actor?: string;
    reason?: string;
    confirm_auto_adjustment_rollback?: boolean;
  } = {},
): Promise<Record<string, unknown>> {
  return fetchJson<Record<string, unknown>>(
    `${BASE}/api/v1/auto-adjustments/${encodeURIComponent(adjustmentId)}/rollback`,
    {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(request),
    },
  );
}

export async function fetchRegulatorState(): Promise<RegulatorStateResponse> {
  return fetchJson<RegulatorStateResponse>(`${BASE}/api/v1/regulator/state`);
}
