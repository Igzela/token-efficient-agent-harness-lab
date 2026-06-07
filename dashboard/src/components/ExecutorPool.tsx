import { useCallback, useEffect, useState } from "react";
import { ApiError, fetchExecutorPool } from "@/lib/api-client";
import type { ExecutorPoolEntry, ExecutorPoolStatus } from "@/lib/types";
import { EmptyState } from "./EmptyState";
import { StateBanner } from "./StateBanner";

type PoolError = {
  message: string;
  type: "permission" | "error";
};

function poolError(error: unknown): PoolError {
  if (error instanceof ApiError && (error.status === 401 || error.status === 403)) {
    return {
      message: error.status === 403
        ? "The current API key lacks executor:read scope for pool status."
        : "Executor pool status requires protected local API access.",
      type: "permission",
    };
  }
  return {
    message: error instanceof Error ? error.message : "Failed to load executor pool",
    type: "error",
  };
}

function statusPill(status: string): string {
  if (status === "available") return "ok";
  if (status === "cooldown") return "warn";
  return "risk";
}

function capacityColor(active: number, capacity: number): string {
  if (capacity === 0) return "var(--muted)";
  const pct = (active / capacity) * 100;
  if (pct > 90) return "var(--risk)";
  if (pct > 75) return "var(--warn)";
  return "var(--ok)";
}

function formatCooldown(until: string | null): string | null {
  if (!until) return null;
  const diff = new Date(until).getTime() - Date.now();
  if (diff <= 0) return null;
  if (diff < 60_000) return `${Math.ceil(diff / 1000)}s`;
  return `${Math.ceil(diff / 60_000)}m`;
}

function CapacityBar({ entry }: { entry: ExecutorPoolEntry }) {
  const pct = entry.capacity > 0 ? (entry.active_count / entry.capacity) * 100 : 0;
  const color = capacityColor(entry.active_count, entry.capacity);
  return (
    <div className="bar">
      <div className="bar-row">
        <span>{entry.active_count}/{entry.capacity}</span>
        <span className="muted">{pct.toFixed(0)}%</span>
      </div>
      <div className="bar-track">
        <div className="bar-fill" style={{ width: `${Math.min(pct, 100)}%`, background: color }} />
      </div>
    </div>
  );
}

function EntryRow({ entry }: { entry: ExecutorPoolEntry }) {
  const cooldown = formatCooldown(entry.cooldown_until);
  return (
    <tr>
      <td className="mono" style={{ fontSize: "0.8rem" }}>{entry.executor_type}</td>
      <td>
        <span className={`pill ${statusPill(entry.status)}`}>{entry.status}</span>
        {cooldown && (
          <span className="pill warn" style={{ marginLeft: 4 }}>{cooldown} left</span>
        )}
      </td>
      <td style={{ minWidth: 120 }}><CapacityBar entry={entry} /></td>
      <td className={entry.failure_score > 0.5 ? "error-text" : ""}>{entry.failure_score.toFixed(2)}</td>
      <td>{(entry.success_rate * 100).toFixed(1)}%</td>
      <td>{entry.avg_latency_ms > 0 ? `${entry.avg_latency_ms.toFixed(0)}ms` : "—"}</td>
      <td>{entry.cost_per_execution > 0 ? `$${entry.cost_per_execution.toFixed(4)}` : "—"}</td>
      <td>{entry.daily_cost > 0 ? `$${entry.daily_cost.toFixed(2)}` : "—"}</td>
    </tr>
  );
}

export function ExecutorPool() {
  const [pool, setPool] = useState<ExecutorPoolStatus | null>(null);
  const [error, setError] = useState<PoolError | null>(null);
  const [loading, setLoading] = useState(true);

  const load = useCallback(() => {
    setLoading(true);
    setError(null);
    fetchExecutorPool()
      .then((res) => setPool(res.pool))
      .catch((e) => setError(poolError(e)))
      .finally(() => setLoading(false));
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  return (
    <section className="card stack">
      <div className="flex-between">
        <h2>Executor Pool</h2>
        <button onClick={load} type="button">Refresh</button>
      </div>

      {error?.type === "permission" && (
        <StateBanner title="Permission required" tone="warn"><p>{error.message}</p></StateBanner>
      )}
      {error?.type === "error" && (
        <StateBanner title="Failed to load" tone="risk"><p>{error.message}</p></StateBanner>
      )}

      {loading ? (
        <div className="loading-row"><span className="spinner" /> Loading executor pool...</div>
      ) : !pool ? (
        <EmptyState
          title="Executor pool unavailable"
          description="Could not retrieve executor pool status from the engine."
          tone="warn"
        />
      ) : pool.entries.length === 0 ? (
        <EmptyState
          title="No executors registered"
          description="Executors will appear here once configured in the engine."
          tone="info"
        />
      ) : (
        <>
          <div className="detail-summary">
            <div className="summary-tile">
              <span className="metric-label">Total active</span>
              <strong>{pool.total_active}</strong>
            </div>
            <div className="summary-tile">
              <span className="metric-label">Total capacity</span>
              <strong>{pool.total_capacity}</strong>
            </div>
            <div className="summary-tile">
              <span className="metric-label">Utilization</span>
              <strong>
                {pool.total_capacity > 0
                  ? `${((pool.total_active / pool.total_capacity) * 100).toFixed(0)}%`
                  : "0%"}
              </strong>
            </div>
            <div className="summary-tile">
              <span className="metric-label">Executor types</span>
              <strong>{pool.entries.length}</strong>
            </div>
          </div>

          <table className="table">
            <thead>
              <tr>
                <th>Type</th>
                <th>Status</th>
                <th>Active / Capacity</th>
                <th>Failure Score</th>
                <th>Success Rate</th>
                <th>Avg Latency</th>
                <th>Cost / Exec</th>
                <th>Daily Cost</th>
              </tr>
            </thead>
            <tbody>
              {pool.entries.map((entry) => (
                <EntryRow key={entry.executor_type} entry={entry} />
              ))}
            </tbody>
          </table>
        </>
      )}
    </section>
  );
}
