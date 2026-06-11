export type * from "./wire-types.js";

import type {
  ApiStatus,
  DecisionDetailResponse,
  DecisionListResponse,
  DecisionStatsResponse,
  DispatchBundle,
  DispatchRequest,
  PlanCreateRequest,
  PlanListResponse,
  PlanResponse,
  WorkflowRunActionRequest,
  WorkflowRunApprovalListResponse,
  WorkflowRunApprovalRequest,
  WorkflowRunApprovalResponse,
  WorkflowRunCreateRequest,
  WorkflowRunEventListResponse,
  WorkflowRunEventRequest,
  WorkflowRunEventResponse,
  WorkflowRunListResponse,
  WorkflowRunResponse,
  WorkflowRunTickRequest,
  WorkflowRunTickResponse,
  SchedulerStatus,
  SupervisedPatchArtifactListResponse,
  SupervisedPatchArtifactResponse,
  SupervisedPatchCaptureResponse,
  SupervisedPatchExportRequest,
  SupervisedPatchExportResponse,
  SupervisedPatchWorkspaceActionResponse,
  SupervisedPatchWorkspaceCreateRequest,
  SupervisedPatchWorkspaceCreateResponse,
  SupervisedPatchWorkspaceListResponse,
  SupervisedPatchWorkspaceResponse,
  LocalCostSummary,
  LocalDispatchCostDetail,
  LocalDashboardState,
  DispatchListResponse,
  DispatchDetailResponse,
  ConfigResponse,
  TeamResponse,
  ExportResponse,
  AuditResponse,
  ProviderHealthStatus,
  ProviderAuditResponse,
  DispatchMetricsResponse,
  FeedbackCostOfPassResponse,
  FeedbackPatternListResponse,
  FeedbackTraceListResponse,
  ProposalActionRequest,
  ProposalCreateRequest,
  ProposalListResponse,
  ProposalResponse,
  SimulationReportResponse,
  BackupListResponse,
  BackupCreateResponse,
  BackupDeleteResponse,
  BackupRestoreResponse,
  BackupRestoreDryRunResponse,
  BackupVerifyResponse,
  KeyListResponse,
  KeyCreateResponse,
  KeyRotateResponse,
  OkResponse,
  KeyScopesResponse,
  MemberCreateResponse,
  MemberUpdateResponse,
  MemberDeleteResponse,
  StorageIntegrityResponse,
  ImportResponse,
  OperationsMetricsResponse,
  ExecutorPoolStatus,
  QueueStatusResponse,
  QueueRunListResponse,
  QueueRunResponse,
  QueueTenantListResponse,
} from "./wire-types.js";

export interface AgentControlPlaneClientOptions {
  baseUrl: string;
  apiKey?: string;
  fetchImpl?: typeof fetch;
}

export interface DispatchListOptions {
  limit?: number;
  offset?: number;
  search?: string;
}

export interface PlanListOptions {
  limit?: number;
  offset?: number;
  search?: string;
}

export interface WorkflowRunListOptions {
  limit?: number;
  offset?: number;
  search?: string;
}

export interface WorkflowRunChildListOptions {
  limit?: number;
}

export interface SupervisedPatchListOptions {
  limit?: number;
}

export interface AuditListOptions {
  limit?: number;
  offset?: number;
  redact?: boolean;
  search?: string;
}

export interface ProviderAuditOptions {
  limit?: number;
  offset?: number;
}

export interface CostDetailsOptions {
  limit?: number;
}

export interface DecisionListOptions {
  limit?: number;
  offset?: number;
  search?: string;
  run_id?: string;
}

export interface DispatchMetricsOptions {
  limit?: number;
}

export interface FeedbackTraceOptions {
  limit?: number;
  offset?: number;
  task_class?: string;
  tier?: string;
  status?: string;
}

export interface FeedbackCostOfPassOptions {
  task_class?: string;
  tier?: string;
}

export interface FeedbackPatternOptions {
  task_class?: string;
  tier?: string;
}

export interface SimulationReportOptions {
  limit?: number;
}

export interface ProposalListOptions {
  limit?: number;
  offset?: number;
  status?: string;
}

export class AgentControlPlaneClient {
  private readonly baseUrl: string;
  private readonly apiKey?: string;
  private readonly fetchImpl: typeof fetch;

  constructor(options: AgentControlPlaneClientOptions) {
    this.baseUrl = options.baseUrl.replace(/\/+$/, "");
    this.apiKey = options.apiKey;
    this.fetchImpl = options.fetchImpl ?? fetch;
  }

