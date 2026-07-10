import { describe, expect, test } from "bun:test";
import { hasRawTraceLeak, summarizeScorecardArtifact, summarizeScorecardComparison } from "./scorecard-evidence";

const baseArtifact = {
  artifact_id: "scorecard-run-1-abc123",
  created_at: "2026-07-06T00:00:00Z",
  read_only: true,
  scorecard: {
    status: "pass",
    quality_method: "test",
    redaction_status: "redacted",
    input_token_total: 10,
    output_token_total: 0,
    context_token_total: 0,
    tool_call_count: 0,
    redundant_tool_call_count: 0,
    retry_count: 0,
    step_count: 0,
    duration_ms: 0,
    estimated_cost_usd: 0,
    derived_metrics: {
      total_tokens: 10,
      repeated_context_ratio: 0,
    },
  },
};

describe("scorecard evidence summaries", () => {
  test("maps read-only scorecard metadata without requiring raw trace fields", () => {
    const summary = summarizeScorecardArtifact(baseArtifact);
    expect(summary.artifact_id).toBe("scorecard-run-1-abc123");
    expect(summary.status).toBe("pass");
    expect(summary.quality_method).toBe("test");
    expect(summary.total_tokens).toBe(10);
    expect(summary.output_tokens).toBe(0);
    expect(summary.context_tokens).toBe(0);
    expect(summary.repeated_context_ratio).toBe(0);
    expect(summary.estimated_cost_usd).toBe(0);
    expect(summary.read_only).toBe(true);
    expect(hasRawTraceLeak(baseArtifact)).toBe(false);
  });

  test("returns clear unknown/null values for no-scorecard or failed-run metadata", () => {
    const summary = summarizeScorecardArtifact({ artifact_id: "scorecard-failed", read_only: true, scorecard: { status: "fail" } });
    expect(summary.status).toBe("fail");
    expect(summary.quality_method).toBe("unknown");
    expect(summary.total_tokens).toBeNull();
    expect(summary.redaction_status).toBe("unknown");
  });

  test("flags raw prompt/output/transcript/private-path shaped data if it reaches the client model", () => {
    expect(hasRawTraceLeak({ scorecard: { raw_prompt: "do not display" } })).toBe(true);
    expect(hasRawTraceLeak({ scorecard: { transcript: "do not display" } })).toBe(true);
    expect(hasRawTraceLeak({ scorecard: { private_path: "/tmp/repo" } })).toBe(true);
  });
});

describe("scorecard scenario comparison summaries", () => {
  test("maps baseline candidate and bounded metric deltas", () => {
    const summary = summarizeScorecardComparison({
      scenario_id: "langgraph-pilot",
      baseline: {
        adapter_run_id: "stateless",
        runtime_kind: "langgraph",
        runtime_version: "1.2.9",
        mode: "stateless_reread",
        status: "pass",
        quality_score: 1,
        total_tokens: 38452,
        repeated_context_ratio: 0.714913,
        estimated_cost_usd: 0,
        duration_ms: 39,
        retry_count: 0,
      },
      candidate: {
        adapter_run_id: "stateful",
        runtime_kind: "langgraph",
        runtime_version: "1.2.9",
        mode: "stateful_store",
        status: "pass",
        quality_score: 1,
        total_tokens: 11294,
        repeated_context_ratio: 0.01589,
        estimated_cost_usd: 0,
        duration_ms: 4,
        retry_count: 0,
      },
      quality_gate: { threshold: 1, both_qualified: true },
      deltas: {
        total_tokens: -27158,
        repeated_context_ratio: -0.699023,
        estimated_cost_usd: 0,
        duration_ms: -35,
        retry_count: 0,
        quality_score: 0,
      },
      advantages: {
        token: { reported: true, reduction_ratio: 0.706283 },
        cost: { reported: false, reduction_usd: null },
      },
    });

    expect(summary?.scenario_id).toBe("langgraph-pilot");
    expect(summary?.baseline.total_tokens).toBe(38452);
    expect(summary?.candidate.runtime_kind).toBe("langgraph");
    expect(summary?.candidate.mode).toBe("stateful_store");
    expect(summary?.deltas.total_tokens).toBe(-27158);
    expect(summary?.deltas.repeated_context_ratio).toBe(-0.699023);
    expect(summary?.deltas.estimated_cost_usd).toBe(0);
    expect(summary?.deltas.duration_ms).toBe(-35);
    expect(summary?.deltas.retry_count).toBe(0);
    expect(summary?.deltas.quality_score).toBe(0);
    expect(summary?.token_advantage_reported).toBe(true);
    expect(summary?.cost_advantage_reported).toBe(false);
  });

  test("rejects incomplete comparison objects", () => {
    expect(summarizeScorecardComparison({ scenario_id: "missing-rows" })).toBeNull();
  });
});

import { fetchOperatorEvidence, fetchScorecards } from "./api-client";

describe("scorecard read-only API client", () => {
  test("fetchScorecards uses the read-only scorecard list endpoint", async () => {
    const calls: string[] = [];
    const originalFetch = globalThis.fetch;
    globalThis.fetch = (async (url: RequestInfo | URL) => {
      calls.push(String(url));
      return new Response(JSON.stringify({ metadata_only: true, read_only: true, target_repository_writes: "disabled", artifacts: [] }), { status: 200 });
    }) as typeof fetch;
    try {
      const result = await fetchScorecards({ dispatch_id: "dispatch-1", limit: 5 });
      expect(result.read_only).toBe(true);
      expect(result.artifacts).toEqual([]);
      expect(calls[0]).toBe("/api/v1/scorecards?dispatch_id=dispatch-1&limit=5");
    } finally {
      globalThis.fetch = originalFetch;
    }
  });

  test("fetchOperatorEvidence reads run-scoped operator evidence scorecards", async () => {
    const calls: string[] = [];
    const originalFetch = globalThis.fetch;
    globalThis.fetch = (async (url: RequestInfo | URL) => {
      calls.push(String(url));
      return new Response(JSON.stringify({ schema_version: "axum_api.v1", run_id: "run-1", scorecard_artifact_count: 0, scorecards: [] }), { status: 200 });
    }) as typeof fetch;
    try {
      const result = await fetchOperatorEvidence("run-1");
      expect(result.scorecard_artifact_count).toBe(0);
      expect(result.scorecards).toEqual([]);
      expect(calls[0]).toBe("/api/v1/operator/evidence/run-1");
    } finally {
      globalThis.fetch = originalFetch;
    }
  });
});
