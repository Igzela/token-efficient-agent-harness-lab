# UI Dashboard Design

## Goals

1. **Read-only observability.** Display harness state without mutation. The dashboard is a window, not a control panel.
2. **CA maturity visibility.** Show which Controlled Adaptive gates have passed, failed, or are pending. Surface CA-7 closeout status.
3. **Governance traceability.** Link every policy decision, gate result, and approval to its source evidence.
4. **Eval / cost visibility.** Show eval results, cost-of-pass estimates, and usage ledger summaries.
5. **Security baseline.** Display security checker results, redaction policy compliance, and credential status.
6. **Future-track readiness.** Show which future tracks (sandbox, provider integration, multi-agent) are designed, in-progress, or blocked.

## Non-Goals

The dashboard does NOT:

- **Mutate state.** No state creation, modification, or deletion through the dashboard.
- **Activate or approve policies.** Policy lifecycle actions remain in governance tooling, not the dashboard.
- **Call providers.** No LLM API calls, no model inference, no streaming.
- **Execute sandbox operations.** No code execution, no container management.
- **Create or merge PRs.** No git operations through the dashboard.
- **Replace existing tooling.** The dashboard complements, not replaces, CLI-based workflows.

## Dashboard Views

### 1. Overview

Single-pane summary of harness health. Shows gate status, eval results, cost summary, security status, and active tracks. Read-only.

### 2. CA Gates

List of all Controlled Adaptive gates (CA-1 through CA-7). For each gate: status (passed/failed/pending/blocked), evidence file references, approval status, and linked commits. Read-only.

### 3. Eval Evidence

Eval results dashboard. Shows eval name, pass/fail status, fixture counts, cost-of-pass estimates, and links to eval fixture files. Read-only.

### 4. Policy Lifecycle

All policies in the system. For each: current status (draft/approved/active/rejected/retired), author, approval chain, linked governance docs. Read-only — no activation or modification.

### 5. Governance

Governance approval flow. Shows pending approvals, approval history, governance doc status, and stakeholder sign-offs. Read-only.

### 6. Usage Cost

Usage ledger summaries. Shows cost-of-pass per eval, total estimated costs, token usage breakdowns by provider/model. Read-only.

### 7. Model Profiles

Registered model profiles. For each: model ID, provider, capabilities, routing rules, shadow status, linked eval results. Read-only.

### 8. Context Pack

Context pack v2 status. Shows pack contents, version, last-updated timestamp, linked eval fixtures, and coverage metrics. Read-only.

### 9. Tool Error

Tool error taxonomy. Shows error categories, frequency counts, last-seen timestamps, linked fixes, and regression status. Read-only.

### 10. Security

Security checker results. Shows credential status, redaction compliance, sensitive file detection, and security advisory status. Read-only.

### 11. Future Tracks

Status of future development tracks. Shows designed vs. implemented vs. blocked for sandbox execution, provider integration, multi-agent coordination, and other roadmap items. Read-only.

## First Implementation Mode

The first implementation must be one of:

- **Static report.** A generated markdown or HTML file viewed locally.
- **Local HTML file.** An HTML file opened directly in a browser (file:// protocol). No server required.

### Requirements for first implementation

- No server. No HTTP listener. No network binding.
- No frontend framework. Plain HTML/CSS/JS or static markdown.
- No build step. No `npm install`, `pip install`, or equivalent.
- No external dependencies. All assets self-contained.
- No authentication. Local file access only.
- No telemetry. No analytics, no external calls.

## Core Principle

**UI displays state. UI does not create state.**

The dashboard reads existing data files (JSON, markdown, eval fixtures, usage ledgers) and renders them. It never writes to these files. Any future write capability requires a separate design, entry criteria, and human approval.