  health(): Promise<ApiStatus> {
    return this.getJson<ApiStatus>("/api/v1/health");
  }

  ready(): Promise<ApiStatus> {
    return this.getJson<ApiStatus>("/api/v1/ready");
  }

  openapi(): Promise<Record<string, unknown>> {
    return this.getJson<Record<string, unknown>>("/api/v1/openapi.json");
  }

  dashboard(): Promise<LocalDashboardState> {
    return this.getJson<LocalDashboardState>("/api/v1/dashboard");
  }

  metrics(): Promise<OperationsMetricsResponse> {
    return this.getJson<OperationsMetricsResponse>("/api/v1/metrics");
  }

  dispatchMetrics(options: DispatchMetricsOptions = {}): Promise<DispatchMetricsResponse> {
    return this.getJson<DispatchMetricsResponse>(`/api/v1/dispatch-metrics${queryString({
      limit: options.limit,
    })}`);
  }

  feedbackTraces(options: FeedbackTraceOptions = {}): Promise<FeedbackTraceListResponse> {
    return this.getJson<FeedbackTraceListResponse>(`/api/v1/feedback/traces${queryString({
      limit: options.limit,
      offset: options.offset,
      task_class: options.task_class,
      tier: options.tier,
      status: options.status,
    })}`);
  }

  feedbackCostOfPass(options: FeedbackCostOfPassOptions = {}): Promise<FeedbackCostOfPassResponse> {
    return this.getJson<FeedbackCostOfPassResponse>(`/api/v1/feedback/cost-of-pass${queryString({
      task_class: options.task_class,
      tier: options.tier,
    })}`);
  }

  feedbackPatterns(options: FeedbackPatternOptions = {}): Promise<FeedbackPatternListResponse> {
    return this.getJson<FeedbackPatternListResponse>(`/api/v1/feedback/patterns${queryString({
      task_class: options.task_class,
      tier: options.tier,
    })}`);
  }

  simulationReport(options: SimulationReportOptions = {}): Promise<SimulationReportResponse> {
    return this.getJson<SimulationReportResponse>(`/api/v1/simulation/report${queryString({
      limit: options.limit,
    })}`);
  }

  proposals(options: ProposalListOptions = {}): Promise<ProposalListResponse> {
    return this.getJson<ProposalListResponse>(`/api/v1/proposals${queryString({
      limit: options.limit,
      offset: options.offset,
      status: options.status,
    })}`);
  }

  createProposal(request: ProposalCreateRequest): Promise<ProposalResponse> {
    return this.postJson<ProposalResponse>("/api/v1/proposals", {
      title: request.title,
      summary: request.summary,
      task_class: request.task_class,
      task_domain: request.task_domain,
      task_intent: request.task_intent,
      tier: request.tier,
      target_tier: request.target_tier,
      payload: request.payload,
      evidence: request.evidence,
    });
  }

  proposal(proposalId: string): Promise<ProposalResponse> {
    return this.getJson<ProposalResponse>(`/api/v1/proposals/${encodeURIComponent(proposalId)}`);
  }

  approveProposal(
    proposalId: string,
    request: ProposalActionRequest = {},
  ): Promise<ProposalResponse> {
    return this.postJson<ProposalResponse>(
      `/api/v1/proposals/${encodeURIComponent(proposalId)}/approve`,
      {
        actor: request.actor,
        reason: request.reason,
        confirm_policy_override: request.confirm_policy_override ?? true,
      },
    );
  }

  rejectProposal(
    proposalId: string,
    request: ProposalActionRequest = {},
  ): Promise<ProposalResponse> {
    return this.postJson<ProposalResponse>(
      `/api/v1/proposals/${encodeURIComponent(proposalId)}/reject`,
      {
        actor: request.actor,
        reason: request.reason,
      },
    );
  }

  rollbackProposal(
    proposalId: string,
    request: ProposalActionRequest = {},
  ): Promise<ProposalResponse> {
    return this.postJson<ProposalResponse>(
      `/api/v1/proposals/${encodeURIComponent(proposalId)}/rollback`,
      {
        actor: request.actor,
        reason: request.reason,
        confirm_policy_override: request.confirm_policy_override ?? true,
      },
    );
  }

