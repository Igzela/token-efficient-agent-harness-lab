import { useEffect, useState } from "react";
import { fetchAudit } from "@/lib/api-client";

const PAGE_SIZE = 25;

export function AuditLog() {
  const [events, setEvents] = useState<Array<Record<string, unknown>>>([]);
  const [error, setError] = useState<string | null>(null);
  const [search, setSearch] = useState("");
  const [page, setPage] = useState(0);

  useEffect(() => {
    fetchAudit()
      .then((r) => { setEvents((r.events as Array<Record<string, unknown>>) ?? []); setError(null); })
      .catch((e) => setError(e instanceof Error ? e.message : "Failed to load audit events"));
  }, []);

  const q = search.toLowerCase();
  const filtered = q
    ? events.filter(
        (e) =>
          String(e.actor).toLowerCase().includes(q) ||
          String(e.action).toLowerCase().includes(q) ||
          String(e.resource).toLowerCase().includes(q),
      )
    : events;
  const totalPages = Math.max(1, Math.ceil(filtered.length / PAGE_SIZE));
  const pageItems = filtered.slice(page * PAGE_SIZE, (page + 1) * PAGE_SIZE);

  return (
    <section className="card stack">
      <h2>Audit Log</h2>
      {error && <p className="error-text">{error}</p>}
      <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
        <input
          placeholder="Search audit events..."
          value={search}
          onChange={(e) => { setSearch(e.target.value); setPage(0); }}
          style={{ flex: 1 }}
        />
        <span className="muted" style={{ fontSize: 12, whiteSpace: "nowrap" }}>
          {filtered.length} event{filtered.length !== 1 ? "s" : ""}
        </span>
      </div>
      {pageItems.length === 0 && !error ? (
        <p className="muted">{search ? "No matching events" : "No audit events"}</p>
      ) : (
        <table>
          <thead>
            <tr>
              <th>ID</th>
              <th>Date</th>
              <th>Actor</th>
              <th>Action</th>
              <th>Resource</th>
              <th>Details</th>
            </tr>
          </thead>
          <tbody>
            {pageItems.map((e) => (
              <tr key={String(e.audit_id)}>
                <td className="mono">{String(e.audit_id)}</td>
                <td>{String(e.created_at)}</td>
                <td>{String(e.actor)}</td>
                <td>{String(e.action)}</td>
                <td className="mono">{String(e.resource)}</td>
                <td>
                  <details>
                    <summary style={{ cursor: "pointer" }}>view</summary>
                    <pre style={{ fontSize: 11, whiteSpace: "pre-wrap", marginTop: 4 }}>
                      {JSON.stringify(e.details, null, 2)}
                    </pre>
                  </details>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      {totalPages > 1 && (
        <div style={{ display: "flex", gap: 8, justifyContent: "center", marginTop: 8 }}>
          <button onClick={() => setPage((p) => Math.max(0, p - 1))} disabled={page === 0} type="button">Prev</button>
          <span className="muted" style={{ fontSize: 12, alignSelf: "center" }}>
            Page {page + 1} of {totalPages}
          </span>
          <button onClick={() => setPage((p) => Math.min(totalPages - 1, p + 1))} disabled={page >= totalPages - 1} type="button">Next</button>
        </div>
      )}
    </section>
  );
}
