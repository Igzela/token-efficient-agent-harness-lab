import { useCallback, useEffect, useState } from "react";
import {
  ApiError,
  captureSupervisedPatch,
  cleanupSupervisedPatchWorkspace,
  createSupervisedPatchWorkspace,
  exportSupervisedPatchArtifact,
  fetchSupervisedPatchArtifactDetail,
  fetchSupervisedPatchArtifacts,
  fetchSupervisedPatchWorkspaceDetail,
  fetchSupervisedPatchWorkspaces,
  quarantineSupervisedPatchWorkspace,
  recordWorkflowRunApproval,
  verifySupervisedPatchWorkspace,
} from "@/lib/api-client";
import type {
  SupervisedPatchArtifact,
  SupervisedPatchWorkspace,
} from "@/lib/types";
import { ConfirmDialog, type ConfirmAction } from "./ConfirmDialog";
import { EmptyState } from "./EmptyState";
import { StateBanner } from "./StateBanner";

type PatchError = {
  message: string;
  type: "permission" | "error";
};

function patchError(error: unknown): PatchError {
  if (error instanceof ApiError && (error.status === 401 || error.status === 403)) {
    return {
      message: error.status === 403
        ? "The current API key lacks dispatch:read scope for supervised patch metadata."
        : "Supervised patch metadata requires protected local API access.",
      type: "permission",
    };
  }
  return {
    message: error instanceof Error ? error.message : "Failed to load supervised patch metadata",
    type: "error",
  };
}

function BoundaryBadges({ metadataOnly, executionAuthority, patchApplyAuthority, verificationAuthority }: {
  metadataOnly?: boolean;
  executionAuthority?: string;
  patchApplyAuthority?: string;
  verificationAuthority?: string;
}) {
  return (
    <div className="flex-row gap-sm" style={{ marginTop: "0.25rem" }}>
      {metadataOnly && <span className="pill info">metadata only</span>}
      {executionAuthority && (
        <span className={`pill ${executionAuthority === "disabled" ? "ok" : "warn"}`}>
          exec: {executionAuthority}
        </span>
      )}
      {verificationAuthority && (
        <span className="pill warn">verify: {verificationAuthority}</span>
      )}
      {patchApplyAuthority && (
        <span className={`pill ${patchApplyAuthority === "disabled" ? "ok" : "warn"}`}>
          apply: {patchApplyAuthority}
        </span>
      )}
    </div>
  );
}

function CreateWorkspaceForm({
  onCreated,
  onCancel,
}: {
  onCreated: () => void;
  onCancel: () => void;
}) {
  const [runId, setRunId] = useState("");
  const [targetId, setTargetId] = useState("");
  const [targetRepoPath, setTargetRepoPath] = useState("");
  const [sourceRevision, setSourceRevision] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    setSubmitting(true);
    setError(null);
    try {
      await createSupervisedPatchWorkspace({
        run_id: runId,
        target_id: targetId,
        target_repo_path: targetRepoPath,
        source_revision: sourceRevision,
      });
      onCreated();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to create workspace");
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <form onSubmit={handleSubmit} className="card stack" style={{ marginTop: "0.5rem" }}>
      <h3>Create Workspace</h3>
      {error && <StateBanner title="Error" tone="risk"><p>{error}</p></StateBanner>}
      <label className="stack" style={{ gap: "0.25rem" }}>
        <span className="muted">Run ID</span>
        <input
          type="text"
          value={runId}
          onChange={(e) => setRunId(e.target.value)}
          required
          placeholder="run-..."
        />
      </label>
      <label className="stack" style={{ gap: "0.25rem" }}>
        <span className="muted">Target ID</span>
        <input
          type="text"
          value={targetId}
          onChange={(e) => setTargetId(e.target.value)}
          required
          placeholder="target-..."
        />
      </label>
      <label className="stack" style={{ gap: "0.25rem" }}>
        <span className="muted">Target repo path</span>
        <input
          type="text"
          value={targetRepoPath}
          onChange={(e) => setTargetRepoPath(e.target.value)}
          required
          placeholder="/path/to/repo"
        />
      </label>
      <label className="stack" style={{ gap: "0.25rem" }}>
        <span className="muted">Source revision</span>
        <input
          type="text"
          value={sourceRevision}
          onChange={(e) => setSourceRevision(e.target.value)}
          required
          placeholder="commit hash or ref"
        />
      </label>
      <div className="flex-end" style={{ gap: "0.5rem" }}>
        <button type="button" onClick={onCancel} disabled={submitting}>Cancel</button>
        <button type="submit" disabled={submitting}>
          {submitting ? "Creating..." : "Create"}
        </button>
      </div>
    </form>
  );
}

