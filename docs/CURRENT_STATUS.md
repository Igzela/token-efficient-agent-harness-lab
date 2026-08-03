# Current Status

Last updated: 2026-08-03.

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
- PR #336 is merged as `17723bb66a1274498c32aef0f6cac85ad339efea` from exact head `6fdb00dd4b4710b795be3181aed46e3a6ee31d9e`, but its canonical acceptance is incomplete: dormant-surface governance code landed, while the recorded run `30680937667` had `source_matrix: success`, `terminal_context_capsule: failure`, and `overall_canonical_run: failure`; `failure_reason: PR merged before terminal required job`. Its material governance findings were repaired and accepted through PRs #339 and #340; no successful capsule is retroactively claimed for `30680937667`.
- PR #339 is merged and accepted as `a7ddf1cd588c71d553bf4d0644a6dabdd55e5ea` from exact head `240e80065b9632b75ce7c733b63da59fb14c0680`. It corrected the #336 evidence and repaired the sole CI/security-baseline governance owner, including the requirement that terminal `context-capsule` success is a merge gate. Its exact-head canonical run `30695452770` passed all source jobs and terminal capsule job `91358472963`, publishing artifact `context-capsule-30695452770-1-240e80065b9632b75ce7c733b63da59fb14c0680`.
- PR #340 is merged and accepted as `dc1d839316771145a0b1c079bfbc66b30c0ab61a` from exact head `4aed0af5227c53efeb711c8123d922c2e3133cea`. It repaired accepted-main `workflow_dispatch` capsule validation without weakening pull-request exact-head validation. The first post-merge dispatch `30695885514` correctly remains a failed canonical run because its terminal capsule still required an unavailable PR head; replacement post-merge full accepted-main run `30696953015` passed all source jobs and terminal capsule job `91362363217`, publishing non-expired artifact `context-capsule-30696953015-1-dc1d839316771145a0b1c079bfbc66b30c0ab61a`.
- PR #342 is merged and accepted as `e1e08ddcb745b02892f099b9de1436c99c25d533` from exact head `666cabeab31c14c77389646edc140c2d8ae7eb86`. It completes Packet B: pre-effect missing GitHub credentials leave the existing output operation recoverable without decreasing ProductTask versions or rebinding the original operation/request identity; restart recovery reissues canonical managed identities/scopes through the existing owner; reviewer/output scopes remain minimal; duplicate, stale, foreign, late, concurrent, outcome-unknown, target-drift, artifact/approval, lease/spend/delegation, and cleanup boundaries fail closed. Canonical exact-head run `30710854561` passed the complete source matrix, PostgreSQL parity/PE-6 owner evidence, and terminal `context-capsule` (`91399051309`). The exact-head independent delta review for `4b5f8d42..666cabea` was PASS with no unresolved objections; no provider call or target effect occurred.
- PR #346 is merged and accepted as `adcb87b4a3ece961a46455117ae4323b4f54c2fa` from exact head `64d45f5e55f4393737c26b26dbd66976b8145d5d` (base `a85f9c1db776f17c30de871105d24941cade2ce6`). It is a Packet B repair that binds managed identity mutation to the canonical bootstrap: reviewer/output-operator identity creation and mutation require the store-owned canonical bootstrap principal and exact local tenant; least-scope validation is retained and foreign-tenant reserved bootstrap keys are rejected; SQLite/PG-parity and HTTP restart/reissuance fixtures initialize the canonical owner through the store/API contract; the canonical Rust test lane is serialized because HTTP tests use process-wide environment gates. The key-authority handlers now run synchronous store operations through `tokio::task::spawn_blocking`, resolving a PostgreSQL "Cannot start a runtime from within a runtime" panic in async axum handlers, and the PostgreSQL ProductTask tests gain full `ACP_PRODUCT_WORKSPACE_ROOT`/`PRODUCT_TASK_GATE` env isolation. Canonical exact-head run `30732749013` passed the complete source matrix including `pg-integration-tests` and the terminal `context-capsule`; the exact-head independent delta review for `89920d99..64d45f5` was APPROVED with no blocking findings; no provider call or target effect occurred.
- PR #353 is merged and accepted as `4e6ceca804c329c7356dc4254302bf7f83b78cb2` from exact head `708b431e4ff4edfbdee999dfb092e04935a95e24` (base `f37ad7f72c7d49257b8cf28df4ca4388ad2249f4`): local-loop repair that keeps Issue admission available when the deferred Plan lane cannot be read. GitHub contents base64 is whitespace-stripped before decode, and Plan-lane `LoopUnavailable`/`PlanLaneError` become non-admission `plan_lane_deferred:*` records while active-scope/capacity checks and Issue evaluation stay fail-closed unchanged; operator-local proxy/CA/XDG environment is forwarded into the sanitized Codex child (connection configuration only; `GH_TOKEN`/`GITHUB_TOKEN`/provider API keys remain excluded). Canonical exact-head run `30805127840` passed the complete source matrix plus the terminal `context-capsule`; the exact-head complete-diff review receipt `5165189771` is PASS with no blocking findings. No provider call or target effect occurred.
- Outbound local-loop smoke (packet `TOOL-LOCAL-LOOP-CONTROL-PLANE-1`): after the proxy-environment repair above, a real `loopctl poll → claim-local → run-once` cycle against Issue #355 completed with `handed_off`, producing Draft PR #356 at exact head `d61bf3fe65406ba7e8fa24784f223e19f2303f01` (docs-only smoke note, closed without merge; facts recorded here). The legacy public self-hosted execution path is not simultaneously active: workflow `agent-intake` is `disabled_manually`, and Control Issue #208 carries `agent-emergency-stop` with no `agent-orchestrator-enabled` label since 2026-08-03T09:57:43Z; the orchestrator was enabled only for the bounded smoke window 09:11–09:57Z, and no controller or intake run is queued, in progress, or later than that window. This smoke is operator evidence toward the packet's acceptance criteria, not RWE or managed acceptance.
- The owner-authorized clean live reseal for `PE7-PRODUCT-GOLDEN-PATH-DEEPSEEK-LIVE-SEAL-1` was executed and independently accepted from Harness accepted head `a1878b2a282303d6e187f35c437875493c0f5296`: ProductTask `ptask-20260802061735-18c7e889a3570d82`, attempt `attempt-pe7-live-seal-1785651576`, and delegation `delegation-pe7-live-seal-1785651576` completed through terminal closure. Target `Igzela/alters-lab/main` remained unchanged at `6240768506320a324d68787b9eaa86971c8c930c`; one unmerged Draft PR #5 was produced at exact head `967c902487edf3959090e76c442092f75b0ba10a`. The run used three zero-retry provider requests, realized 6,800 tokens, and reconciled client-side realized cost `$0.000583862`; spend and delegation expired, the attempt lease closed, workspace cleanup/rollback completed, and exact-head receipt comment `5158092741` records PASS. This is exactly one workflow sample and remains `INSUFFICIENT_REPETITIONS`; it establishes neither RWE, reliability, success probability, ROI, learning, release, deployment, installation, merge, nor target-main authority.
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
| `Igzela/alters-lab#5` | External Draft-PR evidence output for the accepted clean live seal | OPEN, Draft, unmerged; exact head `967c902487edf3959090e76c442092f75b0ba10a`; receipt comment `5158092741`; no merge authority |
| #350 | Packet `TOOL-REVIEW-TRANSPORT-IDEMPOTENCY-REPAIR-1` implementation | MERGED at `0bd95012…` after exact-head independent review PASS and full canonical source matrix; non-authoritative review-loop transport core (idempotent delivery, strict receipt parsing, comment idempotency) + provider-free tests |
| #353 | Local-loop repair: plan-document base64/parse failures do not block Issue poll; proxy/CA/XDG env forwarded to Codex child | MERGED at `4e6ceca80…` after complete-matrix canonical run `30805127840` (terminal `context-capsule` green) and exact-head review receipt `5165189771` PASS |
| #356 | Outbound local-loop smoke note (Issue #355, exact head `d61bf3fe654…`, docs-only) | CLOSED without merge; smoke facts recorded in this file; the Draft PR remains as the smoke evidence record |

