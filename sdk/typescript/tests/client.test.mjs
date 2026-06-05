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

test("dashboard reads local dashboard endpoint", async () => {
  const { calls, fetchImpl } = captureFetch({ schema_version: "local_dashboard.v1", counts: { dispatches: 1 } });
  const client = new AgentControlPlaneClient({ baseUrl: "http://127.0.0.1:8080", fetchImpl });

  const result = await client.dashboard();

  assert.equal(result.schema_version, "local_dashboard.v1");
  assert.equal(calls[0].url, "http://127.0.0.1:8080/api/v1/dashboard");
  assert.equal(calls[0].init.method, "GET");
});

test("local state readers use product endpoints", async () => {
  const { calls, fetchImpl } = captureFetch({ ok: true });
  const client = new AgentControlPlaneClient({ baseUrl: "http://127.0.0.1:8080", fetchImpl });

  await client.dispatches();
  await client.config();
  await client.team();
  await client.costs();
  await client.exportState();
  await client.audit();

  assert.deepEqual(
    calls.map((call) => call.url),
    [
      "http://127.0.0.1:8080/api/v1/dispatches",
      "http://127.0.0.1:8080/api/v1/config",
      "http://127.0.0.1:8080/api/v1/team",
      "http://127.0.0.1:8080/api/v1/costs",
      "http://127.0.0.1:8080/api/v1/export",
      "http://127.0.0.1:8080/api/v1/audit",
    ],
  );
  assert(calls.every((call) => call.init.method === "GET"));
});

test("dispatches sends pagination and search query params", async () => {
  const { calls, fetchImpl } = captureFetch({ schema_version: "axum_api.v1", dispatches: [] });
  const client = new AgentControlPlaneClient({ baseUrl: "http://127.0.0.1:8080", fetchImpl });

  await client.dispatches({ limit: 25, offset: 50, search: "alpha parser&owner=bad" });

  assert.equal(
    calls[0].url,
    "http://127.0.0.1:8080/api/v1/dispatches?limit=25&offset=50&search=alpha+parser%26owner%3Dbad",
  );
  assert.equal(calls[0].init.method, "GET");
});

test("planner methods call read-only plan endpoints", async () => {
  const { calls, fetchImpl } = captureFetch({ schema_version: "axum_api.v1", plan: { plan_id: "plan-0001" }, plans: [] });
  const client = new AgentControlPlaneClient({ baseUrl: "http://127.0.0.1:8080", fetchImpl });

  await client.createPlan({ raw_request: "Plan docs", request_source: "api" });
  await client.plans({ limit: 25, offset: 50, search: "docs plan" });
  await client.plan("plan/0001");

  assert.equal(calls[0].url, "http://127.0.0.1:8080/api/v1/plans");
  assert.equal(calls[0].init.method, "POST");
  assert.deepEqual(JSON.parse(calls[0].init.body), {
    raw_request: "Plan docs",
    request_source: "api",
  });
  assert.equal(calls[1].url, "http://127.0.0.1:8080/api/v1/plans?limit=25&offset=50&search=docs+plan");
  assert.equal(calls[1].init.method, "GET");
  assert.equal(calls[2].url, "http://127.0.0.1:8080/api/v1/plans/plan%2F0001");
  assert.equal(calls[2].init.method, "GET");
});

