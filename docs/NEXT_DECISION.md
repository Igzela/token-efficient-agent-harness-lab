# Next Decision

## Current Direction

Phase 8, V2 Real Production Output, Real Output Closeout, and Adaptive Fusion AF-0 through AF-7 are complete.

The Trusted Local Autonomous Execution Track (IAE) was approved on 2026-06-22. It authorizes the project and maintaining agents to move from fragmented opt-in execution toward a bounded trusted-local operating profile. That profile is now the recommended internal execution path.

IAE may change local defaults for provider execution, adaptive routing, experiments, automatic promotion, default routing, and supervised workers. It must not bypass protected auth, symbolic credential handling, budget/token/call/time/concurrency ceilings, provider/model identity, redaction, audit, snapshots, rollback, approval-bound target output, or kill switches. Missing prerequisites must fail closed.

IAE-1 through IAE-3 are implemented. `ACP_TRUSTED_LOCAL_PROFILE=1` validates protected auth, endpoint metadata, symbolic credentials, positive endpoint pricing, and per-dispatch/daily cost caps before activating the existing provider, adaptive routing, experiment, promotion, and default-routing gates. `ACP_TRUSTED_LOCAL_TASK_ADVANCEMENT=1` separately acknowledges bounded background advancement of already-created queued workflow runs through a pinned `adaptive_provider` worker. The dashboard/API expose effective authority, bounded operator evidence, existing pause/resume/kill controls, and policy rollback; invalid experiment or auto-promotion policy configuration fails closed.

On 2026-06-23 Charlie approved Full Agent Autonomy Mode for this repository. Maintaining agents may now autonomously propose, implement, test, review, merge, and iterate high-risk architecture work when the change is repo-scoped, testable, CI-gated, and rollbackable. This includes architecture direction changes, authority-boundary changes, default execution/profile changes, auth/security redesign, database migrations, release/tag/deploy workflow changes, target-output authority changes, and superseding accepted decisions.

## Stable Tracks

| Track | Status |
|---|---|
| Core dispatch kernel | Complete |
| Architecture refactor R-series | Sealed at R7; Full Agent Autonomy Mode may supersede this only through an explicit documented change |
| V2 Real Production Output | Complete through V2-5 |
| Real Output Closeout | Complete; `v0.1.0` published and installer verified |
| Adaptive Fusion AF-0 through AF-7 | Complete; current runtime gates remain implemented |
| Trusted Local Autonomous Execution | Complete through IAE-3 |
| Full Agent Autonomy Mode | Active for high-risk architecture, authority, migration, release/deploy workflow, and decision-supersession experiments |
| Agent Autonomous Maintenance Mode | Active for implementation, docs, tests, CI, review, and bounded shipping |

## Trusted Local Boundary

Current binaries support both the IAE-1 trusted-local profile and the legacy AF-6/V2 explicit gates. The profile fails closed unless it validates:

- protected authentication for live execution
- configured endpoint metadata and symbolic credential references
- per-request and daily budget controls
- token, call, timeout, and concurrency ceilings
- provider/model identity validation
- redaction and output caps
- circuit breakers and kill switches
- persistent audit events
- policy snapshots and rollback
- tests and CI before merge
- fail-closed startup and visible operator readiness

The token, call, timeout, concurrency, identity, redaction, audit, circuit-breaker, pause, kill, snapshot, and rollback controls remain enforced by their existing runtime modules. Runtime pause and kill state does not deconfigure the profile, so operators can recover without rebuilding the executor.

## Auto-Merge Policy

Auto-merge eligible: docs-only, tests-only, CI fix, low-risk code fix, high-risk architecture change, auth/security redesign, database migration, release/tag/deploy workflow change, target-output authority change, accepted-decision supersession, and default-execution/profile change, when the PR is green, scoped, reviewed by the maintaining agent, and has a rollback path.

Documentation-only factual, policy, or governance corrections may be committed directly to `main` after validation. Code and workflow changes may use PRs by default; under Full Agent Autonomy Mode, maintaining agents may merge green PRs without a separate per-PR confirmation when the change is testable and rollbackable.

## Autonomously maintain and evolve

Maintaining agents may autonomously audit, plan, implement, test, review, simplify, document, create PRs, repair CI, merge green work, and continue iterative architecture evolution. They may continue bounded internal execution, operator-surface, IAE work, release workflow work, database migration work, target-output work, and authority-boundary work without requesting approval again.

The purpose of this mode is to test how far agent-driven architecture and product engineering can progress under CI, repository history, rollback, and observable evidence.

