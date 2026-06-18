import { useCallback, useEffect, useState } from "react";
import { ApiError, controlScheduler, fetchSchedulerStatus } from "@/lib/api-client";
import type { SchedulerStatus as SchedulerStatusType } from "@/lib/types";
import { ConfirmDialog, type ConfirmAction } from "./ConfirmDialog";
import { EmptyState } from "./EmptyState";
import { StateBanner } from "./StateBanner";

type SchedError = {
  message: string;
  type: "permission" | "error";
};

function schedError(error: unknown): SchedError {
  if (error instanceof ApiError && (error.status === 401 || error.status === 403)) {
    return {
      message: error.status === 403
        ? "The current API key lacks health:read scope for scheduler status."
        : "Scheduler status requires protected local API access.",
      type: "permission",
    };
  }
  return {
    message: error instanceof Error ? error.message : "Failed to load scheduler status",
    type: "error",
  };
}

function formatNs(ns: number): string {
  if (ns < 1_000_000) return `${(ns / 1_000).toFixed(1)}μs`;
  if (ns < 1_000_000_000) return `${(ns / 1_000_000).toFixed(1)}ms`;
  return `${(ns / 1_000_000_000).toFixed(1)}s`;
}

export function SchedulerStatus() {
  const [status, setStatus] = useState<SchedulerStatusType | null>(null);
  const [error, setError] = useState<SchedError | null>(null);
  const [loading, setLoading] = useState(true);
  const [confirmAction, setConfirmAction] = useState<ConfirmAction>(null);
  const [mutating, setMutating] = useState(false);

  const load = useCallback(() => {
    setLoading(true);
    setError(null);
    fetchSchedulerStatus()
      .then((res) => setStatus(res.scheduler))
      .catch((e) => setError(schedError(e)))
      .finally(() => setLoading(false));
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  function handleConfirm() {
    if (!confirmAction || confirmAction.type !== "schedulerControl") return;
    const action = confirmAction.action;
    setConfirmAction(null);
    setMutating(true);
    setError(null);
    controlScheduler(action)
      .then((res) => setStatus(res.scheduler))
      .catch((e) => setError(schedError(e)))
      .finally(() => setMutating(false));
  }

  return (
    <section className="card stack">
      <div className="flex-between">
        <h2>Scheduler</h2>
        <button onClick={load} type="button">Refresh</button>
      </div>

      {error?.type === "permission" && (
        <StateBanner title="Permission required" tone="warn"><p>{error.message}</p></StateBanner>
      )}
      {error?.type === "error" && (
        <StateBanner title="Failed to load" tone="risk"><p>{error.message}</p></StateBanner>
      )}

      {loading ? (
        <div className="loading-row"><span className="spinner" /> Loading scheduler status...</div>
      ) : !status ? (
        <EmptyState
          title="Scheduler unavailable"
          description="Could not retrieve scheduler status from the engine."
          tone="warn"
        />
      ) : !status.enabled ? (
        <EmptyState
          title="Scheduler not enabled"
          description={status.message ?? "Set ACP_ENABLE_SCHEDULER=1 to enable the workflow scheduler."}
          tone="info"
        />
      ) : (
        <>
          <div className="detail-summary">
            <div className="summary-tile">
              <span className="metric-label">Running</span>
              <strong><span className={`pill ${status.running ? "ok" : "risk"}`}>{status.running ? "yes" : "no"}</span></strong>
            </div>
            <div className="summary-tile">
              <span className="metric-label">Workers</span>
              <strong>{status.worker_count ?? status.workers?.length ?? 0}</strong>
            </div>
            <div className="summary-tile">
              <span className="metric-label">Paused</span>
              <strong><span className={`pill ${status.paused ? "warn" : "ok"}`}>{status.paused ? "yes" : "no"}</span></strong>
            </div>
            <div className="summary-tile">
              <span className="metric-label">Kill</span>
              <strong><span className={`pill ${status.kill_requested ? "risk" : "ok"}`}>{status.kill_requested ? "requested" : "clear"}</span></strong>
            </div>
            <div className="summary-tile">
              <span className="metric-label">Active runs</span>
              <strong>{status.active_runs ?? 0}</strong>
            </div>
            <div className="summary-tile">
              <span className="metric-label">Tick count</span>
              <strong>{status.tick_count ?? 0}</strong>
            </div>
            <div className="summary-tile">
              <span className="metric-label">Error count</span>
              <strong className={status.error_count && status.error_count > 0 ? "error-text" : ""}>{status.error_count ?? 0}</strong>
            </div>
          </div>

          <div className="subcard stack">
            <h4>Configuration</h4>
            {status.config && (
              <>
                <div className="kv-row"><span className="muted">Executor type</span><span>{status.config.executor_type}</span></div>
                <div className="kv-row"><span className="muted">Interval</span><span>{status.config.interval_ms}ms</span></div>
                <div className="kv-row"><span className="muted">Max concurrent</span><span>{status.config.max_concurrent}</span></div>
                <div className="kv-row"><span className="muted">Lease timeout</span><span>{status.config.lease_timeout_ms}ms</span></div>
                {status.config.heartbeat_interval_sec != null && (
                  <div className="kv-row"><span className="muted">Heartbeat interval</span><span>{status.config.heartbeat_interval_sec}s</span></div>
                )}
              </>
            )}
          </div>

          <div className="subcard stack">
            <div className="flex-between">
              <h4>Worker Control</h4>
              <span className={`pill ${status.supervised_workers_enabled ? "ok" : "warn"}`}>
                supervised: {status.supervised_workers_enabled ? "enabled" : "off"}
              </span>
            </div>
            <div className="workflow-actions">
              <button type="button" onClick={() => setConfirmAction({ type: "schedulerControl", action: "pause" })} disabled={mutating}>
                {mutating ? "Working..." : "Pause"}
              </button>
              <button type="button" onClick={() => setConfirmAction({ type: "schedulerControl", action: "resume" })} disabled={mutating}>
                {mutating ? "Working..." : "Resume"}
              </button>
              <button type="button" className="risk-action" onClick={() => setConfirmAction({ type: "schedulerControl", action: "kill" })} disabled={mutating}>
                {mutating ? "Working..." : "Kill"}
              </button>
            </div>
            {status.workers && status.workers.length > 0 ? (
              <div className="mission-decision-list">
                {status.workers.map((worker) => (
                  <div className="mission-decision" key={worker.worker_id}>
                    <div className="flex-between">
                      <strong>{worker.worker_id}</strong>
                      <span className={`pill ${worker.state === "running" || worker.state === "idle" ? "ok" : worker.state === "killed" ? "risk" : "warn"}`}>{worker.state}</span>
                    </div>
                    <div className="mission-node-meta">
                      <span>{worker.tick_count} ticks</span>
                      <span>{worker.error_count} errors</span>
                      <span>{worker.last_heartbeat_at}</span>
                    </div>
                  </div>
                ))}
              </div>
            ) : (
              <p className="muted">No worker heartbeats recorded yet.</p>
            )}
          </div>

          <div className="subcard stack">
            <h4>Runtime</h4>
            {status.started_at && <div className="kv-row"><span className="muted">Started</span><span>{status.started_at}</span></div>}
            {status.last_tick_at && <div className="kv-row"><span className="muted">Last tick</span><span>{status.last_tick_at}</span></div>}
            {status.retry_count != null && <div className="kv-row"><span className="muted">Retry count</span><span>{status.retry_count}</span></div>}
            {status.total_execution_time_ns != null && (
              <div className="kv-row"><span className="muted">Total execution time</span><span>{formatNs(status.total_execution_time_ns)}</span></div>
            )}
            {status.last_error && (
              <div className="kv-row"><span className="muted">Last error</span><span className="error-text">{status.last_error}</span></div>
            )}
          </div>
        </>
      )}
      <ConfirmDialog
        action={confirmAction}
        onConfirm={handleConfirm}
        onCancel={() => setConfirmAction(null)}
      />
    </section>
  );
}
