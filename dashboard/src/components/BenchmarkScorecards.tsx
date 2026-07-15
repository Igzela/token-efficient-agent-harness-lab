"use client";

import { useCallback, useEffect, useState } from "react";
import { fetchBudgetEvidence, fetchScorecards } from "@/lib/api-client";
import {
  fetchRegressionArtifacts,
  fetchRegressionTrend,
  summarizeRegressionArtifact,
  summarizeRegressionTrend,
  type RegressionEvidenceReference,
  type RegressionOutcome,
  type RegressionReportSummary,
  type RegressionTrendSummary,
} from "@/lib/regression-evidence";
import { summarizeScorecardComparison, summarizeScorecardMatrix } from "@/lib/scorecard-evidence";
import type { BudgetEvidenceArtifact, ScorecardMatrixSummary, ScorecardScenarioComparisonSummary } from "@/lib/types";
import { StateBanner } from "./StateBanner";

const DEFAULT_SCENARIO = "langgraph_offline_state_retention_pilot_2026_07_10";

function formatNumber(value: number | null | undefined): string {
  return value === null || value === undefined
    ? "—"
    : new Intl.NumberFormat("en-US", { maximumFractionDigits: 6 }).format(value);
}

function formatRatio(value: number | null | undefined): string {
  return value === null || value === undefined ? "—" : `${(value * 100).toFixed(4)}%`;
}

function formatCost(value: number | null | undefined): string {
  return value === null || value === undefined ? "—" : `$${value.toFixed(6)}`;
}

function formatTimestamp(value: string): string {
  const parsed = new Date(value);
  return Number.isNaN(parsed.getTime()) ? value : parsed.toLocaleString();
}

function compactHash(value: string): string {
  return value.length > 16 ? `${value.slice(0, 8)}…${value.slice(-8)}` : value;
}

