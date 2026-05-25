const SAMPLE_REPORT = {
  blockers: [],
  checks: [
    {
      check_id: "required_files",
      evidence: [
        "AGENTS.md",
        "docs/harness/PROJECT_BOARD.md",
        "docs/harness/TASK_QUEUE.md",
        "docs/harness/QUALITY_GATES.md",
        "docs/harness/DECISION_RECORD.md",
        "docs/harness/RISK_REGISTER.md",
      ],
      message: "all required harness control files are present",
      status: "PASS",
    },
    {
      check_id: "optional_files",
      evidence: ["docs/harness/FINAL_GATE.md", "docs/harness/EVIDENCE_INDEX.md"],
      message: "optional recommended control files are present",
      status: "PASS",
    },
    {
      check_id: "agents_policy",
      evidence: [
        "agent is described as execution adapter",
        "agent is not governance authority",
        "human authority is referenced",
        "provider integration is guarded",
        "active state mutation is guarded",
      ],
      message: "AGENTS.md execution adapter policy reviewed",
      status: "PASS_WITH_NOTES",
    },
    {
      check_id: "project_board",
      evidence: [
        "task state vocabulary exists",
        "phase/closeout status appears documented",
        "approval/blocking statuses are visible",
      ],
      message: "project board sanity check complete",
      status: "PASS_WITH_NOTES",
    },
    {
      check_id: "task_queue",
      evidence: [
        "execution slices found: 100",
        "Status fields: 100; Goal fields: 99",
        "non-executable statuses are present",
        "approval-gated or blocked slices detected",
      ],
      message: "task queue sanity check complete",
      status: "PASS_WITH_NOTES",
    },
    {
      check_id: "quality_gates",
      evidence: [
        "unknown_error requires human review",
        "provider or LLM boundary present",
        "active state mutation requires approval",
        "auto modification is forbidden or reviewed",
        "read-only or evidence-only boundary present",
      ],
      message: "quality gate sanity check complete",
      status: "PASS",
    },
    {
      check_id: "risk_register",
      evidence: [
        "active risks exist",
        "mitigated risks exist",
        "provider/LLM premature integration risk exists",
        "scope drift risk exists",
        "mutation/active state risk exists",
      ],
      message: "risk register sanity check complete",
      status: "PASS",
    },
    {
      check_id: "closeout_reports",
      evidence: [
        "docs/harness/P7_CLOSEOUT_REPORT.md",
        "docs/harness/PHASE1_CLOSEOUT_REPORT.md",
        "docs/harness/PHASE2_CLOSEOUT_REPORT.md",
        "docs/harness/PHASE3_CLOSEOUT_REPORT.md; status=PASS_WITH_NOTES; tests=607; sealed_candidate=True",
        "docs/harness/PHASE4_CLOSEOUT_REPORT.md; status=PASS; tests=729; sealed_candidate=True",
        "docs/harness/PHASE5_CLOSEOUT_REPORT.md",
      ],
      message: "closeout reports detected",
      status: "PASS",
    },
  ],
  recommended_next_actions: [
    "Review warnings and convert high-friction manual controls into machine-readable indexes.",
    "Keep using the target repository as a controlled harness instance.",
    "Do not allow the execution adapter to approve its own work.",
    "Use human approval before active state mutation, provider integration, sandbox execution, or main-branch push.",
  ],
  target_repo: "../alters-lab",
  verdict: "PASS_WITH_NOTES",
  warnings: [
    "AGENTS.md does not explicitly mention main/master push restrictions",
    "PROJECT_BOARD.md has structurally suspicious table rows: line 32: P1-004 | Controlled Snapshot YAML Write | done |; line 33: P1-005 | Controlled Branches YAML Write | done |; line 34: P1-006 | Controlled Alter YAML Write | done |; line 36: P1-008 | Controlled Dialogue YAML Write | done |; line 37: P1-009 | Reality Trace / Weekly Evidence Controlled Write | done |",
  ],
};

const statusLabel = {
  PASS: "Pass",
  PASS_WITH_NOTES: "Notes",
  WARN: "Warn",
  FAIL: "Fail",
  BLOCKED: "Blocked",
  READY_FOR_REVIEW: "Ready for review",
  NEEDS_APPROVAL: "Needs approval",
};