test("workflow run methods call inert runtime state endpoints", async () => {
  const { calls, fetchImpl } = captureFetch({
    schema_version: "axum_api.v1",
    run: { run_id: "run-0001" },
    runs: [],
    event: { event_id: "workflow-event-0001" },
    events: [],
    approval: { approval_id: "workflow-approval-0001" },
    approvals: [],
  });
  const client = new AgentControlPlaneClient({ baseUrl: "http://127.0.0.1:8080", fetchImpl });

  await client.createWorkflowRun({ plan_id: "plan-0001" });
  await client.workflowRuns({ limit: 25, offset: 50, search: "run plan" });
  await client.workflowRun("run/0001");
  await client.recordWorkflowRunEvent("run/0001", {
    node_id: "node-a",
    event_type: "node_status_observed",
    details: { status: "ready" },
  });
  await client.workflowRunEvents("run/0001", { limit: 10 });
  await client.recordWorkflowRunApproval("run/0001", {
    node_id: "node-a",
    decision: "approved",
    reason: "metadata only",
  });
  await client.workflowRunApprovals("run/0001", { limit: 10 });
  await client.resumeWorkflowRun("run/0001", { reason: "metadata resume" });
  await client.cancelWorkflowRun("run/0001", { reason: "metadata cancel" });

  assert.equal(calls[0].url, "http://127.0.0.1:8080/api/v1/workflow-runs");
  assert.equal(calls[0].init.method, "POST");
  assert.deepEqual(JSON.parse(calls[0].init.body), { plan_id: "plan-0001" });
  assert.equal(calls[1].url, "http://127.0.0.1:8080/api/v1/workflow-runs?limit=25&offset=50&search=run+plan");
  assert.equal(calls[2].url, "http://127.0.0.1:8080/api/v1/workflow-runs/run%2F0001");
  assert.equal(calls[3].url, "http://127.0.0.1:8080/api/v1/workflow-runs/run%2F0001/events");
  assert.equal(calls[3].init.method, "POST");
  assert.deepEqual(JSON.parse(calls[3].init.body), {
    node_id: "node-a",
    event_type: "node_status_observed",
    details: { status: "ready" },
  });
  assert.equal(calls[4].url, "http://127.0.0.1:8080/api/v1/workflow-runs/run%2F0001/events?limit=10");
  assert.equal(calls[5].url, "http://127.0.0.1:8080/api/v1/workflow-runs/run%2F0001/approvals");
  assert.equal(calls[5].init.method, "POST");
  assert.deepEqual(JSON.parse(calls[5].init.body), {
    node_id: "node-a",
    decision: "approved",
    reason: "metadata only",
  });
  assert.equal(calls[6].url, "http://127.0.0.1:8080/api/v1/workflow-runs/run%2F0001/approvals?limit=10");
  assert.equal(calls[7].url, "http://127.0.0.1:8080/api/v1/workflow-runs/run%2F0001/resume");
  assert.deepEqual(JSON.parse(calls[7].init.body), { reason: "metadata resume" });
  assert.equal(calls[8].url, "http://127.0.0.1:8080/api/v1/workflow-runs/run%2F0001/cancel");
  assert.deepEqual(JSON.parse(calls[8].init.body), { reason: "metadata cancel" });
});

test("audit sends pagination query params", async () => {
  const { calls, fetchImpl } = captureFetch({ schema_version: "axum_api.v1", events: [] });
  const client = new AgentControlPlaneClient({ baseUrl: "http://127.0.0.1:8080", fetchImpl });

  await client.audit({ limit: 25, offset: 50, redact: true, search: "provider key" });

  assert.equal(
    calls[0].url,
    "http://127.0.0.1:8080/api/v1/audit?limit=25&offset=50&redact=true&search=provider+key",
  );
  assert.equal(calls[0].init.method, "GET");
});

test("metrics sends GET to operations metrics endpoint", async () => {
  const { calls, fetchImpl } = captureFetch({ schema_version: "axum_api.v1", dispatch_count: 0 });
  const client = new AgentControlPlaneClient({ baseUrl: "http://127.0.0.1:8080", fetchImpl });

  const result = await client.metrics();

  assert.equal(result.dispatch_count, 0);
  assert.equal(calls[0].url, "http://127.0.0.1:8080/api/v1/metrics");
  assert.equal(calls[0].init.method, "GET");
});

test("providerAudit sends pagination query params", async () => {
  const { calls, fetchImpl } = captureFetch({ schema_version: "axum_api.v1", events: [] });
  const client = new AgentControlPlaneClient({ baseUrl: "http://127.0.0.1:8080", fetchImpl });

  await client.providerAudit({ limit: 25, offset: 50 });

  assert.equal(calls[0].url, "http://127.0.0.1:8080/api/v1/provider/audit?limit=25&offset=50");
  assert.equal(calls[0].init.method, "GET");
});

