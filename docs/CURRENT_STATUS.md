# Current Status

Last updated: 2026-07-29.

## Verified Repository State

This document separates three states that must not be conflated:

1. **Merged and accepted truth** — code on `main` that passed exact-head/full CI, independent review, merge, and documentation closeout.
2. **Open review surfaces** — proposed code on PR branches. It may be useful or locally green, but it is not authoritative until the final unchanged head passes CI and independent review and is merged.
3. **Blocked or deferred work** — design or implementation that remains ineligible because an earlier evidence or authority gate is incomplete.

A new PR head invalidates earlier CI and review conclusions for that PR.

- Repository: `Igzela/token-efficient-agent-harness-lab`.
- The accepted runtime baseline is merged through PR #300 at schema v34, including provider-free RWE corpus authority, store-owned spend, managed-acceptance authority, one-use spend/attempt leases, durable transition receipts, and PostgreSQL restart repair.
- PR #308 is merged and accepted at schema v35: a ProductTask-owned workspace-preparation receipt for provider-free local worktree recovery. It pins one planned local path before a physical effect, remains under `LocalProductStore`/ProductTask ownership, and does not grant a provider call, live task, credential, budget, scheduler, target-output, or merge authority.
- PR #301 is merged and accepted: CC Switch observation-only adaptation for protocol usage parsing, stream aggregation, model normalization, pricing estimates, and endpoint classification, under `engine/src/execution_usage/`. No authority was imported.
- Context-capsule Phase 1 is accepted through PR #302 and Phase 2 through PR #306: `START_HERE.md` is the canonical session entry, `scripts/project_context.py` generates a fail-closed transport view, CI publishes a short-lived exact-head capsule, and repository-controlled prompts inject a fresh validated capsule. The capsule remains non-authoritative.
- CI execution discipline is accepted through PR #310, PR #311, and PR #315: changing heads are action-enforced Drafts with non-canonical fast feedback; one `ready_for_review` transition triggers canonical exact-head CI; normal prose-only `main` pushes use the accepted-before classifier over the complete `before...after` range; uncertain pushes and explicit dispatches fail closed to the full matrix; and Rust source lanes use pinned `sccache` only as a non-authoritative compiler cache. PR #315 merged as `faac83ac7bcdf60460a966f7483b7e719d4fc1a1`; post-merge `push: main` run `30421284939` passed all seven source jobs and terminal context-capsule artifact `8712219360` bound to that SHA.
- PR #313 is merged as `ca5ce1023664c58be8d15d681a80f262fb2be70b`. Its final PR exact-head matrix passed, and post-merge `push: main` run `30381836225` completed successfully with all seven source jobs plus terminal context-capsule artifact `8697748363` bound to the same SHA.
- Verified Delivery Economics (VDE) is adopted as a provider-free architecture and routing contract. Durable semantics live in `docs/ARCHITECTURE_BOOK.md`; execution order and gates live in `docs/NEXT_DECISION.md`. No runtime, schema, database table, Level-1 `MetricVector`, evaluator, store, budget, or adoption authority is added by this documentation decision.
- Rust `engine/` and `LocalProductStore` remain the sole authorities for workflow state, scheduling, leases, retries, budgets, approvals, evidence, output reconciliation, audit, and persistence.
- SQLite is the default store; PostgreSQL is the supported parity backend.
- ProductTask remains the sole product budget owner.
- `execution_usage_event.v1` is the normalized post-call usage-evidence contract. Gateway evidence is primary for mediated Codex; CLI/session records are corroborating only.
- Codex API-key mediation is classified `mediation_hardened_partial`, not full admission.
- Claude Code managed admission remains fail-closed because provider-independent worktree-only confinement is unproved.
- OpenCode real-binary admission remains deferred because no admitted upstream artifact/checksum exists.
- Harness Evolution Level-1 is a default-off fixture laboratory. It does not establish recursive self-improvement or production self-update, and its current evaluator/`MetricVector` is unchanged by VDE.
- No runtime owner can merge, release, deploy, modify protected branches, or adopt a candidate as the production Harness.

## Open Review Surfaces

| PR | Purpose | Current status |
|---|---|---|
| #225 | Presentation-only Dashboard work | Independent and last |

PR #297 and #298 are closed without merge as superseded by accepted PR #299. PR #303 is closed without merge as superseded by accepted PostgreSQL ordering repair PR #304. PR #301, PR #306, PR #308, PR #310, PR #311, PR #313, and PR #315 are merged and accepted.

No live provider request, live managed acceptance, live RWE baseline, accepted success probability, or realized VDE result is established.

## Current Product Verdict

Product Golden Path authority is accepted through PR #299; the live managed task remains default-off and `AUTHORIZATION_REQUIRED`. No current live-task authorization is recorded.

Fixture evidence proves the existing product sequence:

```text
intake → worktree/source binding → executable graph → scheduler lease
→ bounded executor → verification → artifact → approval
→ separate output confirmation → acp/* Draft PR → terminal evidence
```

