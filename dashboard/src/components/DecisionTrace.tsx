import { useCallback, useEffect, useState } from "react";
import { ApiError, fetchDecisions } from "@/lib/api-client";
import type { DecisionRecord } from "@/lib/types";
import { EmptyState } from "./EmptyState";
import { StateBanner } from "./StateBanner";

type TraceError = {
  message: string;
  type: "permission" | "error";
};

function traceError(error: unknown): TraceError {
  if (error instanceof ApiError && (error.status === 401 || error.status === 403)) {
    return {
      message: error.status === 403
        ? "The current API key lacks dispatch:read scope for decision trace."
        : "Decision trace requires protected local API access.",
      type: "permission",
    };
  }
  return {
    message: error instanceof Error ? error.message : "Failed to load decision trace",
    type: "error",
  };
}

function confidenceBorder(confidence: number): string {
  if (confidence >= 0.8) return "var(--ok)";
  if (confidence >= 0.5) return "var(--warn)";
  return "var(--risk)";
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
      second: "2-digit",
    });
  } catch {
    return ts;
  }
}

interface TraceEntryProps {
  decision: DecisionRecord;
  isLast: boolean;
}

function TraceEntry({ decision, isLast }: TraceEntryProps) {
  const [expanded, setExpanded] = useState(false);
  const borderColor = confidenceBorder(decision.confidence);
  const hasSignals = decision.input_signals && Object.keys(decision.input_signals).length > 0;
  const poolFailure = decision.executor_pool_signal?.failure_score as number | undefined;

  return (
    <div style={{ display: "flex", gap: 12 }}>
      <div style={{ display: "flex", flexDirection: "column", alignItems: "center", minWidth: 20 }}>
        <div
          style={{
            width: 12,
            height: 12,
            borderRadius: "50%",
            background: borderColor,
            flexShrink: 0,
          }}
        />
        {!isLast && (
          <div style={{ width: 2, flex: 1, background: "var(--border)", minHeight: 24 }} />
        )}
      </div>

      <div
        style={{
          flex: 1,
          borderLeft: `3px solid ${borderColor}`,
          paddingLeft: 12,
          paddingBottom: isLast ? 0 : 16,
        }}
      >
        <div style={{ display: "flex", alignItems: "center", gap: 8, flexWrap: "wrap" }}>
          <span className={`pill ${actionBadge(decision.action)}`}>{decision.action}</span>
          {decision.degraded_reason && (
            <span className="pill risk" title={decision.degraded_reason}>degraded</span>
          )}
          <span className={`pill ${decision.confidence >= 0.8 ? "ok" : decision.confidence >= 0.5 ? "warn" : "risk"}`}>
            {(decision.confidence * 100).toFixed(0)}% conf
          </span>
          {decision.executor && (
            <span className="pill info">{decision.executor}</span>
          )}
          {decision.candidate_executors && decision.candidate_executors.length > 0 && (
            <span className="muted" style={{ fontSize: "0.75rem" }}>
              candidates: {decision.candidate_executors.join(", ")}
            </span>
          )}
          {poolFailure != null && (
            <span
              className={`pill ${poolFailure >= 0.7 ? "risk" : poolFailure >= 0.4 ? "warn" : "ok"}`}
              title={`executor pool failure_score: ${poolFailure}`}
            >
              pool {poolFailure.toFixed(2)}
            </span>
          )}
          {decision.quality_signal && (
            <span
              className={`pill ${decision.quality_signal.pass === false ? "risk" : "ok"}`}
              title={JSON.stringify(decision.quality_signal)}
            >
              quality {decision.quality_signal.pass === false ? "fail" : "pass"}
            </span>
          )}
          {decision.node_id && <span className="mono" style={{ fontSize: "0.75rem" }}>{decision.node_id}</span>}
          <span className="muted" style={{ fontSize: "0.75rem" }}>{formatTimestamp(decision.created_at)}</span>
        </div>

        <p style={{ margin: "4px 0 0", fontSize: "0.85rem" }}>{decision.reason}</p>

        {hasSignals && (
          <button
            onClick={() => setExpanded(!expanded)}
            type="button"
            style={{ cursor: "pointer", marginTop: 4, fontSize: "0.8rem" }}
          >
            {expanded ? "Hide input signals" : "Show input signals"}
          </button>
        )}

        {expanded && hasSignals && (
          <pre
            style={{
              margin: "4px 0 0",
              padding: 8,
              background: "var(--surface-2)",
              borderRadius: 4,
              fontSize: "0.75rem",
              whiteSpace: "pre-wrap",
              maxHeight: 200,
              overflow: "auto",
            }}
          >
            {JSON.stringify(decision.input_signals, null, 2)}
          </pre>
        )}
      </div>
    </div>
  );
}

interface DecisionTraceProps {
  runId: string;
}

export function DecisionTrace({ runId }: DecisionTraceProps) {
  const [decisions, setDecisions] = useState<DecisionRecord[]>([]);
  const [error, setError] = useState<TraceError | null>(null);
  const [loading, setLoading] = useState(true);

  const load = useCallback(() => {
    setLoading(true);
    setError(null);
    fetchDecisions({ run_id: runId, limit: 200 })
      .then((res) => setDecisions(res.decisions))
      .catch((e) => setError(traceError(e)))
      .finally(() => setLoading(false));
  }, [runId]);

  useEffect(() => {
    load();
  }, [load]);

  return (
    <section className="card stack">
      <div className="flex-between">
        <h3>Decision Trace</h3>
        <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
          <span className="mono muted" style={{ fontSize: "0.8rem" }}>{runId}</span>
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
        <div className="loading-row"><span className="spinner" /> Loading decision trace...</div>
      ) : decisions.length === 0 ? (
        <EmptyState
          title="No decisions for this run"
          description="Decision records will appear here once the run is processed."
          tone="info"
        />
      ) : (
        <div>
          {decisions.map((d, i) => (
            <TraceEntry key={d.decision_id} decision={d} isLast={i === decisions.length - 1} />
          ))}
        </div>
      )}
    </section>
  );
}
