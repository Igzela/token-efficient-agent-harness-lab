import type { LocalDashboardState } from "@/lib/types";

export function Costs({ dashboard }: { dashboard: LocalDashboardState }) {
  const c = dashboard.costs;
  const maxTier = Math.max(1, ...c.by_tier.map((t) => t.reserved_cost));
  const recentDaily = c.daily.slice(0, 7).reverse();
  const maxDaily = Math.max(1, ...recentDaily.map((d) => d.reserved_cost));
  return (
    <section className="card stack">
      <div className="heading-row">
        <h2>Cost Governance</h2>
        <span className="pill info">{c.currency}</span>
      </div>
      <div className="metrics">
        <div className="metric">
          <span className="metric-label">Reserved Budget</span>
          <strong>${c.total_reserved_cost.toFixed(4)}</strong>
          <span className="info">total</span>
        </div>
        <div className="metric">
          <span className="metric-label">Estimated Cost</span>
          <strong>${c.total_estimated_cost_usd.toFixed(4)}</strong>
          <span className="info">executor</span>
        </div>
        <div className="metric">
          <span className="metric-label">Utilization</span>
          <strong>{(c.cost_utilization * 100).toFixed(1)}%</strong>
          <span className={c.cost_utilization > 0.8 ? "warn" : "ok"}>
            {c.cost_utilization > 0.8 ? "high" : "normal"}
          </span>
        </div>
        <div className="metric">
          <span className="metric-label">Tokens</span>
          <strong>{(c.total_input_tokens + c.total_output_tokens).toLocaleString()}</strong>
          <span className="muted">{c.total_input_tokens.toLocaleString()} in / {c.total_output_tokens.toLocaleString()} out</span>
        </div>
      </div>

      <h3 style={{ marginTop: 16 }}>By Tier</h3>
      {c.by_tier.length === 0 ? (
        <p className="muted">No tier cost data yet</p>
      ) : (
        <div className="bars">
          {c.by_tier.map((t) => (
            <div className="bar" key={t.selected_tier}>
              <div style={{ display: "flex", justifyContent: "space-between", fontSize: 13 }}>
                <span>{t.selected_tier}</span>
                <span>${t.reserved_cost.toFixed(4)}</span>
              </div>
              <div className="bar-track">
                <div className="bar-fill" style={{ width: `${(t.reserved_cost / maxTier) * 100}%` }} />
              </div>
            </div>
          ))}
        </div>
      )}

      <h3 style={{ marginTop: 16 }}>Daily Trend</h3>
      {recentDaily.length === 0 ? (
        <p className="muted">No daily cost data yet</p>
      ) : (
        <div className="bars">
          {recentDaily.map((d) => (
            <div className="bar" key={d.date}>
              <div style={{ display: "flex", justifyContent: "space-between", fontSize: 13 }}>
                <span>{d.date}</span>
                <span>${d.reserved_cost.toFixed(4)}</span>
              </div>
              <div className="bar-track">
                <div className="bar-fill" style={{ width: `${(d.reserved_cost / maxDaily) * 100}%` }} />
              </div>
            </div>
          ))}
        </div>
      )}
    </section>
  );
}
