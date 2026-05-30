import { useState } from "react";
import { fetchDispatchDetail } from "@/lib/api-client";
import type { LocalDispatchHistory } from "@/lib/types";

const PAGE_SIZE = 25;

export function Dispatches({ dispatches }: { dispatches: LocalDispatchHistory[] }) {
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [detail, setDetail] = useState<Record<string, unknown> | null>(null);
  const [loading, setLoading] = useState(false);
  const [detailError, setDetailError] = useState<string | null>(null);
  const [search, setSearch] = useState("");
  const [page, setPage] = useState(0);

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

  const q = search.toLowerCase();
  const filtered = q
    ? dispatches.filter(
        (d) =>
          d.dispatch_id.toLowerCase().includes(q) ||
          d.selected_tier.toLowerCase().includes(q) ||
          d.final_status.toLowerCase().includes(q) ||
          d.risk_level.toLowerCase().includes(q) ||
          d.raw_request.toLowerCase().includes(q),
      )
    : dispatches;
  const totalPages = Math.max(1, Math.ceil(filtered.length / PAGE_SIZE));
  const pageItems = filtered.slice(page * PAGE_SIZE, (page + 1) * PAGE_SIZE);

  if (selectedId) {
    return (
      <section className="card stack">
        <div className="heading-row">
          <button onClick={closeDetail} type="button">Back to list</button>
          <span className="mono">{selectedId}</span>
        </div>
        {loading ? (
          <p className="muted">Loading dispatch detail...</p>
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

  return (
    <section className="grid">
      <div className="table-wrap">
        <div style={{ display: "flex", gap: 8, alignItems: "center", marginBottom: 8 }}>
          <input
            placeholder="Search dispatches..."
            value={search}
            onChange={(e) => { setSearch(e.target.value); setPage(0); }}
            style={{ flex: 1 }}
          />
          <span className="muted" style={{ fontSize: 12, whiteSpace: "nowrap" }}>
            {filtered.length} result{filtered.length !== 1 ? "s" : ""}
          </span>
        </div>
        <table>
          <thead>
            <tr>
              <th>ID</th>
              <th>Task</th>
              <th>Tier</th>
              <th>Status</th>
              <th>Risk</th>
            </tr>
          </thead>
          <tbody>
            {pageItems.length === 0 ? (
              <tr>
                <td className="muted" colSpan={5}>
                  {search ? "No matching dispatches" : "No local dispatch history"}
                </td>
              </tr>
            ) : (
              pageItems.map((item) => (
                <tr
                  key={item.history_id}
                  onClick={() => openDetail(item.dispatch_id)}
                  style={{ cursor: "pointer" }}
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
        {totalPages > 1 && (
          <div style={{ display: "flex", gap: 8, justifyContent: "center", marginTop: 8 }}>
            <button onClick={() => setPage((p) => Math.max(0, p - 1))} disabled={page === 0} type="button">Prev</button>
            <span className="muted" style={{ fontSize: 12, alignSelf: "center" }}>
              Page {page + 1} of {totalPages}
            </span>
            <button onClick={() => setPage((p) => Math.min(totalPages - 1, p + 1))} disabled={page >= totalPages - 1} type="button">Next</button>
          </div>
        )}
      </div>
      <aside className="card stack">
        <div className="heading-row">
          <h2>Quality Gates</h2>
          <span className="pill ok">deterministic</span>
        </div>
        {dispatches.length === 0 ? (
          <p className="muted">No gate records</p>
        ) : (
          <ul className="stack" style={{ fontSize: 13, paddingLeft: 16 }}>
            {dispatches.slice(0, 5).map((d) => (
              <li key={d.history_id}>
                <span className="mono">{d.dispatch_id}</span> — {d.bundle.decision.decision_status}
              </li>
            ))}
          </ul>
        )}
      </aside>
    </section>
  );
}

function DispatchDetail({ detail }: { detail: Record<string, unknown> }) {
  const sections = [
    { label: "Record", key: "record" },
    { label: "Analysis", key: "analysis" },
    { label: "Decision", key: "decision" },
    { label: "Execution", key: "execution_result" },
    { label: "Evaluation", key: "evaluation_result" },
  ];
  return (
    <div className="stack">
      {sections.map(({ label, key }) => {
        const data = detail[key];
        if (!data) return null;
        return (
          <details key={key} open>
            <summary style={{ cursor: "pointer", fontWeight: 600, marginBottom: 8 }}>{label}</summary>
            <pre style={{ fontSize: 12, whiteSpace: "pre-wrap", background: "var(--bg-subtle)", padding: 12, borderRadius: "var(--radius-sm)" }}>
              {JSON.stringify(data, null, 2)}
            </pre>
          </details>
        );
      })}
    </div>
  );
}
