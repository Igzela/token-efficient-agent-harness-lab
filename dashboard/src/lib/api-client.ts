import type { ApiStatus, LocalDashboardState } from "./types";

const BASE = "";

export async function fetchHealth(): Promise<ApiStatus> {
  const res = await fetch(`${BASE}/api/v1/health`);
  if (!res.ok) throw new Error(`Health check failed: ${res.status}`);
  return res.json();
}

export async function fetchReady(): Promise<ApiStatus> {
  const res = await fetch(`${BASE}/api/v1/ready`);
  if (!res.ok) throw new Error(`Ready check failed: ${res.status}`);
  return res.json();
}

export async function fetchDashboard(): Promise<LocalDashboardState> {
  const res = await fetch(`${BASE}/api/v1/dashboard`);
  if (!res.ok) throw new Error(`Dashboard state failed: ${res.status}`);
  return res.json();
}

export async function createApiKey(request: { user_id: string; role: string; scopes: string[]; expires_at?: number }): Promise<Record<string, unknown>> {
    const res = await fetch(`${BASE}/api/v1/keys`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(request),
    });
    if (!res.ok) throw new Error(`createApiKey failed: ${res.status}`);
    return res.json();
}

export async function revokeApiKey(keyId: string): Promise<Record<string, unknown>> {
    const res = await fetch(`${BASE}/api/v1/keys/${encodeURIComponent(keyId)}/revoke`, { method: "POST" });
    if (!res.ok) throw new Error(`revokeApiKey failed: ${res.status}`);
    return res.json();
}

export async function rotateApiKey(keyId: string): Promise<Record<string, unknown>> {
    const res = await fetch(`${BASE}/api/v1/keys/${encodeURIComponent(keyId)}/rotate`, { method: "POST" });
    if (!res.ok) throw new Error(`rotateApiKey failed: ${res.status}`);
    return res.json();
}

export async function deleteApiKey(keyId: string): Promise<Record<string, unknown>> {
    const res = await fetch(`${BASE}/api/v1/keys/${encodeURIComponent(keyId)}`, { method: "DELETE" });
    if (!res.ok) throw new Error(`deleteApiKey failed: ${res.status}`);
    return res.json();
}

export async function createTeamMember(request: { user_id: string; display_name: string; role: string }): Promise<Record<string, unknown>> {
    const res = await fetch(`${BASE}/api/v1/team`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(request),
    });
    if (!res.ok) throw new Error(`createTeamMember failed: ${res.status}`);
    return res.json();
}

export async function updateMemberRole(userId: string, role: string): Promise<Record<string, unknown>> {
    const res = await fetch(`${BASE}/api/v1/team/${encodeURIComponent(userId)}`, {
        method: "PUT",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ role }),
    });
    if (!res.ok) throw new Error(`updateMemberRole failed: ${res.status}`);
    return res.json();
}

export async function deleteMember(userId: string): Promise<Record<string, unknown>> {
    const res = await fetch(`${BASE}/api/v1/team/${encodeURIComponent(userId)}`, { method: "DELETE" });
    if (!res.ok) throw new Error(`deleteMember failed: ${res.status}`);
    return res.json();
}

export async function fetchDispatchDetail(dispatchId: string): Promise<Record<string, unknown>> {
    const res = await fetch(`${BASE}/api/v1/dispatches/${encodeURIComponent(dispatchId)}`);
    if (!res.ok) throw new Error(`Dispatch detail failed: ${res.status}`);
    return res.json();
}

export async function fetchBackups(): Promise<Record<string, unknown>> {
    const res = await fetch(`${BASE}/api/v1/backups`);
    if (!res.ok) throw new Error(`Fetch backups failed: ${res.status}`);
    return res.json();
}

export async function deleteBackup(backupId: string): Promise<Record<string, unknown>> {
    const res = await fetch(`${BASE}/api/v1/backups/${encodeURIComponent(backupId)}`, { method: "DELETE" });
    if (!res.ok) throw new Error(`Delete backup failed: ${res.status}`);
    return res.json();
}

export async function restoreBackup(backupId: string): Promise<Record<string, unknown>> {
    const res = await fetch(`${BASE}/api/v1/backups/${encodeURIComponent(backupId)}/restore`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ confirm_restore: true }),
    });
    if (!res.ok) throw new Error(`Restore backup failed: ${res.status}`);
    return res.json();
}

export async function fetchAudit(): Promise<Record<string, unknown>> {
    const res = await fetch(`${BASE}/api/v1/audit`);
    if (!res.ok) throw new Error(`Fetch audit failed: ${res.status}`);
    return res.json();
}

export async function fetchProviderHealth(): Promise<Record<string, unknown>> {
    const res = await fetch(`${BASE}/api/v1/provider/health`);
    if (!res.ok) throw new Error(`Provider health failed: ${res.status}`);
    return res.json();
}
