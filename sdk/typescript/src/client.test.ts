import { describe, it, expect } from "vitest";
import { AgentControlPlaneClient } from "./index.js";

function mockFetch(responseBody: unknown, status = 200) {
  return async (_url: string | URL | Request, _init?: RequestInit) => {
    return new Response(JSON.stringify(responseBody), {
      status,
      headers: { "content-type": "application/json" },
    });
  };
}

function mockFetchThatCaptures() {
  const captured: { url: string; init?: RequestInit }[] = [];
  const fetchImpl = async (url: string | URL | Request, init?: RequestInit) => {
    captured.push({ url: String(url), init });
    return new Response(JSON.stringify({ schema_version: "axum_api.v1", status: "healthy" }), {
      status: 200,
      headers: { "content-type": "application/json" },
    });
  };
  return { fetchImpl, captured };
}

describe("AgentControlPlaneClient", () => {
  it("health sends GET to /api/v1/health", async () => {
    const { fetchImpl, captured } = mockFetchThatCaptures();
    const client = new AgentControlPlaneClient({ baseUrl: "http://localhost:8080", fetchImpl });
    await client.health();
    expect(captured[0].url).toBe("http://localhost:8080/api/v1/health");
    expect(captured[0].init?.method).toBe("GET");
  });

  it("ready sends GET to /api/v1/ready", async () => {
    const { fetchImpl, captured } = mockFetchThatCaptures();
    const client = new AgentControlPlaneClient({ baseUrl: "http://localhost:8080", fetchImpl });
    await client.ready();
    expect(captured[0].url).toBe("http://localhost:8080/api/v1/ready");
  });

  it("openapi sends GET to /api/v1/openapi.json", async () => {
    const { fetchImpl, captured } = mockFetchThatCaptures();
    const client = new AgentControlPlaneClient({ baseUrl: "http://localhost:8080", fetchImpl });
    await client.openapi();
    expect(captured[0].url).toBe("http://localhost:8080/api/v1/openapi.json");
  });

  it("dispatch sends POST with correct body", async () => {
    const bundle = { record: {}, analysis: {}, decision: {}, execution_result: {}, evaluation_result: {} };
    const fetchImpl = mockFetch(bundle);
    const client = new AgentControlPlaneClient({ baseUrl: "http://localhost:8080", fetchImpl });
    const result = await client.dispatch({ raw_request: "test", request_source: "api" });
    expect(result).toEqual(bundle);
  });

  it("includes bearer token when apiKey is set", async () => {
    const { fetchImpl, captured } = mockFetchThatCaptures();
    const client = new AgentControlPlaneClient({ baseUrl: "http://localhost:8080", apiKey: "tok_abc", fetchImpl });
    await client.health();
    expect(captured[0].init?.headers).toEqual({ authorization: "Bearer tok_abc" });
  });

  it("omits auth header when no apiKey", async () => {
    const { fetchImpl, captured } = mockFetchThatCaptures();
    const client = new AgentControlPlaneClient({ baseUrl: "http://localhost:8080", fetchImpl });
    await client.health();
    expect(captured[0].init?.headers).toEqual({});
  });

  it("strips trailing slashes from baseUrl", async () => {
    const { fetchImpl, captured } = mockFetchThatCaptures();
    const client = new AgentControlPlaneClient({ baseUrl: "http://localhost:8080///", fetchImpl });
    await client.health();
    expect(captured[0].url).toBe("http://localhost:8080/api/v1/health");
  });

  it("throws on HTTP error", async () => {
    const fetchImpl = async () =>
      new Response(JSON.stringify({ error: "unauthorized" }), { status: 401 });
    const client = new AgentControlPlaneClient({ baseUrl: "http://localhost:8080", fetchImpl });
    await expect(client.health()).rejects.toThrow("unauthorized");
  });

  it("throws with status text when no error body", async () => {
    const fetchImpl = async () => new Response("{}", { status: 500 });
    const client = new AgentControlPlaneClient({ baseUrl: "http://localhost:8080", fetchImpl });
    await expect(client.health()).rejects.toThrow();
  });
});
