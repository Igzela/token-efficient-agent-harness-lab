# Current Status

Last updated: 2026-06-22. V2-0 through V2-5, the Real Output Closeout, and Adaptive Fusion AF-0 through AF-7 are complete; `v0.1.0` is published and its online installer path is verified.

## Summary

The core plan, V2 implementation, Real Output Closeout, and Adaptive Fusion Routing track are complete. The system includes prompt-to-CLI execution, bounded verification/repair evidence, optional real GitHub PR creation, a verified release contract, three real repository pilots, a task-first dashboard, guarded automatic provider/model routing with safe evidence feedback, and an Adaptive Fusion operator surface for completion testing and gate review.

The system is useful as an operations/control-plane lab for deterministic dispatch, workflow state, app-owned execution metadata, guarded local controls, SDKs, and audit evidence. It is not a cloud SaaS, hosted multi-tenant service, direct-deploy tool, or unattended autonomous-agent runtime.

## Current Product Boundary

- Rust `engine/` is the sole runtime/API/storage implementation.
- `dashboard/` is the local operations console with guarded app-owned controls. Mission Control exposes the V2-5 product output path and Adaptive Fusion exposes AF-4 policy evidence, AF-6 completion testing, routing metadata, gate status, experiment/promotion status, kill cues, and rollback controls over existing guarded APIs; release/deploy/apply actions remain unavailable.
- TypeScript and Python SDKs cover REST access to dispatch, workflow, config, team, cost, audit, backup/export, supervised patches, and V2-3 target output. The TypeScript SDK also covers Adaptive Fusion policy controls, the guarded completion endpoint, and dashboard operator status typing.
- Provider execution is off unless `ACP_ENABLE_PROVIDER_EXECUTION=1`.
- Adaptive Fusion AF-0 can produce deterministic `efficient` or `quality` single/fusion plans from normalized endpoint observations, but cannot influence live routing or call providers.
- Adaptive Fusion AF-1 can hot-add/update/disable bounded endpoint metadata and emit deterministic secret-safe snapshots; it has no database, HTTP, credential-resolution, network, or execution path.
- Adaptive Fusion AF-2 can adapt existing run traces into bounded offline endpoint/portfolio observations, aggregate evidence by task class, compute Pareto frontiers, calibrate judge bias, and emit `efficient`/`quality` shadow recommendations without changing routing.
- Adaptive Fusion AF-3/AF-6 can execute single, ordered fallback, or bounded parallel-panel fusion plans through fixed provider/model endpoints. Judge and synthesizer remain serial. Live calls require provider+adaptive env gates, configured auth, `dispatch:execute`, call/token/cost/time/concurrency limits, audit, redaction, circuit breakers, and a kill switch.
- Adaptive Fusion AF-4 can promote contextual policies from local evidence behind dual env gates and human confirmation, persist hash-bound snapshots in `local_config`, roll back promoted policies, and optionally assign at most 5% bounded exploration for low/medium-risk contexts. Promoted policies still have no live execution authority without an explicit bounded candidate plan.
- Adaptive Fusion AF-5 exposes active policies, snapshots, safety flags, explicit promotion JSON submission, and snapshot rollback in the dashboard and TypeScript SDK. It adds no provider execution authority, default-on routing, provider failover, or unattended workers.
- Adaptive Fusion AF-6 generates deterministic candidates, persists only safe observation summaries, supports default-off deterministic experiments and evidence-driven auto promotion, and exposes `POST /api/v1/adaptive-fusion/completions`. Routing metadata is hidden by default. Ordinary `/dispatch` behavior changes only when `ACP_ADAPTIVE_DEFAULT_LIVE_ROUTING=1`.
- Adaptive Fusion AF-7 exposes AF-6 in the dashboard through a completion test panel, optional routing metadata display, read-only gate/experiment/promotion/default-routing/kill status, and rollback snapshot cues. It adds no routing-core behavior, provider authority, target-output authority, DB migration, or default-on live execution.
- Installed local Claude/Codex CLIs are discovered by default for explicit workflow ticks. `ACP_ENABLE_CLI_EXECUTION=0` disables local CLI execution.
- V2-3 target output is default-off. It can create an app-owned git worktree and, only after scoped confirmation plus artifact approval/integrity checks, export a patch or push an `acp/*` branch. It never writes the registered target working tree or `main`.
- No hard process/container/VM sandbox is implemented; V2-1 is scoped to app-owned workspace confinement unless separately approved.
- No hosted/cloud/multi-tenant deployment is implemented.
- Bounded supervised workers are implemented behind `ACP_ENABLE_SCHEDULER=1` plus `ACP_ENABLE_SUPERVISED_WORKERS=1`; unattended autonomous-agent loops remain disallowed.
- Target-repo output, provider/CLI execution, supervised workers, and product UX are approved only through the V2 phase plan in `docs/NEXT_DECISION.md`; until each phase lands, the old limitation remains active.
- Cloud SaaS, multi-tenant hosting, app-runtime release/deploy/apply authority, provider failover outside the bounded AF-3 gates, default-on provider API execution, and unattended autonomous-agent loops remain out of scope.

