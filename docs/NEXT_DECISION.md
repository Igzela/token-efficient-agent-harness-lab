# Next Decision

Last updated: 2026-07-13.

## Current Direction

The next objective is to close confirmed integration gaps in existing owners. It is not PE-7, does not create another runtime or control plane, and does not reopen already-correct contracts merely to rename them.

The execution order is:

1. `PR207-REPAIR-1` — refresh and independently validate the existing disabled repository-maintenance orchestrator, repairing any remaining defect;
2. `PE2-RUNTIME-PRODUCER-1` — connect owner-backed usage evidence to forecast/anomaly artifacts;
3. `PE4-EVIDENCE-ENTRY-1` — connect trace-backed replay to the safe evidence-chain promotion owner;
4. `TOOL-DISCOVERY-BENCH-1` — add a deterministic static-all versus retrieve-Top-K tool benchmark through PE-1 evidence owners.

Do not create another roadmap, phase, status, policy, or closeout document. This file is the normative forward plan. Current facts belong in `docs/CURRENT_STATUS.md`; ownership belongs in `docs/MODULE_MAP.md`.

## Verified Baseline

- PR #214 merged at `0d8127e3d779e54c58caf5d93e7589dd1a6df616`;
- PR #214 exact head `ed5e033a5206d2ddfea2d48381217d0a04b4ceb3` passed exact-head CI run `29250861586`;
- PR #207 is open and unmerged on `codex/agent-orchestrator-v1`; its pre-refresh head `06933e0e84f5c92956e9139608b2bfe354fcbeb2` and CI run `29223404792` are historical evidence only;
- the repair branch refreshed from starting `main` `d5354e6866e69cc2ce7c4d12258bfd3c828ce7c4` and preserves the active integration-repair documents;
- its final repaired head requires fresh exact-head seven-job CI, proof that the orchestrator suite and workflow parser ran, and an independent final-diff review; current operational evidence belongs in PR #207;
- PR #207 remains disabled and emergency-stopped.

Every implementation session must refresh actual GitHub and local state before relying on these identifiers.

## Packet States

- `READY_FOR_EXECUTION` — prerequisites and contract are sufficient to begin;
- `BLOCKED_PREREQUISITE` — defined but waiting for an earlier packet or conflicting PR;
- `DECISION_REQUIRED` — a material authority or product decision cannot be derived safely;
- `IN_PROGRESS` — one active branch or PR owns the packet;
- `COMPLETE` — implementation is merged, exact-head and required post-merge evidence are verified, and active documents are synchronized.

## Common Execution Protocol

Each packet must:

- start from the latest actual `main` or the exact existing PR branch specified by the packet;
- preserve existing runtime, storage, scheduler, audit, permission, pause, policy, rollback, and release owners;
- use one focused branch and PR per packet unless the packet explicitly owns an existing PR;
- define versioned bounded inputs, outputs, reason codes, identity bindings, and failure states;
- reject caller-supplied authority where evidence must be derived from existing owners;
- remain deterministic and fail closed on missing, stale, conflicting, tampered, oversized, or incompatible evidence;
- support SQLite and PostgreSQL wherever the owning storage path already supports both;
- add tests that exercise real call paths and state transitions, not only string-presence or isolated constructors;
- run focused tests, the full applicable local baseline, and fresh exact-head GitHub CI;
- independently review the complete final diff after the last code change;
- not merge without separate explicit user authorization.

Documentation-only factual corrections may be committed directly when explicitly authorized. Code, workflow, schema, configuration, dependency, release, or external-state changes require a branch and PR.

## Hard Stops

Stop and report `BLOCKED` rather than improvising when:

- a secret, private signing key, recovery credential, raw prompt/output, transcript, or unredacted sensitive payload would enter version control or an artifact;
- required tests, CI, exact-head identity, review evidence, or a known failure would be hidden or fabricated;
- an irreversible external action lacks explicit authority and a tested recovery path;
- another active PR owns conflicting code that cannot be safely refreshed or separated;
- an existing authority, audit, compensation, or rollback owner would be bypassed or replaced;
- a destructive fault is not bounded to disposable resources;
- exact-head CI is failed, queued, in progress, cancelled, action-required, or unexpectedly skipped;
- a real public release, production installation, destructive external test, production provider call, protected-branch write, or persistent signing identity is needed without explicit authorization.

