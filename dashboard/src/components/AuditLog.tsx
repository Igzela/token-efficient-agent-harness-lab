import { useEffect, useState } from "react";
import { fetchAudit } from "@/lib/api-client";
import { usePaginatedSearch } from "@/lib/hooks";
import { SearchBar } from "./SearchBar";
import { Pagination } from "./Pagination";

export function AuditLog() {
  const [events, setEvents] = useState<Array<Record<string, unknown>>>([]);
  const [error, setError] = useState<string | null>(null);
  const { search, setSearch, page, setPage, filtered, pageItems, totalPages } =
    usePaginatedSearch(events, ["actor", "action", "resource"]);

  useEffect(() => {
    fetchAudit()
      .then((r) => { setEvents((r.events as Array<Record<string, unknown>>) ?? []); setError(null); })
      .catch((e) => setError(e instanceof Error ? e.message : "Failed to load audit events"));
  }, []);

  return (
    <section className="card stack">
      <h2>Audit Log</h2>
      {error && <p className="error-text">{error}</p>}
      <SearchBar
        search={search}
        onSearchChange={(v) => { setSearch(v); setPage(0); }}
        resultCount={filtered.length}
        label="event"
        placeholder="Search audit events..."
      />
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
      <Pagination page={page} totalPages={totalPages} onPageChange={setPage} />
    </section>
  );
}