## Last Recorded Verification

- Branch: main after AF-7 Operator Surface PR #95 merged as eef6aef7cfa8b1ddf72e398910d713f9010546d7.
- Tests: full Rust + TypeScript stack verification passed with 0 failures on 2026-06-22, including AF-6 candidate, execution, observation, experiment, promotion, completion API, dashboard operator status, dashboard build, and TypeScript SDK coverage.
- CI: PR #95 tests workflow run 27925304377 was green across rust, typescript, python, pg, native, docker, and rust-typescript-cutover before merge.
- Release: the `v0.1.0` release workflow run `27891104370` is green; all eight published assets passed checksum/archive inspection.
- Online install: the README installer fetched `v0.1.0` into an isolated home, verified the checksum, installed the runtime/dashboard, and passed health, dashboard API, and HTML smoke checks on 2026-06-21.
- PostgreSQL integration tests are gated behind `cargo test -p engine --features pg-tests` with `ACP_TEST_DATABASE_URL`.
- Live E2E validation evidence is archived at `docs/archive/validation/LIVE_E2E_VALIDATION_REPORT.md` with 48 PASS, 0 FAIL, 1 SKIP on 2026-06-12.

Handoff guard facts:

- Phase 4 is complete and historical as part of the sealed dispatch-kernel sequence.
- Architecture Refactor R-series Seal: **SEALED AT R7**. R8 is not approved.
- Post-R7 Wire/Type Governance Hardening: `scripts/check_wire_codegen_drift.sh`.

For current verification commands, use:

```bash
bash scripts/verify_rust_typescript_stack.sh
uv run --no-project python scripts/check_agent_handoff.py
```

## Complete Tracks

| Track | Status |
|---|---|
| Dispatch Kernel Phases 1-7, including 6A and 6B Gates 1-3 | Stable |
| Language migration to Rust runtime | Complete |
| Dynamic Workflow Batches 1-7 plus scheduler dynamic mode | Complete |
| Macro-Orchestrator Phases 1-5 repair batch | Complete |
| Self-Hosted GA Readiness SG-1 through SG-5 | Complete |
| HA Hardening HA-1 through HA-6 | Complete |
| HybridExecutor with `ACP_EXECUTION_MODE` | Complete |
| Dynamic Regulator MVP Phases 1-5 | Complete |
| Phase 8 final GA seal | Complete; archived at `docs/archive/phase-closeouts/PHASE8_FINAL_COMPLETION_PLAN.md` |
| Product Boundary Repair Track P0-P3 | Complete — PRs #64-#67 |
| V2 Real Production Output Track | Complete — V2-0 through V2-5 merged in PRs #69-#75 |
| Real Output Closeout | Complete — PRs #79-#81; `v0.1.0` published and online installer verified |

## Active Track

| Track | Status |
|---|---|
| Agent Autonomous Maintenance Mode | Active for docs, CI, tests, deterministic regressions, and low-risk PR flow |
| Adaptive Fusion Routing Track | AF-0 through AF-7 implemented; adaptive live execution, experiments, promotion, and default routing remain explicit and default-off |

Historical phase plans, closeouts, and long-form validation reports are retained under `docs/archive/`.

## Active Capability

