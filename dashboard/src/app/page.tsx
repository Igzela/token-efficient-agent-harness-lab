"use client";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  fetchDashboard,
  fetchHealth,
  fetchReady,
  getStoredToken,
  isAuthError,
} from "@/lib/api-client";
import type { LocalDashboardState } from "@/lib/types";
import { AuthPanel } from "@/components/AuthPanel";
import { AuditLog } from "@/components/AuditLog";
import { Backups } from "@/components/Backups";
import { Costs } from "@/components/Costs";
import { Dispatches } from "@/components/Dispatches";
import { Health } from "@/components/Health";
import { Metric } from "@/components/Metric";
import { Routing } from "@/components/Routing";
import { Settings } from "@/components/Settings";
import { Team } from "@/components/Team";

type Tab = "dispatches" | "routing" | "team" | "costs" | "settings" | "health" | "backups" | "audit";

const tabs: { id: Tab; label: string }[] = [
  { id: "dispatches", label: "Dispatches" },
  { id: "routing", label: "Routing" },
  { id: "team", label: "Team" },
  { id: "costs", label: "Costs" },
  { id: "settings", label: "Settings" },
  { id: "health", label: "Health" },
  { id: "backups", label: "Backups" },
  { id: "audit", label: "Audit" },
];

const emptyDashboard: LocalDashboardState = {
  schema_version: "local_dashboard.v1",
  status: "loading",
  counts: {
    api_keys: 0,
    audit_events: 0,
    dispatches: 0,
    team_members: 0,
  },
  dispatches: [],
  team: {
    api_keys: [],
    members: [],
    schema_version: "local_team.v1",
  },
  config: {},
  costs: {
    by_tier: [],
    daily: [],
    currency: "USD",
    dispatch_count: 0,
    schema_version: "local_cost_summary.v2",
    total_reserved_cost: 0,
    total_estimated_cost_usd: 0,
    total_input_tokens: 0,
    total_output_tokens: 0,
    cost_utilization: 0,
  },
  boundaries: {
    deployment: "local-only",
    docker_required: false,
    provider_transport: "stub/off",
    runtime_workers: "disabled",
    sandbox_process_execution: "disabled",
    target_repository_writes: "disabled",
  },
};