const elements = {
  verdict: document.querySelector("#verdict"),
  targetRepo: document.querySelector("#target-repo"),
  checksCount: document.querySelector("#checks-count"),
  warningsCount: document.querySelector("#warnings-count"),
  blockersCount: document.querySelector("#blockers-count"),
  statusStrip: document.querySelector("#status-strip"),
  checksList: document.querySelector("#checks-list"),
  actionsList: document.querySelector("#actions-list"),
  warningsList: document.querySelector("#warnings-list"),
  blockersList: document.querySelector("#blockers-list"),
  reportFile: document.querySelector("#report-file"),
  resetReport: document.querySelector("#reset-report"),
  refreshRepos: document.querySelector("#refresh-repos"),
  runAudit: document.querySelector("#run-audit"),
  repoSelect: document.querySelector("#repo-select"),
  apiState: document.querySelector("#api-state"),
  repoForm: document.querySelector("#repo-form"),
  repoId: document.querySelector("#repo-id"),
  repoName: document.querySelector("#repo-name"),
  repoKind: document.querySelector("#repo-kind"),
  repoLocation: document.querySelector("#repo-location"),
  planForm: document.querySelector("#plan-form"),
  planTaskId: document.querySelector("#plan-task-id"),
  planObjective: document.querySelector("#plan-objective"),
  planTaskType: document.querySelector("#plan-task-type"),
  planRiskLevel: document.querySelector("#plan-risk-level"),
  planContextTokens: document.querySelector("#plan-context-tokens"),
  planExecutionTokens: document.querySelector("#plan-execution-tokens"),
  generatePlan: document.querySelector("#generate-plan"),
  planOutput: document.querySelector("#plan-output"),
  refreshPlans: document.querySelector("#refresh-plans"),
  plansRepoFilter: document.querySelector("#plans-repo-filter"),
  plansStatusFilter: document.querySelector("#plans-status-filter"),
  plansTotal: document.querySelector("#plans-total"),
  plansReady: document.querySelector("#plans-ready"),
  plansNeedsApproval: document.querySelector("#plans-needs-approval"),
  plansBlocked: document.querySelector("#plans-blocked"),
  plansAverageBudget: document.querySelector("#plans-average-budget"),
  planHistoryBody: document.querySelector("#plan-history-body"),
  comparePlanA: document.querySelector("#compare-plan-a"),
  comparePlanB: document.querySelector("#compare-plan-b"),
  comparePlans: document.querySelector("#compare-plans"),
  compareOutput: document.querySelector("#compare-output"),
  guidancePlanSelect: document.querySelector("#guidance-plan-select"),
  generateGuidance: document.querySelector("#generate-guidance"),
  guidanceOutput: document.querySelector("#guidance-output"),
};

let registeredRepos = [];
let registeredPlanSummaries = [];

function normalizeStatus(status) {
  return String(status || "WARN").toUpperCase();
}

function statusClass(status) {
  return `status-${normalizeStatus(status).toLowerCase().replaceAll("_", "-")}`;
}

function setText(node, value) {
  node.textContent = value == null || value === "" ? "-" : String(value);
}

function clear(node) {
  while (node.firstChild) node.removeChild(node.firstChild);
}

function makePill(status) {
  const pill = document.createElement("span");
  pill.className = `pill ${statusClass(status)}`;
  pill.textContent = statusLabel[normalizeStatus(status)] || normalizeStatus(status);
  return pill;
}

function appendEmpty(node, text) {
  const empty = document.createElement("div");
  empty.className = "empty";
  empty.textContent = text;
  node.appendChild(empty);
}

function appendListItems(node, items, emptyText) {
  clear(node);
  if (!items || items.length === 0) {
    appendEmpty(node, emptyText);
    return;
  }
  for (const item of items) {
    const li = document.createElement("li");
    li.textContent = item;
    node.appendChild(li);
  }
}

function renderStatusStrip(checks) {
  clear(elements.statusStrip);
  const counts = checks.reduce((acc, check) => {
    const status = normalizeStatus(check.status);
    acc[status] = (acc[status] || 0) + 1;
    return acc;
  }, {});
  for (const [status, count] of Object.entries(counts)) {
    const pill = makePill(status);
    pill.textContent = `${statusLabel[status] || status}: ${count}`;
    elements.statusStrip.appendChild(pill);
  }
}

