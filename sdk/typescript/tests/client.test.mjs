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

test("adaptiveCompletion posts compact completion request", async () => {
  const response = {
    schema_version: "adaptive_completion.v1",
    output: "answer",
    usage: {
      input_tokens: 10,
      output_tokens: 5,
      estimated_cost_usd: 0.01,
      latency_ms: 25,
    },
  };
  const { calls, fetchImpl } = captureFetch(response);
  const client = new AgentControlPlaneClient({ baseUrl: "http://127.0.0.1:8080", fetchImpl });

  const result = await client.adaptiveCompletion({
    prompt: "Solve",
    objective: "quality",
    include_routing_metadata: false,
  });

  assert.equal(result.output, "answer");
  assert.equal(
    calls[0].url,
    "http://127.0.0.1:8080/api/v1/adaptive-fusion/completions",
  );
  assert.equal(calls[0].init.method, "POST");
  assert.deepEqual(JSON.parse(calls[0].init.body), {
    prompt: "Solve",
    objective: "quality",
    include_routing_metadata: false,
  });
});

test("provider endpoint config methods use safe config endpoint", async () => {
  const response = {
    schema_version: "axum_api.v1",
    source: "local_config",
    endpoints: [],
    runtime: {
      executor_configured: false,
      registry_configured: false,
      workflow_executor_configured: false,
      workflow_registry_configured: false,
      completion_executor_configured: true,
      completion_registry_configured: true,
      local_config_apply_requires_restart: false,
      local_config_applies_to_completion_api: true,
      local_config_error_code: null,
    },
    safety: {
      raw_secrets_allowed: false,
      credential_storage: "env_reference_only",
      supported_provider_types: ["stub", "openai_compatible", "anthropic"],
    },
  };
  const { calls, fetchImpl } = captureFetch(response);
  const client = new AgentControlPlaneClient({ baseUrl: "http://127.0.0.1:8080", fetchImpl });

  await client.providerEndpoints();
  await client.saveProviderEndpoints({
    confirm_provider_endpoint_config: true,
    endpoints: [{
      endpoint_id: "openai-quality",
      provider_type: "openai_compatible",
      base_url: "https://api.openai.example/v1",
      model: "quality-model",
      credential_env: "OPENAI_QUALITY_KEY",
      timeout_ms: 30000,
      input_cost_per_1k_usd: 0.01,
      output_cost_per_1k_usd: 0.03,
    }],
  });

  assert.equal(calls[0].url, "http://127.0.0.1:8080/api/v1/provider/endpoints");
  assert.equal(calls[0].init.method, "GET");
  assert.equal(calls[1].url, "http://127.0.0.1:8080/api/v1/provider/endpoints");
  assert.equal(calls[1].init.method, "PUT");
  assert.deepEqual(JSON.parse(calls[1].init.body), {
    confirm_provider_endpoint_config: true,
    endpoints: [{
      endpoint_id: "openai-quality",
      provider_type: "openai_compatible",
      base_url: "https://api.openai.example/v1",
      model: "quality-model",
      credential_env: "OPENAI_QUALITY_KEY",
      timeout_ms: 30000,
      input_cost_per_1k_usd: 0.01,
      output_cost_per_1k_usd: 0.03,
    }],
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

test("regression readers use bounded list detail and trend endpoints", async () => {
  const { calls, fetchImpl } = captureFetch({ read_only: true });
  const client = new AgentControlPlaneClient({ baseUrl: "http://127.0.0.1:8080", fetchImpl });

  await client.regressions({ scenario_id: "scenario one", limit: 25 });
  await client.regression("artifact/one");
  await client.regressionTrend("scenario/one", 10);

  assert.deepEqual(calls.map((call) => call.url), [
    "http://127.0.0.1:8080/api/v1/regressions?scenario_id=scenario+one&limit=25",
    "http://127.0.0.1:8080/api/v1/regressions/artifact%2Fone",
    "http://127.0.0.1:8080/api/v1/regressions/trends/scenario%2Fone?limit=10",
  ]);
  assert(calls.every((call) => call.init.method === "GET"));
});

test("budget evidence readers use bounded encoded read-only endpoints", async () => {
  const { calls, fetchImpl } = captureFetch({ read_only: true });
  const client = new AgentControlPlaneClient({ baseUrl: "http://127.0.0.1:8080", fetchImpl });

  await client.budgetEvidence({ kind: "anomaly", limit: 25, offset: 5 });
  await client.budgetEvidenceArtifact("budget/anomaly one");

  assert.equal(calls[0].url, "http://127.0.0.1:8080/api/v1/budget-evidence?kind=anomaly&limit=25&offset=5");
  assert.equal(calls[1].url, "http://127.0.0.1:8080/api/v1/budget-evidence/budget%2Fanomaly%20one");
});

test("memory and production evidence methods preserve bounded request contracts", async () => {
  const { calls, fetchImpl } = captureFetch({});
  const client = new AgentControlPlaneClient({ baseUrl: "http://127.0.0.1:8080", fetchImpl });
  const scope = { tenant_id: "local", workspace_id: "ws", agent_id: "agent-1" };
  await client.createMemory({ scope, run_id: "run-1", source_id: "source-1", source_sha256: "a".repeat(64), conflict_key: "fact-1", content: { text: "bounded" }, confidence: 0.9 });
  await client.memory("memory/one", "run-1");
  await client.reviseMemory("memory/one", { run_id: "run-1", scope, expected_version: 1, source_id: "source-2", source_sha256: "b".repeat(64), content: { text: "revised" }, confidence: 1 });
  await client.invalidateMemory("memory/one", { expected_version: 2, run_id: "run-1", scope });
  await client.forgetMemory("memory/one", { expected_version: 3, run_id: "run-1", scope });
  await client.supersedeMemory("memory/one", { run_id: "run-1", scope, winner_expected_version: 4, loser_memory_id: "memory/two", loser_expected_version: 2, confirm_supersede: true });
  await client.pruneMemories({ scope, run_id: "run-1", confirm_prune: true });
  await client.retrieveMemories({ scope, run_id: "run-1", node_id: "node-1", query: "bounded", top_k: 5, max_tokens: 100, max_bytes: 400, allow_lexical_fallback: true });
  await client.usageObservations("run/one", 20);
  await client.recomputeBudgetEvidence({ run_id: "run-1", confirm_recompute: true });
  await client.generateOfflineReplay({ replay: { dispatch_ids: ["dispatch-1"] }, confirm_generation: true });
  await client.replayProductionProfile();
  await client.configureReplayProductionProfile({ profile: { enabled: false }, confirm_profile: true });
  await client.promoteAdaptivePolicyWithEvidence({ replay_artifact_id: "replay-1", promotion: {}, canary: {}, rollout_scope: "local", rollback_target: "snapshot-1", confirm_promotion: true });

  assert.deepEqual(calls.map((call) => [call.init.method, call.url]), [
    ["POST", "http://127.0.0.1:8080/api/v1/memories"],
    ["GET", "http://127.0.0.1:8080/api/v1/memories/memory%2Fone?run_id=run-1"],
    ["POST", "http://127.0.0.1:8080/api/v1/memories/memory%2Fone/revise"],
    ["POST", "http://127.0.0.1:8080/api/v1/memories/memory%2Fone/invalidate"],
    ["POST", "http://127.0.0.1:8080/api/v1/memories/memory%2Fone/forget"],
    ["POST", "http://127.0.0.1:8080/api/v1/memories/memory%2Fone/supersede"],
    ["POST", "http://127.0.0.1:8080/api/v1/memories/prune"],
    ["POST", "http://127.0.0.1:8080/api/v1/memories/retrieve"],
    ["GET", "http://127.0.0.1:8080/api/v1/usage-observations?run_id=run%2Fone&limit=20"],
    ["POST", "http://127.0.0.1:8080/api/v1/budget-evidence/recompute"],
    ["POST", "http://127.0.0.1:8080/api/v1/offline-replays/generate"],
    ["GET", "http://127.0.0.1:8080/api/v1/offline-replays/production-profile"],
    ["PUT", "http://127.0.0.1:8080/api/v1/offline-replays/production-profile"],
    ["POST", "http://127.0.0.1:8080/api/v1/adaptive-fusion/policies/promote-with-evidence"],
  ]);
  assert.deepEqual(JSON.parse(calls[3].init.body), { expected_version: 2, run_id: "run-1", scope });
  assert.equal(JSON.parse(calls[5].init.body).confirm_supersede, true);
  assert.equal(JSON.parse(calls[6].init.body).confirm_prune, true);
  assert.equal(JSON.parse(calls[9].init.body).confirm_recompute, true);
  assert.equal(JSON.parse(calls[10].init.body).confirm_generation, true);
  assert.equal(JSON.parse(calls[12].init.body).confirm_profile, true);
  assert.equal(JSON.parse(calls[13].init.body).confirm_promotion, true);
});

test("operator decision reader uses the bounded read-only endpoint", async () => {
  const { calls, fetchImpl } = captureFetch({ read_only: true });
  const client = new AgentControlPlaneClient({ baseUrl: "http://127.0.0.1:8080", fetchImpl });
  await client.operatorDecisions({ generated_at: "2026-07-11T00:01:00Z", maximum_freshness_seconds: 300, limit: 25, offset: 5 });
  assert.equal(calls[0].url, "http://127.0.0.1:8080/api/v1/operator/decisions?generated_at=2026-07-11T00%3A01%3A00Z&maximum_freshness_seconds=300&limit=25&offset=5");
  assert.equal(calls[0].init.method, "GET");
});

test("operator decision action posts an explicitly confirmed hash-bound request", async () => {
  const { calls, fetchImpl } = captureFetch({ schema_version: "operator_decision_action_result.v1" });
  const client = new AgentControlPlaneClient({ baseUrl: "http://127.0.0.1:8080", fetchImpl });
  await client.applyOperatorDecision("decision/one", {
    queue_sha256: "a".repeat(64),
    generated_at: "2026-07-11T00:01:00Z",
    maximum_freshness_seconds: 300,
    limit: 25,
    offset: 5,
    action: "approve",
    confirm_action: true,
    reason: "reviewed",
  });
  assert.equal(calls[0].url, "http://127.0.0.1:8080/api/v1/operator/decisions/decision%2Fone/actions");
  assert.equal(calls[0].init.method, "POST");
  assert.deepEqual(JSON.parse(calls[0].init.body), {
    queue_sha256: "a".repeat(64),
    generated_at: "2026-07-11T00:01:00Z",
    maximum_freshness_seconds: 300,
    limit: 25,
    offset: 5,
    action: "approve",
    confirm_action: true,
    reason: "reviewed",
  });
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

test("createPlan sends typed bounded agent_steps", async () => {
  const { calls, fetchImpl } = captureFetch({
    schema_version: "axum_api.v1",
    plan: { plan_id: "plan-agent-0001" },
  });
  const client = new AgentControlPlaneClient({ baseUrl: "http://127.0.0.1:8080", fetchImpl });
  await client.createPlan({
    raw_request: "Review the bounded mailbox",
    request_source: "api",
    agent_steps: [{
      agent_id: "agent-1",
      role: "reviewer",
      capability_profile: ["mailbox", "review"],
      profile_id: "reviewer-profile",
      model: "fixture-model",
    }],
    confirm_agent_runtime_plan: true,
  });
  assert.deepEqual(JSON.parse(calls[0].init.body), {
    raw_request: "Review the bounded mailbox",
    request_source: "api",
    agent_steps: [{
      agent_id: "agent-1",
      role: "reviewer",
      capability_profile: ["mailbox", "review"],
      profile_id: "reviewer-profile",
      model: "fixture-model",
    }],
    confirm_agent_runtime_plan: true,
  });
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

test("dispatchMetrics sends limit query param", async () => {
  const { calls, fetchImpl } = captureFetch({ schema_version: "axum_api.v1", metrics: [] });
  const client = new AgentControlPlaneClient({ baseUrl: "http://127.0.0.1:8080", fetchImpl });

  await client.dispatchMetrics({ limit: 30 });

  assert.equal(calls[0].url, "http://127.0.0.1:8080/api/v1/dispatch-metrics?limit=30");
  assert.equal(calls[0].init.method, "GET");
});

test("feedback readers send filters to feedback endpoints", async () => {
  const { calls, fetchImpl } = captureFetch({ schema_version: "axum_api.v1", traces: [], rows: [] });
  const client = new AgentControlPlaneClient({ baseUrl: "http://127.0.0.1:8080", fetchImpl });

  await client.feedbackTraces({
    limit: 25,
    offset: 50,
    task_class: "docs cleanup",
    tier: "standard",
    status: "passed",
  });
  await client.feedbackCostOfPass({
    task_class: "docs cleanup",
    tier: "standard",
  });

  assert.equal(
    calls[0].url,
    "http://127.0.0.1:8080/api/v1/feedback/traces?limit=25&offset=50&task_class=docs+cleanup&tier=standard&status=passed",
  );
  assert.equal(calls[0].init.method, "GET");
  assert.equal(
    calls[1].url,
    "http://127.0.0.1:8080/api/v1/feedback/cost-of-pass?task_class=docs+cleanup&tier=standard",
  );
  assert.equal(calls[1].init.method, "GET");
});

test("simulationReport sends limit query param", async () => {
  const { calls, fetchImpl } = captureFetch({ schema_version: "axum_api.v1", report: [] });
  const client = new AgentControlPlaneClient({ baseUrl: "http://127.0.0.1:8080", fetchImpl });

  await client.simulationReport({ limit: 12 });

  assert.equal(calls[0].url, "http://127.0.0.1:8080/api/v1/simulation/report?limit=12");
  assert.equal(calls[0].init.method, "GET");
});

test("offline replay readers send filters and encode artifact ids", async () => {
  const { calls, fetchImpl } = captureFetch({
    schema_version: "offline_replay_read.v1",
    artifacts: [],
  });
  const client = new AgentControlPlaneClient({ baseUrl: "http://127.0.0.1:8080", fetchImpl });

  await client.offlineReplayArtifacts({
    status: "insufficient evidence",
    limit: 25,
    offset: 5,
  });
  await client.offlineReplayArtifact("offline/replay one");

  assert.equal(
    calls[0].url,
    "http://127.0.0.1:8080/api/v1/offline-replays?status=insufficient+evidence&limit=25&offset=5",
  );
  assert.equal(
    calls[1].url,
    "http://127.0.0.1:8080/api/v1/offline-replays/offline%2Freplay%20one",
  );
  assert.equal(calls[0].init.method, "GET");
  assert.equal(calls[1].init.method, "GET");
});

test("proposal methods call controlled-loop endpoints", async () => {
  const { calls, fetchImpl } = captureFetch({
    schema_version: "axum_api.v1",
    proposal: { proposal_id: "proposal-0001", status: "pending" },
    proposals: [],
  });
  const client = new AgentControlPlaneClient({ baseUrl: "http://127.0.0.1:8080", fetchImpl });

  await client.proposals({ limit: 20, offset: 40, status: "pending" });
  await client.createProposal({
    title: "Tune docs routing",
    summary: "Use standard tier for docs cleanup",
    task_class: "docs cleanup",
    tier: "standard",
    payload: { selected_tier: "standard" },
    evidence: { samples: 10 },
  });
  await client.proposal("proposal/0001");
  await client.approveProposal("proposal/0001", { actor: "human", reason: "reviewed" });
  await client.rejectProposal("proposal/0001", { reason: "insufficient evidence" });
  await client.rollbackProposal("proposal/0001", { reason: "regression" });
  await client.deactivateProposal("proposal/0001", { reason: "superseded" });

  assert.equal(calls[0].url, "http://127.0.0.1:8080/api/v1/proposals?limit=20&offset=40&status=pending");
  assert.equal(calls[0].init.method, "GET");
  assert.equal(calls[1].url, "http://127.0.0.1:8080/api/v1/proposals");
  assert.equal(calls[1].init.method, "POST");
  assert.deepEqual(JSON.parse(calls[1].init.body), {
    title: "Tune docs routing",
    summary: "Use standard tier for docs cleanup",
    task_class: "docs cleanup",
    tier: "standard",
    payload: { selected_tier: "standard" },
    evidence: { samples: 10 },
  });
  assert.equal(calls[2].url, "http://127.0.0.1:8080/api/v1/proposals/proposal%2F0001");
  assert.equal(calls[2].init.method, "GET");
  assert.equal(calls[3].url, "http://127.0.0.1:8080/api/v1/proposals/proposal%2F0001/approve");
  assert.deepEqual(JSON.parse(calls[3].init.body), { actor: "human", reason: "reviewed", confirm_policy_override: true });
  assert.equal(calls[4].url, "http://127.0.0.1:8080/api/v1/proposals/proposal%2F0001/reject");
  assert.deepEqual(JSON.parse(calls[4].init.body), { reason: "insufficient evidence" });
  assert.equal(calls[5].url, "http://127.0.0.1:8080/api/v1/proposals/proposal%2F0001/rollback");
  assert.deepEqual(JSON.parse(calls[5].init.body), { reason: "regression", confirm_policy_override: true });
  assert.equal(calls[6].url, "http://127.0.0.1:8080/api/v1/proposals/proposal%2F0001/deactivate");
  assert.deepEqual(JSON.parse(calls[6].init.body), { reason: "superseded", confirm_policy_override: true });
});

test("adaptive fusion policy reads and rollback use guarded policy endpoints", async () => {
  const { calls, fetchImpl } = captureFetch({
    schema_version: "axum_api.v1",
    policies: [],
    snapshots: [],
    live_execution_authority: false,
    requires_explicit_adaptive_plan: true,
  });
  const client = new AgentControlPlaneClient({ baseUrl: "http://127.0.0.1:8080", fetchImpl });

  await client.adaptiveFusionPolicies();
  await client.rollbackAdaptiveFusionPolicy("adaptive-policy/0001", {
    actor: "operator",
    reason: "regression",
  });

  assert.equal(calls[0].url, "http://127.0.0.1:8080/api/v1/adaptive-fusion/policies");
  assert.equal(calls[0].init.method, "GET");
  assert.equal(
    calls[1].url,
    "http://127.0.0.1:8080/api/v1/adaptive-fusion/policies/adaptive-policy%2F0001/rollback",
  );
  assert.equal(calls[1].init.method, "POST");
  assert.deepEqual(JSON.parse(calls[1].init.body), {
    actor: "operator",
    reason: "regression",
    confirm_adaptive_policy_rollback: true,
  });
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
    workspace_mode: "git_worktree",
  });

  const body = JSON.parse(calls[0].init.body);
  assert.equal(body.plan_id, "plan-0001");
  assert.equal(body.source_tree_hash, "sha256:deadbeef");
  assert.equal(body.workspace_mode, "git_worktree");
});

test("targetRepoOutput posts approval-bound target output request", async () => {
  const { calls, fetchImpl } = captureFetch({
    schema_version: "axum_api.v1",
    output: {
      schema_version: "target_repo_output.v1",
      branch_name: "acp/art-0001",
      patch_hash: "sha256:abc",
    },
  });
  const client = new AgentControlPlaneClient({ baseUrl: "http://127.0.0.1:8080", fetchImpl });

  const result = await client.targetRepoOutput("art/0001", {
    run_id: "run-0001",
    mode: "push_branch",
    confirm_target_output: true,
    branch_name: "acp/art-0001",
    remote: "origin",
    commit_message: "feat: apply artifact",
    pr_title: "Apply artifact",
    create_pull_request: true,
  });

  assert.equal(result.output.patch_hash, "sha256:abc");
  assert.equal(
    calls[0].url,
    "http://127.0.0.1:8080/api/v1/supervised-patch/artifacts/art%2F0001/output",
  );
  assert.equal(calls[0].init.method, "POST");
  assert.deepEqual(JSON.parse(calls[0].init.body), {
    run_id: "run-0001",
    mode: "push_branch",
    confirm_target_output: true,
    branch_name: "acp/art-0001",
    remote: "origin",
    commit_message: "feat: apply artifact",
    pr_title: "Apply artifact",
    create_pull_request: true,
  });
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

test("verifySupervisedPatchWorkspace posts verification request", async () => {
  const { calls, fetchImpl } = captureFetch({
    schema_version: "axum_api.v1",
    verification: { status: "evidence_recorded", command: ["cargo", "test"] },
  });
  const client = new AgentControlPlaneClient({ baseUrl: "http://127.0.0.1:8080", fetchImpl });

  const result = await client.verifySupervisedPatchWorkspace("ws/0001", {
    command: "cargo test",
    confirm_verification: true,
    timeout_ms: 600000,
    repair_executor: "codex_cli",
    max_repair_attempts: 2,
  });

  assert.equal(result.verification.status, "evidence_recorded");
  assert.equal(calls[0].url, "http://127.0.0.1:8080/api/v1/supervised-patch/workspaces/ws%2F0001/verify");
  assert.equal(calls[0].init.method, "POST");
  assert.deepEqual(JSON.parse(calls[0].init.body), {
    command: "cargo test",
    confirm_verification: true,
    timeout_ms: 600000,
    repair_executor: "codex_cli",
    max_repair_attempts: 2,
  });
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

test("controlScheduler posts confirmed worker action", async () => {
  const { calls, fetchImpl } = captureFetch({
    schema_version: "axum_api.v1",
    scheduler: { running: true, paused: true, kill_requested: false },
  });
  const client = new AgentControlPlaneClient({ baseUrl: "http://localhost:8080", fetchImpl });
  const result = await client.controlScheduler("pause", "operator");
  assert.equal(result.scheduler.paused, true);
  assert.ok(calls[0].url.includes("/api/v1/scheduler/control"));
  assert.equal(calls[0].init.method, "POST");
  assert.deepEqual(JSON.parse(calls[0].init.body), {
    action: "pause",
    actor: "operator",
    confirm_control: true,
  });
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

test("tool policy methods preserve confirmation and current hash", async () => {
  const { calls, fetchImpl } = captureFetch({
    schema_version: "axum_api.v1",
    resource: {
      schema_version: "tool_policy_resource.v1",
      resource_kind: "allowlist",
      resource_id: "review/profile",
      resource_sha256: "a".repeat(64),
      changed: true,
      value: { profile_id: "review/profile", tool_names: ["echo"] },
    },
  });
  const client = new AgentControlPlaneClient({ baseUrl: "http://localhost:8080", fetchImpl });

  await client.configureToolAllowlistPolicy("review/profile", {
    tool_names: ["echo"],
    expected_current_sha256: "b".repeat(64),
    confirm_tool_policy: true,
  });

  assert.equal(
    calls[0].url,
    "http://localhost:8080/api/v1/tool-policy/profiles/review%2Fprofile/allowlist",
  );
  assert.equal(calls[0].init.method, "PUT");
  assert.deepEqual(JSON.parse(calls[0].init.body), {
    tool_names: ["echo"],
    expected_current_sha256: "b".repeat(64),
    confirm_tool_policy: true,
  });
});
