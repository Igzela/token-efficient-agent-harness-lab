# Current Status

Last updated: 2026-08-01.

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
- PR #318 is merged as `70700f9bf7eef25c4bdf86be7fc0a78686f0927a`: PostgreSQL and cutover lanes no longer repeat unrelated owners' checks, `cargo-audit` is pinned/cached, and Docker uses independently scoped Buildx/Bake layer caches. Its exact-head and post-merge canonical CI passed; caches remain non-authoritative.
- PR #319 is merged and accepted: provider-free `rwe_economic_protocol.v1` plus four artifact-first VDE contracts are canonical-hash-bound and fail closed on fixture/placeholder real-protocol inputs, sensitive raw fields, protocol drift, incomplete repetitions/costs, and failed comparison gates. It adds no live, spend, reviewer, output, adoption, or persistence authority.
- PR #320 is merged and accepted as `630895b91703eb9e9caada24690a08900c0d6991`: managed Codex runtime-profile admission, capability-probe identity, poison-safe parallel fixtures, bounded `local_folder` source/output behavior, and Dashboard local-folder intake are provider-free and target-free. No migration was added.
- PR #321 is merged and accepted as `542a5a453308f8a84e540f48767a80a9e58bf99d`: provider-free managed DeepSeek dual-protocol support reuses the existing OpenAI-compatible and Anthropic-compatible provider clients under one ProductTask-bound authority, with deterministic mock coverage only. No migration, provider request, or target effect was added.
- PR #322 is merged and accepted as `13f725f949684d179593a6559d8600a5b5d47edf`: the managed DeepSeek executor is wired into the production ProductTask scheduler as Pro planning, Flash bounded implementation, deterministic verification, and Pro review. The repair remained provider-free and added no target effect.
- PR #323 is merged and accepted as `8d9f8dc47ab458fa01873571fbc7b60ebaf211f0` from exact head `3ca951dd8a645e1a7f2b6cc800d9fad54c010fd7`: provider-free delegated autonomous Golden Path at schema v36 (immutable proposal/final manifests, authenticated bounded delegation, separated manifest/spend and artifact/output authority, durable pre-send provider-request journaling, Draft-PR-only output reconciliation, terminal cleanup, and SQLite/PostgreSQL restart parity). Canonical exact-head CI run `30607238397` was successful; Theo GO accepted the exact head before squash merge.
- PR #325 is merged and accepted as `0da5c6c785004784d9ffa3b20e0068f4bac6be71` from exact head `7ecffd5a30426dd1f26ab4d46a8f2a36e7594568`: live-observed repair for live-seal budget reservation release and Draft PR terminal CAS rebind. Canonical exact-head runs `30613256286` and `30613256266` were successful; post-merge `push: main` run `30636441727` passed all jobs and bound the context-capsule artifact to the merge SHA.
- PR #326 is merged and accepted as `6be38ea561286e214bf0b5096bccb4891ea6d8f5` from exact head `ac50c3860ad1dccd5ef72a166cd609688c253a98`: dormant-automation safety cleanup. It deletes the stale unattended loops `scripts/auto_adapt_loop.sh` and `scripts/auto_ga_loop.sh` (no workflow, script, cron, test, or document caller; only `--dangerously-skip-permissions` and unbound `gh run list --limit 1` CI-judgment surfaces) and adds a sixth fail-closed check to the existing `tools/check_security_baseline.py` security-baseline owner that rejects those patterns in repository-controlled workflows and automation scripts, with an explicit minimal reviewable allowlist limited to the detector and its fixture tests. No schema migration, provider effect, or target effect.
- PR #328 is merged and accepted as `7befbd25cb8cd8a65fa5e1894e6709243a1b4216` from exact head `8587b6e236009668fe4333ce709b36060147abe3`: dormant parity-surface cleanup. It deletes the parity/cutover-era `engine/src/doc_generator.rs` and `engine/src/ecosystem/` (benchmark, community_profiles, dashboard, tool_adapter) surfaces plus their five isolated engine tests and public `lib.rs` exports. None had a production caller (composition root, HTTP routes, CLI binaries, scheduler/executor, store owner, workflows, scripts); canonical replacement owners are `efficiency_benchmark_runtime` + scorecard/replay owners, the `dashboard/` app + `sdk/`, and `tool_policy_executor` + the workflow tool registry. The `ecosystem::tool_adapter` `execute_tool` stub (empty `{}` output for any registered tool) was deleted, not wired. No migration, provider, or target effect.
- PR #331 is merged and accepted as `56dd3c25795abdd6a3dd121050456ec453c1e6e6` from exact head `d5b5c9fa10fab06a2ad90460dc1f7ce5963a4d15`: dormant plugin-surface cleanup. It deletes the cutover-era in-memory plugin system and registry (`engine/src/infrastructure/plugin_system.rs`, `plugin_registry.rs`) plus their two isolated engine tests; they had no loader, signing, sandbox, lifecycle, API, or production caller, and the `official` trust level relied on an `// empty = unrestricted` fallback. A seventh fail-closed check (`check_removed_plugin_surface_guard`) added to the sole security-baseline owner rejects `TRUST_LEVEL_OFFICIAL/VERIFIED/COMMUNITY`, `PLUGIN_SYSTEM_SCHEMA_VERSION`, `ALL_KNOWN_PERMISSIONS`, and `empty = unrestricted` tokens in `engine/src/`. No migration, provider, or target effect.
- PR #332 is merged and accepted as `eef63c1760cde6f256de62884dfa1a9ebe751307` from exact head `af2548666340d43b2f85b01f5b3211f224ea37a0`: dormant harness-symbol convergence. It deletes the caller-less `engine/src/harness/` modules `kernel`, `skills`, `routing_experiments`, `orchestrator`, and the whole `model_profiles/` directory, trims `model_gateway.rs` to the adapter surface consumed by `provider::ProviderAdapter` (`ModelTier`, `ModelResponse`, `ModelProvider`; `StubModelProvider` now `#[cfg(test)]`), removes the directory-level `#![allow(dead_code)]` from `harness/mod.rs` (module list reduced to `advisor` + `model_gateway`), and gates the test-only `StubAdvisorProvider` behind `#[cfg(test)]`. `advisor` remains for `dispatch_engine`; `harness_evolution*.rs` remains the canonical Level-1 fixture laboratory. No migration, provider, or target effect.
- PR #334 is merged and accepted as `ac0769b58d1b20411ed1009c114df715e589af6a`: exact-head correction of the PR #326 record in this file (the sole stale copy of the wrong SHA `ac50c386425d9e07d01be985cebd91fd4e09a0b2` was replaced with the verified exact head `ac50c3860ad1dccd5ef72a166cd609688c253a98`). Canonical docs-only CI `30677801503` was green.
- PR #335 is merged and accepted as `bd096e34986f60824908a72ab97df0e8a31eb461`: reference-surface boundary convergence. It deletes the caller-less `engine/src/event_source/` (event store, projection store, task queue, task records, project board, validators) and the `engine/src/errors.rs` shared error module (all consumers lived inside `event_source`), leaving `event_schema.rs` as the independent wire/schema contract. A pre-existing time-bomb was repaired at root cause: the pg RWE fixture authorization used a hardcoded `2026-08-01T00:00:00Z` expiry rejected as already-expired by `rwe_authority.rs` against the real clock after that instant; fixture expiry is now relative to `Utc::now()`. No migration, provider, or target effect.
- PR #336 is merged and accepted as `17723bb66a1274498c32aef0f6cac85ad339efea` from exact head `6fdb00dd4b4710b795be3181aed46e3a6ee31d9e`: dormant-surface governance gate. The sole security-baseline owner gains a dormant-surface heuristic gate (module-level dead-code blankets, lib.rs module islands, self-described placeholder modules, no-op executors, conflicting sole-owner claims; exceptions require a classification entry with owner, reason, review condition, and expiry/recheck) and its automation guard becomes semantic (rejects `--limit 1` CI judgment, `gh run watch` chained to an unbound list, and unbound `gh run watch`; explicit run ids, head-bound queries, and informational bounded queries pass); the removed-plugin guard becomes a composite legacy fingerprint (resurrected paths and `empty = unrestricted` always flag; two or more legacy tokens flag a revival; generic `official`/`verified` identifiers stay legal). Baseline `engine/src/` dead code was removed (module-level allows, `V25_DDL`, stale-lease SQL, four dead pg schema validators, dead scheduler/helper fields) with pg-parity constants retained under targeted reasoned allows; test-only helpers moved to `#[cfg(test)]`; `engine/Cargo.toml` declares `publish = false` (binary archives and provenance, never crates.io). Progress output is sequential `[1/8]..[8/8]`. The recorded run `30680937667` had `source_matrix: success`, `terminal_context_capsule: failure`, and `overall_canonical_run: failure`; the terminal job refused the already-merged PR and therefore did not publish a successful capsule. No migration, provider, or target effect.
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

