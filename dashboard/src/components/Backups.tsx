import { useEffect, useState } from "react";
import { createBackup, deleteBackup, fetchBackups, restoreBackup } from "@/lib/api-client";
import { ConfirmDialog, type ConfirmAction } from "./ConfirmDialog";

export function Backups() {
  const [backups, setBackups] = useState<Array<Record<string, unknown>>>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [confirm, setConfirm] = useState<ConfirmAction>(null);

  function load() {
    fetchBackups()
      .then((r) => { setBackups((r.backups as Array<Record<string, unknown>>) ?? []); setError(null); })
      .catch((e) => setError(e instanceof Error ? e.message : "Failed to load backups"));
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
