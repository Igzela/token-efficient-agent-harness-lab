import { ApiError, getStoredToken } from "./api-client";

export type RegressionOutcome =
  | "pass"
  | "regression"
  | "missing_baseline"
  | "missing_best_known"
  | "incomparable"
  | "quality_failure";

export interface RegressionEvidenceReference {
  adapter_run_id: string;
  artifact_schema_version: string;
  content_sha256: string;
}

export interface RegressionMetricSummary {
  allowed_regression: number | null;
  current: number | null;
  delta: number | null;
  normalized_regression: number | null;
  reference: number | null;
  regressed: boolean;
}

export interface RegressionReportSummary {
  artifact_id: string;
  created_at: string;
  outcome: RegressionOutcome;
  reason_codes: string[];
  report_sha256: string;
  scenario_id: string;
  evidence: Partial<Record<"current" | "baseline" | "best_known", RegressionEvidenceReference>>;
  comparisons: Partial<Record<"baseline" | "best_known", Record<string, RegressionMetricSummary>>>;
}

export interface RegressionTrendPointSummary {
  artifact_id: string;
  created_at: string;
  current_metrics: Record<string, number>;
  evidence: Partial<Record<"current" | "baseline" | "best_known", RegressionEvidenceReference>>;
  outcome: RegressionOutcome;
  reason_codes: string[];
  report_sha256: string;
}

export interface RegressionTrendTransitionSummary {
  from_artifact_id: string;
  from_outcome: RegressionOutcome;
  metric_deltas: Record<string, { delta: number; direction: "improved" | "regressed" | "unchanged" }>;
  new_reason_codes: string[];
  outcome_changed: boolean;
  resolved_reason_codes: string[];
  to_artifact_id: string;
  to_outcome: RegressionOutcome;
}

export interface RegressionTrendSummary {
  point_count: number;
  points: RegressionTrendPointSummary[];
  scenario_id: string;
  transitions: RegressionTrendTransitionSummary[];
  trend_sha256: string;
}

export interface RegressionArtifactEnvelope {
  artifact_id: string;
  artifact_kind: "token_efficiency_regression_report" | "token_efficiency_regression_batch";
  content_sha256: string;
  created_at: string;
  metadata_only: true;
  read_only: true;
  registry_id: string;
  registry_sha256: string;
  report: Record<string, unknown>;
  report_schema_version: string;
  scenario_id: string | null;
  schema_version: "token_efficiency_regression_artifact.v1";
}

export interface RegressionArtifactListResponse {
  artifacts: RegressionArtifactEnvelope[];
  metadata_only: true;
  mutation_authority: "none";
  provider_calls: "disabled";
  read_only: true;
  report_only: true;
  target_repository_writes: "disabled";
}

export interface RegressionTrendResponse extends Omit<RegressionArtifactListResponse, "artifacts"> {
  trend: Record<string, unknown>;
}

const OUTCOMES = new Set<RegressionOutcome>([
  "pass",
  "regression",
  "missing_baseline",
  "missing_best_known",
  "incomparable",
  "quality_failure",
]);

function record(value: unknown): Record<string, unknown> | null {
  return value !== null && typeof value === "object" && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null;
}

function stringValue(value: unknown): string | null {
  return typeof value === "string" && value.length > 0 ? value : null;
}

