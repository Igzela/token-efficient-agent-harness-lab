import { useCallback, useEffect, useState } from "react";
import { ApiError, fetchDecisionStats, fetchDecisions } from "@/lib/api-client";
import type { DecisionListResponse, DecisionLogStats, DecisionRecord } from "@/lib/types";
import { EmptyState } from "./EmptyState";
import { StateBanner } from "./StateBanner";

type DecisionError = {
  message: string;
  type: "permission" | "error";
};

function decisionError(error: unknown): DecisionError {
  if (error instanceof ApiError && (error.status === 401 || error.status === 403)) {
    return {
      message: error.status === 403
        ? "The current API key lacks dispatch:read scope for decision log."
        : "Decision log requires protected local API access.",
      type: "permission",
    };
  }
  return {
    message: error instanceof Error ? error.message : "Failed to load decisions",
    type: "error",
  };
}

function confidencePill(confidence: number): string {
  if (confidence >= 0.8) return "ok";
  if (confidence >= 0.5) return "warn";
  return "risk";
}

function actionBadge(action: string): string {
  if (action === "dispatch") return "ok";
  if (action === "defer" || action === "escalate") return "warn";
  if (action === "reject" || action === "block") return "risk";
  return "info";
}

function formatTimestamp(ts: string): string {
  try {
    return new Date(ts).toLocaleString([], {
      month: "short",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
    });
  } catch {
    return ts;
  }
}

function StatsTiles({ stats }: { stats: DecisionLogStats }) {
  return (
    <div className="detail-summary">
      <div className="summary-tile">
        <span className="metric-label">Total decisions</span>
        <strong>{stats.total_decisions}</strong>
      </div>
      <div className="summary-tile">
        <span className="metric-label">Avg confidence</span>
        <strong>{(stats.avg_confidence * 100).toFixed(1)}%</strong>
      </div>
      {Object.entries(stats.by_action).map(([action, count]) => (
        <div className="summary-tile" key={action}>
          <span className="metric-label">{action}</span>
          <strong>{count}</strong>
        </div>
      ))}
    </div>
  );
}

function DecisionRow({ decision }: { decision: DecisionRecord }) {
  const [expanded, setExpanded] = useState(false);
  const hasSignals = decision.input_signals && Object.keys(decision.input_signals).length > 0;

  return (
    <>
      <tr>
        <td className="mono" style={{ fontSize: "0.75rem" }}>{decision.decision_id.slice(0, 12)}</td>
        <td>
          <span className={`pill ${actionBadge(decision.action)}`}>{decision.action}</span>
        </td>
        <td style={{ maxWidth: 280 }} title={decision.reason}>{decision.reason}</td>
        <td>{decision.executor ?? "—"}</td>
        <td>
          <span className={`pill ${confidencePill(decision.confidence)}`}>
            {(decision.confidence * 100).toFixed(0)}%
          </span>
        </td>
        <td className="mono" style={{ fontSize: "0.75rem" }}>{decision.selected_tier}</td>
        <td>{formatTimestamp(decision.created_at)}</td>
        <td>
          {hasSignals && (
            <button
              className="pill info"
              onClick={() => setExpanded(!expanded)}
              type="button"
              style={{ cursor: "pointer" }}
            >
              {expanded ? "hide" : "signals"}
            </button>
          )}
        </td>
      </tr>
      {expanded && hasSignals && (
        <tr>
          <td colSpan={8} style={{ padding: "4px 12px 8px", background: "var(--surface-2)" }}>
            <pre style={{ margin: 0, fontSize: "0.75rem", whiteSpace: "pre-wrap" }}>
              {JSON.stringify(decision.input_signals, null, 2)}
            </pre>
          </td>
        </tr>
      )}
    </>
  );
}

export function DecisionLog() {
  const [data, setData] = useState<DecisionListResponse | null>(null);
  const [stats, setStats] = useState<DecisionLogStats | null>(null);
  const [error, setError] = useState<DecisionError | null>(null);
  const [loading, setLoading] = useState(true);
  const [search, setSearch] = useState("");

  const load = useCallback(() => {
    setLoading(true);
    setError(null);
    Promise.all([
      fetchDecisions({ limit: 100, search: search || undefined }),
      fetchDecisionStats(),
    ])
      .then(([decisions, statsRes]) => {
        setData(decisions);
        setStats(statsRes.stats);
      })
      .catch((e) => setError(decisionError(e)))
      .finally(() => setLoading(false));
  }, [search]);

  useEffect(() => {
    load();
  }, [load]);

  return (
    <section className="card stack">
      <div className="flex-between">
        <h2>Decision Log</h2>
        <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
          <input
            type="text"
            placeholder="Search decisions..."
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            style={{ padding: "4px 8px", fontSize: "0.85rem" }}
          />
          <button onClick={load} type="button">Refresh</button>
        </div>
      </div>

      {error?.type === "permission" && (
        <StateBanner title="Permission required" tone="warn"><p>{error.message}</p></StateBanner>
      )}
      {error?.type === "error" && (
        <StateBanner title="Failed to load" tone="risk"><p>{error.message}</p></StateBanner>
      )}

      {loading ? (
        <div className="loading-row"><span className="spinner" /> Loading decision log...</div>
      ) : !data ? (
        <EmptyState
          title="Decision log unavailable"
          description="Could not retrieve decision log from the engine."
          tone="warn"
        />
      ) : data.decisions.length === 0 ? (
        <EmptyState
          title="No decisions recorded"
          description="Decision records will appear here once dispatches are processed."
          tone="info"
        />
      ) : (
        <>
          {stats && <StatsTiles stats={stats} />}

          <table className="table">
            <thead>
              <tr>
                <th>ID</th>
                <th>Action</th>
                <th>Reason</th>
                <th>Executor</th>
                <th>Confidence</th>
                <th>Tier</th>
                <th>Created</th>
                <th></th>
              </tr>
            </thead>
            <tbody>
              {data.decisions.map((d) => (
                <DecisionRow key={d.decision_id} decision={d} />
              ))}
            </tbody>
          </table>
        </>
      )}
    </section>
  );
}
