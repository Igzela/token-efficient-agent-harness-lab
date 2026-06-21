# Next Decision

## Current Direction

Phase 8, V2 Real Production Output, Real Output Closeout, and Adaptive Fusion AF-0 through AF-5 are complete.

The next authorized path is **AF-6 Auto Fusion**. Human approval on 2026-06-21 resets the older Adaptive Fusion boundary: AF-6 may pursue automatic candidate generation, parallel panel execution, online experimentation, automatic policy promotion, adaptive live routing after explicit AF-6 enablement, and a completion-style API surface.

This does not give the adaptive router unrelated product powers. AF-6 is about provider/model routing and completion execution. Target-repository output, release controls, deployment controls, and repository merge authority remain separate systems with their own approval paths.

## Stable Tracks

| Track | Status |
|---|---|
| Core dispatch kernel | Complete |
| Architecture refactor R-series | Sealed at R7; no R8 approved |
| V2 Real Production Output | Complete through V2-5 |
| Real Output Closeout | Complete; `v0.1.0` published and installer verified |
| Adaptive Fusion AF-0 through AF-5 | Complete |
| Agent Autonomous Maintenance Mode | Active for docs, tests, CI, deterministic regressions, and low-risk PR flow |
| AF-6 Auto Fusion | Authorized next path |

## Default Boundary

Outside an explicitly authorized track, keep the system conservative. For AF-6 specifically, the older blanket restrictions on provider fallback/fusion, online experiments, automatic policy promotion, and adaptive live routing no longer apply.

AF-6 may implement those capabilities if the implementation keeps:

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

## Allowed Next Paths

- Autonomous maintenance: repair stale docs, CI breakage, test drift, and wire-codegen drift.
- Regression hardening: add or repair tests for existing behavior.
- Pilots: real-world task validation.
- V2 maintenance that preserves the existing V2 output boundary.
- AF-6 Auto Fusion PRs that follow the phase plan below.

## V2 Status

V2 remains the controlled real-output path:

```text
connect repo -> create task -> app-owned workspace execution
-> verification -> evidence -> approval -> patch or PR branch output
```

V2 implementation is complete through V2-5. It remains separate from AF-6. AF-6 provider routing work must not expand V2 output authority.

## Adaptive Fusion Track

Human approval on 2026-06-21 authorizes an adaptive multi-provider/model routing track inspired by Auto Router, Fusion deliberation, provider performance routing, and this repository's existing feedback/regulator loop.

AF-0 through AF-5 are complete:

| Phase | Status |
|---|---|
| AF-0 | Shadow portfolio planning contract implemented |
| AF-1 | Model endpoint registry implemented |
| AF-2 | Offline evaluation and replay implemented |
| AF-3 | Explicit bounded adaptive execution implemented |
| AF-4 | Contextual policy improvement implemented |
| AF-5 | Operator UX implemented |

AF-6 is now authorized as the next Adaptive Fusion phase.

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
| AF-6A | Candidate generator | Deterministic single/fallback/fusion candidates from configured endpoints; bounded IDs, hashes, costs, capabilities, and model bindings |
| AF-6B | Parallel panel execution | Panel calls can run concurrently under bounded limits; judge/synthesizer remain ordered; failures are deterministic and audited |
| AF-6C | Online observation capture | Live adaptive outcomes become bounded learning observations without storing raw prompts, raw outputs, secrets, or repository content |
| AF-6D | Continuous experiments | Controlled traffic allocation tests candidate plans with budget, risk, and kill controls |
| AF-6E | Automatic promotion | Evidence thresholds can promote policy snapshots automatically, with regression guards and rollback |
| AF-6F | Adaptive completion API | Completion-style endpoint and optional default adaptive routing after AF-6 enablement |

AF-6 may introduce or refine gates such as:

```text
ACP_ENABLE_ADAPTIVE_FUSION_EXECUTION=1
ACP_ENABLE_ADAPTIVE_AUTO_ROUTING=1
ACP_ADAPTIVE_AUTO_ROUTING_ACTIVE=1
ACP_ENABLE_ADAPTIVE_ONLINE_EXPERIMENTS=1
ACP_ADAPTIVE_ONLINE_EXPERIMENTS_ACTIVE=1
ACP_ENABLE_ADAPTIVE_AUTO_PROMOTION=1
ACP_ADAPTIVE_AUTO_PROMOTION_ACTIVE=1
ACP_ADAPTIVE_DEFAULT_LIVE_ROUTING=1
ACP_ADAPTIVE_FUSION_KILL_SWITCH=1
ACP_ADAPTIVE_AUTO_PROMOTION_KILL_SWITCH=1
```

Exact gate names may change during implementation, but equivalent controls must exist.

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