  deactivateProposal(
    proposalId: string,
    request: ProposalActionRequest = {},
  ): Promise<ProposalResponse> {
    return this.postJson<ProposalResponse>(
      `/api/v1/proposals/${encodeURIComponent(proposalId)}/deactivate`,
      {
        actor: request.actor,
        reason: request.reason,
        confirm_policy_override: request.confirm_policy_override ?? true,
      },
    );
  }

  dispatches(options: DispatchListOptions = {}): Promise<DispatchListResponse> {
    return this.getJson<DispatchListResponse>(`/api/v1/dispatches${queryString({
      limit: options.limit,
      offset: options.offset,
      search: options.search,
    })}`);
  }

  plans(options: PlanListOptions = {}): Promise<PlanListResponse> {
    return this.getJson<PlanListResponse>(`/api/v1/plans${queryString({
      limit: options.limit,
      offset: options.offset,
      search: options.search,
    })}`);
  }

  createPlan(request: PlanCreateRequest): Promise<PlanResponse> {
    return this.postJson<PlanResponse>("/api/v1/plans", {
      raw_request: request.raw_request,
      request_source: request.request_source,
    });
  }

  plan(planId: string): Promise<PlanResponse> {
    return this.getJson<PlanResponse>(`/api/v1/plans/${encodeURIComponent(planId)}`);
  }

  workflowRuns(options: WorkflowRunListOptions = {}): Promise<WorkflowRunListResponse> {
    return this.getJson<WorkflowRunListResponse>(`/api/v1/workflow-runs${queryString({
      limit: options.limit,
      offset: options.offset,
      search: options.search,
    })}`);
  }

  createWorkflowRun(request: WorkflowRunCreateRequest): Promise<WorkflowRunResponse> {
    return this.postJson<WorkflowRunResponse>("/api/v1/workflow-runs", {
      plan_id: request.plan_id,
    });
  }

  workflowRun(runId: string): Promise<WorkflowRunResponse> {
    return this.getJson<WorkflowRunResponse>(`/api/v1/workflow-runs/${encodeURIComponent(runId)}`);
  }

  workflowRunEvents(
    runId: string,
    options: WorkflowRunChildListOptions = {},
  ): Promise<WorkflowRunEventListResponse> {
    return this.getJson<WorkflowRunEventListResponse>(
      `/api/v1/workflow-runs/${encodeURIComponent(runId)}/events${queryString({
        limit: options.limit,
      })}`,
    );
  }

  recordWorkflowRunEvent(
    runId: string,
    request: WorkflowRunEventRequest,
  ): Promise<WorkflowRunEventResponse> {
    return this.postJson<WorkflowRunEventResponse>(`/api/v1/workflow-runs/${encodeURIComponent(runId)}/events`, {
      node_id: request.node_id,
      event_type: request.event_type,
      details: request.details,
    });
  }

  workflowRunApprovals(
    runId: string,
    options: WorkflowRunChildListOptions = {},
  ): Promise<WorkflowRunApprovalListResponse> {
    return this.getJson<WorkflowRunApprovalListResponse>(
      `/api/v1/workflow-runs/${encodeURIComponent(runId)}/approvals${queryString({
        limit: options.limit,
      })}`,
    );
  }

  recordWorkflowRunApproval(
    runId: string,
    request: WorkflowRunApprovalRequest,
  ): Promise<WorkflowRunApprovalResponse> {
    return this.postJson<WorkflowRunApprovalResponse>(
      `/api/v1/workflow-runs/${encodeURIComponent(runId)}/approvals`,
      {
        node_id: request.node_id,
        decision: request.decision,
        reason: request.reason,
      },
    );
  }

  resumeWorkflowRun(
    runId: string,
    request: WorkflowRunActionRequest = {},
  ): Promise<WorkflowRunResponse> {
    return this.postJson<WorkflowRunResponse>(`/api/v1/workflow-runs/${encodeURIComponent(runId)}/resume`, {
      reason: request.reason,
    });
  }

  cancelWorkflowRun(
    runId: string,
    request: WorkflowRunActionRequest = {},
  ): Promise<WorkflowRunResponse> {
    return this.postJson<WorkflowRunResponse>(`/api/v1/workflow-runs/${encodeURIComponent(runId)}/cancel`, {
      reason: request.reason,
    });
  }

  supervisedPatchWorkspaces(
    options: SupervisedPatchListOptions = {},
  ): Promise<SupervisedPatchWorkspaceListResponse> {
    return this.getJson<SupervisedPatchWorkspaceListResponse>(
      `/api/v1/supervised-patch/workspaces${queryString({ limit: options.limit })}`,
    );
  }

