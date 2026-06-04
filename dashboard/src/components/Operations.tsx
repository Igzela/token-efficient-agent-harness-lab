import { useEffect, useState } from "react";
import { ApiError, fetchMetrics } from "@/lib/api-client";
import type { OperationsMetrics } from "@/lib/types";
import { EmptyState } from "./EmptyState";
import { Metric } from "./Metric";
import { StateBanner } from "./StateBanner";

type OpsError = {
  message: string;
  type: "permission" | "error";
};

function opsError(error: unknown): OpsError {
  if (error instanceof ApiError && (error.status === 401 || error.status === 403)) {
    return {
      message: error.status === 403
        ? "The current API key lacks health:read scope for operations metrics."
        : "Operations metrics require protected local API access.",
      type: "permission",
    };
  }
  return {
    message: error instanceof Error ? error.message : "Failed to load operations metrics",
    type: "error",
  };
}

export function Operations() {
  const [metrics, setMetrics] = useState<OperationsMetrics | null>(null);
  const [error, setError] = useState<OpsError | null>(null);
  const [loading, setLoading] = useState(true);

  function load() {
    setLoading(true);
    fetchMetrics()
      .then((response) => {
        setMetrics(response);
        setError(null);
      })
      .catch((e) => {
        setMetrics(null);
        setError(opsError(e));
      })
      .finally(() => setLoading(false));
  }

  useEffect(() => {
    load();
  }, []);

  return (
    <section className="card stack">
      <div className="flex-between">
        <h2>Operations</h2>
        <button onClick={load} type="button">Refresh</button>
      </div>
      {error?.type === "permission" && (
        <StateBanner title="Operations metrics require health:read" tone="warn">
          <p>{error.message}</p>
        </StateBanner>
      )}
      {error?.type === "error" && (
        <StateBanner title="Operations metrics unavailable" tone="risk">
          <p>{error.message}</p>
        </StateBanner>
      )}
      {loading && !metrics ? (
        <div className="loading-row"><span className="spinner" /> Loading operations metrics...</div>
      ) : !metrics && !error ? (
        <EmptyState
          title="No operations metrics yet"
          description="Start the local engine with app-owned SQLite state to populate operational metrics."
          tone="info"
        />
      ) : metrics ? (
        <>
          <div className="status-strip" aria-label="Operations metrics">
            <Metric label="Executor" value={metrics.executor_type} detail={metrics.provider_enabled ? "provider on" : "provider off"} tone={metrics.provider_enabled ? "warn" : "ok"} />
            <Metric label="Dispatches" value={String(metrics.dispatch_count)} detail="persisted" />
            <Metric label="Backups" value={String(metrics.backup_count)} detail={metrics.latest_backup_created_at ?? "none"} tone={metrics.backup_count > 0 ? "ok" : "warn"} />
            <Metric label="Audit" value={String(metrics.audit_event_count)} detail="events" />
            <Metric label="Cost" value={`$${metrics.total_estimated_cost_usd.toFixed(3)}`} detail="estimated" />
          </div>
          <div className="grid two">
            <div className="subcard stack">
              <h3>Runtime Boundary</h3>
              <div className="kv-row"><span className="muted">Auth</span><span>{metrics.auth_required ? "required" : "off"}</span></div>
              <div className="kv-row"><span className="muted">Provider</span><span>{metrics.boundaries.provider_transport}</span></div>
              <div className="kv-row"><span className="muted">Target writes</span><span>{metrics.boundaries.target_repository_writes}</span></div>
              <div className="kv-row"><span className="muted">Workers</span><span>{metrics.boundaries.runtime_workers}</span></div>
              <div className="kv-row"><span className="muted">Deployment</span><span>{metrics.boundaries.deployment}</span></div>
            </div>
            <div className="subcard stack">
              <h3>Usage Snapshot</h3>
              <div className="kv-row"><span className="muted">Reserved cost</span><span>${metrics.total_reserved_cost.toFixed(3)}</span></div>
              <div className="kv-row"><span className="muted">Estimated cost</span><span>${metrics.total_estimated_cost_usd.toFixed(3)}</span></div>
              <div className="kv-row"><span className="muted">Input tokens</span><span>{metrics.total_input_tokens}</span></div>
              <div className="kv-row"><span className="muted">Output tokens</span><span>{metrics.total_output_tokens}</span></div>
              <div className="kv-row"><span className="muted">API keys</span><span>{metrics.api_key_count}</span></div>
            </div>
          </div>
        </>
      ) : null}
    </section>
  );
}
