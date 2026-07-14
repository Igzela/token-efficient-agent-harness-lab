# Next Decision

Last updated: 2026-07-14.

## Current Direction

The immediate objective is `PR207-SMOKE-REPAIR-1`, followed by `PR207-SMOKE-VERIFY-1`. The repository-maintenance orchestrator is merged, but the first live GPT Web smoke reached the Vader worker and ended blocked before branch or PR creation. Production repository tasks must not use this path until the demonstrated failure is repaired and a replacement smoke completes.

After the repository-agent path is accepted, resume the existing integration-repair sequence:

1. `PR207-SMOKE-REPAIR-1` — diagnose and repair the blocked Vader worker path while preserving emergency stop;
2. `PR207-SMOKE-VERIFY-1` — repeat a bounded one-file smoke through PR, exact-head CI, and independent review with auto-merge disabled;
3. `PE2-RUNTIME-PRODUCER-1` — connect owner-backed usage evidence to forecast/anomaly artifacts;
4. `PE4-EVIDENCE-ENTRY-1` — connect trace-backed replay to the safe evidence-chain promotion owner;
5. `TOOL-DISCOVERY-BENCH-1` — add a deterministic static-all versus retrieve-Top-K tool benchmark through PE-1 evidence owners.

Do not create another roadmap, phase, status, policy, or closeout document. This file is the normative forward plan. Current facts belong in `docs/CURRENT_STATUS.md`; ownership belongs in `docs/MODULE_MAP.md`.

## Verified Baseline

- PR #214 merged the PE-5/PE-6 post-seal repair at `0d8127e3d779e54c58caf5d93e7589dd1a6df616`;
- PR #207 merged the event-driven repository-maintenance orchestrator at `23187bb83dc32165d8982c79be1a1f7f818380a0`;
- PR #216 merged the Codex output and runner-readiness compatibility repair at `2a42c011164765ba6c2dbe940c5a73900a7bb4b1`;
- PR #216 exact head `7210cd1943b075ef07c561f4804bca8230cffd60` passed canonical CI run `29308693744` with all seven required jobs successful;
- the first GPT Web smoke Issue #217 was claimed, dispatched, and entered `agent-running`, then ended `agent-blocked` without an agent branch or PR;
- the issue evidence does not identify the exact failed workflow step or bounded terminal reason, so the root cause remains unresolved;
- Issue #208 is emergency-stopped with both enable labels absent;
- auto-merge was never enabled and no smoke PR was merged.

Every implementation session must refresh actual GitHub, Actions, runner, and local state before relying on these identifiers.

## Packet States

- `READY_FOR_EXECUTION` — prerequisites and contract are sufficient to begin;
- `BLOCKED_PREREQUISITE` — defined but waiting for an earlier packet or conflicting PR;
- `DECISION_REQUIRED` — a material authority or product decision cannot be derived safely;
- `IN_PROGRESS` — one active branch or PR owns the packet;
- `COMPLETE` — implementation is merged, required evidence is verified, and active documents are synchronized.

## Common Execution Protocol

Each packet must:

- start from the latest actual `main` or the exact existing PR branch specified by the packet;
- preserve existing runtime, storage, scheduler, audit, permission, pause, policy, rollback, release, and orchestrator control owners;
- use one focused branch and PR per implementation packet unless the packet explicitly owns an existing PR;
- define versioned bounded inputs, outputs, reason codes, identity bindings, and failure states;
- reject caller-supplied authority where evidence must be derived from existing owners;
- remain deterministic and fail closed on missing, stale, conflicting, tampered, oversized, or incompatible evidence;
- add tests that exercise real call paths and state transitions, not only string-presence or isolated constructors;
- run focused tests, the full applicable local baseline, and fresh exact-head GitHub CI for code/workflow changes;
- independently review the complete final diff after the last code change;
- keep auto-merge disabled unless separately and explicitly authorized;
- not merge without separate explicit user authorization.

Documentation-only factual corrections may use the targeted merge exception in `docs/REAL_WORLD_TESTING_PLAYBOOK.md`. Code, workflow, schema, configuration, dependency, release, or external-state changes require a branch, PR, and complete required CI.

## Hard Stops

Stop and report `BLOCKED` rather than improvising when:

