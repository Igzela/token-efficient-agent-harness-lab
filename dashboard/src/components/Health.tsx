import type { LocalDashboardState } from "@/lib/types";

export function Health({
  dashboard,
  health,
  ready,
}: {
  dashboard: LocalDashboardState;
  health: string;
  ready: string;
}) {
  return (
    <section className="card stack">
      <h2>Health</h2>
      <div className="metrics">
        <div className="metric">
          <span className="metric-label">API</span>
          <strong>{health}</strong>
          <span className={health === "healthy" ? "ok" : "warn"}>{health === "healthy" ? "ok" : "check"}</span>
        </div>
        <div className="metric">
          <span className="metric-label">Readiness</span>
          <strong>{ready}</strong>
          <span className={ready === "ready" ? "ok" : "warn"}>{ready === "ready" ? "ok" : "check"}</span>
        </div>
      </div>
      <h3 style={{ marginTop: 16 }}>State Counts</h3>
      <div className="stack" style={{ fontSize: 14 }}>
        <div style={{ display: "flex", justifyContent: "space-between" }}>
          <span className="muted">Dispatches</span>
          <span>{dashboard.counts.dispatches}</span>
        </div>
        <div style={{ display: "flex", justifyContent: "space-between" }}>
          <span className="muted">Team Members</span>
          <span>{dashboard.counts.team_members}</span>
        </div>
        <div style={{ display: "flex", justifyContent: "space-between" }}>
          <span className="muted">API Keys</span>
          <span>{dashboard.counts.api_keys}</span>
        </div>
        <div style={{ display: "flex", justifyContent: "space-between" }}>
          <span className="muted">Audit Events</span>
          <span>{dashboard.counts.audit_events}</span>
        </div>
      </div>
      <h3 style={{ marginTop: 16 }}>Boundaries</h3>
      <div className="stack" style={{ fontSize: 14 }}>
        {Object.entries(dashboard.boundaries).map(([key, value]) => (
          <div key={key} style={{ display: "flex", justifyContent: "space-between", gap: 16 }}>
            <span className="muted">{key}</span>
            <span className="mono">{String(value)}</span>
          </div>
        ))}
      </div>
    </section>
  );
}
