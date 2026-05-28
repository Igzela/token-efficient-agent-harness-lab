"use client";

import { useEffect, useMemo, useState } from "react";
import { fetchHealth, fetchReady } from "@/lib/api-client";

type Tab = "dispatches" | "routing" | "workflows" | "costs" | "settings" | "health";

const tabs: { id: Tab; label: string }[] = [
  { id: "dispatches", label: "Dispatches" },
  { id: "routing", label: "Routing" },
  { id: "workflows", label: "Agents" },
  { id: "costs", label: "Costs" },
  { id: "settings", label: "Settings" },
  { id: "health", label: "Health" },
];

const dispatches = [
  {
    id: "disp-fixture-001",
    task: "Summarize migration status",
    tier: "balanced_worker",
    status: "not_executed",
    quality: "high",
    cost: 0.014,
    risk: "low",
  },
  {
    id: "disp-fixture-002",
    task: "Review routing policy",
    tier: "strong_planner",
    status: "manual_pending",
    quality: "critical",
    cost: 0.041,
    risk: "medium",
  },
  {
    id: "disp-fixture-003",
    task: "Audit budget ledger",
    tier: "verifier",
    status: "not_executed",
    quality: "standard",
    cost: 0.009,
    risk: "low",
  },
];

const routingRows = [
  { group: "code/review", selected: "strong_planner", fallback: "balanced_worker", confidence: 0.84 },
  { group: "docs/summarize", selected: "cheap_executor", fallback: "balanced_worker", confidence: 0.91 },
  { group: "governance/audit", selected: "verifier", fallback: "strong_planner", confidence: 0.79 },
];

const workflows = [
  { name: "analysis", state: "ready", assigned: 2, blocked: 0 },
  { name: "routing", state: "ready", assigned: 1, blocked: 0 },
  { name: "approval_gate", state: "hold", assigned: 0, blocked: 1 },
];

const settings = [
  ["Provider transport", "stub/off"],
  ["Target repository writes", "disabled"],
  ["Sandbox/process execution", "disabled"],
  ["Dashboard controls", "read-only"],
  ["Runtime workers", "disabled"],
];

export default function DashboardPage() {
  const [tab, setTab] = useState<Tab>("dispatches");
  const [health, setHealth] = useState("unknown");
  const [ready, setReady] = useState("unknown");

  useEffect(() => {
    let cancelled = false;
    Promise.allSettled([fetchHealth(), fetchReady()]).then(([healthResult, readyResult]) => {
      if (cancelled) return;
      setHealth(healthResult.status === "fulfilled" ? healthResult.value.status : "offline");
      setReady(readyResult.status === "fulfilled" ? readyResult.value.status : "offline");
    });
    return () => {
      cancelled = true;
    };
  }, []);

  const totals = useMemo(() => {
    const totalCost = dispatches.reduce((sum, item) => sum + item.cost, 0);
    const blocked = workflows.reduce((sum, item) => sum + item.blocked, 0);
    return { blocked, totalCost };
  }, []);

  return (
    <main>
      <div className="shell">
        <header className="topbar">
          <div>
            <p className="eyebrow">Agent Control Plane</p>
            <h1>Operations Dashboard</h1>
          </div>
          <span className="pill info">Read-only</span>
        </header>

        <section className="status-strip" aria-label="Status summary">
          <Metric label="API health" value={health} tone={health === "healthy" ? "ok" : "warn"} />
          <Metric label="Readiness" value={ready} tone={ready === "ready" ? "ok" : "warn"} />
          <Metric label="Dispatches" value={dispatches.length.toString()} detail="fixture view" />
          <Metric label="Cost" value={`$${totals.totalCost.toFixed(3)}`} detail="reserved" />
          <Metric label="Blocked" value={totals.blocked.toString()} tone={totals.blocked ? "warn" : "ok"} />
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

        {tab === "dispatches" && <Dispatches />}
        {tab === "routing" && <Routing />}
        {tab === "workflows" && <Workflows />}
        {tab === "costs" && <Costs />}
        {tab === "settings" && <Settings />}
        {tab === "health" && <Health health={health} ready={ready} />}
      </div>
    </main>
  );
}

