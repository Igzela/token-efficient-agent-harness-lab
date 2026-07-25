# Current Status

Last updated: 2026-07-25.

## Verified Repository State

This document separates three states that must not be conflated:

1. **Merged and accepted truth** — code on `main` that passed exact-head/full CI, independent review, merge, and documentation closeout.
2. **Open review surfaces** — proposed code on PR branches. It may be useful or locally green, but it is not authoritative until the final unchanged head passes CI and independent review and is merged.
3. **Blocked or deferred work** — design or implementation that remains ineligible because an earlier evidence or authority gate is incomplete.

A new PR head invalidates earlier CI and review conclusions for that PR.

- Repository: `Igzela/token-efficient-agent-harness-lab`.
- Accepted runtime baseline is merged through PR #296 plus its documentation seal.
- Rust `engine/` and `LocalProductStore` remain the sole authorities for workflow state, scheduling, leases, retries, budgets, approvals, evidence, output reconciliation, audit, and persistence.
- SQLite is the default store; PostgreSQL is the supported parity backend.
- ProductTask remains the sole product budget owner.
- `execution_usage_event.v1` is the normalized post-call usage-evidence contract. Gateway evidence is primary for mediated Codex; CLI/session records are corroborating only.
- Codex API-key mediation is classified `mediation_hardened_partial`, not full admission.
- Claude Code managed admission remains fail-closed because provider-independent worktree-only confinement is unproved.
- OpenCode real-binary admission remains deferred because no admitted upstream artifact/checksum exists.
- Harness Evolution Level-1 is a default-off fixture laboratory. It does not establish recursive self-improvement or production self-update.
- No runtime owner can merge, release, deploy, modify protected branches, or adopt a candidate as the production Harness.

## Open Review Surfaces

These surfaces are not accepted truth and must not be merged independently when their dependency is unresolved:

| PR | Purpose | Current status |
|---|---|---|
| #299 | Cumulative Codex residual-admission, risk acknowledgement, one-use spend, owner-derived preflight, and managed-attempt authority | Open; supersedes #297/#298; requires final independent review and exact-head/full CI on its final unchanged head |
| #300 | Provider-free first-RWE corpus, authorization, runner, and evidence preparation stacked on #299 | Open; must remain blocked until #299 authority is accepted; no live baseline exists |
| #301 | CC Switch observation-only adaptation for protocol usage parsing, stream aggregation, model normalization, pricing estimates, and endpoint classification | Open; observation scope only; does not close #299/#300 authority; requires token-bucket/identity corrections and final CI |
| #297/#298 | Earlier residual-admission and authority-decision slices | Superseded by #299; do not merge separately |
| #225 | Presentation-only Dashboard work | Independent and last |

No live provider request, live managed acceptance, or live RWE baseline is established by these open PRs.

## Current Product Verdict

Product Golden Path is default-off and `IN_PROGRESS`.

Fixture evidence proves the existing product sequence:

```text
intake → worktree/source binding → executable graph → scheduler lease
→ bounded executor → verification → artifact → approval
→ separate output confirmation → acp/* Draft PR → terminal evidence
```

The remaining product proof is one tightly bounded live managed coding task under an accepted authority decision, authenticated non-fixture principal, parent-only provider credential, one-use spend authorization, unchanged target `main`, Draft-PR-only output, and exact terminal evidence.

The residual technical risks remain explicit:

1. Codex internal retries are not wire-labeled with a trustworthy retry identity.
2. Product-enforced loopback-only network confinement is not proved under the current unprivileged host profile.
3. User/PID namespace support is host-dependent and may fail closed.
4. Live operator credential, risk acknowledgement, and spend authorization are not repository defaults.

Therefore live acceptance is not blocked only by credential presence.

## Capability Status

| Stage | State | Entry requirement |
|---|---|---|
| Golden Path residual seal | `IN_PROGRESS` | Accept the final provider-free authority surface, then complete one bounded live managed task |
| First Real Workload Evidence | `BLOCKED_PREREQUISITE` | Accepted Golden Path terminal evidence plus a separately authorized RWE spend envelope |
| Architecture Convergence AC1–AC7 | `BLOCKED_PREREQUISITE` | Frozen and independently accepted pre-convergence RWE baseline |
| Same-corpus RWE replay | `BLOCKED_PREREQUISITE` | Architecture Convergence complete |
| Level-2 GO/NO-GO | `BLOCKED_PREREQUISITE` | Comparable pre/post-convergence evidence and lifecycle-cost evidence |
| Level-2 generational controller | `BLOCKED_PREREQUISITE` | Explicit evidence-backed GO decision |
| Meta Improver experiment | `BLOCKED_PREREQUISITE` | Accepted Level-2 plus a separately authorized unseen-task experiment |
| Dashboard #225 | Deferred | Handle last; presentation cannot substitute for runtime proof |

## Project Objective

The repository's single first-order objective is:

> Under non-negotiable quality, safety, traceability, compatibility, and rollback constraints, continuously improve the amount of verifiable and reusable task delivery obtained per unit of total lifecycle cost.

Token reduction alone is not success. Lower cost is valid only when the compared runs meet the same accepted quality and safety gates.

Lifecycle cost includes:

- provider requests, tokens, monetary cost or explicit cost unavailability, latency, and infrastructure;
- Agent sessions, review cycles, CI runs/compute time, and repair iterations;
- migrations, compatibility adapters, authority boundaries touched, rollback complexity, and external dependencies;
- long-term maintenance surface, failure recovery, state contamination risk, and expected reuse.

These engineering-cost dimensions are evidence for RWE replay and Level-2 decisions. They do not create a second runtime budget owner.

## Confirmed Integration Gaps

1. No accepted live managed coding-executor E2E exists.
2. No accepted live RWE baseline exists.
3. Architecture Convergence cannot begin before that baseline is frozen.
4. No automatic multi-generation parent-selection loop is implemented.
5. No demonstrated cross-task continuous-learning or Meta Improver result exists.
6. Open PR claims remain proposals until their final heads are independently accepted.

## Supporting Programs

- **PE-5 Release Provenance:** implemented; product/evolution runtime gains no release authority.
- **PE-6 Fault Injection and Recovery Drills:** implemented for disposable recovery evidence; no production bypass authority.
- **Post-R7 wire/type governance:** implemented; `scripts/check_wire_codegen_drift.sh` remains a required guard.

## Active Tracks

- Provider-free Golden Path authority: #299, open and under final independent review.
- Provider-free RWE preparation: #300, stacked and blocked on accepted #299 authority.
- Observation adaptation: #301, open and limited to evidence parsing/normalization.
- Live Golden Path, live RWE, Architecture Convergence, Level-2, and Meta remain blocked by their named prerequisites.

## Open Work Coordination

PRs #297/#298 are superseded by #299 and must not merge separately. #300 must be updated onto the final accepted #299 semantics. #301 may remain an independent observation-only layer but must not introduce a second budget, proxy, credential, store, or authorization owner. PR #225 remains presentation-only and last.

All active branches must refresh this main documentation convergence before final merge and must not overwrite it with stale branch-local status text.

## Safety Boundary

Default-off execution; no provider call in CI; no target-default-branch write; no auto-merge; no release or deployment authority; no reusable credential in a child; no secret, raw prompt, raw output, transcript, private path, or fixture-only result may become durable acceptance evidence.
