export type ConfirmAction =
  | { type: "deleteBackup" | "restoreBackup"; backupId: string }
  | { type: "deleteMember" | "revokeKey" | "deleteKey" | "rotateKey"; id: string }
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
                : "Are you sure?");
  return (
    <div className="confirm-overlay" onClick={onCancel}>
      <div className="confirm-card" onClick={(e) => e.stopPropagation()}>
        <p>{msg}</p>
        <div className="flex-end">
          <button onClick={onCancel} type="button">Cancel</button>
          <button onClick={onConfirm} type="button" className="risk-action">Confirm</button>
        </div>
      </div>
    </div>
  );
}
