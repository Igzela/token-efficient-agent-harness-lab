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
| What are the mission/stage contracts, autonomy, testing, review, and merge rules? | `docs/AUTONOMY.md` |
| What are the high-level roadmap milestones and research horizons? | `docs/ROADMAP.md` |
| What are the live observed PR heads, CI results, and review observations? | A fresh `scripts/project_context.py` capsule, verified against GitHub |
| How does independent review converge (severity vs disposition, R1/R2 budget, exact PASS + deferred notes)? | `docs/AUTONOMY.md` → **Review Convergence Protocol** |
| What operator procedure has actually been proved? | `docs/RUNBOOK.md` |
| What may an implementation agent do? | `AGENTS.md` |
| What is the public product and how is it used? | `README.md` |
| What is specific to Claude Code? | `CLAUDE.md` |

Current code, merged history, exact-head CI, and authoritative documents outrank stale chat summaries, old local branches, prior review conclusions, or branch-local status prose.

Independent review is a **convergence process**, not an unbounded nit loop. Exact `PASS` is the only merge-authorizing control verdict and may carry deferred non-blocking notes; open blocking disposition, not zero suggestions, gates merge eligibility. Capsule generators project review state only—they do not decide severity, disposition, or repair rounds. Full rules live in `docs/AUTONOMY.md`; do not restate them elsewhere.

Review has no `COMPLETE` verdict. Exact `PASS` satisfies only the independent-review gate for one exact head. Mission and Stage lifecycle completion remain owned by `docs/AUTONOMY.md` and live journal facts; never infer it from a receipt, CI result, capsule, PR merge alone, or handoff headline.

## Establish the Leading Valid Frontier

Never interpret “latest” as “the newest branch wins.” Establish these three layers first:

1. **Accepted baseline** — the latest remote `main` commit that defines repository authority.
2. **Active implementation frontier** — the latest exact head of the active Stage or WorkCard owned PR.
3. **Blocked future frontier** — stacked, deferred, or unaccepted work that may be inspected but cannot define accepted truth or authorize later work.

Before editing:

```text
refresh remote main
→ refresh open PR heads, dependencies, CI, and reviews
→ read ARCHITECTURE and AUTONOMY from the accepted baseline
→ resolve the active Stage or WorkCard
→ confirm its exact owned PR head
→ confirm the local checkout/worktree matches the intended frontier
→ read only the relevant owner, architecture, playbook, code, and tests
→ state scope, non-goals, authority, acceptance, rollback, and hard stops
→ begin work
```

A new PR head invalidates earlier CI and review conclusions for that PR. A blocked downstream PR never becomes the baseline for its prerequisite. Branch-local status or routing prose is proposed content until merged and must not override accepted-main navigation.

## One-Command Session Bootstrap

Every repository-maintenance session starts here, but no agent should load every maintained document. A coding agent, including a fresh successor resuming interrupted work, first runs one accepted-main entry command:

```bash
uv run --no-project python scripts/session_context.py enter --role coding
```

The digest-bound JSON composes the accepted current packet/mission contract, its complete bounded autonomous worker dispatch capsule, current checkout, and any Git-private checkpoint. Treat `deferred_documents` as already projected startup context: do not reread them unless the entry reports a conflict, a missing fact, or a stop condition.

Planning, review, CI-repair, operator, and contributor sessions request their bounded accepted document route:

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

The machine-readable `agent-context-routes:v1` marker above is the enforced route contract for `scripts/session_context.py` and `scripts/check_agent_handoff.py`; the human table below is its readable projection.

| Role | Reading route |
|---|---|
| Planning or architecture model | `START_HERE.md` → `docs/ARCHITECTURE.md` → `docs/AUTONOMY.md` → `docs/ROADMAP.md` |
| Coding agent | `START_HERE.md` → `AGENTS.md` → `docs/ARCHITECTURE.md` → `docs/AUTONOMY.md` → relevant code/tests |
| Independent reviewer | `START_HERE.md` → `docs/AUTONOMY.md` → complete `base...head` diff → relevant owners/tests |
| CI repair agent | `START_HERE.md` → `AGENTS.md` → `docs/AUTONOMY.md` → exact failing logs → relevant owners/tests |
| Operator | `START_HERE.md` → `docs/ARCHITECTURE.md` → `docs/RUNBOOK.md` |
| Contributor or user | `README.md`; use this file before repository-maintenance work |

Use targeted reads. Do not load every document when the role and task narrow the necessary context.

## Planning and Execution Separation

The user owns the high-level Mission objective and approval. Autonomous Steward owns Stage breakdown and WorkCard generation. An implementation agent owns only bounded task execution: inspect code, implement minimal surgical edits, run tests, and return structured evidence.

## Generate a Fresh Handoff Capsule

From a repository checkout, run:

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
mission_id:
stage_id:
card_id:
working_pr:
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
