import { describe, expect, test } from "bun:test";
import { hasRawTraceLeak, summarizeScorecardArtifact } from "./scorecard-evidence";

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
