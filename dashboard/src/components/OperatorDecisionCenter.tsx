import { useCallback, useEffect, useState } from "react";
import { ApiError, fetchOperatorDecisions } from "@/lib/api-client";
import type { OperatorDecisionItem, OperatorDecisionQueueResponse } from "@/lib/types";
import { EmptyState } from "./EmptyState";
import { StateBanner } from "./StateBanner";

function tone(item: OperatorDecisionItem): "ok" | "warn" | "risk" | "info" {
  if (item.outcome === "conflict" || item.severity === "critical") return "risk";
  if (item.outcome === "ready" || item.severity === "warning") return "warn";
  return "info";
}

function message(error: unknown): string {
  if (error instanceof ApiError && (error.status === 401 || error.status === 403)) {
    return error.status === 403 ? "The current API key lacks dispatch:read scope." : "Decision Center requires protected local API access.";
  }
  return error instanceof Error ? error.message : "Failed to load the decision queue.";
}

export function OperatorDecisionCenter() {
  const [data, setData] = useState<OperatorDecisionQueueResponse | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const load = useCallback(async () => {
    setLoading(true); setError(null);
    try { setData(await fetchOperatorDecisions({ limit: 50 })); }
    catch (cause) { setError(message(cause)); }
    finally { setLoading(false); }
  }, []);
  useEffect(() => { void load(); }, [load]);
  if (loading) return <StateBanner title="Loading Decision Center" tone="info"><p>Recomputing bounded evidence from existing owners.</p></StateBanner>;
  if (error) return <StateBanner title="Decision Center unavailable" tone="risk"><p>{error}</p><button type="button" onClick={() => void load()}>Retry</button></StateBanner>;
  const queue = data!.queue;
  if (queue.items.length === 0) return <EmptyState title="No operator decisions" description="No fresh actionable, conflicting, expired, insufficient, or resolved evidence is currently available." />;
  return <section className="stack">
    <div className="heading-row"><div><p className="eyebrow">Derived evidence</p><h2>Decision Center</h2></div><span className="pill info">{queue.total} items</span></div>
    <p className="muted">Read-only queue. Suggested actions are not controls and require their existing guarded owners.</p>
    <div className="detail-summary"><div className="summary-tile"><span className="metric-label">Freshness bound</span><strong>{queue.maximum_freshness_seconds}s</strong></div><div className="summary-tile"><span className="metric-label">Queue hash</span><strong><code>{queue.queue_sha256.slice(0, 12)}</code></strong></div></div>
    {queue.items.map((item) => <article className="card stack" key={item.decision_id}>
      <div className="heading-row"><div><h3>{item.resource_id}</h3><p className="muted">{item.conflict_key}</p></div><span className={`pill ${tone(item)}`}>{item.outcome}</span></div>
      <div className="detail-summary"><div className="summary-tile"><span className="metric-label">Suggested action</span><strong>{item.recommended_action ?? "None"}</strong></div><div className="summary-tile"><span className="metric-label">Confidence</span><strong>{(item.confidence * 100).toFixed(0)}%</strong></div><div className="summary-tile"><span className="metric-label">Evidence</span><strong>{item.evidence_references.length}</strong></div></div>
      <p className="muted">{item.reason_codes.join(", ")}</p>
      {item.selected_source && <p className="muted">Selected source: <code>{item.selected_source.evidence_type}/{item.selected_source.evidence_id}</code></p>}
    </article>)}
  </section>;
}
