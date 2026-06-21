# Current Status

Last updated: 2026-06-21. V2-0 through V2-5 and the Real Output Closeout are complete; `v0.1.0` is published and its online installer path is verified.

## Summary

The core plan, V2 implementation, and Real Output Closeout are complete. The system includes prompt-to-CLI execution, bounded verification/repair evidence, optional real GitHub PR creation, a verified release contract, three real repository pilots, and a task-first dashboard. The Adaptive Fusion Routing track is active with an AF-0 shadow-only portfolio planner; live multi-provider/fusion execution is not yet implemented.

The system is useful as an operations/control-plane lab for deterministic dispatch, workflow state, app-owned execution metadata, guarded local controls, SDKs, and audit evidence. It is not a cloud SaaS, hosted multi-tenant service, direct-deploy tool, or unattended autonomous-agent runtime.

## Current Product Boundary

- Rust `engine/` is the sole runtime/API/storage implementation.
- `dashboard/` is the local operations console with guarded app-owned controls. Mission Control now exposes the V2-5 product output path over existing guarded APIs; release/deploy/apply actions remain unavailable.
- TypeScript and Python SDKs cover REST access to dispatch, workflow, config, team, cost, audit, backup/export, supervised patches, and V2-3 target output.
- Provider execution is off unless `ACP_ENABLE_PROVIDER_EXECUTION=1`.
- Adaptive Fusion AF-0 can produce deterministic `efficient` or `quality` single/fusion plans from normalized endpoint observations, but cannot influence live routing or call providers.
- Installed local Claude/Codex CLIs are discovered by default for explicit workflow ticks. `ACP_ENABLE_CLI_EXECUTION=0` disables local CLI execution.
- V2-3 target output is default-off. It can create an app-owned git worktree and, only after scoped confirmation plus artifact approval/integrity checks, export a patch or push an `acp/*` branch. It never writes the registered target working tree or `main`.
- No hard process/container/VM sandbox is implemented; V2-1 is scoped to app-owned workspace confinement unless separately approved.
- No hosted/cloud/multi-tenant deployment is implemented.
- Bounded supervised workers are implemented behind `ACP_ENABLE_SCHEDULER=1` plus `ACP_ENABLE_SUPERVISED_WORKERS=1`; unattended autonomous-agent loops remain disallowed.
- Target-repo output, provider/CLI execution, supervised workers, and product UX are approved only through the V2 phase plan in `docs/NEXT_DECISION.md`; until each phase lands, the old limitation remains active.
- Cloud SaaS, multi-tenant hosting, app-runtime release/deploy/apply authority, provider failover outside the bounded AF-3 gates, default-on provider API execution, and unattended autonomous-agent loops remain out of scope.

## Last Recorded Verification

- Branch: `main`.
- Tests: **1587 Rust tests pass**, 0 failures, recorded 2026-06-21 on `codex/adaptive-fusion-af0`.
- CI: the `tests` workflow run `27892330465` on `main` is green as of 2026-06-21.
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
| Adaptive Fusion Routing Track | AF-0 shadow portfolio planner implemented; AF-1 through AF-5 pending |

Historical phase plans, closeouts, and long-form validation reports are retained under `docs/archive/`.

## Active Capability

- Deterministic dispatch pipeline: task analysis, model tier selection, budget reservation, executor selection, evaluation, and ledger persistence.
- Workflow runtime: persisted workflow runs, nodes, edges, events, approvals, queue/backpressure state, executor-pool binding, and opt-in dynamic graph mutation.
- Supervised execution primitives: app-owned workspace lifecycle, `NodeExecutor` trait, allowlisted `CommandNodeExecutor`, workflow tick endpoint, artifact capture, secret scan, integrity validation, approval binding, and export gate.
- V2-1 safety base: workspace IDs are path-safe, workspace copies stay under the app-owned workspace root, symlinks are skipped, copy file/byte ceilings are enforced, secret findings are redacted, secret-hit diffs are suppressed, command cwd is validated, command env is cleared except `PATH`, and command output is capped.
- V2-2 provider/CLI output path: provider nodes still require `ACP_ENABLE_PROVIDER_EXECUTION=1`; installed Claude/Codex CLIs are discovered by default and run only on explicit workflow ticks; plan `raw_request` becomes the node prompt unless a command override is supplied; outputs are redacted/capped and subprocess env remains restricted.
- CLI capability visibility: the dashboard API exposes only enabled/detected booleans from the startup snapshot; the dashboard distinguishes Claude/Codex availability from supervised-worker status without exposing binary paths or granting execution authority.
- Adaptive Fusion AF-0: deterministic capability/budget filtering and auditable `efficient`/`quality` single or bounded fusion planning over model endpoints; all outputs are shadow-only with no selected-tier, executor, retry, or active-policy influence.
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
- Live model-endpoint registry, provider portfolio execution/fallback, judge/synthesizer calls, contextual-bandit exploration, and policy promotion remain unimplemented until AF-1 through AF-4.
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