function BudgetEvidenceLatest({ artifact }: { artifact: BudgetEvidenceArtifact }) {
  const outcome = typeof artifact.evidence.outcome === "string" ? artifact.evidence.outcome : "invalid";
  const reasons = Array.isArray(artifact.evidence.reason_codes)
    ? artifact.evidence.reason_codes.filter((value): value is string => typeof value === "string")
    : [];
  const references = Array.isArray(artifact.evidence.evidence_references)
    ? artifact.evidence.evidence_references.flatMap((value) => {
      if (!value || typeof value !== "object") return [];
      const reference = value as Record<string, unknown>;
      return typeof reference.evidence_type === "string" && typeof reference.evidence_id === "string"
        ? [`${reference.evidence_type}:${reference.evidence_id}`]
        : [];
    }).slice(0, 5)
    : [];
  const tone = outcome === "supported" ? "ok" : outcome === "insufficient_evidence" ? "warn" : "risk";
  const title = artifact.artifact_kind === "forecast" ? "Latest budget forecast" : "Latest anomaly finding";
  return <div className="card stack">
    <div className="heading-row"><div><p className="eyebrow">Read-only budget evidence</p><h3>{title}</h3></div><span className={`pill ${tone}`}>{outcome.replaceAll("_", " ")}</span></div>
    <p className="muted">{reasons.length ? reasons.join(", ") : "No bounded reason code was returned."}</p>
    <div className="detail-summary"><div className="summary-tile"><span className="metric-label">Evidence</span><strong>{artifact.evidence_id}</strong></div><div className="summary-tile"><span className="metric-label">Hash</span><strong>{compactHash(artifact.evidence_sha256)}</strong></div><div className="summary-tile"><span className="metric-label">References</span><strong>{references.length}</strong><span className="muted">{references.length ? references.join(", ") : "none"}</span></div></div>
  </div>;
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

function outcomeCopy(outcome: RegressionOutcome): { label: string; tone: "info" | "ok" | "warn" | "risk"; detail: string } {
  switch (outcome) {
    case "pass":
      return { label: "pass", tone: "ok", detail: "Current bounded evidence is within the configured regression limits." };
    case "regression":
      return { label: "regression", tone: "risk", detail: "One or more metrics exceed the configured allowed regression." };
    case "missing_baseline":
      return { label: "missing baseline", tone: "warn", detail: "No baseline evidence is available; the report does not claim a pass." };
    case "missing_best_known":
      return { label: "missing best-known", tone: "warn", detail: "No best-known evidence is available; the report does not claim a pass." };
    case "incomparable":
      return { label: "incomparable", tone: "warn", detail: "The evidence contracts or reference quality are not comparable." };
    case "quality_failure":
      return { label: "quality failure", tone: "risk", detail: "Current evidence did not meet the configured quality threshold." };
  }
}

function metricValue(name: string, value: number | undefined): string {
  if (value === undefined) return "—";
  if (name === "repeated_context_ratio") return formatRatio(value);
  if (name === "estimated_cost_usd") return formatCost(value);
  if (name === "duration_ms") return `${formatNumber(value)}ms`;
  return formatNumber(value);
}

function EvidenceReference({ role, value }: { role: string; value?: RegressionEvidenceReference }) {
  return (
    <div className="summary-tile">
      <span className="metric-label">{role}</span>
      {value ? (
        <>
          <strong>{value.adapter_run_id}</strong>
          <span className="muted">{value.artifact_schema_version}</span>
          <code title={value.content_sha256}>{compactHash(value.content_sha256)}</code>
        </>
      ) : (
        <strong>not available</strong>
      )}
    </div>
  );
}

function RegressionLatest({ report }: { report: RegressionReportSummary }) {
  const state = outcomeCopy(report.outcome);
  const baselineMetrics = report.comparisons.baseline ?? {};
  const bestKnownMetrics = report.comparisons.best_known ?? {};
  const metricNames = Array.from(new Set([...Object.keys(baselineMetrics), ...Object.keys(bestKnownMetrics)])).sort();
  return (
    <div className="card stack">
      <div className="heading-row">
        <div>
          <p className="eyebrow">Latest regression report</p>
          <h3>{report.scenario_id}</h3>
        </div>
        <span className={`pill ${state.tone === "ok" ? "ok" : state.tone === "risk" ? "risk" : "info"}`}>{state.label}</span>
      </div>
      <StateBanner title={state.label} tone={state.tone}>
        <p>{state.detail}</p>
        {report.reason_codes.length > 0 && <p><strong>Reason codes:</strong> {report.reason_codes.join(", ")}</p>}
      </StateBanner>
      <div className="detail-summary">
        <EvidenceReference role="Current" value={report.evidence.current} />
        <EvidenceReference role="Baseline" value={report.evidence.baseline} />
        <EvidenceReference role="Best-known" value={report.evidence.best_known} />
      </div>
      <div className="heading-row">
        <span className="muted">Created {formatTimestamp(report.created_at)}</span>
        <a href={`/api/v1/regressions/${encodeURIComponent(report.artifact_id)}`} rel="noreferrer" target="_blank">Open bounded report</a>
      </div>
      <p className="muted">Report hash <code title={report.report_sha256}>{compactHash(report.report_sha256)}</code></p>
      {metricNames.length > 0 && (
        <div className="table-wrap">
          <table>
            <thead><tr><th scope="col">Metric</th><th scope="col">Current</th><th scope="col">Baseline delta</th><th scope="col">Best-known delta</th><th scope="col">Status</th></tr></thead>
            <tbody>
              {metricNames.map((name) => {
                const baseline = baselineMetrics[name];
                const bestKnown = bestKnownMetrics[name];
                const regressed = Boolean(baseline?.regressed || bestKnown?.regressed);
                return (
                  <tr key={name}>
                    <th scope="row">{name}</th>
                    <td>{metricValue(name, baseline?.current ?? bestKnown?.current ?? undefined)}</td>
                    <td>{metricValue(name, baseline?.delta ?? undefined)}</td>
                    <td>{metricValue(name, bestKnown?.delta ?? undefined)}</td>
                    <td><span className={`pill ${regressed ? "risk" : "ok"}`}>{regressed ? "regressed" : "within limit"}</span></td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}

function RegressionHistory({ trend }: { trend: RegressionTrendSummary }) {
  if (trend.point_count === 0) {
    return <StateBanner title="No regression history" tone="info"><p>No persisted report points are available for this scenario.</p></StateBanner>;
  }
  const latestTransition = trend.transitions.at(-1);
  return (
    <div className="card stack">
      <div className="heading-row">
        <div>
          <p className="eyebrow">Bounded report history</p>
          <h3>{trend.point_count} point{trend.point_count === 1 ? "" : "s"}</h3>
        </div>
        <span className="pill info">server-capped history</span>
      </div>
      {trend.point_count === 1 && <StateBanner title="One-point history" tone="info"><p>A trend direction requires at least two persisted points.</p></StateBanner>}
      {latestTransition && (
        <StateBanner title="Latest transition" tone={latestTransition.to_outcome === "regression" || latestTransition.to_outcome === "quality_failure" ? "risk" : "info"}>
          <p>{latestTransition.from_outcome} → {latestTransition.to_outcome}</p>
          {latestTransition.new_reason_codes.length > 0 && <p><strong>New reasons:</strong> {latestTransition.new_reason_codes.join(", ")}</p>}
          {latestTransition.resolved_reason_codes.length > 0 && <p><strong>Resolved reasons:</strong> {latestTransition.resolved_reason_codes.join(", ")}</p>}
        </StateBanner>
      )}
      <div className="table-wrap">
        <table>
          <thead><tr><th scope="col">Created</th><th scope="col">Outcome</th><th scope="col">Reasons</th><th scope="col">Tokens</th><th scope="col">Repeated context</th><th scope="col">Cost</th><th scope="col">Quality</th><th scope="col">Evidence</th></tr></thead>
          <tbody>
            {trend.points.map((point) => {
              const state = outcomeCopy(point.outcome);
              return (
                <tr key={point.artifact_id}>
                  <td>{formatTimestamp(point.created_at)}</td>
                  <td><span className={`pill ${state.tone === "ok" ? "ok" : state.tone === "risk" ? "risk" : "info"}`}>{state.label}</span></td>
                  <td>{point.reason_codes.length > 0 ? point.reason_codes.join(", ") : "—"}</td>
                  <td>{metricValue("total_tokens", point.current_metrics.total_tokens)}</td>
                  <td>{metricValue("repeated_context_ratio", point.current_metrics.repeated_context_ratio)}</td>
                  <td>{metricValue("estimated_cost_usd", point.current_metrics.estimated_cost_usd)}</td>
                  <td>{metricValue("quality_score", point.current_metrics.quality_score)}</td>
                  <td><a href={`/api/v1/regressions/${encodeURIComponent(point.artifact_id)}`} rel="noreferrer" target="_blank">report</a></td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
      <p className="muted">Trend hash <code title={trend.trend_sha256}>{compactHash(trend.trend_sha256)}</code></p>
    </div>
  );
}

export function BenchmarkScorecards() {
  const [scenarioId, setScenarioId] = useState(DEFAULT_SCENARIO);
  const [comparison, setComparison] = useState<ScorecardScenarioComparisonSummary | null>(null);
  const [matrix, setMatrix] = useState<ScorecardMatrixSummary | null>(null);
  const [regression, setRegression] = useState<RegressionReportSummary | null>(null);
  const [trend, setTrend] = useState<RegressionTrendSummary | null>(null);
  const [loading, setLoading] = useState(false);
  const [benchmarkError, setBenchmarkError] = useState<string | null>(null);
  const [regressionError, setRegressionError] = useState<string | null>(null);
  const [budgetEvidence, setBudgetEvidence] = useState<BudgetEvidenceArtifact[]>([]);
  const [budgetEvidenceError, setBudgetEvidenceError] = useState<string | null>(null);

  const loadScenario = useCallback(async (requestedScenario: string) => {
    const normalized = requestedScenario.trim();
    if (!normalized) {
      setBenchmarkError("Scenario ID is required.");
      setRegressionError("Scenario ID is required.");
      return;
    }
    setLoading(true);
    setBenchmarkError(null);
    setRegressionError(null);
    setBudgetEvidenceError(null);
    const [scorecardsResult, artifactsResult, trendResult, budgetResult] = await Promise.allSettled([
      fetchScorecards({ scenario_id: normalized, limit: 100 }),
      fetchRegressionArtifacts({ scenario_id: normalized, limit: 100 }),
      fetchRegressionTrend(normalized, { limit: 100 }),
      fetchBudgetEvidence({ limit: 100 }),
    ]);

    if (scorecardsResult.status === "fulfilled") {
      const summary = summarizeScorecardComparison(scorecardsResult.value.comparison);
      const matrixSummary = summarizeScorecardMatrix(scorecardsResult.value.comparison);
      setComparison(summary);
      setMatrix(matrixSummary);
      if (!summary && !matrixSummary) setBenchmarkError("The scenario does not contain valid bounded comparison evidence.");
    } else {
      setComparison(null);
      setMatrix(null);
      setBenchmarkError(scorecardsResult.reason instanceof Error ? scorecardsResult.reason.message : "Failed to load benchmark scenario.");
    }

    if (artifactsResult.status === "fulfilled") {
      const reports = artifactsResult.value.artifacts
        .map(summarizeRegressionArtifact)
        .filter((item): item is RegressionReportSummary => item !== null);
      setRegression(reports.at(-1) ?? null);
    } else {
      setRegression(null);
      setRegressionError(artifactsResult.reason instanceof Error ? artifactsResult.reason.message : "Failed to load regression reports.");
    }

    if (trendResult.status === "fulfilled") {
      const summary = summarizeRegressionTrend(trendResult.value.trend);
      setTrend(summary);
      if (!summary) setRegressionError((current) => current ?? "Regression trend response is incomplete.");
    } else {
      setTrend(null);
      setRegressionError((current) => current ?? (trendResult.reason instanceof Error ? trendResult.reason.message : "Failed to load regression trend."));
    }
    if (budgetResult.status === "fulfilled") {
      setBudgetEvidence(budgetResult.value.artifacts);
    } else {
      setBudgetEvidence([]);
      setBudgetEvidenceError(budgetResult.reason instanceof Error ? budgetResult.reason.message : "Failed to load budget evidence.");
    }
    setLoading(false);
  }, []);

  useEffect(() => {
    void loadScenario(DEFAULT_SCENARIO);
  }, [loadScenario]);

  return (
    <section className="grid">
      <div className="card stack">
        <div className="heading-row">
          <div>
            <p className="eyebrow">Read-only scorecard and regression evidence</p>
            <h2>Benchmark scenarios</h2>
          </div>
          <span className="pill info">summary-level only</span>
        </div>
        <p className="muted">Compare an explicit stateless baseline with a stateful candidate, then inspect report-only regression history. No prompts, outputs, messages, checkpoints, spans, or tool payloads are exposed.</p>
        <form className="table-toolbar" onSubmit={(event) => { event.preventDefault(); void loadScenario(scenarioId); }}>
          <label className="stack" style={{ flex: 1 }}>
            <span className="metric-label">Scenario ID</span>
            <input className="search-input" value={scenarioId} onChange={(event) => setScenarioId(event.target.value)} aria-label="Benchmark scenario ID" />
          </label>
          <button disabled={loading} type="submit">{loading ? "Loading…" : "Load scenario"}</button>
        </form>
        {benchmarkError && <StateBanner title="Benchmark comparison unavailable" tone="risk"><p>{benchmarkError}</p></StateBanner>}
        {regressionError && <StateBanner title="Regression evidence unavailable" tone="risk"><p>{regressionError}</p></StateBanner>}
        {budgetEvidenceError && <StateBanner title="Budget evidence unavailable" tone="risk"><p>{budgetEvidenceError}</p></StateBanner>}
      </div>

      {comparison && (
        <div className="card stack">
          <div className="heading-row">
            <div><p className="eyebrow">{comparison.scenario_id}</p><h3>{comparison.baseline.runtime_kind} {comparison.baseline.runtime_version}</h3></div>
            <span className={`pill ${comparison.both_qualified ? "ok" : "risk"}`}>quality {comparison.both_qualified ? "qualified" : "not qualified"}</span>
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
              <tbody>{comparisonRows(comparison).map(([metric, baseline, candidate, delta]) => <tr key={metric}><th scope="row">{metric}</th><td>{baseline}</td><td>{candidate}</td><td>{delta}</td></tr>)}</tbody>
            </table>
          </div>
          <div className="heading-row">
            <span className={`pill ${comparison.token_advantage_reported ? "ok" : "info"}`}>token advantage {comparison.token_advantage_reported ? "reported" : "not reported"}</span>
            <span className={`pill ${comparison.cost_advantage_reported ? "ok" : "info"}`}>cost advantage {comparison.cost_advantage_reported ? "reported" : "not reported"}</span>
          </div>
        </div>
      )}

      {matrix && (
        <div className="card stack">
          <div className="heading-row">
            <div><p className="eyebrow">{matrix.scenario_id}</p><h3>Native / external strategy matrix</h3></div>
            <span className={`pill ${matrix.comparison_status === "comparable" ? "ok" : "risk"}`}>{matrix.comparison_status}</span>
          </div>
          {matrix.incomparable_reasons.length > 0 && <p className="muted">Reasons: {matrix.incomparable_reasons.join(", ")}</p>}
          <div className="table-wrap">
            <table>
              <thead><tr><th scope="col">Runtime</th><th scope="col">Strategy</th><th scope="col">Status</th><th scope="col">Quality</th><th scope="col">Tokens</th><th scope="col">Cost</th><th scope="col">Latency</th><th scope="col">Confidence</th></tr></thead>
              <tbody>{matrix.rows.map((row) => <tr key={row.artifact_id}><td>{row.runtime_kind} {row.runtime_version}</td><td>{row.strategy}</td><td>{row.status}</td><td>{formatNumber(row.quality_score)}</td><td>{formatNumber(row.total_tokens)}</td><td>{formatCost(row.estimated_cost_usd)}</td><td>{formatNumber(row.duration_ms)} ms</td><td>{row.measurement_confidence ?? "—"}</td></tr>)}</tbody>
            </table>
          </div>
        </div>
      )}

      {regression && <RegressionLatest report={regression} />}
      {trend && <RegressionHistory trend={trend} />}
      {!budgetEvidenceError && budgetEvidence.filter((artifact) => artifact.artifact_kind === "forecast").at(-1) && <BudgetEvidenceLatest artifact={budgetEvidence.filter((artifact) => artifact.artifact_kind === "forecast").at(-1)!} />}
      {!budgetEvidenceError && budgetEvidence.filter((artifact) => artifact.artifact_kind === "anomaly").at(-1) && <BudgetEvidenceLatest artifact={budgetEvidence.filter((artifact) => artifact.artifact_kind === "anomaly").at(-1)!} />}
      {!loading && !budgetEvidenceError && budgetEvidence.length === 0 && <StateBanner title="No budget evidence" tone="info"><p>No persisted forecast or anomaly evidence is available yet.</p></StateBanner>}
      {!loading && !regressionError && !regression && !trend && <StateBanner title="No regression evidence" tone="info"><p>No persisted regression report is available for this scenario.</p></StateBanner>}
    </section>
  );
}
