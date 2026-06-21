import type {
  AdaptivePolicySnapshot,
  PromotedAdaptivePolicy,
} from "@/lib/types";
import { EmptyState } from "./EmptyState";

function formatPercent(value: number): string {
  return `${Math.round(value * 100)}%`;
}

function formatDelta(value: number, positiveLabel: string, negativeLabel: string): string {
  const label = value >= 0 ? positiveLabel : negativeLabel;
  return `${value >= 0 ? "+" : ""}${formatPercent(value)} ${label}`;
}

function PolicyRow({ policy }: { policy: PromotedAdaptivePolicy }) {
  return (
    <tr>
      <td>
        <strong>{policy.task_class}</strong>
        <br />
        <span className="muted mono">{policy.policy_key}</span>
      </td>
      <td>{policy.objective}</td>
      <td>
        <span className="mono">{policy.candidate_id}</span>
        <br />
        <span className="muted">baseline {policy.baseline_candidate_id}</span>
      </td>
      <td>{policy.sample_count}</td>
      <td>{formatPercent(policy.confidence)}</td>
      <td>{formatDelta(policy.mean_quality_delta, "quality", "quality")}</td>
      <td>{formatDelta(policy.mean_cost_reduction, "cost reduction", "cost increase")}</td>
      <td>
        {policy.live_execution_authority ? (
          <span className="pill risk">live</span>
        ) : (
          <span className="pill info">shadow</span>
        )}
      </td>
    </tr>
  );
}

function SnapshotRow({
  onRollback,
  snapshot,
}: {
  onRollback: (snapshot: AdaptivePolicySnapshot) => void;
  snapshot: AdaptivePolicySnapshot;
}) {
  const active = snapshot.status === "active";
  return (
    <tr>
      <td>
        <strong>{snapshot.adjustment_id}</strong>
        <br />
        <span className="muted">{snapshot.updated_at}</span>
      </td>
      <td>{snapshot.policy_key}</td>
      <td>{snapshot.candidate_id}</td>
      <td>{snapshot.actor}</td>
      <td>
        <span className={active ? "pill ok" : "pill info"}>{snapshot.status}</span>
      </td>
      <td>
        <button
          disabled={!active}
          onClick={() => onRollback(snapshot)}
          type="button"
        >
          Rollback
        </button>
      </td>
    </tr>
  );
}

export function AdaptiveFusionPolicyTable({
  policies,
}: {
  policies: PromotedAdaptivePolicy[];
}) {
  if (policies.length === 0) {
    return (
      <EmptyState
        title="No promoted policies"
        description="Promotion is gated by evidence, env flags, and explicit confirmation."
      />
    );
  }

  return (
    <div className="table-wrap">
      <table>
        <thead>
          <tr>
            <th>Policy</th>
            <th>Objective</th>
            <th>Candidate</th>
            <th>Samples</th>
            <th>Confidence</th>
            <th>Quality</th>
            <th>Cost</th>
            <th>Authority</th>
          </tr>
        </thead>
        <tbody>
          {policies.map((policy) => (
            <PolicyRow key={policy.policy_key} policy={policy} />
          ))}
        </tbody>
      </table>
    </div>
  );
}

export function AdaptiveFusionSnapshotTable({
  onRollback,
  snapshots,
}: {
  onRollback: (snapshot: AdaptivePolicySnapshot) => void;
  snapshots: AdaptivePolicySnapshot[];
}) {
  if (snapshots.length === 0) {
    return (
      <EmptyState
        title="No policy snapshots"
        description="Accepted promotions create rollback snapshots with safety hashes."
      />
    );
  }

  return (
    <div className="table-wrap">
      <table>
        <thead>
          <tr>
            <th>Snapshot</th>
            <th>Policy Key</th>
            <th>Candidate</th>
            <th>Actor</th>
            <th>Status</th>
            <th>Action</th>
          </tr>
        </thead>
        <tbody>
          {snapshots.map((snapshot) => (
            <SnapshotRow
              key={snapshot.snapshot_id}
              onRollback={onRollback}
              snapshot={snapshot}
            />
          ))}
        </tbody>
      </table>
    </div>
  );
}
