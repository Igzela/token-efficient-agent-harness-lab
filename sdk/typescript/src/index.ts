export type * from "./wire-types.js";

import type { ApiStatus, DispatchBundle, DispatchRequest } from "./wire-types.js";

export interface AgentControlPlaneClientOptions {
  baseUrl: string;
  apiKey?: string;
  fetchImpl?: typeof fetch;
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

  dashboard(): Promise<Record<string, unknown>> {
    return this.getJson<Record<string, unknown>>("/api/v1/dashboard");
  }

  dispatches(): Promise<Record<string, unknown>> {
    return this.getJson<Record<string, unknown>>("/api/v1/dispatches");
  }

  config(): Promise<Record<string, unknown>> {
    return this.getJson<Record<string, unknown>>("/api/v1/config");
  }

  team(): Promise<Record<string, unknown>> {
    return this.getJson<Record<string, unknown>>("/api/v1/team");
  }

  costs(): Promise<Record<string, unknown>> {
    return this.getJson<Record<string, unknown>>("/api/v1/costs");
  }

  exportState(): Promise<Record<string, unknown>> {
    return this.getJson<Record<string, unknown>>("/api/v1/export");
  }

  audit(): Promise<Record<string, unknown>> {
    return this.getJson<Record<string, unknown>>("/api/v1/audit");
  }

  providerHealth(): Promise<Record<string, unknown>> {
    return this.getJson<Record<string, unknown>>("/api/v1/provider/health");
  }

  providerAudit(): Promise<Record<string, unknown>> {
    return this.getJson<Record<string, unknown>>("/api/v1/provider/audit");
  }

  dispatch(request: DispatchRequest): Promise<DispatchBundle> {
    return this.postJson<DispatchBundle>("/api/v1/dispatch", {
      raw_request: request.raw_request,
      request_source: request.request_source,
    });
  }

  createBackup(request: { label?: string; confirmLocalBackup: boolean }): Promise<Record<string, unknown>> {
    return this.postJson<Record<string, unknown>>("/api/v1/backups", {
      label: request.label,
      confirm_local_backup: request.confirmLocalBackup,
    });
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
