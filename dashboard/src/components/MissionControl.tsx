import { useCallback, useEffect, useMemo, useState } from "react";
import {
  ApiError,
  captureSupervisedPatch,
  controlScheduler,
  approveProductTask,
  compileAndScheduleProductTask,
  createProductTask,
  createSupervisedPatchWorkspace,
  createWorkflowPlan,
  createWorkflowRun,
  finalizeProductTask,
  outputProductTask,
  exportSupervisedPatchArtifact,
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
  recordWorkflowRunApproval,
  targetRepoOutput,
  tickWorkflowRun,
  verifySupervisedPatchWorkspace,
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
import { ConfirmDialog, type ConfirmAction } from "./ConfirmDialog";
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

type ProductTaskControlState = {
  task_id: string;
  status: string;
  version: number;
  run_id?: string;
};

type ProductOutputApprovalState = {
  approval_id: string;
  approved_by: string;
  artifact_id: string;
  verification_sha256: string;
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
  if (error instanceof ApiError && error.status >= 500) {
    return {
      message: "Engine API is unavailable. Start the local runtime or check the dashboard proxy.",
      type: "error",
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

function approvedForArtifact(
  artifact: SupervisedPatchArtifact | null,
  approvals: WorkflowRunApproval[],
): boolean {
  if (!artifact) return false;
  return approvals.some((approval) =>
    approval.decision === "approved"
    && approval.bound_patch_hash === artifact.patch_hash
    && JSON.stringify(approval.bound_changed_files ?? []) === JSON.stringify(artifact.changed_files),
  );
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

function OutputActionRail({
  approvals,
  artifacts,
  onCreatedRun,
  onRefresh,
  run,
  scheduler,
  workspaces,
}: {
  approvals: WorkflowRunApproval[];
  artifacts: SupervisedPatchArtifact[];
  onCreatedRun: (runId: string) => void;
  onRefresh: () => void;
  run: WorkflowRun | null;
  scheduler: SchedulerStatus | null;
  workspaces: SupervisedPatchWorkspace[];
}) {
  const [rawRequest, setRawRequest] = useState("Implement a small verified change");
  const [targetRepoPath, setTargetRepoPath] = useState("");
  const [targetId, setTargetId] = useState("local-target");
  const [sourceRevision, setSourceRevision] = useState("HEAD");
  const [executor, setExecutor] = useState("codex_cli");
  const [verificationCommand, setVerificationCommand] = useState("");
  const [outputMode, setOutputMode] = useState<"export_patch" | "push_branch">("export_patch");
  const [branchName, setBranchName] = useState("acp/generated-output");
  const [remote, setRemote] = useState("origin");
  const [commitMessage, setCommitMessage] = useState("Apply supervised patch");
  const [prTitle, setPrTitle] = useState("Supervised patch output");
  const [createPullRequest, setCreatePullRequest] = useState(true);
  const [mutating, setMutating] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [confirmAction, setConfirmAction] = useState<ConfirmAction>(null);
  const [productTask, setProductTask] = useState<ProductTaskControlState | null>(null);
  const [productApproval, setProductApproval] = useState<ProductOutputApprovalState | null>(null);

  const workspace = workspaces[0] ?? null;
  const artifact = artifacts[0] ?? null;
  const terminal = run ? ["completed", "failed", "cancelled"].includes(run.status) : false;
  const approved = approvedForArtifact(artifact, approvals);
  const canCreateWorkspace = Boolean(run && targetRepoPath.trim() && sourceRevision.trim());
  const canVerify = Boolean(workspace && verificationCommand.trim());
  const canCapture = workspace?.verification?.status === "evidence_recorded";
  const canApprove = Boolean(artifact && artifact.redaction_status === "redacted" && !approved);
  const canExport = Boolean(artifact && approved);
  const canTargetOutput = Boolean(artifact && approved);

  function runMutation<T>(operation: () => Promise<T>, success: (result: T) => string, after?: (result: T) => void) {
    setMutating(true);
    setError(null);
    setMessage(null);
    operation()
      .then((result) => {
        after?.(result);
        setMessage(success(result));
        onRefresh();
      })
      .catch((err) => setError(err instanceof Error ? err.message : "Operation failed"))
      .finally(() => setMutating(false));
  }

  function createPlanAndRun() {
    if (!rawRequest.trim()) {
      setError("Task prompt is required.");
      return;
    }
    runMutation(
      async () => {
        const planResult = await createWorkflowPlan({
          raw_request: rawRequest.trim(),
          request_source: "dashboard",
        });
        return createWorkflowRun(planResult.plan.plan_id);
      },
      (result) => `Created run ${short(result.run.run_id)} from a new plan.`,
      (result) => onCreatedRun(result.run.run_id),
    );
  }

  function submitProductGoldenPath() {
    if (!rawRequest.trim() || !targetRepoPath.trim() || !sourceRevision.trim()) {
      setError("Product golden path requires prompt, target repo path, and source revision.");
      return;
    }
    runMutation(
      async () => {
        const admit = await createProductTask({
          objective: rawRequest.trim(),
          target_id: targetId.trim() || "dashboard-target",
          target_repo_path: targetRepoPath.trim(),
          source_revision: sourceRevision.trim(),
          allowed_paths: ["README.md"],
          verification_commands: [{ command: "test -f README.md", timeout_ms: 5000 }],
          output_intent: "artifact_only",
          executor_policy: {
            allowed_executors: [executor === "noop" ? "command" : executor],
            prefer: executor === "noop" ? "command" : executor,
          },
          risk_class: "low",
          approval_required: true,
          confirm_execution: true,
          idempotency_key: `dashboard-${Date.now()}`,
          workspace_mode: "git_worktree",
        });
        const taskId = String(
          (admit as { task_id?: string; task?: { task_id?: string } }).task_id
            ?? (admit as { task?: { task_id?: string } }).task?.task_id
            ?? "",
        );
        if (!taskId) {
          throw new Error("product task intake returned no task_id");
        }
        const compiled = await compileAndScheduleProductTask(taskId);
        return compiled;
      },
      (result) => {
        const task = (result as { result?: { task?: { task_id?: string; status?: string; run_id?: string } } })
          .result?.task;
        return `Product golden path ${short(task?.task_id)} status=${task?.status ?? "unknown"} run=${short(task?.run_id)}.`;
      },
      (result) => {
        const task = (result as {
          result?: { task?: ProductTaskControlState };
        }).result?.task;
        if (task?.task_id && typeof task.version === "number") {
          setProductTask(task);
          setProductApproval(null);
        }
        const runId = task?.run_id;
        if (runId) onCreatedRun(runId);
      },
    );
  }

  function finalizeGoldenPathTask() {
    if (!productTask) return;
    runMutation(
      () => finalizeProductTask(productTask.task_id),
      (result) => {
        const task = (result as { result?: { task?: ProductTaskControlState } }).result?.task;
        return `Product task ${short(productTask.task_id)} status=${task?.status ?? "unknown"}; approval is still required.`;
      },
      (result) => {
        const task = (result as { result?: { task?: ProductTaskControlState } }).result?.task;
        if (task?.task_id && typeof task.version === "number") setProductTask(task);
      },
    );
  }

  function approveGoldenPathTask() {
    if (!productTask) return;
    runMutation(
      () => approveProductTask(productTask.task_id, productTask.version),
      (result) => {
        const approval = (result as { approval?: ProductOutputApprovalState }).approval;
        return `Approved ${short(approval?.artifact_id)} as ${short(approval?.approval_id)} by ${approval?.approved_by ?? "unknown"}.`;
      },
      (result) => {
        const approval = (result as { approval?: ProductOutputApprovalState }).approval;
        if (approval?.approval_id) setProductApproval(approval);
      },
    );
  }

  function confirmGoldenPathOutput() {
    if (!productTask || !productApproval) return;
    runMutation(
      () => outputProductTask(
        productTask.task_id,
        productTask.version,
        productApproval.approval_id,
        true,
      ),
      (result) => {
        const task = (result as { result?: { task?: ProductTaskControlState } }).result?.task;
        return `Output result for ${short(productTask.task_id)}: ${task?.status ?? "unknown"}.`;
      },
      (result) => {
        const task = (result as { result?: { task?: ProductTaskControlState } }).result?.task;
        if (task?.task_id && typeof task.version === "number") setProductTask(task);
      },
    );
  }

  function handleConfirm() {
    if (!confirmAction) return;
    const action = confirmAction;
    setConfirmAction(null);

    if (action.type === "tickRun") {
      runMutation(
        () => tickWorkflowRun(action.runId, { executor }),
        (result) => `Tick completed with status ${String(result.tick.status ?? "unknown")}.`,
      );
    } else if (action.type === "verifyWorkspace") {
      runMutation(
        () => verifySupervisedPatchWorkspace(action.workspaceId, {
          command: action.command,
          confirm_verification: true,
          timeout_ms: 600_000,
          repair_executor: action.repairExecutor,
          max_repair_attempts: action.repairExecutor ? 2 : undefined,
        }),
        (result) => result.verification.status === "evidence_recorded"
          ? `Verification passed after ${result.verification.verification_attempts.length} attempt(s).`
          : `Verification failed after ${result.verification.verification_attempts.length} attempt(s).`,
      );
    } else if (action.type === "capturePatch") {
      runMutation(
        () => captureSupervisedPatch(action.workspaceId),
        (result) => `Captured artifact ${short(result.artifact.artifact_id)}.`,
      );
    } else if (action.type === "approveArtifact" || action.type === "rejectArtifact") {
      const current = artifacts.find((item) => item.artifact_id === action.artifactId);
      if (!current) {
        setError("Artifact is no longer available.");
        return;
      }
      runMutation(
        () => recordWorkflowRunApproval(action.runId, {
          node_id: "dashboard-output-approval",
          decision: action.type === "approveArtifact" ? "approved" : "rejected",
          reason: action.type === "approveArtifact" ? "dashboard approval" : "dashboard rejection",
          bound_patch_hash: current.patch_hash,
          bound_source_revision: current.source_revision,
          bound_changed_files: current.changed_files,
          expires_at: "2099-12-31T23:59:59Z",
        }),
        (result) => `Recorded ${result.approval.decision} approval.`,
      );
    } else if (action.type === "exportArtifact") {
      runMutation(
        () => exportSupervisedPatchArtifact(action.artifactId, action.runId),
        () => "Exported approved patch artifact.",
      );
    } else if (action.type === "targetOutput") {
      if (!run || !artifact) return;
      runMutation(
        () => targetRepoOutput(artifact.artifact_id, {
          run_id: run.run_id,
          mode: outputMode,
          confirm_target_output: true,
          branch_name: outputMode === "push_branch" ? branchName : undefined,
          remote: outputMode === "push_branch" ? remote : undefined,
          commit_message: outputMode === "push_branch" ? commitMessage : undefined,
          pr_title: outputMode === "push_branch" ? prTitle : undefined,
          create_pull_request: outputMode === "push_branch" ? createPullRequest : undefined,
        }),
        (result) => outputMode === "push_branch"
          ? result.output.pull_request
            ? `Opened PR #${result.output.pull_request.number}: ${result.output.pull_request.url}`
            : `Pushed ${String(result.output.branch_name ?? "branch")} at ${short(result.output.commit_sha, 10)}.`
          : `Generated patch output for ${short(result.output.patch_hash, 16)}.`,
      );
    } else if (action.type === "schedulerControl") {
      runMutation(
        () => controlScheduler(action.action),
        (result) => `Scheduler ${action.action} accepted; running=${String(result.scheduler.running)}.`,
      );
    }
  }

  return (
    <div className="subcard stack action-rail">
      <div className="flex-between">
        <div>
          <h3>Task Workflow</h3>
          <p className="muted" style={{ fontSize: "13px", marginTop: 4 }}>
            Task, run, workspace, patch, approval, and output controls in one path.
          </p>
        </div>
        <span className={`pill ${run ? statusPill(run.status) : "info"}`}>{run?.status ?? "no run"}</span>
      </div>

      {error && <StateBanner title="Action failed" tone="risk"><p>{error}</p></StateBanner>}
      {message && <StateBanner title="Action completed" tone="ok"><p>{message}</p></StateBanner>}

      <div className="action-grid">
        <label className="stack" style={{ gap: 4 }}>
          <span className="muted">Task prompt</span>
          <textarea
            rows={3}
            value={rawRequest}
            onChange={(event) => setRawRequest(event.target.value)}
            placeholder="Describe the output task"
          />
        </label>
        <div className="action-column">
          <label className="stack" style={{ gap: 4 }}>
            <span className="muted">Target repo path</span>
            <input value={targetRepoPath} onChange={(event) => setTargetRepoPath(event.target.value)} placeholder="/path/to/repo" />
          </label>
          <div className="split-row">
            <label className="stack" style={{ gap: 4 }}>
              <span className="muted">Target ID</span>
              <input value={targetId} onChange={(event) => setTargetId(event.target.value)} />
            </label>
            <label className="stack" style={{ gap: 4 }}>
              <span className="muted">Source ref</span>
              <input value={sourceRevision} onChange={(event) => setSourceRevision(event.target.value)} />
            </label>
          </div>
        </div>
      </div>

      <div className="workflow-actions">
        <button type="button" onClick={createPlanAndRun} disabled={mutating || !rawRequest.trim()}>
          {mutating ? "Working..." : "Create plan + run"}
        </button>
        <button
          type="button"
          onClick={submitProductGoldenPath}
          disabled={mutating || !rawRequest.trim() || !targetRepoPath.trim() || !sourceRevision.trim()}
          title="Requires ACP_PRODUCT_GOLDEN_PATH=1 and ACP_ENABLE_TARGET_REPO_OUTPUT=1"
        >
          {mutating ? "Working..." : "Product golden path"}
        </button>
        <button
          type="button"
          onClick={finalizeGoldenPathTask}
          disabled={mutating || !productTask || !["graph_ready", "running", "verifying"].includes(productTask.status)}
        >
          Finalize verification
        </button>
        <button
          type="button"
          onClick={approveGoldenPathTask}
          disabled={mutating || productTask?.status !== "awaiting_approval" || Boolean(productApproval)}
        >
          Approve exact evidence
        </button>
        <button
          type="button"
          onClick={confirmGoldenPathOutput}
          disabled={mutating || !productTask || !productApproval || !["awaiting_approval", "output_pending", "outcome_unknown"].includes(productTask.status)}
        >
          Confirm output
        </button>
        {productTask && (
          <span className={`pill ${statusPill(productTask.status)}`}>
            task {short(productTask.task_id)} v{productTask.version} {productTask.status}
          </span>
        )}
        {productApproval && (
          <span className="muted" title={`verification ${productApproval.verification_sha256}`}>
            approved by {productApproval.approved_by}: {short(productApproval.artifact_id)} / {short(productApproval.approval_id)}
          </span>
        )}
        <label className="inline-control">
          <span className="muted">Executor</span>
          <select value={executor} onChange={(event) => setExecutor(event.target.value)}>
            <option value="noop">noop</option>
            <option value="command">command</option>
            <option value="claude_code_cli">claude_code_cli</option>
            <option value="codex_cli">codex_cli</option>
          </select>
        </label>
        <button
          type="button"
          onClick={() => run && setConfirmAction({ type: "tickRun", runId: run.run_id })}
          disabled={mutating || !run || terminal}
        >
          Tick run
        </button>
        <button
          type="button"
          onClick={() => {
            if (!run) return;
            runMutation(
              () => createSupervisedPatchWorkspace({
                run_id: run.run_id,
                plan_id: run.plan_id ?? undefined,
                target_id: targetId.trim(),
                target_repo_path: targetRepoPath.trim(),
                source_revision: sourceRevision.trim(),
                workspace_mode: "git_worktree",
              }),
              (result) => `Created workspace ${short(result.workspace.workspace_id)}.`,
            );
          }}
          disabled={mutating || !canCreateWorkspace}
        >
          Create workspace
        </button>
        <label className="inline-control">
          <span className="muted">Verify</span>
          <input
            aria-label="Verification command"
            value={verificationCommand}
            onChange={(event) => setVerificationCommand(event.target.value)}
            placeholder="cargo test / npm test"
          />
        </label>
        <button
          type="button"
          onClick={() => {
            if (!workspace) return;
            const repairExecutor = executor === "codex_cli" || executor === "claude_code_cli"
              ? executor
              : undefined;
            setConfirmAction({
              type: "verifyWorkspace",
              workspaceId: workspace.workspace_id,
              command: verificationCommand.trim(),
              repairExecutor,
            });
          }}
          disabled={mutating || !canVerify}
        >
          Verify workspace
        </button>
        <button
          type="button"
          onClick={() => workspace && setConfirmAction({ type: "capturePatch", workspaceId: workspace.workspace_id })}
          disabled={mutating || !canCapture}
        >
          Capture patch
        </button>
        {workspace && (
          <span className={`pill ${canCapture ? "ok" : workspace.verification ? "risk" : "warn"}`}>
            {workspace.verification?.status ?? "verification required"}
          </span>
        )}
        <button
          type="button"
          onClick={() => artifact && run && setConfirmAction({ type: "approveArtifact", artifactId: artifact.artifact_id, runId: run.run_id })}
          disabled={mutating || !canApprove}
        >
          Approve artifact
        </button>
        <button
          type="button"
          onClick={() => artifact && run && setConfirmAction({ type: "exportArtifact", artifactId: artifact.artifact_id, runId: run.run_id })}
          disabled={mutating || !canExport}
        >
          Export patch
        </button>
      </div>

      <div className="target-output-row">
        <label className="inline-control">
          <span className="muted">Output</span>
          <select value={outputMode} onChange={(event) => setOutputMode(event.target.value as "export_patch" | "push_branch")}>
            <option value="export_patch">patch</option>
            <option value="push_branch">acp branch</option>
          </select>
        </label>
        {outputMode === "push_branch" && (
          <>
            <input aria-label="Branch name" value={branchName} onChange={(event) => setBranchName(event.target.value)} />
            <input aria-label="Remote" value={remote} onChange={(event) => setRemote(event.target.value)} />
            <input aria-label="Commit message" value={commitMessage} onChange={(event) => setCommitMessage(event.target.value)} />
            <input aria-label="PR title" value={prTitle} onChange={(event) => setPrTitle(event.target.value)} />
            <label className="inline-control">
              <input
                type="checkbox"
                checked={createPullRequest}
                onChange={(event) => setCreatePullRequest(event.target.checked)}
              />
              <span>Create PR</span>
            </label>
          </>
        )}
        <button
          type="button"
          onClick={() => artifact && setConfirmAction({ type: "targetOutput", artifactId: artifact.artifact_id, mode: outputMode })}
          disabled={mutating || !canTargetOutput}
        >
          {outputMode === "push_branch" ? "Push branch" : "Output patch"}
        </button>
      </div>

      <div className="runtime-control-row">
        <span className="muted">
          Scheduler: {scheduler?.enabled ? `${scheduler.running ? "running" : "stopped"} / ${scheduler.worker_count ?? 0} workers` : "disabled"}
        </span>
        <button type="button" onClick={() => setConfirmAction({ type: "schedulerControl", action: "pause" })} disabled={mutating || !scheduler?.enabled}>
          Pause
        </button>
        <button type="button" onClick={() => setConfirmAction({ type: "schedulerControl", action: "resume" })} disabled={mutating || !scheduler?.enabled}>
          Resume
        </button>
        <button type="button" className="risk-action" onClick={() => setConfirmAction({ type: "schedulerControl", action: "kill" })} disabled={mutating || !scheduler?.enabled}>
          Kill
        </button>
      </div>

      <ConfirmDialog action={confirmAction} onConfirm={handleConfirm} onCancel={() => setConfirmAction(null)} />
    </div>
  );
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
      detail: run && !terminal ? "Use the task workflow controls below to advance the next ready node." : "Tick is available only while the selected run is active.",
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
  const [detailReloadKey, setDetailReloadKey] = useState(0);

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
  }, [selectedRunId, detailReloadKey]);

  const refreshMissionState = useCallback(() => {
    loadOverview();
    setDetailReloadKey((key) => key + 1);
  }, [loadOverview]);

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
          <h2>Tasks</h2>
          <p className="muted" style={{ fontSize: "13px", marginTop: 4 }}>
            Create a task, run it, inspect failures, approve the result, and publish the output.
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
        <div className="loading-row"><span className="spinner" /> Loading task state...</div>
      ) : runs.length === 0 ? (
        <>
          <EmptyState
            title="No workflow runs"
            description="Create a plan and run from the output workflow below."
            tone="info"
          />
          <OutputActionRail
            approvals={[]}
            artifacts={[]}
            onCreatedRun={(runId) => {
              setSelectedRunId(runId);
              setDetailReloadKey((key) => key + 1);
            }}
            onRefresh={refreshMissionState}
            run={null}
            scheduler={scheduler}
            workspaces={[]}
          />
        </>
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
                  onRefresh={refreshMissionState}
                  run={run}
                />

                <OutputActionRail
                  approvals={approvals}
                  artifacts={runArtifacts}
                  onCreatedRun={(runId) => {
                    setSelectedRunId(runId);
                    setDetailReloadKey((key) => key + 1);
                  }}
                  onRefresh={refreshMissionState}
                  run={run}
                  scheduler={scheduler}
                  workspaces={runWorkspaces}
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
                          <span className="mono muted">
                            {approval.tool_name
                              ? `${approval.tool_name}/${short(approval.action_sha256, 12)}`
                              : short(approval.bound_patch_hash, 16)}
                          </span>
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
