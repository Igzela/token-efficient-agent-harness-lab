import { useCallback, useEffect, useState } from "react";
import {
  ApiError,
  cancelWorkflowRun,
  fetchWorkflowRunApprovals,
  fetchWorkflowRunDetail,
  fetchWorkflowRunEvents,
  fetchWorkflowRuns,
  tickWorkflowRun,
} from "@/lib/api-client";
import type {
  WorkflowRun,
  WorkflowRunApproval,
  WorkflowRunEdge,
  WorkflowRunEvent,
  WorkflowRunNode,
} from "@/lib/types";
import { ConfirmDialog, type ConfirmAction } from "./ConfirmDialog";
import { EmptyState } from "./EmptyState";
import { SearchBar } from "./SearchBar";
import { StateBanner } from "./StateBanner";

function getRunNodes(run: WorkflowRun): WorkflowRunNode[] {
  const r = run as unknown as Record<string, unknown>;
  return (r.nodes as WorkflowRunNode[])
    ?? ((r.graph as Record<string, unknown>)?.nodes as WorkflowRunNode[])
    ?? [];
}

function getRunEdges(run: WorkflowRun): WorkflowRunEdge[] {
  const r = run as unknown as Record<string, unknown>;
  return (r.edges as WorkflowRunEdge[])
    ?? ((r.graph as Record<string, unknown>)?.edges as WorkflowRunEdge[])
    ?? [];
}

type RunError = {
  message: string;
  type: "permission" | "error";
};

function runError(error: unknown): RunError {
  if (error instanceof ApiError && (error.status === 401 || error.status === 403)) {
    return {
      message: error.status === 403
        ? "The current API key lacks dispatch:read scope for workflow runs."
        : "Workflow runs require protected local API access.",
      type: "permission",
    };
  }
  return {
    message: error instanceof Error ? error.message : "Failed to load workflow runs",
    type: "error",
  };
}

function statusPill(status: string): string {
  if (status === "completed" || status === "approved") return "ok";
  if (status === "failed" || status === "cancelled" || status === "rejected" || status === "quarantined") return "risk";
  if (status === "running" || status === "executing" || status === "pending_approval") return "warn";
  return "info";
}