The remaining product proof is one tightly bounded live managed coding task under the accepted authority decision, authenticated non-fixture principal, parent-only provider credential, one-use spend authorization, unchanged target `main`, Draft-PR-only output, and exact terminal evidence. Before any live task, the external authorization manifest in `docs/NEXT_DECISION.md` must be current and persisted; context-capsule automation and the VDE decision contract do not satisfy that authority gate.

Residual risks remain explicit:

1. Codex internal retries are not wire-labeled with a trustworthy retry identity.
2. Product-enforced loopback-only network confinement is not proved under the current unprivileged host profile.
3. User/PID namespace support is host-dependent and may fail closed.
4. Live operator credential, risk acknowledgement, and spend authorization are not repository defaults.

Therefore live acceptance is not blocked only by credential presence.

## Capability Status

| Stage | State | Entry requirement |
|---|---|---|
| Golden Path residual seal | `AUTHORIZATION_REQUIRED` | Supply the current exact live-authority manifest and separately authorize one bounded live managed task |
| Context capsule automation | `COMPLETE` | PR #306 provides publication/injection; PR #313 proves the repaired post-merge push terminal path on `ca5ce102…` |
| VDE decision and measurement contract | `COMPLETE` | Provider-free architecture/routing contract only; no accepted live measurement or implementation artifact exists |
| First Real Workload Evidence | `BLOCKED_PREREQUISITE` | Accepted Golden Path terminal evidence, frozen real economic corpus/protocol, and separately authorized RWE spend envelope |
| Architecture Convergence AC1–AC7 | `BLOCKED_PREREQUISITE` | Frozen and independently accepted pre-convergence RWE baseline |
| Same-corpus RWE replay | `BLOCKED_PREREQUISITE` | Architecture Convergence complete |
| Level-2 GO/NO-GO | `BLOCKED_PREREQUISITE` | Comparable pre/post-convergence layered-success, reliability, lifecycle-cost, VDE/Pareto, and maintenance evidence |
| Level-2 generational controller | `BLOCKED_PREREQUISITE` | Explicit evidence-backed GO decision |
| Meta Improver experiment | `BLOCKED_PREREQUISITE` | Accepted Level-2 plus a separately authorized unseen-task experiment |
| Dashboard #225 | Deferred | Handle last; presentation cannot substitute for runtime proof |

## Project Objective

The repository seeks verifiable and reusable task delivery per unit of total lifecycle cost, subject to hard quality, safety, traceability, compatibility, recovery, and rollback gates. `docs/ARCHITECTURE_BOOK.md` owns the full VDE semantics; this status page records only that the direction is adopted and that no live VDE observation or improvement claim exists yet.

## Confirmed Integration Gaps

1. No accepted live managed coding-executor E2E exists.
2. No accepted live RWE baseline exists.
3. No frozen real economic corpus, reviewer protocol, repetition grid, or accepted VDE observation exists.
4. Architecture Convergence cannot begin before that baseline is frozen.
5. No automatic multi-generation parent-selection loop is implemented.
6. No demonstrated cross-task continuous-learning or Meta Improver result exists.
7. Open PR claims remain proposals until their final heads are independently accepted.

## Supporting Programs

- **PE-5 Release Provenance:** implemented; product/evolution runtime gains no release authority.
- **PE-6 Fault Injection and Recovery Drills:** implemented for disposable recovery evidence; no production bypass authority.
- **Post-R7 wire/type governance:** implemented; `scripts/check_wire_codegen_drift.sh` remains a required guard.

## Active Tracks

- Provider-free Golden Path authority: PR #299 merged and accepted at schema v33; PR #300 merged and accepted at schema v34.
- Product Golden Path preflight: PR #308 is merged and accepted at schema v35; the live Golden Path remains `AUTHORIZATION_REQUIRED`.
- Context/CI governance: PR #302, PR #306, PR #310, PR #311, PR #313, and PR #315 are merged and accepted; transport, fast feedback, and cache state remain non-authoritative.
- VDE governance: the provider-free decision contract is documented; artifact schemas, real corpus, live observations, persistence automation, and Dashboard projection remain gated future work.
- Observation adaptation: PR #301 is merged and accepted; observation-only and restacked onto accepted main.
- Live RWE, Architecture Convergence, Level-2, and Meta remain blocked by their named prerequisites.

## Open Work Coordination

The sole next product route is the live Golden Path external authorization gate. Provider-free VDE contract preparation may continue only when it does not start live RWE, add a second owner, or change that route. PR #225 remains presentation-only and last.

All active branches must refresh this main documentation convergence before final merge and must not overwrite it with stale branch-local status text.

## Safety Boundary

Default-off execution; no provider call in CI; no target-default-branch write; no auto-merge; no release or deployment authority; no reusable credential in a child; no secret, raw prompt, raw output, transcript, private path, fixture-only result, forecast value, or scalar VDE index may become durable production-adoption authority.