- a secret, private signing key, recovery credential, raw prompt/output, transcript, or unredacted sensitive payload would enter version control or an artifact;
- required tests, CI, exact-head identity, review evidence, or a known failure would be hidden or fabricated;
- an irreversible external action lacks explicit authority and a tested recovery path;
- another active PR owns conflicting code that cannot be safely refreshed or separated;
- an existing authority, audit, compensation, control, or rollback owner would be bypassed or replaced;
- a destructive fault is not bounded to disposable resources;
- exact-head CI is failed, queued, in progress, cancelled, action-required, or unexpectedly skipped;
- a real public release, production installation, destructive external test, production provider call, protected-branch write, or persistent signing identity is needed without explicit authorization;
- Issue #208 is ambiguous, missing, malformed, or unexpectedly live while a repair requires emergency stop;
- a worker remains active without bounded progress or terminal evidence and late writes cannot be ruled out.

A stale document, a failed first implementation, or a bounded missing design detail is not itself a hard stop. Audit, repair, test, and continue when the result remains explicit, compatible, observable, and rollbackable.

## Packet PR207-REPAIR-1 — Repository-maintenance orchestrator implementation and compatibility repair

**State:** `COMPLETE`

**Goal:** Deliver the disabled-by-default GitHub Issues/Actions → Vader Codex → GitHub-hosted finalizer architecture and repair its known pre-deployment compatibility defects.

**Completion evidence:**

- PR #207 merged the orchestrator with exact-head CI and independent review;
- PR #216 corrected the false JSON assumption for Codex last-message output and replaced unsupported `config.sh --check` readiness logic;
- the registered Vader runner, scoped `AGENT_PUSH_TOKEN`, and preflight were reported ready before the live smoke;
- the control Issue retained a separate emergency-stop, orchestrator-enable, and auto-merge-enable contract.

This packet establishes the implementation baseline only. Live end-to-end acceptance is owned by the following smoke packets and is not implied by this `COMPLETE` state.

## Packet PR207-SMOKE-REPAIR-1 — Diagnose and repair the blocked live worker path

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** PR207-REPAIR-1

**Goal:** Determine why smoke Issue #217 entered the Vader worker and then became `agent-blocked` before branch/PR creation, repair the actual root cause, and make future failures bounded, attributable, and capacity-safe.

**Observed evidence:**

- Issue #217 had one valid narrow path and was accepted by intake;
- dispatcher state recorded `claimed` and `dispatched` for `agent-worker.yml`;
- the Issue reached `agent-running` and later `agent-blocked`;
- no `agent/issue-217` branch, PR, exact-head CI binding, or durable terminal reason was produced;
- Issue #208 has been returned to emergency stop.

**Required investigation:**

- retrieve the exact `agent-worker` workflow run and every job/step conclusion;
- inspect bounded GitHub job logs and Vader runner diagnostics without publishing prompts, raw model output, credentials, or unrelated environment data;
- distinguish queue/runner loss, setup failure, worktree failure, prompt construction, Codex timeout/nonzero exit, artifact construction/upload, finalizer validation, PAT push, PR binding, CI acquisition, and control-state interruption;
- do not guess from the terminal Issue label alone.

**Required behavior:**

- keep Issue #208 emergency-stopped and both enable labels absent throughout the repair PR;
- add explicit bounded timeout ownership to the Vader/Codex path so one worker cannot remain unbounded;
- persist or expose a bounded workflow run identity, failed phase, fixed reason code, and terminal capacity-release result through trusted GitHub-hosted evidence;
- ensure unexpected cancellation, runner loss, timeout, nonzero Codex exit, malformed artifact, finalizer failure, and emergency stop each end in a deterministic non-active state;
- preserve raw-output suppression, credential isolation, narrow artifact scope, exact-base/final-head binding, and GitHub-hosted mutation authority;
- do not weaken the successful Codex text-boundary or runner-readiness repair from PR #216;
- do not enable auto-merge or run another live smoke from the repair branch.

**Verification:** focused tests for each terminal failure class, timeout behavior, missing/partial run evidence, idempotent release, late completion after stop, duplicate delivery, and secret/output suppression; full orchestrator suite; workflow YAML parsing; security baseline; exact-head canonical seven-job CI; independent complete-diff review.

**Completion:** one focused repair PR is merged with the exact #217 root cause recorded, all required CI green, Issue #208 still emergency-stopped, and no production task or replacement smoke dispatched during implementation.

