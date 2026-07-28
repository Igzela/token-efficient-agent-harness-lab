# Current Status

Last updated: 2026-07-28.

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
- Context-capsule Phase 1 is accepted through PR #302 and Phase 2 is accepted through PR #306 (squash merge `3cc38e3158d71068abf03f445657f8bce4d485e3`): `START_HERE.md` is the canonical session entry, `scripts/project_context.py` generates a fail-closed transport view, CI publishes a short-lived exact-head capsule, and repository-controlled prompts inject a fresh validated capsule. The capsule remains non-authoritative.
- CI execution discipline is accepted through PR #310 and PR #311: changing Draft heads use non-canonical fast feedback; Ready heads use one canonical `tests` workflow with accepted-base documentation-only classification or the complete matrix; and Rust source lanes use pinned `sccache` only as a non-authoritative compiler cache. Documentation-only CI proves the exact prose diff and targeted guards, not unrelated runtime behavior.
- PR #313 is merged as `ca5ce1023664c58be8d15d681a80f262fb2be70b` after a green final PR exact-head matrix. It repairs push-event context-capsule generation without changing the seven-source-job matrix or PR exact-head proof. Its post-merge push workflow result remains an observed-but-not-yet-bound fact for this document until the exact run is retrieved.
- Verified Delivery Economics (VDE) is adopted as a provider-free architecture and routing contract: hard gates precede economic metrics; success remains layered; realized facts remain separate from forecasts; value bases are typed; Pareto comparison precedes scalar display; and initial persistence is artifact-first through existing owners. No runtime, schema, database table, Level-1 `MetricVector`, evaluator, store, budget, or adoption authority is added by this documentation decision.
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

These surfaces are not accepted truth and must not be merged independently when their dependency is unresolved:

| PR | Purpose | Current status |
|---|---|---|
| #225 | Presentation-only Dashboard work | Independent and last |

PR #297 and #298 are closed without merge as superseded by accepted PR #299. PR #303 is closed without merge as superseded by accepted PostgreSQL ordering repair PR #304. PR #301, PR #306, PR #308, PR #310, and PR #311 are merged and accepted. PR #313 is merged; its final PR exact-head matrix is green, while its post-merge push run remains to be bound before using it as accepted-main CI evidence.

No live provider request, live managed acceptance, live RWE baseline, accepted success probability, or realized VDE result is established by these open or recently merged surfaces.

## Current Product Verdict

Product Golden Path authority is accepted through PR #299; the live managed task remains default-off and `AUTHORIZATION_REQUIRED`. No current live-task authorization is recorded.

Fixture evidence proves the existing product sequence:

```text
intake → worktree/source binding → executable graph → scheduler lease
→ bounded executor → verification → artifact → approval
→ separate output confirmation → acp/* Draft PR → terminal evidence
```

The remaining product proof is one tightly bounded live managed coding task under the accepted authority decision, authenticated non-fixture principal, parent-only provider credential, one-use spend authorization, unchanged target `main`, Draft-PR-only output, and exact terminal evidence. PR #301 and PR #306 are accepted. Before any live Golden Path task, the external authorization manifest in `docs/NEXT_DECISION.md` must be current and persisted; context-capsule automation and the VDE decision contract do not satisfy that authority gate.

The residual technical risks remain explicit:

1. Codex internal retries are not wire-labeled with a trustworthy retry identity.
2. Product-enforced loopback-only network confinement is not proved under the current unprivileged host profile.
3. User/PID namespace support is host-dependent and may fail closed.
4. Live operator credential, risk acknowledgement, and spend authorization are not repository defaults.

Therefore live acceptance is not blocked only by credential presence.

## Capability Status

| Stage | State | Entry requirement |
|---|---|---|
| Golden Path residual seal | `AUTHORIZATION_REQUIRED` | Supply the current exact live-authority manifest and separately authorize one bounded live managed task |
| Context capsule automation | `COMPLETE` | PR #306 merged with exact-head publication and fresh session-start injection under the existing `START_HERE.md` automation boundary; PR #313 repairs push-event terminal validation pending separately bound post-merge evidence |
| VDE decision and measurement contract | `COMPLETE` | Provider-free architecture/routing contract only; no accepted live measurement or implementation artifact exists |
| First Real Workload Evidence | `BLOCKED_PREREQUISITE` | Accepted Golden Path terminal evidence, frozen real economic corpus/protocol, and separately authorized RWE spend envelope |
| Architecture Convergence AC1–AC7 | `BLOCKED_PREREQUISITE` | Frozen and independently accepted pre-convergence RWE baseline |
| Same-corpus RWE replay | `BLOCKED_PREREQUISITE` | Architecture Convergence complete |
| Level-2 GO/NO-GO | `BLOCKED_PREREQUISITE` | Comparable pre/post-convergence layered-success, reliability, lifecycle-cost, VDE/Pareto, and maintenance evidence |
| Level-2 generational controller | `BLOCKED_PREREQUISITE` | Explicit evidence-backed GO decision |
| Meta Improver experiment | `BLOCKED_PREREQUISITE` | Accepted Level-2 plus a separately authorized unseen-task experiment |
| Dashboard #225 | Deferred | Handle last; presentation cannot substitute for runtime proof |