function WorkspaceDetail({
  workspace,
  onBack,
  onMutated,
}: {
  workspace: SupervisedPatchWorkspace;
  onBack: () => void;
  onMutated: () => void;
}) {
  const [confirmAction, setConfirmAction] = useState<ConfirmAction>(null);
  const [mutating, setMutating] = useState(false);
  const [mutationError, setMutationError] = useState<string | null>(null);
  const [verificationCommand, setVerificationCommand] = useState("");
  const verificationReady = workspace.verification?.status === "evidence_recorded";

  function handleConfirm() {
    if (!confirmAction) return;
    const action = confirmAction;
    setConfirmAction(null);
    setMutating(true);
    setMutationError(null);

    const promise =
      action.type === "cleanupWorkspace"
        ? cleanupSupervisedPatchWorkspace(action.workspaceId)
        : action.type === "quarantineWorkspace"
          ? quarantineSupervisedPatchWorkspace(action.workspaceId)
          : action.type === "verifyWorkspace"
            ? verifySupervisedPatchWorkspace(action.workspaceId, {
                command: action.command,
                confirm_verification: true,
                timeout_ms: 600_000,
              })
          : action.type === "capturePatch"
            ? captureSupervisedPatch(action.workspaceId)
            : Promise.resolve();

    promise
      .then(() => onMutated())
      .catch((err) => setMutationError(err instanceof Error ? err.message : "Operation failed"))
      .finally(() => setMutating(false));
  }

  return (
    <div className="card stack">
      <div className="flex-between">
        <h3>Workspace {workspace.workspace_id.slice(0, 12)}</h3>
        <button onClick={onBack} type="button">Back to list</button>
      </div>
      <BoundaryBadges
        metadataOnly={workspace.metadata_only}
        executionAuthority={workspace.execution_authority}
        verificationAuthority={workspace.verification_execution_authority}
      />
      {mutationError && (
        <StateBanner title="Operation failed" tone="risk"><p>{mutationError}</p></StateBanner>
      )}
      <div className="subcard stack">
        <h4>Details</h4>
        <div className="kv-row"><span className="muted">Status</span><span>{workspace.status}</span></div>
        <div className="kv-row"><span className="muted">Target</span><span>{workspace.target_id}</span></div>
        <div className="kv-row"><span className="muted">Target repo path</span><span className="mono" style={{ fontSize: "0.8rem" }}>{workspace.target_repo_path}</span></div>
        <div className="kv-row"><span className="muted">Workspace path</span><span className="mono" style={{ fontSize: "0.8rem" }}>{workspace.workspace_path}</span></div>
        <div className="kv-row"><span className="muted">Source revision</span><span className="mono" style={{ fontSize: "0.8rem" }}>{workspace.source_revision}</span></div>
        {workspace.source_tree_hash && (
          <div className="kv-row"><span className="muted">Source tree hash</span><span className="mono" style={{ fontSize: "0.8rem" }}>{workspace.source_tree_hash}</span></div>
        )}
        <div className="kv-row"><span className="muted">Created</span><span>{workspace.created_at}</span></div>
        <div className="kv-row"><span className="muted">Updated</span><span>{workspace.updated_at}</span></div>
        <div className="kv-row">
          <span className="muted">Verification</span>
          <span>{workspace.verification?.status ?? "not run"}</span>
        </div>
        {workspace.plan_id && <div className="kv-row"><span className="muted">Plan</span><span className="mono" style={{ fontSize: "0.8rem" }}>{workspace.plan_id}</span></div>}
        <div className="kv-row"><span className="muted">Run</span><span className="mono" style={{ fontSize: "0.8rem" }}>{workspace.run_id}</span></div>
      </div>
      {Object.keys(workspace.boundary).length > 0 && (
        <div className="subcard stack">
          <h4>Boundary</h4>
          {Object.entries(workspace.boundary).map(([k, v]) => (
            <div className="kv-row" key={k}>
              <span className="muted">{k}</span>
              <span>{typeof v === "string" ? v : JSON.stringify(v)}</span>
            </div>
          ))}
        </div>
      )}
      <div className="subcard stack">
        <h4>Verification</h4>
        <label className="stack" style={{ gap: "0.25rem" }}>
          <span className="muted">Allowlisted command</span>
          <input
            type="text"
            value={verificationCommand}
            onChange={(event) => setVerificationCommand(event.target.value)}
            placeholder="cargo test / npm test"
          />
        </label>
        <div className="flex-end">
          <button
            type="button"
            onClick={() => setConfirmAction({
              type: "verifyWorkspace",
              workspaceId: workspace.workspace_id,
              command: verificationCommand.trim(),
            })}
            disabled={mutating || !verificationCommand.trim()}
          >
            {mutating ? "Working..." : "Verify Workspace"}
          </button>
        </div>
      </div>
      <div className="flex-end" style={{ gap: "0.5rem" }}>
        <button
          type="button"
          onClick={() => setConfirmAction({ type: "capturePatch", workspaceId: workspace.workspace_id })}
          disabled={mutating || !verificationReady}
        >
          {mutating ? "Working..." : "Capture Patch"}
        </button>
        <button
          type="button"
          onClick={() => setConfirmAction({ type: "cleanupWorkspace", workspaceId: workspace.workspace_id })}
          disabled={mutating}
        >
          {mutating ? "Working..." : "Cleanup"}
        </button>
        <button
          type="button"
          className="risk-action"
          onClick={() => setConfirmAction({ type: "quarantineWorkspace", workspaceId: workspace.workspace_id })}
          disabled={mutating}
        >
          {mutating ? "Working..." : "Quarantine"}
        </button>
      </div>
      <ConfirmDialog
        action={confirmAction}
        onConfirm={handleConfirm}
        onCancel={() => setConfirmAction(null)}
      />
    </div>
  );
}

