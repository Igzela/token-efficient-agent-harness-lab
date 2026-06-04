import { useEffect, useState } from "react";
import { ApiError, createBackup, deleteBackup, fetchBackups, restoreBackup } from "@/lib/api-client";
import { ConfirmDialog, type ConfirmAction } from "./ConfirmDialog";
import { EmptyState } from "./EmptyState";
import { StateBanner } from "./StateBanner";

type BackupError = {
  message: string;
  status?: number;
  type: "permission" | "error";
};

function backupError(error: unknown, fallback: string): BackupError {
  if (error instanceof ApiError && (error.status === 401 || error.status === 403)) {
    return {
      message: error.status === 403
        ? "The current API key does not include backup:admin scope."
        : "Local backups are available only when protected mode is enabled.",
      status: error.status,
      type: "permission",
    };
  }
  return {
    message: error instanceof Error ? error.message : fallback,
    type: "error",
  };
}

export function Backups() {
  const [backups, setBackups] = useState<Array<Record<string, unknown>>>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<BackupError | null>(null);
  const [confirm, setConfirm] = useState<ConfirmAction>(null);

  function load() {
    fetchBackups()
      .then((r) => { setBackups((r.backups as Array<Record<string, unknown>>) ?? []); setError(null); })
      .catch((e) => setError(backupError(e, "Failed to load backups")));
  }

  useEffect(() => { load(); }, []);

  async function doConfirm() {
    if (!confirm || !("backupId" in confirm)) return;
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
      setError(backupError(e, "Operation failed"));
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
      setError(backupError(e, "Backup creation failed"));
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className="card stack">
      <div className="flex-between">
        <h2>Backups</h2>
        <button disabled={busy || error?.type === "permission"} onClick={handleCreateBackup} type="button" className="button-primary">
          Create Backup
        </button>
      </div>
      {error?.type === "permission" && (
        <StateBanner title="Protected backup access required" tone="warn">
          <p>{error.message}</p>
          <p>
            Start the engine with <code>ACP_REQUIRE_AUTH=1</code>, set an admin key, then use a
            token with <code>backup:admin</code> scope.
          </p>
        </StateBanner>
      )}
      {error?.type === "error" && <StateBanner title="Backup state unavailable" tone="risk"><p>{error.message}</p></StateBanner>}
      <ConfirmDialog action={confirm} onConfirm={doConfirm} onCancel={() => setConfirm(null)} />
      {backups.length === 0 && !error ? (
        <EmptyState
          title="No local backups yet"
          description="Backups protect the app-owned SQLite state. Create one before risky local maintenance such as restore testing or data migration."
          tone="info"
        />
      ) : error?.type === "permission" ? (
        <EmptyState
          title="Backups are locked"
          description="This tab is intentionally admin-only. The rest of the dashboard can remain readable while backup create, restore, and delete stay protected."
          tone="warn"
        >
          <div className="command-block">
            <span className="label">Protected runtime example</span>
            <code>ACP_REQUIRE_AUTH=1 ACP_ADMIN_API_KEY=harness_&lt;64 hex&gt; ACP_DASHBOARD_DIR=dashboard/out cargo run -p engine</code>
          </div>
        </EmptyState>
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
                <td className="action-cell">
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
                    className="risk-action"
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
