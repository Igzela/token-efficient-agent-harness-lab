"use client";

import { useEffect, useState, useCallback } from "react";
import { ApiError, fetchRegulatorState } from "@/lib/api-client";
import type { RegulatorStateResponse } from "@/lib/types";
import { EmptyState } from "./EmptyState";
import { Metric } from "./Metric";
import { StateBanner } from "./StateBanner";

type OperatorData = RegulatorStateResponse | null;
type OperatorError = { message: string; type: "permission" | "error" } | null;

function mapError(error: unknown): OperatorError {
  if (error instanceof ApiError && (error.status === 401 || error.status === 403)) {
    return {
      message: error.status === 403
        ? "Current API key lacks regulator:read scope."
        : "Operator surface requires protected local API access.",
      type: "permission",
    };
  }
  return {
    message: error instanceof Error ? error.message : "Failed to load regulator state",
    type: "error",
  };
}

function modeColor(mode: string): string {
  switch (mode) {
    case "active":
      return "var(--ok)";
    case "dry_run":
      return "var(--warn)";
    default:
      return "var(--muted)";
  }
}

function modeLabel(mode: string): string {
  switch (mode) {
    case "active":
      return "Active";
    case "dry_run":
      return "Dry Run";
    default:
      return "Disabled";
  }
}