function ArtifactDetail({
  artifact,
  onBack,
  onMutated,
}: {
  artifact: SupervisedPatchArtifact;
  onBack: () => void;
  onMutated: () => void;
}) {
  const [confirmAction, setConfirmAction] = useState<ConfirmAction>(null);
  const [mutating, setMutating] = useState(false);
  const [mutationError, setMutationError] = useState<string | null>(null);
  const [exportResult, setExportResult] = useState<Record<string, unknown> | null>(null);

  function handleConfirm() {
    if (!confirmAction) return;
    const action = confirmAction;
    setConfirmAction(null);
    setMutating(true);
    setMutationError(null);

    if (action.type === "approveArtifact" || action.type === "rejectArtifact") {
      recordWorkflowRunApproval(action.runId, {
        node_id: "dashboard-output-approval",
        decision: action.type === "approveArtifact" ? "approved" : "rejected",
        reason: action.type === "approveArtifact" ? "dashboard approval" : "dashboard rejection",
        bound_patch_hash: artifact.patch_hash,
        bound_source_revision: artifact.source_revision,
        bound_changed_files: artifact.changed_files,
        expires_at: "2099-12-31T23:59:59Z",
      })
        .then(() => onMutated())
        .catch((err) => setMutationError(err instanceof Error ? err.message : "Approval failed"))
        .finally(() => setMutating(false));
    } else if (action.type === "exportArtifact") {
      exportSupervisedPatchArtifact(action.artifactId, action.runId)
        .then((result) => {
          setExportResult(result.export as Record<string, unknown>);
        })
        .catch((err) => setMutationError(err instanceof Error ? err.message : "Export failed"))
        .finally(() => setMutating(false));
    }
  }

  return (
    <div className="card stack">
      <div className="flex-between">
        <h3>Artifact {artifact.artifact_id.slice(0, 12)}</h3>
        <button onClick={onBack} type="button">Back to list</button>
      </div>
      <BoundaryBadges
        metadataOnly={artifact.metadata_only}
        executionAuthority={artifact.execution_authority}
        patchApplyAuthority={artifact.patch_apply_authority}
      />
      {mutationError && (
        <StateBanner title="Operation failed" tone="risk"><p>{mutationError}</p></StateBanner>
      )}
      {exportResult && (
        <StateBanner title="Export succeeded" tone="ok">
          <p>Artifact exported by {String(exportResult.exported_by ?? "unknown")} at {String(exportResult.exported_at ?? "unknown")}</p>
        </StateBanner>
      )}
      <div className="subcard stack">
        <h4>Details</h4>
        <div className="kv-row"><span className="muted">Type</span><span>{artifact.artifact_type}</span></div>
        <div className="kv-row"><span className="muted">Workspace</span><span className="mono" style={{ fontSize: "0.8rem" }}>{artifact.workspace_id}</span></div>
        <div className="kv-row"><span className="muted">Target</span><span>{artifact.target_id}</span></div>
        <div className="kv-row"><span className="muted">Source revision</span><span className="mono" style={{ fontSize: "0.8rem" }}>{artifact.source_revision}</span></div>
        <div className="kv-row"><span className="muted">Patch hash</span><span className="mono" style={{ fontSize: "0.8rem" }}>{artifact.patch_hash}</span></div>
        <div className="kv-row"><span className="muted">Redaction</span>
          <span className={`pill ${artifact.redaction_status === "redacted" ? "ok" : artifact.redaction_status === "failed" ? "risk" : "warn"}`}>
            {artifact.redaction_status}
          </span>
        </div>
        <div className="kv-row"><span className="muted">Created</span><span>{artifact.created_at}</span></div>
        {artifact.plan_id && <div className="kv-row"><span className="muted">Plan</span><span className="mono" style={{ fontSize: "0.8rem" }}>{artifact.plan_id}</span></div>}
        <div className="kv-row"><span className="muted">Run</span><span className="mono" style={{ fontSize: "0.8rem" }}>{artifact.run_id}</span></div>
      </div>
      {artifact.changed_files.length > 0 && (
        <div className="subcard stack">
          <h4>Changed files ({artifact.changed_files.length})</h4>
          <ul style={{ margin: 0, paddingLeft: "1.25rem" }}>
            {artifact.changed_files.map((f) => (
              <li key={f} className="mono" style={{ fontSize: "0.8rem" }}>{f}</li>
            ))}
          </ul>
        </div>
      )}
      {artifact.storage_refs && Object.keys(artifact.storage_refs).length > 0 && (
        <div className="subcard stack">
          <h4>Storage refs</h4>
          {Object.entries(artifact.storage_refs).map(([k, v]) => (
            <div className="kv-row" key={k}>
              <span className="muted">{k}</span>
              <span className="mono" style={{ fontSize: "0.8rem" }}>{typeof v === "string" ? v : JSON.stringify(v)}</span>
            </div>
          ))}
        </div>
      )}
      <div className="flex-end" style={{ gap: "0.5rem" }}>
        <button
          type="button"
          onClick={() => setConfirmAction({ type: "approveArtifact", artifactId: artifact.artifact_id, runId: artifact.run_id })}
          disabled={mutating}
        >
          {mutating ? "Working..." : "Approve"}
        </button>
        <button
          type="button"
          className="risk-action"
          onClick={() => setConfirmAction({ type: "rejectArtifact", artifactId: artifact.artifact_id, runId: artifact.run_id })}
          disabled={mutating}
        >
          {mutating ? "Working..." : "Reject"}
        </button>
        <button
          type="button"
          onClick={() => setConfirmAction({ type: "exportArtifact", artifactId: artifact.artifact_id, runId: artifact.run_id })}
          disabled={mutating}
        >
          {mutating ? "Working..." : "Export"}
        </button>
      </div>
      <ConfirmDialog
        action={confirmAction}
        onConfirm={handleConfirm}
        onCancel={() => setConfirmAction(null)}
      />
    </div>
  );
}

