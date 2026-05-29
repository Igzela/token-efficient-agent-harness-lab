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
