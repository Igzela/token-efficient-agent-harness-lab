import type {
  ScorecardArtifact,
  ScorecardComparisonRowSummary,
  ScorecardEvidenceSummary,
  ScorecardScenarioComparisonSummary,
} from "./types";

function asRecord(value: unknown): Record<string, unknown> {
  return value && typeof value === "object" && !Array.isArray(value) ? value as Record<string, unknown> : {};
}

function numberOrNull(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

function stringOrNull(value: unknown): string | null {
  return typeof value === "string" && value.length > 0 ? value : null;
}

export function summarizeScorecardArtifact(artifact: ScorecardArtifact): ScorecardEvidenceSummary {
  const root = asRecord(artifact);
  const scorecard = asRecord(root.scorecard);
  const derived = asRecord(scorecard.derived_metrics);
  return {
    artifact_id: stringOrNull(root.artifact_id) ?? "unknown",
    created_at: stringOrNull(root.created_at),
    read_only: root.read_only === true,
    status: stringOrNull(scorecard.status) ?? "unknown",
    quality_method: stringOrNull(scorecard.quality_method) ?? "unknown",
    redaction_status: stringOrNull(scorecard.redaction_status) ?? "unknown",
    total_tokens: numberOrNull(derived.total_tokens) ?? numberOrNull(scorecard.total_tokens),
    input_tokens: numberOrNull(scorecard.input_token_total),
    output_tokens: numberOrNull(scorecard.output_token_total),
    context_tokens: numberOrNull(scorecard.context_token_total),
    repeated_context_ratio: numberOrNull(derived.repeated_context_ratio),
    tool_call_count: numberOrNull(scorecard.tool_call_count),
    redundant_tool_call_count: numberOrNull(scorecard.redundant_tool_call_count),
    retry_count: numberOrNull(scorecard.retry_count),
    step_count: numberOrNull(scorecard.step_count),
    duration_ms: numberOrNull(scorecard.duration_ms),
    estimated_cost_usd: numberOrNull(scorecard.estimated_cost_usd),
  };
}

function summarizeComparisonRow(value: unknown): ScorecardComparisonRowSummary | null {
  const row = asRecord(value);
  const adapterRunId = stringOrNull(row.adapter_run_id);
  const runtimeKind = stringOrNull(row.runtime_kind);
  const runtimeVersion = stringOrNull(row.runtime_version);
  const mode = stringOrNull(row.mode);
  if (!adapterRunId || !runtimeKind || !runtimeVersion || !mode) return null;
  return {
    adapter_run_id: adapterRunId,
    runtime_kind: runtimeKind,
    runtime_version: runtimeVersion,
    mode,
    status: stringOrNull(row.status) ?? "unknown",
    quality_score: numberOrNull(row.quality_score),
    total_tokens: numberOrNull(row.total_tokens),
    repeated_context_ratio: numberOrNull(row.repeated_context_ratio),
    estimated_cost_usd: numberOrNull(row.estimated_cost_usd),
    duration_ms: numberOrNull(row.duration_ms),
    retry_count: numberOrNull(row.retry_count),
  };
}

export function summarizeScorecardComparison(value: unknown): ScorecardScenarioComparisonSummary | null {
  const comparison = asRecord(value);
  const scenarioId = stringOrNull(comparison.scenario_id);
  const baseline = summarizeComparisonRow(comparison.baseline);
  const candidate = summarizeComparisonRow(comparison.candidate);
  if (!scenarioId || !baseline || !candidate) return null;
  const qualityGate = asRecord(comparison.quality_gate);
  const deltas = asRecord(comparison.deltas);
  const advantages = asRecord(comparison.advantages);
  const tokenAdvantage = asRecord(advantages.token);
  const costAdvantage = asRecord(advantages.cost);
  return {
    scenario_id: scenarioId,
    baseline,
    candidate,
    quality_threshold: numberOrNull(qualityGate.threshold),
    both_qualified: qualityGate.both_qualified === true,
    deltas: {
      total_tokens: numberOrNull(deltas.total_tokens),
      repeated_context_ratio: numberOrNull(deltas.repeated_context_ratio),
      estimated_cost_usd: numberOrNull(deltas.estimated_cost_usd),
      duration_ms: numberOrNull(deltas.duration_ms),
      retry_count: numberOrNull(deltas.retry_count),
      quality_score: numberOrNull(deltas.quality_score),
    },
    token_advantage_reported: tokenAdvantage.reported === true,
    token_reduction_ratio: numberOrNull(tokenAdvantage.reduction_ratio),
    cost_advantage_reported: costAdvantage.reported === true,
    cost_reduction_usd: numberOrNull(costAdvantage.reduction_usd),
  };
}

export function hasRawTraceLeak(value: unknown): boolean {
  const blocked = new Set([
    "raw_prompt",
    "raw_output",
    "prompt",
    "output",
    "transcript",
    "repo_path",
    "repository_path",
    "private_path",
    "secret",
    "api_key",
  ]);
  function visit(node: unknown): boolean {
    if (!node || typeof node !== "object") return false;
    if (Array.isArray(node)) return node.some(visit);
    return Object.entries(node as Record<string, unknown>).some(([key, child]) => {
      const normalized = key.toLowerCase();
      return blocked.has(normalized) || normalized.includes("secret") || visit(child);
    });
  }
  return visit(value);
}
