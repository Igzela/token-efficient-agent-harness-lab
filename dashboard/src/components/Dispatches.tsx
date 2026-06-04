import { useEffect, useState } from "react";
import { ApiError, fetchDispatchDetail, fetchDispatches } from "@/lib/api-client";
import type { LocalDispatchHistory } from "@/lib/types";
import { EmptyState } from "./EmptyState";
import { StateBanner } from "./StateBanner";
import { SearchBar } from "./SearchBar";
import { Pagination } from "./Pagination";

const PAGE_SIZE = 25;
const noopDispatchCommand = `curl -X POST http://127.0.0.1:8080/api/v1/dispatch -H "content-type: application/json" -d '{"raw_request":"Review local docs","request_source":"manual"}'`;

export function Dispatches({
  dispatches,
  totalDispatches = dispatches.length,
}: {
  dispatches: LocalDispatchHistory[];
  totalDispatches?: number;
}) {
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [detail, setDetail] = useState<Record<string, unknown> | null>(null);
  const [loading, setLoading] = useState(false);
  const [detailError, setDetailError] = useState<string | null>(null);
  const [search, setSearch] = useState("");
  const [page, setPage] = useState(0);
  const [rows, setRows] = useState<LocalDispatchHistory[]>(dispatches.slice(0, PAGE_SIZE));
  const [hasNext, setHasNext] = useState(totalDispatches > PAGE_SIZE);
  const [listLoading, setListLoading] = useState(false);
  const [listError, setListError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setListLoading(true);
    fetchDispatches({ limit: PAGE_SIZE + 1, offset: page * PAGE_SIZE, search })
      .then((response) => {
        if (cancelled) return;
        setRows(response.dispatches.slice(0, PAGE_SIZE));
        setHasNext(response.dispatches.length > PAGE_SIZE);
        setListError(null);
      })
      .catch((error) => {
        if (cancelled) return;
        setListError(dispatchListError(error));
        setRows([]);
        setHasNext(false);
      })
      .finally(() => {
        if (!cancelled) setListLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [page, search]);

  function openDetail(id: string) {
    setSelectedId(id);
    setLoading(true);
    setDetailError(null);
    fetchDispatchDetail(id)
      .then((r) => { setDetail(r.dispatch as Record<string, unknown>); setDetailError(null); })
      .catch((e) => { setDetail(null); setDetailError(e instanceof Error ? e.message : "Failed to load"); })
      .finally(() => setLoading(false));
  }

  function closeDetail() {
    setSelectedId(null);
    setDetail(null);
  }

  if (selectedId) {
    return (
      <section className="card stack">
        <div className="heading-row">
          <button onClick={closeDetail} type="button">Back to list</button>
          <span className="mono">{selectedId}</span>
        </div>
        {loading ? (
          <p className="muted"><span className="spinner" />Loading dispatch detail...</p>
        ) : detailError ? (
          <p className="error-text">{detailError}</p>
        ) : detail ? (
          <DispatchDetail detail={detail} />
        ) : (
          <p className="muted">Dispatch not found</p>
        )}
      </section>
    );
  }

  const knownTotalPages = search ? undefined : Math.max(1, Math.ceil(totalDispatches / PAGE_SIZE));
  const resultText = listLoading
    ? "Loading dispatches..."
    : search
      ? `${rows.length}${hasNext ? "+" : ""} matching dispatch${rows.length === 1 && !hasNext ? "" : "es"}`
      : `${totalDispatches} total dispatch${totalDispatches === 1 ? "" : "es"}`;

  return (
    <section className="grid">
      <div className="table-wrap">
        <div className="table-toolbar">
          <SearchBar
            search={search}
            onSearchChange={(v) => { setSearch(v); setPage(0); }}
            resultCount={rows.length}
            resultText={resultText}
            label="result"
            placeholder="Search dispatches..."
          />
        </div>
        {listError && (
          <StateBanner title="Dispatch list unavailable" tone="risk">
            <p>{listError}</p>
          </StateBanner>
        )}
        <table>
          <thead>
            <tr>
              <th scope="col">ID</th>
              <th scope="col">Task</th>
              <th scope="col">Tier</th>
              <th scope="col">Status</th>
              <th scope="col">Risk</th>
            </tr>
          </thead>
          <tbody>
            {rows.length === 0 ? (
              <tr>
                <td colSpan={5}>
                  <EmptyState
                    title={search ? "No matching dispatches" : "No dispatch history yet"}
                    description={search
                      ? "Try searching by dispatch ID, tier, status, risk, or task text."
                      : "Dispatch records appear after a local API dispatch. The default runtime uses noop execution unless an explicit opt-in path is enabled."}
                    tone="info"
                  >
                    {!search && (
                      <div className="command-block">
                        <span className="label">Create a noop dispatch</span>
                        <code>{noopDispatchCommand}</code>
                      </div>
                    )}
                  </EmptyState>
                </td>
              </tr>
            ) : (
              rows.map((item) => (
                <tr
                  key={item.history_id}
                  onClick={() => openDetail(item.dispatch_id)}
                  onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); openDetail(item.dispatch_id); } }}
                  className="clickable-row"
                  role="button"
                  tabIndex={0}
                  aria-label={`View dispatch ${item.dispatch_id}`}
                >
                  <td className="mono">{item.dispatch_id}</td>
                  <td>{item.raw_request}</td>
                  <td>{item.selected_tier}</td>
                  <td>
                    <span className="pill info">{item.final_status}</span>
                  </td>
                  <td>
                    <span className={`pill ${item.risk_level === "low" ? "ok" : "warn"}`}>{item.risk_level}</span>
                  </td>
                </tr>
              ))
            )}
          </tbody>
        </table>
        <Pagination
          page={page}
          totalPages={knownTotalPages}
          hasNext={hasNext}
          onPageChange={setPage}
        />
      </div>
      <aside className="card stack">
        <div className="heading-row">
          <h2>Quality Gates</h2>
          <span className="pill ok">deterministic</span>
        </div>
        {dispatches.length === 0 ? (
          <EmptyState
            title="No gate records"
            description="Quality gate summaries will appear after dispatch records are present."
            tone="info"
          />
        ) : (
          <ul className="compact-list">
            {dispatches.slice(0, 5).map((d) => (
              <li key={d.history_id}>
                <span className="mono">{d.dispatch_id}</span> - {d.bundle.decision.decision_status}
              </li>
            ))}
          </ul>
        )}
      </aside>
    </section>
  );
}

