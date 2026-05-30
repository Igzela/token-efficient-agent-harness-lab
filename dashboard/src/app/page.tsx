"use client";

import { useCallback, useEffect, useMemo, useState } from "react";
import {
  ApiError,
  clearStoredToken,
  createApiKey,
  createBackup,
  createTeamMember,
  deleteApiKey,
  deleteBackup,
  deleteMember,
  fetchAudit,
  fetchBackups,
  fetchDashboard,
  fetchDispatchDetail,
  fetchHealth,
  fetchProviderHealth,
  fetchReady,
  getStoredToken,
  isAuthError,
  restoreBackup,
  revokeApiKey,
  rotateApiKey,
  setStoredToken,
  updateMemberRole,
} from "@/lib/api-client";
import type { LocalDashboardState, LocalDispatchHistory } from "@/lib/types";

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

function AuthPanel({
  status,
  message,
  onSaved,
}: {
  status: "missing" | "denied" | "offline";
  message: string;
  onSaved: () => void;
}) {
  const [tokenInput, setTokenInput] = useState(getStoredToken() ?? "");

  function handleSave() {
    const trimmed = tokenInput.trim();
    if (trimmed) {
      setStoredToken(trimmed);
    } else {
      clearStoredToken();
    }
    onSaved();
  }

  function handleClear() {
    setTokenInput("");
    clearStoredToken();
    onSaved();
  }

  const icon = status === "offline" ? "🔌" : "🔑";

  return (
    <section className="card stack" style={{ maxWidth: 480, margin: "16px auto" }}>
      <h2>{icon} {status === "offline" ? "Engine Offline" : "Authentication Required"}</h2>
      <p className="muted">{message}</p>
      {status !== "offline" && (
        <>
          <label style={{ display: "flex", flexDirection: "column", gap: 4 }}>
            <span style={{ fontSize: "0.875rem" }}>Local API Key</span>
            <input
              type="password"
              value={tokenInput}
              onChange={(e) => setTokenInput(e.target.value)}
              placeholder="acp-..."
              style={{
                padding: "8px 10px",
                borderRadius: "var(--radius-sm)",
                border: "1px solid var(--border)",
                background: "var(--panel)",
                color: "var(--ink)",
              }}
            />
          </label>
          <div style={{ display: "flex", gap: 8, justifyContent: "flex-end" }}>
            {getStoredToken() && (
              <button onClick={handleClear} type="button" style={{ color: "#c0392b" }}>
                Clear Token
              </button>
            )}
            <button onClick={handleSave} type="button" disabled={!tokenInput.trim()}>
              Save &amp; Retry
            </button>
          </div>
        </>
      )}
      {status === "offline" && (
        <p className="muted">Start the engine and reload this page.</p>
      )}
    </section>
  );
}

