import { useCallback, useEffect, useMemo, useState } from "react";
import {
  ApiError,
  fetchDecisions,
  fetchExecutorPool,
  fetchQueueRuns,
  fetchQueueStatus,
  fetchSchedulerStatus,
  fetchSupervisedPatchArtifacts,
  fetchSupervisedPatchWorkspaces,
  fetchWorkflowRunApprovals,
  fetchWorkflowRunDetail,
  fetchWorkflowRunEvents,
  fetchWorkflowRuns,
} from "@/lib/api-client";
import type {
  DecisionRecord,
  ExecutorPoolStatus,
  QueueRunSummary,
  QueueStatus,
  SchedulerStatus,
  SupervisedPatchArtifact,
  SupervisedPatchWorkspace,
  WorkflowRun,
  WorkflowRunApproval,
  WorkflowRunEvent,
  WorkflowRunNode,
} from "@/lib/types";
import { EmptyState } from "./EmptyState";
import { StateBanner } from "./StateBanner";

type MissionError = {
  message: string;
  type: "permission" | "error";
};

type TimelineItem =
  | { kind: "event"; at: string; label: string; tone: string; detail: string }
  | { kind: "node"; at: string; label: string; tone: string; detail: string };

type WorkflowStep = {
  detail: string;
  label: string;
  state: "done" | "now" | "blocked" | "todo";
};

function missionError(error: unknown): MissionError {
  if (error instanceof ApiError && (error.status === 401 || error.status === 403)) {
    return {
      message: error.status === 403
        ? "The current API key lacks the scope required for mission-control state."
        : "Mission-control state requires protected local API access.",
      type: "permission",
    };
  }
  return {
    message: error instanceof Error ? error.message : "Failed to load mission-control state",
    type: "error",
  };
}

function statusPill(status: string): string {
  if (["available", "completed", "approved", "redacted", "running"].includes(status)) return "ok";
  if (["failed", "cancelled", "rejected", "unavailable", "quarantined"].includes(status)) return "risk";
  if (["cooldown", "pending", "pending_approval", "requested", "paused", "retry_scheduled"].includes(status)) return "warn";
  return "info";
}

function eventTone(eventType: string): string {
  if (eventType.includes("failed") || eventType.includes("quarantine")) return "risk";
  if (eventType.includes("retry") || eventType.includes("approval") || eventType.includes("mutation")) return "warn";
  if (eventType.includes("completed") || eventType.includes("export")) return "ok";
  return "info";
}

function short(value: string | null | undefined, length = 12): string {
  if (!value) return "-";
  return value.length > length ? value.slice(0, length) : value;
}

function readableTime(value: string | null | undefined): string {
  if (!value) return "-";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleString([], {
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    month: "short",
    second: "2-digit",
  });
}

function detailText(details: Record<string, unknown> | null): string {
  if (!details || Object.keys(details).length === 0) return "-";
  const reason = details.reason ?? details.action ?? details.status ?? details.error ?? details.mutation_type;
  if (typeof reason === "string") return reason;
  return JSON.stringify(details);
}

function pickRun(runs: WorkflowRun[], selectedRunId: string | null): string | null {
  if (selectedRunId && runs.some((run) => run.run_id === selectedRunId)) return selectedRunId;
  const active = runs.find((run) => !["completed", "failed", "cancelled"].includes(run.status));
  return active?.run_id ?? runs[0]?.run_id ?? null;
}

function nodeById(run: WorkflowRun | null): Map<string, WorkflowRunNode> {
  return new Map((run?.nodes ?? []).map((node) => [node.node_id, node]));
}

function approvalCounts(approvals: WorkflowRunApproval[]) {
  return {
    approved: approvals.filter((approval) => approval.decision === "approved").length,
    requested: approvals.filter((approval) => approval.decision === "requested").length,
  };
}

function exportReadyCount(artifacts: SupervisedPatchArtifact[], approvals: WorkflowRunApproval[]): number {
  return artifacts.filter((artifact) =>
    artifact.redaction_status === "redacted"
    && approvals.some((approval) =>
      approval.decision === "approved"
      && approval.bound_patch_hash === artifact.patch_hash,
    ),
  ).length;
}