function dispatchListError(error: unknown): string {
  if (error instanceof ApiError && (error.status === 401 || error.status === 403)) {
    return error.status === 403
      ? "The current API key lacks dispatch:read scope."
      : "Dispatch history requires protected local API access.";
  }
  return error instanceof Error ? error.message : "Failed to load dispatch history";
}

function DispatchDetail({ detail }: { detail: Record<string, unknown> }) {
  const bundle = detail.bundle && typeof detail.bundle === "object"
    ? detail.bundle as Record<string, unknown>
    : detail;
  const record = asRecord(bundle.record);
  const analysis = asRecord(bundle.analysis);
  const decision = asRecord(bundle.decision);
  const execution = asRecord(bundle.execution_result);
  const evaluation = asRecord(bundle.evaluation_result);
  const summary = [
    { label: "Status", value: text(record.final_status ?? detail.final_status) },
    { label: "Selected tier", value: text(decision.selected_tier ?? detail.selected_tier) },
    { label: "Risk", value: text(analysis.risk_level ?? detail.risk_level) },
    { label: "Complexity", value: text(analysis.complexity_score) },
    { label: "Executor", value: text(execution.executor_type) },
    { label: "Execution", value: text(execution.status) },
    { label: "Quality", value: text(evaluation.quality_score) },
  ].filter((item) => item.value !== "unknown");
  const sections = [
    { label: "Record", key: "record" },
    { label: "Analysis", key: "analysis" },
    { label: "Decision", key: "decision" },
    { label: "Execution", key: "execution_result" },
    { label: "Evaluation", key: "evaluation_result" },
  ];
  return (
    <div className="stack">
      <div className="detail-summary">
        {summary.map((item) => (
          <div className="summary-tile" key={item.label}>
            <span className="metric-label">{item.label}</span>
            <strong>{item.value}</strong>
          </div>
        ))}
      </div>
      {sections.map(({ label, key }) => {
        const data = bundle[key];
        if (!data || typeof data !== "object") return null;
        const obj = data as Record<string, unknown>;
        const known = knownFields[key];
        return (
          <details key={key} open>
            <summary>{label}</summary>
            {known ? (
              <div className="stack detail-fields">
                {known.map((field) => {
                  const val = obj[field];
                  if (val === undefined || val === null) return null;
                  if (Array.isArray(val) && val.length === 0) return null;
                  if (typeof val === "object") {
                    return (
                      <div key={field} className="kv-row">
                        <span className="muted">{field}</span>
                        <pre className="inline-json">
                          {JSON.stringify(val, null, 2)}
                        </pre>
                      </div>
                    );
                  }
                  return (
                    <div key={field} className="kv-row">
                      <span className="muted">{field}</span>
                      <span className={typeof val === "number" ? "" : "mono"}>{String(val)}</span>
                    </div>
                  );
                })}
              </div>
            ) : (
              <pre>
                {JSON.stringify(data, null, 2)}
              </pre>
            )}
          </details>
        );
      })}
      <details>
        <summary>Raw dispatch bundle</summary>
        <pre>{JSON.stringify(bundle, null, 2)}</pre>
      </details>
    </div>
  );
}

function asRecord(value: unknown): Record<string, unknown> {
  return value && typeof value === "object" ? value as Record<string, unknown> : {};
}

function text(value: unknown): string {
  if (value === undefined || value === null || value === "") return "unknown";
  if (typeof value === "number") return Number.isInteger(value) ? value.toString() : value.toFixed(3);
  return String(value);
}

const knownFields: Record<string, string[]> = {
  record: ["dispatch_id", "request_snapshot", "request_source", "final_status", "created_at", "completed_at", "history_id", "analysis_id", "decision_id", "execution_id", "evaluation_id"],
  analysis: ["task_domain", "task_intent", "complexity_score", "risk_level", "risk_flags", "confidence", "positive_evidence", "negative_evidence", "estimated_tokens"],
  decision: ["selected_tier", "fallback_tier", "routing_reason", "decision_status", "confidence", "budget_reservation", "execution_gates"],
  execution_result: ["executor_type", "status", "output", "input_tokens", "output_tokens", "latency_ms", "error_message"],
  evaluation_result: ["status", "quality_score", "requires_retry", "checks"],
};