function renderChecks(checks) {
  clear(elements.checksList);
  if (!checks || checks.length === 0) {
    appendEmpty(elements.checksList, "No checks reported.");
    return;
  }

  for (const check of checks) {
    const row = document.createElement("article");
    row.className = "check-row";

    const meta = document.createElement("div");
    meta.className = "check-meta";
    const id = document.createElement("div");
    id.className = "check-id";
    id.textContent = check.check_id || "unknown_check";
    meta.append(id, makePill(check.status));

    const body = document.createElement("div");
    const message = document.createElement("p");
    message.className = "check-message";
    message.textContent = check.message || "No message.";
    const evidence = document.createElement("ul");
    evidence.className = "evidence-list";
    for (const item of check.evidence || []) {
      const li = document.createElement("li");
      li.textContent = item;
      evidence.appendChild(li);
    }
    body.append(message, evidence);
    row.append(meta, body);
    elements.checksList.appendChild(row);
  }
}

function renderReport(report) {
  const checks = Array.isArray(report.checks) ? report.checks : [];
  const warnings = Array.isArray(report.warnings) ? report.warnings : [];
  const blockers = Array.isArray(report.blockers) ? report.blockers : [];
  const actions = Array.isArray(report.recommended_next_actions)
    ? report.recommended_next_actions
    : [];

  setText(elements.verdict, report.verdict);
  elements.verdict.className = statusClass(report.verdict);
  setText(elements.targetRepo, report.target_repo);
  setText(elements.checksCount, checks.length);
  setText(elements.warningsCount, warnings.length);
  setText(elements.blockersCount, blockers.length);

  renderStatusStrip(checks);
  renderChecks(checks);
  appendListItems(elements.warningsList, warnings, "None");
  appendListItems(elements.blockersList, blockers, "None");
  appendListItems(elements.actionsList, actions, "No actions reported.");
}

function setApiState(text) {
  elements.apiState.textContent = text;
}

function renderRepos(repos) {
  registeredRepos = Array.isArray(repos) ? repos : [];
  clear(elements.repoSelect);
  renderRepoFilterOptions();
  if (registeredRepos.length === 0) {
    const option = document.createElement("option");
    option.textContent = "No repos registered";
    option.value = "";
    elements.repoSelect.appendChild(option);
    elements.runAudit.disabled = true;
    elements.generatePlan.disabled = true;
    return;
  }

  for (const repo of registeredRepos) {
    const option = document.createElement("option");
    option.value = repo.id;
    const location = repo.kind === "local" ? repo.path : repo.url;
    option.textContent = `${repo.name} (${repo.kind}) - ${location}`;
    elements.repoSelect.appendChild(option);
  }
  renderRepoFilterOptions();
  elements.runAudit.disabled = false;
  elements.generatePlan.disabled = false;
}

function renderRepoFilterOptions() {
  clear(elements.plansRepoFilter);
  const all = document.createElement("option");
  all.value = "";
  all.textContent = "all repos";
  elements.plansRepoFilter.appendChild(all);
  for (const repo of registeredRepos) {
    const option = document.createElement("option");
    option.value = repo.id;
    option.textContent = repo.name;
    elements.plansRepoFilter.appendChild(option);
  }
}

async function fetchJson(url, options) {
  const response = await fetch(url, options);
  const data = await response.json();
  if (!response.ok) {
    const message = data?.error?.message || `Request failed with ${response.status}`;
    throw new Error(message);
  }
  return data;
}

async function refreshRepos() {
  try {
    setApiState("Connecting");
    const data = await fetchJson("/api/repos");
    renderRepos(data.repos);
    setApiState("API connected");
    await refreshPlanWorkbench();
  } catch (error) {
    renderRepos([]);
    renderPlanWorkbenchEmpty();
    setApiState("Static sample");
  }
}

async function runSelectedAudit() {
  const repoId = elements.repoSelect.value;
  if (!repoId) return;
  try {
    setApiState("Running audit");
    const data = await fetchJson(`/api/audit?repo_id=${encodeURIComponent(repoId)}`);
    renderReport(data.audit);
    setApiState("API connected");
  } catch (error) {
    renderReport({
      ...SAMPLE_REPORT,
      verdict: "BLOCKED",
      target_repo: repoId,
      blockers: [error.message],
      warnings: [],
      checks: [],
      recommended_next_actions: ["Review the repository registration and retry the read-only audit."],
    });
    setApiState("API error");
  }
}

