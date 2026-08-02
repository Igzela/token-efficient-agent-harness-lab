# Next Decision

Last updated: 2026-08-02.

## Current Direction

The repository optimizes one outcome:

> Under non-negotiable quality, safety, traceability, compatibility, and rollback constraints, increase verifiable and reusable task delivery per unit of total lifecycle cost.

Quality, safety, integrity, authority, compatibility, evidence completeness, and rollback are hard gates. Accepted delivery, reliability, token use, monetary cost, latency, engineering effort, maintenance surface, recovery burden, and observed reuse are optimization evidence only after those gates pass.

Do not substitute feature count, model/provider count, Dashboard completeness, PR creation, fixture success, a single successful run, or a scalar efficiency index for product capability or learning.

The authoritative order is:

```text
provider-free RWE authority reconciliation (#300)
→ observation-only reconciliation (#301)
→ context-capsule automation (#306/#313 accepted)
→ provider-free RWE economic protocol and VDE artifact contracts
→ managed-coding boundary generalization
→ provider-free DeepSeek dual-protocol integration
→ production DeepSeek ProductTask runner wiring (#322 accepted)
→ delegated autonomous Golden Path authority and provider-free proof (#323 accepted)
→ exactly one owner-authorized bounded DeepSeek Golden Path live seal
→ non-authoritative independent-review transport idempotency/recovery repair
→ outbound local loop control-plane and run-once worker cutover
→ freeze one operator-supplied real RWE corpus under the accepted protocol
→ first frozen Real Workload Evidence baseline
→ Architecture Convergence AC1–AC7
→ identical-corpus and identical-protocol replay
→ VDE/Pareto evidence and Level-2 GO/NO-GO
→ bounded Level-2 controller only on GO
→ separately authorized Meta Improver experiment
→ Dashboard #225 last
```