  supervisedPatchWorkspaceDetail(workspaceId: string): Promise<SupervisedPatchWorkspaceResponse> {
    return this.getJson<SupervisedPatchWorkspaceResponse>(
      `/api/v1/supervised-patch/workspaces/${encodeURIComponent(workspaceId)}`,
    );
  }

  supervisedPatchArtifacts(
    options: SupervisedPatchListOptions = {},
  ): Promise<SupervisedPatchArtifactListResponse> {
    return this.getJson<SupervisedPatchArtifactListResponse>(
      `/api/v1/supervised-patch/artifacts${queryString({ limit: options.limit })}`,
    );
  }

  supervisedPatchArtifactDetail(artifactId: string): Promise<SupervisedPatchArtifactResponse> {
    return this.getJson<SupervisedPatchArtifactResponse>(
      `/api/v1/supervised-patch/artifacts/${encodeURIComponent(artifactId)}`,
    );
  }

  createSupervisedPatchWorkspace(
    request: SupervisedPatchWorkspaceCreateRequest,
  ): Promise<SupervisedPatchWorkspaceCreateResponse> {
    return this.postJson<SupervisedPatchWorkspaceCreateResponse>(
      "/api/v1/supervised-patch/workspaces",
      {
        run_id: request.run_id,
        target_id: request.target_id,
        target_repo_path: request.target_repo_path,
        source_revision: request.source_revision,
        plan_id: request.plan_id,
        source_tree_hash: request.source_tree_hash,
      },
    );
  }

  cleanupSupervisedPatchWorkspace(workspaceId: string): Promise<SupervisedPatchWorkspaceActionResponse> {
    return this.postJson<SupervisedPatchWorkspaceActionResponse>(
      `/api/v1/supervised-patch/workspaces/${encodeURIComponent(workspaceId)}/cleanup`,
      {},
    );
  }

  quarantineSupervisedPatchWorkspace(workspaceId: string): Promise<SupervisedPatchWorkspaceActionResponse> {
    return this.postJson<SupervisedPatchWorkspaceActionResponse>(
      `/api/v1/supervised-patch/workspaces/${encodeURIComponent(workspaceId)}/quarantine`,
      {},
    );
  }

  captureSupervisedPatch(workspaceId: string): Promise<SupervisedPatchCaptureResponse> {
    return this.postJson<SupervisedPatchCaptureResponse>(
      `/api/v1/supervised-patch/workspaces/${encodeURIComponent(workspaceId)}/capture`,
      {},
    );
  }

  exportSupervisedPatchArtifact(
    artifactId: string,
    request: SupervisedPatchExportRequest,
  ): Promise<SupervisedPatchExportResponse> {
    return this.postJson<SupervisedPatchExportResponse>(
      `/api/v1/supervised-patch/artifacts/${encodeURIComponent(artifactId)}/export`,
      { run_id: request.run_id },
    );
  }

  tickWorkflowRun(runId: string, request: WorkflowRunTickRequest = {}): Promise<WorkflowRunTickResponse> {
    return this.postJson<WorkflowRunTickResponse>(
      `/api/v1/workflow-runs/${encodeURIComponent(runId)}/tick`,
      {
        actor: request.actor,
        max_retries: request.max_retries,
        executor: request.executor,
        timeout_ms: request.timeout_ms,
        command: request.command,
      },
    );
  }

  schedulerStatus(): Promise<SchedulerStatus> {
    return this.getJson<SchedulerStatus>("/api/v1/scheduler/status");
  }

  fetchExecutorPool(): Promise<ExecutorPoolStatus> {
    return this.getJson<ExecutorPoolStatus>("/api/v1/executor-pool");
  }

  fetchQueueStatus(): Promise<QueueStatusResponse> {
    return this.getJson<QueueStatusResponse>("/api/v1/queue/status");
  }

  fetchQueueRuns(limit?: number, offset?: number): Promise<QueueRunListResponse> {
    return this.getJson<QueueRunListResponse>(`/api/v1/queue/runs${queryString({ limit, offset })}`);
  }

  updateRunPriority(runId: string, priority: number): Promise<QueueRunResponse> {
    return this.putJson<QueueRunResponse>(`/api/v1/queue/runs/${encodeURIComponent(runId)}/priority`, { priority });
  }