export default function DashboardPage() {
  const [tab, setTab] = useState<Tab>("dispatches");
  const [health, setHealth] = useState("unknown");
  const [ready, setReady] = useState("unknown");
  const [dashboard, setDashboard] = useState<LocalDashboardState>(emptyDashboard);
  const [authStatus, setAuthStatus] = useState<"ok" | "missing" | "denied" | "offline">("ok");
  const [authMessage, setAuthMessage] = useState("");
  const [reloadKey, setReloadKey] = useState(0);
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

  useEffect(() => {
    let cancelled = false;
    Promise.allSettled([fetchHealth(), fetchReady(), fetchDashboard()]).then(
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
    return () => {
      cancelled = true;
    };
  }, [reloadKey]);

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
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [detail, setDetail] = useState<Record<string, unknown> | null>(null);
  const [loading, setLoading] = useState(false);
  const [detailError, setDetailError] = useState<string | null>(null);

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

function DispatchDetail({ detail }: { detail: Record<string, unknown> }) {
  const bundle = detail.bundle as Record<string, unknown> | undefined;
  const analysis = bundle?.analysis as Record<string, unknown> | undefined;
  const decision = bundle?.decision as Record<string, unknown> | undefined;
  const execution = bundle?.execution_result as Record<string, unknown> | undefined;
  const evaluation = bundle?.evaluation_result as Record<string, unknown> | undefined;

  return (
    <div className="split">
      <div className="card stack">
        <h3>Record</h3>
        <div className="row"><span>dispatch_id</span><span className="mono">{String(detail.dispatch_id)}</span></div>
        <div className="row"><span>raw_request</span><span>{String(detail.raw_request)}</span></div>
        <div className="row"><span>tier</span><span>{String(detail.selected_tier)}</span></div>
        <div className="row"><span>status</span><span className="pill info">{String(detail.final_status)}</span></div>
        <div className="row"><span>risk</span><span className={`pill ${detail.risk_level === "low" ? "ok" : "warn"}`}>{String(detail.risk_level)}</span></div>
        <div className="row"><span>executor</span><span className="mono">{String(detail.executor_type)}</span></div>
        {detail.estimated_cost_usd != null && (
          <div className="row"><span>est. cost</span><span>${Number(detail.estimated_cost_usd).toFixed(4)}</span></div>
        )}
        {detail.latency_ms != null && (
          <div className="row"><span>latency</span><span>{String(detail.latency_ms)} ms</span></div>
        )}
      </div>
      {analysis && (
        <div className="card stack">
          <h3>Analysis</h3>
          <div className="row"><span>task_domain</span><span>{String(analysis.task_domain)}</span></div>
          <div className="row"><span>task_intent</span><span>{String(analysis.task_intent)}</span></div>
          <div className="row"><span>complexity</span><span>{String(analysis.complexity)}</span></div>
          <div className="row"><span>user_negated_provider</span><span>{String(analysis.user_negated_provider)}</span></div>
        </div>
      )}
      {decision && (
        <div className="card stack">
          <h3>Decision</h3>
          <div className="row"><span>selected_tier</span><span>{String(decision.selected_tier)}</span></div>
          <div className="row"><span>decision_status</span><span className="pill info">{String(decision.decision_status)}</span></div>
          <div className="row"><span>confidence</span><span>{String(decision.confidence)}</span></div>
          <div className="row"><span>risk_level</span><span>{String(decision.risk_level)}</span></div>
          <div className="row"><span>expected_quality_band</span><span>{String(decision.expected_quality_band)}</span></div>
          {decision.budget_reservation != null && (
            <div className="row"><span>reserved_cost</span><span>${Number((decision.budget_reservation as Record<string, unknown>).reserved_cost).toFixed(4)}</span></div>
          )}
        </div>
      )}
      {execution && (
        <div className="card stack">
          <h3>Execution</h3>
          <div className="row"><span>executor_type</span><span>{String(execution.executor_type)}</span></div>
          <div className="row"><span>input_tokens</span><span className="mono">{String(execution.input_tokens)}</span></div>
          <div className="row"><span>output_tokens</span><span className="mono">{String(execution.output_tokens)}</span></div>
          <div className="row"><span>estimated_cost</span><span>${Number(execution.estimated_cost ?? 0).toFixed(4)}</span></div>
          <div className="row"><span>latency_ms</span><span>{String(execution.latency_ms)}</span></div>
        </div>
      )}
      {evaluation && (
        <div className="card stack">
          <h3>Evaluation</h3>
          <div className="row"><span>status</span><span className={`pill ${evaluation.status === "pass" ? "ok" : "warn"}`}>{String(evaluation.status)}</span></div>
          <div className="row"><span>final_status</span><span className="pill info">{String(evaluation.final_status)}</span></div>
        </div>
      )}
    </div>
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
  const [teamError, setTeamError] = useState<string | null>(null);
  const [confirmAction, setConfirmAction] = useState<{
    type: "deleteMember" | "revokeKey" | "deleteKey" | "rotateKey";
    id: string;
  } | null>(null);

  const refresh = () => fetchDashboard().then((d) => refreshDashboard(d));

  async function handleCreateMember() {
    if (!userId || !displayName) return;
    setBusy(true);
    setTeamError(null);
    try {
      await createTeamMember({ user_id: userId, display_name: displayName, role });
      setUserId("");
      setDisplayName("");
      setRole("readonly");
      refresh();
    } catch (e) {
      setTeamError(e instanceof Error ? e.message : "Failed to create member");
    } finally {
      setBusy(false);
    }
  }

  async function handleUpdateRole(uid: string, newRole: string) {
    setBusy(true);
    setTeamError(null);
    try {
      await updateMemberRole(uid, newRole);
      refresh();
    } catch (e) {
      setTeamError(e instanceof Error ? e.message : "Failed to update role");
    } finally {
      setBusy(false);
    }
  }

  async function handleDeleteMember(uid: string) {
    setConfirmAction({ type: "deleteMember", id: uid });
  }

  async function doTeamConfirm() {
    if (!confirmAction) return;
    setBusy(true);
    setTeamError(null);
    try {
      if (confirmAction.type === "deleteMember") {
        await deleteMember(confirmAction.id);
      } else if (confirmAction.type === "revokeKey") {
        await revokeApiKey(confirmAction.id);
      } else if (confirmAction.type === "deleteKey") {
        await deleteApiKey(confirmAction.id);
      } else if (confirmAction.type === "rotateKey") {
        const res = await rotateApiKey(confirmAction.id);
        const rawKey = (res as Record<string, unknown>).raw_key;
        if (rawKey) {
          alert(`Rotated API key (copy now — shown once):\n${rawKey}`);
        }
      }
      refresh();
    } catch (e) {
      setTeamError(e instanceof Error ? e.message : "Operation failed");
    } finally {
      setBusy(false);
      setConfirmAction(null);
    }
  }

  async function handleCreateKey() {
    if (!keyUserId) return;
    setBusy(true);
    setTeamError(null);
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
    } catch (e) {
      setTeamError(e instanceof Error ? e.message : "Failed to create API key");
    } finally {
      setBusy(false);
    }
  }

  async function handleRevokeKey(keyId: string) {
    setConfirmAction({ type: "revokeKey", id: keyId });
  }

  async function handleDeleteKey(keyId: string) {
    setConfirmAction({ type: "deleteKey", id: keyId });
  }

  async function handleRotateKey(keyId: string) {
    setConfirmAction({ type: "rotateKey", id: keyId });
  }

  function toggleScope(scope: string) {
    setKeyScopes((prev) =>
      prev.includes(scope) ? prev.filter((s) => s !== scope) : [...prev, scope],
    );
  }

  const teamConfirmMessages: Record<string, string> = {
    deleteMember: `Delete member ${confirmAction?.id}? This cannot be undone.`,
    revokeKey: `Revoke key ${confirmAction?.id}? This key will no longer authenticate.`,
    deleteKey: `Permanently delete key ${confirmAction?.id}? This cannot be undone.`,
    rotateKey: `Rotate key ${confirmAction?.id}? A new key will be created and the old one revoked.`,
  };

  return (
    <section className="split">
      {teamError && <p className="error-text" style={{ gridColumn: "1 / -1" }}>{teamError}</p>}
      {confirmAction && (
        <div className="confirm-overlay" onClick={() => setConfirmAction(null)}>
          <div className="confirm-card" onClick={(e) => e.stopPropagation()}>
            <p>{teamConfirmMessages[confirmAction.type]}</p>
            <div style={{ display: "flex", gap: 8, justifyContent: "flex-end" }}>
              <button onClick={() => setConfirmAction(null)} type="button">Cancel</button>
              <button onClick={doTeamConfirm} disabled={busy} type="button" style={{ color: "#c0392b" }}>Confirm</button>
            </div>
          </div>
        </div>
      )}
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
  const c = dashboard.costs;
  const maxTier = Math.max(1, ...c.by_tier.map((t) => t.reserved_cost));
  const recentDaily = c.daily.slice(0, 7).reverse();
  const maxDaily = Math.max(1, ...recentDaily.map((d) => d.reserved_cost));
  return (
    <section className="card stack">
      <div className="heading-row">
        <h2>Cost Governance</h2>
        <span className="pill info">{c.currency}</span>
      </div>
      <div className="metrics">
        <div className="metric">
          <span className="metric-label">Reserved Budget</span>
          <strong>${c.total_reserved_cost.toFixed(4)}</strong>
        </div>
        <div className="metric">
          <span className="metric-label">Provider Estimated</span>
          <strong>${c.total_estimated_cost_usd.toFixed(4)}</strong>
        </div>
        <div className="metric">
          <span className="metric-label">Utilization</span>
          <strong>{(c.cost_utilization * 100).toFixed(1)}%</strong>
        </div>
        <div className="metric">
          <span className="metric-label">Tokens (in/out)</span>
          <strong>{c.total_input_tokens.toLocaleString()} / {c.total_output_tokens.toLocaleString()}</strong>
        </div>
      </div>
      {c.by_tier.length > 0 && (
        <>
          <h3>By Tier</h3>
          <div className="bars">
            {c.by_tier.map((item) => (
              <div className="bar" key={item.selected_tier}>
                <div className="row">
                  <span>{item.selected_tier}</span>
                  <span>${item.estimated_cost_usd.toFixed(4)} / ${item.reserved_cost.toFixed(4)}</span>
                </div>
                <div className="bar-track">
                  <div
                    className="bar-fill"
                    style={{ width: `${(item.reserved_cost / maxTier) * 100}%`, opacity: 0.35 }}
                  />
                  <div
                    className="bar-fill"
                    style={{
                      width: `${(item.estimated_cost_usd / maxTier) * 100}%`,
                      position: "absolute",
                    }}
                  />
                </div>
              </div>
            ))}
          </div>
          <div className="legend" style={{ fontSize: "0.75rem", opacity: 0.6, display: "flex", gap: "1rem" }}>
            <span><span style={{ display: "inline-block", width: 10, height: 10, background: "var(--accent)", opacity: 0.35, marginRight: 4 }} />Reserved</span>
            <span><span style={{ display: "inline-block", width: 10, height: 10, background: "var(--accent)", marginRight: 4 }} />Estimated</span>
          </div>
        </>
      )}
      {recentDaily.length > 0 && (
        <>
          <h3>Daily Trend (last {recentDaily.length} days)</h3>
          <div className="bars">
            {recentDaily.map((day) => (
              <div className="bar" key={day.date}>
                <div className="row">
                  <span>{day.date}</span>
                  <span>${day.estimated_cost_usd.toFixed(4)} ({day.dispatch_count})</span>
                </div>
                <div className="bar-track">
                  <div
                    className="bar-fill"
                    style={{ width: `${(day.reserved_cost / maxDaily) * 100}%`, opacity: 0.35 }}
                  />
                  <div
                    className="bar-fill"
                    style={{
                      width: `${(day.estimated_cost_usd / maxDaily) * 100}%`,
                      position: "absolute",
                    }}
                  />
                </div>
              </div>
            ))}
          </div>
        </>
      )}
      {c.by_tier.length === 0 && recentDaily.length === 0 && (
        <p className="muted">No local cost records</p>
      )}
    </section>
  );
}

function Settings({ dashboard }: { dashboard: LocalDashboardState }) {
  const [providerInfo, setProviderInfo] = useState<Record<string, unknown> | null>(null);
  const [providerError, setProviderError] = useState<string | null>(null);

  useEffect(() => {
    fetchProviderHealth()
      .then(setProviderInfo)
      .catch((e) => setProviderError(e instanceof Error ? e.message : "Failed to load"));
  }, []);

  const rows: [string, string | number | boolean | null][] = [
    ["Workspace", dashboard.config.workspace_name ?? "Local Team"],
    ["Provider transport", dashboard.boundaries.provider_transport],
    ["Target repository writes", dashboard.boundaries.target_repository_writes],
    ["Sandbox/process execution", dashboard.boundaries.sandbox_process_execution],
    ["Runtime workers", dashboard.boundaries.runtime_workers],
    ["Docker required", dashboard.boundaries.docker_required ? "yes" : "no"],
  ];
  return (
    <section className="split">
      <div className="card">
        <h2>Settings</h2>
        {rows.map(([label, value]) => (
          <div className="setting" key={label}>
            <span className="label">{label}</span>
            <strong>{String(value)}</strong>
          </div>
        ))}
      </div>
      <div className="card stack">
        <h2>Provider</h2>
        {providerError ? (
          <p className="error-text">{providerError}</p>
        ) : providerInfo ? (
          <>
            <div className="row">
              <span>Provider ID</span>
              <span className="mono">{String(providerInfo.provider_id ?? "none")}</span>
            </div>
            <div className="row">
              <span>Status</span>
              <span className={`pill ${providerInfo.status === "ok" ? "ok" : "warn"}`}>{String(providerInfo.status)}</span>
            </div>
            <div className="row">
              <span>Enabled</span>
              <span className="pill info">{String(providerInfo.enabled ?? false)}</span>
            </div>
          </>
        ) : (
          <p className="muted">Loading provider status...</p>
        )}
      </div>
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

type ConfirmAction = { type: "deleteBackup" | "restoreBackup"; backupId: string } | null;

function ConfirmDialog({
  action,
  onConfirm,
  onCancel,
}: {
  action: ConfirmAction;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  if (!action) return null;
  const messages: Record<string, string> = {
    deleteBackup: `Delete backup ${action.backupId}? This cannot be undone.`,
    restoreBackup: `Restore from backup ${action.backupId}? Current data will be replaced.`,
  };
  return (
    <div className="confirm-overlay" onClick={onCancel}>
      <div className="confirm-card" onClick={(e) => e.stopPropagation()}>
        <p>{messages[action.type]}</p>
        <div style={{ display: "flex", gap: 8, justifyContent: "flex-end" }}>
          <button onClick={onCancel} type="button">Cancel</button>
          <button onClick={onConfirm} type="button" style={{ color: "#c0392b" }}>Confirm</button>
        </div>
      </div>
    </div>
  );
}

function Backups() {
  const [backups, setBackups] = useState<Array<Record<string, unknown>>>([]);
  const [busy, setBusy] = useState(false);
  const [confirm, setConfirm] = useState<ConfirmAction>(null);
  const [error, setError] = useState<string | null>(null);

  const load = () =>
    fetchBackups()
      .then((r) => { setBackups((r.backups as Array<Record<string, unknown>>) ?? []); setError(null); })
      .catch((e) => setError(e instanceof Error ? e.message : "Failed to load backups"));

  useEffect(() => { load(); }, []);

  async function doConfirm() {
    if (!confirm) return;
    setBusy(true);
    setError(null);
    try {
      if (confirm.type === "deleteBackup") {
        await deleteBackup(confirm.backupId);
      } else if (confirm.type === "restoreBackup") {
        await restoreBackup(confirm.backupId);
      }
      load();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Operation failed");
    } finally {
      setBusy(false);
      setConfirm(null);
    }
  }

  async function handleCreateBackup() {
    setBusy(true);
    setError(null);
    try {
      await createBackup();
      load();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Backup creation failed");
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className="card stack">
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
        <h2>Backups</h2>
        <button disabled={busy} onClick={handleCreateBackup} type="button">Create Backup</button>
      </div>
      {error && <p className="error-text">{error}</p>}
      <ConfirmDialog action={confirm} onConfirm={doConfirm} onCancel={() => setConfirm(null)} />
      {backups.length === 0 && !error ? (
        <p className="muted">No local backups</p>
      ) : (
        <table>
          <thead>
            <tr>
              <th>ID</th>
              <th>Label</th>
              <th>Created</th>
              <th>Size</th>
              <th>Actions</th>
            </tr>
          </thead>
          <tbody>
            {backups.map((b) => (
              <tr key={String(b.backup_id)}>
                <td className="mono">{String(b.backup_id)}</td>
                <td>{String(b.label)}</td>
                <td>{String(b.created_at)}</td>
                <td>{String(b.size_bytes)} bytes</td>
                <td style={{ display: "flex", gap: 4 }}>
                  <button
                    disabled={busy}
                    onClick={() => setConfirm({ type: "restoreBackup", backupId: String(b.backup_id) })}
                    type="button"
                  >
                    Restore
                  </button>
                  <button
                    disabled={busy}
                    onClick={() => setConfirm({ type: "deleteBackup", backupId: String(b.backup_id) })}
                    type="button"
                    style={{ color: "#c0392b" }}
                  >
                    Delete
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </section>
  );
}

function AuditLog() {
  const [events, setEvents] = useState<Array<Record<string, unknown>>>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    fetchAudit()
      .then((r) => { setEvents((r.events as Array<Record<string, unknown>>) ?? []); setError(null); })
      .catch((e) => setError(e instanceof Error ? e.message : "Failed to load audit events"));
  }, []);

  return (
    <section className="card stack">
      <h2>Audit Log</h2>
      {error && <p className="error-text">{error}</p>}
      {events.length === 0 && !error ? (
        <p className="muted">No audit events</p>
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
    </section>
  );
}
