import { useEffect, useState } from "react";
import { ApiError, fetchAudit } from "@/lib/api-client";
import type { LocalAuditEvent } from "@/lib/types";
import { SearchBar } from "./SearchBar";
import { Pagination } from "./Pagination";
import { EmptyState } from "./EmptyState";
import { StateBanner } from "./StateBanner";

const PAGE_SIZE = 25;

type AuditError = {
  message: string;
  type: "permission" | "error";
};

function auditError(error: unknown): AuditError {
  if (error instanceof ApiError && (error.status === 401 || error.status === 403)) {
    return {
      message: error.status === 403
        ? "The current API key lacks audit:read scope."
        : "Audit events require protected local API access.",
      type: "permission",
    };
  }
  return {
    message: error instanceof Error ? error.message : "Failed to load audit events",
    type: "error",
  };
}

export function AuditLog() {
  const [events, setEvents] = useState<LocalAuditEvent[]>([]);
  const [error, setError] = useState<AuditError | null>(null);
  const [search, setSearch] = useState("");
  const [page, setPage] = useState(0);
  const [hasNext, setHasNext] = useState(false);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    fetchAudit({ limit: PAGE_SIZE + 1, offset: page * PAGE_SIZE, search })
      .then((response) => {
        if (cancelled) return;
        setEvents(response.events.slice(0, PAGE_SIZE));
        setHasNext(response.events.length > PAGE_SIZE);
        setError(null);
      })
      .catch((e) => {
        if (cancelled) return;
        setEvents([]);
        setHasNext(false);
        setError(auditError(e));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [page, search]);

  const resultText = loading
    ? "Loading audit events..."
    : `${events.length}${hasNext ? "+" : ""} event${events.length === 1 && !hasNext ? "" : "s"}`;

  return (
    <section className="card stack">
      <h2>Audit Log</h2>
      {error?.type === "permission" && (
        <StateBanner title="Audit log access requires audit:read" tone="warn">
          <p>{error.message}</p>
        </StateBanner>
      )}
      {error?.type === "error" && (
        <StateBanner title="Audit log unavailable" tone="risk">
          <p>{error.message}</p>
        </StateBanner>
      )}
      <SearchBar
        search={search}
        onSearchChange={(v) => { setSearch(v); setPage(0); }}
        resultCount={events.length}
        resultText={resultText}
        label="event"
        placeholder="Search audit events..."
      />
      {events.length === 0 && !error ? (
        <EmptyState
          title={search ? "No matching audit events" : "No audit events yet"}
          description={search
            ? "Try a broader actor, action, or resource search."
            : "Admin actions such as backup create, key rotate, and restore will appear here once protected mode is in use."}
          tone="info"
        />
      ) : error?.type === "permission" ? (
        <EmptyState
          title="Audit history is locked"
          description="Use a local API key with audit:read scope to inspect admin activity without exposing backup or team-admin controls."
          tone="warn"
        />
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
            {events.map((e) => (
              <tr key={String(e.audit_id)}>
                <td className="mono">{String(e.audit_id)}</td>
                <td>{String(e.created_at)}</td>
                <td>{String(e.actor)}</td>
                <td>{String(e.action)}</td>
                <td className="mono">{String(e.resource)}</td>
                <td>
                  <details>
                    <summary>View details</summary>
                    <AuditDetails details={e.details} />
                  </details>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      <Pagination page={page} hasNext={hasNext} onPageChange={setPage} />
    </section>
  );
}

function AuditDetails({ details }: { details: unknown }) {
  const data = details && typeof details === "object" ? details as Record<string, unknown> : {};
  const entries = Object.entries(data).filter(([, value]) => value !== null && value !== undefined);
  if (entries.length === 0) {
    return <p className="muted">No structured details</p>;
  }
  return (
    <div className="stack audit-detail">
      {entries.slice(0, 5).map(([key, value]) => (
        <div className="kv-row" key={key}>
          <span className="muted">{key}</span>
          <span className={typeof value === "object" ? "" : "mono"}>{formatAuditValue(value)}</span>
        </div>
      ))}
      {entries.length > 5 && <p className="muted">{entries.length - 5} more fields in raw details</p>}
      <details>
        <summary>Raw details</summary>
        <pre>{JSON.stringify(details, null, 2)}</pre>
      </details>
    </div>
  );
}

function formatAuditValue(value: unknown): string {
  if (Array.isArray(value)) return value.join(", ");
  if (value && typeof value === "object") return JSON.stringify(value);
  return String(value);
}