function Metric({
  detail,
  label,
  tone = "info",
  value,
}: {
  detail?: string;
  label: string;
  tone?: "ok" | "warn" | "info";
  value: string;
}) {
  return (
    <article className="metric">
      <span className="label">{label}</span>
      <strong>{value}</strong>
      <span className={tone}>{detail ?? tone}</span>
    </article>
  );
}

function Dispatches() {
  return (
    <section className="grid">
      <div className="table-wrap">
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
            {dispatches.map((item) => (
              <tr key={item.id}>
                <td className="mono">{item.id}</td>
                <td>{item.task}</td>
                <td>{item.tier}</td>
                <td>
                  <span className="pill info">{item.status}</span>
                </td>
                <td>
                  <span className={`pill ${item.risk === "low" ? "ok" : "warn"}`}>{item.risk}</span>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <aside className="card stack">
        <div className="heading-row">
          <h2>Quality Gates</h2>
          <span className="pill ok">deterministic</span>
        </div>
        {dispatches.map((item) => (
          <div className="row" key={item.id}>
            <span>{item.quality}</span>
            <span className="mono">{item.id}</span>
          </div>
        ))}
      </aside>
    </section>
  );
}

function Routing() {
  return (
    <section className="wide-grid">
      {routingRows.map((row) => (
        <article className="card stack" key={row.group}>
          <div className="heading-row">
            <h2>{row.group}</h2>
            <span className="pill info">{Math.round(row.confidence * 100)}%</span>
          </div>
          <div className="setting">
            <span className="label">Selected</span>
            <strong>{row.selected}</strong>
          </div>
          <div className="setting">
            <span className="label">Fallback</span>
            <strong>{row.fallback}</strong>
          </div>
        </article>
      ))}
    </section>
  );
}

function Workflows() {
  return (
    <section className="split">
      <div className="card stack">
        <h2>Agent Roles</h2>
        {workflows.map((item) => (
          <div className="row" key={item.name}>
            <span>{item.name}</span>
            <span className={`pill ${item.state === "hold" ? "warn" : "ok"}`}>{item.state}</span>
          </div>
        ))}
      </div>
      <div className="card stack">
        <h2>Workflow DAG</h2>
        {workflows.map((item) => (
          <div className="row" key={item.name}>
            <span>{item.name}</span>
            <span className="mono">
              assigned {item.assigned} / blocked {item.blocked}
            </span>
          </div>
        ))}
      </div>
    </section>
  );
}

function Costs() {
  const max = Math.max(...dispatches.map((item) => item.cost));
  return (
    <section className="card stack">
      <div className="heading-row">
        <h2>Budget Ledger</h2>
        <span className="pill info">reserved</span>
      </div>
      <div className="bars">
        {dispatches.map((item) => (
          <div className="bar" key={item.id}>
            <div className="row">
              <span>{item.tier}</span>
              <span>${item.cost.toFixed(3)}</span>
            </div>
            <div className="bar-track">
              <div className="bar-fill" style={{ width: `${(item.cost / max) * 100}%` }} />
            </div>
          </div>
        ))}
      </div>
    </section>
  );
}

function Settings() {
  return (
    <section className="card">
      <h2>Settings</h2>
      {settings.map(([label, value]) => (
        <div className="setting" key={label}>
          <span className="label">{label}</span>
          <strong>{value}</strong>
        </div>
      ))}
    </section>
  );
}

function Health({ health, ready }: { health: string; ready: string }) {
  return (
    <section className="split">
      <article className="card stack">
        <h2>API</h2>
        <div className="row">
          <span>health</span>
          <span className="pill info">{health}</span>
        </div>
        <div className="row">
          <span>ready</span>
          <span className="pill info">{ready}</span>
        </div>
      </article>
      <article className="card stack">
        <h2>Boundaries</h2>
        {settings.slice(0, 3).map(([label, value]) => (
          <div className="row" key={label}>
            <span>{label}</span>
            <span className="pill ok">{value}</span>
          </div>
        ))}
      </article>
    </section>
  );
}