  pauseRun(runId: string, reason?: string | null): Promise<QueueRunResponse> {
    return this.putJson<QueueRunResponse>(`/api/v1/queue/runs/${encodeURIComponent(runId)}/pause`, { reason: reason ?? null });
  }

  fetchQueueTenants(): Promise<QueueTenantListResponse> {
    return this.getJson<QueueTenantListResponse>("/api/v1/queue/tenants");
  }

  decisions(options: DecisionListOptions = {}): Promise<DecisionListResponse> {
    return this.getJson<DecisionListResponse>(`/api/v1/decisions${queryString({
      limit: options.limit,
      offset: options.offset,
      search: options.search,
      run_id: options.run_id,
    })}`);
  }

  decisionDetail(decisionId: string): Promise<DecisionDetailResponse> {
    return this.getJson<DecisionDetailResponse>(`/api/v1/decisions/${encodeURIComponent(decisionId)}`);
  }

  decisionStats(): Promise<DecisionStatsResponse> {
    return this.getJson<DecisionStatsResponse>("/api/v1/decisions/stats");
  }

  config(): Promise<ConfigResponse> {
    return this.getJson<ConfigResponse>("/api/v1/config");
  }

  team(): Promise<TeamResponse> {
    return this.getJson<TeamResponse>("/api/v1/team");
  }

  costs(): Promise<LocalCostSummary> {
    return this.getJson<LocalCostSummary>("/api/v1/costs");
  }

  costDetails(options: CostDetailsOptions | number = {}): Promise<LocalDispatchCostDetail> {
    const limit = typeof options === "number" ? options : options.limit;
    return this.getJson<LocalDispatchCostDetail>(`/api/v1/costs/dispatches${queryString({ limit })}`);
  }

  exportState(): Promise<ExportResponse> {
    return this.getJson<ExportResponse>("/api/v1/export");
  }

  audit(options: AuditListOptions = {}): Promise<AuditResponse> {
    return this.getJson<AuditResponse>(`/api/v1/audit${queryString({
      limit: options.limit,
      offset: options.offset,
      redact: options.redact,
      search: options.search,
    })}`);
  }

  providerHealth(): Promise<ProviderHealthStatus> {
    return this.getJson<ProviderHealthStatus>("/api/v1/provider/health");
  }

  providerAudit(options: ProviderAuditOptions = {}): Promise<ProviderAuditResponse> {
    return this.getJson<ProviderAuditResponse>(`/api/v1/provider/audit${queryString({
      limit: options.limit,
      offset: options.offset,
    })}`);
  }

  dispatch(request: DispatchRequest): Promise<DispatchBundle> {
    return this.postJson<DispatchBundle>("/api/v1/dispatch", {
      raw_request: request.raw_request,
      request_source: request.request_source,
    });
  }

  createBackup(request: { label?: string; confirmLocalBackup: boolean }): Promise<BackupCreateResponse> {
    return this.postJson<BackupCreateResponse>("/api/v1/backups", {
      label: request.label,
      confirm_local_backup: request.confirmLocalBackup,
    });
  }

  listApiKeys(): Promise<KeyListResponse> {
    return this.getJson<KeyListResponse>("/api/v1/keys");
  }

  async createApiKey(request: { user_id: string; role: string; scopes: string[]; expires_at?: number }): Promise<KeyCreateResponse> {
    return this.postJson<KeyCreateResponse>("/api/v1/keys", request);
  }

  async revokeApiKey(keyId: string): Promise<OkResponse> {
    return this.postJson<OkResponse>(`/api/v1/keys/${encodeURIComponent(keyId)}/revoke`, {});
  }

  async rotateApiKey(keyId: string): Promise<KeyRotateResponse> {
    return this.postJson<KeyRotateResponse>(`/api/v1/keys/${encodeURIComponent(keyId)}/rotate`, {});
  }

  async deleteApiKey(keyId: string): Promise<OkResponse> {
    const response = await this.fetchImpl(`${this.baseUrl}/api/v1/keys/${encodeURIComponent(keyId)}`, {
      method: "DELETE",
      headers: this.headers(),
    });
    return parseResponse<OkResponse>(response);
  }

  async updateKeyScopes(keyId: string, scopes: string[]): Promise<KeyScopesResponse> {
    return this.postJson<KeyScopesResponse>(`/api/v1/keys/${encodeURIComponent(keyId)}/scopes`, { scopes });
  }