## Project Objective

The repository's single first-order objective is:

> Under non-negotiable quality, safety, traceability, compatibility, and rollback constraints, continuously improve the amount of verifiable and reusable task delivery obtained per unit of total lifecycle cost.

Token reduction alone is not success. Lower cost is valid only when the compared runs meet the same accepted quality and safety gates.

VDE formalizes this objective as a read-only evidence projection. It preserves:

```text
verified_success
maintainer_accepted_success
delivered_success
```

A failed hard gate is ineligible rather than a low score. Different task-value bases are not implicitly added. General Lifecycle Cost of Accepted Pass is cumulative realized cost until delivered success or the frozen stop rule; a simple cost/probability ratio is valid only under explicitly proved fixed independent-attempt assumptions.

Lifecycle cost includes:

- provider requests, tokens, monetary cost or explicit cost unavailability, latency, and infrastructure;
- Agent sessions, review cycles, material rework, CI runs/compute time, and repair iterations;
- migrations, compatibility adapters, authority boundaries touched, rollback complexity, and external dependencies;
- long-term maintenance surface, failure recovery, state contamination risk, and observed reuse;
- separately labeled forecast maintenance and expected reuse scenarios, which cannot establish realized improvement.

These engineering-cost dimensions are evidence for RWE replay and Level-2 decisions. They do not create a second runtime budget, evaluator, store, or adoption owner.

## Confirmed Integration Gaps

1. No accepted live managed coding-executor E2E exists.
2. No accepted live RWE baseline exists.
3. No frozen real economic corpus, reviewer protocol, repetition grid, or accepted VDE observation exists.
4. Architecture Convergence cannot begin before that baseline is frozen.
5. Context capsule Phase 2 is accepted; PR #313 is merged, but its exact post-merge push run still needs to be bound before accepted-main CI is claimed for that commit.
6. No automatic multi-generation parent-selection loop is implemented.
7. No demonstrated cross-task continuous-learning or Meta Improver result exists.
8. Open PR claims remain proposals until their final heads are independently accepted.

## Supporting Programs

- **PE-5 Release Provenance:** implemented; product/evolution runtime gains no release authority.
- **PE-6 Fault Injection and Recovery Drills:** implemented for disposable recovery evidence; no production bypass authority.
- **Post-R7 wire/type governance:** implemented; `scripts/check_wire_codegen_drift.sh` remains a required guard.

## Active Tracks

- Provider-free Golden Path authority: PR #299 merged and accepted at schema v33; PR #300 merged and accepted at schema v34.
- Product Golden Path preflight: PR #308 is merged and accepted at schema v35; one persisted ProductTask preparation receipt plus local synchronization only; changed roots or unproved physical outcomes require reconciliation, and the live Golden Path remains `AUTHORIZATION_REQUIRED`.
- Context governance: PR #302 and PR #306 are merged; on-demand fail-closed capsule generation, exact-head workflow publication, and fresh session injection are accepted transport behavior, not authority. PR #313 repairs push-event validation without importing authority.
- CI governance: PR #310 and PR #311 are merged; Draft fast feedback remains non-canonical, Ready exact-head `tests` remains the sole CI authority, and compiler cache state cannot become acceptance evidence. PR #313 is merged pending separately bound post-merge run evidence.
- VDE governance: provider-free decision and measurement rules are documented; artifact schemas, real corpus, live observations, persistence automation, and Dashboard projection remain future gated work.
- Observation adaptation: PR #301 is merged and accepted; observation-only and restacked onto accepted main.
- Live Golden Path is blocked at `AUTHORIZATION_REQUIRED`; live RWE, Architecture Convergence, Level-2, and Meta remain blocked by their named prerequisites.

## Open Work Coordination

PRs #297/#298 are closed without merge as superseded by merged PR #299. PR #300, PR #301, PR #306, PR #308, PR #310, and PR #311 are merged and accepted. PR #313 is merged with green final PR evidence but pending separately bound post-merge push evidence. PR #301 is observation-only and did not introduce a second budget, proxy, credential, store, or authorization owner; PR #306 is non-authoritative context transport only; PR #308 is provider-free workspace-preparation and recovery hardening only; PR #310/#311/#313 change CI execution discipline or terminal validation only. The sole next product route is the live Golden Path external authorization gate. Provider-free VDE contract preparation may continue only when it does not start live RWE, add a second owner, or change that route. PR #225 remains presentation-only and last.

All active branches must refresh this main documentation convergence before final merge and must not overwrite it with stale branch-local status text.

## Safety Boundary

Default-off execution; no provider call in CI; no target-default-branch write; no auto-merge; no release or deployment authority; no reusable credential in a child; no secret, raw prompt, raw output, transcript, private path, fixture-only result, forecast value, or scalar VDE index may become durable production-adoption authority.
