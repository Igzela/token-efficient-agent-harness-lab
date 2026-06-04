import { useState } from "react";
import {
  createApiKey,
  createTeamMember,
  deleteApiKey,
  deleteMember,
  fetchDashboard,
  revokeApiKey,
  rotateApiKey,
  updateMemberRole,
} from "@/lib/api-client";
import type { LocalDashboardState } from "@/lib/types";
import { ConfirmDialog, type ConfirmAction } from "./ConfirmDialog";
import { EmptyState } from "./EmptyState";
import { KeyRevealModal } from "./KeyRevealModal";
import { StateBanner } from "./StateBanner";

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

export function Team({
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
  const [confirmAction, setConfirmAction] = useState<ConfirmAction>(null);
  const [revealedKey, setRevealedKey] = useState<{ rawKey: string; label: string } | null>(null);

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
    if (!confirmAction || !("id" in confirmAction)) return;
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
          setRevealedKey({ rawKey: String(rawKey), label: "Rotated API Key" });
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
        setRevealedKey({ rawKey: String(rawKey), label: "New API Key" });
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

  function toggleScope(scope: string) {
    setKeyScopes((prev) =>
      prev.includes(scope) ? prev.filter((s) => s !== scope) : [...prev, scope],
    );
  }

  return (
    <section className="split">
      {teamError && (
        <div className="full-row">
          <StateBanner title="Team operation failed" tone="risk">
            <p>{teamError}</p>
          </StateBanner>
        </div>
      )}
      <ConfirmDialog action={confirmAction} onConfirm={doTeamConfirm} onCancel={() => setConfirmAction(null)} />
      {revealedKey && (
        <KeyRevealModal
          rawKey={revealedKey.rawKey}
          label={revealedKey.label}
          onClose={() => setRevealedKey(null)}
        />
      )}
      <div className="card stack">
        <h2>Members</h2>
        <div className="form-stack">
          <label htmlFor="member-user-id" className="label">User ID</label>
          <input
            id="member-user-id"
            placeholder="user_id"
            value={userId}
            onChange={(e) => setUserId(e.target.value)}
          />
          <label htmlFor="member-display-name" className="label">Display Name</label>
          <input
            id="member-display-name"
            placeholder="display name"
            value={displayName}
            onChange={(e) => setDisplayName(e.target.value)}
          />
          <label htmlFor="member-role" className="label">Role</label>
          <select id="member-role" value={role} onChange={(e) => setRole(e.target.value)}>
            <option value="admin">admin</option>
            <option value="readonly">readonly</option>
          </select>
          <button onClick={handleCreateMember} disabled={busy} type="button" className="button-primary">
            Create Member
          </button>
        </div>
        {dashboard.team.members.length === 0 ? (
          <EmptyState
            title="No local members yet"
            description="Members define who owns local API keys and role boundaries. Add a member before creating scoped keys."
            tone="info"
          />
        ) : (
          dashboard.team.members.map((item) => (
            <div className="row item-row" key={item.user_id}>
              <span>{item.display_name}</span>
              <span className="inline-actions">
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
                  className="risk-action"
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
        <div className="form-stack">
          <label htmlFor="key-user-id" className="label">User ID</label>
          <input
            id="key-user-id"
            placeholder="user_id"
            value={keyUserId}
            onChange={(e) => setKeyUserId(e.target.value)}
          />
          <label htmlFor="key-role" className="label">Role</label>
          <select id="key-role" value={keyRole} onChange={(e) => setKeyRole(e.target.value)}>
            <option value="admin">admin</option>
            <option value="readonly">readonly</option>
          </select>
          <div className="scope-list">
            {ALL_SCOPES.map((s) => (
              <label key={s} className="scope-option">
                <input
                  type="checkbox"
                  checked={keyScopes.includes(s)}
                  onChange={() => toggleScope(s)}
                />
                {s}
              </label>
            ))}
          </div>
          <button onClick={handleCreateKey} disabled={busy} type="button" className="button-primary">
            Create API Key
          </button>
        </div>
        {dashboard.team.api_keys.length === 0 ? (
          <EmptyState
            title="No API key metadata yet"
            description="Create scoped keys to unlock protected dashboard tabs such as audit, team administration, and backups."
            tone="info"
          />
        ) : (
          dashboard.team.api_keys.map((item) => (
            <div className="row item-row wrap-row" key={item.key_id}>
              <span className="mono">{item.key_id}</span>
              <span className={`pill ${item.role === "admin" ? "warn" : "ok"}`}>{item.role}</span>
              {item.scopes && item.scopes.length > 0 && (
                <span className="muted metadata-chip">{item.scopes.join(", ")}</span>
              )}
              {item.last_used_at && (
                <span className="muted metadata-chip">used: {item.last_used_at}</span>
              )}
              {item.expires_at && (
                <span className="muted metadata-chip">expires: {item.expires_at}</span>
              )}
              {item.revoked_at ? (
                <span className="pill warn">revoked</span>
              ) : (
                <span className="inline-actions">
                  <button
                    onClick={() => setConfirmAction({ type: "rotateKey", id: item.key_id })}
                    disabled={busy}
                    type="button"
                  >
                    Rotate
                  </button>
                  <button
                    onClick={() => setConfirmAction({ type: "revokeKey", id: item.key_id })}
                    disabled={busy}
                    type="button"
                  >
                    Revoke
                  </button>
                  <button
                    onClick={() => setConfirmAction({ type: "deleteKey", id: item.key_id })}
                    disabled={busy}
                    type="button"
                    className="risk-action"
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
