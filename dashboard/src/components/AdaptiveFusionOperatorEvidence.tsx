"use client";

import { useCallback, useEffect, useState } from "react";
import { ApiError, controlScheduler, fetchAudit } from "@/lib/api-client";
import type { AdaptiveFusionOperatorStatus, LocalAuditEvent } from "@/lib/types";
import { ConfirmDialog, type ConfirmAction } from "./ConfirmDialog";
import { Metric } from "./Metric";
import { StateBanner } from "./StateBanner";

function formatUsd(value: number | null): string {
  return value == null ? "not set" : `$${value.toFixed(4)}`;
}

function AuthorityPill({ label, value }: { label: string; value: boolean }) {
  return (
    <span className={`pill ${value ? "ok" : "info"}`}>
      {label}: {value ? "on" : "off"}
    </span>
  );
}

export function AdaptiveFusionOperatorEvidence({
  status,
}: {
  status: AdaptiveFusionOperatorStatus;
}) {
  const [auditEvents, setAuditEvents] = useState<LocalAuditEvent[]>([]);
  const [auditMessage, setAuditMessage] = useState("");
  const [scheduler, setScheduler] = useState(status.scheduler);
  const [controlMessage, setControlMessage] = useState("");
  const [confirmAction, setConfirmAction] = useState<ConfirmAction>(null);
  const [mutating, setMutating] = useState(false);

  const loadAudit = useCallback(() => {
    setAuditMessage("");
    Promise.all([
      fetchAudit({ limit: 8, redact: true, search: "adaptive" }),
      fetchAudit({ limit: 8, redact: true, search: "scheduler.control" }),
    ])
      .then(([adaptive, schedulerAudit]) => {
        const unique = new Map<string, LocalAuditEvent>();
        [...adaptive.events, ...schedulerAudit.events].forEach((event) => {
          unique.set(String(event.audit_id), event);
        });
        setAuditEvents(
          [...unique.values()]
            .sort((left, right) => right.created_at.localeCompare(left.created_at))
            .slice(0, 8),
        );
      })
      .catch((error) => {
        setAuditEvents([]);
        setAuditMessage(
          error instanceof ApiError && error.status === 403
            ? "Recent evidence requires audit:read scope."
            : error instanceof Error
              ? error.message
              : "Recent operator evidence is unavailable.",
        );
      });
  }, []);

  useEffect(() => {
    setScheduler(status.scheduler);
  }, [status.scheduler]);

  useEffect(() => {
    loadAudit();
  }, [loadAudit]);

  async function confirmSchedulerControl() {
    if (!confirmAction || confirmAction.type !== "schedulerControl") return;
    const action = confirmAction.action;
    setConfirmAction(null);
    setMutating(true);
    setControlMessage("");
    try {
      const response = await controlScheduler(action);
      const next = response.scheduler;
      setScheduler((current) => ({
        ...current,
        running: next.running,
        supervised_workers_enabled:
          next.supervised_workers_enabled ?? current.supervised_workers_enabled,
        paused: next.paused ?? false,
        kill_requested: next.kill_requested ?? false,
        worker_count: next.worker_count ?? current.worker_count,
        max_concurrent: next.config?.max_concurrent ?? current.max_concurrent,
        executor_type: next.config?.executor_type ?? current.executor_type,
        active_runs: next.active_runs ?? current.active_runs,
        tick_count: next.tick_count ?? current.tick_count,
        error_count: next.error_count ?? current.error_count,
        last_tick_at: next.last_tick_at ?? current.last_tick_at,
      }));
      setControlMessage(`Scheduler ${action} completed.`);
      loadAudit();
    } catch (error) {
      setControlMessage(
        error instanceof Error ? error.message : `Scheduler ${action} failed.`,
      );
    } finally {
      setMutating(false);
    }
  }

  const authority = status.authority;
  const bounds = status.bounds;
  const observations = status.observations;
  const policyBlockers = [
    ...bounds.experiment_policy_blockers.map((blocker) => `experiment: ${blocker}`),
    ...bounds.auto_promotion_policy_blockers.map((blocker) => `promotion: ${blocker}`),
  ];
  return (
    <>
      <div className="subcard stack">
        <div className="flex-between">
          <div>
            <h3>Authority & Bounds</h3>
            <p className="muted">
              Effective local authority and configured ceilings. Values are read-only and contain no credentials or model content.
            </p>
          </div>
          <span className={`pill ${authority.task_advancement_active ? "warn" : "ok"}`}>
            workers {authority.task_advancement_active ? "active" : "inactive"}
          </span>
        </div>
        <div className="boundary-badges" aria-label="Effective adaptive authority">
          <AuthorityPill label="provider active" value={authority.provider_execution_active} />
          <AuthorityPill label="adaptive active" value={authority.adaptive_execution_active} />
          <AuthorityPill label="default routing active" value={authority.default_routing_active} />
          <AuthorityPill label="experiments active" value={authority.experiments_active} />
          <AuthorityPill label="promotion active" value={authority.auto_promotion_active} />
          <AuthorityPill label="task advancement active" value={authority.task_advancement_active} />
        </div>
        {policyBlockers.length > 0 ? (
          <StateBanner
            tone="warn"
            title="Adaptive policy configuration is invalid"
          >
            {policyBlockers.join(", ")}
          </StateBanner>
        ) : null}
        <div className="status-strip" aria-label="Adaptive authority bounds">
          <Metric
            label="Today Cost"
            value={formatUsd(bounds.today_cost_usd)}
            detail={`daily cap ${formatUsd(bounds.daily_cost_cap_usd)}`}
            tone={bounds.daily_cost_remaining_usd === 0 ? "warn" : "info"}
          />
          <Metric
            label="Cost Remaining"
            value={formatUsd(bounds.daily_cost_remaining_usd)}
            detail={`per request ${formatUsd(bounds.per_dispatch_cost_cap_usd)}`}
          />
          <Metric
            label="Experiment Traffic"
            value={`${(bounds.experiment_traffic_rate * 100).toFixed(1)}%`}
            detail={
              bounds.experiment_policy_valid
                ? `${bounds.experiment_max_calls} calls / ${bounds.experiment_max_concurrency} concurrent`
                : "invalid policy"
            }
            tone={bounds.experiment_policy_valid ? "info" : "warn"}
          />
          <Metric
            label="Experiment Tokens"
            value={String(bounds.experiment_max_total_tokens)}
            detail={`${bounds.experiment_max_elapsed_ms}ms / ${formatUsd(bounds.experiment_max_cost_usd)}`}
          />
          <Metric
            label="Promotion Rollout"
            value={`${bounds.auto_promotion_rollout_percentage}%`}
            detail={bounds.auto_promotion_policy_valid ? "snapshot + rollback" : "invalid policy"}
            tone={bounds.auto_promotion_policy_valid ? "info" : "warn"}
          />
          <Metric
            label="Worker Bound"
            value={`${bounds.worker_count}/${bounds.worker_max_concurrent}`}
            detail={status.trusted_local_task_advancement.executor_type}
          />
        </div>
        <div className="status-strip" aria-label="Safe adaptive observation summary">
          <Metric label="Observations" value={String(observations.count)} detail="safe summaries" />
          <Metric label="Successful" value={String(observations.success_count)} />
          <Metric
            label="Failed"
            value={String(observations.failure_count)}
            tone={observations.failure_count > 0 ? "warn" : "ok"}
          />
          <Metric
            label="Observed Cost"
            value={formatUsd(observations.total_cost_usd)}
            detail={observations.latest_at ?? "no observations"}
          />
        </div>
      </div>

      <div className="subcard stack">
        <div className="flex-between">
          <div>
            <h3>Worker Control & Evidence</h3>
            <p className="muted">
              Reuses authenticated scheduler controls and redacted audit evidence. Kill remains non-reversible without a process restart.
            </p>
          </div>
          <button onClick={loadAudit} type="button">Refresh Evidence</button>
        </div>
        <div className="status-strip" aria-label="Adaptive scheduler status">
          <Metric
            label="Scheduler"
            value={scheduler.running ? "running" : scheduler.enabled ? "stopped" : "off"}
            detail={scheduler.executor_type ?? "no executor"}
            tone={scheduler.running ? "ok" : "warn"}
          />
          <Metric
            label="Paused"
            value={scheduler.paused ? "yes" : "no"}
            tone={scheduler.paused ? "warn" : "ok"}
          />
          <Metric
            label="Kill"
            value={scheduler.kill_requested ? "requested" : "clear"}
            tone={scheduler.kill_requested ? "warn" : "ok"}
          />
          <Metric
            label="Workers"
            value={`${scheduler.worker_count}/${scheduler.max_concurrent}`}
            detail={`${scheduler.active_runs} active runs`}
          />
          <Metric
            label="Ticks"
            value={String(scheduler.tick_count)}
            detail={`${scheduler.error_count} errors`}
            tone={scheduler.error_count > 0 ? "warn" : "info"}
          />
        </div>
        <div className="workflow-actions">
          <button
            disabled={mutating || !scheduler.running || scheduler.paused || scheduler.kill_requested}
            onClick={() => setConfirmAction({ type: "schedulerControl", action: "pause" })}
            type="button"
          >
            Pause
          </button>
          <button
            disabled={mutating || !scheduler.paused || scheduler.kill_requested}
            onClick={() => setConfirmAction({ type: "schedulerControl", action: "resume" })}
            type="button"
          >
            Resume
          </button>
          <button
            className="risk-action"
            disabled={mutating || !scheduler.running || scheduler.kill_requested}
            onClick={() => setConfirmAction({ type: "schedulerControl", action: "kill" })}
            type="button"
          >
            Kill
          </button>
        </div>
        {controlMessage && (
          <StateBanner title="Scheduler control result" tone="info">
            <p>{controlMessage}</p>
          </StateBanner>
        )}
        {auditMessage ? (
          <StateBanner title="Recent operator evidence unavailable" tone="warn">
            <p>{auditMessage}</p>
          </StateBanner>
        ) : auditEvents.length === 0 ? (
          <p className="muted">No recent adaptive or scheduler control audit events.</p>
        ) : (
          <div className="mission-decision-list" aria-label="Recent adaptive audit evidence">
            {auditEvents.map((event) => (
              <div className="mission-decision" key={String(event.audit_id)}>
                <div className="flex-between">
                  <strong>{event.action}</strong>
                  <span className="pill info">{event.created_at}</span>
                </div>
                <div className="mission-node-meta">
                  <span>{event.resource}</span>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
      <ConfirmDialog
        action={confirmAction}
        onCancel={() => setConfirmAction(null)}
        onConfirm={confirmSchedulerControl}
      />
    </>
  );
}