export function OperatorSurface() {
  const [data, setData] = useState<OperatorData>(null);
  const [error, setError] = useState<OperatorError>(null);
  const [loading, setLoading] = useState(true);

  const load = useCallback(() => {
    setLoading(true);
    fetchRegulatorState()
      .then((res) => {
        setData(res);
        setError(null);
      })
      .catch((e) => {
        setData(null);
        setError(mapError(e));
      })
      .finally(() => setLoading(false));
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  return (
    <section
      style={{
        background: "var(--panel)",
        border: "1px solid var(--border)",
        borderRadius: "var(--radius)",
        padding: "1.5rem",
        display: "flex",
        flexDirection: "column",
        gap: "1.25rem",
      }}
    >
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
        <h2 style={{ margin: 0, fontSize: "1.125rem", color: "var(--ink)" }}>
          Operator Surface
        </h2>
        <button
          type="button"
          onClick={load}
          disabled={loading}
          style={{
            background: "var(--bg-subtle)",
            color: "var(--ink-subtle)",
            border: "1px solid var(--border)",
            borderRadius: "var(--radius-sm)",
            padding: "0.375rem 0.75rem",
            cursor: loading ? "not-allowed" : "pointer",
            fontSize: "0.8125rem",
            opacity: loading ? 0.6 : 1,
          }}
        >
          Refresh
        </button>
      </div>

      {loading && (
        <div
          role="status"
          aria-live="polite"
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            gap: "0.75rem",
            padding: "2rem",
            color: "var(--ink-subtle)",
          }}
        >
          <span
            style={{
              width: "1.25rem",
              height: "1.25rem",
              border: "2px solid var(--border)",
              borderTopColor: "var(--accent)",
              borderRadius: "50%",
              animation: "spin 0.8s linear infinite",
            }}
          />
          Loading regulator state...
        </div>
      )}

      {error?.type === "permission" && (
        <StateBanner title="Access restricted" tone="warn">
          <p style={{ margin: 0 }}>{error.message}</p>
        </StateBanner>
      )}

      {error?.type === "error" && (
        <div style={{ display: "flex", flexDirection: "column", gap: "0.75rem", alignItems: "center", padding: "1rem 0" }}>
          <StateBanner title="Regulator state unavailable" tone="risk">
            <p style={{ margin: 0 }}>{error.message}</p>
          </StateBanner>
          <button
            type="button"
            onClick={load}
            style={{
              background: "var(--accent)",
              color: "var(--accent-fg)",
              border: "none",
              borderRadius: "var(--radius-sm)",
              padding: "0.5rem 1rem",
              cursor: "pointer",
              fontSize: "0.8125rem",
            }}
          >
            Retry
          </button>
        </div>
      )}

      {!loading && !error && !data && (
        <EmptyState
          title="No regulator data available"
          description="The regulator state endpoint returned no data. Ensure the engine is running and the regulator is configured."
          tone="info"
        />
      )}

      {!loading && !error && data && (
        <>
          {/* Mode indicator */}
          <div
            style={{
              display: "flex",
              alignItems: "center",
              gap: "0.75rem",
              padding: "1rem",
              background: "var(--bg-subtle)",
              borderRadius: "var(--radius-sm)",
              border: "1px solid var(--border)",
            }}
          >
            <span
              style={{
                display: "inline-block",
                width: "0.75rem",
                height: "0.75rem",
                borderRadius: "50%",
                background: modeColor(data.regulator.mode),
              }}
            />
            <span style={{ color: "var(--ink)", fontWeight: 600 }}>
              Mode: {modeLabel(data.regulator.mode)}
            </span>
            <span
              style={{
                marginLeft: "auto",
                padding: "0.25rem 0.625rem",
                borderRadius: "var(--radius-sm)",
                fontSize: "0.75rem",
                fontWeight: 600,
                background: modeColor(data.regulator.mode),
                color: "var(--panel)",
              }}
            >
              {modeLabel(data.regulator.mode)}
            </span>
          </div>

          {/* Gate indicators */}
          <div
            style={{
              display: "flex",
              gap: "1rem",
              flexWrap: "wrap",
            }}
          >
            {[
              { label: "Auto-adjustment gate", enabled: data.regulator.env_gate_enabled },
              { label: "Dry-run mode", enabled: data.regulator.dry_run_enabled },
              { label: "Active mode", enabled: data.regulator.active_gate_enabled },
            ].map((gate) => (
              <div
                key={gate.label}
                style={{
                  flex: "1 1 0",
                  minWidth: "140px",
                  display: "flex",
                  alignItems: "center",
                  gap: "0.5rem",
                  padding: "0.75rem 1rem",
                  background: "var(--bg-subtle)",
                  borderRadius: "var(--radius-sm)",
                  border: "1px solid var(--border)",
                }}
              >
                <span
                  style={{
                    color: gate.enabled ? "var(--ok)" : "var(--risk)",
                    fontWeight: 700,
                    fontSize: "1rem",
                  }}
                  aria-label={gate.enabled ? "Enabled" : "Disabled"}
                >
                  {gate.enabled ? "✓" : "✗"}
                </span>
                <span style={{ color: "var(--ink-subtle)", fontSize: "0.8125rem" }}>
                  {gate.label}
                </span>
              </div>
            ))}
          </div>

          {/* Proposal counts */}
          <div style={{ display: "flex", gap: "1rem", flexWrap: "wrap" }}>
            <Metric
              label="Pending Proposals"
              value={String(data.proposals.pending_count)}
              detail="awaiting action"
              tone="info"
            />
            <Metric
              label="Active Proposals"
              value={String(data.proposals.active_count)}
              detail="in effect"
              tone="ok"
            />
          </div>

          {/* Auto-adjustment status */}
          <div
            style={{
              padding: "1rem",
              background: "var(--bg-subtle)",
              borderRadius: "var(--radius-sm)",
              border: "1px solid var(--border)",
              display: "flex",
              flexDirection: "column",
              gap: "0.5rem",
            }}
          >
            <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
              <span style={{ color: "var(--ink)", fontWeight: 600, fontSize: "0.875rem" }}>
                Auto-Adjustments
              </span>
              <span
                style={{
                  padding: "0.25rem 0.625rem",
                  borderRadius: "var(--radius-sm)",
                  fontSize: "0.75rem",
                  fontWeight: 600,
                  background: data.auto_adjustments.active_count > 0 ? "var(--ok-soft)" : "var(--bg)",
                  color: data.auto_adjustments.active_count > 0 ? "var(--ok)" : "var(--muted)",
                }}
              >
                {data.auto_adjustments.active_count} active
              </span>
            </div>
            {data.auto_adjustments.report && Object.keys(data.auto_adjustments.report).length > 0 ? (
              <pre
                style={{
                  margin: 0,
                  padding: "0.5rem",
                  background: "var(--panel)",
                  borderRadius: "var(--radius-sm)",
                  fontSize: "0.75rem",
                  color: "var(--ink-subtle)",
                  overflow: "auto",
                  maxHeight: "120px",
                  border: "1px solid var(--border)",
                }}
              >
                {JSON.stringify(data.auto_adjustments.report, null, 2)}
              </pre>
            ) : (
              <span style={{ color: "var(--muted)", fontSize: "0.8125rem" }}>
                No adjustment report available
              </span>
            )}
          </div>

          {/* Warnings */}
          {data.warnings.length > 0 && (
            <div
              role="alert"
              style={{
                padding: "1rem",
                background: "var(--warn-soft)",
                borderRadius: "var(--radius-sm)",
                border: "1px solid var(--warn)",
                display: "flex",
                flexDirection: "column",
                gap: "0.5rem",
              }}
            >
              <span style={{ fontWeight: 600, color: "var(--warn)", fontSize: "0.875rem" }}>
                Warnings ({data.warnings.length})
              </span>
              <ul style={{ margin: 0, padding: "0 0 0 1.25rem", listStyle: "disc" }}>
                {data.warnings.map((warning, idx) => (
                  <li
                    key={idx}
                    style={{
                      color: "var(--ink)",
                      fontSize: "0.8125rem",
                      lineHeight: 1.5,
                    }}
                  >
                    {warning}
                  </li>
                ))}
              </ul>
            </div>
          )}

          {/* Active routing policy */}
          <div
            style={{
              padding: "1rem",
              background: "var(--bg-subtle)",
              borderRadius: "var(--radius-sm)",
              border: "1px solid var(--border)",
            }}
          >
            <span
              style={{
                fontWeight: 600,
                color: "var(--ink)",
                fontSize: "0.875rem",
                display: "block",
                marginBottom: "0.5rem",
              }}
            >
              Active Routing Policy
            </span>
            {data.active_routing_policy ? (
              <pre
                style={{
                  margin: 0,
                  padding: "0.5rem",
                  background: "var(--panel)",
                  borderRadius: "var(--radius-sm)",
                  fontSize: "0.75rem",
                  color: "var(--ink-subtle)",
                  overflow: "auto",
                  maxHeight: "120px",
                  border: "1px solid var(--border)",
                }}
              >
                {JSON.stringify(data.active_routing_policy, null, 2)}
              </pre>
            ) : (
              <span style={{ color: "var(--muted)", fontSize: "0.8125rem" }}>
                No active policy
              </span>
            )}
          </div>

          {/* PostgreSQL indicator */}
          <div
            style={{
              padding: "0.75rem 1rem",
              background: "var(--bg-subtle)",
              borderRadius: "var(--radius-sm)",
              border: "1px solid var(--border)",
              display: "flex",
              alignItems: "center",
              gap: "0.5rem",
            }}
          >
            <span
              style={{
                display: "inline-block",
                width: "0.5rem",
                height: "0.5rem",
                borderRadius: "50%",
                background: data.regulator.pg_database_url_configured ? "var(--ok)" : "var(--muted)",
              }}
            />
            <span style={{ color: "var(--ink-subtle)", fontSize: "0.8125rem" }}>
              PG: {data.regulator.pg_database_url_configured ? "configured" : "not configured"}
            </span>
          </div>
        </>
      )}
    </section>
  );
}