**Rollback:** restore/add emergency stop, remove both enable labels, disable the affected workflow entry point if necessary, and revert the repair PR while retaining bounded GitHub evidence.

## Packet PR207-SMOKE-VERIFY-1 — Repeat bounded GPT Web end-to-end smoke

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PR207-SMOKE-REPAIR-1

**Goal:** Prove that GPT Web can translate a normal-language repository request into a bounded Issue and drive the complete orchestrator chain without requiring the user to remember workflow parameters.

**Required sequence:**

```text
GPT Web natural-language request
→ bounded Agent Task Issue with exact allowed path
→ agent-ready intake and unique dispatch
→ Vader Codex isolated worktree
→ bounded validated artifact
→ GitHub-hosted branch push and Issue-bound PR
→ exact-head canonical seven-job CI
→ independent schema-valid review
→ manual merge decision only
```

**Smoke scope:** create or update one disposable documentation file only. The Issue must forbid every other path, controls, workflows, credentials, source code, and merge action.

**Acceptance:**

- one and only one agent branch and PR are bound to the smoke Issue;
- changed files exactly equal the allowed path set;
- worker/finalizer evidence contains bounded run and phase identity but no raw model output or secret;
- exact-head CI has all seven required jobs successful;
- independent review is durably bound to the same head;
- auto-merge remains disabled and the smoke PR remains open until a separate user decision;
- Issue #208 and the task end in explicit non-active states;
- README/AGENTS natural-language usage claims match the demonstrated path.

**Failure:** immediately emergency-stop on any scope mismatch, duplicate dispatch, stale binding, credential exposure, missing terminal evidence, unexpected write, or inability to prove exact-head identity.

**Completion:** the smoke evidence is reviewed and accepted, the production-use restriction is removed from active documents, and normal GPT Web requests may use the repository agent with auto-merge off by default.

## Packet PE2-RUNTIME-PRODUCER-1 — Connect budget evidence production

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PR207-SMOKE-VERIFY-1

**Goal:** Derive and persist forecast/anomaly evidence from existing posted owner data so the already-implemented read, Dashboard, operator-decision, auto-pause, and recovery paths receive real artifacts.

**Required chain:**

```text
provider audit and workflow usage owners
→ bounded observation projection
→ build_budget_forecast / detect_budget_anomaly
→ record_budget_forecast_evidence / record_budget_anomaly_finding
→ existing read/API/Dashboard surfaces
→ existing policy-gated pause and recovery owners
```

**Required behavior:**

- choose an existing runtime, explicit operator API, or bounded scheduled owner; do not create a second scheduler;
- derive timestamps, scope, run/workspace/provider/model identity, token counts, cost, retry, latency, and context evidence from persisted owners rather than caller assertions;
- preserve exact `(evidence_type, evidence_id)` identity, content hashes where present, deduplication, freshness, sparse-evidence, mixed-dimension, pricing-completeness, and invalid-evidence behavior;
- persist Supported, InsufficientEvidence, and InvalidEvidence outcomes when the contract permits them; never manufacture Supported evidence;
- make repeated production requests idempotent by canonical evidence identity/hash;
- ensure pause mutation still requires supported, fresh, high-confidence, policy-eligible anomaly evidence and explicit existing permissions/confirmation;
- expose no new generic mutation authority.

**Verification:** SQLite and PostgreSQL end-to-end tests from posted usage → artifact → read API → operator queue/pause consumer; sparse-but-dimension-complete, invalid, duplicate, stale, wrong-scope, incomplete-pricing, and retry/idempotency cases; full CI.

**Completion:** a production caller exists, call-site search proves it is not test-only, and generated artifacts are consumable by existing owners without manual fixture insertion.

## Packet PE4-EVIDENCE-ENTRY-1 — Connect replay and safe promotion

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE2-RUNTIME-PRODUCER-1

**Goal:** Add one explicit, permissioned owner path that derives and persists trace-backed offline replay evidence, derives shadow and canary bindings, and invokes the existing evidence-chain promotion owner.

**Required chain:**

```text
owner-backed dispatch_history provenance
→ replay eligibility
→ record_offline_replay
→ ShadowRouter::compare_replay_report
→ bounded canary evidence
→ explicit confirm + permission
→ promote_adaptive_fusion_policy_with_evidence_chain
→ existing snapshot/apply/rollback owner
```

**Required behavior:**

