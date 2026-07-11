import { describe, expect, test } from "bun:test";
import {
  fetchRegressionArtifacts,
  fetchRegressionTrend,
  summarizeRegressionArtifact,
  summarizeRegressionTrend,
  type RegressionOutcome,
} from "./regression-evidence";

const digest = "a".repeat(64);

function artifact(outcome: RegressionOutcome, reasonCodes: string[] = []) {
  const evidence = {
    current: {
      adapter_run_id: "current-run",
      artifact_schema_version: "scorecard_artifact.v2",
      content_sha256: digest,
    },
    baseline: outcome === "missing_baseline" ? null : {
      adapter_run_id: "baseline-run",
      artifact_schema_version: "scorecard_artifact.v2",
      content_sha256: "b".repeat(64),
    },
    best_known: outcome === "missing_baseline" || outcome === "missing_best_known" ? null : {
      adapter_run_id: "best-run",
      artifact_schema_version: "scorecard_artifact.v2",
      content_sha256: "c".repeat(64),
    },
  };
  return {
    schema_version: "token_efficiency_regression_artifact.v1",
    artifact_id: `regression-report-${outcome}`,
    artifact_kind: "token_efficiency_regression_report",
    report_schema_version: "token_efficiency_regression_report.v1",
    content_sha256: digest,
    registry_id: "pe1-registry",
    registry_sha256: digest,
    scenario_id: "scenario/encoded",
    created_at: "2026-07-11T00:00:00Z",
    read_only: true,
    metadata_only: true,
    report: {
      schema_version: "token_efficiency_regression_report.v1",
      scenario_id: "scenario/encoded",
      report_sha256: digest,
      outcome,
      reason_codes: reasonCodes,
      evidence,
      comparisons: outcome === "pass" || outcome === "regression" ? {
        baseline: {
          metrics: {
            total_tokens: {
              current: 120,
              reference: 100,
              delta: 20,
              normalized_regression: 0.2,
              allowed_regression: 0.1,
              regressed: outcome === "regression",
            },
          },
        },
        best_known: {
          metrics: {
            quality_score: {
              current: 1,
              reference: 1,
              delta: 0,
              normalized_regression: 0,
              allowed_regression: 0,
              regressed: false,
            },
          },
        },
      } : {},
      raw_prompt: "must not be surfaced",
    },
  };
}

describe("PE-1 regression report summaries", () => {
  test("preserves all bounded outcome states without converting failures into passes", () => {
    const outcomes: RegressionOutcome[] = [
      "pass",
      "regression",
      "missing_baseline",
      "missing_best_known",
      "incomparable",
      "quality_failure",
    ];
    for (const outcome of outcomes) {
      const summary = summarizeRegressionArtifact(artifact(outcome, [`reason.${outcome}`]));
      expect(summary?.outcome).toBe(outcome);
      expect(summary?.reason_codes).toEqual([`reason.${outcome}`]);
      expect(JSON.stringify(summary)).not.toContain("must not be surfaced");
    }
  });

  test("shows baseline and best-known configuration only when present", () => {
    expect(summarizeRegressionArtifact(artifact("missing_baseline"))?.evidence.baseline).toBeUndefined();
    expect(summarizeRegressionArtifact(artifact("missing_best_known"))?.evidence.best_known).toBeUndefined();
    const complete = summarizeRegressionArtifact(artifact("regression", ["baseline.total_tokens"]));
    expect(complete?.evidence.baseline?.adapter_run_id).toBe("baseline-run");
    expect(complete?.evidence.best_known?.adapter_run_id).toBe("best-run");
    expect(complete?.comparisons.baseline?.total_tokens.regressed).toBe(true);
  });
});

describe("PE-1 regression trend summaries", () => {
  test("handles empty and one-point histories deterministically", () => {
    const empty = summarizeRegressionTrend({
      scenario_id: "empty",
      trend_sha256: digest,
      points: [],
      transitions: [],
    });
    expect(empty?.point_count).toBe(0);
    expect(empty?.transitions).toEqual([]);

    const one = summarizeRegressionTrend({
      scenario_id: "one",
      trend_sha256: digest,
      points: [{
        artifact_id: "report-1",
        created_at: "2026-07-11T00:00:00Z",
        outcome: "pass",
        reason_codes: [],
        report_sha256: digest,
        evidence: artifact("pass").report.evidence,
        current_metrics: { total_tokens: 100, quality_score: 1 },
      }],
      transitions: [],
    });
    expect(one?.point_count).toBe(1);
    expect(one?.points[0].current_metrics.total_tokens).toBe(100);
  });

  test("maps explicit transition directions and reason changes", () => {
    const trend = summarizeRegressionTrend({
      scenario_id: "history",
      trend_sha256: digest,
      points: [
        { artifact_id: "from", created_at: "2026-07-10T00:00:00Z", outcome: "pass", reason_codes: [], report_sha256: digest, current_metrics: { total_tokens: 100 }, evidence: {} },
        { artifact_id: "to", created_at: "2026-07-11T00:00:00Z", outcome: "regression", reason_codes: ["baseline.total_tokens"], report_sha256: digest, current_metrics: { total_tokens: 120 }, evidence: {} },
      ],
      transitions: [{
        from_artifact_id: "from",
        to_artifact_id: "to",
        from_outcome: "pass",
        to_outcome: "regression",
        outcome_changed: true,
        new_reason_codes: ["baseline.total_tokens"],
        resolved_reason_codes: [],
        metric_deltas: { total_tokens: { delta: 20, direction: "regressed" } },
      }],
    });
    expect(trend?.transitions[0].outcome_changed).toBe(true);
    expect(trend?.transitions[0].metric_deltas.total_tokens.direction).toBe("regressed");
  });
});

describe("PE-1 regression read-only client", () => {
  test("uses bounded list and encoded trend paths", async () => {
    const calls: string[] = [];
    const originalFetch = globalThis.fetch;
    globalThis.fetch = (async (url: RequestInfo | URL) => {
      calls.push(String(url));
      if (String(url).includes("/trends/")) {
        return new Response(JSON.stringify({ metadata_only: true, read_only: true, report_only: true, provider_calls: "disabled", mutation_authority: "none", target_repository_writes: "disabled", trend: { scenario_id: "scenario/encoded", trend_sha256: digest, points: [], transitions: [] } }), { status: 200 });
      }
      return new Response(JSON.stringify({ metadata_only: true, read_only: true, report_only: true, provider_calls: "disabled", mutation_authority: "none", target_repository_writes: "disabled", artifacts: [] }), { status: 200 });
    }) as typeof fetch;
    try {
      const list = await fetchRegressionArtifacts({ scenario_id: "scenario/encoded", limit: 100 });
      const trend = await fetchRegressionTrend("scenario/encoded", { limit: 100 });
      expect(list.artifacts).toEqual([]);
      expect(trend.trend).toBeDefined();
      expect(calls[0]).toBe("/api/v1/regressions?scenario_id=scenario%2Fencoded&limit=100");
      expect(calls[1]).toBe("/api/v1/regressions/trends/scenario%2Fencoded?limit=100");
    } finally {
      globalThis.fetch = originalFetch;
    }
  });
});
