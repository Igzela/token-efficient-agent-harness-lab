export type * from "./wire-types.js";

import type {
  ApiStatus,
  DispatchBundle,
  DispatchRequest,
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
  BackupListResponse,
  BackupCreateResponse,
  BackupDeleteResponse,
  BackupRestoreResponse,
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

  dispatches(options: DispatchListOptions = {}): Promise<DispatchListResponse> {
    return this.getJson<DispatchListResponse>(`/api/v1/dispatches${queryString({
      limit: options.limit,
      offset: options.offset,
      search: options.search,
    })}`);
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

  costDetails(limit: number = 50): Promise<LocalDispatchCostDetail> {
    return this.getJson<LocalDispatchCostDetail>(`/api/v1/costs/dispatches?limit=${limit}`);
  }

  exportState(): Promise<ExportResponse> {
    return this.getJson<ExportResponse>("/api/v1/export");
  }

  audit(): Promise<AuditResponse> {
    return this.getJson<AuditResponse>("/api/v1/audit");
  }

  providerHealth(): Promise<ProviderHealthStatus> {
    return this.getJson<ProviderHealthStatus>("/api/v1/provider/health");
  }

  providerAudit(): Promise<ProviderAuditResponse> {
    return this.getJson<ProviderAuditResponse>("/api/v1/provider/audit");
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

function queryString(params: Record<string, number | string | undefined>): string {
  const query = new URLSearchParams();
  for (const [key, value] of Object.entries(params)) {
    if (value === undefined || value === "") continue;
    query.set(key, String(value));
  }
  const text = query.toString();
  return text ? `?${text}` : "";
}
