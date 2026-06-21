"use client";

import { useCallback, useEffect, useMemo, useState } from "react";
import {
  ApiError,
  fetchAdaptiveFusionPolicies,
  promoteAdaptiveFusionPolicy,
  rollbackAdaptiveFusionPolicy,
} from "@/lib/api-client";
import type {
  AdaptiveFusionPoliciesResponse,
  AdaptivePolicyPromotionRequest,
  AdaptivePolicySnapshot,
} from "@/lib/types";
import { EmptyState } from "./EmptyState";
import {
  AdaptiveFusionPolicyTable,
  AdaptiveFusionSnapshotTable,
} from "./AdaptiveFusionPolicyTables";
import { AdaptiveFusionPromotionForm } from "./AdaptiveFusionPromotionForm";
import { AdaptiveFusionRollbackDialog } from "./AdaptiveFusionRollbackDialog";
import { Metric } from "./Metric";
import { StateBanner } from "./StateBanner";

type FusionError = { message: string; type: "permission" | "error" } | null;

function mapError(error: unknown): FusionError {
  if (error instanceof ApiError && (error.status === 401 || error.status === 403)) {
    return {
      message: error.status === 403
        ? "Current API key lacks dispatch:read or team:admin scope."
        : "Adaptive fusion controls require protected local API access.",
      type: "permission",
    };
  }
  return {
    message: error instanceof Error ? error.message : "Failed to load adaptive fusion policies",
    type: "error",
  };
}

export function AdaptiveFusion() {
  const [data, setData] = useState<AdaptiveFusionPoliciesResponse | null>(null);
  const [error, setError] = useState<FusionError>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [actionMessage, setActionMessage] = useState("");
  const [rollbackTarget, setRollbackTarget] = useState<AdaptivePolicySnapshot | null>(null);

  const load = useCallback(() => {
    setLoading(true);
    fetchAdaptiveFusionPolicies()
      .then((res) => {
        setData(res);
        setError(null);
      })
      .catch((e) => {
        setData(null);
        setError(mapError(e));
      })
      .finally(() => setLoading(false));
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  const activeSnapshots = useMemo(
    () => data?.snapshots.filter((snapshot) => snapshot.status === "active") ?? [],
    [data],
  );

  async function submitPromotion(request: AdaptivePolicyPromotionRequest) {
    setBusy(true);
    setActionMessage("");
    try {
      await promoteAdaptiveFusionPolicy(request);
      setActionMessage("Adaptive policy promotion was accepted.");
      load();
    } catch (e) {
      setActionMessage(e instanceof Error ? e.message : "Adaptive policy promotion failed.");
    } finally {
      setBusy(false);
    }
  }

  async function confirmRollback() {
    if (!rollbackTarget) return;
    setBusy(true);
    setActionMessage("");
    try {
      await rollbackAdaptiveFusionPolicy(rollbackTarget.adjustment_id, {
        actor: "operator",
        reason: "operator requested rollback from dashboard",
      });
      setActionMessage(`Rolled back ${rollbackTarget.adjustment_id}.`);
      setRollbackTarget(null);
      load();
    } catch (e) {
      setActionMessage(e instanceof Error ? e.message : "Adaptive policy rollback failed.");
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className="card stack">
      <div className="flex-between">
        <div>
          <h2>Adaptive Fusion</h2>
          <p className="muted">
            Operator review for promoted contextual policies. Policies remain shadow-first and require explicit adaptive plans.
          </p>
        </div>
        <button disabled={loading} onClick={load} type="button">
          Refresh
        </button>
      </div>

      {error?.type === "permission" && (
        <StateBanner title="Adaptive fusion access restricted" tone="warn">
          <p>{error.message}</p>
        </StateBanner>
      )}
      {error?.type === "error" && (
        <StateBanner title="Adaptive fusion data unavailable" tone="risk">
          <p>{error.message}</p>
        </StateBanner>
      )}

      {loading && !data ? (
        <div className="loading-row"><span className="spinner" /> Loading adaptive fusion policies...</div>
      ) : !data && !error ? (
        <EmptyState
          title="No adaptive fusion data"
          description="Promoted policy state will appear here after AF4 policy gates accept a candidate."
        />
      ) : data ? (
        <>
          <div className="status-strip" aria-label="Adaptive fusion safety summary">
            <Metric label="Active Policies" value={String(data.policies.length)} detail="promoted" />
            <Metric label="Snapshots" value={String(data.snapshots.length)} detail="rollback records" />
            <Metric label="Active Snapshots" value={String(activeSnapshots.length)} detail="rollbackable" />
            <Metric
              label="Live Authority"
              value={data.live_execution_authority ? "yes" : "no"}
              detail="policy only"
              tone={data.live_execution_authority ? "warn" : "ok"}
            />
            <Metric
              label="Explicit Plan"
              value={data.requires_explicit_adaptive_plan ? "required" : "off"}
              detail="execution gate"
              tone={data.requires_explicit_adaptive_plan ? "ok" : "warn"}
            />
          </div>

          <div className="grid two">
            <div className="subcard stack">
              <h3>Active Policies</h3>
              <AdaptiveFusionPolicyTable policies={data.policies} />
            </div>

            <AdaptiveFusionPromotionForm busy={busy} onSubmit={submitPromotion} />
          </div>

          <div className="subcard stack">
            <h3>Policy Snapshots</h3>
            <AdaptiveFusionSnapshotTable
              onRollback={setRollbackTarget}
              snapshots={data.snapshots}
            />
          </div>
        </>
      ) : null}

      {actionMessage && (
        <StateBanner title="Adaptive fusion action result" tone="info">
          <p>{actionMessage}</p>
        </StateBanner>
      )}

      {rollbackTarget && (
        <AdaptiveFusionRollbackDialog
          busy={busy}
          onCancel={() => setRollbackTarget(null)}
          onConfirm={confirmRollback}
          target={rollbackTarget}
        />
      )}
    </section>
  );
}