- reuse `policy_replay_contract.v3`, `trace_replay_evidence.v2`, `offline_policy_replay.v2`, dispatch-history trace ownership, calibration, coverage, OOD, shadow, canary, policy snapshot, compensation, and rollback contracts;
- do not permit callers to supply `eligible`, accepted observations, coverage, calibration, content hashes, or current-state authority;
- keep replay read-only until the separate promotion mutation step receives exact current-state binding, confirmation, and permission;
- preserve existing rollback-target and active-policy hash checks;
- retain the old observation-only auto-promotion path as fail-closed or remove its unreachable mutation claim; do not restore it as a shortcut;
- expose bounded read/report evidence for operator inspection before mutation.

**Verification:** end-to-end owner traces → replay artifact → shadow → canary → blocked/accepted promotion; stale/tampered/uncalibrated/OOD/coverage/current-policy/rollback mismatch; concurrent or changed active-policy state; SQLite/PostgreSQL atomicity and compensation; API permission and confirmation tests; full CI.

**Completion:** `record_offline_replay` and `promote_adaptive_fusion_policy_with_evidence_chain` each have a production caller, and no production caller reaches a promotion from observation summaries alone.

## Packet TOOL-DISCOVERY-BENCH-1 — Static-all versus retrieve-Top-K benchmark

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE4-EVIDENCE-ENTRY-1

**Goal:** Measure whether deterministic retrieval of a bounded Top-K tool set reduces tool-schema context while preserving required-tool availability and task quality.

**Scope:** benchmark/evidence only. This packet does not authorize dynamic production tool loading or execution.

**Required design:**

- use existing tool descriptors/capabilities as the canonical corpus;
- define a versioned scenario registry with task text, required/acceptable tool identities, forbidden tools, static-all baseline, retrieval method/version, K, deterministic tie-breaking, and quality method;
- implement a deterministic retriever or retrieval adapter with bounded corpus, query, K, output size, and no hidden network/provider dependency in CI;
- run paired static-all and retrieve-Top-K variants under equivalent tasks and quality criteria;
- record required-tool recall, selection precision, irrelevant-tool count, schema/context bytes or tokens, total input/output tokens, latency, cost when trustworthy, task success, quality, and failure reasons;
- bind corpus hash, descriptor identity/version, scenario registry hash, retriever version/config, selected tool IDs/order/scores, and scorecard evidence;
- feed results through existing PE-1 scorecard, report, batch, trend, API, SDK, and Dashboard owners where compatible; extend the existing versioned evidence contract only when required, with compatibility tests;
- fail closed on missing required tools, nondeterministic ordering, duplicate identity, stale corpus binding, tampered selection, incomparable variants, or quality regression.

**Verification:** deterministic repeated retrieval; tie behavior; K bounds; required-tool miss; distractor-heavy corpus; static baseline parity; scorecard/report tamper; registry/corpus change sensitivity; no raw prompt/output/credential/private-path leakage; full CI.

**Completion:** one checked representative scenario set demonstrates a reproducible paired comparison, but no production routing or tool-execution authority changes.

## Preserved Boundaries

- Rust `engine/` remains the only runtime/API/storage authority.
- Existing SQLite/PostgreSQL owners remain authoritative; no second database or event store.
- Existing workflow scheduler, pause/recovery, adaptive policy, release, audit, target-output, and orchestrator control owners remain authoritative.
- Vader remains artifact-only; GitHub-hosted finalizers own branch/PR/label/comment mutations.
- The local runner workflow remains Stub/Fake-only; live provider execution remains explicit local operator/CLI work.
- PE-5 production release verification requires v2 evidence and immutable attested bootstrap assets; legacy v1 remains fixture-only and non-authorizing.
- PE-6 faults remain allowlisted and disposable; unsupported environments report unsupported rather than pass.
- Real tags, public releases, production installation, destructive external faults, production provider calls, target-repository mutation outside the bounded orchestrator, and persistent signing secrets require separate explicit authorization.

## Final Reporting Contract

For each packet report:

- actual starting `main`, branch, PR, and exact final head;
- files changed and owner paths reused;
- production call sites added or corrected;
- focused and full local test commands/results;
- exact-head CI run and all required job results;
- independent review findings and repairs;
- compatibility, security, rollback, and residual risks;
- control Issue labels and whether auto-merge was enabled;
- whether the packet is `COMPLETE`, `BLOCKED`, or `MERGE_READY`;
- no merge unless separately authorized.
