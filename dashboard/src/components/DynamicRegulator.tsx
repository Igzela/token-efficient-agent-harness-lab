import { useEffect, useMemo, useState } from "react";
import {
  ApiError,
  approveProposal,
  applyAutoAdjustment,
  deactivateProposal,
  fetchAutoAdjustments,
  fetchDispatchMetrics,
  fetchFeedbackCostOfPass,
  fetchFeedbackPatterns,
  fetchFeedbackTraces,
  fetchGeneratedProposals,
  fetchOfflineReplayArtifacts,
  fetchProposals,
  fetchPolicySimulationReport,
  fetchSimulationReport,
  rejectProposal,
  rollbackAutoAdjustment,
  rollbackProposal,
} from "@/lib/api-client";
import type {
  AutoAdjustmentsReport,
  ControlledLoopProposal,
  DispatchMetricsResponse,
  FeedbackCostOfPassResponse,
  FeedbackPatternListResponse,
  FeedbackTraceListResponse,
  OfflineReplayArtifactListResponse,
  PolicySimulationResult,
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
  policySimulation: PolicySimulationResult | null;
  offlineReplay: OfflineReplayArtifactListResponse | null;
  proposals: ControlledLoopProposal[];
  autoAdjustments: AutoAdjustmentsReport | null;
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
  policySimulation: null,
  offlineReplay: null,
  proposals: [],
  autoAdjustments: null,
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

function replayReasonCodes(artifact: OfflineReplayArtifactListResponse["artifacts"][number]): string {
  const reasons = artifact.report.reason_codes;
  return Array.isArray(reasons) && reasons.length > 0
    ? reasons.slice(0, 3).map(String).join(", ")
    : "no blocking reason recorded";
}

export function DynamicRegulator() {
  const [data, setData] = useState<RegulatorData>(emptyData);
  const [error, setError] = useState<RegulatorError | null>(null);
  const [loading, setLoading] = useState(true);
  const [confirmAction, setConfirmAction] = useState<ConfirmAction>(null);
  const [busy, setBusy] = useState(false);
  const [generatedProposals, setGeneratedProposals] = useState<any[]>([]);
  const [aaBusy, setAaBusy] = useState<string | null>(null);

  function load() {
    setLoading(true);
    let offlineReplayError: string | null = null;
    Promise.all([
      Promise.all([
        fetchDispatchMetrics({ limit: 200 }),
        fetchFeedbackTraces({ limit: 20 }),
        fetchFeedbackCostOfPass(),
        fetchFeedbackPatterns({ limit: 20 }),
        fetchSimulationReport({ limit: 50 }),
        fetchPolicySimulationReport({ limit: 50, policy: "complexity_aware" }),
        fetchProposals({ limit: 20 }),
        fetchGeneratedProposals({ limit: 10 }),
        fetchAutoAdjustments({ limit: 20 }),
      ]),
      fetchOfflineReplayArtifacts({ limit: 20 }).catch((e) => {
        offlineReplayError = e instanceof Error ? e.message : "Offline replay evidence unavailable";
        return null;
      }),
    ])
      .then(([[metrics, traces, costs, patterns, simulation, policySimulation, proposals, generated, autoAdjustments], offlineReplay]) => {
        setData({
          metrics,
          traces,
          costs,
          patterns,
          simulation,
          policySimulation,
          offlineReplay,
          proposals: proposals.proposals,
          autoAdjustments,
        });
        setGeneratedProposals(generated.candidates || []);
        setError(offlineReplayError ? { message: offlineReplayError, type: "error" } : null);
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
  const policyDelta = data.policySimulation;
  const replayArtifacts = data.offlineReplay?.artifacts ?? [];

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
            <h3>Auto-Adjustments</h3>
            {!data.autoAdjustments ? (
              <EmptyState title="No auto-adjustments data" description="Auto-adjustments report requires feedback infrastructure." />
            ) : (
              <>
                <div style={{ display: "flex", gap: "8px", flexWrap: "wrap", marginBottom: "8px" }}>
                  <span className="pill info">{data.autoAdjustments.mode}</span>
                  {data.autoAdjustments.env_gate && <span className="pill info">env gate</span>}
                  {data.autoAdjustments.dry_run && <span className="pill warn">dry-run</span>}
                  {data.autoAdjustments.no_live_mutation && <span className="pill warn">no-live-mutation</span>}
                  {data.autoAdjustments.active_apply_available && <span className="pill ok">apply ready</span>}
                </div>
                {data.autoAdjustments.blocked_reasons.length > 0 && (
                  <ul style={{ color: "var(--color-warn, #f39c12)", fontSize: "0.85rem", margin: "4px 0" }}>
                    {data.autoAdjustments.blocked_reasons.map((r, i) => <li key={i}>{r}</li>)}
                  </ul>
                )}
                {data.autoAdjustments.active_auto_adjustments.length > 0 && (
                  <table>
                    <thead>
                      <tr>
                        <th>Adjustment</th>
                        <th>Policy</th>
                        <th>Tier</th>
                        <th>Status</th>
                        <th>Actions</th>
                      </tr>
                    </thead>
                    <tbody>
                      {data.autoAdjustments.active_auto_adjustments.slice(0, 10).map((adj: any) => (
                        <tr key={adj.adjustment_id ?? adj.snapshot_id}>
                          <td>{adj.adjustment_id ?? adj.snapshot_id}</td>
                          <td>{adj.policy_key ?? "n/a"}</td>
                          <td>{adj.target_tier ?? "n/a"}</td>
                          <td>{adj.status ?? "active"}</td>
                          <td>
                            <button
                              type="button"
                              disabled={aaBusy !== null}
                              onClick={async () => {
                                setAaBusy(adj.adjustment_id);
                                try {
                                  await rollbackAutoAdjustment(adj.adjustment_id, {
                                    reason: "Rollback via dashboard",
                                    confirm_auto_adjustment_rollback: true,
                                  });
                                  load();
                                } catch {
                                  setError({ message: "Auto-adjustment rollback failed", type: "error" });
                                } finally {
                                  setAaBusy(null);
                                }
                              }}
                            >
                              {aaBusy === adj.adjustment_id ? "Rolling back..." : "Rollback"}
                            </button>
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                )}
                {data.autoAdjustments.decisions.length > 0 && (
                  <details>
                    <summary style={{ cursor: "pointer", fontSize: "0.85rem" }}>
                      Decisions ({data.autoAdjustments.decisions.length})
                    </summary>
                    <table>
                      <thead>
                        <tr><th>ID</th><th>Key</th><th>Tier</th><th>Status</th><th>Actions</th></tr>
                      </thead>
                      <tbody>
                        {data.autoAdjustments.decisions.slice(0, 10).map((d: any, i) => (
                          <tr key={d.proposal_id ?? i}>
                            <td>{d.proposal_id ?? i}</td>
                            <td>{d.policy_key ?? "n/a"}</td>
                            <td>{d.target_tier ?? "n/a"}</td>
                            <td>{d.status ?? "pending"}</td>
                            <td>
                              <button
                                type="button"
                                disabled={aaBusy !== null}
                                onClick={async () => {
                                  setAaBusy("apply");
                                  try {
                                    await applyAutoAdjustment({
                                      candidate_id: d.candidate_id,
                                      confirm_auto_adjustment: true,
                                    });
                                    load();
                                  } catch {
                                    setError({ message: "Apply auto-adjustment failed", type: "error" });
                                  } finally {
                                    setAaBusy(null);
                                  }
                                }}
                              >
                                Apply
                              </button>
                            </td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </details>
                )}
              </>
            )}
          </div>

          <div className="subcard stack">
            <h3>Generated Suggestions</h3>
            <p style={{ fontSize: "0.8rem", color: "var(--text-muted, #888)", margin: 0 }}>
              Auto-generated from feedback patterns and simulation evidence — not active until approved
            </p>
            {generatedProposals.length === 0 ? (
              <EmptyState title="No generated suggestions" description="Generated proposals appear when feedback patterns and simulation evidence suggest routing changes." />
            ) : (
              <table>
                <thead>
                  <tr>
                    <th>Suggestion</th>
                    <th>Key</th>
                    <th>Tier</th>
                    <th>Confidence</th>
                    <th>Risk</th>
                  </tr>
                </thead>
                <tbody>
                  {generatedProposals.slice(0, 6).map((c) => (
                    <tr key={c.proposal_id ?? c.candidate_id}>
                      <td>{c.title ?? c.proposal_id}</td>
                      <td>{c.policy_key ?? c.task_class ?? "unknown"}</td>
                      <td>{c.target_tier ?? "unknown"}</td>
                      <td>{c.confidence != null ? Math.round(c.confidence * 100) + "%" : "n/a"}</td>
                      <td>
                        <span style={{
                          color: c.risk_level === "high" ? "var(--color-risk, #e74c3c)" : c.risk_level === "medium" ? "var(--color-warn, #f39c12)" : "var(--color-ok, #27ae60)",
                          fontWeight: 600,
                        }}>
                          {c.risk_level ?? "unknown"}
                        </span>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </div>

          <div className="subcard stack">
            <h3>Shadow Simulation</h3>
            <p style={{ fontSize: "0.8rem", color: "var(--text-muted, #888)", margin: 0 }}>
              shadow-only, no live routing effect
            </p>
            {policyDelta && policyDelta.input_trace_count > 0 && (
              <div style={{ display: "grid", gridTemplateColumns: "repeat(4, 1fr)", gap: "8px", marginTop: "4px" }}>
                <div className="subcard" style={{ padding: "8px" }}>
                  <div style={{ fontSize: "0.75rem", color: "var(--text-muted, #888)" }}>Success Rate</div>
                  <div style={{ fontWeight: 600 }}>
                    {formatRate(policyDelta.actual_success_rate)} → {formatRate(policyDelta.simulated_success_rate)}
                  </div>
                  <div style={{ color: policyDelta.success_rate_delta >= 0 ? "var(--color-ok, #27ae60)" : "var(--color-risk, #e74c3c)", fontWeight: 600 }}>
                    {policyDelta.success_rate_delta >= 0 ? "+" : ""}{formatRate(policyDelta.success_rate_delta)}
                  </div>
                </div>
                <div className="subcard" style={{ padding: "8px" }}>
                  <div style={{ fontSize: "0.75rem", color: "var(--text-muted, #888)" }}>Avg Cost</div>
                  <div style={{ fontWeight: 600 }}>
                    {formatCost(policyDelta.actual_average_cost)} → {formatCost(policyDelta.simulated_average_cost)}
                  </div>
                  <div style={{ color: policyDelta.cost_delta <= 0 ? "var(--color-ok, #27ae60)" : "var(--color-risk, #e74c3c)", fontWeight: 600 }}>
                    {policyDelta.cost_delta >= 0 ? "+" : ""}{formatCost(policyDelta.cost_delta)}
                  </div>
                </div>
                <div className="subcard" style={{ padding: "8px" }}>
                  <div style={{ fontSize: "0.75rem", color: "var(--text-muted, #888)" }}>Avg Latency</div>
                  <div style={{ fontWeight: 600 }}>
                    {Math.round(policyDelta.actual_average_latency_ms)}ms → {Math.round(policyDelta.simulated_average_latency_ms)}ms
                  </div>
                  <div style={{ color: policyDelta.latency_delta <= 0 ? "var(--color-ok, #27ae60)" : "var(--color-risk, #e74c3c)", fontWeight: 600 }}>
                    {policyDelta.latency_delta >= 0 ? "+" : ""}{Math.round(policyDelta.latency_delta)}ms
                  </div>
                </div>
                <div className="subcard" style={{ padding: "8px" }}>
                  <div style={{ fontSize: "0.75rem", color: "var(--text-muted, #888)" }}>Human Review Rate</div>
                  <div style={{ fontWeight: 600 }}>
                    {formatRate(policyDelta.actual_human_review_rate)} → {formatRate(policyDelta.simulated_human_review_rate)}
                  </div>
                  <div style={{ color: policyDelta.human_review_rate_delta <= 0 ? "var(--color-ok, #27ae60)" : "var(--color-warn, #f39c12)", fontWeight: 600 }}>
                    {policyDelta.human_review_rate_delta >= 0 ? "+" : ""}{formatRate(policyDelta.human_review_rate_delta)}
                  </div>
                </div>
              </div>
            )}
            {policyDelta && (
              <div style={{ fontSize: "0.75rem", color: "var(--text-muted, #888)" }}>
                scenario: {policyDelta.scenario_id} | candidate: {policyDelta.candidate_policy_id} | traces: {policyDelta.input_trace_count}
              </div>
            )}
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

          <div className="subcard stack">
            <h3>Trace-backed Offline Replay</h3>
            <p style={{ fontSize: "0.8rem", color: "var(--text-muted, #888)", margin: 0 }}>
              read-only evidence; no provider calls or live policy effect
            </p>
            {data.offlineReplay === null && error?.type === "error" ? (
              <StateBanner title="Offline replay evidence unavailable" tone="risk">
                <p>{error.message}</p>
              </StateBanner>
            ) : replayArtifacts.length === 0 ? (
              <EmptyState
                title="No offline replay artifacts"
                description="Accepted trace-backed replay evidence appears here after an offline replay is recorded. Empty does not authorize promotion."
                tone="info"
              />
            ) : (
              <table>
                <thead>
                  <tr>
                    <th>Status</th>
                    <th>Reason</th>
                    <th>Evidence hash</th>
                    <th>Created</th>
                  </tr>
                </thead>
                <tbody>
                  {replayArtifacts.slice(0, 8).map((artifact) => (
                    <tr key={artifact.artifact_id}>
                      <td>
                        {artifact.historical_only
                          ? "historical (not authorizing)"
                          : artifact.status}
                      </td>
                      <td>{replayReasonCodes(artifact)}</td>
                      <td>{artifact.content_sha256.slice(0, 12)}…</td>
                      <td>{artifact.created_at}</td>
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