function failureReason(nodes: WorkflowRunNode[]): string {
  const failed = nodes.find((node) => node.status === "failed" || node.error_domain || node.error_message);
  if (!failed) return "No failing node recorded.";
  return failed.error_message ?? failed.error_domain ?? `${failed.task_type} failed`;
}

function nextWorkflowStep({
  approvals,
  artifacts,
  mutationEvents,
  run,
}: {
  approvals: WorkflowRunApproval[];
  artifacts: SupervisedPatchArtifact[];
  mutationEvents: WorkflowRunEvent[];
  run: WorkflowRun | null;
}): string {
  if (!run) return "Create or select a workflow run.";
  const failedNodes = run.nodes.filter((node) => node.status === "failed");
  const pendingApproval = approvalCounts(approvals).requested;
  const readyExports = exportReadyCount(artifacts, approvals);
  if (!["completed", "failed", "cancelled"].includes(run.status)) return "Tick the selected run.";
  if (failedNodes.length > 0 && mutationEvents.length === 0) return "Inspect the failure reason and run a retry/fix path.";
  if (pendingApproval > 0) return "Review and approve or reject the pending artifact.";
  if (readyExports > 0) return "Export the approved redacted artifact.";
  return "Inspect status, approvals, and export readiness.";
}

function PrimaryWorkflowPath({
  approvals,
  artifacts,
  mutationEvents,
  onRefresh,
  run,
}: {
  approvals: WorkflowRunApproval[];
  artifacts: SupervisedPatchArtifact[];
  mutationEvents: WorkflowRunEvent[];
  onRefresh: () => void;
  run: WorkflowRun | null;
}) {
  const failedNodes = run?.nodes.filter((node) => node.status === "failed") ?? [];
  const terminal = run ? ["completed", "failed", "cancelled"].includes(run.status) : false;
  const counts = approvalCounts(approvals);
  const readyExports = exportReadyCount(artifacts, approvals);
  const nextStep = nextWorkflowStep({ approvals, artifacts, mutationEvents, run });
  const steps: WorkflowStep[] = [
    {
      detail: run ? `Selected ${short(run.run_id)} (${run.status}).` : "Create a plan through the API or select an existing run below.",
      label: "Create/select run",
      state: run ? "done" : "now",
    },
    {
      detail: run && !terminal ? "Use the Runs tab tick control to advance the next ready node." : "Tick is available only while the selected run is active.",
      label: "Tick",
      state: run && !terminal ? "now" : run ? "done" : "todo",
    },
    {
      detail: run ? `${run.status}; ${failureReason(run.nodes)}` : "Run status appears after a run is selected.",
      label: "Inspect failure/status",
      state: failedNodes.length > 0 ? "blocked" : run ? "done" : "todo",
    },
    {
      detail: mutationEvents.length > 0 ? `${mutationEvents.length} retry/fix or mutation event${mutationEvents.length === 1 ? "" : "s"} recorded.` : "If a node fails, inspect failure details then tick/resume to follow the existing recovery path.",
      label: "Retry/fix path",
      state: failedNodes.length > 0 && mutationEvents.length === 0 ? "now" : mutationEvents.length > 0 ? "done" : "todo",
    },
    {
      detail: counts.requested > 0 ? `${counts.requested} approval request${counts.requested === 1 ? "" : "s"} pending.` : `${counts.approved} approval${counts.approved === 1 ? "" : "s"} recorded.`,
      label: "Approve",
      state: counts.requested > 0 ? "now" : counts.approved > 0 ? "done" : "todo",
    },
    {
      detail: readyExports > 0 ? `${readyExports} artifact${readyExports === 1 ? "" : "s"} ready for approval-bound export.` : "Export requires a redacted artifact bound to an approval.",
      label: "Export",
      state: readyExports > 0 ? "now" : "todo",
    },
  ];

  return (
    <div className="subcard stack">
      <div className="flex-between">
        <div>
          <h3>Primary Workflow</h3>
          <p className="muted" style={{ fontSize: "13px", marginTop: 4 }}>
            Create/select run -&gt; tick -&gt; inspect failure/status -&gt; retry/fix path -&gt; approve -&gt; export.
          </p>
        </div>
        <button onClick={onRefresh} type="button">Refresh path</button>
      </div>
      <StateBanner title="Next step" tone={nextStep.includes("Export") ? "ok" : nextStep.includes("Inspect") ? "warn" : "info"}>
        <p>{nextStep}</p>
      </StateBanner>
      <ol className="setup-list">
        {steps.map((step) => (
          <li className={`setup-step setup-step-${step.state === "done" ? "done" : step.state === "blocked" ? "warn" : step.state === "now" ? "warn" : "todo"}`} key={step.label}>
            <span aria-hidden="true" className="setup-dot" />
            <div>
              <strong>{step.label}</strong>
              <p>{step.detail}</p>
            </div>
          </li>
        ))}
      </ol>
    </div>
  );
}