export function SupervisedPatch() {
  const [workspaces, setWorkspaces] = useState<SupervisedPatchWorkspace[]>([]);
  const [artifacts, setArtifacts] = useState<SupervisedPatchArtifact[]>([]);
  const [error, setError] = useState<PatchError | null>(null);
  const [loading, setLoading] = useState(true);
  const [detailMode, setDetailMode] = useState<{ kind: "workspace"; id: string } | { kind: "artifact"; id: string } | null>(null);
  const [detailData, setDetailData] = useState<SupervisedPatchWorkspace | SupervisedPatchArtifact | null>(null);
  const [detailLoading, setDetailLoading] = useState(false);
  const [showCreateForm, setShowCreateForm] = useState(false);

  const load = useCallback(() => {
    setLoading(true);
    setError(null);
    setDetailMode(null);
    setDetailData(null);
    setWorkspaces([]);
    setArtifacts([]);
    Promise.allSettled([
      fetchSupervisedPatchWorkspaces({ limit: 50 }),
      fetchSupervisedPatchArtifacts({ limit: 50 }),
    ]).then(([wsResult, arResult]) => {
      const nextWorkspaces = wsResult.status === "fulfilled" ? wsResult.value.workspaces : [];
      const nextArtifacts = arResult.status === "fulfilled" ? arResult.value.artifacts : [];
      const firstError =
        wsResult.status === "rejected"
          ? patchError(wsResult.reason)
          : arResult.status === "rejected"
            ? patchError(arResult.reason)
            : null;
      setWorkspaces(nextWorkspaces);
      setArtifacts(nextArtifacts);
      setError(firstError);
    }).finally(() => setLoading(false));
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  function openDetail(kind: "workspace" | "artifact", id: string) {
    setDetailLoading(true);
    setDetailMode({ kind, id });
    const fetcher = kind === "workspace"
      ? fetchSupervisedPatchWorkspaceDetail(id).then((r) => r.workspace)
      : fetchSupervisedPatchArtifactDetail(id).then((r) => r.artifact);
    fetcher
      .then((data) => setDetailData(data))
      .catch((e) => setError(patchError(e)))
      .finally(() => setDetailLoading(false));
  }

  function handleMutated() {
    load();
  }

  const empty = workspaces.length === 0 && artifacts.length === 0;

  return (
    <section className="card stack">
      <div className="flex-between">
        <h2>Supervised Patch</h2>
        <div className="flex-end" style={{ gap: "0.5rem" }}>
          <button onClick={() => setShowCreateForm(!showCreateForm)} type="button">
            {showCreateForm ? "Hide Form" : "Create Workspace"}
          </button>
          <button onClick={load} type="button">Refresh</button>
        </div>
      </div>
      {showCreateForm && (
        <CreateWorkspaceForm
          onCreated={() => { setShowCreateForm(false); load(); }}
          onCancel={() => setShowCreateForm(false)}
        />
      )}
      <StateBanner title="Guarded local operations" tone="info">
        <p>
          Verification is limited to allowlisted commands in app-owned workspaces. Capture and output remain approval-bound.
        </p>
      </StateBanner>
      {error?.type === "permission" && (
        <StateBanner title="Permission required" tone="warn">
          <p>{error.message}</p>
        </StateBanner>
      )}
      {error?.type === "error" && (
        <StateBanner title="Failed to load" tone="risk">
          <p>{error.message}</p>
        </StateBanner>
      )}
      {loading ? (
        <div className="loading-row"><span className="spinner" /> Loading supervised patch metadata...</div>
      ) : detailMode && detailLoading ? (
        <div className="loading-row"><span className="spinner" /> Loading detail...</div>
      ) : detailMode && detailData ? (
        detailMode.kind === "workspace" ? (
          <WorkspaceDetail
            workspace={detailData as SupervisedPatchWorkspace}
            onBack={() => { setDetailMode(null); setDetailData(null); }}
            onMutated={handleMutated}
          />
        ) : (
          <ArtifactDetail
            artifact={detailData as SupervisedPatchArtifact}
            onBack={() => { setDetailMode(null); setDetailData(null); }}
            onMutated={handleMutated}
          />
        )
      ) : empty && !error ? (
        <EmptyState
          title="No supervised patch records"
          description="Patch workspace and artifact metadata will appear here once stored by the engine."
          tone="info"
        />
      ) : (
        <>
          {workspaces.length > 0 && (
            <div className="subcard stack">
              <h3>Workspaces ({workspaces.length})</h3>
              <table className="table">
                <thead>
                  <tr>
                    <th>ID</th>
                    <th>Target</th>
                    <th>Status</th>
                    <th>Source rev</th>
                    <th>Created</th>
                    <th />
                  </tr>
                </thead>
                <tbody>
                  {workspaces.map((ws) => (
                    <tr key={ws.workspace_id}>
                      <td className="mono" style={{ fontSize: "0.8rem" }}>{ws.workspace_id.slice(0, 12)}</td>
                      <td>{ws.target_id}</td>
                      <td><span className="pill info">{ws.status}</span></td>
                      <td className="mono" style={{ fontSize: "0.8rem" }}>{ws.source_revision.slice(0, 12)}</td>
                      <td>{ws.created_at}</td>
                      <td>
                        <button onClick={() => openDetail("workspace", ws.workspace_id)} type="button">View</button>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
          {artifacts.length > 0 && (
            <div className="subcard stack">
              <h3>Artifacts ({artifacts.length})</h3>
              <table className="table">
                <thead>
                  <tr>
                    <th>ID</th>
                    <th>Type</th>
                    <th>Workspace</th>
                    <th>Redaction</th>
                    <th>Files</th>
                    <th>Created</th>
                    <th />
                  </tr>
                </thead>
                <tbody>
                  {artifacts.map((ar) => (
                    <tr key={ar.artifact_id}>
                      <td className="mono" style={{ fontSize: "0.8rem" }}>{ar.artifact_id.slice(0, 12)}</td>
                      <td>{ar.artifact_type}</td>
                      <td className="mono" style={{ fontSize: "0.8rem" }}>{ar.workspace_id.slice(0, 12)}</td>
                      <td>
                        <span className={`pill ${ar.redaction_status === "redacted" ? "ok" : ar.redaction_status === "failed" ? "risk" : "warn"}`}>
                          {ar.redaction_status}
                        </span>
                      </td>
                      <td>{ar.changed_files.length}</td>
                      <td>{ar.created_at}</td>
                      <td>
                        <button onClick={() => openDetail("artifact", ar.artifact_id)} type="button">View</button>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </>
      )}
    </section>
  );
}