test("costDetails sends limit query param", async () => {
  const { calls, fetchImpl } = captureFetch({ schema_version: "local_dispatch_cost_detail.v1", dispatches: [] });
  const client = new AgentControlPlaneClient({ baseUrl: "http://127.0.0.1:8080", fetchImpl });

  await client.costDetails({ limit: 25 });

  assert.equal(calls[0].url, "http://127.0.0.1:8080/api/v1/costs/dispatches?limit=25");
  assert.equal(calls[0].init.method, "GET");
});

test("backup creation posts explicit local confirmation", async () => {
  const { calls, fetchImpl } = captureFetch({ backup: { backup_id: "backup-0001" } });
  const client = new AgentControlPlaneClient({
    apiKey: "test",
    baseUrl: "http://127.0.0.1:8080",
    fetchImpl,
  });

  const result = await client.createBackup({ label: "manual", confirmLocalBackup: true });

  assert.equal(result.backup.backup_id, "backup-0001");
  assert.equal(calls[0].url, "http://127.0.0.1:8080/api/v1/backups");
  assert.equal(calls[0].init.method, "POST");
  assert.deepEqual(JSON.parse(calls[0].init.body), {
    confirm_local_backup: true,
    label: "manual",
  });
  assert.equal(calls[0].init.headers.authorization, ["Bearer", "test"].join(" "));
});

