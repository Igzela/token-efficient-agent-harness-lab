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
  const advisory = {
    schema_version: "plan_advisory.v1",
    mode: "recommendation_only",
    status: "recommendation_ready",
    blockers: [],
    recommendations: [],
    quality: {},
    routing: {},
    retry: {},
    observability: {},
    decision: { execution_allowed: false },
  };
  const { calls, fetchImpl } = captureFetch({
    schema_version: "axum_api.v1",
    plan: { plan_id: "plan-0001", advisory },
    plans: [{ plan_id: "plan-0001", advisory }],
  });
  const client = new AgentControlPlaneClient({ baseUrl: "http://127.0.0.1:8080", fetchImpl });

  const created = await client.createPlan({ raw_request: "Plan docs", request_source: "api" });
  const listed = await client.plans({ limit: 25, offset: 50, search: "docs plan" });
  await client.plan("plan/0001");

  assert.equal(created.plan.advisory.schema_version, "plan_advisory.v1");
  assert.equal(listed.plans[0].advisory.decision.execution_allowed, false);

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

test("supervised patch methods call read-only metadata endpoints", async () => {
  const { calls, fetchImpl } = captureFetch({
    schema_version: "axum_api.v1",
    metadata_only: true,
    execution_authority: "disabled",
    workspace: { workspace_id: "patch-workspace-0001" },
    workspaces: [],
    artifact: { artifact_id: "patch-artifact-0001" },
    artifacts: [],
  });
  const client = new AgentControlPlaneClient({ baseUrl: "http://127.0.0.1:8080", fetchImpl });

  const workspaces = await client.supervisedPatchWorkspaces({ limit: 25 });
  const workspace = await client.supervisedPatchWorkspaceDetail("workspace/0001");
  const artifacts = await client.supervisedPatchArtifacts({ limit: 10 });
  const artifact = await client.supervisedPatchArtifactDetail("artifact/0001");

  assert.equal(workspaces.metadata_only, true);
  assert.equal(workspace.execution_authority, "disabled");
  assert.equal(artifacts.metadata_only, true);
  assert.equal(artifact.execution_authority, "disabled");
  assert.deepEqual(
    calls.map((call) => call.url),
    [
      "http://127.0.0.1:8080/api/v1/supervised-patch/workspaces?limit=25",
      "http://127.0.0.1:8080/api/v1/supervised-patch/workspaces/workspace%2F0001",
      "http://127.0.0.1:8080/api/v1/supervised-patch/artifacts?limit=10",
      "http://127.0.0.1:8080/api/v1/supervised-patch/artifacts/artifact%2F0001",
    ],
  );
  assert(calls.every((call) => call.init.method === "GET"));
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

test("createSupervisedPatchWorkspace posts workspace creation request", async () => {
  const { calls, fetchImpl } = captureFetch({
    schema_version: "axum_api.v1",
    workspace: { workspace_id: "ws-0001", status: "workspace_created" },
  });
  const client = new AgentControlPlaneClient({ baseUrl: "http://127.0.0.1:8080", fetchImpl });

  const result = await client.createSupervisedPatchWorkspace({
    run_id: "run-0001",
    target_id: "target-a",
    target_repo_path: "/tmp/repo",
    source_revision: "abc123",
  });

  assert.equal(result.workspace.workspace_id, "ws-0001");
  assert.equal(calls[0].url, "http://127.0.0.1:8080/api/v1/supervised-patch/workspaces");
  assert.equal(calls[0].init.method, "POST");
  assert.deepEqual(JSON.parse(calls[0].init.body), {
    run_id: "run-0001",
    target_id: "target-a",
    target_repo_path: "/tmp/repo",
    source_revision: "abc123",
  });
});

test("createSupervisedPatchWorkspace includes optional fields", async () => {
  const { calls, fetchImpl } = captureFetch({
    schema_version: "axum_api.v1",
    workspace: { workspace_id: "ws-0002" },
  });
  const client = new AgentControlPlaneClient({ baseUrl: "http://127.0.0.1:8080", fetchImpl });

  await client.createSupervisedPatchWorkspace({
    run_id: "run-0001",
    target_id: "target-a",
    target_repo_path: "/tmp/repo",
    source_revision: "abc123",
    plan_id: "plan-0001",
    source_tree_hash: "sha256:deadbeef",
  });

  const body = JSON.parse(calls[0].init.body);
  assert.equal(body.plan_id, "plan-0001");
  assert.equal(body.source_tree_hash, "sha256:deadbeef");
});

test("cleanupSupervisedPatchWorkspace posts cleanup action", async () => {
  const { calls, fetchImpl } = captureFetch({
    schema_version: "axum_api.v1",
    workspace: { workspace_id: "ws-0001", status: "cleaned" },
  });
  const client = new AgentControlPlaneClient({ baseUrl: "http://127.0.0.1:8080", fetchImpl });

  const result = await client.cleanupSupervisedPatchWorkspace("ws/0001");

  assert.equal(result.workspace.status, "cleaned");
  assert.equal(calls[0].url, "http://127.0.0.1:8080/api/v1/supervised-patch/workspaces/ws%2F0001/cleanup");
  assert.equal(calls[0].init.method, "POST");
  assert.deepEqual(JSON.parse(calls[0].init.body), {});
});

test("quarantineSupervisedPatchWorkspace posts quarantine action", async () => {
  const { calls, fetchImpl } = captureFetch({
    schema_version: "axum_api.v1",
    workspace: { workspace_id: "ws-0001", status: "quarantined" },
  });
  const client = new AgentControlPlaneClient({ baseUrl: "http://127.0.0.1:8080", fetchImpl });

  const result = await client.quarantineSupervisedPatchWorkspace("ws-0001");

  assert.equal(result.workspace.status, "quarantined");
  assert.equal(calls[0].url, "http://127.0.0.1:8080/api/v1/supervised-patch/workspaces/ws-0001/quarantine");
  assert.equal(calls[0].init.method, "POST");
});

test("captureSupervisedPatch posts capture to workspace", async () => {
  const { calls, fetchImpl } = captureFetch({
    schema_version: "axum_api.v1",
    artifact: { artifact_id: "art-0001", artifact_type: "patch_diff" },
  });
  const client = new AgentControlPlaneClient({ baseUrl: "http://127.0.0.1:8080", fetchImpl });

  const result = await client.captureSupervisedPatch("ws-0001");

  assert.equal(result.artifact.artifact_id, "art-0001");
  assert.equal(calls[0].url, "http://127.0.0.1:8080/api/v1/supervised-patch/workspaces/ws-0001/capture");
  assert.equal(calls[0].init.method, "POST");
});

test("exportSupervisedPatchArtifact posts export with run_id", async () => {
  const { calls, fetchImpl } = captureFetch({
    schema_version: "axum_api.v1",
    export: { artifact_id: "art-0001", exported_by: "key-1", exported_at: "2026-06-05T00:00:00Z" },
  });
  const client = new AgentControlPlaneClient({ baseUrl: "http://127.0.0.1:8080", fetchImpl });

  const result = await client.exportSupervisedPatchArtifact("art/0001", { run_id: "run-0001" });

  assert.equal(result.export.artifact_id, "art-0001");
  assert.equal(calls[0].url, "http://127.0.0.1:8080/api/v1/supervised-patch/artifacts/art%2F0001/export");
  assert.equal(calls[0].init.method, "POST");
  assert.deepEqual(JSON.parse(calls[0].init.body), { run_id: "run-0001" });
});

test("tickWorkflowRun posts tick to workflow run", async () => {
  const { calls, fetchImpl } = captureFetch({
    schema_version: "axum_api.v1",
    tick: { node_id: "node-a", status: "completed" },
  });
  const client = new AgentControlPlaneClient({ baseUrl: "http://127.0.0.1:8080", fetchImpl });

  const result = await client.tickWorkflowRun("run/0001");

  assert.equal(result.tick.node_id, "node-a");
  assert.equal(calls[0].url, "http://127.0.0.1:8080/api/v1/workflow-runs/run%2F0001/tick");
  assert.equal(calls[0].init.method, "POST");
  assert.deepEqual(JSON.parse(calls[0].init.body), {});
});

test("tickWorkflowRun passes optional parameters", async () => {
  const { calls, fetchImpl } = captureFetch({
    schema_version: "axum_api.v1",
    tick: { status: "completed" },
  });
  const client = new AgentControlPlaneClient({ baseUrl: "http://127.0.0.1:8080", fetchImpl });

  await client.tickWorkflowRun("run-0001", {
    actor: "user-1",
    max_retries: 3,
    executor: "command",
    timeout_ms: 60000,
  });

  const body = JSON.parse(calls[0].init.body);
  assert.equal(body.actor, "user-1");
  assert.equal(body.max_retries, 3);
  assert.equal(body.executor, "command");
  assert.equal(body.timeout_ms, 60000);
});

test("tickWorkflowRun passes command override to executor", async () => {
  const { calls, fetchImpl } = captureFetch({
    schema_version: "axum_api.v1",
    tick: { status: "completed" },
  });
  const client = new AgentControlPlaneClient({ baseUrl: "http://127.0.0.1:8080", fetchImpl });

  await client.tickWorkflowRun("run-0001", {
    executor: "command",
    command: "echo hello",
    timeout_ms: 5000,
  });

  const body = JSON.parse(calls[0].init.body);
  assert.equal(body.executor, "command");
  assert.equal(body.command, "echo hello");
  assert.equal(body.timeout_ms, 5000);
});

test("schedulerStatus gets scheduler health", async () => {
  const { calls, fetchImpl } = captureFetch({
    schema_version: "axum_api.v1",
    scheduler: { enabled: true, running: true, interval_ms: 2000 },
  });
  const client = new AgentControlPlaneClient({ baseUrl: "http://localhost:8080", fetchImpl });
  const result = await client.schedulerStatus();
  assert.equal(result.scheduler.enabled, true);
  assert.equal(result.scheduler.interval_ms, 2000);
  assert.ok(calls[0].url.includes("/api/v1/scheduler/status"));
});

test("fetchExecutorPool sends GET to executor-pool endpoint", async () => {
  const { calls, fetchImpl } = captureFetch({
    schema_version: "executor_pool.v1",
    executors: [
      {
        executor_type: "mock",
        capabilities: {
          supported_task_types: ["generate", "review"],
          supported_task_domains: ["code", "docs"],
          requires_auth: false,
          requires_cli: false,
          max_timeout_ms: 30000,
        },
        available: true,
        active_count: 0,
        concurrency_limit: 4,
        cooldown_until: null,
        failure_score: 0.0,
        cost_per_execution_usd: null,
        daily_cost_usd: 0.0,
        daily_cost_limit_usd: null,
        total_executions: 42,
        success_rate: 1.0,
        avg_latency_ms: 150,
        last_executed_at: "2026-06-07T00:00:00Z",
      },
    ],
    total_active: 0,
    total_capacity: 4,
  });
  const client = new AgentControlPlaneClient({ baseUrl: "http://127.0.0.1:8080", fetchImpl });

  const result = await client.fetchExecutorPool();

  assert.equal(result.schema_version, "executor_pool.v1");
  assert.equal(result.executors.length, 1);
  assert.equal(result.executors[0].executor_type, "mock");
  assert.equal(result.executors[0].capabilities.requires_auth, false);
  assert.equal(result.executors[0].available, true);
  assert.equal(result.total_active, 0);
  assert.equal(result.total_capacity, 4);
  assert.equal(calls[0].url, "http://127.0.0.1:8080/api/v1/executor-pool");
  assert.equal(calls[0].init.method, "GET");
});
