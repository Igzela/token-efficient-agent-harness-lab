import type { AdaptivePolicySnapshot } from "@/lib/types";

export function AdaptiveFusionRollbackDialog({
  busy,
  onCancel,
  onConfirm,
  target,
}: {
  busy: boolean;
  onCancel: () => void;
  onConfirm: () => void;
  target: AdaptivePolicySnapshot;
}) {
  return (
    <div className="confirm-overlay" role="presentation">
      <section
        aria-labelledby="adaptive-rollback-title"
        aria-modal="true"
        className="confirm-card stack"
        role="dialog"
      >
        <h3 id="adaptive-rollback-title">Rollback adaptive policy?</h3>
        <p>
          This will rollback <span className="mono">{target.adjustment_id}</span> and restore the prior
          policy state recorded by its validated snapshot.
        </p>
        <div className="flex-between">
          <button disabled={busy} onClick={onCancel} type="button">
            Cancel
          </button>
          <button disabled={busy} onClick={onConfirm} type="button">
            Confirm Rollback
          </button>
        </div>
      </section>
    </div>
  );
}
