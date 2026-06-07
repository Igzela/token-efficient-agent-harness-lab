import { useCallback, useEffect, useState } from "react";
import { ApiError, fetchQueueRuns, fetchQueueStatus, fetchQueueTenants, pauseRun, updateRunPriority } from "@/lib/api-client";
import type { QueueRunListResponse, QueueRunSummary, QueueStatus, QueueStatusResponse, QueueTenantListResponse, TenantQueueInfo } from "@/lib/types";
import { EmptyState } from "./EmptyState";
import { StateBanner } from "./StateBanner";

type QueueError = {
  message: string;
  type: "permission" | "error";
};

function queueError(error: unknown): QueueError {
  if (error instanceof ApiError && (error.status === 401 || error.status === 403)) {
    return {
      message: error.status === 403
        ? "The current API key lacks health:read scope for queue status."
        : "Queue status requires protected local API access.",
      type: "permission",
    };
  }
  return {
    message: error instanceof Error ? error.message : "Failed to load queue status",
    type: "error",
  };
}

function capacityColor(pct: number): string {
  if (pct > 90) return "var(--risk)";
  if (pct > 75) return "var(--warn)";
  return "var(--ok)";
}

function CapacityBar({ utilization }: { utilization: number }) {
  const pct = Math.min(utilization * 100, 100);
  const color = capacityColor(pct);
  return (
    <div className="bar">
      <div className="bar-row">
        <span>{pct.toFixed(0)}%</span>
        <span className="muted">capacity</span>
      </div>
      <div className="bar-track">
        <div className="bar-fill" style={{ width: `${pct}%`, background: color }} />
      </div>
    </div>
  );
}

function BackpressureBadge({ active }: { active: boolean }) {
  return (
    <span className={`pill ${active ? "risk" : "ok"}`}>
      {active ? "Backpressure ACTIVE" : "Backpressure off"}
    </span>
  );
}

function RunRow({ run, onPriorityChange, onPause }: {
  run: QueueRunSummary;
  onPriorityChange: (runId: string, priority: number) => void;
  onPause: (runId: string) => void;
}) {
  const statusPill = run.status === "running" ? "ok" : run.status === "paused" ? "warn" : run.status === "failed" ? "risk" : "info";
  return (
    <tr>
      <td className="mono" style={{ fontSize: "0.8rem" }}>{run.run_id.slice(0, 8)}</td>
      <td>{run.workflow_id}</td>
      <td><span className={`pill ${statusPill}`}>{run.status}</span></td>
      <td>
        <select
          value={run.priority}
          onChange={(e) => onPriorityChange(run.run_id, Number(e.target.value))}
          style={{ fontSize: "0.8rem" }}
        >
          {[1, 2, 3, 4, 5, 6, 7, 8, 9, 10].map((p) => (
            <option key={p} value={p}>{p}</option>
          ))}
        </select>
      </td>
      <td>{run.tenant_id}</td>
      <td>{run.queue_position ?? "—"}</td>
      <td className="muted" style={{ fontSize: "0.75rem" }}>{run.pause_reason ?? "—"}</td>
      <td className="muted" style={{ fontSize: "0.75rem" }}>{run.degrade_mode ?? "—"}</td>
      <td>
        {run.status !== "paused" && run.status !== "completed" && run.status !== "failed" && (
          <button onClick={() => onPause(run.run_id)} type="button" style={{ fontSize: "0.75rem" }}>
            Pause
          </button>
        )}
      </td>
    </tr>
  );
}

function TenantRow({ tenant }: { tenant: TenantQueueInfo }) {
  return (
    <tr>
      <td className="mono" style={{ fontSize: "0.8rem" }}>{tenant.tenant_id}</td>
      <td>{tenant.run_count}</td>
      <td>{tenant.avg_priority.toFixed(1)}</td>
    </tr>
  );
}