function GraphView({ run }: { run: WorkflowRun }) {
  const incoming = new Map<string, string[]>();
  const outgoing = new Map<string, string[]>();
  for (const edge of run.edges) {
    incoming.set(edge.to_node_id, [...(incoming.get(edge.to_node_id) ?? []), edge.from_node_id]);
    outgoing.set(edge.from_node_id, [...(outgoing.get(edge.from_node_id) ?? []), edge.to_node_id]);
  }

  return (
    <div className="mission-graph">
      {run.nodes.map((node) => (
        <div className="mission-node" key={node.node_id}>
          <div className="flex-between">
            <span className="mono">{short(node.node_id)}</span>
            <span className={`pill ${statusPill(node.status)}`}>{node.status}</span>
          </div>
          <strong>{node.task_type}</strong>
          <div className="mission-node-meta">
            <span>{incoming.get(node.node_id)?.length ?? 0} in</span>
            <span>{outgoing.get(node.node_id)?.length ?? 0} out</span>
            <span>{node.executor_type ?? "no executor"}</span>
          </div>
        </div>
      ))}
      {run.edges.length > 0 && (
        <div className="mission-edge-list">
          {run.edges.map((edge) => (
            <span key={edge.edge_id} className="mono">
              {short(edge.from_node_id, 8)} {"->"} {short(edge.to_node_id, 8)}
            </span>
          ))}
        </div>
      )}
    </div>
  );
}

function OperatorSummary({
  artifacts,
  approvals,
  pool,
  queue,
  run,
  scheduler,
}: {
  artifacts: SupervisedPatchArtifact[];
  approvals: WorkflowRunApproval[];
  pool: ExecutorPoolStatus | null;
  queue: QueueStatus | null;
  run: WorkflowRun | null;
  scheduler: SchedulerStatus | null;
}) {
  const failedNodes = run?.nodes.filter((node) => node.status === "failed").length ?? 0;
  const pendingApprovals = approvalCounts(approvals).requested;
  const exportReady = exportReadyCount(artifacts, approvals);
  const utilization = pool && pool.total_capacity > 0 ? pool.total_active / pool.total_capacity : 0;

  return (
    <div className="detail-summary">
      <div className="summary-tile">
        <span className="metric-label">Selected run</span>
        <strong><span className={`pill ${statusPill(run?.status ?? "none")}`}>{run?.status ?? "none"}</span></strong>
      </div>
      <div className="summary-tile">
        <span className="metric-label">Scheduler</span>
        <strong><span className={`pill ${scheduler?.running ? "ok" : "warn"}`}>{scheduler?.running ? "running" : "stopped"}</span></strong>
      </div>
      <div className="summary-tile">
        <span className="metric-label">Pool utilization</span>
        <strong className={utilization >= 0.9 ? "error-text" : ""}>{(utilization * 100).toFixed(0)}%</strong>
      </div>
      <div className="summary-tile">
        <span className="metric-label">Backpressure</span>
        <strong><span className={`pill ${queue?.backpressure_active ? "risk" : "ok"}`}>{queue?.backpressure_active ? "active" : "off"}</span></strong>
      </div>
      <div className="summary-tile">
        <span className="metric-label">Approvals pending</span>
        <strong className={pendingApprovals > 0 ? "error-text" : ""}>{pendingApprovals}</strong>
      </div>
      <div className="summary-tile">
        <span className="metric-label">Export ready</span>
        <strong>{exportReady}</strong>
      </div>
      <div className="summary-tile">
        <span className="metric-label">Failure path</span>
        <strong className={failedNodes > 0 ? "error-text" : ""}>{failedNodes}</strong>
      </div>
    </div>
  );
}