test("api key becomes auth header", async () => {
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

test("dispatchDetail sends GET with dispatchId", async () => {
  const { calls, fetchImpl } = captureFetch({ dispatch: { dispatch_id: "d-1" } });
  const client = new AgentControlPlaneClient({ baseUrl: "http://127.0.0.1:8080", fetchImpl });

  const result = await client.dispatchDetail("d-1");

  assert.equal(result.dispatch.dispatch_id, "d-1");
  assert.equal(calls[0].url, "http://127.0.0.1:8080/api/v1/dispatches/d-1");
  assert.equal(calls[0].init.method, "GET");
});

test("listBackups sends GET to backups endpoint", async () => {
  const { calls, fetchImpl } = captureFetch({ backups: [] });
  const client = new AgentControlPlaneClient({ baseUrl: "http://127.0.0.1:8080", fetchImpl });

  const result = await client.listBackups();

  assert.deepEqual(result.backups, []);
  assert.equal(calls[0].url, "http://127.0.0.1:8080/api/v1/backups");
  assert.equal(calls[0].init.method, "GET");
});

test("deleteBackup sends DELETE to backups/:backupId", async () => {
  const { calls, fetchImpl } = captureFetch({ ok: true, backup_id: "backup-0001" });
  const client = new AgentControlPlaneClient({ baseUrl: "http://127.0.0.1:8080", fetchImpl });

  const result = await client.deleteBackup("backup-0001");

  assert.equal(result.ok, true);
  assert.equal(calls[0].url, "http://127.0.0.1:8080/api/v1/backups/backup-0001");
  assert.equal(calls[0].init.method, "DELETE");
});

test("verifyBackup sends GET to backups/:id/verify", async () => {
  const { calls, fetchImpl } = captureFetch({ verification: { backup_id: "backup/0001", success: true } });
  const client = new AgentControlPlaneClient({ baseUrl: "http://127.0.0.1:8080", fetchImpl });

  const result = await client.verifyBackup("backup/0001");

  assert.equal(result.verification.success, true);
  assert.equal(calls[0].url, "http://127.0.0.1:8080/api/v1/backups/backup%2F0001/verify");
  assert.equal(calls[0].init.method, "GET");
});

test("restoreBackupDryRun posts explicit dry-run confirmation", async () => {
  const { calls, fetchImpl } = captureFetch({ restore_dry_run: { backup_id: "backup-0001", dry_run: true, success: true } });
  const client = new AgentControlPlaneClient({ baseUrl: "http://127.0.0.1:8080", fetchImpl });

  const result = await client.restoreBackupDryRun("backup-0001");

  assert.equal(result.restore_dry_run.dry_run, true);
  assert.equal(calls[0].url, "http://127.0.0.1:8080/api/v1/backups/backup-0001/restore/dry-run");
  assert.equal(calls[0].init.method, "POST");
  assert.deepEqual(JSON.parse(calls[0].init.body), { confirm_restore_dry_run: true });
});

test("storageIntegrity sends GET to storage/integrity", async () => {
  const { calls, fetchImpl } = captureFetch({ status: "ok" });
  const client = new AgentControlPlaneClient({ baseUrl: "http://127.0.0.1:8080", fetchImpl });

  const result = await client.storageIntegrity();

  assert.equal(result.status, "ok");
  assert.equal(calls[0].url, "http://127.0.0.1:8080/api/v1/storage/integrity");
  assert.equal(calls[0].init.method, "GET");
});

test("importSnapshot sends POST to import with confirm_import", async () => {
  const { calls, fetchImpl } = captureFetch({ imported: 5 });
  const client = new AgentControlPlaneClient({ baseUrl: "http://127.0.0.1:8080", fetchImpl });

  const result = await client.importSnapshot({ config: {}, team: [] });

  assert.equal(result.imported, 5);
  assert.equal(calls[0].url, "http://127.0.0.1:8080/api/v1/import");
  assert.equal(calls[0].init.method, "POST");
  assert.deepEqual(JSON.parse(calls[0].init.body), {
    snapshot: { config: {}, team: [] },
    confirm_import: true,
  });
});

test("restoreBackup sends POST to backups/:id/restore with confirm_restore", async () => {
  const { calls, fetchImpl } = captureFetch({ success: true });
  const client = new AgentControlPlaneClient({ baseUrl: "http://127.0.0.1:8080", fetchImpl });

  const result = await client.restoreBackup("backup-0001");

  assert.equal(result.success, true);
  assert.equal(calls[0].url, "http://127.0.0.1:8080/api/v1/backups/backup-0001/restore");
  assert.equal(calls[0].init.method, "POST");
  assert.deepEqual(JSON.parse(calls[0].init.body), { confirm_restore: true });
});

test("dispatch bundle with CLI execution status round-trips", async () => {
  const bundle = {
    record: { dispatch_id: "cli-1", final_status: "completed" },
    analysis: {},
    decision: {},
    execution_result: {
      executor_type: "claude_code_cli",
      status: "cli_completed",
      output: "done",
    },
    evaluation_result: { status: "pass" },
  };
  const { calls, fetchImpl } = captureFetch(bundle);
  const client = new AgentControlPlaneClient({ baseUrl: "http://127.0.0.1:8080", fetchImpl });

  const result = await client.dispatch({ raw_request: "refactor utils", request_source: "api" });

  assert.equal(result.execution_result.executor_type, "claude_code_cli");
  assert.equal(result.execution_result.status, "cli_completed");
  assert.equal(result.record.dispatch_id, "cli-1");
});

test("dispatch bundle with codex_cli executor type round-trips", async () => {
  const bundle = {
    record: { dispatch_id: "cli-2", final_status: "completed" },
    analysis: {},
    decision: {},
    execution_result: {
      executor_type: "codex_cli",
      status: "cli_completed",
      output: "done",
    },
    evaluation_result: { status: "pass" },
  };
  const { fetchImpl } = captureFetch(bundle);
  const client = new AgentControlPlaneClient({ baseUrl: "http://127.0.0.1:8080", fetchImpl });

  const result = await client.dispatch({ raw_request: "generate tests", request_source: "cli" });

  assert.equal(result.execution_result.executor_type, "codex_cli");
  assert.equal(result.execution_result.status, "cli_completed");
});

test("dispatch bundle with provider_completed status round-trips", async () => {
  const bundle = {
    record: { dispatch_id: "prov-1", final_status: "completed" },
    analysis: {},
    decision: {},
    execution_result: {
      executor_type: "provider",
      status: "provider_completed",
      output: "result",
    },
    evaluation_result: { status: "pass" },
  };
  const { fetchImpl } = captureFetch(bundle);
  const client = new AgentControlPlaneClient({ baseUrl: "http://127.0.0.1:8080", fetchImpl });

  const result = await client.dispatch({ raw_request: "summarize", request_source: "api" });

  assert.equal(result.execution_result.executor_type, "provider");
  assert.equal(result.execution_result.status, "provider_completed");
});
