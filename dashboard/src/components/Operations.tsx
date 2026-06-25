import { useEffect, useState } from "react";
import { ApiError, fetchCircuitBreakerStatus, fetchMetrics, fetchObservabilityMetrics, fetchStorageIntegrity } from "@/lib/api-client";
import type { CircuitBreakerStatusResponse, ObservabilityMetricsResponse, OperationsMetrics, StorageIntegrityResponse } from "@/lib/types";
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
  const [cbStatus, setCbStatus] = useState<CircuitBreakerStatusResponse | null>(null);
  const [obsMetrics, setObsMetrics] = useState<ObservabilityMetricsResponse | null>(null);
  const [integrity, setIntegrity] = useState<StorageIntegrityResponse | null>(null);
  const [integrityBusy, setIntegrityBusy] = useState(false);

  function load() {
    setLoading(true);
    Promise.all([
      fetchMetrics(),
      fetchCircuitBreakerStatus(),
      fetchObservabilityMetrics(),
    ])
      .then(([response, cb, obs]) => {
        setMetrics(response);
        setCbStatus(cb);
        setObsMetrics(obs);
        setError(null);
      })
      .catch((e) => {
        setMetrics(null);
        setError(opsError(e));
      })
      .finally(() => setLoading(false));
  }

  async function handleIntegrityCheck() {
    setIntegrityBusy(true);
    try {
      const result = await fetchStorageIntegrity();
      setIntegrity(result);
    } catch {
      setError({ message: "Storage integrity check failed", type: "error" });
    } finally {
      setIntegrityBusy(false);
    }
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
          {metrics.provider_enabled && !metrics.pricing_configured && (metrics.total_input_tokens + metrics.total_output_tokens) > 0 && (
            <StateBanner title="Provider pricing not configured" tone="warn">
              <p>Provider token usage is tracked, but price rates are not configured.</p>
            </StateBanner>
          )}
          <div className="status-strip" aria-label="Operations metrics">
            <Metric label="Executor" value={metrics.executor_type} detail={metrics.provider_enabled ? "provider on" : "provider off"} tone={metrics.provider_enabled ? "warn" : "ok"} />
            <Metric label="Dispatches" value={String(metrics.dispatch_count)} detail="persisted" />
            <Metric label="Backups" value={String(metrics.backup_count)} detail={metrics.latest_backup_created_at ?? "none"} tone={metrics.backup_count > 0 ? "ok" : "warn"} />
            <Metric label="Audit" value={String(metrics.audit_event_count)} detail="events" />
            <Metric
              label="Cost"
              value={metrics.estimated_cost_available ? `$${metrics.total_estimated_cost_usd.toFixed(3)}` : "unavailable"}
              detail={metrics.pricing_configured ? "estimated" : "pricing missing"}
              tone={metrics.pricing_configured ? "ok" : "warn"}
            />
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
              <div className="kv-row">
                <span className="muted">Estimated cost</span>
                <span>{metrics.estimated_cost_available ? `$${metrics.total_estimated_cost_usd.toFixed(3)}` : "unavailable"}</span>
              </div>
              <div className="kv-row"><span className="muted">Pricing config</span><span>{metrics.pricing_configured ? "configured" : "missing"}</span></div>
              <div className="kv-row"><span className="muted">Input tokens</span><span>{metrics.total_input_tokens}</span></div>
              <div className="kv-row"><span className="muted">Output tokens</span><span>{metrics.total_output_tokens}</span></div>
              <div className="kv-row"><span className="muted">API keys</span><span>{metrics.api_key_count}</span></div>
            </div>
          </div>

          {obsMetrics && obsMetrics.total_requests > 0 && (
            <div className="subcard stack">
              <h3>Request Metrics</h3>
              <div className="status-strip" aria-label="Request metrics">
                <Metric label="Requests" value={String(obsMetrics.total_requests)} detail="total" />
                <Metric label="Errors" value={String(obsMetrics.error_count)} detail={obsMetrics.error_count > 0 ? "failing" : "none"} tone={obsMetrics.error_count > 0 ? "warn" : "ok"} />
                <Metric label="Avg Duration" value={`${obsMetrics.avg_duration_ms.toFixed(1)}ms`} detail="average" />
              </div>
              {obsMetrics.recent_metrics.length > 0 && (
                <table>
                  <thead>
                    <tr>
                      <th>Component</th>
                      <th>Action</th>
                      <th>Duration</th>
                      <th>Status</th>
                      <th>Timestamp</th>
                    </tr>
                  </thead>
                  <tbody>
                    {obsMetrics.recent_metrics.slice(0, 30).map((m) => (
                      <tr key={m.request_id}>
                        <td>{m.component}</td>
                        <td>{m.action}</td>
                        <td>{m.duration_ms.toFixed(1)}ms</td>
                        <td>
                          <span style={{
                            color: m.status === "error" || m.status.startsWith("5") ? "var(--color-risk, #e74c3c)" : m.status.startsWith("4") ? "var(--color-warn, #f39c12)" : "var(--color-ok, #27ae60)",
                            fontWeight: 600,
                          }}>
                            {m.status}
                          </span>
                        </td>
                        <td>{new Date(m.timestamp * 1000).toISOString().slice(11, 19)}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              )}
              {obsMetrics.snapshots.length > 0 && (
                <details>
                  <summary style={{ cursor: "pointer", fontSize: "0.85rem" }}>
                    Metric Snapshots ({obsMetrics.snapshots.length})
                  </summary>
                  <table>
                    <thead>
                      <tr><th>Name</th><th>Value</th><th>Labels</th></tr>
                    </thead>
                    <tbody>
                      {obsMetrics.snapshots.map((s, i) => (
                        <tr key={i}>
                          <td>{s.name}</td>
                          <td>{s.value.toFixed(2)}</td>
                          <td>{Object.entries(s.labels).map(([k, v]) => `${k}=${v}`).join(", ")}</td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </details>
              )}
            </div>
          )}

          <div className="subcard stack">
            <h3>Storage Integrity</h3>
            <button onClick={handleIntegrityCheck} disabled={integrityBusy} type="button">
              {integrityBusy ? "Checking..." : integrity ? "Re-check" : "Run Integrity Check"}
            </button>
            {integrity && (
              <div style={{ marginTop: "8px" }}>
                <div className="kv-row"><span className="muted">Status</span><span style={{ fontWeight: 600, color: integrity.integrity.status === "ok" ? "var(--color-ok, #27ae60)" : "var(--color-risk, #e74c3c)" }}>{integrity.integrity.status}</span></div>
                <div className="kv-row"><span className="muted">Schema</span><span>v{integrity.integrity.schema_version}</span></div>
                <details>
                  <summary style={{ cursor: "pointer", fontSize: "0.85rem" }}>
                    Tables ({integrity.integrity.tables.length})
                  </summary>
                  <table>
                    <thead>
                      <tr><th>Table</th><th>Rows</th><th>Status</th></tr>
                    </thead>
                    <tbody>
                      {integrity.integrity.tables.map((t) => (
                        <tr key={t.name}>
                          <td>{t.name}</td>
                          <td>{t.row_count}</td>
                          <td>
                            <span style={{
                              color: t.status === "ok" ? "var(--color-ok, #27ae60)" : "var(--color-risk, #e74c3c)",
                              fontWeight: 600,
                            }}>
                              {t.status}
                            </span>
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </details>
              </div>
            )}
          </div>

          {cbStatus && cbStatus.total_breakers > 0 && (
            <div className="subcard stack">
              <h3>Circuit Breakers</h3>
              <div className="status-strip" aria-label="Circuit breaker summary">
                <Metric label="Total" value={String(cbStatus.total_breakers)} detail="breakers" />
                <Metric label="Open" value={String(cbStatus.open)} detail={cbStatus.open > 0 ? "failing" : "none"} tone={cbStatus.open > 0 ? "warn" : "ok"} />
                <Metric label="Half-Open" value={String(cbStatus.half_open)} detail="recovering" tone={cbStatus.half_open > 0 ? "warn" : "ok"} />
                <Metric label="Closed" value={String(cbStatus.closed)} detail="normal" tone="ok" />
              </div>
              {cbStatus.breakers.some((b) => b.state !== "Closed") && (
                <table>
                  <thead>
                    <tr>
                      <th>Name</th>
                      <th>State</th>
                      <th>Failures</th>
                      <th>Total</th>
                      <th>Last Failure</th>
                    </tr>
                  </thead>
                  <tbody>
                    {cbStatus.breakers.filter((b) => b.state !== "Closed").map((b) => (
                      <tr key={b.name}>
                        <td>{b.name}</td>
                        <td>
                          <span style={{
                            color: b.state === "Open" ? "var(--color-risk, #e74c3c)" : b.state === "HalfOpen" ? "var(--color-warn, #f39c12)" : "var(--color-ok, #27ae60)",
                            fontWeight: 600,
                          }}>
                            {b.state}
                          </span>
                        </td>
                        <td>{b.failure_count}/{b.failure_threshold}</td>
                        <td>{b.total_calls}</td>
                        <td>{b.last_failure_at ?? "n/a"}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              )}
            </div>
          )}
        </>
      ) : null}
    </section>
  );
}
