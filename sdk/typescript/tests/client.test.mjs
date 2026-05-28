import assert from "node:assert/strict";
import { test } from "node:test";

import { AgentControlPlaneClient } from "../dist/index.js";

function captureFetch(body = { schema_version: "axum_api.v1", status: "ok" }) {
  const calls = [];
  const fetchImpl = async (url, init) => {
    calls.push({ url: String(url), init });
    return new Response(JSON.stringify(body), {
      headers: { "content-type": "application/json" },
      status: 200,
    });
  };
  return { calls, fetchImpl };
}

test("health sends GET to health endpoint", async () => {
  const { calls, fetchImpl } = captureFetch({ schema_version: "axum_api.v1", status: "healthy" });
  const client = new AgentControlPlaneClient({ baseUrl: "http://127.0.0.1:8080", fetchImpl });

  const result = await client.health();

  assert.equal(result.status, "healthy");
  assert.equal(calls[0].url, "http://127.0.0.1:8080/api/v1/health");
  assert.equal(calls[0].init.method, "GET");
});

test("dispatch posts request body to dispatch endpoint", async () => {
  const bundle = {
    analysis: {},
    decision: {},
    evaluation_result: {},
    execution_result: {},
    record: { dispatch_id: "disp-1" },
  };
  const { calls, fetchImpl } = captureFetch(bundle);
  const client = new AgentControlPlaneClient({ baseUrl: "http://127.0.0.1:8080///", fetchImpl });

  const result = await client.dispatch({ raw_request: "Summarize docs", request_source: "api" });

  assert.equal(result.record.dispatch_id, "disp-1");
  assert.equal(calls[0].url, "http://127.0.0.1:8080/api/v1/dispatch");
  assert.equal(calls[0].init.method, "POST");
  assert.deepEqual(JSON.parse(calls[0].init.body), {
    raw_request: "Summarize docs",
    request_source: "api",
  });
});

test("api key becomes bearer authorization header", async () => {
  const { calls, fetchImpl } = captureFetch();
  const client = new AgentControlPlaneClient({
    apiKey: "local-test-key",
    baseUrl: "http://127.0.0.1:8080",
    fetchImpl,
  });

  await client.ready();

  assert.deepEqual(calls[0].init.headers, { authorization: ["Bearer", "local-test-key"].join(" ") });
});

test("HTTP error reports API error message", async () => {
  const fetchImpl = async () =>
    new Response(JSON.stringify({ error: "unauthorized" }), {
      headers: { "content-type": "application/json" },
      status: 401,
    });
  const client = new AgentControlPlaneClient({ baseUrl: "http://127.0.0.1:8080", fetchImpl });

  await assert.rejects(client.health(), /unauthorized/);
});
