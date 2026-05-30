import { useEffect, useState } from "react";
import { fetchProviderHealth } from "@/lib/api-client";
import type { LocalDashboardState } from "@/lib/types";

export function Settings({ dashboard }: { dashboard: LocalDashboardState }) {
  const [providerHealth, setProviderHealth] = useState<Record<string, unknown> | null>(null);
  const [providerError, setProviderError] = useState<string | null>(null);

  useEffect(() => {
    fetchProviderHealth()
      .then((r) => { setProviderHealth(r); setProviderError(null); })
      .catch((e) => setProviderError(e instanceof Error ? e.message : "Failed to load provider health"));
  }, []);

  return (
    <section className="card stack">
      <h2>Settings</h2>
      <div className="stack" style={{ fontSize: 14 }}>
        {Object.entries(dashboard.config).map(([key, value]) => (
          <div key={key} style={{ display: "flex", justifyContent: "space-between", gap: 16 }}>
            <span className="muted">{key}</span>
            <span className="mono">{String(value)}</span>
          </div>
        ))}
      </div>
      <h3 style={{ marginTop: 16 }}>Provider Health</h3>
      {providerError ? (
        <p className="error-text">{providerError}</p>
      ) : providerHealth ? (
        <pre style={{ fontSize: 12, whiteSpace: "pre-wrap", background: "var(--bg-subtle)", padding: 12, borderRadius: "var(--radius-sm)" }}>
          {JSON.stringify(providerHealth, null, 2)}
        </pre>
      ) : (
        <p className="muted">Loading...</p>
      )}
    </section>
  );
}
