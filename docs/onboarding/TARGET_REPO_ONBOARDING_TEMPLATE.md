# Target Repo Onboarding Templates

Generic templates for onboarding a non-harness-managed repo. Replace `{{placeholders}}` with target-specific values.

**Safety:** These templates do not authorize execution, deployment, or autonomous action. Target repo writes require human approval.

---

## AGENTS.md

```markdown
# {{PROJECT_NAME}}

{{ONE_LINE_DESCRIPTION}}

## Identity

- **Repository:** {{TARGET_REPO_PATH}}
- **Current commit:** {{CURRENT_COMMIT}}
- **Purpose:** {{PROJECT_PURPOSE}}

## What This Project Is Not

- Not a production runtime unless separately approved.
- No provider/model calls unless separately approved.
- No sandbox/process/container/VM execution unless separately approved.
- No autonomous workers unless separately approved.

## Safety Boundaries

- Target repo writes require human approval.
- Execution adapter is not governance authority.
- No provider/model/sandbox/worker/deployment unless separately approved.

## Harness Control Files

This repo has been onboarded with minimal harness control files under `docs/harness/`.
These files are governance metadata. They do not change product or runtime behavior.
```

---

## docs/harness/PROJECT_BRIEF.md

```markdown
# Project Brief: {{PROJECT_NAME}}

## Overview

{{PROJECT_DESCRIPTION}}

## Key Components

{{COMPONENT_LIST}}

## Repository Structure

{{STRUCTURE_DESCRIPTION}}

## Current State

- Commit: {{CURRENT_COMMIT}}
- Branch: {{CURRENT_BRANCH}}
- Onboarding date: {{ONBOARDING_DATE}}
```

---

## docs/harness/PROJECT_BOARD.md

```markdown
# Project Board

| Track | Status | Description |
|-------|--------|-------------|
| {{TRACK_1_NAME}} | {{TRACK_1_STATUS}} | {{TRACK_1_DESCRIPTION}} |
| {{TRACK_2_NAME}} | {{TRACK_2_STATUS}} | {{TRACK_2_DESCRIPTION}} |

Status vocabulary: Complete | In Progress | Planned | Blocked | Deferred
```

---

## docs/harness/TASK_QUEUE.md

```markdown
# Task Queue

| Task ID | Objective | Status | Risk Level | Assignee |
|---------|-----------|--------|------------|----------|
| {{TASK_1_ID}} | {{TASK_1_OBJECTIVE}} | {{TASK_1_STATUS}} | {{TASK_1_RISK}} | {{TASK_1_ASSIGNEE}} |

Status vocabulary: pending | in_progress | completed | blocked | deferred
Risk vocabulary: low | medium | high | critical
```

---

## docs/harness/QUALITY_GATES.md

```markdown
# Quality Gates

| Gate | Criteria | Status | Active |
|------|----------|--------|--------|
| {{GATE_1_NAME}} | {{GATE_1_CRITERIA}} | {{GATE_1_STATUS}} | {{GATE_1_ACTIVE}} |

Status vocabulary: PASS | FAIL | PENDING | NOT_APPLICABLE
```

---

## docs/harness/DECISION_RECORD.md

```markdown
# Decision Record

| ID | Date | Decision | Rationale | Alternatives |
|----|------|----------|-----------|--------------|
| {{DECISION_1_ID}} | {{DECISION_1_DATE}} | {{DECISION_1_TEXT}} | {{DECISION_1_RATIONALE}} | {{DECISION_1_ALTERNATIVES}} |
```

---

## docs/harness/RISK_REGISTER.md

```markdown
# Risk Register

| Risk ID | Description | Likelihood | Impact | Mitigation | Owner | Status |
|---------|-------------|------------|--------|------------|-------|--------|
| {{RISK_1_ID}} | {{RISK_1_DESCRIPTION}} | {{RISK_1_LIKELIHOOD}} | {{RISK_1_IMPACT}} | {{RISK_1_MITIGATION}} | {{RISK_1_OWNER}} | {{RISK_1_STATUS}} |

Likelihood vocabulary: low | medium | high
Impact vocabulary: low | medium | high | critical
Status vocabulary: open | mitigated | closed | accepted
```

---

## docs/harness/EVIDENCE_INDEX.md (optional)

```markdown
# Evidence Index

| Evidence ID | Description | Location | Date |
|-------------|-------------|----------|------|
| {{EVIDENCE_1_ID}} | {{EVIDENCE_1_DESCRIPTION}} | {{EVIDENCE_1_LOCATION}} | {{EVIDENCE_1_DATE}} |
```

---

## docs/harness/FINAL_GATE.md (optional)

```markdown
# Final Gate

## Criteria

{{FINAL_GATE_CRITERIA}}

## Current Status

{{FINAL_GATE_STATUS}}

## Definition of Done

{{DEFINITION_OF_DONE}}
```

---

## docs/harness/RUN_LOG.md (optional)

```markdown
# Run Log

| Date | Action | Result | Notes |
|------|--------|--------|-------|
| {{RUN_DATE}} | {{RUN_ACTION}} | {{RUN_RESULT}} | {{RUN_NOTES}} |
```
