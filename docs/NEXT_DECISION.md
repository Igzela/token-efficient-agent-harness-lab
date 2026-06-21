# Next Decision

## Current Direction

Phase 8, V2 Real Production Output, Real Output Closeout, and Adaptive Fusion AF-0 through AF-6 are complete.

No AF-7 or other new product track is authorized. The next work is repository maintenance, regression hardening, real-world validation, and evidence-based review of whether another explicitly approved track is needed.

AF-6 remains provider/model routing and completion execution only. Target-repository output, release controls, deployment controls, and repository merge authority remain separate systems with their own approval paths.

## Stable Tracks

| Track | Status |
|---|---|
| Core dispatch kernel | Complete |
| Architecture refactor R-series | Sealed at R7; no R8 approved |
| V2 Real Production Output | Complete through V2-5 |
| Real Output Closeout | Complete; `v0.1.0` published and installer verified |
| Adaptive Fusion AF-0 through AF-6 | Complete; live use remains explicit and default-off |
| Agent Autonomous Maintenance Mode | Active for docs, tests, CI, deterministic regressions, and low-risk PR flow |

## Default Boundary

Outside an explicitly authorized track, keep the system conservative. AF-6 permits provider fallback/fusion, online experiments, automatic policy promotion, and adaptive live routing only behind the implemented explicit gates.

The completed AF-6 implementation keeps:

- authentication for live provider execution
- per-request and daily budget controls
- token, call, timeout, and concurrency ceilings
- provider/model identity validation
- redaction and output caps
- circuit breakers and kill switches
- persistent audit events
- policy snapshots and rollback
- tests and CI before merge

## Auto-Merge Policy

Auto-merge eligible: docs-only, tests-only, CI fix, small low-risk code fix, all CI green, handoff guard pass, and clear rollback.

Not auto-merge eligible: auth, security, provider routing authority, database schema, release, deployment, policy mutation, failing CI, unclear rollback, or any AF-6 implementation slice.

## Autonomously maintain

The system may autonomously advance safe repository work: repair stale handoff docs, status drift, and wire-codegen guard drift; fix failing tests, CI breakage, lint/security baseline failures, and deterministic regressions; advance the next documented dispatch-kernel phase when the change is inside approved scope and respects all hard boundaries; and create branches, commits, PRs, and low-risk merges through the real-world testing playbook.

## Disallowed by Default

Provider/CLI execution boundary expansion, auth/security boundary changes, DB migrations, release/tag/deploy, active YAML/rubric/policy mutation, and destructive operations all require explicit human approval.

## Architecture refactor (R-series)

The R-series is sealed at R7. **SEALED AT R7.** R8 is not approved. No further R-series file splitting is approved.

## Allowed Next Paths

- Autonomous maintenance: repair stale docs, CI breakage, test drift, and wire-codegen drift.
- Regression hardening: add or repair tests for existing behavior.
- Pilots: real-world task validation.
- V2 maintenance that preserves the existing V2 output boundary.
- Adaptive Fusion maintenance that preserves the AF-6 gates and target-output separation.

## V2 Status

V2 remains the controlled real-output path:

```text
connect repo -> create task -> app-owned workspace execution
-> verification -> evidence -> approval -> patch or PR branch output
```

V2 implementation is complete through V2-5. It remains separate from AF-6. AF-6 provider routing work must not expand V2 output authority.

## Adaptive Fusion Track

Human approval on 2026-06-21 authorizes an adaptive multi-provider/model routing track inspired by Auto Router, Fusion deliberation, provider performance routing, and this repository's existing feedback/regulator loop.

AF-0 through AF-6 are complete.

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

## AF-6 Auto Fusion Plan

Planning baseline: `docs/archive/strategy/AF6_CONTROLLED_AUTO_FUSION_PROPOSAL.md`.

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
| AF-6D | Continuous experiments | **Complete** — default-off deterministic traffic allocation with risk, budget, token, call, time, concurrency, pause, and kill controls |
| AF-6E | Automatic promotion | **Complete** — default-off evidence thresholds, confidence/regression guards, hash-bound snapshots, rollout, stale-evidence rejection, and rollback |
| AF-6F | Adaptive completion API | **Complete** — authenticated `POST /api/v1/adaptive-fusion/completions`; compact metadata-hidden responses; optional default `/dispatch` routing only behind an explicit gate |

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

## AF-6 PR Requirements

Every AF-6 implementation PR must list:

- completed AF-6 slice
- intentionally unfinished slices
- live-influence status
- provider/cost/concurrency gates
- verification
- residual risk
- rollback path
- next slice

Required verification per implementation slice:

```bash
cargo fmt --all -- --check
cargo clippy -p engine --all-targets -- -D warnings
cargo test -p engine --test test_adaptive_fusion_execution
cargo test -p engine --test test_contextual_adaptive_policy
bash scripts/verify_rust_typescript_stack.sh
uv run --no-project python scripts/check_agent_handoff.py
git diff --check
```

Docs-only AF-6 planning PRs may use docs-only verification plus `uv run --no-project python scripts/check_agent_handoff.py`.

## Before Starting Autonomous Work

1. Read `docs/CURRENT_STATUS.md` only when status facts are unclear or the task updates status.
2. Read `docs/REAL_WORLD_TESTING_PLAYBOOK.md` for PR/merge/CI work, docs cleanup, and real-world pilot tasks.
3. Confirm the task is allowed under this file.
4. Keep the change commit-sized and run relevant verification.
5. Update handoff docs before committing and pushing.
