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
| What is merged and accepted? | `docs/CURRENT_STATUS.md`, verified against remote `main`, merged history, tests, and CI |
| What is next, eligible, or blocked? | `docs/NEXT_DECISION.md` |
| Who owns a module or responsibility? | `docs/MODULE_MAP.md` |
| What are the durable architecture, authority, security, and recovery rules? | `docs/ARCHITECTURE_BOOK.md` |
| How are PRs, CI, review, merge, and rollback handled? | `docs/REAL_WORLD_TESTING_PLAYBOOK.md` |
| How does independent review converge (severity vs disposition, R1/R2 budget, exact PASS + deferred notes)? | `docs/REAL_WORLD_TESTING_PLAYBOOK.md` → **Review Convergence Protocol** |
| What operator procedure has actually been proved? | `docs/RUNBOOK.md` |
| What may an implementation agent do? | `AGENTS.md` |
| What is the public product and how is it used? | `README.md` |
| What is specific to Claude Code? | `CLAUDE.md` |

Current code, merged history, exact-head CI, and authoritative documents outrank stale chat summaries, old local branches, prior review conclusions, or branch-local status prose.

Independent review is a **convergence process**, not an unbounded nit loop. Exact `PASS` is the only merge-authorizing control verdict and may carry deferred non-blocking notes; open blocking disposition, not zero suggestions, gates merge eligibility. Capsule generators project review state only—they do not decide severity, disposition, or repair rounds. Full rules live in the playbook section above; do not restate them elsewhere.

Review Convergence Protocol has **no `COMPLETE` review verdict**. Exact `PASS` binds only the reviewed exact head and satisfies only the independent-review gate. Packet lifecycle state `COMPLETE` is owned by `docs/NEXT_DECISION.md` and accepted facts by `docs/CURRENT_STATUS.md`; it may be claimed only after the packet is merged, verified, independently reviewed, and documented. Never infer packet `COMPLETE` from a review receipt, CI result, capsule projection, PR merge alone, or a handoff headline.

## Establish the Leading Valid Frontier

Never interpret “latest” as “the newest branch wins.” Establish these three layers first:

1. **Accepted baseline** — the latest remote `main` commit that defines repository authority.
2. **Active implementation frontier** — the latest exact head of the earliest eligible packet’s owned PR.
3. **Blocked future frontier** — stacked, deferred, or unaccepted work that may be inspected but cannot define accepted truth or authorize later work.

Before editing:

```text
refresh remote main
→ refresh open PR heads, dependencies, CI, and reviews
→ read CURRENT_STATUS and NEXT_DECISION from the accepted baseline
→ resolve the earliest eligible packet
→ confirm its exact owned PR head
→ confirm the local checkout/worktree matches the intended frontier
→ read only the relevant owner, architecture, playbook, code, and tests
→ state scope, non-goals, authority, acceptance, rollback, and hard stops
→ begin work
```

A new PR head invalidates earlier CI and review conclusions for that PR. A blocked downstream PR never becomes the baseline for its prerequisite. Branch-local status or routing prose is proposed content until merged and must not override accepted-main navigation.

## Role Routes

| Role | Reading route |
|---|---|
| Planning or architecture model | `START_HERE.md` → `docs/CURRENT_STATUS.md` → `docs/NEXT_DECISION.md` → relevant `docs/ARCHITECTURE_BOOK.md` sections → code/tests |
| Coding agent | `START_HERE.md` → `AGENTS.md` → current status/next decision → `docs/MODULE_MAP.md` → relevant code/tests |
| Independent reviewer | `START_HERE.md` → current status/next decision → testing playbook **Review Convergence Protocol** + Exact-Head Review Receipt → complete `base...head` diff → relevant owners/tests; emit exact `PASS` with empty open blockers (deferred notes allowed) or stop with blockers / `DECISION_REQUIRED` |
| CI repair agent | `START_HERE.md` → `AGENTS.md` → testing playbook → exact failing logs → relevant owners/tests |
| Operator | `START_HERE.md` → current status → `docs/RUNBOOK.md` |
| Contributor or user | `README.md`; use this file before repository-maintenance work |

Use targeted reads. Do not load every document when the role and packet narrow the necessary context.

