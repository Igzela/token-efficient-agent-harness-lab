import { useEffect, useRef } from "react";

export type ConfirmAction =
  | { type: "deleteBackup" | "restoreBackup"; backupId: string }
  | { type: "deleteMember" | "revokeKey" | "deleteKey" | "rotateKey"; id: string }
  | { type: "cleanupWorkspace" | "quarantineWorkspace" | "capturePatch"; workspaceId: string }
  | {
      type: "verifyWorkspace";
      workspaceId: string;
      command: string;
      repairExecutor?: "codex_cli" | "claude_code_cli";
    }
  | { type: "approveArtifact" | "rejectArtifact" | "exportArtifact"; artifactId: string; runId: string }
  | { type: "targetOutput"; artifactId: string; mode: "export_patch" | "push_branch" }
  | { type: "approveProposal" | "rejectProposal" | "rollbackProposal" | "deactivateProposal"; proposalId: string }
  | { type: "tickRun" | "cancelRun"; runId: string }
  | { type: "schedulerControl"; action: "pause" | "resume" | "kill" }
  | null;

const messages: Record<string, string> = {};

export function ConfirmDialog({
  action,
  onConfirm,
  onCancel,
}: {
  action: ConfirmAction;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  const cancelRef = useRef<HTMLButtonElement>(null);
  const cardRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!action) return;
    cancelRef.current?.focus();

    function handleKeyDown(e: KeyboardEvent) {
      if (e.key === "Escape") {
        e.preventDefault();
        onCancel();
      }
      if (e.key === "Tab" && cardRef.current) {
        const focusable = cardRef.current.querySelectorAll<HTMLElement>(
          'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])',
        );
        if (focusable.length === 0) return;
        const first = focusable[0];
        const last = focusable[focusable.length - 1];
        if (e.shiftKey && document.activeElement === first) {
          e.preventDefault();
          last.focus();
        } else if (!e.shiftKey && document.activeElement === last) {
          e.preventDefault();
          first.focus();
        }
      }
    }

    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [action, onCancel]);

  if (!action) return null;
  const msg =
    messages[action.type] ??
    (action.type === "deleteBackup"
      ? `Delete backup ${(action as { backupId: string }).backupId}? This cannot be undone.`
      : action.type === "restoreBackup"
        ? `Restore from backup ${(action as { backupId: string }).backupId}? Current data will be replaced.`
        : action.type === "deleteMember"
          ? `Delete member ${(action as { id: string }).id}? This cannot be undone.`
          : action.type === "revokeKey"
            ? `Revoke key ${(action as { id: string }).id}? This key will no longer authenticate.`
            : action.type === "deleteKey"
              ? `Permanently delete key ${(action as { id: string }).id}? This cannot be undone.`
              : action.type === "rotateKey"
                ? `Rotate key ${(action as { id: string }).id}? A new key will be created and the old one revoked.`
                : action.type === "cleanupWorkspace"
                  ? `Clean up workspace ${(action as { workspaceId: string }).workspaceId.slice(0, 12)}? This transitions the workspace to cleaned status.`
                  : action.type === "quarantineWorkspace"
                    ? `Quarantine workspace ${(action as { workspaceId: string }).workspaceId.slice(0, 12)}? This isolates the workspace.`
                    : action.type === "verifyWorkspace"
                      ? `Run "${(action as { command: string }).command}" in workspace ${(action as { workspaceId: string }).workspaceId.slice(0, 12)}?`
                    : action.type === "capturePatch"
                      ? `Capture patch from workspace ${(action as { workspaceId: string }).workspaceId.slice(0, 12)}?`
                      : action.type === "approveArtifact"
                        ? `Approve artifact ${(action as { artifactId: string }).artifactId.slice(0, 12)}? This binds the approval to run ${(action as { runId: string }).runId.slice(0, 12)}.`
                        : action.type === "rejectArtifact"
                          ? `Reject artifact ${(action as { artifactId: string }).artifactId.slice(0, 12)}?`
                          : action.type === "exportArtifact"
                            ? `Export artifact ${(action as { artifactId: string }).artifactId.slice(0, 12)}? Requires valid approval binding.`
                            : action.type === "targetOutput"
                              ? `${(action as { mode: string }).mode === "push_branch" ? "Push branch output" : "Export patch output"} for artifact ${(action as { artifactId: string }).artifactId.slice(0, 12)}? This requires approval binding and explicit target-output gates.`
                            : action.type === "approveProposal"
                              ? `Approve proposal ${(action as { proposalId: string }).proposalId.slice(0, 12)}? This records explicit human approval.`
                              : action.type === "rejectProposal"
                                ? `Reject proposal ${(action as { proposalId: string }).proposalId.slice(0, 12)}? The controlled loop will not apply it.`
                                : action.type === "rollbackProposal"
                                  ? `Rollback proposal ${(action as { proposalId: string }).proposalId.slice(0, 12)}? This requires human confirmation.`
                                  : action.type === "deactivateProposal"
                                    ? `Deactivate proposal ${(action as { proposalId: string }).proposalId.slice(0, 12)}? The policy will no longer be applied.`
                                    : action.type === "tickRun"
                                    ? `Execute one tick on run ${(action as { runId: string }).runId.slice(0, 12)}? This will advance the next ready node.`
                                    : action.type === "cancelRun"
                                      ? `Cancel run ${(action as { runId: string }).runId.slice(0, 12)}? This will stop execution.`
                                      : action.type === "schedulerControl"
                                        ? `${(action as { action: string }).action} supervised workers? This action is audited and requires scheduler control authority.`
                                      : "Are you sure?");
  return (
    <div className="confirm-overlay" onClick={onCancel} role="dialog" aria-modal="true" aria-label={msg}>
      <div className="confirm-card" onClick={(e) => e.stopPropagation()} ref={cardRef}>
        <p>{msg}</p>
        <div className="flex-end">
          <button onClick={onCancel} type="button" ref={cancelRef}>Cancel</button>
          <button onClick={onConfirm} type="button" className="risk-action">Confirm</button>
        </div>
      </div>
    </div>
  );
}
