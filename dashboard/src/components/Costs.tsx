import type { LocalDashboardState } from "@/lib/types";
import { EmptyState } from "./EmptyState";
import { StateBanner } from "./StateBanner";

export function Costs({ dashboard }: { dashboard: LocalDashboardState }) {
  const c = dashboard.costs;
  const pricingMissingWithUsage = dashboard.boundaries.provider_transport === "provider/enabled"
    && c.pricing_configured === false
    && (c.total_input_tokens + c.total_output_tokens) > 0;
  const maxTier = Math.max(1, ...c.by_tier.map((t) => t.reserved_cost));
  const recentDaily = c.daily.slice(0, 7).reverse();
  const maxDaily = Math.max(1, ...recentDaily.map((d) => d.reserved_cost));
  return (
    <section className="card stack">
      <div className="heading-row">
        <h2>Cost Governance</h2>
        <span className="pill info">{c.currency}</span>
      </div>
      {pricingMissingWithUsage && (
        <StateBanner title="Provider pricing not configured" tone="warn">
          <p>Provider token usage is tracked, but price rates are not configured.</p>
        </StateBanner>
      )}
      <div className="metrics">
        <div className="metric">
          <span className="metric-label">Reserved Budget</span>
          <strong>${c.total_reserved_cost.toFixed(4)}</strong>
          <span className="info">total</span>
        </div>
        <div className="metric">
          <span className="metric-label">Estimated Cost</span>
          <strong>{c.estimated_cost_available ? `$${c.total_estimated_cost_usd.toFixed(4)}` : "unavailable"}</strong>
          <span className={c.estimated_cost_available ? "info" : "warn"}>
            {c.pricing_configured === false ? "pricing missing" : "executor"}
          </span>
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

      <h3 className="section-subhead">By Tier</h3>
      {c.by_tier.length === 0 ? (
        <EmptyState
          title="No tier cost data yet"
          description="Cost by tier appears after dispatch records include reserved or estimated usage."
          tone="info"
        />
      ) : (
        <div className="bars">
          {c.by_tier.map((t) => (
            <div className="bar" key={t.selected_tier}>
              <div className="bar-row">
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

      <h3 className="section-subhead">Daily Trend</h3>
      {recentDaily.length === 0 ? (
        <EmptyState
          title="No daily cost trend yet"
          description="Daily cost bars will populate as local dispatch records accumulate."
          tone="info"
        />
      ) : (
        <div className="bars">
          {recentDaily.map((d) => (
            <div className="bar" key={d.date}>
              <div className="bar-row">
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