## Hard Stops

Even under Full Agent Autonomy Mode, agents must not commit real secrets, intentionally hide failures, falsify test/CI evidence, remove rollback paths, or perform irreversible external destruction without a recovery path. Everything else is eligible for autonomous proposal, implementation, merge, and iteration when it is repo-scoped, testable, observable, and rollbackable.

## Architecture refactor (R-series)

The R-series is sealed at R7 as the current baseline. Full Agent Autonomy Mode may reopen or supersede this only by updating this file or a decision record with the new architecture direction and rollback path.

## Allowed Next Paths

- Autonomous maintenance: repair stale docs, CI breakage, test drift, and wire-codegen drift.
- Regression hardening: add or repair tests for existing behavior.
- Pilots: real-world task validation.
- V2 maintenance and V2 authority expansion experiments.
- Adaptive Fusion maintenance and adaptive routing authority experiments.
- Trusted-local execution evolution.
- Auth/security redesign.
- Database migration design and implementation.
- Release/tag/deploy workflow automation.
- Target-output authority expansion.
- Architecture refactor or architecture replacement.
- Accepted decision supersession when documented and rollbackable.

## Trusted Local Autonomous Execution Track

| Phase | Goal | Acceptance |
|---|---|---|
| IAE-0 | Governance and permission baseline | **Complete** — trusted-local expansion authorized; safety invariants and fail-closed prerequisites recorded |
| IAE-1 | Trusted-local execution profile | **Complete** — one fail-closed profile activates existing provider/adaptive/experiment/promotion/default-routing capabilities after auth, endpoint, credential, positive pricing, and cost-cap validation; legacy flags remain compatible and runtime safety controls remain authoritative |
| IAE-2 | Bounded autonomous task advancement | **Complete** — an explicit trusted-local acknowledgement enables the existing scheduler to advance already-created queued runs through a pinned adaptive-provider executor; invalid worker configuration fails closed and existing task, time, cost, token, call, concurrency, identity, audit, redaction, pause, and kill controls remain authoritative |
| IAE-3 | Operator control and evidence | **Complete** — dashboard/API expose effective authority, spend/traffic/worker ceilings, safe observation aggregates, scheduler pause/resume/kill state and controls, redacted recent audit actions, and existing policy rollback without secrets or raw model/repository content; invalid experiment, promotion, or rollout configuration is visible and fails closed |

IAE implementation should extend existing modules unless Full Agent Autonomy Mode selects and verifies a replacement architecture.

## V2 Status

V2 remains the controlled real-output path:

```text
connect repo -> create task -> app-owned workspace execution
-> verification -> evidence -> approval -> patch or PR branch output
```

V2 implementation is complete through V2-5. Under Full Agent Autonomy Mode, future work may expand, replace, or merge V2 with adaptive execution if the resulting behavior is tested, documented, observable, and rollbackable.

## Adaptive Fusion Track

Human approval on 2026-06-21 authorizes an adaptive multi-provider/model routing track inspired by Auto Router, Fusion deliberation, provider performance routing, and this repository's existing feedback/regulator loop.

AF-0 through AF-7 are complete.

| Phase | Status |
|---|---|
| AF-0 | Shadow portfolio planning contract implemented |
| AF-1 | Model endpoint registry implemented |
| AF-2 | Offline evaluation and replay implemented |
| AF-3 | Explicit bounded adaptive execution implemented |
| AF-4 | Contextual policy improvement implemented |
| AF-5 | Operator UX implemented |
| AF-6A | Deterministic candidate generator implemented |
| AF-6B | Bounded parallel panel execution implemented |
| AF-6C | Safe observation capture implemented |
| AF-6D | Controlled online experiments implemented |
| AF-6E | Evidence-driven auto promotion implemented |
| AF-6F | Guarded adaptive completion API implemented |
| AF-7 | Operator surface for AF-6 implemented |

## AF-6 Auto Fusion Plan

Planning baseline: retained in `docs/archive/`.

AF-6 target flow:

```text
completion request or explicit workflow tick
-> classify task context/objective/risk
-> generate eligible provider/model candidates
-> select single, fallback, or fusion candidate
-> execute bounded live plan
-> judge and synthesize when using fusion
-> return output
-> persist observation summary
-> update evidence aggregates
-> promote better policy when thresholds pass
-> rollback or kill if gates trip
```

AF-6 implementation slices:

