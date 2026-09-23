# Start Here

This is the canonical navigation entry for every maintainer, planning model, coding agent, reviewer, CI-repair agent, and operator working on this repository.

It does not own current status, future routing, architecture details, or operational procedures. It tells you where those truths live and how to establish the latest valid working frontier before acting.

## Quality Order

Use this priority order whenever goals compete:

```text
correctness, safety, evidence, recovery, and rollback
→ architecture and authority integrity
→ maintainability and one canonical owner
→ low duplication and low context cost
```

Conciseness is a quality-preserving optimization, never a reason to remove required authority, failure, compatibility, audit, or recovery semantics.

## Source-of-Truth Hierarchy

| Question | Canonical source |
|---|---|
| Who owns a module or architecture boundary? | `docs/ARCHITECTURE.md` |
| What are the autonomy, testing, review, and merge rules? | `docs/AUTONOMY.md` |
| What are the closed-loop roadmap milestones and research programs? | `docs/ROADMAP.md` |
| What product or maintenance work is next? | The user's latest request; otherwise `docs/ROADMAP.md` plus live GitHub issues/PRs |
| What are the live PR heads, CI results, and review observations? | Fresh Git/GitHub state; `scripts/project_context.py` is an optional summary |
| How does independent review converge (severity vs disposition, R1/R2 budget, exact PASS + deferred notes)? | `docs/AUTONOMY.md` → **Review Convergence Protocol** |
| What operator procedure has actually been proved? | `docs/RUNBOOK.md` |
| What may an implementation agent do? | `AGENTS.md` |
| What is the public product and how is it used? | `README.md` |
| What is specific to Claude Code? | `CLAUDE.md` |

Current code, merged history, exact-head CI, and authoritative documents outrank stale chat summaries, old local branches, prior review conclusions, or branch-local status prose.

Independent review is a **convergence process**, not an unbounded nit loop. Exact `PASS` is the only merge-authorizing control verdict and may carry deferred non-blocking notes; open blocking disposition, not zero suggestions, gates merge eligibility. Capsule generators project review state only—they do not decide severity, disposition, or repair rounds. Full rules live in `docs/AUTONOMY.md`; do not restate them elsewhere.

Review has no `COMPLETE` verdict. Exact `PASS` satisfies only the independent-review gate for one exact head; never infer completion from a receipt, CI result, capsule, PR merge alone, or a handoff headline.

## Choose and Start the Work

Use this sequence:

1. The user's latest explicit request is the task.
2. If no task was given, refresh `main`, open PRs, issues, and CI, then select the next unblocked product-development or repository-maintenance item in `docs/ROADMAP.md`. Do not start a research/model experiment as a substitute for implementation work.
3. Work on a feature branch. If the checkout is `main`, refresh it and create or switch to a feature branch before editing.
4. Read only the relevant owner docs, source, and tests; make one coherent change, verify it, and leave a reviewable PR.

Never interpret “latest” as “the newest branch wins.” A blocked or unmerged PR is context, not accepted truth. A new PR head invalidates its previous CI and review evidence. Ordinary coding has no generated task assignment or lifecycle prerequisite.

## Optional Session Context Tool

The normal entrypoint is this file and the task in the user's request. No generated capsule, command, checkpoint, Mission, Stage, WorkCard, or Steward service is required before an agent can inspect or edit a feature branch. For convenience, this local command summarizes accepted docs and checkout facts; it is optional and never decides what work is allowed:

```bash
uv run --no-project python scripts/session_context.py enter --role coding
```

On a feature branch it returns ordinary repository context. On `main` it is read-only and tells the agent to create a feature branch. `.git` and `.github` remain protected; exact-head review, CI, rollback, and guarded merge still apply before delivery.

Other roles may use the helper to print a bounded reading route; the human-readable routes above remain sufficient:

```bash
uv run --no-project python scripts/session_context.py route --role planning
```

Replace `planning` with `review`, `ci-repair`, `operator`, or `contributor`. The route contains at most six ordered documents and `START_HERE.md` is always first.