- Deterministic dispatch pipeline: task analysis, model tier selection, budget reservation, executor selection, evaluation, and ledger persistence.
- Workflow runtime: persisted workflow runs, nodes, edges, events, approvals, queue/backpressure state, executor-pool binding, and opt-in dynamic graph mutation.
- Supervised execution primitives: app-owned workspace lifecycle, `NodeExecutor` trait, allowlisted `CommandNodeExecutor`, workflow tick endpoint, artifact capture, secret scan, integrity validation, approval binding, and export gate.
- V2-1 safety base: workspace IDs are path-safe, workspace copies stay under the app-owned workspace root, symlinks are skipped, copy file/byte ceilings are enforced, secret findings are redacted, secret-hit diffs are suppressed, command cwd is validated, command env is cleared except `PATH`, and command output is capped.
- V2-2 provider/CLI output path: provider nodes still require `ACP_ENABLE_PROVIDER_EXECUTION=1`; installed Claude/Codex CLIs are discovered by default and run only on explicit workflow ticks; plan `raw_request` becomes the node prompt unless a command override is supplied; outputs are redacted/capped and subprocess env remains restricted.
- CLI capability visibility: the dashboard API exposes only enabled/detected booleans from the startup snapshot; the dashboard distinguishes Claude/Codex availability from supervised-worker status without exposing binary paths or granting execution authority.
- Adaptive Fusion AF-0: deterministic capability/budget filtering and auditable `efficient`/`quality` single or bounded fusion planning over model endpoints; all outputs are shadow-only with no selected-tier, executor, retry, or active-policy influence.
- Adaptive Fusion AF-1: bounded in-memory model-endpoint registry with capability, context, tool, pricing, health, and symbolic credential-reference metadata; deterministic content hashes; idempotent upsert/disable; no secret values or live execution authority.
- Adaptive Fusion AF-2: bounded offline replay over existing run-trace/evaluator evidence; deterministic endpoint/portfolio aggregates, multi-dimensional Pareto frontiers, judge signed-bias/error calibration, and objective-specific shadow recommendations with evidence run IDs and zero live influence.
- Adaptive Fusion AF-3: up to eight startup-configured provider/model endpoints; explicit authenticated `adaptive_provider` ticks; single/fallback/fusion execution; fixed model binding; max calls, total tokens, dollars, elapsed time, panel size, and serial concurrency; existing daily/per-dispatch cost gates; provider audit, circuit breakers, redaction/capping, and kill path.
- Adaptive Fusion AF-4: contextual bandit scoring over task class and objective; non-stationary sequence decay; dual-gated promotion with minimum sample/confidence/regression checks; local evidence ID verification; hash-bound active-policy snapshots in `local_config`; rollback; high/critical-risk exploration exclusion; and optional low/medium-risk exploration capped at 5%.
- Adaptive Fusion AF-5: dashboard and TypeScript SDK operator surface for active policies, snapshots, safety flags, explicit promotion request submission, and snapshot rollback. It consumes the AF-4 guarded endpoints and does not add execution, provider, failover, merge, or deploy authority.
- Adaptive Fusion AF-6A: deterministic candidate generator that produces single, ordered fallback, and fusion candidates from configured endpoints. Generation is pure/deterministic with no provider calls. Candidates include schema-versioned IDs, content hashes, endpoint bindings, estimated costs/tokens/latency, and required capabilities. Aggregate caps (cost, tokens, latency) suppress fallback/fusion when exceeded. Duplicate endpoint IDs are fully excluded. Total emitted candidates bounded by `max_candidates`.
- Adaptive Fusion AF-6B: fusion panel calls execute in bounded parallel waves with deterministic evidence ordering and quorum handling; judge and synthesizer remain serial.
- Adaptive Fusion AF-6C: adaptive executions persist idempotent, tamper-checked observation summaries in existing local storage. Raw prompts, outputs, transcripts, secrets, repository content, and private paths are prohibited.
- Adaptive Fusion AF-6D: deterministic online experiments are dual-gated, capped at configured traffic, excluded for high/critical risk, and constrained by budget, tokens, calls, time, concurrency, pause, and kill controls.
- Adaptive Fusion AF-6E: automatic promotion is dual-gated and evidence-driven, with minimum samples/confidence, quality/cost/latency/failure regression guards, freshness checks, rollout percentage, snapshots, rollback, and a kill switch.
- Adaptive Fusion AF-6F: authenticated completion API with compact responses, optional routing metadata, deterministic candidate/policy selection, global cost accounting, observation capture, and optional `/dispatch` delegation only behind `ACP_ADAPTIVE_DEFAULT_LIVE_ROUTING=1`.
- Adaptive Fusion AF-7: the dashboard exposes a guarded completion test form, optional routing metadata, read-only AF gate status, experiment/promotion/default-routing/kill indicators, and rollback snapshot cues. The `/api/v1/dashboard` snapshot includes secret-free `adaptive_fusion` operator status; no raw prompt, raw output, transcript, repository content, secret, or private path is persisted by the operator status surface.
- V2-3 target repo output: `git_worktree` creation and output require `dispatch:execute` plus `ACP_ENABLE_TARGET_REPO_OUTPUT=1`; artifact hashes bind patch content and actual allowlisted verification evidence; output requires same-run approval, integrity, redaction, explicit confirmation, bounded text files, and remote controls. Optional GitHub PR creation additionally requires `ACP_ENABLE_GITHUB_PR_OUTPUT=1` and `ACP_GITHUB_TOKEN_ENV`.
- V2-4 bounded workers: scheduler startup requires both scheduler and supervised-worker env gates; worker count is bounded by global concurrency and 32; each worker claims at most one node per cycle through the existing atomic DB lease; heartbeat metadata exposes worker state; stale recovery is audited; `dispatch:execute` plus confirmation controls pause/resume/kill; env pause and kill switches remain available.
- Verification/repair: `/supervised-patch/workspaces/{id}/verify` runs allowlisted test tools in the app-owned workspace, stores redacted/capped evidence, and can invoke at most two CLI repair attempts before output remains blocked.
- V2-5 product output UX: the first navigation group is `Tasks / Runs / Outputs`; operational/admin tabs are secondary and collapsed. The task surface defaults to local Codex CLI and keeps task, workspace, approval, and branch/PR output in one path.
- Real output pilots: `scripts/real_output_pilots.py` completed Python, Rust, and Node repositories through real Claude CLI execution, real tests, artifact capture, approval, and three distinct `acp/*` branches. All three verification runs passed on the first attempt and all target `main` refs remained unchanged. Evidence: `/tmp/acp-real-output-pilots-e2qi2dmx/summary.json`.
- Release contract: canonical assets use `agent-control-plane-v0.1.0-<rust-target>.tar.gz` with a same-name top-level directory. Local packaging and `scripts/smoke_release.sh 0.1.0` passed 16 checks.
- Local storage: SQLite default with PostgreSQL optional via `ACP_DATABASE_URL`; schema version is documented in `docs/ARCHITECTURE_BOOK.md`.
- Operations: health, metrics, backups, restore smoke, circuit breaker state, audit log, and release-readiness checks.
- Dashboard: local operations console with guarded app-owned controls for workflow runs, scheduler state, proposals, patches, config/team/costs, and app-owned actions.
- Dashboard product-polish closeout: boundary lint checks dashboard app/components/lib for forbidden boundary controls; runtime gates are visible; Mission Control exposes a primary workflow path from run selection through tick, failure/status inspection, retry/fix path, approval, and export readiness.

