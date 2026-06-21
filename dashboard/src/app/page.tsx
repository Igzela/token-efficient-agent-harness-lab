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
import { DynamicRegulator } from "@/components/DynamicRegulator";
import { Health } from "@/components/Health";
import { Metric } from "@/components/Metric";
import { MissionControl } from "@/components/MissionControl";
import { Operations } from "@/components/Operations";
import { Routing } from "@/components/Routing";
import { DecisionLog } from "@/components/DecisionLog";
import { ExecutorPool } from "@/components/ExecutorPool";
import { QueueStatusComponent } from "@/components/QueueStatus";
import { RuntimeGates } from "@/components/RuntimeGates";
import { SchedulerStatus } from "@/components/SchedulerStatus";
import { SupervisedPatch } from "@/components/SupervisedPatch";
import { Settings } from "@/components/Settings";
import { Team } from "@/components/Team";
import { TabGroup, type TabGroupDef } from "@/components/TabGroup";
import { OperatorSurface } from "@/components/OperatorSurface";
import { WorkflowRuns } from "@/components/WorkflowRuns";

type Tab = "mission" | "dispatches" | "routing" | "regulator" | "operator" | "decisions" | "team" | "costs" | "operations" | "runs" | "patches" | "scheduler" | "pool" | "queue" | "settings" | "health" | "backups" | "audit";
type AuthStatus = "ok" | "missing" | "denied" | "offline";
type SetupStep = {
  detail: string;
  label: string;
  state: "done" | "todo" | "warn";
};

const allTabs: { id: Tab; label: string }[] = [
  { id: "mission", label: "Mission Control" },
  { id: "dispatches", label: "Dispatches" },
  { id: "routing", label: "Routing" },
  { id: "regulator", label: "Regulator" },
  { id: "operator", label: "Operator" },
  { id: "decisions", label: "Decisions" },
  { id: "team", label: "Team" },
  { id: "costs", label: "Costs" },
  { id: "operations", label: "Operations" },
  { id: "runs", label: "Runs" },
  { id: "patches", label: "Patches" },
  { id: "scheduler", label: "Scheduler" },
  { id: "pool", label: "Pool" },
  { id: "queue", label: "Queue" },
  { id: "settings", label: "Settings" },
  { id: "health", label: "Health" },
  { id: "backups", label: "Backups" },
  { id: "audit", label: "Audit" },
];

const tabGroups: TabGroupDef[] = [
  {
    label: "Work",
    tabs: [
      { id: "mission", label: "Tasks" },
      { id: "runs", label: "Runs" },
      { id: "patches", label: "Outputs" },
    ],
  },
  {
    label: "Activity",
    tabs: [
      { id: "dispatches", label: "Dispatches" },
      { id: "decisions", label: "Decisions" },
      { id: "costs", label: "Costs" },
    ],
    collapsible: true,
    defaultCollapsed: true,
  },
  {
    label: "Operations",
    tabs: [
      { id: "operations", label: "Overview" },
      { id: "scheduler", label: "Scheduler" },
      { id: "pool", label: "Pool" },
      { id: "queue", label: "Queue" },
      { id: "routing", label: "Routing" },
      { id: "regulator", label: "Regulator" },
      { id: "operator", label: "Operator" },
    ],
    collapsible: true,
    defaultCollapsed: true,
  },
  {
    label: "Admin",
    tabs: [
      { id: "team", label: "Team" },
      { id: "settings", label: "Settings" },
      { id: "health", label: "Health" },
      { id: "backups", label: "Backups" },
      { id: "audit", label: "Audit" },
    ],
    collapsible: true,
    defaultCollapsed: true,
  },
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
  cli: {
    enabled: false,
    claude_code: false,
    codex: false,
  },
};