  async createTeamMember(request: { user_id: string; display_name: string; role: string }): Promise<MemberCreateResponse> {
    return this.postJson<MemberCreateResponse>("/api/v1/team", request);
  }

  async updateMemberRole(userId: string, role: string): Promise<MemberUpdateResponse> {
    const response = await this.fetchImpl(`${this.baseUrl}/api/v1/team/${encodeURIComponent(userId)}`, {
      method: "PUT",
      headers: { ...this.headers(), "content-type": "application/json" },
      body: JSON.stringify({ role }),
    });
    return parseResponse<MemberUpdateResponse>(response);
  }

  async deleteMember(userId: string): Promise<MemberDeleteResponse> {
    const response = await this.fetchImpl(`${this.baseUrl}/api/v1/team/${encodeURIComponent(userId)}`, {
      method: "DELETE",
      headers: this.headers(),
    });
    return parseResponse<MemberDeleteResponse>(response);
  }

  dispatchDetail(dispatchId: string): Promise<DispatchDetailResponse> {
    return this.getJson<DispatchDetailResponse>(`/api/v1/dispatches/${encodeURIComponent(dispatchId)}`);
  }

  listBackups(): Promise<BackupListResponse> {
    return this.getJson<BackupListResponse>("/api/v1/backups");
  }

  verifyBackup(backupId: string): Promise<BackupVerifyResponse> {
    return this.getJson<BackupVerifyResponse>(`/api/v1/backups/${encodeURIComponent(backupId)}/verify`);
  }

  restoreBackupDryRun(backupId: string): Promise<BackupRestoreDryRunResponse> {
    return this.postJson<BackupRestoreDryRunResponse>(
      `/api/v1/backups/${encodeURIComponent(backupId)}/restore/dry-run`,
      { confirm_restore_dry_run: true },
    );
  }

  async deleteBackup(backupId: string): Promise<BackupDeleteResponse> {
    const response = await this.fetchImpl(`${this.baseUrl}/api/v1/backups/${encodeURIComponent(backupId)}`, {
      method: "DELETE",
      headers: this.headers(),
    });
    return parseResponse<BackupDeleteResponse>(response);
  }

  storageIntegrity(): Promise<StorageIntegrityResponse> {
    return this.getJson<StorageIntegrityResponse>("/api/v1/storage/integrity");
  }

  async importSnapshot(snapshot: Record<string, unknown>): Promise<ImportResponse> {
    return this.postJson<ImportResponse>("/api/v1/import", {
      snapshot,
      confirm_import: true,
    });
  }

  async restoreBackup(backupId: string): Promise<BackupRestoreResponse> {
    return this.postJson<BackupRestoreResponse>(
      `/api/v1/backups/${encodeURIComponent(backupId)}/restore`,
      { confirm_restore: true },
    );
  }

  private async getJson<T>(path: string): Promise<T> {
    const response = await this.fetchImpl(`${this.baseUrl}${path}`, {
      headers: this.headers(),
      method: "GET",
    });
    return parseResponse<T>(response);
  }

  private async postJson<T>(path: string, body: unknown): Promise<T> {
    const response = await this.fetchImpl(`${this.baseUrl}${path}`, {
      body: JSON.stringify(body),
      headers: {
        ...this.headers(),
        "content-type": "application/json",
      },
      method: "POST",
    });
    return parseResponse<T>(response);
  }

  private async putJson<T>(path: string, body: unknown): Promise<T> {
    const response = await this.fetchImpl(`${this.baseUrl}${path}`, {
      body: JSON.stringify(body),
      headers: {
        ...this.headers(),
        "content-type": "application/json",
      },
      method: "PUT",
    });
    return parseResponse<T>(response);
  }

  private headers(): Record<string, string> {
    return this.apiKey ? { authorization: `Bearer ${this.apiKey}` } : {};
  }
}

async function parseResponse<T>(response: Response): Promise<T> {
  const text = await response.text();
  const body = text ? JSON.parse(text) : null;
  if (!response.ok) {
    const error = body?.error ?? `HTTP ${response.status}`;
    throw new Error(error);
  }
  return body as T;
}

function queryString(params: Record<string, boolean | number | string | undefined>): string {
  const query = new URLSearchParams();
  for (const [key, value] of Object.entries(params)) {
    if (value === undefined || value === "") continue;
    query.set(key, String(value));
  }
  const text = query.toString();
  return text ? `?${text}` : "";
}
