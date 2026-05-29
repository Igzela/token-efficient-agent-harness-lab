# Dashboard Data Contract

## Overview

The dashboard reads from existing data sources. It does not write to them. Every data source is defined with: source file, read mode, display fields, sensitive fields, evidence status, and whether it can trigger actions.

## Data Sources

### 1. Project Board

- **Source file:** `PROJECT_BOARD.md` or equivalent
- **Read mode:** Read-only file read
- **Display fields:** Board name, status, linked tracks, last updated
- **Sensitive fields:** None
- **Evidence status:** Evidence file reference
- **Can trigger action:** No

### 2. Task Queue

- **Source file:** `TASKS.md` or task tracking file
- **Read mode:** Read-only file read
- **Display fields:** Task ID, title, status, assignee, dependencies
- **Sensitive fields:** None
- **Evidence status:** Task link
- **Can trigger action:** No

### 3. Final Gate (CA-7)

- **Source file:** `CA7_CONTROLLED_ADAPTIVE_CLOSEOUT_REPORT.md`
- **Read mode:** Read-only file read
- **Display fields:** Gate ID, status (passed/failed/pending), approval date, approver, evidence references
- **Sensitive fields:** None
- **Evidence status:** Report file reference
- **Can trigger action:** No

### 4. Quality Gate

- **Source file:** Quality gate result files
- **Read mode:** Read-only file read
- **Display fields:** Gate name, pass/fail, criteria met, linked tests
- **Sensitive fields:** None
- **Evidence status:** Gate result file
- **Can trigger action:** No

### 5. Eval Fixtures

- **Source file:** `tests/evals/` directory, eval fixture JSON files
- **Read mode:** Read-only directory scan + file read
- **Display fields:** Eval name, fixture count, pass/fail, cost estimate, last run date
- **Sensitive fields:** Raw provider responses (redacted)
- **Evidence status:** Eval fixture file reference
- **Can trigger action:** No

### 6. Context Pack

- **Source file:** `docs/context_pack_v2.md`
- **Read mode:** Read-only file read
- **Display fields:** Pack version, last updated, contents summary, coverage metrics
- **Sensitive fields:** None
- **Evidence status:** Context pack file reference
- **Can trigger action:** No

### 7. Usage Ledger

- **Source file:** `USAGE_LEDGER.md` or usage tracking files
- **Read mode:** Read-only file read
- **Display fields:** Cost-of-pass estimates, token usage, provider breakdowns, model breakdowns
- **Sensitive fields:** API keys (never displayed), raw cost data (redacted to summaries)
- **Evidence status:** Ledger file reference
- **Can trigger action:** No

### 8. Model Profiles

- **Source file:** `docs/model_profiles_shadow_routing.md`
- **Read mode:** Read-only file read
- **Display fields:** Model ID, provider, capabilities, routing rules, shadow status
- **Sensitive fields:** Provider credentials (redacted)
- **Evidence status:** Profile file reference
- **Can trigger action:** No

### 9. Security Checker

- **Source file:** `docs/security/` directory, security check results
- **Read mode:** Read-only directory scan + file read
- **Display fields:** Check name, status (pass/fail/warn), description, linked remediation
- **Sensitive fields:** Credential files (referenced, not displayed), raw scan output (redacted)
- **Evidence status:** Security check result file
- **Can trigger action:** No

### 10. CI Results

- **Source file:** CI workflow results, test output files
- **Read mode:** Read-only file read
- **Display fields:** Workflow name, status, test count, pass/fail, duration, commit SHA
- **Sensitive fields:** CI secrets (never displayed)
- **Evidence status:** CI result file reference
- **Can trigger action:** No

### 11. CA-7 Report

- **Source file:** `CA7_CONTROLLED_ADAPTIVE_CLOSEOUT_REPORT.md`
- **Read mode:** Read-only file read
- **Display fields:** Report sections, gate checklist, approval status, linked evidence
- **Sensitive fields:** None
- **Evidence status:** Report file reference
- **Can trigger action:** No

### 12. Policy Candidates

- **Source file:** `docs/policy_candidate_lifecycle.md`
- **Read mode:** Read-only file read
- **Display fields:** Policy name, status (draft/approved/active/rejected/retired), author, approval chain
- **Sensitive fields:** None
- **Evidence status:** Policy file reference
- **Can trigger action:** No

### 13. Governance Docs

- **Source file:** `docs/governance_approval_path.md`, governance approval records
- **Read mode:** Read-only file read
- **Display fields:** Doc name, status, approval chain, sign-off dates
- **Sensitive fields:** Signer identities (displayed with permission)
- **Evidence status:** Governance doc reference
- **Can trigger action:** No