export function QueueStatusComponent() {
  const [status, setStatus] = useState<QueueStatus | null>(null);
  const [runs, setRuns] = useState<QueueRunSummary[]>([]);
  const [tenants, setTenants] = useState<TenantQueueInfo[]>([]);
  const [error, setError] = useState<QueueError | null>(null);
  const [loading, setLoading] = useState(true);
  const [actionError, setActionError] = useState<string | null>(null);

  const load = useCallback(() => {
    setLoading(true);
    setError(null);
    setActionError(null);
    Promise.all([fetchQueueStatus(), fetchQueueRuns(), fetchQueueTenants()])
      .then(([statusRes, runsRes, tenantsRes]) => {
        setStatus(statusRes.queue);
        setRuns(runsRes.runs);
        setTenants(tenantsRes.tenants);
      })
      .catch((e) => setError(queueError(e)))
      .finally(() => setLoading(false));
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  const handlePriorityChange = useCallback((runId: string, priority: number) => {
    setActionError(null);
    updateRunPriority(runId, priority)
      .then((res) => {
        setRuns((prev) => prev.map((r) => (r.run_id === runId ? res.run : r)));
      })
      .catch((e) => setActionError(e instanceof Error ? e.message : "Failed to update priority"));
  }, []);

  const handlePause = useCallback((runId: string) => {
    setActionError(null);
    pauseRun(runId, "manual pause from dashboard")
      .then((res) => {
        setRuns((prev) => prev.map((r) => (r.run_id === runId ? res.run : r)));
        load();
      })
      .catch((e) => setActionError(e instanceof Error ? e.message : "Failed to pause run"));
  }, [load]);

  const pausedRuns = runs.filter((r) => r.status === "paused");

  return (
    <section className="card stack">
      <div className="flex-between">
        <h2>Queue</h2>
        <button onClick={load} type="button">Refresh</button>
      </div>

      {error?.type === "permission" && (
        <StateBanner title="Permission required" tone="warn"><p>{error.message}</p></StateBanner>
      )}
      {error?.type === "error" && (
        <StateBanner title="Failed to load" tone="risk"><p>{error.message}</p></StateBanner>
      )}
      {actionError && (
        <StateBanner title="Action failed" tone="risk"><p>{actionError}</p></StateBanner>
      )}

      {loading ? (
        <div className="loading-row"><span className="spinner" /> Loading queue status...</div>
      ) : !status ? (
        <EmptyState
          title="Queue unavailable"
          description="Could not retrieve queue status from the engine."
          tone="warn"
        />
      ) : (
        <>
          <div className="detail-summary">
            <div className="summary-tile">
              <span className="metric-label">Total Queued</span>
              <strong>{status.total_queued}</strong>
            </div>
            <div className="summary-tile">
              <span className="metric-label">Running</span>
              <strong>{status.total_running}</strong>
            </div>
            <div className="summary-tile">
              <span className="metric-label">Paused</span>
              <strong>{status.total_paused}</strong>
            </div>
            <div className="summary-tile">
              <span className="metric-label">Overdue</span>
              <strong className={status.overdue_count > 0 ? "error-text" : ""}>{status.overdue_count}</strong>
            </div>
            <div className="summary-tile">
              <span className="metric-label">Avg Priority</span>
              <strong>{status.avg_priority.toFixed(1)}</strong>
            </div>
            <div className="summary-tile">
              <span className="metric-label">Effective Concurrency</span>
              <strong>{status.effective_concurrency}</strong>
            </div>
          </div>

          <div style={{ display: "flex", gap: "1rem", alignItems: "center", flexWrap: "wrap" }}>
            <div style={{ flex: 1, minWidth: 200 }}>
              <CapacityBar utilization={status.capacity_utilization} />
            </div>
            <BackpressureBadge active={status.backpressure_active} />
          </div>

          {status.queue_config && (
            <div className="detail-summary">
              <div className="summary-tile">
                <span className="metric-label">Max Concurrent</span>
                <strong>{status.queue_config.max_concurrent}</strong>
              </div>
              <div className="summary-tile">
                <span className="metric-label">Max Queued</span>
                <strong>{status.queue_config.max_queued}</strong>
              </div>
              <div className="summary-tile">
                <span className="metric-label">Backpressure Threshold</span>
                <strong>{(status.queue_config.backpressure_activation * 100).toFixed(0)}%</strong>
              </div>
              <div className="summary-tile">
                <span className="metric-label">Backpressure Enabled</span>
                <strong>{status.queue_config.backpressure_enabled ? "Yes" : "No"}</strong>
              </div>
            </div>
          )}

          <h3>Runs in Queue</h3>
          {runs.length === 0 ? (
            <EmptyState
              title="No runs in queue"
              description="The queue is empty."
              tone="info"
            />
          ) : (
            <table className="table">
              <thead>
                <tr>
                  <th>Run ID</th>
                  <th>Workflow</th>
                  <th>Status</th>
                  <th>Priority</th>
                  <th>Tenant</th>
                  <th>Position</th>
                  <th>Pause Reason</th>
                  <th>Degrade Mode</th>
                  <th>Action</th>
                </tr>
              </thead>
              <tbody>
                {runs.map((run) => (
                  <RunRow
                    key={run.run_id}
                    run={run}
                    onPriorityChange={handlePriorityChange}
                    onPause={handlePause}
                  />
                ))}
              </tbody>
            </table>
          )}

          <h3>Tenant Breakdown</h3>
          {tenants.length === 0 ? (
            <EmptyState
              title="No tenant data"
              description="No tenant queue information available."
              tone="info"
            />
          ) : (
            <table className="table">
              <thead>
                <tr>
                  <th>Tenant ID</th>
                  <th>Run Count</th>
                  <th>Avg Priority</th>
                </tr>
              </thead>
              <tbody>
                {tenants.map((tenant) => (
                  <TenantRow key={tenant.tenant_id} tenant={tenant} />
                ))}
              </tbody>
            </table>
          )}

          {pausedRuns.length > 0 && (
            <>
              <h3>Paused Runs</h3>
              <table className="table">
                <thead>
                  <tr>
                    <th>Run ID</th>
                    <th>Workflow</th>
                    <th>Priority</th>
                    <th>Tenant</th>
                    <th>Pause Reason</th>
                    <th>Degrade Mode</th>
                  </tr>
                </thead>
                <tbody>
                  {pausedRuns.map((run) => (
                    <tr key={run.run_id}>
                      <td className="mono" style={{ fontSize: "0.8rem" }}>{run.run_id.slice(0, 8)}</td>
                      <td>{run.workflow_id}</td>
                      <td>{run.priority}</td>
                      <td>{run.tenant_id}</td>
                      <td>{run.pause_reason ?? "—"}</td>
                      <td>{run.degrade_mode ?? "—"}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </>
          )}
        </>
      )}
    </section>
  );
}
