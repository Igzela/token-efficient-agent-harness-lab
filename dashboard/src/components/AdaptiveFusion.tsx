"use client";

import { useCallback, useEffect, useMemo, useState, type FormEvent } from "react";
import {
  ApiError,
  createAdaptiveCompletion,
  fetchAdaptiveFusionPolicies,
  promoteAdaptiveFusionPolicy,
  rollbackAdaptiveFusionPolicy,
} from "@/lib/api-client";
import type {
  AdaptiveCompletionRequest,
  AdaptiveCompletionResponse,
  AdaptiveFusionPoliciesResponse,
  AdaptiveFusionOperatorStatus,
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
type CompletionForm = {
  includeRoutingMetadata: boolean;
  objective: "efficient" | "quality";
  prompt: string;
  riskLevel: "low" | "medium" | "high" | "critical";
  taskClass: string;
};

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

function statusTone(value: boolean, activeIsWarn = false): "ok" | "warn" | "info" {
  if (value) return activeIsWarn ? "warn" : "ok";
  return activeIsWarn ? "ok" : "info";
}

function GatePill({
  activeIsWarn = false,
  label,
  value,
}: {
  activeIsWarn?: boolean;
  label: string;
  value: boolean;
}) {
  return (
    <span className={`pill ${statusTone(value, activeIsWarn)}`}>
      {label}: {value ? "on" : "off"}
    </span>
  );
}

function AdaptiveFusionGatePanel({
  status,
}: {
  status?: AdaptiveFusionOperatorStatus;
}) {
  if (!status) {
    return (
      <StateBanner title="Adaptive fusion gate status unavailable" tone="warn">
        <p>Refresh dashboard data from an engine that exposes adaptive fusion operator status.</p>
      </StateBanner>
    );
  }

  const ready = status.completion_api.ready_for_live_completion;
  const profile = status.trusted_local_profile;
  const profileState = profile.ready ? "ready" : profile.requested ? "blocked" : "off";
  const taskAdvancement = status.trusted_local_task_advancement;
  const taskAdvancementState = taskAdvancement.ready
    ? "ready"
    : taskAdvancement.requested
      ? "blocked"
      : "off";
  return (
    <div className="subcard stack">
      <div className="flex-between">
        <div>
          <h3>Gate Status</h3>
          <p className="muted">
            Read-only view of completion, experiment, promotion, default routing, kill, and rollback controls.
          </p>
        </div>
        <span className={`pill ${ready ? "ok" : "info"}`}>
          completion {ready ? "ready" : "gated"}
        </span>
      </div>

      <StateBanner
        title={`Trusted local profile ${profileState}`}
        tone={profile.ready ? "ok" : profile.requested ? "warn" : "info"}
      >
        <p>
          {profile.ready
            ? "Bounded provider, adaptive routing, experiments, and promotion gates are active through the trusted-local profile."
            : profile.requested
              ? `Fail closed: ${profile.blockers.join(", ")}`
              : "Set ACP_TRUSTED_LOCAL_PROFILE=1 after auth, endpoint pricing, credentials, and cost caps are configured."}
        </p>
      </StateBanner>

      <StateBanner
        title={`Trusted task advancement ${taskAdvancementState}`}
        tone={taskAdvancement.ready ? "ok" : taskAdvancement.requested ? "warn" : "info"}
      >
        <p>
          {taskAdvancement.ready
            ? `${taskAdvancement.worker_count} bounded worker(s) pinned to ${taskAdvancement.executor_type}; max concurrency ${taskAdvancement.max_concurrent}.`
            : taskAdvancement.requested
              ? `Fail closed: ${taskAdvancement.blockers.join(", ")}`
              : "Set ACP_TRUSTED_LOCAL_TASK_ADVANCEMENT=1 to authorize bounded workers for explicit adaptive plans."}
        </p>
      </StateBanner>

      <div className="boundary-badges" aria-label="Adaptive fusion gate states">
        <GatePill label="provider" value={status.gates.provider_execution} />
        <GatePill label="adaptive" value={status.gates.adaptive_execution} />
        <GatePill label="auth" value={status.gates.auth} />
        <GatePill activeIsWarn label="fusion kill" value={status.gates.fusion_kill_switch} />
        <GatePill label="experiments enabled" value={status.gates.experiments_enabled} />
        <GatePill label="experiments active" value={status.gates.experiments_active} />
        <GatePill activeIsWarn label="experiments paused" value={status.gates.experiments_paused} />
        <GatePill activeIsWarn label="experiments kill" value={status.gates.experiments_kill_switch} />
        <GatePill label="auto promotion enabled" value={status.gates.auto_promotion_enabled} />
        <GatePill label="auto promotion active" value={status.gates.auto_promotion_active} />
        <GatePill activeIsWarn label="promotion kill" value={status.gates.auto_promotion_kill_switch} />
        <GatePill activeIsWarn label="default routing" value={status.completion_api.default_routing_enabled} />
      </div>

      <div className="status-strip" aria-label="Adaptive fusion operator counts">
        <Metric
          label="Completion API"
          value={status.completion_api.available ? "available" : "off"}
          detail={status.completion_api.ready_for_live_completion ? "live ready" : "needs gates"}
          tone={status.completion_api.ready_for_live_completion ? "warn" : "ok"}
        />
        <Metric
          label="Executor"
          value={status.completion_api.executor_configured ? "configured" : "missing"}
          detail="provider executor"
          tone={status.completion_api.executor_configured ? "ok" : "info"}
        />
        <Metric
          label="Registry"
          value={status.completion_api.registry_configured ? "configured" : "missing"}
          detail="model endpoints"
          tone={status.completion_api.registry_configured ? "ok" : "info"}
        />
        <Metric
          label="Experiments"
          value={status.gates.experiments_active ? "active" : "inactive"}
          detail={status.gates.experiments_kill_switch ? "kill switch on" : "dual gated"}
          tone={status.gates.experiments_active ? "warn" : "ok"}
        />
        <Metric
          label="Promotion"
          value={status.gates.auto_promotion_active ? "active" : "inactive"}
          detail={status.gates.auto_promotion_kill_switch ? "kill switch on" : "rollbackable"}
          tone={status.gates.auto_promotion_active ? "warn" : "ok"}
        />
      </div>
    </div>
  );
}

function RoutingMetadata({
  response,
}: {
  response: AdaptiveCompletionResponse;
}) {
  const metadata = response.routing_metadata;
  if (!metadata) {
    return (
      <StateBanner title="Routing metadata hidden" tone="info">
        <p>Metadata is omitted by default. Enable routing metadata before running a test to inspect candidate and policy IDs.</p>
      </StateBanner>
    );
  }
  return (
    <div className="command-block">
      <code>{JSON.stringify(metadata, null, 2)}</code>
    </div>
  );
}

function AdaptiveCompletionTester() {
  const [form, setForm] = useState<CompletionForm>({
    includeRoutingMetadata: false,
    objective: "efficient",
    prompt: "",
    riskLevel: "low",
    taskClass: "general",
  });
  const [busy, setBusy] = useState(false);
  const [response, setResponse] = useState<AdaptiveCompletionResponse | null>(null);
  const [message, setMessage] = useState("");

  async function submitCompletion(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const prompt = form.prompt.trim();
    if (!prompt) {
      setMessage("Prompt is required.");
      return;
    }
    setBusy(true);
    setMessage("");
    setResponse(null);
    const request: AdaptiveCompletionRequest = {
      prompt,
      objective: form.objective,
      risk_level: form.riskLevel,
      task_class: form.taskClass.trim() || "general",
      include_routing_metadata: form.includeRoutingMetadata,
    };
    try {
      const result = await createAdaptiveCompletion(request);
      setResponse(result);
      setMessage("Completion request finished.");
    } catch (e) {
      setMessage(e instanceof Error ? e.message : "Adaptive completion request failed.");
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="subcard stack">
      <div>
        <h3>Completion Test</h3>
        <p className="muted">
          Executes the existing guarded completion API. Provider calls still require auth, budget, token, timeout, concurrency, audit, redaction, and kill gates.
        </p>
      </div>
      <form className="form-stack" onSubmit={submitCompletion}>
        <label className="muted" htmlFor="adaptive-completion-prompt">
          Prompt
        </label>
        <textarea
          id="adaptive-completion-prompt"
          onChange={(event) => setForm((value) => ({ ...value, prompt: event.target.value }))}
          placeholder="Run a bounded adaptive completion test..."
          rows={5}
          value={form.prompt}
        />

        <div className="split">
          <label className="stack">
            <span className="muted">Objective</span>
            <select
              onChange={(event) => setForm((value) => ({
                ...value,
                objective: event.target.value as CompletionForm["objective"],
              }))}
              value={form.objective}
            >
              <option value="efficient">efficient</option>
              <option value="quality">quality</option>
            </select>
          </label>
          <label className="stack">
            <span className="muted">Risk</span>
            <select
              onChange={(event) => setForm((value) => ({
                ...value,
                riskLevel: event.target.value as CompletionForm["riskLevel"],
              }))}
              value={form.riskLevel}
            >
              <option value="low">low</option>
              <option value="medium">medium</option>
              <option value="high">high</option>
              <option value="critical">critical</option>
            </select>
          </label>
        </div>

        <label className="stack">
          <span className="muted">Task class</span>
          <input
            onChange={(event) => setForm((value) => ({ ...value, taskClass: event.target.value }))}
            value={form.taskClass}
          />
        </label>

        <label>
          <input
            checked={form.includeRoutingMetadata}
            onChange={(event) => setForm((value) => ({
              ...value,
              includeRoutingMetadata: event.target.checked,
            }))}
            type="checkbox"
          />
          <span>Show routing metadata in this response</span>
        </label>

        <button disabled={busy} type="submit">
          {busy ? "Running..." : "Run Completion"}
        </button>
      </form>

      {message && (
        <StateBanner title="Completion test result" tone={response ? "ok" : "info"}>
          <p>{message}</p>
        </StateBanner>
      )}

      {response && (
        <div className="stack">
          <div className="status-strip" aria-label="Completion usage">
            <Metric label="Input Tokens" value={String(response.usage.input_tokens)} />
            <Metric label="Output Tokens" value={String(response.usage.output_tokens)} />
            <Metric label="Cost" value={`$${response.usage.estimated_cost_usd.toFixed(4)}`} />
            <Metric label="Latency" value={`${response.usage.latency_ms}ms`} />
            <Metric
              label="Metadata"
              value={response.routing_metadata ? "shown" : "hidden"}
              detail="default hidden"
              tone={response.routing_metadata ? "warn" : "ok"}
            />
          </div>
          <div className="command-block" aria-label="Completion output">
            <code>{response.output ?? ""}</code>
          </div>
          <RoutingMetadata response={response} />
        </div>
      )}
    </div>
  );
}

export function AdaptiveFusion({
  operatorStatus,
}: {
  operatorStatus?: AdaptiveFusionOperatorStatus;
}) {
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

          <AdaptiveFusionGatePanel status={operatorStatus} />

          <AdaptiveCompletionTester />

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