A stale document, a failed first implementation, or a bounded missing design detail is not itself a hard stop. Audit, repair, test, and continue when the result remains explicit, compatible, observable, and rollbackable.

## Packet PR207-REPAIR-1 — Existing repository-maintenance orchestrator acceptance repair

**State:** `IN_PROGRESS`

**Owner:** existing PR #207 and its current branch. Do not create a replacement orchestrator or a second PR for this packet.

**Goal:** Refresh the branch from current `main`, independently verify that the disabled GitHub Issues/Actions → Vader self-hosted Codex → GitHub-hosted finalizer flow is executable, deterministic, capacity-safe, and exact-head bound, and repair any defect still present while preserving the emergency stop.

The current PR body claims the previously identified defects are repaired and its current exact-head CI is green. The following items are an acceptance checklist, not an assumption that each remains broken:

1. PR creation uses supported GitHub CLI/API behavior and resolves the exact PR number deterministically.
2. CI-repair Python invocation works from the actual workflow working directory.
3. Repair validates the failed old head before push and binds/verifies the new head after push.
4. `setup-controls` maps to a real idempotent control-state command.
5. PASS with auto-merge disabled and all non-PASS outcomes release active capacity into explicit non-running states.
6. The official Issue template contains a valid editable scope marker and invalid scope is rejected before Vader dispatch and at finalization.
7. PAT pushes reuse a naturally triggered exact-head canonical CI run; manual dispatch is a bounded fallback, with one authoritative CI binding per exact head.
8. The GitHub-hosted finalizer performs bounded deterministic post-patch validation before commit, or the documentation truthfully narrows the claim.

**Required tests:** execute or faithfully fake the actual shell/CLI and state-machine paths for all eight checklist items. Static substring checks are insufficient.

**Boundaries:**

- Vader remains artifact-only and does not own GitHub mutations;
- finalizers own branch/PR/label/comment writes;
- `AGENT_PUSH_TOKEN` is exposed only to the bounded push step;
- no OpenAI API key, GitHub App, Actions Variable control plane, or broad self-hosted credential is added;
- control Issue remains labeled `agent-emergency-stop` and activation remains disabled;
- no merge without separate user authorization.

**Completion:** refresh from current `main`; reconcile the active documents without deleting current integration-repair state; run fresh exact-head seven-job CI and prove orchestrator regression tests ran; independently review the complete final diff; report remaining Stage B runner/service-account prerequisites. Code-level blockers must be zero before reporting `MERGE_READY_DISABLED_ONLY`.

## Packet PE2-RUNTIME-PRODUCER-1 — Connect budget evidence production

**State:** `BLOCKED_PREREQUISITE` until PR #207 is resolved or its shared CI/document conflicts are explicitly separated.

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

**State:** `BLOCKED_PREREQUISITE` until `PE2-RUNTIME-PRODUCER-1` is complete or an explicit non-conflicting execution decision is recorded.

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

**State:** `BLOCKED_PREREQUISITE` until `PE4-EVIDENCE-ENTRY-1` is complete.

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
- Existing workflow scheduler, pause/recovery, adaptive policy, release, audit, and target-output owners remain authoritative.
- The local runner workflow remains Stub/Fake-only; live provider execution remains explicit local operator/CLI work.
- PE-5 production release verification requires v2 evidence and immutable attested bootstrap assets; legacy v1 remains fixture-only and non-authorizing.
- PE-6 faults remain allowlisted and disposable; unsupported environments report unsupported rather than pass.
- Real tags, public releases, production installation, destructive external faults, production provider calls, target-repository mutation, and persistent signing secrets require separate explicit authorization.

## Final Reporting Contract

For each packet report:

- actual starting `main`, branch, PR, and exact final head;
- files changed and owner paths reused;
- production call sites added or corrected;
- focused and full local test commands/results;
- exact-head CI run and all required job results;
- independent review findings and repairs;
- compatibility, security, rollback, and residual risks;
- whether the packet is `COMPLETE`, `BLOCKED`, or `MERGE_READY`;
- no merge unless separately authorized.