| Slice | Goal | Acceptance |
|---|---|---|
| AF-6A | Candidate generator | **Complete** — deterministic single/fallback/fusion candidates from configured endpoints; bounded IDs, hashes, costs, capabilities, aggregate caps, duplicate detection, and model bindings |
| AF-6B | Parallel panel execution | **Complete** — panel calls run concurrently with a bounded cap; judge and synthesizer remain serial; quorum, timeout, identity, token, cost, audit, redaction, and kill behavior remain enforced |
| AF-6C | Online observation capture | **Complete** — safe summaries feed contextual scoring without raw prompts, outputs, transcripts, secrets, repository content, or private paths |
| AF-6D | Continuous experiments | **Complete** — deterministic traffic allocation with risk, budget, token, call, time, concurrency, pause, and kill controls; activated by a ready trusted-local profile or standalone legacy gates |
| AF-6E | Automatic promotion | **Complete** — evidence thresholds, confidence/regression guards, hash-bound snapshots, rollout, stale-evidence rejection, and rollback; activated by a ready trusted-local profile or standalone legacy gates |
| AF-6F | Adaptive completion API | **Complete** — authenticated `POST /api/v1/adaptive-fusion/completions`; compact metadata-hidden responses; default `/dispatch` routing enabled by a ready trusted-local profile or the standalone legacy gate |

## AF-7 Operator Surface

AF-7 exposes AF-6 to operators through the existing local dashboard only:

- completion test panel for `POST /api/v1/adaptive-fusion/completions`
- optional routing metadata display for candidate, policy, experiment, and observation IDs
- read-only gate status for provider execution, adaptive execution, auth, default routing, experiments, auto promotion, pause, and kill switches
- experiment/promotion status summary and rollback snapshot counts
- kill switch and rollback cues without adding new mutation authority
- secret-safe provider endpoint configuration for `stub`, `openai_compatible`, and `anthropic` endpoint metadata; a protected legacy adaptive runtime may start fail-closed without endpoint metadata so operators can bootstrap through the dashboard/API, dashboard edits persist symbolic credential environment names only, reject missing credential environment variables for real providers, apply validated local config to the adaptive completion API without restart, and restore the startup-bound workflow/scheduler executor on the next process start; explicit endpoint JSON remains authoritative while present

AF-7 does not add provider execution authority, default-on routing, new target-output behavior, DB migrations, release/deploy controls, unattended workers, or policy mutation outside existing guarded endpoints. Full Agent Autonomy Mode may supersede these limits through a documented, tested, rollbackable change.

Implemented AF-6 gates:

```text
ACP_ENABLE_ADAPTIVE_FUSION_EXECUTION=1
ACP_ENABLE_ADAPTIVE_EXPERIMENTS=1
ACP_ADAPTIVE_EXPERIMENTS_ACTIVE=1
ACP_ENABLE_ADAPTIVE_AUTO_PROMOTION=1
ACP_ADAPTIVE_AUTO_PROMOTION_ACTIVE=1
ACP_ADAPTIVE_DEFAULT_LIVE_ROUTING=1
ACP_ADAPTIVE_FUSION_KILL_SWITCH=1
ACP_ADAPTIVE_EXPERIMENTS_KILL_SWITCH=1
ACP_ADAPTIVE_AUTO_PROMOTION_KILL_SWITCH=1
```

## Adaptive Fusion Maintenance Requirements

AF-6 and AF-7 are complete. Any future Adaptive Fusion or autonomy PR should list:

- affected capability or architecture area
- intentionally unfinished follow-up, if any
- live-influence status
- provider/cost/concurrency gates
- verification
- residual risk
- rollback path
- next recommended action

Minimum verification for code maintenance:

```bash
cargo fmt --all -- --check
cargo clippy -p engine --all-targets -- -D warnings
cargo test -p engine --test test_adaptive_fusion_execution
cargo test -p engine --test test_contextual_adaptive_policy
bash scripts/verify_rust_typescript_stack.sh
uv run --no-project python scripts/check_agent_handoff.py
git diff --check
```

Docs-only Adaptive Fusion and autonomy corrections may use docs-only verification plus `uv run --no-project python scripts/check_agent_handoff.py`.

## Before Starting Autonomous Work

1. Read `docs/CURRENT_STATUS.md` only when status facts are unclear or the task updates status.
2. Read `docs/REAL_WORLD_TESTING_PLAYBOOK.md` for PR/merge/CI work, docs cleanup, and real-world pilot tasks.
3. Confirm the task is allowed under this file.
4. Keep the change scoped enough for review and rollback.
5. Run relevant verification.
6. Update handoff docs before committing and pushing when status or policy changed.
