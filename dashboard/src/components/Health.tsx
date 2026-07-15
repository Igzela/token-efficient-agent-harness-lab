import type { LocalDashboardState } from "@/lib/types";
import { TermTooltip } from "./TermTooltip";

const boundaryExplanations: Record<string, string> = {
  deployment: "Where the system runs",
  docker_required: "Whether Docker is needed",
  provider_transport: "How model calls are routed",
  runtime_workers: "Background worker processes",
  sandbox_process_execution: "Isolated process execution",
  target_repository_writes: "Whether the app can write to target repos",
};

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
      <p className="muted" style={{ fontSize: "13px" }}>
        {health === "healthy" && ready === "ready"
          ? "All systems operational. The engine API is reachable and runtime readiness checks pass."
          : health === "healthy"
            ? "Engine API is reachable but runtime readiness is not confirmed. Check scheduler and executor status."
            : "Engine API is not reachable. Start the installed runtime with: agent-control-plane"}
      </p>
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
      <h3 className="section-subhead">State Counts</h3>
      <p className="muted" style={{ fontSize: "12px" }}>These counts reflect persisted state in the local SQLite database.</p>
      <div className="stack readable-list">
        <div className="kv-row">
          <span className="muted">Dispatches</span>
          <span>{dashboard.counts.dispatches}</span>
        </div>
        <div className="kv-row">
          <span className="muted">Team Members</span>
          <span>{dashboard.counts.team_members}</span>
        </div>
        <div className="kv-row">
          <span className="muted">API Keys</span>
          <span>{dashboard.counts.api_keys}</span>
        </div>
        <div className="kv-row">
          <span className="muted">Audit Events</span>
          <span>{dashboard.counts.audit_events}</span>
        </div>
      </div>
      <h3 className="section-subhead">Provider Embedding Receipts</h3>
      <p className="muted" style={{ fontSize: "12px" }}>
        Read-only redacted receipt and failure evidence. Raw queries, memory content, vectors, and credentials are excluded.
      </p>
      {dashboard.provider_embedding_receipts.length === 0 ? (
        <p className="muted">No provider embedding receipts persisted.</p>
      ) : (
        <div className="stack readable-list">
          {dashboard.provider_embedding_receipts.map((receipt) => (
            <div className="kv-row" key={receipt.operation_id}>
              <span className="muted">
                {receipt.operation_kind} · {receipt.requested_model_id} · {receipt.dimensions}d
              </span>
              <span className="mono">
                {receipt.state} · attempt {receipt.attempt_count}
                {receipt.error_domain ? ` · ${receipt.error_domain}` : ""}
                {` · ${receipt.receipt_sha256.slice(0, 12)}`}
              </span>
            </div>
          ))}
        </div>
      )}
      <h3 className="section-subhead">Boundaries</h3>
      <div className="stack readable-list">
        {Object.entries(dashboard.boundaries).map(([key, value]) => (
          <div key={key} className="kv-row">
            <span className="muted">
              <TermTooltip term={key}>{boundaryExplanations[key] ?? key}</TermTooltip>
            </span>
            <span className="mono">{String(value)}</span>
          </div>
        ))}
      </div>
    </section>
  );
}
