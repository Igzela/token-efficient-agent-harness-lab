import { useEffect, useRef } from "react";

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