function formatDuration(start: string, end?: string | null): string {
  const s = new Date(start).getTime();
  const e = end ? new Date(end).getTime() : Date.now();
  const ms = e - s;
  if (ms < 1000) return `${ms}ms`;
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)}s`;
  return `${(ms / 60_000).toFixed(1)}m`;
}

function summarizeRunPath(run: WorkflowRun): { failure: string; next: string; readiness: string } {
  const nodes = getRunNodes(run);
  const failed = nodes.find((node) => node.status === "failed" || node.error_domain || node.error_message);
  const active = !["completed", "failed", "cancelled"].includes(run.status);
  if (active) {
    return {
      failure: failed?.error_message ?? failed?.error_domain ?? "No blocking failure recorded.",
      next: "Tick this run to advance the next ready node.",
      readiness: "Approval/export readiness appears after artifacts and approvals are recorded.",
    };
  }
  if (failed) {
    return {
      failure: failed.error_message ?? failed.error_domain ?? `${failed.task_type} failed`,
      next: "Inspect the failed node, then use the existing tick/resume path after the fix node is available.",
      readiness: "Export remains blocked until a redacted artifact is bound to an approval.",
    };
  }
  return {
    failure: "No failed node recorded.",
    next: "Review approvals and supervised patch artifacts before export.",
    readiness: "Export requires approval binding and redacted artifact state.",
  };
}

function NodeRow({ node, onClick }: { node: WorkflowRunNode; onClick: () => void }) {
  return (
    <tr
      role="button"
      tabIndex={0}
      onClick={onClick}
      onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); onClick(); } }}
      className="clickable-row"
    >
      <td className="mono" style={{ fontSize: "0.8rem" }}>{node.node_id.slice(0, 12)}</td>
      <td>{node.task_type}</td>
      <td><span className={`pill ${statusPill(node.status)}`}>{node.status}</span></td>
      <td>{node.executor_type ?? "—"}</td>
      <td>{node.latency_ms != null ? `${node.latency_ms}ms` : "—"}</td>
      <td>{node.attempt}</td>
      <td>{node.error_domain ?? "—"}</td>
    </tr>
  );
}

function NodeDetail({ node, onBack }: { node: WorkflowRunNode; onBack: () => void }) {
  return (
    <div className="subcard stack">
      <div className="flex-between">
        <h4>Node {node.node_id.slice(0, 12)}</h4>
        <button onClick={onBack} type="button">Back</button>
      </div>
      <div className="kv-row"><span className="muted">ID</span><span className="mono" style={{ fontSize: "0.8rem" }}>{node.node_id}</span></div>
      <div className="kv-row"><span className="muted">Task type</span><span>{node.task_type}</span></div>
      <div className="kv-row"><span className="muted">Status</span><span className={`pill ${statusPill(node.status)}`}>{node.status}</span></div>
      <div className="kv-row"><span className="muted">Executor</span><span>{node.executor_type ?? "none"}</span></div>
      <div className="kv-row"><span className="muted">Latency</span><span>{node.latency_ms != null ? `${node.latency_ms}ms` : "—"}</span></div>
      <div className="kv-row"><span className="muted">Attempt</span><span>{node.attempt}</span></div>
      <div className="kv-row"><span className="muted">Cost incurred</span><span>{node.cost_incurred > 0 ? `$${node.cost_incurred.toFixed(4)}` : "—"}</span></div>
      {node.output_ref && <div className="kv-row"><span className="muted">Output ref</span><span className="mono" style={{ fontSize: "0.8rem" }}>{node.output_ref}</span></div>}
      {node.error_domain && <div className="kv-row"><span className="muted">Error domain</span><span className="error-text">{node.error_domain}</span></div>}
      {node.error_message && <div className="kv-row"><span className="muted">Error</span><span className="error-text">{node.error_message}</span></div>}
      {node.lease_expires_at && <div className="kv-row"><span className="muted">Lease expires</span><span>{node.lease_expires_at}</span></div>}
      <div className="kv-row"><span className="muted">Created</span><span>{node.created_at}</span></div>
      <div className="kv-row"><span className="muted">Updated</span><span>{node.updated_at}</span></div>
      {node.input_refs.length > 0 && (
        <div className="subcard stack">
          <h4>Input refs</h4>
          <ul className="compact-list">
            {node.input_refs.map((r) => <li key={r} className="mono" style={{ fontSize: "0.8rem" }}>{r}</li>)}
          </ul>
        </div>
      )}
    </div>
  );
}

function EventTimeline({ events }: { events: WorkflowRunEvent[] }) {
  if (events.length === 0) return <p className="muted">No events recorded.</p>;
  return (
    <div className="subcard stack">
      <h4>Event timeline ({events.length})</h4>
      <div className="stack" style={{ gap: "6px" }}>
        {events.map((ev) => (
          <div key={ev.event_id} className="flex-row" style={{ gap: "12px", fontSize: "13px" }}>
            <span className="mono muted" style={{ fontSize: "11px", flexShrink: 0, width: "80px" }}>
              {new Date(ev.created_at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" })}
            </span>
            <span className={`pill ${statusPill(ev.event_type)}`} style={{ flexShrink: 0 }}>{ev.event_type}</span>
            {ev.node_id && <span className="mono muted" style={{ fontSize: "11px" }}>node:{ev.node_id.slice(0, 8)}</span>}
            <span className="muted" style={{ fontSize: "12px" }}>by {ev.actor}</span>
            {ev.details && Object.keys(ev.details).length > 0 && (
              <details>
                <summary>details</summary>
                <pre>{JSON.stringify(ev.details, null, 2)}</pre>
              </details>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}

function ApprovalList({ approvals }: { approvals: WorkflowRunApproval[] }) {
  if (approvals.length === 0) return <p className="muted">No approvals recorded.</p>;
  return (
    <div className="subcard stack">
      <h4>Approvals ({approvals.length})</h4>
      <table className="table">
        <thead>
          <tr>
            <th>Node</th>
            <th>Decision</th>
            <th>Decided by</th>
            <th>Reason</th>
            <th>Patch hash</th>
            <th>Created</th>
          </tr>
        </thead>
        <tbody>
          {approvals.map((ap) => (
            <tr key={ap.approval_id}>
              <td className="mono" style={{ fontSize: "0.8rem" }}>{ap.node_id.slice(0, 12)}</td>
              <td><span className={`pill ${statusPill(ap.decision)}`}>{ap.decision}</span></td>
              <td>{ap.decided_by}</td>
              <td>{ap.reason ?? "—"}</td>
              <td className="mono" style={{ fontSize: "0.8rem" }}>{ap.bound_patch_hash?.slice(0, 12) ?? "—"}</td>
              <td>{ap.created_at}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function RunDetail({
  run,
  onBack,
  onMutated,
}: {
  run: WorkflowRun;
  onBack: () => void;
  onMutated: () => void;
}) {
  const [events, setEvents] = useState<WorkflowRunEvent[]>([]);
  const [approvals, setApprovals] = useState<WorkflowRunApproval[]>([]);
  const [selectedNode, setSelectedNode] = useState<WorkflowRunNode | null>(null);
  const [loadingExtra, setLoadingExtra] = useState(true);
  const [confirmAction, setConfirmAction] = useState<ConfirmAction>(null);
  const [mutating, setMutating] = useState(false);
  const [mutationError, setMutationError] = useState<string | null>(null);
  const [tickResult, setTickResult] = useState<Record<string, unknown> | null>(null);
  const [executor, setExecutor] = useState("noop");

  useEffect(() => {
    setLoadingExtra(true);
    Promise.allSettled([
      fetchWorkflowRunEvents(run.run_id, { limit: 100 }),
      fetchWorkflowRunApprovals(run.run_id, { limit: 100 }),
    ]).then(([evResult, apResult]) => {
      if (evResult.status === "fulfilled") setEvents(evResult.value.events);
      if (apResult.status === "fulfilled") setApprovals(apResult.value.approvals);
    }).finally(() => setLoadingExtra(false));
  }, [run.run_id]);

  const isTerminal = ["completed", "failed", "cancelled"].includes(run.status);

  function handleConfirm() {
    if (!confirmAction) return;
    const action = confirmAction;
    setConfirmAction(null);
    setMutating(true);
    setMutationError(null);
    setTickResult(null);

    if (action.type === "cancelRun") {
      cancelWorkflowRun(action.runId)
        .then(() => onMutated())
        .catch((err) => setMutationError(err instanceof Error ? err.message : "Cancel failed"))
        .finally(() => setMutating(false));
    } else if (action.type === "tickRun") {
      tickWorkflowRun(action.runId, { executor })
        .then((result) => {
          setTickResult(result.tick);
          onMutated();
        })
        .catch((err) => setMutationError(err instanceof Error ? err.message : "Tick failed"))
        .finally(() => setMutating(false));
    }
  }

  const completedNodes = getRunNodes(run).filter((n) => n.status === "completed").length;
  const failedNodes = getRunNodes(run).filter((n) => n.status === "failed").length;
  const totalCost = getRunNodes(run).reduce((sum, n) => sum + n.cost_incurred, 0);
  const totalLatency = getRunNodes(run).reduce((sum, n) => sum + (n.latency_ms ?? 0), 0);
  const pathSummary = summarizeRunPath(run);

  return (
    <div className="card stack">
      <div className="flex-between">
        <h3>Run {run.run_id.slice(0, 12)}</h3>
        <button onClick={onBack} type="button">Back to list</button>
      </div>

      {mutationError && (
        <StateBanner title="Operation failed" tone="risk"><p>{mutationError}</p></StateBanner>
      )}
      {tickResult && (
        <StateBanner title="Tick executed" tone="ok">
          <p>Status: {String(tickResult.status ?? "unknown")}{tickResult.executor_type ? ` (${tickResult.executor_type})` : ""}</p>
        </StateBanner>
      )}

      <div className="detail-summary">
        <div className="summary-tile">
          <span className="metric-label">Status</span>
          <strong><span className={`pill ${statusPill(run.status)}`}>{run.status}</span></strong>
        </div>
        <div className="summary-tile">
          <span className="metric-label">Nodes</span>
          <strong>{completedNodes}/{getRunNodes(run).length}</strong>
        </div>
        <div className="summary-tile">
          <span className="metric-label">Cost</span>
          <strong>{totalCost > 0 ? `$${totalCost.toFixed(4)}` : "—"}</strong>
        </div>
        <div className="summary-tile">
          <span className="metric-label">Latency</span>
          <strong>{totalLatency > 0 ? `${totalLatency}ms` : "—"}</strong>
        </div>
      </div>

      <div className="subcard stack">
        <h4>Primary Workflow Step</h4>
        <div className="kv-row"><span className="muted">Next step</span><span>{pathSummary.next}</span></div>
        <div className="kv-row"><span className="muted">Failure reason</span><span>{pathSummary.failure}</span></div>
        <div className="kv-row"><span className="muted">Approval/export readiness</span><span>{pathSummary.readiness}</span></div>
      </div>

      <div className="subcard stack">
        <h4>Details</h4>
        <div className="kv-row"><span className="muted">Run ID</span><span className="mono" style={{ fontSize: "0.8rem" }}>{run.run_id}</span></div>
        <div className="kv-row"><span className="muted">Workflow ID</span><span className="mono" style={{ fontSize: "0.8rem" }}>{run.workflow_id}</span></div>
        {run.plan_id && <div className="kv-row"><span className="muted">Plan ID</span><span className="mono" style={{ fontSize: "0.8rem" }}>{run.plan_id}</span></div>}
        <div className="kv-row"><span className="muted">Initiated by</span><span>{run.initiated_by}</span></div>
        <div className="kv-row"><span className="muted">Created</span><span>{run.created_at}</span></div>
        <div className="kv-row"><span className="muted">Updated</span><span>{run.updated_at}</span></div>
        <div className="kv-row"><span className="muted">Duration</span><span>{formatDuration(run.created_at, isTerminal ? run.updated_at : null)}</span></div>
        {failedNodes > 0 && <div className="kv-row"><span className="muted">Failed nodes</span><span className="error-text">{failedNodes}</span></div>}
      </div>

      {Object.keys(run.boundaries).length > 0 && (
        <div className="subcard stack">
          <h4>Boundaries</h4>
          {Object.entries(run.boundaries).map(([k, v]) => (
            <div className="kv-row" key={k}>
              <span className="muted">{k}</span>
              <span>{typeof v === "string" ? v : JSON.stringify(v)}</span>
            </div>
          ))}
        </div>
      )}

      <div className="subcard stack">
        <h4>Nodes ({getRunNodes(run).length})</h4>
        {selectedNode ? (
          <NodeDetail node={selectedNode} onBack={() => setSelectedNode(null)} />
        ) : (
          <table className="table">
            <thead>
              <tr>
                <th>ID</th>
                <th>Task type</th>
                <th>Status</th>
                <th>Executor</th>
                <th>Latency</th>
                <th>Attempt</th>
                <th>Error</th>
              </tr>
            </thead>
            <tbody>
              {getRunNodes(run).map((node) => (
                <NodeRow key={node.node_id} node={node} onClick={() => setSelectedNode(node)} />
              ))}
            </tbody>
          </table>
        )}
      </div>

      {loadingExtra ? (
        <div className="loading-row"><span className="spinner" /> Loading events and approvals...</div>
      ) : (
        <>
          <EventTimeline events={events} />
          <ApprovalList approvals={approvals} />
        </>
      )}

      <div className="flex-end" style={{ gap: "0.5rem" }}>
        {!isTerminal && (
          <>
            <label className="flex-row" style={{ gap: "4px", fontSize: "13px" }}>
              <span className="muted">Executor:</span>
              <select value={executor} onChange={(e) => setExecutor(e.target.value)}>
                <option value="noop">noop</option>
                <option value="command">command</option>
                <option value="claude_code_cli">claude_code_cli</option>
                <option value="codex_cli">codex_cli</option>
              </select>
            </label>
            <button
              type="button"
              onClick={() => setConfirmAction({ type: "tickRun", runId: run.run_id })}
              disabled={mutating}
            >
              {mutating ? "Working..." : "Tick"}
            </button>
            <button
              type="button"
              className="risk-action"
              onClick={() => setConfirmAction({ type: "cancelRun", runId: run.run_id })}
              disabled={mutating}
            >
              {mutating ? "Working..." : "Cancel"}
            </button>
          </>
        )}
      </div>
      <ConfirmDialog
        action={confirmAction}
        onConfirm={handleConfirm}
        onCancel={() => setConfirmAction(null)}
      />
    </div>
  );
}

export function WorkflowRuns() {
  const [runs, setRuns] = useState<WorkflowRun[]>([]);
  const [error, setError] = useState<RunError | null>(null);
  const [loading, setLoading] = useState(true);
  const [search, setSearch] = useState("");
  const [selectedRun, setSelectedRun] = useState<WorkflowRun | null>(null);
  const [detailLoading, setDetailLoading] = useState(false);

  const load = useCallback(() => {
    setLoading(true);
    setError(null);
    setSelectedRun(null);
    fetchWorkflowRuns({ limit: 50, search: search || undefined })
      .then((res) => setRuns(res.runs))
      .catch((e) => setError(runError(e)))
      .finally(() => setLoading(false));
  }, [search]);

  useEffect(() => {
    load();
  }, [load]);

  function openDetail(runId: string) {
    setDetailLoading(true);
    fetchWorkflowRunDetail(runId)
      .then((res) => setSelectedRun(res.run))
      .catch((e) => setError(runError(e)))
      .finally(() => setDetailLoading(false));
  }

  return (
    <section className="card stack">
      <div className="flex-between">
        <h2>Workflow Runs</h2>
        <button onClick={load} type="button">Refresh</button>
      </div>

      {error?.type === "permission" && (
        <StateBanner title="Permission required" tone="warn"><p>{error.message}</p></StateBanner>
      )}
      {error?.type === "error" && (
        <StateBanner title="Failed to load" tone="risk"><p>{error.message}</p></StateBanner>
      )}

      {!selectedRun && (
        <SearchBar
          search={search}
          onSearchChange={setSearch}
          resultCount={runs.length}
          label="run"
          placeholder="Search runs..."
        />
      )}

      {loading ? (
        <div className="loading-row"><span className="spinner" /> Loading workflow runs...</div>
      ) : detailLoading ? (
        <div className="loading-row"><span className="spinner" /> Loading run detail...</div>
      ) : selectedRun ? (
        <RunDetail
          run={selectedRun}
          onBack={() => setSelectedRun(null)}
          onMutated={() => openDetail(selectedRun.run_id)}
        />
      ) : runs.length === 0 && !error ? (
        <EmptyState
          title="No workflow runs"
          description="Create a plan via the API to start a workflow run."
          tone="info"
        >
          <div className="command-block">
            <span className="label">Create a plan</span>
            <code>{`curl -X POST http://127.0.0.1:9999/api/v1/plans -H "content-type: application/json" -d '{"raw_request":"Implement feature X","request_source":"manual"}'`}</code>
          </div>
        </EmptyState>
      ) : (
        <table className="table">
          <thead>
            <tr>
              <th>ID</th>
              <th>Status</th>
              <th>Workflow</th>
              <th>Nodes</th>
              <th>Initiated by</th>
              <th>Created</th>
              <th />
            </tr>
          </thead>
          <tbody>
            {runs.map((run) => {
              const completed = getRunNodes(run).filter((n) => n.status === "completed").length;
              return (
                <tr key={run.run_id}>
                  <td className="mono" style={{ fontSize: "0.8rem" }}>{run.run_id.slice(0, 12)}</td>
                  <td><span className={`pill ${statusPill(run.status)}`}>{run.status}</span></td>
                  <td className="mono" style={{ fontSize: "0.8rem" }}>{run.workflow_id.slice(0, 12)}</td>
                  <td>{completed}/{getRunNodes(run).length}</td>
                  <td>{run.initiated_by}</td>
                  <td>{run.created_at}</td>
                  <td>
                    <button onClick={() => openDetail(run.run_id)} type="button">View</button>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      )}
    </section>
  );
}
