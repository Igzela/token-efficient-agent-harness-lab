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
import { BoundaryBadges } from "@/components/BoundaryBadges";
import { Costs } from "@/components/Costs";
import { Dispatches } from "@/components/Dispatches";
import { Health } from "@/components/Health";
import { Metric } from "@/components/Metric";
import { Operations } from "@/components/Operations";
import { Routing } from "@/components/Routing";
import { SupervisedPatch } from "@/components/SupervisedPatch";
import { Settings } from "@/components/Settings";
import { Team } from "@/components/Team";

type Tab = "dispatches" | "routing" | "team" | "costs" | "operations" | "patches" | "settings" | "health" | "backups" | "audit";

const tabs: { id: Tab; label: string }[] = [
  { id: "dispatches", label: "Dispatches" },
  { id: "routing", label: "Routing" },
  { id: "team", label: "Team" },
  { id: "costs", label: "Costs" },
  { id: "operations", label: "Operations" },
  { id: "patches", label: "Patches" },
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
    estimated_cost_available: false,
    pricing_configured: false,
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
  const hasLocalToken = Boolean(getStoredToken());
  const setupSteps = useMemo(
    () => [
      {
        detail: health === "healthy" ? "Engine API is reachable." : "Start the Rust engine on 127.0.0.1.",
        label: "Engine reachable",
        state: health === "healthy" ? "done" : "warn",
      },
      {
        detail: ready === "ready" ? "Runtime readiness checks pass." : "Readiness endpoint is not reporting ready.",
        label: "Runtime ready",
        state: ready === "ready" ? "done" : "warn",
      },
      {
        detail: hasLocalToken
          ? "A local API key is stored for protected tabs."
          : "Needed for backups, audit history, and team administration.",
        label: "Admin key available",
        state: hasLocalToken ? "done" : "todo",
      },
      {
        detail: dashboard.counts.dispatches > 0
          ? `${dashboard.counts.dispatches} dispatch record${dashboard.counts.dispatches === 1 ? "" : "s"} persisted.`
          : "Create a noop dispatch through the API to populate history.",
        label: "First dispatch recorded",
        state: dashboard.counts.dispatches > 0 ? "done" : "todo",
      },
      {
        detail: dashboard.counts.team_members > 0 || dashboard.counts.api_keys > 0
          ? `${dashboard.counts.team_members} member${dashboard.counts.team_members === 1 ? "" : "s"}, ${dashboard.counts.api_keys} key${dashboard.counts.api_keys === 1 ? "" : "s"}.`
          : "Add a local member and scoped API key when protected mode is enabled.",
        label: "Team boundary configured",
        state: dashboard.counts.team_members > 0 || dashboard.counts.api_keys > 0 ? "done" : "todo",
      },
    ],
    [dashboard.counts.api_keys, dashboard.counts.dispatches, dashboard.counts.team_members, hasLocalToken, health, ready],
  );

  return (
    <main>
      <div className="shell">
        <header className="topbar">
          <div className="topbar-main">
            <p className="eyebrow">Agent Control Plane</p>
            <h1>Local Operations Console</h1>
            <p className="hero-copy">
              Monitor local dispatch history, team state, cost usage, audit events, and data
              operations without enabling cloud or target-repo execution paths.
            </p>
            <BoundaryBadges
              authStatus={authStatus}
              boundaries={dashboard.boundaries}
              hasToken={hasLocalToken}
            />
          </div>
          <div className="topbar-meta">
            {lastUpdated && authStatus === "ok" && (
              <span className="muted timestamp">
                {lastUpdated.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}
              </span>
            )}
            {authStatus === "ok" && (
              <button onClick={() => refreshAll()} type="button" className="topbar-btn" aria-label="Refresh dashboard data">
                Refresh
              </button>
            )}
            <button onClick={toggleTheme} type="button" className="topbar-btn" aria-label={theme === "dark" ? "Switch to light mode" : "Switch to dark mode"}>
              {theme === "dark" ? "Light" : "Dark"}
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
          <Metric label="API" value={health} detail={health === "healthy" ? "healthy" : "check engine"} tone={health === "healthy" ? "ok" : "warn"} />
          <Metric label="Ready" value={ready} detail={ready === "ready" ? "ready" : "not ready"} tone={ready === "ready" ? "ok" : "warn"} />
          <Metric label="Dispatches" value={dashboard.counts.dispatches.toString()} detail="persisted" />
          <Metric label="Cost" value={`$${dashboard.costs.total_reserved_cost.toFixed(3)}`} detail="reserved" />
          <Metric label="Team" value={dashboard.counts.team_members.toString()} detail={`${dashboard.counts.api_keys} keys`} />
        </section>

        <SetupChecklist steps={setupSteps} />

        <nav className="nav" aria-label="Dashboard sections" role="tablist">
          {tabs.map((item) => (
            <button
              aria-selected={item.id === tab}
              aria-controls={`tabpanel-${item.id}`}
              className="tab"
              id={`tab-${item.id}`}
              key={item.id}
              onClick={() => setTab(item.id)}
              onKeyDown={(e) => {
                const idx = tabs.findIndex((t) => t.id === item.id);
                if (e.key === "ArrowRight") {
                  e.preventDefault();
                  const next = tabs[(idx + 1) % tabs.length];
                  setTab(next.id);
                  document.getElementById(`tab-${next.id}`)?.focus();
                } else if (e.key === "ArrowLeft") {
                  e.preventDefault();
                  const prev = tabs[(idx - 1 + tabs.length) % tabs.length];
                  setTab(prev.id);
                  document.getElementById(`tab-${prev.id}`)?.focus();
                }
              }}
              role="tab"
              type="button"
            >
              {item.label}
            </button>
          ))}
        </nav>

        <div role="tabpanel" id={`tabpanel-${tab}`} aria-labelledby={`tab-${tab}`}>
          {tab === "dispatches" && (
            <Dispatches
              dispatches={dashboard.dispatches}
              totalDispatches={dashboard.counts.dispatches}
            />
          )}
          {tab === "routing" && <Routing rows={routingRows} />}
          {tab === "team" && (
            <Team dashboard={dashboard} refreshDashboard={(d) => setDashboard(d)} />
          )}
          {tab === "costs" && <Costs dashboard={dashboard} />}
          {tab === "operations" && <Operations />}
          {tab === "patches" && <SupervisedPatch />}
          {tab === "settings" && <Settings dashboard={dashboard} />}
          {tab === "health" && <Health dashboard={dashboard} health={health} ready={ready} />}
          {tab === "backups" && <Backups />}
          {tab === "audit" && <AuditLog />}
        </div>
      </div>
    </main>
  );
}

function SetupChecklist({
  steps,
}: {
  steps: Array<{ detail: string; label: string; state: string }>;
}) {
  const completed = steps.filter((step) => step.state === "done").length;
  return (
    <section className="setup-card" aria-label="Local setup checklist">
      <div className="setup-heading">
        <div>
          <p className="label">Setup checklist</p>
          <h2>{completed}/{steps.length} local readiness steps complete</h2>
        </div>
        <span className="pill info">local-only</span>
      </div>
      <ol className="setup-list">
        {steps.map((step) => (
          <li className={`setup-step setup-step-${step.state}`} key={step.label}>
            <span aria-hidden="true" className="setup-dot" />
            <div>
              <strong>{step.label}</strong>
              <p>{step.detail}</p>
            </div>
          </li>
        ))}
      </ol>
    </section>
  );
}