One owner-authorized live attempt was made and recorded truthfully under `PE7-PRODUCT-GOLDEN-PATH-DEEPSEEK-LIVE-SEAL-1`: a real `Pro → Flash → deterministic verify → Pro` workflow with target `Igzela/alters-lab` main unchanged, one unmerged `acp/*` Draft PR (alters-lab PR #3), realized cost approximately `$0.0016`, expired one-use spend, closed attempt lease, cleaned workspace, and a realized workflow sample labeled `INSUFFICIENT_REPETITIONS`. It is not a clean accepted seal: the provider-stage binary came from a dirty tree and did not equal an accepted commit or the final #325 head, and terminal closeout used later local CAS-rebind code. PR #325 is the accepted live-observed repair. No live managed acceptance, live RWE baseline, accepted success probability, or realized VDE result is established.

## Current Product Verdict

Product Golden Path authority includes the provider-free production runner through accepted PR #322, the provider-free delegated autonomous path through accepted PR #323 at schema v36, and the live-observed budget/CAS repair through accepted PR #325. Live execution remains default-off. The sole active frontier is `PE7-PRODUCT-GOLDEN-PATH-DEEPSEEK-LIVE-SEAL-1`, which is `DECISION_REQUIRED` (nearest existing fail-closed equivalent of `LIVE_OBSERVED_RESEAL_REQUIRED`) pending a new explicit one-use owner authorization for one clean exact-main live reseal. No provider request or target effect is a default.

Fixture evidence proves the existing product sequence:

```text
intake → worktree/source binding → executable graph → scheduler lease
→ bounded executor → verification → artifact → approval
→ separate output confirmation → acp/* Draft PR → terminal evidence
```

The remaining product proof is one clean exact-main live reseal under the accepted authority decision, authenticated non-fixture principal, parent-only provider credential, fresh one-use spend authorization, unchanged target `main`, Draft-PR-only output, recorded exact source tree and binary SHA, one ProductTask from intake through terminal closure, one bounded unmerged `acp/*` Draft PR, and exact terminal evidence. The first two observed live attempts remain `LIVE_OBSERVED_NOT_ACCEPTED`; the later attempt's restart recovery used direct SQLite authority mutation and a manual ProductTask version rollback. The owner has now explicitly authorized up to three separate zero-retry repair/reseal attempts in `docs/NEXT_DECISION.md`; no attempt may start before the canonical repair packets and their evidence are accepted on main. Before any live task, the external authorization manifest in `docs/NEXT_DECISION.md` must be current and persisted; context-capsule automation and the VDE decision contract do not satisfy that authority gate.

Residual risks remain explicit:

1. Codex internal retries are not wire-labeled with a trustworthy retry identity.
2. Product-enforced loopback-only network confinement is not proved under the current unprivileged host profile.
3. User/PID namespace support is host-dependent and may fail closed.
4. Live operator credential, risk acknowledgement, and spend authorization are not repository defaults.

Therefore live acceptance is not blocked only by credential presence.

## Capability Status

| Stage | State | Entry requirement |
|---|---|---|
| Provider-free RWE/VDE artifact contracts | `COMPLETE` | PR #319 freezes hash-bound schemas and fail-closed validation without provider/runtime authority |
| Managed-coding boundary generalization | `COMPLETE` | Packet `PE7-MANAGED-CODING-BOUNDARY-GENERALIZATION-1`; PR #320 exact-head/full CI, independent review, squash merge, and merge SHA `630895b9…` |
| DeepSeek dual-protocol managed coding | `COMPLETE` | Packet `PE7-DEEPSEEK-DUAL-PROTOCOL-MANAGED-CODING-1`; PR #321 merged at `542a5a45…` with provider-free deterministic mocks only |
| DeepSeek live-runner wiring repair | `COMPLETE` | Packet `PE7-DEEPSEEK-LIVE-RUNNER-WIRING-REPAIR-1`; PR #322 merged at `13f725f9…` with provider-free scheduler-path proof |
| Delegated autonomous Golden Path | `COMPLETE` | Packet `PE7-DELEGATED-AUTONOMOUS-GOLDEN-PATH-1`; PR #323 merged as `8d9f8dc4…` after exact-head CI `30607238397` and Theo GO on `3ca951dd…` |
| Golden Path live residual seal | `DECISION_REQUIRED` | PR #323 accepted, PR #325 accepted as the live-observed repair; one clean exact-main live reseal remains and requires a new explicit one-use owner authorization |
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

1. No accepted live managed coding-executor E2E exists; PR #323 establishes provider-free production-path proof only, and PR #325 is the accepted live-observed budget/CAS repair. The first authorized live attempt was observed but did not seal cleanly; one clean exact-main live reseal remains and requires a new explicit one-use owner authorization.
2. No accepted live RWE baseline exists.
3. No frozen operator-supplied real economic corpus, reviewer protocol instance, repetition grid, or accepted VDE observation exists. Provider-free schema work does not satisfy this gap.
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
- Product Golden Path preflight: PR #308 is merged and accepted at schema v35; PR #320–#322 complete managed-coding boundary, DeepSeek protocol, and production runner wiring; PR #323 completes the provider-free delegated autonomous path at schema v36; PR #325 is the accepted live-observed budget/CAS repair. The active frontier is the clean exact-main live reseal, which must not start without a new explicit one-use owner authorization.
- Context/CI governance: PR #302, PR #306, PR #310, PR #311, PR #313, PR #315, and PR #318 are merged and accepted; transport, fast feedback, and cache state remain non-authoritative.
- VDE governance: the provider-free decision contract and artifact-schema validation are complete through PR #319; a real corpus, live observations, persistence automation, and Dashboard projection remain gated future work.
- Observation adaptation: PR #301 is merged and accepted; observation-only and restacked onto accepted main.
- Live RWE, Architecture Convergence, Level-2, and Meta remain blocked by their named prerequisites.

## Open Work Coordination

The active frontier is the clean exact-main Golden Path live reseal after accepted PR #323 and PR #325. It does not make RWE, Architecture Convergence, Level-2, Meta, or Dashboard work eligible. PR #225 remains presentation-only and last.

All active branches must refresh this main documentation convergence before final merge and must not overwrite it with stale branch-local status text.

## Safety Boundary

Default-off execution; no provider call in CI; no target-default-branch write; no auto-merge; no release or deployment authority; no reusable credential in a child; no secret, raw prompt, raw output, transcript, private path, fixture-only result, forecast value, or scalar VDE index may become durable production-adoption authority.