## Planning and Execution Separation

The planning or architecture process owns cross-packet direction, architecture and authority choices, packet scope and ordering, acceptance gates, and GO/NO-GO decisions. An implementation agent owns only packet-internal execution planning: inspect the real code, choose the smallest quality-preserving implementation, implement, test, synchronize the smallest canonical documents, and return evidence.

An implementation agent may resolve bounded design gaps only when doing so does not change the packet goal, prerequisites, authority, schema or durable contract, safety boundary, acceptance criteria, or sequence. If implementation requires any such change, stop with `DECISION_REQUIRED` and return the evidence, options, and consequences. A proposal does not become accepted direction until the appropriate canonical owner is updated under verified authority.

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

The command derives a compact, non-authoritative view from Git, GitHub CLI when available, and the canonical documents. It must mark unavailable facts as unavailable rather than guessing.

The capsule includes:

- accepted remote-main identity and evidence source;
- earliest routed packet and state;
- active PR exact head, CI summary, and review state when discoverable;
- blocked/open frontiers;
- next permitted action;
- required reading and hard stops.

The generated capsule is a transport view, not a new store or authority owner.

## Automation Boundary

The repository provides an on-demand generator and a terminal `context-capsule` CI job. After all seven source-test jobs reach terminal state, that job generates a token-free capsule, publishes a sanitized job summary and one-day artifact, and fails unless the complete source-check matrix is successful. Repository-controlled implementation, CI-repair, and review prompt builders regenerate and inject a fresh validated capsule at session start. No capsule is automatically injected into an arbitrary later ChatGPT, Claude Code, Codex, reviewer, or repair session.

This automation is non-authoritative and must preserve these rules:

- generate once per exact-head workflow, not once per job;
- bind the snapshot to accepted-main SHA, PR exact head, workflow run, required-check matrix, and observation time;
- do not infer exact-head review acceptance from an unbound aggregate review label;
- publish only a short-lived workflow artifact or job summary, never a committed dynamic “latest context” file;
- regenerate and inject a fresh capsule at the start of repository-controlled implementation, CI-repair, and review prompts;
- treat every generated snapshot as stale when main, head, CI, review, or canonical documents change.

## End-of-Work Handoff

Every implementation or review board should leave a compact report containing:

```yaml
accepted_main_sha:
packet:
packet_state:                # from CURRENT_STATUS/NEXT_DECISION; never inferred from review PASS
working_pr:
exact_head:
what_changed:
what_was_verified:
ci:
independent_review:          # PASS/BLOCKED/FAIL/DECISION_REQUIRED; never COMPLETE
remaining_blockers:
next_permitted_action:
forbidden_next_actions:
documents_updated:
```

When independent review or repair rounds ran, also report the bounded convergence fields that were observed (omit or mark unavailable when not run):

```yaml
review_protocol_version:
review_mode:                 # full | repair_verification
review_round:                # 1 | 2; never more without explicit human authority
prior_reviewed_head:
finding_ledger_digest:
open_blocker_ids:
deferred_note_ids:
autonomous_repairs_remaining:
stop_reason:                 # empty | decision_required | budget_exhausted | ...
```

Report unavailable evidence explicitly. Do not claim acceptance, CI success, merge eligibility, provider effects, or cost measurements that were not observed. Do not treat deferred notes as open blockers, and do not claim exact-head review acceptance from an unbound aggregate label.

## Staleness and Conflict Rules

- Chat summaries and generated capsules expire when `main`, a PR head, CI, review, or active documents change.
- Branch-local documents do not overwrite accepted-main facts before merge.
- When documents disagree, stop and reconcile the smallest canonical owner instead of creating another summary document.
- When code and prose disagree, treat the discrepancy as a defect; do not silently choose the more convenient claim.
- Do not advance a downstream packet while its named prerequisite is unaccepted.

## Documentation Discipline

Prefer complete, accurate, canonical, low-duplication documentation, then make it as short as those qualities permit.

One fact should have one full owner. Other entrypoints should link to that owner instead of copying its contract. Replace stale status rather than appending history; Git already preserves history. Add a document only when no existing canonical owner can hold the information without mixing responsibilities.