function readTabFromHash(): Tab {
  if (typeof window === "undefined") return "dispatches";
  const hash = window.location.hash.replace(/^#/, "");
  if (tabs.some((t) => t.id === hash)) return hash as Tab;
  return "dispatches";
}

export default function DashboardPage() {
  const [tab, setTab] = useState<Tab>(readTabFromHash);
  const [health, setHealth] = useState("unknown");
  const [ready, setReady] = useState("unknown");
  const [dashboard, setDashboard] = useState<LocalDashboardState>(emptyDashboard);
  const [authStatus, setAuthStatus] = useState<"ok" | "missing" | "denied" | "offline">("ok");
  const [authMessage, setAuthMessage] = useState("");
  const [reloadKey, setReloadKey] = useState(0);
  const [lastUpdated, setLastUpdated] = useState<Date | null>(null);
  const [theme, setTheme] = useState<"light" | "dark">(() => {
    if (typeof window === "undefined") return "light";
    const saved = localStorage.getItem("acp-theme");
    if (saved === "dark" || saved === "light") return saved;
    return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
  });

  useEffect(() => {
    document.documentElement.setAttribute("data-theme", theme);
    localStorage.setItem("acp-theme", theme);
  }, [theme]);

  const toggleTheme = useCallback(() => setTheme((t) => (t === "dark" ? "light" : "dark")), []);

  const syncingHash = useRef(false);
  useEffect(() => {
    syncingHash.current = true;
    window.location.hash = tab;
    syncingHash.current = false;
  }, [tab]);

  useEffect(() => {
    function onHashChange() {
      if (syncingHash.current) return;
      setTab(readTabFromHash());
    }
    window.addEventListener("hashchange", onHashChange);
    return () => window.removeEventListener("hashchange", onHashChange);
  }, []);

  const refreshAll = useCallback(() => {
    let cancelled = false;
    return Promise.allSettled([fetchHealth(), fetchReady(), fetchDashboard()]).then(
      ([healthResult, readyResult, dashboardResult]) => {
        if (cancelled) return;
        const healthOk = healthResult.status === "fulfilled";
        const dashOk = dashboardResult.status === "fulfilled";
        setHealth(healthOk ? healthResult.value.status : "offline");
        setReady(readyResult.status === "fulfilled" ? readyResult.value.status : "offline");
        if (dashOk) {
          setDashboard(dashboardResult.value);
          setAuthStatus("ok");
          setAuthMessage("");
          setLastUpdated(new Date());
        } else {
          const err = dashboardResult.status === "rejected" ? dashboardResult.reason : null;
          if (isAuthError(err)) {
            if (!getStoredToken()) {
              setAuthStatus("missing");
              setAuthMessage("This dashboard requires a local API key. Enter it below.");
            } else {
              setAuthStatus("denied");
              setAuthMessage(err.status === 403
                ? "API key accepted but lacks required scope. Check your key's scopes."
                : "API key rejected. It may be expired, revoked, or invalid.");
            }
          } else if (!healthOk) {
            setAuthStatus("offline");
            setAuthMessage("Cannot reach the engine. Is it running?");
          }
        }
      },
    );
  }, []);

  useEffect(() => {
    refreshAll();
  }, [reloadKey, refreshAll]);

  // Auto-refresh every 60 seconds, paused when tab is hidden
  useEffect(() => {
    if (authStatus !== "ok") return;
    let intervalId: ReturnType<typeof setInterval> | null = null;

    function startPolling() {
      if (intervalId) return;
      intervalId = setInterval(() => {
        if (document.visibilityState === "visible") {
          refreshAll();
        }
      }, 60_000);
    }

    function stopPolling() {
      if (intervalId) {
        clearInterval(intervalId);
        intervalId = null;
      }
    }

    function handleVisibility() {
      if (document.visibilityState === "visible") {
        refreshAll();
        startPolling();
      } else {
        stopPolling();
      }
    }

    startPolling();
    document.addEventListener("visibilitychange", handleVisibility);
    return () => {
      stopPolling();
      document.removeEventListener("visibilitychange", handleVisibility);
    };
  }, [authStatus, refreshAll]);

  const routingRows = useMemo(
    () =>
      dashboard.dispatches.map((item) => ({
        confidence: item.bundle.decision.confidence,
        fallback: item.bundle.decision.fallback_tier,
        group: `${item.bundle.analysis.task_domain}/${item.bundle.analysis.task_intent}`,
        selected: item.selected_tier,
      })),
    [dashboard.dispatches],
  );

  return (
    <main>
      <div className="shell">
        <header className="topbar">
          <div>
            <p className="eyebrow">Agent Control Plane</p>
            <h1>Operations Dashboard</h1>
          </div>
          <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
            {lastUpdated && authStatus === "ok" && (
              <span className="muted" style={{ fontSize: 12 }}>
                {lastUpdated.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}
              </span>
            )}
            {authStatus === "ok" && (
              <button onClick={() => refreshAll()} type="button" style={{ padding: "6px 10px", fontSize: 14 }} title="Refresh">
                ↻
              </button>
            )}
            <span className="pill info">Local</span>
            <button onClick={toggleTheme} type="button" style={{ padding: "6px 10px", fontSize: 14 }}>
              {theme === "dark" ? "☀" : "☾"}
            </button>
          </div>
        </header>

        {authStatus !== "ok" && (
          <AuthPanel
            status={authStatus}
            message={authMessage}
            onSaved={() => setReloadKey((k) => k + 1)}
          />
        )}

        <section className="status-strip" aria-label="Status summary">
          <Metric label="API health" value={health} tone={health === "healthy" ? "ok" : "warn"} />
          <Metric label="Readiness" value={ready} tone={ready === "ready" ? "ok" : "warn"} />
          <Metric label="Dispatches" value={dashboard.counts.dispatches.toString()} detail="persisted" />
          <Metric label="Cost" value={`$${dashboard.costs.total_reserved_cost.toFixed(3)}`} detail="reserved" />
          <Metric label="Team" value={dashboard.counts.team_members.toString()} detail={`${dashboard.counts.api_keys} keys`} />
        </section>

        <nav className="nav" aria-label="Dashboard sections">
          {tabs.map((item) => (
            <button
              aria-selected={item.id === tab}
              className="tab"
              key={item.id}
              onClick={() => setTab(item.id)}
              type="button"
            >
              {item.label}
            </button>
          ))}
        </nav>

        {tab === "dispatches" && <Dispatches dispatches={dashboard.dispatches} />}
        {tab === "routing" && <Routing rows={routingRows} />}
        {tab === "team" && (
          <Team dashboard={dashboard} refreshDashboard={(d) => setDashboard(d)} />
        )}
        {tab === "costs" && <Costs dashboard={dashboard} />}
        {tab === "settings" && <Settings dashboard={dashboard} />}
        {tab === "health" && <Health dashboard={dashboard} health={health} ready={ready} />}
        {tab === "backups" && <Backups />}
        {tab === "audit" && <AuditLog />}
      </div>
    </main>
  );
}