async function registerRepo(event) {
  event.preventDefault();
  const kind = elements.repoKind.value;
  const location = elements.repoLocation.value.trim();
  const payload = {
    id: elements.repoId.value.trim(),
    name: elements.repoName.value.trim(),
    kind,
  };
  if (kind === "local") {
    payload.path = location;
  } else {
    payload.url = location;
  }

  try {
    setApiState("Registering repo");
    await fetchJson("/api/repos", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(payload),
    });
    elements.repoForm.reset();
    await refreshRepos();
  } catch (error) {
    setApiState(`Registry error: ${error.message}`);
  }
}

function readTokenBudget(node) {
  const value = Number.parseInt(node.value, 10);
  if (!Number.isInteger(value) || value < 0) {
    throw new Error("Token budgets must be non-negative integers.");
  }
  return value;
}

function planPayload() {
  const repoId = elements.repoSelect.value;
  if (!repoId) {
    throw new Error("Select a registered repo before planning.");
  }
  return {
    task_id: elements.planTaskId.value.trim() || "task",
    repo_id: repoId,
    objective: elements.planObjective.value.trim(),
    task_type: elements.planTaskType.value,
    risk_level: elements.planRiskLevel.value,
    constraints: ["MVP3 deterministic planning only", "executable=false"],
    max_context_tokens: readTokenBudget(elements.planContextTokens),
    max_execution_tokens: readTokenBudget(elements.planExecutionTokens),
  };
}

function renderPlanEmpty() {
  clear(elements.planOutput);
  appendEmpty(elements.planOutput, "No plan generated.");
}

function renderPlanError(message) {
  clear(elements.planOutput);
  const box = document.createElement("div");
  box.className = "plan-error";
  box.textContent = message;
  elements.planOutput.appendChild(box);
}

function appendPlanList(section, title, items, emptyText) {
  const wrapper = document.createElement("div");
  wrapper.className = "plan-subsection";
  const heading = document.createElement("h3");
  heading.textContent = title;
  const list = document.createElement("ul");
  list.className = "notice-list";
  wrapper.append(heading, list);
  section.appendChild(wrapper);
  appendListItems(list, items, emptyText);
}

function renderPlan(plan) {
  clear(elements.planOutput);
  if (!plan) {
    renderPlanEmpty();
    return;
  }

  const summary = document.createElement("div");
  summary.className = "plan-summary";
  summary.appendChild(makePill(plan.status));

  const fields = [
    ["Plan", plan.plan_id],
    ["Risk", plan.effective_risk],
    ["Executable", String(plan.executable)],
    ["Total budget", plan.total_token_budget],
    ["Context", plan.context_budget],
    ["Execution", plan.execution_budget],
  ];
  for (const [label, value] of fields) {
    const item = document.createElement("div");
    const dt = document.createElement("span");
    dt.className = "metric-label";
    dt.textContent = label;
    const dd = document.createElement("strong");
    dd.textContent = value == null || value === "" ? "-" : String(value);
    item.append(dt, dd);
    summary.appendChild(item);
  }
  elements.planOutput.appendChild(summary);

  const stepSection = document.createElement("div");
  stepSection.className = "plan-subsection";
  const stepHeading = document.createElement("h3");
  stepHeading.textContent = "Planned Steps";
  const stepList = document.createElement("ol");
  stepList.className = "step-list";
  stepSection.append(stepHeading, stepList);
  elements.planOutput.appendChild(stepSection);

  const steps = Array.isArray(plan.steps) ? plan.steps : [];
  if (steps.length === 0) {
    appendEmpty(stepList, "No executable steps. Plan is blocked or waiting for approval.");
  } else {
    for (const step of steps) {
      const item = document.createElement("li");
      const head = document.createElement("div");
      head.className = "step-head";
      const role = document.createElement("strong");
      role.textContent = `planned role: ${step.role}`;
      const budget = document.createElement("span");
      budget.textContent = `${step.token_budget} tokens`;
      head.append(role, budget);

      const action = document.createElement("p");
      action.textContent = step.action;
      const meta = document.createElement("p");
      meta.className = "step-meta";
      meta.textContent = `context=${step.context_mode}; approval_required=${step.approval_required}; ${step.reason}`;
      item.append(head, action, meta);
      stepList.appendChild(item);
    }
  }

  appendPlanList(elements.planOutput, "Approval Gates", plan.approval_gates, "None");
  appendPlanList(elements.planOutput, "Blockers", plan.blockers, "None");
  appendPlanList(elements.planOutput, "Token Efficiency Notes", plan.token_efficiency_notes, "None");
}

