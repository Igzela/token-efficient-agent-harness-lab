"use client";

import { useCallback, useEffect, useState } from "react";
import { fetchScorecards } from "@/lib/api-client";
import { summarizeScorecardComparison } from "@/lib/scorecard-evidence";
import type { ScorecardScenarioComparisonSummary } from "@/lib/types";
import { StateBanner } from "./StateBanner";

const DEFAULT_SCENARIO = "langgraph_offline_state_retention_pilot_2026_07_10";

function formatNumber(value: number | null): string {
  return value === null ? "—" : new Intl.NumberFormat("en-US", { maximumFractionDigits: 6 }).format(value);
}

function formatRatio(value: number | null): string {
  return value === null ? "—" : `${(value * 100).toFixed(4)}%`;
}

function formatCost(value: number | null): string {
  return value === null ? "—" : `$${value.toFixed(6)}`;
}

function comparisonRows(summary: ScorecardScenarioComparisonSummary) {
  return [
    ["Total tokens", formatNumber(summary.baseline.total_tokens), formatNumber(summary.candidate.total_tokens), formatNumber(summary.deltas.total_tokens)],
    ["Repeated context", formatRatio(summary.baseline.repeated_context_ratio), formatRatio(summary.candidate.repeated_context_ratio), formatRatio(summary.deltas.repeated_context_ratio)],
    ["Estimated cost", formatCost(summary.baseline.estimated_cost_usd), formatCost(summary.candidate.estimated_cost_usd), formatCost(summary.deltas.estimated_cost_usd)],
    ["Latency", `${formatNumber(summary.baseline.duration_ms)}ms`, `${formatNumber(summary.candidate.duration_ms)}ms`, `${formatNumber(summary.deltas.duration_ms)}ms`],
    ["Retries", formatNumber(summary.baseline.retry_count), formatNumber(summary.candidate.retry_count), formatNumber(summary.deltas.retry_count)],
    ["Quality", formatNumber(summary.baseline.quality_score), formatNumber(summary.candidate.quality_score), formatNumber(summary.deltas.quality_score)],
  ];
}

export function BenchmarkScorecards() {
  const [scenarioId, setScenarioId] = useState(DEFAULT_SCENARIO);
  const [comparison, setComparison] = useState<ScorecardScenarioComparisonSummary | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const loadScenario = useCallback(async (requestedScenario: string) => {
    const normalized = requestedScenario.trim();
    if (!normalized) {
      setError("Scenario ID is required.");
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const response = await fetchScorecards({ scenario_id: normalized, limit: 100 });
      const summary = summarizeScorecardComparison(response.comparison);
      if (!summary) throw new Error("The scenario does not contain one comparable baseline/candidate pair.");
      setComparison(summary);
    } catch (cause) {
      setComparison(null);
      setError(cause instanceof Error ? cause.message : "Failed to load benchmark scenario.");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void loadScenario(DEFAULT_SCENARIO);
  }, [loadScenario]);

  return (
    <section className="grid">
      <div className="card stack">
        <div className="heading-row">
          <div>
            <p className="eyebrow">Read-only scorecard evidence</p>
            <h2>Benchmark scenarios</h2>
          </div>
          <span className="pill info">summary-level only</span>
        </div>
        <p className="muted">Compare an explicit stateless baseline with a stateful candidate. No prompts, outputs, messages, checkpoints, spans, or tool payloads are exposed.</p>
        <form
          className="table-toolbar"
          onSubmit={(event) => {
            event.preventDefault();
            void loadScenario(scenarioId);
          }}
        >
          <label className="stack" style={{ flex: 1 }}>
            <span className="metric-label">Scenario ID</span>
            <input
              className="search-input"
              value={scenarioId}
              onChange={(event) => setScenarioId(event.target.value)}
              aria-label="Benchmark scenario ID"
            />
          </label>
          <button disabled={loading} type="submit">{loading ? "Loading…" : "Load scenario"}</button>
        </form>
        {error && <StateBanner title="Benchmark scenario unavailable" tone="risk"><p>{error}</p></StateBanner>}
      </div>

      {comparison && (
        <div className="card stack">
          <div className="heading-row">
            <div>
              <p className="eyebrow">{comparison.scenario_id}</p>
              <h3>{comparison.baseline.runtime_kind} {comparison.baseline.runtime_version}</h3>
            </div>
            <span className={`pill ${comparison.both_qualified ? "ok" : "risk"}`}>
              quality {comparison.both_qualified ? "qualified" : "not qualified"}
            </span>
          </div>
          <div className="detail-summary">
            <div className="summary-tile"><span className="metric-label">Baseline</span><strong>{comparison.baseline.mode}</strong></div>
            <div className="summary-tile"><span className="metric-label">Candidate</span><strong>{comparison.candidate.mode}</strong></div>
            <div className="summary-tile"><span className="metric-label">Token reduction</span><strong>{formatRatio(comparison.token_reduction_ratio)}</strong></div>
            <div className="summary-tile"><span className="metric-label">Quality threshold</span><strong>{formatNumber(comparison.quality_threshold)}</strong></div>
          </div>
          <div className="table-wrap">
            <table>
              <thead><tr><th scope="col">Metric</th><th scope="col">Baseline</th><th scope="col">Candidate</th><th scope="col">Delta</th></tr></thead>
              <tbody>
                {comparisonRows(comparison).map(([metric, baseline, candidate, delta]) => (
                  <tr key={metric}><th scope="row">{metric}</th><td>{baseline}</td><td>{candidate}</td><td>{delta}</td></tr>
                ))}
              </tbody>
            </table>
          </div>
          <div className="heading-row">
            <span className={`pill ${comparison.token_advantage_reported ? "ok" : "info"}`}>token advantage {comparison.token_advantage_reported ? "reported" : "not reported"}</span>
            <span className={`pill ${comparison.cost_advantage_reported ? "ok" : "info"}`}>cost advantage {comparison.cost_advantage_reported ? "reported" : "not reported"}</span>
          </div>
        </div>
      )}
    </section>
  );
}