The owner has authorized and accepted the production runner repair (#322), the delegated autonomous authority packet (#323), and the repair/reseal objective in authorization `GOLDEN-PATH-RECOVERY-AND-CLEAN-RESEAL-20260801` issued at `2026-08-01T14:29:00+09:00`. The earlier observed attempts remain non-accepted: the first used a dirty/non-exact source identity, and the later `Igzela/alters-lab#4` recovery required direct SQLite authority mutation plus a manual ProductTask version rollback. The current authorization permits a maximum of three separately consumed attempts, one ProductTask per attempt, at most three provider requests per attempt, zero retries, at most one new `acp/*` branch and one Draft PR per attempt, and a combined provider spend cap of `$1.00`. Its route is `deepseek-v4-pro` planning, `deepseek-v4-flash` implementation, deterministic non-provider verification, and `deepseek-v4-pro` review. Each attempt binds `Igzela/alters-lab/main` to a freshly resolved exact SHA, leaves target main unchanged, and forbids merge, auto-merge, release, deployment, RWE, direct SQL mutation, credential disclosure, raw prompt/output persistence, or outcome-unknown retry. Fixture completion is not live acceptance. Context-capsule automation is transport, not authority. VDE is a read-only evidence projection, not a new execution or adoption authority.

Clean reseal attempt-1 has been consumed, terminalized, and independently accepted. The remaining attempt-2 and attempt-3 allowances are unused ceilings, not authority to run additional reseals after acceptance; any further live-seal attempt requires a new planning-layer decision.

## Active Routing

1. `TOOL-REVIEW-TRANSPORT-IDEMPOTENCY-REPAIR-1` — `READY_FOR_EXECUTION`: harden the non-authoritative independent-review transport before automated review reuse or RWE.
2. `TOOL-LOCAL-LOOP-CONTROL-PLANE-1` — `BLOCKED_PREREQUISITE`: a separate Draft may prepare the outbound local `poll`/`run-once` adapter, but implementation acceptance and merge wait for the review-transport prerequisite.
3. `PE7-REAL-WORKLOAD-EVIDENCE-1` — `BLOCKED_PREREQUISITE` on the accepted review-transport and local-loop repairs, frozen real corpus/protocol, and separately persisted one-use RWE spend envelope.
4. `PE7-ARCHITECTURE-CONVERGENCE-1` — `BLOCKED_PREREQUISITE`.
5. `PE7-REAL-WORKLOAD-EVIDENCE-REPLAY-1` — `BLOCKED_PREREQUISITE`.
6. `PE7-HARNESS-EVOLUTION-LEVEL2-GENERATIONAL-CONTROLLER-1` — `BLOCKED_PREREQUISITE`.
7. `PE7-META-IMPROVER-EXPERIMENT-1` — `BLOCKED_PREREQUISITE`.
The delegated autonomous Golden Path packet remains complete through merged PR #323 and is no longer routed. The live-seal packet becomes accepted `COMPLETE` only when this canonical closeout diff passes exact-head review, canonical CI, and merge.

## Packet States

- `READY_FOR_EXECUTION` — prerequisites and authority are sufficient to begin.
- `BLOCKED_PREREQUISITE` — a named earlier evidence or authority condition is incomplete.
- `DECISION_REQUIRED` — safe authority cannot be derived automatically.
- `IN_PROGRESS` — one current branch/PR board owns the work.
- `COMPLETE` — merged, verified, independently reviewed, and documented.

Historical compatibility labels retained for handoff checks only: Packet PR207-REPAIR-1; Packet PE2-RUNTIME-PRODUCER-1; Packet PE4-EVIDENCE-ENTRY-1; Packet TOOL-DISCOVERY-BENCH-1. They are not active routing.

## Open PR Coordination

- PR #299 is merged and accepted; superseded PRs #297/#298 are closed without merge.
- PR #300 is merged and accepted.
- PR #301 is merged and accepted; it is observation-only and did not import authority.
- PR #306 is merged and accepted; it publishes/injects a non-authoritative fresh context capsule.
- PR #308 is merged and accepted; it hardens provider-free ProductTask workspace preparation and recovery without importing live authority.
- PR #310 and PR #311 are merged CI-governance changes.
- PR #313 is merged and accepted. Post-merge `push: main` run `30381836225` passed all seven source jobs and produced terminal capsule artifact `8697748363` bound to `ca5ce1023664c58be8d15d681a80f262fb2be70b`.
- PR #325 is merged and accepted as `0da5c6c785004784d9ffa3b20e0068f4bac6be71` from exact head `7ecffd5a30426dd1f26ab4d46a8f2a36e7594568`: live-seal budget reservation release and Draft PR terminal CAS rebind. Exact-head canonical `tests` runs `30613256286` (full) and `30613256266` (PR) were successful; post-merge `push: main` run `30636441727` passed all jobs and bound context-capsule artifact to the merge SHA.
- PR #336 is merged as `17723bb66a1274498c32aef0f6cac85ad339efea`, but canonical acceptance is incomplete. Its run `30680937667` is recorded as `source_matrix: success`, `terminal_context_capsule: failure`, `overall_canonical_run: failure`, `failure_reason: PR merged before terminal required job`; Packet A repairs the material governance findings. It does not authorize a live task, provider call, spend, merge policy change, or any RWE/VDE reclassification.
- PR #342 is merged and accepted as `e1e08ddcb745b02892f099b9de1436c99c25d533` from exact head `666cabeab31c14c77389646edc140c2d8ae7eb86`. It completes Packet B's canonical restart recovery and minimal managed-identity authority. Exact-head run `30710854561` passed all source jobs, `pg-integration-tests`, and terminal context capsule `91399051309`; the exact-head independent delta review was PASS with no unresolved objections.
- PR #346 is merged and accepted as `adcb87b4a3ece961a46455117ae4323b4f54c2fa` from exact head `64d45f5e55f4393737c26b26dbd66976b8145d5d`. It is the Packet B closeout repair binding managed identity mutation to the canonical bootstrap store owner (reviewer/output-operator identity creation and mutation require the store-owned canonical bootstrap principal and exact local tenant), serializing the canonical Rust test lane, and wrapping PostgreSQL key-authority store calls in `spawn_blocking` to remove an async-handler runtime-in-runtime panic. Exact-head run `30732749013` passed all source jobs including `pg-integration-tests` and the terminal context capsule; the exact-head independent delta review for `89920d99..64d45f5` was APPROVED with no blocking findings.
- External evidence PR `Igzela/alters-lab#5` remains OPEN, Draft, and unmerged at exact head `967c902487edf3959090e76c442092f75b0ba10a` over target-main base `6240768506320a324d68787b9eaa86971c8c930c`. Exact-head independent terminal-evidence receipt comment `5158092741` is PASS and explicitly grants no merge authority.
- PR #225 is presentation-only and remains last.

The provider-free `PE7-VDE-RWE-ARTIFACT-CONTRACTS-1` packet is complete through PR #319, managed-coding generalization through PR #320, protocol support through PR #321, live-runner wiring through PR #322, delegated autonomous authority through PR #323, Packet A through PRs #339/#340, and Packet B through PRs #342 and #346. The clean live reseal has accepted terminal evidence; the canonical closeout and then `TOOL-REVIEW-TRANSPORT-IDEMPOTENCY-REPAIR-1` are the active route. Do not begin RWE or later stages before their updated prerequisites.

## Evidence Required for Every Engineering Board

Each coherent board must return a bounded `implementation_cost_receipt` in its final report. This is review evidence, not a new runtime store or budget authority.

Record when available:

```text
agent_sessions
review_cycles
repair_iterations
ci_runs
ci_compute_minutes
files_changed
schema_migrations
compatibility_adapters_added
authority_boundaries_touched
external_dependencies_added
rollback_complexity
known_maintenance_surface
observed_reuse_count
expected_reuse_count
cost_or_measurement_unavailable_fields
```

The receipt may begin as a report/document contract. Persisting or automating it requires a later reviewed design and must reuse existing evidence/artifact owners.

Separate realized facts from forecasts:

```text
realized_lifecycle_cost
forecast_lifecycle_cost
observed_reuse_count
expected_reuse_scenario
```

Expected reuse, future maintenance, and amortization are scenario inputs until observed. Failed, cancelled, timed-out, killed, recovered, and outcome-unknown attempts retain their consumed cost; successful-run-only costing is prohibited.

A Level-2 GO decision requires more than runtime token improvement. It must consider comparable layered success, reliability, provider/token/latency/cost evidence, implementation and review cost, migration/rollback risk, maintenance surface, authority growth, failure recovery, observed reuse, uncertainty, and realistic implementation feasibility.

A change that reduces tokens but increases total lifecycle cost, weakens reliability, increases material rework, or broadens authority without accepted benefit is not an efficiency improvement.

## VDE Routing Contract

`docs/ARCHITECTURE_BOOK.md` is the sole full owner of VDE semantics: layered success, typed value bases, realized/forecast separation, LCAP, evidence-sufficiency states, reviewer measurement, artifact-first persistence, Pareto precedence, and non-authority boundaries.

This document owns only execution routing:

- the first live Golden Path sample may prove evidence wiring and realized-cost capture only; it remains `INSUFFICIENT_REPETITIONS`;
- before RWE, freeze the exact real corpus, source/verifier, primary value basis, reviewer policy, repetitions, budget grid, stop rules, non-inferiority margins, cost completeness, seeds, and statistical method;
- replay the identical corpus and protocol after Architecture Convergence;
- require `COMPARISON_ELIGIBLE` evidence and hard-gate non-inferiority before Level-2 GO;
- do not extend Level-1 `MetricVector`, add a VDE table, automate adoption, or create a second evidence authority in this packet.

This provider-free contract does not move the active frontier, authorize a live task, establish an RWE baseline, or create a VDE result.

## Common Execution Protocol

- Refresh actual `main`, open PR heads, CI, reviews, active documents, and overlapping ownership before work.
- Generate a fresh context capsule from the confirmed accepted baseline; treat it as stale when `main`, PR head, CI, review, or canonical documents change.
- Use one Agent session per coherent board when practical, with internal commit boundaries rather than repeated approval interruptions.
- Do not combine unrelated authority surfaces into one unreviewable commit.
- A new head invalidates earlier CI and review conclusions.
- Reuse the existing scheduler, executor, worktree, verification, artifact, approval, output, replay, scorecard, audit, and `LocalProductStore` owners.
- Bind authority from persisted current owners, never caller assertions.
- Preserve SQLite/PostgreSQL parity, atomicity, restart, concurrency, idempotency, cancellation, lease ownership, late-write refusal, and rollback.
- Keep provider execution off in CI; keep target `main` unchanged; keep auto-merge disabled.
- No Agent may self-approve risk, spend, merge, release, deployment, production adoption, value basis, reviewer acceptance, or economic improvement.
- Finish focused/full checks, exact-head CI, complete-diff review, handoff validation, and rollback review before merge.

## Golden Path Acceptance Gate

A live managed task may start only when all of these are current and exact:

- accepted decision and residual-risk hashes;
- authenticated non-fixture operator principal and required scopes;
- separate one-use spend authorization;
- parent-only credential that never enters the child;
- versioned managed-coding runtime profile and exact observed executable path/version/SHA/capabilities when a binary exists;
- exact provider/protocol/host/base URL/admitted paths/requested model/resolved model;
- exact ProductTask/workflow/node/attempt identity;
- exact target repository and target-main SHA;
- request/retry/token/time/cost contract;
- Draft-PR-only output, no auto-merge, no release/deploy;
- gateway/session usage reconciliation;
- cancellation, cleanup, rollback, approval, output-confirmation, and terminal-evidence owners;
- a fresh context capsule bound to the current accepted-main SHA, active PR exact head, workflow evidence, review observation time, and next permitted action.

Codex remains `mediation_hardened_partial`. Retry identity, product-enforced loopback-only network confinement, and host namespace limitations remain explicit residual risks unless separately proved.

The first bounded live Golden Path task also records one complete realized workflow sample when available: provider/request/token/latency/cost-source evidence, human preparation, review and material rework time, repair iterations, CI effort, recovery, approval/output, cleanup, and terminal evidence. That sample must not be reported as ROI, stable VDE, success probability, or an RWE baseline.

## Hard Stops

Stop before any of the following:

- secret, credential, raw prompt/output/transcript, private path, or repository-content exposure;
- second runtime, scheduler, store, evaluator, budget, approval, output, audit, rollback, VDE authority, or context-authority owner;
- caller-asserted authority, stale or conflicting identity, duplicate effect, late write, missing lease, or outcome-unknown treated as success;
- provider call in CI;
- target-default-branch write, auto-merge, merge, release, deployment, installation, or production adoption;
- unreviewed schema migration or SQLite/PostgreSQL semantic divergence;
- performance, cost, value, reliability, VDE, ROI, or learning claim without comparable frozen evidence;
- implicit aggregation across incompatible value bases;
- treating forecast cost/reuse as realized evidence;
- changing corpus, reviewer policy, budget, verifier, or thresholds after observing comparison results.

## Packet PE7-OBSERVATION-RESTACK-1 — observation-only PR #301 reconciliation

**State:** `COMPLETE`

**Owned PR:** #301 (merged and accepted)

PR #300 is the accepted prerequisite and RWE authority foundation. PR #301 was mechanically restacked as an observation-only layer onto accepted main, its observation file blob identity was verified, and it passed complete exact-head CI and independent review. This packet is observation-only and did not import authority, credentials, proxy ownership, budget ownership, or live execution.

## Packet PE7-CONTEXT-CAPSULE-AUTOMATION-1 — exact-head publication and session injection

**State:** `COMPLETE`

**Owned PR:** #306, with push-terminal repair accepted through #313

**Prerequisite:** PE7-OBSERVATION-RESTACK-1 (COMPLETE)

Phase 1 is accepted through PR #302: `START_HERE.md` owns navigation and `scripts/project_context.py` generates an on-demand fail-closed Markdown or JSON transport view. Phase 2 is accepted through PR #306. PR #313 proves the repaired post-merge push terminal path. The automation does not create a status database, current-state owner, authorization owner, or committed dynamic `latest context` file.

Required result:

- generate once per terminal exact-head workflow, not once per job;
- bind the capsule to accepted-main SHA, active packet, owned PR exact head, workflow run, complete required-check matrix, exact-head review/objection observation, and observation time;
- publish only a short-lived workflow artifact and/or job summary;
- inject or fetch a fresh capsule at the start of repository-controlled implementation, CI-repair, and review sessions;
- mark evidence unavailable rather than guessing and invalidate the view whenever `main`, head, CI, review, or canonical documents change;
- preserve secret, raw prompt/output/transcript, private-path, and repository-content redaction;
- reuse `START_HERE.md`, `scripts/project_context.py`, its tests, and the handoff checker as the sole navigation/transport owners.

This packet proves context freshness and routing only. It cannot authorize provider spend, live execution, output, merge, release, deployment, RWE acceptance, VDE acceptance, or a later packet.

## Packet PE7-VDE-RWE-ARTIFACT-CONTRACTS-1 — provider-free measurement contracts

**State:** `COMPLETE`

**Owned PR:** #319

**Prerequisite:** PE7-CONTEXT-CAPSULE-AUTOMATION-1 (COMPLETE)

Implement immutable, canonical-hash-bound `rwe_economic_protocol.v1`, `task_value_profile.v1`, `implementation_cost_receipt.v1`, `verified_delivery_observation.v1`, and `verified_delivery_comparison.v1` contracts under the existing `engine/src/rwe/` owner.

Required behavior:

- freeze exact task/source/tree/definition/mutable-surface/verification/output/executor/model/cleanup identities;
- bind typed value basis, source, confidence, acceptance rubric, reviewer policy, repetitions, budget grid, stop rules, non-inferiority margins, cost completeness, seeds, and statistical method before results;
- derive evidence sufficiency without treating unavailable evidence as zero;
- keep realized cost, forecasts, observed reuse, and expected reuse separate;
- require identical-protocol and hard-gate evidence before `COMPARISON_ELIGIBLE`;
- retain truthful insufficient, failed, and NO-GO artifacts;
- reject fixture/placeholder sources and raw prompt/output/transcript/credential fields from a real protocol;
- add no database table, migration, runtime execution, provider call, spend lease, reviewer authority, output authority, Level-1 `MetricVector` change, automated adoption, release, or deployment.

Acceptance requires focused Rust tests, complete applicable CI, complete-diff review, handoff validation, and rollback by reverting the packet. This packet does not freeze an operator-supplied real corpus, establish a baseline, or unblock Architecture Convergence.

## Packet PE7-MANAGED-CODING-BOUNDARY-GENERALIZATION-1

**State:** `COMPLETE`

**Owned PR:** #320, merged as `630895b91703eb9e9caada24690a08900c0d6991`

**Prerequisite:** accepted provider-free Golden Path authority through PR #319. Satisfied.

Generalize the ProductTask-owned managed-coding boundary without replacing the Rust runtime, scheduler, `LocalProductStore`, ProductTask budget, managed-acceptance decision/risk/spend/attempt receipts, workspace, verification, artifact, approval, output, audit, or terminal-evidence owners. Rust-owned wire governance must define a versioned runtime profile containing executor kind, protocol kind, executable identity where applicable, capability probes, requested/resolved model, thinking configuration, provider/credential/endpoint identity, usage-parser and pricing provenance, and admission class.

Codex may no longer be admitted by a compile-time `0.145.0` equality check. A profile-controlled compatible range or explicit list is permitted, but every execution must canonicalize a regular executable; reject symlinks, missing/non-executable files, drift, failed probes, hash/profile mutation, and revalidate immediately before spawn. The exact observed path, version, SHA-256, capabilities, and profile hash bind the attempt. Existing Codex schemas and fixtures remain readable through explicit compatibility adapters.

Add `git_repository` and `local_folder` source kinds. Git retains exact remote/default-branch SHA, app-owned detached worktree, bounded mutable paths, unchanged target main, and Draft-PR/export-only output. Local folders require an absolute canonical root, a safe source manifest/tree hash, staging-copy execution, secret/private-path exclusions, original-manifest revalidation, and public redaction. `artifact_only`, export/bounded bundle, and separately confirmed `apply_local_changes` are distinct outputs; the latter must verify preimages, create an app-owned rollback bundle, refuse stale/duplicate/late/cancelled/unknown effects, and retain redacted cleanup/rollback evidence.

Required evidence includes compatible Codex patch admission without a Rust constant change; failed capability, symlink, replacement, and profile-mutation cases; legacy fixture readability; local-folder staging, verification, rollback, stale-preimage, symlink-escape, cancellation, restart, duplicate, cleanup, migration/idempotency, and SQLite/PostgreSQL parity. No provider call or target output is permitted.

## Packet PE7-DEEPSEEK-DUAL-PROTOCOL-MANAGED-CODING-1

**State:** `COMPLETE`

**Owned PR:** #321, merged as `542a5a453308f8a84e540f48767a80a9e58bf99d`

**Prerequisite:** Packet `PE7-MANAGED-CODING-BOUNDARY-GENERALIZATION-1` accepted and merged. Satisfied by PR #320.

Introduce one protocol-neutral, ProductTask-bound managed provider-call authority. It binds ProductTask/workflow/node/attempt, model role, provider/protocol/host/base URL/path, requested/resolved model, symbolic credential reference, request/retry/token/time/cost limits, current spend authorization, and attempt lease. It reuses the existing provider clients, AgentStep/scheduler/tool-policy/supervised-workspace path, normalized `execution_usage_event.v1`, and parent-owned journal; it must not form a hidden agent loop or another budget owner.

The only accepted DeepSeek identities are `deepseek-v4-flash` and `deepseek-v4-pro`. Support both official compatibility routes: OpenAI-compatible `https://api.deepseek.com` plus `/chat/completions` with `Authorization: Bearer`, and Anthropic-compatible `https://api.deepseek.com/anthropic` plus `/v1/messages` with `x-api-key`. The parent Harness resolves symbolic `DEEPSEEK_API_KEY`; no raw credential may enter a model-created command, child environment, persistence, public evidence, or log. Missing/ambiguous/aliased/conflicting returned model or insufficient required usage fails closed, including the provider's documented fallback mapping for unsupported Anthropic model names.

Default roles are Pro planner, Flash implementer, deterministic verification, then Pro bounded review, under one ProductTask budget envelope with optional bounded per-role sublimits. Profiles must preserve exact request paths, thinking/effort, tool semantics, stream and non-stream parsing, request IDs, stop status, normalized cache/reasoning usage, retry/outcome-unknown classification, versioned price source/verified-at metadata, and conservative pre-send reservation. Missing or stale dollar pricing blocks a dollar live gate; provider-free fixtures may use token-only bounds.

Required evidence is deterministic local mock-provider coverage for both models and both protocols, role routing, tools/malformed tools, identity and alias rejection, usage conflicts, cache/reasoning accounting, all limits, pre-send versus outcome-unknown behavior, malformed/truncated streams, credential redaction, cancellation/restart/duplicate/lease/late-response handling, existing-provider/Codex compatibility, and SQLite/PostgreSQL parity. Canonical CI never calls DeepSeek.

## Packet PE7-DEEPSEEK-LIVE-RUNNER-WIRING-REPAIR-1

**State:** `COMPLETE`

**Owned PR:** #322, merged as `13f725f949684d179593a6559d8600a5b5d47edf`

**Prerequisite:** PR #321 accepted and merged. Satisfied.

Wire the existing `ManagedDeepSeekProvider::invoke_with_authority` into the existing ProductTask scheduler/executor path as Pro planning, Flash bounded implementation, deterministic verification, and Pro review. Preserve one ProductTask budget and the existing store, scheduler, workspace, approval, output, audit, rollback, and terminal-evidence owners. Deterministic verification decides success. Exact request/model/protocol/usage identity, conservative reservation, no retry after outcome unknown, cleanup, and restart behavior remain fail closed.

## Packet PE7-DELEGATED-AUTONOMOUS-GOLDEN-PATH-1

**State:** `COMPLETE`

**Owned PR:** #323, merged as `8d9f8dc47ab458fa01873571fbc7b60ebaf211f0` from exact head `3ca951dd8a645e1a7f2b6cc800d9fad54c010fd7`

**Prerequisite:** PR #322 accepted and merged. Satisfied.

**Acceptance evidence:** canonical exact-head CI run `30607238397` successful; independent Theo GO on the exact head; squash merge to main.

Immutable final execution manifests, separated delegated approval/output authority, one-use spend and attempt leases, durable pre-send journaling, Draft-PR-only output, terminal cleanup, and SQLite/PostgreSQL restart parity are accepted on main. Canonical CI remains provider-free and target-free.

## Packet CI-EVIDENCE-AND-GOVERNANCE-CLOSEOUT-REPAIR-1

**State:** `COMPLETE`

**Owned PRs:** #339 and #340, both merged; exact heads and merge SHAs are recorded below.

**Prerequisite:** accepted main `1dac6d16f7a99faacb856d6c0cbdb5eed9fd881b`. Satisfied.

Correct the false PR #336 CI claim and repair material governance findings from the independent complete-diff review of merge `17723bb66a1274498c32aef0f6cac85ad339efea`. The existing security-baseline owner remains the sole owner of automation, plugin-fingerprint, dormant-surface, allowlist, and review-receipt checks. The repair preserves fail-closed behavior, exact-head binding, terminal context-capsule requirements, provider-free CI, and reversible documentation rollback.

Acceptance requires: the exact #336 distinction `source_matrix: success`, `terminal_context_capsule: failure`, `overall_canonical_run: failure`, `failure_reason: PR merged before terminal required job`; complete independent re-review of the merged #336 range; repaired allowlist/automation/plugin/dormant-surface/review-receipt tests; exact-head independent reviews of PRs #339 and #340; canonical `tests` success including terminal capsule and artifact publication; and a post-merge full accepted-main verification run recorded truthfully. The merge discipline now requires every required job, including the terminal context-capsule, to be completed successfully before merge; it does not weaken exact-head or terminal checks.

**Merged implementation evidence:**

```yaml
pr_339:
  exact_head: 240e80065b9632b75ce7c733b63da59fb14c0680
  merge_sha: a7ddf1cd588c71d553bf4d0644a6dabdd55e5ea
  exact_head_review: PASS_WITH_NOTES
  reviewer_session_identity: a4ac23a9-5c63-4623-8532-83b3901261f5
  canonical_run: 30695452770
  terminal_capsule_job: 91358472963
  artifact: context-capsule-30695452770-1-240e80065b9632b75ce7c733b63da59fb14c0680
pr_340:
  exact_head: 4aed0af5227c53efeb711c8123d922c2e3133cea
  merge_sha: dc1d839316771145a0b1c079bfbc66b30c0ab61a
  exact_head_review: PASS_WITH_NOTES
  reviewer_session_identity: d02c0cd6-6d40-4524-a14b-2da670d20de4
  canonical_run: 30696552639
  terminal_capsule_job: 91361226959
  artifact: context-capsule-30696552639-1-4aed0af5227c53efeb711c8123d922c2e3133cea
post_merge_accepted_main_verification:
  failed_run: 30695885514
  failed_run_reason: terminal capsule applied pull_request_only_matching_to_workflow_dispatch_and_rejected_unavailable_pr_head
  replacement_run: 30696953015
  source_matrix: success
  terminal_context_capsule: success
  overall_canonical_run: success
  terminal_capsule_job: 91362363217
  artifact: context-capsule-30696953015-1-dc1d839316771145a0b1c079bfbc66b30c0ab61a
  artifact_expired: false
```

**Persisted independent complete-diff review:**

```yaml
independent_complete_diff_review:
  reviewed_merge_sha: 17723bb66a1274498c32aef0f6cac85ad339efea
  reviewed_range: bd096e34986f60824908a72ab97df0e8a31eb461..17723bb66a1274498c32aef0f6cac85ad339efea
  reviewer_session_identity: 019fbc9e-75be-7491-b2e9-65862a337e91
  supporting_reviewer_session_identity: 019fbc9e-7481-7451-b230-b99e35066357
  reviewer_authenticated_identity: unavailable
  review_transport: local_read_only_inspection_not_direct_authenticated_github_review
  observed_at: 2026-08-01T09:30:40Z
  correctness: PASS_WITH_NOTES
  security: PASS_WITH_NOTES
  authority_integrity: PASS_WITH_NOTES
  compatibility: PASS_WITH_NOTES
  CI_governance: PASS_WITH_NOTES
  rollback: PASS
  unresolved_objections: none_for_reviewed_merged_range
  verdict: PASS_WITH_NOTES
```

This historical record remains evidence for the merged #336 range only. It is complemented by the exact-head receipts, canonical runs, terminal artifacts, and post-merge accepted-main verification recorded above; the failed `30695885514` is not retroactively treated as green.

## Packet PRODUCT-OUTPUT-RESTART-RECOVERY-REPAIR-1

**State:** `COMPLETE`

**Accepted evidence:** PR #342 merged at `e1e08ddcb745b02892f099b9de1436c99c25d533` from exact head `666cabeab31c14c77389646edc140c2d8ae7eb86`; canonical run `30710854561` passed the complete source matrix, PG parity/PE-6 owner evidence, and terminal context capsule `91399051309`. The complete-diff review receipt covers the cumulative range `d5662d55..666cabea`; the final independent delta review covers `4b5f8d42..666cabea` and found no unresolved objections.

Repair the ProductTask output path so a known pre-effect GitHub credential failure marks the existing output operation `failed_known` through `LocalProductStore`, leaves ProductTask status/version unchanged, and can be reclaimed after restart with the same immutable operation/request identity. Add deterministic SQLite/PostgreSQL coverage for monotonic versions, current-version terminal CAS, canonical key/principal reissuance and scope restoration, concurrency/idempotency, duplicate-effect refusal, outcome-unknown/late-response refusal, target/approval/artifact binding, lease/spend/delegation terminalization, and workspace cleanup. No direct SQL mutation, manual version rollback, second identity store, or replacement output operation is permitted.

## Packet PE7-PRODUCT-GOLDEN-PATH-DEEPSEEK-LIVE-SEAL-1

**State:** `COMPLETE`

**Prerequisite:** Packets `CI-EVIDENCE-AND-GOVERNANCE-CLOSEOUT-REPAIR-1` and `PRODUCT-OUTPUT-RESTART-RECOVERY-REPAIR-1` accepted and merged. Satisfied by PRs #339/#340, #342, and the #346 Packet B closeout repair.

**Authority record:** owner authorization `GOLDEN-PATH-RECOVERY-AND-CLEAN-RESEAL-20260801` allowed at most three separately consumed attempts. Clean reseal attempt-1 was consumed and terminalized successfully. Attempt-2 and attempt-3 remain unused and confer no authority for additional reseal execution after this accepted completion without a new planning-layer decision.

Before any provider request, persist and display the complete derived final manifest, recalculate its SHA-256, verify the delegated approval/spend receipt binds that exact hash, confirm accepted Harness and target main SHAs are unchanged, and report only that the parent-held credential is present. Never print, persist, request, unset, rotate, or forward it to a child.

The sole target is `Igzela/alters-lab/main`, resolved and bound to an exact SHA immediately before each final manifest. Execute Pro planning, Flash bounded implementation, deterministic verification, and Pro review; neither model receives spend, approval, output, merge, release, or deployment authority. The only target output is one new unmerged `acp/*` Draft PR and target main must remain unchanged. Provider route, credential boundary, retry, branch/PR, and forbidden-action limits are those in the owner authorization above.

### First live observation (recorded truthfully)

The owner-authorized one-use live attempt executed a real `Pro → Flash → deterministic verify → Pro` workflow. It did not produce a clean accepted seal. The truthful observation is:

- real Pro planning, Flash bounded implementation, deterministic verification, and Pro review;
- target `Igzela/alters-lab` main unchanged;
- one unmerged `acp/*` Draft PR produced: alters-lab PR #3 `acp/product-ptask-20260731064541-18c74ce920c84d22` at head `8700e783eb7c30af10c822ca56403d8e94ae95bd`, base `6240768506320a324d68787b9eaa86971c8c930c`, one-line `docs/USER_GUIDE.md` change only;
- realized cost approximately `$0.0016`;
- one-use spend expired, attempt lease closed, workspace cleaned;
- realized workflow sample labeled `INSUFFICIENT_REPETITIONS` (not success probability, VDE, or RWE evidence);
- the provider-stage binary came from a dirty tree and did not equal an accepted commit or the final #325 head;
- terminal closeout used later local CAS-rebind code (the code accepted in PR #325).

PR #325 is the accepted live-observed repair for budget reservation release and Draft PR terminal CAS rebind.

### Second live observation (recorded truthfully)

The prior `Igzela/alters-lab#4` attempt remains `LIVE_OBSERVED_NOT_ACCEPTED`. Its realized evidence is preserved: three provider requests, 6,976 realized tokens, realized cost `$0.00143`, target main unchanged, one new unmerged Draft PR, and terminal task state `succeeded` after manual recovery. The recovery directly modified SQLite authority, manually reduced the ProductTask version from v8 to v7, manually reapplied scopes, and retried `/output`; those actions invalidate clean acceptance because canonical restart recovery failed. PR #4 and its branch must not be reused for a clean attempt.

### Clean reseal attempt-1 — accepted terminal evidence

The clean post-#346 exact-main reseal satisfied the previously remaining acceptance condition:

- Harness accepted commit: `a1878b2a282303d6e187f35c437875493c0f5296`;
- accepted source tree: `2280632259cc78bf9e252dfded39cccd20cd99d7`;
- runtime binary SHA-256: `3d6322ed046ebe217456d933f762d336770680ec37ab357970600a2313b53017`;
- ProductTask: `ptask-20260802061735-18c7e889a3570d82`, version `9`, status `completed`;
- run: `run-0001`, status `completed`;
- attempt: `attempt-pe7-live-seal-1785651576`;
- delegation: `delegation-pe7-live-seal-1785651576`;
- target repository/base: `Igzela/alters-lab` / `6240768506320a324d68787b9eaa86971c8c930c`;
- output: one OPEN, Draft, unmerged PR #5 at exact head `967c902487edf3959090e76c442092f75b0ba10a`;
- provider sequence: Pro planning → Flash bounded implementation → deterministic verification → Pro review;
- provider requests: `3`; retries: `0`; realized tokens: `6800`;
- reconciled client-side realized cost: `$0.000583862`;
- terminal receipt SHA-256: `205ad9539155bb0100709e88a16360ac9a5325df42449206e4ce0345f2955477`;
- evidence-index SHA-256: `d9a8e5b8be5dcfe35a9c3cb70a50a2a9a06d26c701ab5a854400bad19c5de82c`;
- exact-head independent receipt: `Igzela/alters-lab#5` comment `5158092741`, PASS;
- spend authorization and delegation expired, attempt lease closed, workspace cleanup/rollback completed;
- target main remained unchanged.

This is one realized workflow sample labeled `INSUFFICIENT_REPETITIONS`. It is not an RWE baseline, success-probability estimate, reliability result, economic comparison, ROI result, learning result, release/deployment proof, installation authority, target-main authority, or merge approval.

### Completion boundary

The previously remaining clean exact-main live-reseal condition is satisfied by the accepted attempt above. This packet becomes accepted `COMPLETE` only when the canonical-document closeout PR containing this record:

- passes a complete exact-head independent documentation review;
- passes canonical trusted `docs_only` CI, including the required terminal `context-capsule`;
- merges to `main`;
- is followed by a refreshed accepted-main handoff.

Until that merge, branch-local `COMPLETE` prose is proposed state only. Completion authorizes neither RWE execution nor economic, reliability, learning, ROI, success-probability, release, deployment, installation, target-main, auto-merge, or PR #5 merge claims.

## Packet TOOL-REVIEW-TRANSPORT-IDEMPOTENCY-REPAIR-1 — non-authoritative independent-review transport hardening

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** `PE7-PRODUCT-GOLDEN-PATH-DEEPSEEK-LIVE-SEAL-1` accepted `COMPLETE`.

**Goal:** make repository-controlled independent-review request construction, delivery reconciliation, verdict parsing, and GitHub receipt commenting idempotent, live-validated, single-process, recoverable, and fail closed.

**Owned paths:**

- `scripts/agent-control/review_loop/`
- one thin repository CLI entrypoint under `scripts/agent-control/`
- `tests/test_review_loop.py`
- the smallest required packet-state documentation

**Required behavior:**

- canonical request envelope and visible `Review-Request-SHA256`;
- live PR base/head/state and evidence-index validation before delivery;
- `ALREADY_PRESENT`, `SENT_CONFIRMED`, and `DELIVERY_OUTCOME_UNKNOWN` semantics;
- no resend after an unknown effect until the thread is reconciled;
- per-chat single-process lock and owned-process cleanup only;
- append-only local journal plus rebuildable non-authoritative projection;
- strict versioned receipt parsing; only exact structured `PASS` with matching identities and no objections is acceptable;
- exact-head revalidation before posting;
- idempotent ordinary GitHub comment posting with conflict detection;
- provider-free CI using fakes/fixtures only.

**Forbidden:**

- changes under `engine/`, `wire_contract/`, migrations, schema, auth scopes, ProductTask/store/scheduler/provider/output owners;
- a second runtime, state, authority, approval, budget, audit, rollback, merge, release, or deployment owner;
- ChatGPT login, real ChatGPT traffic, provider calls, credentials, cookies, or browser profiles in CI;
- GitHub APPROVE, mark-ready, merge, release, deployment, protected-branch writes, or automatic canonical-document state changes;
- treating malformed, partial, conflicting, `PASS_WITH_NOTES`, `NEEDS_CHANGES`, `UNREVIEWABLE`, or outcome-unknown results as PASS.

**Acceptance:** focused deterministic tests, full canonical source matrix, one bounded operator smoke outside CI, exact-head independent review, successful terminal context-capsule, rollback by revert/removing the local launcher, and a separate canonical closeout before `COMPLETE`.

## Packet TOOL-LOCAL-LOOP-CONTROL-PLANE-1 — outbound local worker and durable engineering loop

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** `TOOL-REVIEW-TRANSPORT-IDEMPOTENCY-REPAIR-1` accepted `COMPLETE`. Work may proceed on a separate Draft PR, but it must not merge or claim acceptance first.

**Goal:** replace host-pushed public-repository agent execution with a thin outbound local adapter while preserving GitHub as the durable task/evidence exchange layer and reusing all existing controller owners.

**Owned paths:**

- `scripts/agent-control/local_loop.py` and `scripts/agent-control/loopctl.py`;
- the smallest required extensions to existing `state_manager`, dispatcher, worktree, artifact, PR-binding, and workflow adapters;
- `tests/test_agent_local_loop.py` and the smallest required canonical documentation.

**Required behavior:**

- `poll` is one read-only bounded step, not an internal `while true`; it validates identity, control state, exact main/task binding, scope, dependencies, PR association and active capacity, then admits only a deterministic batch of mutually non-overlapping tasks;
- `run-once` obtains one GitHub-serialized lease/attempt before local mutation, then creates one verified isolated worktree and one fresh model session;
- Issue/PR text and model output remain untrusted data; only repository-owned reviewed commands execute, and GitHub/API/cloud credentials never enter the model child;
- model changes become a bounded patch artifact, are independently revalidated against exact base/scope/preimage, and may produce only one Issue-bound Draft PR;
- every externally visible transition is restart-reconciled; timeout, cancellation, process death, unknown delivery/output, stale main/head, lost lease, duplicate claim, invalid artifact, failed checks, or exhausted repair fails closed to a truthful non-success state;
- CI, review, repair, merge eligibility, and terminal evidence remain owned by the existing GitHub workflows/state contracts. Implementation and review sessions are always distinct;
- a stateless supervisor may run admitted `run-once` processes concurrently; GitHub owns queue/lease state and overlapping scopes serialize;
- after parity is proved, public-repository self-hosted jobs are retired rather than left as a second execution path.

**Current tracer bullet:** `repo-agent-loop-poll.v1` and `loopctl poll` implement deterministic, provider-free, non-overlapping batch admission bounded by the single canonical active-capacity owner `state_manager.MAX_ACTIVE` (K=2); `poll --max-active` throttles locally to 1..K only. They do not yet claim, invoke a provider, create a worktree, push, or open a PR and therefore do not authorize self-hosted cutover.

**Forbidden:** a local authoritative state database, arbitrary Issue shell, provider calls in CI, credential inheritance into the child, fork-authored tasks, unbounded polling/retry/concurrency, direct default-branch writes, non-Draft output, self-review, auto-merge, release, deployment, or parallel state/artifact/PR/CI/review/merge owners.

**Acceptance:** provider-free unit/integration tests for every transition and crash point; bounded owner-operated `poll` and separately authorized `run-once` smoke; exact-head Draft PR, canonical CI including terminal context capsule, independent DeepSeek review with no unresolved objections, rollback by disabling the local adapter and reverting the packet, and evidence that the legacy public self-hosted execution path is no longer simultaneously active.

## Packet PE7-REAL-WORKLOAD-EVIDENCE-1 — first bounded baseline

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** `PE7-PRODUCT-GOLDEN-PATH-DEEPSEEK-LIVE-SEAL-1`, `TOOL-REVIEW-TRANSPORT-IDEMPOTENCY-REPAIR-1`, and `TOOL-LOCAL-LOOP-CONTROL-PLANE-1`

PR #300 may prepare provider-free corpus, authorization, runner, and evidence contracts, but live RWE requires the accepted Golden Path terminal evidence, accepted review-transport repair, a frozen operator-supplied real corpus/protocol, and a separately persisted one-use RWE spend envelope.

Before execution, freeze a real, versioned, hash-bound, replayable `rwe_economic_corpus.v1`-class contract. Each task binds exact source repository/commit, task definition/reference, allowed mutable surface, verification, expected class, output bounds, timeout/cancel behavior, executor identity, budget, cleanup, primary value basis/source/confidence, layered acceptance rubric, reviewer policy, minimum repetitions, budget points, stop rules, non-inferiority margins, cost-completeness requirements, seeds, and statistical method.

Fixture authority corpora remain separate and cannot establish task value or economic performance. Different value bases remain separate unless a pre-registered versioned conversion contract exists.

The baseline records layered success, failure class, request/retry/token/latency/cost-source semantics, timeout/cancel/pause/kill/restart/outcome-unknown, SQLite/PostgreSQL parity, approval/output/target-main/Draft-PR/terminal evidence, realized lifecycle cost, review/rework/recovery evidence, evidence-sufficiency state, and the implementation-cost receipt.

The baseline may report raw observations and uncertainty. It must not claim `COMPARISON_ELIGIBLE` until minimum repetitions and cost completeness are satisfied.

## Packet PE7-ARCHITECTURE-CONVERGENCE-1 — compatibility convergence

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-REAL-WORKLOAD-EVIDENCE-1

Implement incrementally:

1. AC1 unified process supervision.
2. AC2 typed execution boundary.
3. AC3 Golden Path responsibility split.
4. AC4 transaction-scoped domain views.
5. AC5 runtime composition.
6. AC6 Rust-authoritative API/SDK/Dashboard schema convergence.
7. AC7 obsolete-abstraction cleanup after all callers and evidence migrate.

Each packet changes one coherent ownership boundary, preserves compatibility and rollback, and records implementation cost. It must not create a second scheduler, store, budget, approval, output, evidence, VDE, or rollback owner.

## Packet PE7-REAL-WORKLOAD-EVIDENCE-REPLAY-1 — post-convergence comparison

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-ARCHITECTURE-CONVERGENCE-1

Replay the identical frozen corpus, source identities, verifier, reviewer policy, value basis, budget grid, seed set, stop rules, and statistical method. Compare layered success/failure classifications, reliability, request/retry/token/latency/cost evidence, restart/recovery, approval/output/terminal behavior, realized lifecycle cost, review/rework burden, implementation cost, maintenance surface, rollback burden, LCAP, human-relative saving when comparable, and the lifecycle-cost Pareto frontier. Do not tune the corpus, thresholds, or reviewer policy from convergence results.

## Packet PE7-HARNESS-EVOLUTION-LEVEL2-GENERATIONAL-CONTROLLER-1 — bounded multi-generation decision

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-REAL-WORKLOAD-EVIDENCE-REPLAY-1

First record an evidence-backed GO/NO-GO. GO requires all hard gates, pre-registered quality and reliability non-inferiority, comparable value semantics, `COMPARISON_ELIGIBLE` evidence, uncertainty-aware VDE/Pareto improvement, and no unacceptable review/rework/recovery/maintenance/authority/rollback regression. A scalar index cannot independently satisfy GO.

On GO only, implement a default-off bounded laboratory controller with small fixed generation/candidate/evaluation limits, deterministic global budgets, one selected laboratory parent per generation, restart/lease/concurrency/exactly-once evidence, sealed-evaluator separation, and SQLite/PostgreSQL parity.

It may not modify `main`, merge, deploy, change the active production Harness, rewrite its evaluator, expand its own permissions, or continue across runs without explicit authority.

## Packet PE7-META-IMPROVER-EXPERIMENT-1 — separate unseen-task experiment

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HARNESS-EVOLUTION-LEVEL2-GENERATIONAL-CONTROLLER-1

Require pre-registered unseen tasks, immutable evaluator/labels, contamination controls, baselines, statistical/effect/error thresholds, seeds, budgets, stop/rollback rules, and immutable active-Harness identity. A NO-GO result is valid completion.
