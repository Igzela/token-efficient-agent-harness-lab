import { useEffect, useMemo, useState } from "react";
import {
  ApiError,
  approveProposal,
  deactivateProposal,
  fetchDispatchMetrics,
  fetchFeedbackCostOfPass,
  fetchFeedbackPatterns,
  fetchFeedbackTraces,
  fetchProposals,
  fetchSimulationReport,
  rejectProposal,
  rollbackProposal,
} from "@/lib/api-client";
import type {
  ControlledLoopProposal,
  DispatchMetricsResponse,
  FeedbackCostOfPassResponse,
  FeedbackPatternListResponse,
  FeedbackTraceListResponse,
  SimulationReportResponse,
} from "@/lib/types";
import { ConfirmDialog, type ConfirmAction } from "./ConfirmDialog";
import { EmptyState } from "./EmptyState";
import { Metric } from "./Metric";
import { StateBanner } from "./StateBanner";

type RegulatorData = {
  metrics: DispatchMetricsResponse | null;
  traces: FeedbackTraceListResponse | null;
  costs: FeedbackCostOfPassResponse | null;
  patterns: FeedbackPatternListResponse | null;
  simulation: SimulationReportResponse | null;
  proposals: ControlledLoopProposal[];
};

type RegulatorError = {
  message: string;
  type: "permission" | "error";
};

const emptyData: RegulatorData = {
  metrics: null,
  traces: null,
  costs: null,
  patterns: null,
  simulation: null,
  proposals: [],
};

function regulatorError(error: unknown): RegulatorError {
  if (error instanceof ApiError && (error.status === 401 || error.status === 403)) {
    return {
      message: error.status === 403
        ? "Current API key lacks dispatch:read or cost:read scope."
        : "Dynamic regulator views require protected local API access.",
      type: "permission",
    };
  }
  return {
    message: error instanceof Error ? error.message : "Failed to load dynamic regulator data",
    type: "error",
  };
}

function formatRate(value: unknown): string {
  return typeof value === "number" ? `${Math.round(value * 100)}%` : "0%";
}

function formatCost(value: unknown): string {
  return typeof value === "number" ? `$${value.toFixed(4)}` : "n/a";
}

