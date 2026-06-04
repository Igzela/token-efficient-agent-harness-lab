import type {
  ApiStatus,
  AuditListResponse,
  DispatchListResponse,
  LocalDashboardState,
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

function withQuery(path: string, params: Record<string, string | number | undefined>): string {
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

export async function fetchDashboard(): Promise<LocalDashboardState> {
  return fetchJson<LocalDashboardState>(`${BASE}/api/v1/dashboard`);
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

export async function fetchAudit(params: {
  limit?: number;
  offset?: number;
  search?: string;
} = {}): Promise<AuditListResponse> {
  return fetchJson<AuditListResponse>(withQuery("/api/v1/audit", params));
}

export async function fetchProviderHealth(): Promise<Record<string, unknown>> {
  return fetchJson<Record<string, unknown>>(`${BASE}/api/v1/provider/health`);
}