function planListUrl() {
  const params = new URLSearchParams();
  if (elements.plansRepoFilter.value) {
    params.set("repo_id", elements.plansRepoFilter.value);
  }
  if (elements.plansStatusFilter.value) {
    params.set("status", elements.plansStatusFilter.value);
  }
  const query = params.toString();
  return query ? `/api/plans?${query}` : "/api/plans";
}

function planSummaryUrl() {
  const params = new URLSearchParams();
  if (elements.plansRepoFilter.value) {
    params.set("repo_id", elements.plansRepoFilter.value);
  }
  const query = params.toString();
  return query ? `/api/plans/summary?${query}` : "/api/plans/summary";
}

function renderPlanWorkbenchEmpty() {
  registeredPlanSummaries = [];
  renderPlanSummary({
    total_plans: 0,
    by_status: { ready_for_review: 0, needs_approval: 0, blocked: 0 },
    average_token_budget: 0,
  });
  renderPlanHistory([]);
  renderCompareOptions([]);
  renderCompareEmpty("No plans available for comparison.");
  renderGuidanceOptions([]);
  renderGuidanceEmpty("No plans available for guidance.");
}

function renderPlanSummary(summary) {
  const byStatus = summary.by_status || {};
  setText(elements.plansTotal, summary.total_plans || 0);
  setText(elements.plansReady, byStatus.ready_for_review || 0);
  setText(elements.plansNeedsApproval, byStatus.needs_approval || 0);
  setText(elements.plansBlocked, byStatus.blocked || 0);
  setText(elements.plansAverageBudget, summary.average_token_budget || 0);
}

function renderPlanHistory(plans) {
  clear(elements.planHistoryBody);
  if (!plans || plans.length === 0) {
    const row = document.createElement("tr");
    const cell = document.createElement("td");
    cell.colSpan = 9;
    cell.className = "table-empty";
    cell.textContent = "No stored plans match the current filters.";
    row.appendChild(cell);
    elements.planHistoryBody.appendChild(row);
    return;
  }

  for (const plan of plans) {
    const row = document.createElement("tr");
    appendCell(row, plan.plan_id);
    appendCell(row, plan.repo_id);
    const statusCell = document.createElement("td");
    statusCell.appendChild(makePill(plan.status));
    row.appendChild(statusCell);
    appendCell(row, String(plan.executable));
    appendCell(row, plan.total_token_budget);
    appendCell(row, plan.approval_gate_count);
    appendCell(row, plan.blocker_count);
    appendCell(row, plan.next_review_action);
    const actionCell = document.createElement("td");
    const button = document.createElement("button");
    button.type = "button";
    button.textContent = "View plan";
    button.addEventListener("click", () => viewStoredPlan(plan.plan_id));
    actionCell.appendChild(button);
    row.appendChild(actionCell);
    elements.planHistoryBody.appendChild(row);
  }
}

function appendCell(row, value) {
  const cell = document.createElement("td");
  cell.textContent = value == null || value === "" ? "-" : String(value);
  row.appendChild(cell);
}

function renderCompareOptions(plans) {
  clear(elements.comparePlanA);
  clear(elements.comparePlanB);
  for (const plan of plans || []) {
    const optionA = document.createElement("option");
    optionA.value = plan.plan_id;
    optionA.textContent = plan.plan_id;
    const optionB = optionA.cloneNode(true);
    elements.comparePlanA.appendChild(optionA);
    elements.comparePlanB.appendChild(optionB);
  }
  if (plans && plans.length > 1) {
    elements.comparePlanB.selectedIndex = 1;
  }
  elements.comparePlans.disabled = !plans || plans.length < 2;
}