export function DynamicRegulator() {
  const [data, setData] = useState<RegulatorData>(emptyData);
  const [error, setError] = useState<RegulatorError | null>(null);
  const [loading, setLoading] = useState(true);
  const [confirmAction, setConfirmAction] = useState<ConfirmAction>(null);
  const [busy, setBusy] = useState(false);

  function load() {
    setLoading(true);
    Promise.all([
      fetchDispatchMetrics({ limit: 200 }),
      fetchFeedbackTraces({ limit: 20 }),
      fetchFeedbackCostOfPass(),
      fetchFeedbackPatterns({ limit: 20 }),
      fetchSimulationReport({ limit: 50 }),
      fetchProposals({ limit: 20 }),
    ])
      .then(([metrics, traces, costs, patterns, simulation, proposals]) => {
        setData({
          metrics,
          traces,
          costs,
          patterns,
          simulation,
          proposals: proposals.proposals,
        });
        setError(null);
      })
      .catch((e) => {
        setData(emptyData);
        setError(regulatorError(e));
      })
      .finally(() => setLoading(false));
  }

  useEffect(() => {
    load();
  }, []);

  async function doConfirm() {
    if (!confirmAction || !("proposalId" in confirmAction)) return;
    setBusy(true);
    try {
      if (confirmAction.type === "approveProposal") {
        await approveProposal(confirmAction.proposalId);
      } else if (confirmAction.type === "rejectProposal") {
        await rejectProposal(confirmAction.proposalId);
      } else if (confirmAction.type === "rollbackProposal") {
        await rollbackProposal(confirmAction.proposalId);
      } else if (confirmAction.type === "deactivateProposal") {
        await deactivateProposal(confirmAction.proposalId);
      }
      load();
    } catch {
      setError({ message: "Proposal action failed", type: "error" });
    } finally {
      setBusy(false);
      setConfirmAction(null);
    }
  }

  const totals = data.metrics?.metrics.totals;
  const topTiers = useMemo(
    () => data.metrics?.metrics.by_tier.slice(0, 5) ?? [],
    [data.metrics],
  );
  const traces = data.traces?.traces ?? [];
  const costRows = data.costs?.rows ?? [];
  const patternRows = data.patterns?.patterns ?? [];
  const simulationRows = data.simulation?.report ?? [];

  return (
    <section className="card stack">
      <div className="flex-between">
        <h2>Dynamic Regulator</h2>
        <button onClick={load} type="button">Refresh</button>
      </div>

      {error?.type === "permission" && (
        <StateBanner title="Dynamic regulator data requires scopes" tone="warn">
          <p>{error.message}</p>
        </StateBanner>
      )}
      {error?.type === "error" && (
        <StateBanner title="Dynamic regulator data unavailable" tone="risk">
          <p>{error.message}</p>
        </StateBanner>
      )}

      {loading && !data.metrics ? (
        <div className="loading-row"><span className="spinner" /> Loading dynamic regulator data...</div>
      ) : !data.metrics && !error ? (
        <EmptyState
          title="No regulator data yet"
          description="Persist dispatch records to populate feedback, cost, and shadow routing views."
          tone="info"
        />
      ) : data.metrics ? (
        <>
          <div className="status-strip" aria-label="Dynamic regulator metrics">
            <Metric label="Dispatches" value={String(totals?.dispatch_count ?? 0)} detail="sampled" />
            <Metric label="Pass Rate" value={formatRate(totals?.success_rate)} detail="feedback" />
            <Metric label="Cost" value={formatCost(totals?.total_estimated_cost_usd)} detail="estimated" />
            <Metric label="Shadow Routes" value={String(data.simulation?.summary?.shadow_route_count ?? 0)} detail="diagnostic" />
            <Metric label="Active Proposals" value={String(data.proposals.filter((p) => p.status === "active").length)} detail="controlled" />
          </div>

          <div className="grid two">
            <div className="subcard stack">
              <h3>Tier Metrics</h3>
              {topTiers.length === 0 ? (
                <EmptyState title="No tier metrics" description="No dispatch records in current sample." />
              ) : (
                <table>
                  <thead>
                    <tr>
                      <th>Tier</th>
                      <th>Dispatches</th>
                      <th>Pass</th>
                      <th>Cost</th>
                    </tr>
                  </thead>
                  <tbody>
                    {topTiers.map((row) => (
                      <tr key={String(row.selected_tier ?? row.tier ?? "unknown")}>
                        <td>{String(row.selected_tier ?? row.tier ?? "unknown")}</td>
                        <td>{row.dispatch_count}</td>
                        <td>{formatRate(row.success_rate)}</td>
                        <td>{formatCost(row.total_estimated_cost_usd)}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              )}
            </div>

            <div className="subcard stack">
              <h3>Cost Of Pass</h3>
              {costRows.length === 0 ? (
                <EmptyState title="No cost rows" description="No pass/fail cost aggregates available." />
              ) : (
                <table>
                  <thead>
                    <tr>
                      <th>Class</th>
                      <th>Tier</th>
                      <th>Pass</th>
                      <th>Avg Cost</th>
                    </tr>
                  </thead>
                  <tbody>
                    {costRows.slice(0, 8).map((row) => (
                      <tr key={`${row.task_class}-${row.tier}`}>
                        <td>{row.task_class}</td>
                        <td>{row.tier}</td>
                        <td>{formatRate(row.pass_rate)}</td>
                        <td>{formatCost(row.average_cost_usd)}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              )}
            </div>
          </div>

          <div className="subcard stack">
            <h3>Feedback Patterns</h3>
            {patternRows.length === 0 ? (
              <EmptyState title="No patterns detected" description="Feedback patterns emerge from aggregate dispatch outcomes over time." />
            ) : (
              <table>
                <thead>
                  <tr>
                    <th>Pattern Type</th>
                    <th>Affected</th>
                    <th>Count</th>
                    <th>Rate</th>
                    <th>Severity</th>
                    <th>Recommendation</th>
                  </tr>
                </thead>
                <tbody>
                  {patternRows.slice(0, 10).map((p) => (
                    <tr key={p.pattern_id}>
                      <td>{p.pattern_type}</td>
                      <td>{p.affected_tier ?? p.affected_task_class ?? "n/a"}</td>
                      <td>{p.count}</td>
                      <td>{formatRate(p.rate)}</td>
                      <td>
                        <span style={{
                          color: p.severity === "high" ? "var(--color-risk, #e74c3c)" : p.severity === "medium" ? "var(--color-warn, #f39c12)" : "var(--color-ok, #27ae60)",
                          fontWeight: 600,
                        }}>
                          {p.severity}
                        </span>
                      </td>
                      <td>{p.recommendation_hint}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </div>

          <div className="grid two">
            <div className="subcard stack">
              <h3>Feedback Traces</h3>
              {traces.length === 0 ? (
                <EmptyState title="No traces" description="Feedback traces are derived from persisted dispatch history." />
              ) : (
                <table>
                  <thead>
                    <tr>
                      <th>Trace</th>
                      <th>Class</th>
                      <th>Tier</th>
                      <th>Status</th>
                    </tr>
                  </thead>
                  <tbody>
                    {traces.slice(0, 8).map((trace) => (
                      <tr key={trace.trace_id}>
                        <td>{trace.trace_id}</td>
                        <td>{trace.task_class}</td>
                        <td>{trace.tier}</td>
                        <td>{trace.status}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              )}
            </div>

            <div className="subcard stack">
              <h3>Controlled Loop</h3>
              {data.proposals.length === 0 ? (
                <EmptyState title="No proposals" description="No controlled-loop policy proposals found." />
              ) : (
                <table>
                  <thead>
                    <tr>
                      <th>Proposal</th>
                      <th>Status</th>
                      <th>Key</th>
                      <th>Tier</th>
                      <th>Actions</th>
                    </tr>
                  </thead>
                  <tbody>
                    {data.proposals.slice(0, 8).map((proposal) => (
                      <tr key={proposal.proposal_id}>
                        <td>{proposal.title ?? proposal.proposal_id}</td>
                        <td>{proposal.status}</td>
                        <td>{proposal.policy_key ?? proposal.task_class ?? "unknown"}</td>
                        <td>{proposal.target_tier ?? proposal.tier ?? "unknown"}</td>
                        <td>
                          {proposal.status === "pending" && (
                            <span style={{ display: "inline-flex", gap: "4px" }}>
                              <button
                                type="button"
                                disabled={busy}
                                onClick={() => setConfirmAction({ type: "approveProposal", proposalId: proposal.proposal_id })}
                              >
                                Approve
                              </button>
                              <button
                                type="button"
                                className="risk-action"
                                disabled={busy}
                                onClick={() => setConfirmAction({ type: "rejectProposal", proposalId: proposal.proposal_id })}
                              >
                                Reject
                              </button>
                            </span>
                          )}
                          {proposal.status === "active" && (
                            <span style={{ display: "inline-flex", gap: "4px" }}>
                              <button
                                type="button"
                                disabled={busy}
                                onClick={() => setConfirmAction({ type: "rollbackProposal", proposalId: proposal.proposal_id })}
                              >
                                Rollback
                              </button>
                              <button
                                type="button"
                                className="risk-action"
                                disabled={busy}
                                onClick={() => setConfirmAction({ type: "deactivateProposal", proposalId: proposal.proposal_id })}
                              >
                                Deactivate
                              </button>
                            </span>
                          )}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              )}
            </div>
          </div>

          <div className="subcard stack">
            <h3>Shadow Simulation</h3>
            {simulationRows.length === 0 ? (
              <EmptyState title="No simulation rows" description="Shadow routes appear after dispatch decisions include diagnostic alternatives." />
            ) : (
              <table>
                <thead>
                  <tr>
                    <th>Scenario</th>
                    <th>Class</th>
                    <th>Tier</th>
                    <th>Status</th>
                    <th>Recommendation</th>
                  </tr>
                </thead>
                <tbody>
                  {simulationRows.slice(0, 10).map((row) => (
                    <tr key={row.scenario_id}>
                      <td>{row.scenario_id}</td>
                      <td>{row.task_class ?? "unknown"}</td>
                      <td>{row.tier ?? "unknown"}</td>
                      <td>{row.status}</td>
                      <td>{row.recommendation ?? "diagnostic"}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </div>
        </>
      ) : null}
      <ConfirmDialog
        action={confirmAction}
        onConfirm={doConfirm}
        onCancel={() => setConfirmAction(null)}
      />
    </section>
  );
}