## Current Gaps

- Engine/API/SDK/dashboard output is end to end for a supplied git repo: natural-language CLI execution, controlled worktree, real verification with bounded repair, artifact evidence, approval, patch/branch output, and optional GitHub PR creation.
- Product fit is stronger for local operations/research than for public-facing production UX.
- The UI is task-first, while detailed operations and administration remain available as secondary views.
- Security posture is suitable for local/small-team self-hosting only; hosted/multi-tenant use would require a new threat model and approved implementation plan.
- No hard process/container/VM sandbox isolation exists.
- Provider API execution remains default-off; local CLI discovery is default-on but execution still requires an explicit task tick.
- Promoted policies, experiments, auto promotion, and completion routing can influence live adaptive execution only when their independent gates are enabled. Provider execution remains authenticated, bounded, audited, killable, and default-off.
- Cloud SaaS, multi-tenant hosting, app-runtime merge/release/deploy/apply authority, and unattended autonomous-agent loops remain out of scope.

## Documentation Discipline

Active documentation is intentionally small:

- `docs/ARCHITECTURE_BOOK.md` — current architecture baseline
- `docs/CURRENT_STATUS.md` — current status and limits
- `docs/NEXT_DECISION.md` — single forward plan
- `docs/MODULE_MAP.md` — source/test ownership
- `docs/REAL_WORLD_TESTING_PLAYBOOK.md` — branch/PR/CI/maintenance workflow
- `docs/RUNBOOK.md` — operator procedures

All other Markdown under `docs/` is historical or low-frequency reference material in `docs/archive/`.

Do not add new roadmap, next-step, closeout, status, or productization documents unless the user explicitly asks for a new artifact. Prefer editing, shortening, or archiving existing docs.
