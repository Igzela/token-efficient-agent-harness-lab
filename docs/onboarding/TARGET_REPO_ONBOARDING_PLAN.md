# Target Repo Onboarding Plan

## 1. Purpose

This plan defines a safe, minimal process for onboarding a non-harness-managed repository so the existing Harness App can audit and plan against it.

**This plan does not authorize modifying any target repo.** Onboarding requires explicit user approval before any target repo writes occur.

## 2. Trigger

Trial 2 (hermes-gateway-lab) returned audit verdict `BLOCKED` because required harness control files were missing. This is correct behavior — the app correctly refuses to plan for repos it cannot audit.

The onboarding process addresses this gap: it adds minimal control files to a target repo so the app can audit, plan, and provide guidance without changing the target's product or runtime behavior.

## 3. Onboarding Principles

1. **Minimal control surface first.** Add only the files the auditor requires. Do not import full harness complexity.
2. **Read-only audit before planning.** Run the audit after onboarding. Only proceed if it passes.
3. **Human approval before any target repo writes.** Onboarding writes to the target repo. This requires explicit user approval.
4. **Target repo owns its boundaries.** Onboarding files reflect the target's own project structure, not the harness lab's.
5. **No execution authority.** Onboarding files do not authorize execution, deployment, or autonomous action.

## 4. Minimal Required Files

### Required (auditor checks for these)

| File | Purpose |
|------|---------|
| `AGENTS.md` | Top-level project identity and agent behavior rules |
| `docs/harness/PROJECT_BOARD.md` | Project board with active/completed/blocked tracks |
| `docs/harness/TASK_QUEUE.md` | Current task queue with status |
| `docs/harness/QUALITY_GATES.md` | Quality gates and pass/fail criteria |
| `docs/harness/DECISION_RECORD.md` | Key decisions and rationale |
| `docs/harness/RISK_REGISTER.md` | Known risks and mitigations |

### Optional (auditor notes but does not block)

| File | Purpose |
|------|---------|
| `docs/harness/EVIDENCE_INDEX.md` | Index of evidence artifacts |
| `docs/harness/FINAL_GATE.md` | Final gate criteria for project completion |
| `docs/harness/RUN_LOG.md` | Execution log |

## 5. File Purpose Table

| File | Minimum Content | What Not To Include | Template Available | Human Review Required |
|------|----------------|---------------------|-------------------|----------------------|
| `AGENTS.md` | Project name, repo path, what the project is, what it is not, safety boundaries | Secrets, credentials, real config values | Yes | Yes |
| `PROJECT_BOARD.md` | Track names, status (Complete/In Progress/Planned/Blocked), one-line description per track | Execution details, code references | Yes | Yes |
| `TASK_QUEUE.md` | Current tasks with ID, objective, status, risk level | Actual implementation steps | Yes | Yes |
| `QUALITY_GATES.md` | Gate names, criteria, pass/fail status, which gates are active | Specific test commands (use placeholders) | Yes | Yes |
| `DECISION_RECORD.md` | Decision ID, date, decision, rationale, alternatives considered | Blame, personal opinions | Yes | Yes |
| `RISK_REGISTER.md` | Risk ID, description, likelihood, impact, mitigation, owner | Confidential information | Yes | Yes |
| `EVIDENCE_INDEX.md` | Evidence ID, description, location, date | Actual evidence content (link only) | Yes | Optional |
| `FINAL_GATE.md` | Gate criteria, current status, what "done" means | Ambiguous or moving criteria | Yes | Optional |
| `RUN_LOG.md` | Date, action, result, notes | Sensitive operational details | Yes | Optional |

## 6. Safety Boundary

Onboarding must:

- **Require explicit user approval** before any target repo writes.
- **Not change product/runtime behavior.** Onboarding files are governance metadata, not code.
- **Not edit source code.** Only add files under `AGENTS.md` and `docs/harness/`.
- **Not mark blocked/future work as executable.** Task states reflect reality.
- **Not add provider/sandbox/worker/deployment behavior.** These require separate approval.
- **Not commit secrets or local config.** Templates use placeholders only.
- **Preserve existing dirty/untracked state.** Record it, do not clean or alter it.

## 7. hermes-gateway-lab Candidate Onboarding Notes

Based on Trial 2 findings:

| Property | Value |
|----------|-------|
| Repo type | Operational gateway/worker system |
| Key components | approval queues, dry-run executors, permission deny patterns, systemd services |
| Docs | 25+ design docs, runbooks, safety models |
| Scripts | 40+ Python/shell files, smoke tests |
| Pre-existing dirty state | 1 modified draft file (`drafts/hermes-local-execution-worker.service.draft`) |
| Onboarding approach | Add minimal control files that reflect the repo's operational nature |

Do not clean or alter the pre-existing dirty state. Record it in the onboarding baseline.

## 8. Onboarding Workflow

### A. Record target baseline

```bash
git -C <target> status -sb
git -C <target> status --porcelain=v1
git -C <target> diff --stat
git -C <target> log --oneline -1
```

### B. Create onboarding branch in target repo

**Only after user approval.**

```bash
git -C <target> checkout -b harness-onboarding
```

### C. Add minimal harness control files

Use templates from `TARGET_REPO_ONBOARDING_TEMPLATE.md`. Replace placeholders with target-specific values.

### D. Run harness audit

```bash
curl -s "http://127.0.0.1:8769/api/audit?repo_id=<target-id>"
```

### E. Expected audit result

- Verdict: `PASS` or `PASS_WITH_NOTES`
- Warnings: explainable (e.g., missing optional files)
- Blockers: `[]`

### F. Commit target repo onboarding files

```bash
git -C <target> add AGENTS.md docs/harness/
git -C <target> commit -m "Add minimal harness control files for onboarding"
```

### G. Do not proceed to planning until audit passes

If audit is still BLOCKED, investigate and fix the onboarding files. Do not bypass the audit.

## 9. Acceptance Criteria

- Target repo gains minimal harness control files.
- No source/runtime files changed.
- No secrets committed.
- Existing dirty state unchanged or explicitly handled.
- Harness audit no longer BLOCKED for missing required files.
- App can create non-executable plans.
- Target remains under human authority.

## 10. Failure Criteria

- Source code changed.
- Runtime behavior changed.
- Secrets/config included.
- Existing dirty state overwritten.
- App still BLOCKED after onboarding.
- Onboarding files imply execution authority.
- Provider/sandbox/worker/deployment behavior introduced.
