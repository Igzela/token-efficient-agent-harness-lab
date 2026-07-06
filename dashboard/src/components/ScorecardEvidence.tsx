import { summarizeScorecardArtifact } from "@/lib/scorecard-evidence";
import type { ScorecardArtifact } from "@/lib/types";
import { EmptyState } from "./EmptyState";
import { StateBanner } from "./StateBanner";

function formatNumber(value: number | null): string {
  if (value === null) return "—";
  return new Intl.NumberFormat("en-US", { maximumFractionDigits: 3 }).format(value);
}

function formatCost(value: number | null): string {
  if (value === null) return "—";
  return `$${value.toFixed(6)}`;
}

function formatRatio(value: number | null): string {
  if (value === null) return "—";
  return `${(value * 100).toFixed(2)}%`;
}

function toneForStatus(status: string): string {
  if (status === "pass") return "ok";
  if (status === "fail" || status === "failed") return "risk";
  return "info";
}

export function ScorecardEvidence({
  artifacts,
  loading = false,
  error = null,
}: {
  artifacts: ScorecardArtifact[];
  loading?: boolean;
  error?: string | null;
}) {
  if (loading) return <div className="loading-row"><span className="spinner" /> Loading scorecard evidence...</div>;
  if (error) {
    return (
      <StateBanner title="Scorecard evidence unavailable" tone="risk">
        <p>{error}</p>
      </StateBanner>
    );
  }
  if (artifacts.length === 0) {
    return (
      <EmptyState
        title="No scorecard evidence"
        description="This read-only view appears after a terminal native workflow records a token-efficiency scorecard artifact. No raw prompts, outputs, transcripts, repository content, private paths, or secrets are shown."
        tone="info"
      />
    );
  }

  return (
    <div className="stack">
      <div className="heading-row">
        <h4>Token-efficiency scorecard evidence</h4>
        <span className="pill info">read-only metadata</span>
      </div>
      {artifacts.map((artifact) => {
        const summary = summarizeScorecardArtifact(artifact);
        return (
          <div className="subcard stack" key={summary.artifact_id}>
            <div className="heading-row">
              <span className={`pill ${toneForStatus(summary.status)}`}>{summary.status}</span>
              <span className="mono" style={{ fontSize: "0.78rem" }}>{summary.artifact_id}</span>
            </div>
            <div className="detail-summary">
              <div className="summary-tile"><span className="metric-label">Quality method</span><strong>{summary.quality_method}</strong></div>
              <div className="summary-tile"><span className="metric-label">Total tokens</span><strong>{formatNumber(summary.total_tokens)}</strong></div>
              <div className="summary-tile"><span className="metric-label">Repeated context</span><strong>{formatRatio(summary.repeated_context_ratio)}</strong></div>
              <div className="summary-tile"><span className="metric-label">Estimated cost</span><strong>{formatCost(summary.estimated_cost_usd)}</strong></div>
            </div>
            <div className="stack detail-fields">
              <div className="kv-row"><span className="muted">Input / output / context tokens</span><span>{formatNumber(summary.input_tokens)} / {formatNumber(summary.output_tokens)} / {formatNumber(summary.context_tokens)}</span></div>
              <div className="kv-row"><span className="muted">Tool calls / redundant tool calls</span><span>{formatNumber(summary.tool_call_count)} / {formatNumber(summary.redundant_tool_call_count)}</span></div>
              <div className="kv-row"><span className="muted">Retries / steps</span><span>{formatNumber(summary.retry_count)} / {formatNumber(summary.step_count)}</span></div>
              <div className="kv-row"><span className="muted">Duration</span><span>{summary.duration_ms === null ? "—" : `${formatNumber(summary.duration_ms)}ms`}</span></div>
              <div className="kv-row"><span className="muted">Created</span><span>{summary.created_at ?? "—"}</span></div>
              <div className="kv-row"><span className="muted">Redaction</span><span className="pill ok">{summary.redaction_status}</span></div>
              <div className="kv-row"><span className="muted">Artifact mode</span><span>{summary.read_only ? "read-only" : "not marked read-only"}</span></div>
            </div>
          </div>
        );
      })}
    </div>
  );
}
