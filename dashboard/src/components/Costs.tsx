import { useState } from "react";
import { fetchCostDetails } from "@/lib/api-client";
import type { LocalDashboardState, LocalDispatchCostDetail } from "@/lib/types";
import { EmptyState } from "./EmptyState";
import { StateBanner } from "./StateBanner";

export function Costs({ dashboard }: { dashboard: LocalDashboardState }) {
  const [costDetails, setCostDetails] = useState<LocalDispatchCostDetail | null>(null);
  const [detailsBusy, setDetailsBusy] = useState(false);
  const c = dashboard.costs;
  const pricingMissingWithUsage = dashboard.boundaries.provider_transport === "provider/enabled"
    && c.pricing_configured === false
    && (c.total_input_tokens + c.total_output_tokens) > 0;
  const maxTier = Math.max(1, ...c.by_tier.map((t) => t.reserved_cost));
  const recentDaily = c.daily.slice(0, 7).reverse();
  const maxDaily = Math.max(1, ...recentDaily.map((d) => d.reserved_cost));

  async function handleShowDetails() {
    if (costDetails) { setCostDetails(null); return; }
    setDetailsBusy(true);
    try {
      setCostDetails(await fetchCostDetails({ limit: 50 }));
    } catch {
      setCostDetails(null);
    } finally {
      setDetailsBusy(false);
    }
  }

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

      <h3 className="section-subhead">Per-Dispatch Details</h3>
      <button onClick={handleShowDetails} disabled={detailsBusy} type="button">
        {detailsBusy ? "Loading..." : costDetails ? "Hide Details" : "Show Details"}
      </button>
      {costDetails && costDetails.dispatches.length > 0 && (
        <table>
          <thead>
            <tr>
              <th>Dispatch</th>
              <th>Tier</th>
              <th>Tokens</th>
              <th>Cost</th>
              <th>Executor</th>
              <th>Latency</th>
            </tr>
          </thead>
          <tbody>
            {costDetails.dispatches.map((d) => (
              <tr key={d.history_id}>
                <td className="mono">{d.dispatch_id.slice(0, 12)}</td>
                <td>{d.selected_tier}</td>
                <td>{d.input_tokens + d.output_tokens}</td>
                <td>${d.estimated_cost_usd.toFixed(4)}</td>
                <td>{d.executor_type}</td>
                <td>{d.latency_ms != null ? `${d.latency_ms}ms` : "n/a"}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </section>
  );
}