<!-- agent-context-routes:v1
{
  "max_required_documents": 6,
  "roles": {
    "ci-repair": {
      "optional": {
        "owners": "docs/ARCHITECTURE.md"
      },
      "required": [
        "START_HERE.md",
        "AGENTS.md",
        "docs/AUTONOMY.md"
      ]
    },
    "coding": {
      "optional": {
        "roadmap": "docs/ROADMAP.md"
      },
      "required": [
        "START_HERE.md",
        "AGENTS.md",
        "docs/ARCHITECTURE.md",
        "docs/AUTONOMY.md"
      ]
    },
    "contributor": {
      "optional": {
        "implementation": "AGENTS.md"
      },
      "required": [
        "START_HERE.md",
        "README.md"
      ]
    },
    "operator": {
      "optional": {},
      "required": [
        "START_HERE.md",
        "docs/ARCHITECTURE.md",
        "docs/RUNBOOK.md"
      ]
    },
    "planning": {
      "optional": {
        "roadmap": "docs/ROADMAP.md"
      },
      "required": [
        "START_HERE.md",
        "docs/ARCHITECTURE.md",
        "docs/AUTONOMY.md"
      ]
    },
    "review": {
      "optional": {
        "architecture": "docs/ARCHITECTURE.md"
      },
      "required": [
        "START_HERE.md",
        "docs/AUTONOMY.md"
      ]
    }
  },
  "schema_version": "agent_context_routes.v1"
}
-->

## Role Routes

The machine-readable `agent-context-routes:v1` marker above is an optional route contract for `scripts/session_context.py` and `scripts/check_agent_handoff.py`; the human table below is its readable projection.

| Role | Reading route |
|---|---|
| Planning or architecture model | `START_HERE.md` → `docs/ARCHITECTURE.md` → `docs/AUTONOMY.md` → `docs/ROADMAP.md` |
| Coding agent | `START_HERE.md` → `AGENTS.md` → `docs/ARCHITECTURE.md` → `docs/AUTONOMY.md` → relevant code/tests |
| Independent reviewer | `START_HERE.md` → `docs/AUTONOMY.md` → complete `base...head` diff → relevant owners/tests |
| CI repair agent | `START_HERE.md` → `AGENTS.md` → `docs/AUTONOMY.md` → exact failing logs → relevant owners/tests |
| Operator | `START_HERE.md` → `docs/ARCHITECTURE.md` → `docs/RUNBOOK.md` |
| Contributor or user | `README.md`; use this file before repository-maintenance work |

Use targeted reads. Do not load every document when the role and task narrow the necessary context.

## Ordinary Planning and Execution

The user's request or the next unblocked roadmap item supplies the objective. The coding agent inspects the repository, chooses a coherent implementation slice, edits the source directly, runs tests, and reports evidence. Planning and implementation do not require a separate assignment object or generated task card. The normal repository boundaries still apply: do not commit secrets, write protected `main` directly, invoke providers, deploy, release, or bypass review/CI/rollback requirements.

## Optional Handoff Capsule

For a compact status summary, you may run this from a repository checkout:

```bash
uv run --no-project python scripts/project_context.py
```

Useful variants:

```bash
uv run --no-project python scripts/project_context.py --format json
uv run --no-project python scripts/project_context.py --offline
```

## Automation Boundary

The repository provides an on-demand generator and a terminal `context-capsule` CI job. After all source-test jobs reach terminal state, that job generates a token-free capsule, publishes a sanitized job summary and artifact, and validates the source-check matrix.

## End-of-Work Handoff

Every implementation or review board should leave a compact report containing:

```yaml
accepted_main_sha:
branch:
exact_head:
what_changed:
what_was_verified:
ci:
independent_review:          # PASS/BLOCKED/FAIL/DECISION_REQUIRED
remaining_blockers:
next_permitted_action:
forbidden_next_actions:
documents_updated:
```

When independent review or repair rounds ran, also report:

```yaml
review_protocol_version:
review_mode:                 # full | repair_verification
review_round:                # 1 | 2
prior_reviewed_head:
finding_ledger_digest:
open_blocker_ids:
deferred_note_ids:
autonomous_repairs_remaining:
stop_reason:
```

## Documentation Discipline

Prefer complete, accurate, canonical, low-duplication documentation, then make it as short as those qualities permit. Active governance documents are strictly capped at seven files:
1. `README.md`
2. `START_HERE.md`
3. `AGENTS.md`
4. `docs/ARCHITECTURE.md`
5. `docs/AUTONOMY.md`
6. `docs/ROADMAP.md`
7. `docs/RUNBOOK.md`