PR #297 and #298 are closed without merge as superseded by accepted PR #299. PR #303 is closed without merge as superseded by accepted PostgreSQL ordering repair PR #304. PR #301, PR #306, PR #308, PR #310, PR #311, PR #313, and PR #315 are merged and accepted.

One owner-authorized live attempt was made and recorded truthfully under `PE7-PRODUCT-GOLDEN-PATH-DEEPSEEK-LIVE-SEAL-1`: a real `Pro → Flash → deterministic verify → Pro` workflow with target `Igzela/alters-lab` main unchanged, one unmerged `acp/*` Draft PR (alters-lab PR #3), realized cost approximately `$0.0016`, expired one-use spend, closed attempt lease, cleaned workspace, and a realized workflow sample labeled `INSUFFICIENT_REPETITIONS`. It is not a clean accepted seal: the provider-stage binary came from a dirty tree and did not equal an accepted commit or the final #325 head, and terminal closeout used later local CAS-rebind code. PR #325 is the accepted live-observed repair. No live managed acceptance, live RWE baseline, accepted success probability, or realized VDE result is established.

The later observed `Igzela/alters-lab#4` attempt is also `LIVE_OBSERVED_NOT_ACCEPTED`: three provider requests, 6,976 realized tokens, `$0.00143` realized cost, target main unchanged, and one unmerged Draft PR were observed, but canonical restart recovery failed. Direct SQLite authority mutation, manual ProductTask version rollback, and manual scope restoration invalidate its clean E2E claim. PR #4 and its branch are not reusable for a clean attempt.

## Current Product Verdict

Product Golden Path authority includes the provider-free production runner through accepted PR #322, the provider-free delegated autonomous path through accepted PR #323 at schema v36, the live-observed budget/CAS repair through accepted PR #325, Packet A through accepted PRs #339 and #340, and Packet B through accepted PRs #342 and #346. The owner-authorized clean exact-main live reseal has now satisfied the remaining live-seal acceptance condition: one ProductTask completed from intake through terminal closure under the accepted Harness source/binary identity, target main remained unchanged, one bounded unmerged `acp/*` Draft PR was produced, usage and cost reconciled, and spend/delegation/lease/cleanup terminalized. Live execution remains default-off, and no provider request or target effect is a repository default.

Fixture evidence proves the existing product sequence:

```text
intake → worktree/source binding → executable graph → scheduler lease
→ bounded executor → verification → artifact → approval
→ separate output confirmation → acp/* Draft PR → terminal evidence
```

The live residual seal is therefore complete as one independently accepted workflow observation. It remains exactly one sample labeled `INSUFFICIENT_REPETITIONS`; it does not establish an RWE baseline, stable reliability, success probability, economic improvement, ROI, cross-task learning, release readiness, deployment readiness, or merge authority. The next eligible packet is `TOOL-REVIEW-TRANSPORT-IDEMPOTENCY-REPAIR-1`; live RWE remains blocked until that packet is accepted and the separate RWE corpus/protocol/spend gates are satisfied. Unused reseal attempt-2 and attempt-3 sub-authorizations do not authorize additional runs after the accepted seal without a new planning-layer decision.

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
| Golden Path live residual seal | `COMPLETE` | Attempt-1 completed from accepted exact-main source/binary identity through independently reviewed terminal closure; target main unchanged; one unmerged Draft PR; one sample remains `INSUFFICIENT_REPETITIONS` |
| Independent review transport hardening | `COMPLETE` | Packet `TOOL-REVIEW-TRANSPORT-IDEMPOTENCY-REPAIR-1`; PR #350 merged at `0bd95012…` after exact-head independent review PASS and full canonical source matrix |
| Context capsule automation | `COMPLETE` | PR #306 provides publication/injection; PR #313 proves the repaired post-merge push terminal path on `ca5ce102…` |
| VDE decision and measurement contract | `COMPLETE` | Provider-free architecture/routing contract only; no accepted live measurement or implementation artifact exists |
| First Real Workload Evidence | `BLOCKED_PREREQUISITE` | Accepted review-transport repair, frozen operator-supplied real corpus/protocol, and separately persisted one-use RWE spend envelope |
| Architecture Convergence AC1–AC7 | `BLOCKED_PREREQUISITE` | Frozen and independently accepted pre-convergence RWE baseline |
| Same-corpus RWE replay | `BLOCKED_PREREQUISITE` | Architecture Convergence complete |
| Level-2 GO/NO-GO | `BLOCKED_PREREQUISITE` | Comparable pre/post-convergence layered-success, reliability, lifecycle-cost, VDE/Pareto, and maintenance evidence |
| Level-2 generational controller | `BLOCKED_PREREQUISITE` | Explicit evidence-backed GO decision |
| Meta Improver experiment | `BLOCKED_PREREQUISITE` | Accepted Level-2 plus a separately authorized unseen-task experiment |
| Dashboard #225 | Deferred | Handle last; presentation cannot substitute for runtime proof |

## Project Objective

The repository seeks verifiable and reusable task delivery per unit of total lifecycle cost, subject to hard quality, safety, traceability, compatibility, recovery, and rollback gates. `docs/ARCHITECTURE_BOOK.md` owns the full VDE semantics; this status page records only that the direction is adopted and that no live VDE observation or improvement claim exists yet.

## Confirmed Integration Gaps

1. One clean exact-main live managed-coding E2E now exists and has independently accepted terminal evidence, but it is exactly one workflow sample and remains `INSUFFICIENT_REPETITIONS`. No repeated live RWE baseline, stable reliability estimate, success probability, or economic comparison exists. The observed independent-review transport idempotency, recovery, live-validation, strict-verdict parsing, and comment-posting gaps must be repaired before unattended review reuse or RWE execution.
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
- Product Golden Path preflight: PR #308 is merged and accepted at schema v35; PR #320–#322 complete managed-coding boundary, DeepSeek protocol, and production runner wiring; PR #323 completes the provider-free delegated autonomous path at schema v36; PR #325 is the accepted live-observed budget/CAS repair; Packet A is accepted through PRs #339 and #340; Packet B is accepted through PRs #342 and #346; the clean exact-main live reseal is complete; `TOOL-REVIEW-TRANSPORT-IDEMPOTENCY-REPAIR-1` is the next eligible frontier.
- Context/CI governance: PR #302, PR #306, PR #310, PR #311, PR #313, PR #315, and PR #318 are merged and accepted; transport, fast feedback, and cache state remain non-authoritative.
- VDE governance: the provider-free decision contract and artifact-schema validation are complete through PR #319; a real corpus, live observations, persistence automation, and Dashboard projection remain gated future work.
- Observation adaptation: PR #301 is merged and accepted; observation-only and restacked onto accepted main.
- Live RWE, Architecture Convergence, Level-2, and Meta remain blocked by their named prerequisites.

## Open Work Coordination

The accepted clean exact-main Golden Path live reseal is complete as one `INSUFFICIENT_REPETITIONS` workflow observation. The active frontier is `TOOL-REVIEW-TRANSPORT-IDEMPOTENCY-REPAIR-1`; RWE, Architecture Convergence, Level-2, Meta, and Dashboard work remain ineligible until their named prerequisites, and PR #225 remains presentation-only and last.

All active branches must refresh this main documentation convergence before final merge and must not overwrite it with stale branch-local status text.

## Safety Boundary

Default-off execution; no provider call in CI; no target-default-branch write; no auto-merge; no release or deployment authority; no reusable credential in a child; no secret, raw prompt, raw output, transcript, private path, fixture-only result, forecast value, or scalar VDE index may become durable production-adoption authority.