function renderCompareEmpty(text) {
  clear(elements.compareOutput);
  appendEmpty(elements.compareOutput, text);
}

function renderGuidanceOptions(plans) {
  clear(elements.guidancePlanSelect);
  for (const plan of plans || []) {
    const option = document.createElement("option");
    option.value = plan.plan_id;
    option.textContent = `${plan.plan_id} (${plan.status})`;
    elements.guidancePlanSelect.appendChild(option);
  }
  const hasPlans = Boolean(plans && plans.length);
  if (!hasPlans) {
    const option = document.createElement("option");
    option.value = "";
    option.textContent = "No plans available";
    elements.guidancePlanSelect.appendChild(option);
  }
  elements.generateGuidance.disabled = !hasPlans;
}

function renderGuidanceEmpty(text) {
  clear(elements.guidanceOutput);
  appendEmpty(elements.guidanceOutput, text);
}

function renderComparison(comparison) {
  clear(elements.compareOutput);
  const rows = [
    ["Status", comparison.status_delta],
    ["Review action", comparison.next_review_action_delta],
    ["Total budget delta", comparison.token_budget_delta],
    ["Context delta", comparison.context_budget_delta],
    ["Execution delta", comparison.execution_budget_delta],
    ["Step count delta", comparison.step_count_delta],
    ["Approval gate delta", comparison.approval_gate_delta],
    ["Blocker delta", comparison.blocker_delta],
  ];
  const dl = document.createElement("dl");
  dl.className = "comparison-list";
  for (const [label, value] of rows) {
    const item = document.createElement("div");
    const dt = document.createElement("dt");
    dt.textContent = label;
    const dd = document.createElement("dd");
    dd.textContent = value == null || value === "" ? "-" : String(value);
    item.append(dt, dd);
    dl.appendChild(item);
  }
  const note = document.createElement("p");
  note.className = "step-meta";
  note.textContent = comparison.efficiency_note || "No comparison note.";
  elements.compareOutput.append(dl, note);
}

function renderGuidanceList(section, title, items, emptyText, formatter) {
  const wrapper = document.createElement("section");
  wrapper.className = "guidance-subsection";
  const heading = document.createElement("h3");
  heading.textContent = title;
  const list = document.createElement("ul");
  list.className = "notice-list";
  wrapper.append(heading, list);
  section.appendChild(wrapper);
  if (!items || items.length === 0) {
    appendEmpty(list, emptyText);
    return;
  }
  for (const item of items) {
    const li = document.createElement("li");
    li.textContent = formatter(item);
    list.appendChild(li);
  }
}

function renderGuidance(guidance) {
  clear(elements.guidanceOutput);
  if (!guidance) {
    renderGuidanceEmpty("No guidance returned.");
    return;
  }

  const summary = document.createElement("dl");
  summary.className = "guidance-summary";
  const rows = [
    ["Plan", guidance.plan_id],
    ["Status", guidance.status],
    ["Recommended option", guidance.recommended_option],
    ["Next review action", guidance.next_review_action],
    ["Executable", String(guidance.executable)],
    ["Preview only", String(guidance.preview_only)],
  ];
  for (const [label, value] of rows) {
    const item = document.createElement("div");
    const dt = document.createElement("dt");
    dt.textContent = label;
    const dd = document.createElement("dd");
    dd.textContent = value == null || value === "" ? "-" : String(value);
    item.append(dt, dd);
    summary.appendChild(item);
  }
  elements.guidanceOutput.appendChild(summary);

  renderGuidanceList(
    elements.guidanceOutput,
    "Advisory options",
    guidance.options,
    "No options returned.",
    (option) => `${option.option}: ${option.reason} (${option.allowed_effect})`,
  );
  renderGuidanceList(
    elements.guidanceOutput,
    "Evidence requirements",
    guidance.evidence_requirements,
    "No evidence requirements returned.",
    (requirement) => `${requirement.kind}: ${requirement.reason}; required=${requirement.required}`,
  );
  renderGuidanceList(
    elements.guidanceOutput,
    "Token-efficiency guidance",
    guidance.token_efficiency_guidance,
    "No token-efficiency guidance returned.",
    (item) => item,
  );

  const boundary = document.createElement("p");
  boundary.className = "guidance-boundary";
  boundary.textContent = guidance.boundary_notice || "Guidance is advisory only.";
  elements.guidanceOutput.appendChild(boundary);
}