function readTabFromHash(): Tab {
  if (typeof window === "undefined") return "mission";
  const hash = window.location.hash.replace(/^#/, "");
  if (allTabs.some((t) => t.id === hash)) return hash as Tab;
  return "mission";
}

function localAccessStep(authStatus: AuthStatus, hasLocalToken: boolean): SetupStep {
  if (authStatus === "ok") {
    return {
      detail: hasLocalToken
        ? "A local API key is stored for protected tabs."
        : "Open local mode; no API key is required.",
      label: "Local access ready",
      state: "done",
    };
  }
  if (authStatus === "missing") {
    return {
      detail: "Protected mode requires ACP_ADMIN_API_KEY and a matching key stored in this browser.",
      label: "Local access ready",
      state: "todo",
    };
  }
  return {
    detail: authStatus === "denied"
      ? "The stored API key was rejected or lacks the required scope."
      : "Start the local runtime before checking authentication.",
    label: "Local access ready",
    state: "warn",
  };
}

export default function DashboardPage() {
  const [tab, setTab] = useState<Tab>(readTabFromHash);
  const [mobileNavOpen, setMobileNavOpen] = useState(false);
  const [health, setHealth] = useState("unknown");
  const [ready, setReady] = useState("unknown");
  const [dashboard, setDashboard] = useState<LocalDashboardState>(emptyDashboard);
  const [authStatus, setAuthStatus] = useState<AuthStatus>("ok");
  const [authMessage, setAuthMessage] = useState("");
  const [reloadKey, setReloadKey] = useState(0);
  const [lastUpdated, setLastUpdated] = useState<Date | null>(null);
  const [theme, setTheme] = useState<"light" | "dark">("light");
  const themeInitialized = useRef(false);

  useEffect(() => {
    if (!themeInitialized.current) {
      themeInitialized.current = true;
      const saved = localStorage.getItem("acp-theme");
      const initialTheme = saved === "dark" || saved === "light"
        ? saved
        : window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
      document.documentElement.setAttribute("data-theme", initialTheme);
      if (initialTheme !== theme) setTheme(initialTheme);
      return;
    }
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
      setMobileNavOpen(false);
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
  const setupSteps = useMemo<SetupStep[]>(
    () => [
      {
        detail: health === "healthy"
          ? "Engine API is reachable."
          : "Start the installed runtime with: agent-control-plane",
        label: "Engine reachable",
        state: health === "healthy" ? "done" : "warn",
      },
      {
        detail: ready === "ready"
          ? "Runtime readiness checks pass."
          : "Review scheduler and executor status under Operations.",
        label: "Runtime ready",
        state: ready === "ready" ? "done" : "warn",
      },
      localAccessStep(authStatus, hasLocalToken),
      {
        detail: dashboard.counts.dispatches > 0
          ? `${dashboard.counts.dispatches} dispatch record${dashboard.counts.dispatches === 1 ? "" : "s"} persisted.`
          : "Create your first task in Tasks.",
        label: "First task recorded",
        state: dashboard.counts.dispatches > 0 ? "done" : "todo",
      },
      {
        detail: dashboard.counts.team_members > 0 || dashboard.counts.api_keys > 0
          ? `${dashboard.counts.team_members} member${dashboard.counts.team_members === 1 ? "" : "s"}, ${dashboard.counts.api_keys} key${dashboard.counts.api_keys === 1 ? "" : "s"}.`
          : authStatus === "ok" && !hasLocalToken
            ? "Optional for solo local use; configure members and keys in Team before sharing access."
            : "Configure members and API keys in Team.",
        label: "Team boundary",
        state: dashboard.counts.team_members > 0 || dashboard.counts.api_keys > 0 || (authStatus === "ok" && !hasLocalToken) ? "done" : "todo",
      },
    ],
    [authStatus, dashboard.counts.api_keys, dashboard.counts.dispatches, dashboard.counts.team_members, hasLocalToken, health, ready],
  );

  return (
    <main>
      <div className="shell ops-shell">
        <aside className={`ops-sidebar${mobileNavOpen ? " nav-open" : ""}`} aria-label="Dashboard navigation">
          <div className="ops-brand">
            <span className="ops-brand-mark" aria-hidden="true" />
            <div>
              <p className="eyebrow">ACP</p>
              <strong>Agent Control</strong>
            </div>
            <button
              aria-controls="dashboard-navigation"
              aria-expanded={mobileNavOpen}
              aria-label={mobileNavOpen ? "Close dashboard navigation" : "Open dashboard navigation"}
              className="mobile-nav-toggle"
              onClick={() => setMobileNavOpen((open) => !open)}
              type="button"
            >
              <span aria-hidden="true" className="mobile-nav-toggle-mark">
                <span />
                <span />
                <span />
              </span>
              {mobileNavOpen ? "Close" : "Menu"}
            </button>
          </div>

          <div id="dashboard-navigation">
            <TabGroup
              groups={tabGroups}
              activeTab={tab}
              onTabChange={(id) => {
                setTab(id as Tab);
                setMobileNavOpen(false);
              }}
            />
          </div>
        </aside>

        <section className="ops-main">
          <header className="topbar">
            <div className="topbar-main">
              <p className="eyebrow">Agent Control Plane / Local Runtime</p>
              <h1>Agent Workspace</h1>
              <BoundaryBadges
                authStatus={authStatus}
                boundaries={dashboard.boundaries}
                cli={dashboard.cli}
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
              <a
                className="topbar-btn"
                href="https://github.com/Igzela/token-efficient-agent-harness-lab"
                target="_blank"
                rel="noopener noreferrer"
              >
                Docs
              </a>
            </div>
          </header>

          {authStatus !== "ok" && (
            <AuthPanel
              status={authStatus}
              message={authMessage}
              onSaved={() => setReloadKey((k) => k + 1)}
            />
          )}

          <div className="content-panel" role="tabpanel">
            {tab === "mission" && <MissionControl />}
            {tab === "dispatches" && (
              <Dispatches
                dispatches={dashboard.dispatches}
                totalDispatches={dashboard.counts.dispatches}
              />
            )}
            {tab === "routing" && <Routing rows={routingRows} />}
            {tab === "regulator" && <DynamicRegulator />}
            {tab === "operator" && <OperatorSurface />}
            {tab === "decisions" && <DecisionLog />}
            {tab === "team" && (
              <Team dashboard={dashboard} refreshDashboard={(d) => setDashboard(d)} />
            )}
            {tab === "costs" && <Costs dashboard={dashboard} />}
            {tab === "operations" && <Operations />}
            {tab === "runs" && <WorkflowRuns />}
            {tab === "patches" && <SupervisedPatch />}
            {tab === "scheduler" && <SchedulerStatus />}
            {tab === "pool" && <ExecutorPool />}
            {tab === "queue" && <QueueStatusComponent />}
            {tab === "settings" && <Settings dashboard={dashboard} />}
            {tab === "health" && <Health dashboard={dashboard} health={health} ready={ready} />}
            {tab === "backups" && <Backups />}
            {tab === "audit" && <AuditLog />}
          </div>

          <section className="status-strip" aria-label="Status summary">
            <Metric label="API" value={health} detail={health === "healthy" ? "healthy" : "check engine"} tone={health === "healthy" ? "ok" : "warn"} />
            <Metric label="Ready" value={ready} detail={ready === "ready" ? "ready" : "not ready"} tone={ready === "ready" ? "ok" : "warn"} />
            <Metric label="Dispatches" value={dashboard.counts.dispatches.toString()} detail="persisted" />
            <Metric label="Cost" value={`$${dashboard.costs.total_reserved_cost.toFixed(3)}`} detail="reserved" />
            <Metric label="Team" value={dashboard.counts.team_members.toString()} detail={`${dashboard.counts.api_keys} keys`} />
          </section>

          <SetupChecklist steps={setupSteps} />

          <RuntimeGates
            authStatus={authStatus}
            boundaries={dashboard.boundaries}
            cli={dashboard.cli}
            hasToken={hasLocalToken}
          />
        </section>
      </div>
    </main>
  );
}

function SetupChecklist({
  steps,
}: {
  steps: SetupStep[];
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