function numberValue(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

function stringList(value: unknown): string[] {
  return Array.isArray(value) ? value.filter((item): item is string => typeof item === "string") : [];
}

function outcomeValue(value: unknown): RegressionOutcome | null {
  return typeof value === "string" && OUTCOMES.has(value as RegressionOutcome)
    ? value as RegressionOutcome
    : null;
}

function evidenceReference(value: unknown): RegressionEvidenceReference | null {
  const item = record(value);
  if (!item) return null;
  const adapterRunId = stringValue(item.adapter_run_id);
  const artifactSchemaVersion = stringValue(item.artifact_schema_version);
  const contentSha256 = stringValue(item.content_sha256);
  if (!adapterRunId || !artifactSchemaVersion || !contentSha256) return null;
  return {
    adapter_run_id: adapterRunId,
    artifact_schema_version: artifactSchemaVersion,
    content_sha256: contentSha256,
  };
}

function evidenceReferences(value: unknown): RegressionReportSummary["evidence"] {
  const source = record(value);
  const result: RegressionReportSummary["evidence"] = {};
  if (!source) return result;
  for (const role of ["current", "baseline", "best_known"] as const) {
    const reference = evidenceReference(source[role]);
    if (reference) result[role] = reference;
  }
  return result;
}

function metricSummary(value: unknown): RegressionMetricSummary | null {
  const item = record(value);
  if (!item || typeof item.regressed !== "boolean") return null;
  return {
    allowed_regression: numberValue(item.allowed_regression),
    current: numberValue(item.current),
    delta: numberValue(item.delta),
    normalized_regression: numberValue(item.normalized_regression),
    reference: numberValue(item.reference),
    regressed: item.regressed,
  };
}

function comparisonMetrics(value: unknown): Record<string, RegressionMetricSummary> {
  const comparison = record(value);
  const metrics = record(comparison?.metrics);
  const result: Record<string, RegressionMetricSummary> = {};
  if (!metrics) return result;
  for (const [name, rawMetric] of Object.entries(metrics)) {
    const metric = metricSummary(rawMetric);
    if (metric) result[name] = metric;
  }
  return result;
}

export function summarizeRegressionArtifact(value: unknown): RegressionReportSummary | null {
  const artifact = record(value);
  const report = record(artifact?.report);
  if (!artifact || !report) return null;
  const artifactId = stringValue(artifact.artifact_id);
  const createdAt = stringValue(artifact.created_at);
  const scenarioId = stringValue(report.scenario_id);
  const reportSha256 = stringValue(report.report_sha256);
  const outcome = outcomeValue(report.outcome);
  if (!artifactId || !createdAt || !scenarioId || !reportSha256 || !outcome) return null;
  const comparisons = record(report.comparisons);
  const summary: RegressionReportSummary = {
    artifact_id: artifactId,
    created_at: createdAt,
    outcome,
    reason_codes: stringList(report.reason_codes),
    report_sha256: reportSha256,
    scenario_id: scenarioId,
    evidence: evidenceReferences(report.evidence),
    comparisons: {},
  };
  for (const role of ["baseline", "best_known"] as const) {
    const metrics = comparisonMetrics(comparisons?.[role]);
    if (Object.keys(metrics).length > 0) summary.comparisons[role] = metrics;
  }
  return summary;
}

function trendPoint(value: unknown): RegressionTrendPointSummary | null {
  const item = record(value);
  if (!item) return null;
  const artifactId = stringValue(item.artifact_id);
  const createdAt = stringValue(item.created_at);
  const outcome = outcomeValue(item.outcome);
  const reportSha256 = stringValue(item.report_sha256);
  if (!artifactId || !createdAt || !outcome || !reportSha256) return null;
  const rawMetrics = record(item.current_metrics);
  const currentMetrics: Record<string, number> = {};
  if (rawMetrics) {
    for (const [name, rawValue] of Object.entries(rawMetrics)) {
      const metric = numberValue(rawValue);
      if (metric !== null) currentMetrics[name] = metric;
    }
  }
  return {
    artifact_id: artifactId,
    created_at: createdAt,
    current_metrics: currentMetrics,
    evidence: evidenceReferences(item.evidence),
    outcome,
    reason_codes: stringList(item.reason_codes),
    report_sha256: reportSha256,
  };
}

function trendTransition(value: unknown): RegressionTrendTransitionSummary | null {
  const item = record(value);
  if (!item) return null;
  const fromArtifactId = stringValue(item.from_artifact_id);
  const toArtifactId = stringValue(item.to_artifact_id);
  const fromOutcome = outcomeValue(item.from_outcome);
  const toOutcome = outcomeValue(item.to_outcome);
  if (!fromArtifactId || !toArtifactId || !fromOutcome || !toOutcome || typeof item.outcome_changed !== "boolean") return null;
  const rawDeltas = record(item.metric_deltas);
  const metricDeltas: RegressionTrendTransitionSummary["metric_deltas"] = {};
  if (rawDeltas) {
    for (const [name, rawDelta] of Object.entries(rawDeltas)) {
      const delta = record(rawDelta);
      const amount = numberValue(delta?.delta);
      const direction = delta?.direction;
      if (amount !== null && (direction === "improved" || direction === "regressed" || direction === "unchanged")) {
        metricDeltas[name] = { delta: amount, direction };
      }
    }
  }
  return {
    from_artifact_id: fromArtifactId,
    from_outcome: fromOutcome,
    metric_deltas: metricDeltas,
    new_reason_codes: stringList(item.new_reason_codes),
    outcome_changed: item.outcome_changed,
    resolved_reason_codes: stringList(item.resolved_reason_codes),
    to_artifact_id: toArtifactId,
    to_outcome: toOutcome,
  };
}

export function summarizeRegressionTrend(value: unknown): RegressionTrendSummary | null {
  const trend = record(value);
  if (!trend) return null;
  const scenarioId = stringValue(trend.scenario_id);
  const trendSha256 = stringValue(trend.trend_sha256);
  if (!scenarioId || !trendSha256 || !Array.isArray(trend.points) || !Array.isArray(trend.transitions)) return null;
  const points = trend.points.map(trendPoint).filter((item): item is RegressionTrendPointSummary => item !== null);
  const transitions = trend.transitions.map(trendTransition).filter((item): item is RegressionTrendTransitionSummary => item !== null);
  return {
    point_count: points.length,
    points,
    scenario_id: scenarioId,
    transitions,
    trend_sha256: trendSha256,
  };
}

function authHeaders(): Record<string, string> {
  const token = getStoredToken();
  return token ? { Authorization: `Bearer ${token}` } : {};
}

async function fetchRegressionJson<T>(url: string): Promise<T> {
  let response: Response;
  try {
    response = await fetch(url, { headers: authHeaders() });
  } catch {
    throw new ApiError(0, "Network error - is the engine running?");
  }
  if (!response.ok) {
    let message = `${response.status} ${response.statusText}`;
    let code: string | undefined;
    let body: unknown;
    try {
      body = await response.json();
      const payload = record(body);
      if (typeof payload?.error === "string") message = payload.error;
      if (typeof payload?.code === "string") code = payload.code;
    } catch {
      body = undefined;
    }
    throw new ApiError(response.status, message, code, body);
  }
  return response.json();
}

export async function fetchRegressionArtifacts(params: {
  scenario_id?: string;
  limit?: number;
} = {}): Promise<RegressionArtifactListResponse> {
  const query = new URLSearchParams();
  if (params.scenario_id) query.set("scenario_id", params.scenario_id);
  if (params.limit !== undefined) query.set("limit", String(params.limit));
  const suffix = query.toString();
  return fetchRegressionJson<RegressionArtifactListResponse>(`/api/v1/regressions${suffix ? `?${suffix}` : ""}`);
}

export async function fetchRegressionTrend(
  scenarioId: string,
  params: { limit?: number } = {},
): Promise<RegressionTrendResponse> {
  const query = new URLSearchParams();
  if (params.limit !== undefined) query.set("limit", String(params.limit));
  const suffix = query.toString();
  return fetchRegressionJson<RegressionTrendResponse>(
    `/api/v1/regressions/scenarios/${encodeURIComponent(scenarioId)}/trend${suffix ? `?${suffix}` : ""}`,
  );
}