async function refreshPlanWorkbench() {
  try {
    const [listData, summaryData] = await Promise.all([
      fetchJson(planListUrl()),
      fetchJson(planSummaryUrl()),
    ]);
    registeredPlanSummaries = Array.isArray(listData.plans) ? listData.plans : [];
    renderPlanSummary(summaryData.summary || {});
    renderPlanHistory(registeredPlanSummaries);
    renderCompareOptions(registeredPlanSummaries);
    renderGuidanceOptions(registeredPlanSummaries);
    if (registeredPlanSummaries.length < 2) {
      renderCompareEmpty("Select two stored plans to compare.");
    } else {
      renderCompareEmpty("Choose two stored plans and compare.");
    }
    if (registeredPlanSummaries.length === 0) {
      renderGuidanceEmpty("No plans available for guidance.");
    } else {
      renderGuidanceEmpty("Select a stored plan and generate guidance.");
    }
  } catch (error) {
    registeredPlanSummaries = [];
    renderPlanWorkbenchEmpty();
    renderCompareEmpty(error.message);
  }
}

async function viewStoredPlan(planId) {
  try {
    const data = await fetchJson(`/api/plans/${encodeURIComponent(planId)}`);
    renderPlan(data.plan);
  } catch (error) {
    renderPlanError(error.message);
  }
}

async function compareSelectedPlans() {
  const first = elements.comparePlanA.value;
  const second = elements.comparePlanB.value;
  if (!first || !second) {
    renderCompareEmpty("Select two stored plans to compare.");
    return;
  }
  try {
    const data = await fetchJson(
      `/api/plans/compare?plan_id=${encodeURIComponent(first)}&plan_id=${encodeURIComponent(second)}`,
    );
    renderComparison(data.comparison);
  } catch (error) {
    renderCompareEmpty(error.message);
  }
}

async function generateReviewGuidance() {
  const planId = elements.guidancePlanSelect.value;
  if (!planId) {
    renderGuidanceEmpty("Select a stored plan and generate guidance.");
    return;
  }
  try {
    setApiState("Reviewing plan");
    const data = await fetchJson(`/api/plans/review-guidance?plan_id=${encodeURIComponent(planId)}`);
    renderGuidance(data.guidance);
    setApiState("API connected");
  } catch (error) {
    renderGuidanceEmpty(error.message);
    setApiState("Guidance error");
  }
}

async function generatePlan(event) {
  event.preventDefault();
  try {
    setApiState("Planning");
    const data = await fetchJson("/api/plans", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(planPayload()),
    });
    renderPlan(data.plan);
    await refreshPlanWorkbench();
    setApiState("API connected");
  } catch (error) {
    renderPlanError(error.message);
    setApiState("Planning error");
  }
}

async function readFileAsJson(file) {
  const text = await file.text();
  return JSON.parse(text);
}

elements.reportFile.addEventListener("change", async (event) => {
  const [file] = event.target.files;
  if (!file) return;
  try {
    renderReport(await readFileAsJson(file));
  } catch (error) {
    renderReport({
      ...SAMPLE_REPORT,
      verdict: "BLOCKED",
      blockers: [`Invalid JSON report: ${error.message}`],
      warnings: [],
    });
  } finally {
    event.target.value = "";
  }
});

elements.resetReport.addEventListener("click", () => {
  renderReport(SAMPLE_REPORT);
  setApiState("Static sample");
});

elements.refreshRepos.addEventListener("click", refreshRepos);
elements.runAudit.addEventListener("click", runSelectedAudit);
elements.repoForm.addEventListener("submit", registerRepo);
elements.planForm.addEventListener("submit", generatePlan);
elements.refreshPlans.addEventListener("click", refreshPlanWorkbench);
elements.plansRepoFilter.addEventListener("change", refreshPlanWorkbench);
elements.plansStatusFilter.addEventListener("change", refreshPlanWorkbench);
elements.comparePlans.addEventListener("click", compareSelectedPlans);
elements.generateGuidance.addEventListener("click", generateReviewGuidance);

renderReport(SAMPLE_REPORT);
renderPlanEmpty();
renderPlanWorkbenchEmpty();
refreshRepos();