export function MissionControl() {
  const [runs, setRuns] = useState<WorkflowRun[]>([]);
  const [selectedRunId, setSelectedRunId] = useState<string | null>(null);
  const [run, setRun] = useState<WorkflowRun | null>(null);
  const [events, setEvents] = useState<WorkflowRunEvent[]>([]);
  const [approvals, setApprovals] = useState<WorkflowRunApproval[]>([]);
  const [decisions, setDecisions] = useState<DecisionRecord[]>([]);
  const [scheduler, setScheduler] = useState<SchedulerStatus | null>(null);
  const [pool, setPool] = useState<ExecutorPoolStatus | null>(null);
  const [queue, setQueue] = useState<QueueStatus | null>(null);
  const [queueRuns, setQueueRuns] = useState<QueueRunSummary[]>([]);
  const [artifacts, setArtifacts] = useState<SupervisedPatchArtifact[]>([]);
  const [workspaces, setWorkspaces] = useState<SupervisedPatchWorkspace[]>([]);
  const [error, setError] = useState<MissionError | null>(null);
  const [loading, setLoading] = useState(true);
  const [detailLoading, setDetailLoading] = useState(false);

  const loadOverview = useCallback(() => {
    setLoading(true);
    setError(null);
    Promise.allSettled([
      fetchWorkflowRuns({ limit: 50 }),
      fetchSchedulerStatus(),
      fetchExecutorPool(),
      fetchQueueStatus(),
      fetchQueueRuns({ limit: 50 }),
      fetchSupervisedPatchArtifacts({ limit: 100 }),
      fetchSupervisedPatchWorkspaces({ limit: 100 }),
    ]).then(([runsResult, schedulerResult, poolResult, queueResult, queueRunsResult, artifactsResult, workspacesResult]) => {
      const nextRuns = runsResult.status === "fulfilled" ? runsResult.value.runs : [];
      setRuns(nextRuns);
      setScheduler(schedulerResult.status === "fulfilled" ? schedulerResult.value.scheduler : null);
      setPool(poolResult.status === "fulfilled" ? poolResult.value.pool : null);
      setQueue(queueResult.status === "fulfilled" ? queueResult.value.queue : null);
      setQueueRuns(queueRunsResult.status === "fulfilled" ? queueRunsResult.value.runs : []);
      setArtifacts(artifactsResult.status === "fulfilled" ? artifactsResult.value.artifacts : []);
      setWorkspaces(workspacesResult.status === "fulfilled" ? workspacesResult.value.workspaces : []);

      const firstError = [
        runsResult,
        schedulerResult,
        poolResult,
        queueResult,
        queueRunsResult,
        artifactsResult,
        workspacesResult,
      ].find((result) => result.status === "rejected");
      if (firstError?.status === "rejected") setError(missionError(firstError.reason));

      setSelectedRunId((current) => pickRun(nextRuns, current));
    }).finally(() => setLoading(false));
  }, []);

  useEffect(() => {
    loadOverview();
  }, [loadOverview]);

  useEffect(() => {
    if (!selectedRunId) {
      setRun(null);
      setEvents([]);
      setApprovals([]);
      setDecisions([]);
      return;
    }
    setDetailLoading(true);
    setError(null);
    Promise.allSettled([
      fetchWorkflowRunDetail(selectedRunId),
      fetchWorkflowRunEvents(selectedRunId, { limit: 200 }),
      fetchWorkflowRunApprovals(selectedRunId, { limit: 200 }),
      fetchDecisions({ run_id: selectedRunId, limit: 200 }),
    ]).then(([runResult, eventsResult, approvalsResult, decisionsResult]) => {
      if (runResult.status === "fulfilled") setRun(runResult.value.run);
      if (eventsResult.status === "fulfilled") setEvents(eventsResult.value.events);
      if (approvalsResult.status === "fulfilled") setApprovals(approvalsResult.value.approvals);
      if (decisionsResult.status === "fulfilled") setDecisions(decisionsResult.value.decisions);
      const firstError = [runResult, eventsResult, approvalsResult, decisionsResult].find((result) => result.status === "rejected");
      if (firstError?.status === "rejected") setError(missionError(firstError.reason));
    }).finally(() => setDetailLoading(false));
  }, [selectedRunId]);

  const selectedQueueRun = queueRuns.find((item) => item.run_id === selectedRunId) ?? null;
  const runArtifacts = artifacts.filter((artifact) => artifact.run_id === selectedRunId);
  const runWorkspaces = workspaces.filter((workspace) => workspace.run_id === selectedRunId);
  const nodesById = nodeById(run);

  const timeline = useMemo<TimelineItem[]>(() => {
    const nodeItems: TimelineItem[] = (run?.nodes ?? []).map((node) => ({
      at: node.updated_at,
      detail: `${node.task_type} via ${node.executor_type ?? "unassigned"}; attempt ${node.attempt}`,
      kind: "node",
      label: `${short(node.node_id)} ${node.status}`,
      tone: statusPill(node.status),
    }));
    const eventItems: TimelineItem[] = events.map((event) => ({
      at: event.created_at,
      detail: detailText(event.details),
      kind: "event",
      label: event.event_type,
      tone: eventTone(event.event_type),
    }));
    return [...nodeItems, ...eventItems]
      .sort((a, b) => new Date(a.at).getTime() - new Date(b.at).getTime());
  }, [events, run?.nodes]);

  const mutationEvents = events.filter((event) =>
    event.event_type.startsWith("dag.mutation.")
    || event.event_type.includes("retry")
    || detailText(event.details).includes("recovered"),
  );
  const failureNodes = run?.nodes.filter((node) => node.status === "failed" || node.error_domain || node.attempt > 0) ?? [];
  const saturatedExecutors = pool?.entries.filter((entry) =>
    entry.status !== "available" || (entry.capacity > 0 && entry.active_count >= entry.capacity),
  ) ?? [];

  return (
    <section className="card stack">
      <div className="flex-between">
        <div>
          <h2>Mission Control</h2>
          <p className="muted" style={{ fontSize: "13px", marginTop: 4 }}>
            Workflow, queue, executor, decision, approval, and export state for the selected run.
          </p>
        </div>
        <button onClick={loadOverview} type="button">Refresh</button>
      </div>

      {error?.type === "permission" && (
        <StateBanner title="Permission required" tone="warn"><p>{error.message}</p></StateBanner>
      )}
      {error?.type === "error" && (
        <StateBanner title="Failed to load" tone="risk"><p>{error.message}</p></StateBanner>
      )}

      {loading ? (
        <div className="loading-row"><span className="spinner" /> Loading mission-control state...</div>
      ) : runs.length === 0 ? (
        <EmptyState
          title="No workflow runs"
          description="Create a plan to populate mission control state."
          tone="info"
        >
          <div className="command-block">
            <span className="label">Create a plan</span>
            <code>{`curl -X POST http://127.0.0.1:9999/api/v1/plans -H "content-type: application/json" -d '{"raw_request":"Implement feature","request_source":"manual"}'`}</code>
          </div>
        </EmptyState>
      ) : (
        <>
          <label className="mission-picker">
            <span className="muted">Workflow run</span>
            <select value={selectedRunId ?? ""} onChange={(event) => setSelectedRunId(event.target.value)}>
              {runs.map((item) => (
                <option key={item.run_id} value={item.run_id}>
                  {item.run_id} - {item.status} - {item.workflow_id}
                </option>
              ))}
            </select>
          </label>

          {detailLoading && (
            <div className="loading-row"><span className="spinner" /> Loading selected run detail...</div>
          )}

          <OperatorSummary
            approvals={approvals}
            artifacts={runArtifacts}
            pool={pool}
            queue={queue}
            run={run}
            scheduler={scheduler}
          />

          {run && (
            <div className="mission-layout">
              <div className="stack">
                <PrimaryWorkflowPath
                  approvals={approvals}
                  artifacts={runArtifacts}
                  mutationEvents={mutationEvents}
                  onRefresh={loadOverview}
                  run={run}
                />

                <div className="subcard stack">
                  <div className="flex-between">
                    <h3>Workflow Graph</h3>
                    <span className="muted">{run.nodes.length} nodes / {run.edges.length} edges</span>
                  </div>
                  <GraphView run={run} />
                </div>

                <div className="subcard stack">
                  <h3>Node Timeline</h3>
                  <div className="mission-timeline">
                    {timeline.map((item, index) => (
                      <div className="mission-timeline-item" key={`${item.kind}-${item.at}-${index}`}>
                        <span className={`pill ${item.tone}`}>{item.kind}</span>
                        <span className="mono muted">{readableTime(item.at)}</span>
                        <strong>{item.label}</strong>
                        <span className="muted">{item.detail}</span>
                      </div>
                    ))}
                  </div>
                </div>

                <div className="subcard stack">
                  <h3>Decision Trace</h3>
                  {decisions.length === 0 ? (
                    <p className="muted">No decisions recorded for this run.</p>
                  ) : (
                    <div className="mission-decision-list">
                      {decisions.map((decision) => (
                        <div className="mission-decision" key={decision.decision_id}>
                          <div className="flex-between">
                            <span className={`pill ${statusPill(decision.action)}`}>{decision.action}</span>
                            <span className="mono muted">{readableTime(decision.created_at)}</span>
                          </div>
                          <p>{decision.reason}</p>
                          <div className="mission-node-meta">
                            <span>{decision.executor ?? decision.selected_executor ?? "no executor"}</span>
                            <span>{(decision.confidence * 100).toFixed(0)}% confidence</span>
                            {decision.node_id && <span>node {short(decision.node_id, 8)}</span>}
                          </div>
                        </div>
                      ))}
                    </div>
                  )}
                </div>
              </div>

              <div className="stack">
                <div className="subcard stack">
                  <h3>Mutation / Recovery Reasons</h3>
                  {mutationEvents.length === 0 ? (
                    <p className="muted">No graph mutation or recovery events recorded.</p>
                  ) : (
                    <div className="stack" style={{ gap: 6 }}>
                      {mutationEvents.map((event) => (
                        <div className="mission-signal" key={event.event_id}>
                          <span className={`pill ${eventTone(event.event_type)}`}>{event.event_type}</span>
                          <span>{detailText(event.details)}</span>
                        </div>
                      ))}
                    </div>
                  )}
                </div>

                <div className="subcard stack">
                  <h3>Executor Resources</h3>
                  {pool ? (
                    <>
                      <div className="kv-row"><span className="muted">Active / capacity</span><span>{pool.total_active}/{pool.total_capacity}</span></div>
                      <div className="kv-row"><span className="muted">Saturated entries</span><span>{saturatedExecutors.length}</span></div>
                      {saturatedExecutors.map((entry) => (
                        <div className="mission-signal" key={entry.executor_type}>
                          <span className={`pill ${statusPill(entry.status)}`}>{entry.executor_type}</span>
                          <span>{entry.active_count}/{entry.capacity} active, failure {entry.failure_score.toFixed(2)}</span>
                        </div>
                      ))}
                    </>
                  ) : (
                    <p className="muted">Executor pool status unavailable.</p>
                  )}
                </div>

                <div className="subcard stack">
                  <h3>Queue / Backpressure</h3>
                  {queue ? (
                    <>
                      <div className="kv-row"><span className="muted">Queued / running</span><span>{queue.total_queued}/{queue.total_running}</span></div>
                      <div className="kv-row"><span className="muted">Effective concurrency</span><span>{queue.effective_concurrency}</span></div>
                      <div className="kv-row"><span className="muted">Backpressure</span><span className={`pill ${queue.backpressure_active ? "risk" : "ok"}`}>{queue.backpressure_active ? "active" : "off"}</span></div>
                      {selectedQueueRun && (
                        <>
                          <div className="kv-row"><span className="muted">Selected run priority</span><span>{selectedQueueRun.priority}</span></div>
                          <div className="kv-row"><span className="muted">Pause reason</span><span>{selectedQueueRun.pause_reason ?? "-"}</span></div>
                          <div className="kv-row"><span className="muted">Degrade mode</span><span>{selectedQueueRun.degrade_mode ?? "-"}</span></div>
                        </>
                      )}
                    </>
                  ) : (
                    <p className="muted">Queue status unavailable.</p>
                  )}
                </div>

                <div className="subcard stack">
                  <h3>Approval Inbox</h3>
                  {approvals.length === 0 ? (
                    <p className="muted">No approvals recorded for this run.</p>
                  ) : (
                    <div className="stack" style={{ gap: 6 }}>
                      {approvals.map((approval) => (
                        <div className="mission-signal" key={approval.approval_id}>
                          <span className={`pill ${statusPill(approval.decision)}`}>{approval.decision}</span>
                          <span>{nodesById.get(approval.node_id)?.task_type ?? short(approval.node_id)}</span>
                          <span className="mono muted">{short(approval.bound_patch_hash, 16)}</span>
                        </div>
                      ))}
                    </div>
                  )}
                </div>

                <div className="subcard stack">
                  <h3>Export State</h3>
                  {runArtifacts.length === 0 && runWorkspaces.length === 0 ? (
                    <p className="muted">No workspace or artifact records linked to this run.</p>
                  ) : (
                    <>
                      {runWorkspaces.map((workspace) => (
                        <div className="mission-signal" key={workspace.workspace_id}>
                          <span className={`pill ${statusPill(workspace.status)}`}>{workspace.status}</span>
                          <span>workspace {short(workspace.workspace_id)}</span>
                        </div>
                      ))}
                      {runArtifacts.map((artifact) => {
                        const approved = approvals.some((approval) =>
                          approval.decision === "approved"
                          && approval.bound_patch_hash === artifact.patch_hash,
                        );
                        return (
                          <div className="mission-signal" key={artifact.artifact_id}>
                            <span className={`pill ${approved && artifact.redaction_status === "redacted" ? "ok" : statusPill(artifact.redaction_status)}`}>
                              {approved && artifact.redaction_status === "redacted" ? "export-ready" : artifact.redaction_status}
                            </span>
                            <span>{artifact.changed_files.length} files</span>
                            <span className="mono muted">{short(artifact.patch_hash, 18)}</span>
                          </div>
                        );
                      })}
                    </>
                  )}
                </div>

                <div className="subcard stack">
                  <h3>Failure / Retry Path</h3>
                  {failureNodes.length === 0 ? (
                    <p className="muted">No failed or retried nodes in the selected run.</p>
                  ) : (
                    <div className="stack" style={{ gap: 6 }}>
                      {failureNodes.map((node) => (
                        <div className="mission-signal" key={node.node_id}>
                          <span className={`pill ${statusPill(node.status)}`}>{node.status}</span>
                          <span>{node.task_type}</span>
                          <span>{node.error_domain ?? `attempt ${node.attempt}`}</span>
                        </div>
                      ))}
                    </div>
                  )}
                </div>
              </div>
            </div>
          )}
        </>
      )}
    </section>
  );
}