### 14. Tool Error Taxonomy

- **Source file:** `docs/tool_error_taxonomy.md`
- **Read mode:** Read-only file read
- **Display fields:** Error category, frequency, last seen, linked fixes, regression status
- **Sensitive fields:** None
- **Evidence status:** Taxonomy file reference
- **Can trigger action:** No

### 15. Forward Plan

- **Source file:** `docs/NEXT_DECISION.md`
- **Read mode:** Read-only file read
- **Display fields:** Track name, status, milestones, dependencies, blockers
- **Sensitive fields:** None
- **Evidence status:** Forward-plan file reference
- **Can trigger action:** No

### 16. Architecture Docs

- **Source file:** `docs/architecture/`, `docs/project_architecture_audit.md`
- **Read mode:** Read-only directory scan + file read
- **Display fields:** Module name, status, dependencies, audit findings
- **Sensitive fields:** None
- **Evidence status:** Architecture doc reference
- **Can trigger action:** No

### 17. Test Matrix

- **Source file:** `docs/TEST_MATRIX.md`
- **Read mode:** Read-only file read
- **Display fields:** Test category, count, pass/fail, coverage percentage
- **Sensitive fields:** None
- **Evidence status:** Test matrix file reference
- **Can trigger action:** No

### 18. Sandbox Design

- **Source file:** `docs/sandbox_execution/` directory
- **Read mode:** Read-only directory scan + file read
- **Display fields:** Design status, implementation status, linked tests
- **Sensitive fields:** None
- **Evidence status:** Design doc reference
- **Can trigger action:** No

### 19. Provider Integration

- **Source file:** `docs/provider_integration/` directory
- **Read mode:** Read-only directory scan + file read
- **Display fields:** Provider name, status, design doc status, implementation status
- **Sensitive fields:** Provider credentials (redacted)
- **Evidence status:** Design doc reference
- **Can trigger action:** No

### 20. Events Log (Read-Only)

- **Source file:** `events.jsonl` (read-only, never written by dashboard)
- **Read mode:** Read-only file read, last N entries
- **Display fields:** Event type, timestamp, summary (no raw payloads)
- **Sensitive fields:** Raw event payloads (redacted), credentials (redacted)
- **Evidence status:** Event log reference
- **Can trigger action:** No

### 21. Packaging Status

- **Source file:** `docs/PACKAGING.md`
- **Read mode:** Read-only file read
- **Display fields:** Package name, version, status, dependencies, build status
- **Sensitive fields:** None
- **Evidence status:** Packaging doc reference
- **Can trigger action:** No

## Schema: `dashboard_snapshot`

A `dashboard_snapshot` is the complete read-only view of harness state at a point in time.

```json
{
  "snapshot_id": "string — timestamp-based unique ID",
  "generated_at": "ISO 8601 timestamp",
  "harness_version": "string — current harness version",
  "gate_status": {
    "ca_1": "passed | failed | pending | blocked",
    "ca_2": "passed | failed | pending | blocked",
    "ca_3": "passed | failed | pending | blocked",
    "ca_4": "passed | failed | pending | blocked",
    "ca_5": "passed | failed | pending | blocked",
    "ca_6": "passed | failed | pending | blocked",
    "ca_7": "passed | failed | pending | blocked"
  },
  "eval_summary": {
    "total_evals": "number",
    "passed": "number",
    "failed": "number",
    "pending": "number",
    "cost_of_pass_total": "string — redacted estimate"
  },
  "security_summary": {
    "checks_run": "number",
    "passed": "number",
    "failed": "number",
    "warnings": "number"
  },
  "usage_summary": {
    "total_tokens": "string — redacted",
    "estimated_cost": "string — redacted",
    "provider_breakdown": "object — redacted per provider"
  },
  "tracks": [
    {
      "name": "string",
      "status": "designed | in_progress | blocked | completed",
      "blocked_reason": "string | null"
    }
  ],
  "panels": ["array of dashboard_panel references"]
}
```

## Schema: `dashboard_panel`

A `dashboard_panel` is a single view within the dashboard.

```json
{
  "panel_id": "string — unique panel identifier",
  "title": "string — human-readable title",
  "description": "string — what this panel shows",
  "data_sources": ["array of data source IDs used"],
  "display_fields": ["array of field names shown"],
  "redacted_fields": ["array of fields that are redacted"],
  "last_updated": "ISO 8601 timestamp",
  "read_only": true,
  "allowed_actions": []
}
```

## Critical Constraint

**`allowed_actions` must be empty.**

The dashboard displays state. It does not act on state. Every panel, every view, every data source is read-only. Any future write capability requires a separate design with its own entry criteria and approval process.
