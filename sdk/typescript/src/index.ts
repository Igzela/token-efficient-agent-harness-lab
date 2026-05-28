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

  dispatch(request: DispatchRequest): Promise<DispatchBundle> {
    return this.postJson<DispatchBundle>("/api/v1/dispatch", {
      raw_request: request.raw_request,
      request_source: request.request_source,
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
