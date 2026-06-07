import { useEffect, useState } from "react";
import { ApiError, fetchProviderHealth } from "@/lib/api-client";
import type { LocalDashboardState } from "@/lib/types";
import { EmptyState } from "./EmptyState";
import { StateBanner } from "./StateBanner";

export function Settings({ dashboard }: { dashboard: LocalDashboardState }) {
  const [providerHealth, setProviderHealth] = useState<Record<string, unknown> | null>(null);
  const [providerError, setProviderError] = useState<string | null>(null);

  useEffect(() => {
    fetchProviderHealth()
      .then((r) => { setProviderHealth(r); setProviderError(null); })
      .catch((e) => {
        if (e instanceof ApiError && (e.status === 401 || e.status === 403)) {
          setProviderError(e.status === 403
            ? "The current API key lacks health:read scope."
            : "Provider health requires local API access.");
        } else {
          setProviderError(e instanceof Error ? e.message : "Failed to load provider health");
        }
      });
  }, []);

  return (
    <section className="card stack">
      <h2>Settings</h2>
      <div className="stack readable-list">
        {Object.entries(dashboard.config).length === 0 ? (
          <EmptyState
            title="No local config overrides"
            description="This runtime is using default local settings. Config values will appear here once they are persisted in app-owned state."
            tone="info"
          />
        ) : (
          Object.entries(dashboard.config).map(([key, value]) => (
            <div key={key} className="kv-row">
              <span className="muted">{key}</span>
              <span className="mono">{String(value)}</span>
            </div>
          ))
        )}
      </div>
      <h3 className="section-subhead">Provider Health</h3>
      {providerError ? (
        <StateBanner title="Provider health unavailable" tone="warn">
          <p>{providerError}</p>
        </StateBanner>
      ) : providerHealth ? (
        <ProviderHealthSummary providerHealth={providerHealth} />
      ) : (
        <p className="muted"><span className="spinner" />Loading...</p>
      )}
    </section>
  );
}

function ProviderHealthSummary({ providerHealth }: { providerHealth: Record<string, unknown> }) {
  const status = String(providerHealth.status ?? "unknown");
  const enabled = providerHealth.enabled === true;
  const providerId = providerHealth.provider_id ? String(providerHealth.provider_id) : "none";
  const message = providerHealth.message ? String(providerHealth.message) : null;
  const tone = status === "ok" ? "ok" : status === "noop" ? "info" : "warn";

  return (
    <div className="stack">
      <StateBanner title={status === "ok" ? "Provider transport is enabled" : "Provider transport is not active"} tone={tone}>
        <p>
          {message ?? (enabled
            ? "A provider adapter is configured for this local runtime."
            : "No real provider calls are configured for this local runtime. Dispatches stay on noop or explicit opt-in paths.")}
        </p>
      </StateBanner>
      <div className="metrics">
        <div className="metric">
          <span className="metric-label">Provider</span>
          <strong>{providerId}</strong>
          <span className={tone}>{status}</span>
        </div>
        <div className="metric">
          <span className="metric-label">Execution</span>
          <strong>{enabled ? "enabled" : "off"}</strong>
          <span className={enabled ? "warn" : "ok"}>{enabled ? "explicit" : "default-safe"}</span>
        </div>
      </div>
      <details>
        <summary>Raw provider response</summary>
        <pre>{JSON.stringify(providerHealth, null, 2)}</pre>
      </details>
    </div>
  );
}
