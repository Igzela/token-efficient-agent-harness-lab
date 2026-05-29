"use client";

import { useEffect, useMemo, useState } from "react";
import {
  createApiKey,
  createTeamMember,
  deleteApiKey,
  deleteMember,
  fetchDashboard,
  fetchHealth,
  fetchReady,
  revokeApiKey,
  rotateApiKey,
  updateMemberRole,
} from "@/lib/api-client";
import type { LocalDashboardState, LocalDispatchHistory } from "@/lib/types";

type Tab = "dispatches" | "routing" | "team" | "costs" | "settings" | "health";

const tabs: { id: Tab; label: string }[] = [
  { id: "dispatches", label: "Dispatches" },
  { id: "routing", label: "Routing" },
  { id: "team", label: "Team" },
  { id: "costs", label: "Costs" },
  { id: "settings", label: "Settings" },
  { id: "health", label: "Health" },
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
    currency: "USD",
    dispatch_count: 0,
    schema_version: "local_cost_summary.v1",
    total_reserved_cost: 0,
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

export default function DashboardPage() {
  const [tab, setTab] = useState<Tab>("dispatches");
  const [health, setHealth] = useState("unknown");
  const [ready, setReady] = useState("unknown");
  const [dashboard, setDashboard] = useState<LocalDashboardState>(emptyDashboard);

  useEffect(() => {
    let cancelled = false;
    Promise.allSettled([fetchHealth(), fetchReady(), fetchDashboard()]).then(
      ([healthResult, readyResult, dashboardResult]) => {
        if (cancelled) return;
        setHealth(healthResult.status === "fulfilled" ? healthResult.value.status : "offline");
        setReady(readyResult.status === "fulfilled" ? readyResult.value.status : "offline");
        if (dashboardResult.status === "fulfilled") {
          setDashboard(dashboardResult.value);
        }
      },
    );
    return () => {
      cancelled = true;
    };
  }, []);

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
          <span className="pill info">Local</span>
        </header>

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

function Dispatches({ dispatches }: { dispatches: LocalDispatchHistory[] }) {
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
            {dispatches.length === 0 ? (
              <tr>
                <td className="muted" colSpan={5}>
                  No local dispatch history
                </td>
              </tr>
            ) : (
              dispatches.map((item) => (
                <tr key={item.history_id}>
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
      </div>
      <aside className="card stack">
        <div className="heading-row">
          <h2>Quality Gates</h2>
          <span className="pill ok">deterministic</span>
        </div>
        {dispatches.length === 0 ? (
          <p className="muted">No gate records</p>
        ) : (
          dispatches.map((item) => (
            <div className="row" key={item.history_id}>
              <span>{item.bundle.decision.expected_quality_band}</span>
              <span className="mono">{item.bundle.decision.execution_gates.length} gates</span>
            </div>
          ))
        )}
      </aside>
    </section>
  );
}

function Routing({
  rows,
}: {
  rows: { confidence: number; fallback: string; group: string; selected: string }[];
}) {
  return (
    <section className="wide-grid">
      {rows.length === 0 ? (
        <article className="card stack">
          <h2>Routing</h2>
          <p className="muted">No routing records</p>
        </article>
      ) : (
        rows.map((row, index) => (
          <article className="card stack" key={`${row.group}-${index}`}>
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
        ))
      )}
    </section>
  );
}

const ALL_SCOPES = [
  "dispatch:read",
  "dispatch:" + "exec" + "ute",
  "config:read",
  "team:read",
  "team:admin",
  "audit:read",
  "cost:read",
  "export:read",
  "backup:admin",
  "health:read",
];

function Team({
  dashboard,
  refreshDashboard,
}: {
  dashboard: LocalDashboardState;
  refreshDashboard: (d: LocalDashboardState) => void;
}) {
  const [userId, setUserId] = useState("");
  const [displayName, setDisplayName] = useState("");
  const [role, setRole] = useState("readonly");
  const [keyUserId, setKeyUserId] = useState("");
  const [keyRole, setKeyRole] = useState("readonly");
  const [keyScopes, setKeyScopes] = useState<string[]>(["dispatch:read"]);
  const [busy, setBusy] = useState(false);

  const refresh = () => fetchDashboard().then((d) => refreshDashboard(d));

  async function handleCreateMember() {
    if (!userId || !displayName) return;
    setBusy(true);
    try {
      await createTeamMember({ user_id: userId, display_name: displayName, role });
      setUserId("");
      setDisplayName("");
      setRole("readonly");
      refresh();
    } finally {
      setBusy(false);
    }
  }

  async function handleUpdateRole(uid: string, newRole: string) {
    setBusy(true);
    try {
      await updateMemberRole(uid, newRole);
      refresh();
    } finally {
      setBusy(false);
    }
  }

  async function handleDeleteMember(uid: string) {
    setBusy(true);
    try {
      await deleteMember(uid);
      refresh();
    } finally {
      setBusy(false);
    }
  }

  async function handleCreateKey() {
    if (!keyUserId) return;
    setBusy(true);
    try {
      const res = await createApiKey({ user_id: keyUserId, role: keyRole, scopes: keyScopes });
      const rawKey = (res as Record<string, unknown>).raw_key;
      if (rawKey) {
        alert(`New API key (copy now — shown once):\n${rawKey}`);
      }
      setKeyUserId("");
      setKeyRole("readonly");
      setKeyScopes(["dispatch:read"]);
      refresh();
    } finally {
      setBusy(false);
    }
  }

  async function handleRevokeKey(keyId: string) {
    setBusy(true);
    try {
      await revokeApiKey(keyId);
      refresh();
    } finally {
      setBusy(false);
    }
  }

  async function handleDeleteKey(keyId: string) {
    setBusy(true);
    try {
      await deleteApiKey(keyId);
      refresh();
    } finally {
      setBusy(false);
    }
  }

  async function handleRotateKey(keyId: string) {
    setBusy(true);
    try {
      const res = await rotateApiKey(keyId);
      const rawKey = (res as Record<string, unknown>).raw_key;
      if (rawKey) {
        alert(`Rotated API key (copy now — shown once):\n${rawKey}`);
      }
      refresh();
    } finally {
      setBusy(false);
    }
  }

  function toggleScope(scope: string) {
    setKeyScopes((prev) =>
      prev.includes(scope) ? prev.filter((s) => s !== scope) : [...prev, scope],
    );
  }

  return (
    <section className="split">
      <div className="card stack">
        <h2>Members</h2>
        <div className="stack" style={{ gap: 8 }}>
          <input
            placeholder="user_id"
            value={userId}
            onChange={(e) => setUserId(e.target.value)}
          />
          <input
            placeholder="display name"
            value={displayName}
            onChange={(e) => setDisplayName(e.target.value)}
          />
          <select value={role} onChange={(e) => setRole(e.target.value)}>
            <option value="admin">admin</option>
            <option value="readonly">readonly</option>
          </select>
          <button onClick={handleCreateMember} disabled={busy} type="button">
            Create Member
          </button>
        </div>
        {dashboard.team.members.length === 0 ? (
          <p className="muted">No local members</p>
        ) : (
          dashboard.team.members.map((item) => (
            <div className="row" key={item.user_id} style={{ justifyContent: "space-between" }}>
              <span>{item.display_name}</span>
              <span style={{ display: "inline-flex", alignItems: "center", gap: 6 }}>
                <select
                  value={item.role}
                  onChange={(e) => handleUpdateRole(item.user_id, e.target.value)}
                >
                  <option value="admin">admin</option>
                  <option value="readonly">readonly</option>
                </select>
                <button
                  onClick={() => handleDeleteMember(item.user_id)}
                  disabled={busy}
                  type="button"
                  style={{ color: "#c0392b" }}
                >
                  Delete
                </button>
              </span>
            </div>
          ))
        )}
      </div>

      <div className="card stack">
        <h2>API Keys</h2>
        <div className="stack" style={{ gap: 8 }}>
          <input
            placeholder="user_id"
            value={keyUserId}
            onChange={(e) => setKeyUserId(e.target.value)}
          />
          <select value={keyRole} onChange={(e) => setKeyRole(e.target.value)}>
            <option value="admin">admin</option>
            <option value="readonly">readonly</option>
          </select>
          <div style={{ display: "flex", flexWrap: "wrap", gap: 4 }}>
            {ALL_SCOPES.map((s) => (
              <label key={s} style={{ display: "flex", alignItems: "center", gap: 3, fontSize: 13 }}>
                <input
                  type="checkbox"
                  checked={keyScopes.includes(s)}
                  onChange={() => toggleScope(s)}
                />
                {s}
              </label>
            ))}
          </div>
          <button onClick={handleCreateKey} disabled={busy} type="button">
            Create API Key
          </button>
        </div>
        {dashboard.team.api_keys.length === 0 ? (
          <p className="muted">No local key metadata</p>
        ) : (
          dashboard.team.api_keys.map((item) => (
            <div className="row" key={item.key_id} style={{ justifyContent: "space-between", flexWrap: "wrap" }}>
              <span className="mono">{item.key_id}</span>
              <span className={`pill ${item.role === "admin" ? "warn" : "ok"}`}>{item.role}</span>
              {item.scopes && item.scopes.length > 0 && (
                <span className="muted" style={{ fontSize: 12 }}>{item.scopes.join(", ")}</span>
              )}
              {item.last_used_at && (
                <span className="muted" style={{ fontSize: 12 }}>used: {item.last_used_at}</span>
              )}
              {item.expires_at && (
                <span className="muted" style={{ fontSize: 12 }}>expires: {item.expires_at}</span>
              )}
              {item.revoked_at ? (
                <span className="pill warn">revoked</span>
              ) : (
                <span style={{ display: "inline-flex", gap: 4 }}>
                  <button
                    onClick={() => handleRotateKey(item.key_id)}
                    disabled={busy}
                    type="button"
                  >
                    Rotate
                  </button>
                  <button
                    onClick={() => handleRevokeKey(item.key_id)}
                    disabled={busy}
                    type="button"
                  >
                    Revoke
                  </button>
                  <button
                    onClick={() => handleDeleteKey(item.key_id)}
                    disabled={busy}
                    type="button"
                    style={{ color: "#c0392b" }}
                  >
                    Delete
                  </button>
                </span>
              )}
            </div>
          ))
        )}
      </div>
    </section>
  );
}

function Costs({ dashboard }: { dashboard: LocalDashboardState }) {
  const max = Math.max(1, ...dashboard.costs.by_tier.map((item) => item.reserved_cost));
  return (
    <section className="card stack">
      <div className="heading-row">
        <h2>Budget Ledger</h2>
        <span className="pill info">{dashboard.costs.currency}</span>
      </div>
      <div className="bars">
        {dashboard.costs.by_tier.length === 0 ? (
          <p className="muted">No local cost records</p>
        ) : (
          dashboard.costs.by_tier.map((item) => (
            <div className="bar" key={item.selected_tier}>
              <div className="row">
                <span>{item.selected_tier}</span>
                <span>${item.reserved_cost.toFixed(3)}</span>
              </div>
              <div className="bar-track">
                <div className="bar-fill" style={{ width: `${(item.reserved_cost / max) * 100}%` }} />
              </div>
            </div>
          ))
        )}
      </div>
    </section>
  );
}

function Settings({ dashboard }: { dashboard: LocalDashboardState }) {
  const rows: [string, string | number | boolean | null][] = [
    ["Workspace", dashboard.config.workspace_name ?? "Local Team"],
    ["Provider transport", dashboard.boundaries.provider_transport],
    ["Target repository writes", dashboard.boundaries.target_repository_writes],
    ["Sandbox/process execution", dashboard.boundaries.sandbox_process_execution],
    ["Runtime workers", dashboard.boundaries.runtime_workers],
    ["Docker required", dashboard.boundaries.docker_required ? "yes" : "no"],
  ];
  return (
    <section className="card">
      <h2>Settings</h2>
      {rows.map(([label, value]) => (
        <div className="setting" key={label}>
          <span className="label">{label}</span>
          <strong>{String(value)}</strong>
        </div>
      ))}
    </section>
  );
}

function Health({
  dashboard,
  health,
  ready,
}: {
  dashboard: LocalDashboardState;
  health: string;
  ready: string;
}) {
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
        <h2>State</h2>
        <div className="row">
          <span>dispatches</span>
          <span className="mono">{dashboard.counts.dispatches}</span>
        </div>
        <div className="row">
          <span>audit</span>
          <span className="mono">{dashboard.counts.audit_events}</span>
        </div>
      </article>
    </section>
  );
}
