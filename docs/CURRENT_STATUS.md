# Current Status

Last recorded full verification: 2026-06-17. Documentation surface pruned and Product Boundary Repair Track completed: 2026-06-17. V2 Real Production Output Track authorized: 2026-06-17.

## Summary

The core plan is complete. The repository is starting the approved V2 Real Production Output Track while keeping the v1 default-safe posture. V2 aims to produce auditable patches or PR branches for real repositories through explicit gates, not by removing safety limits.

The system is useful as an operations/control-plane lab for deterministic dispatch, workflow state, app-owned execution metadata, guarded local controls, SDKs, and audit evidence. It is not a cloud SaaS, hosted multi-tenant service, direct-deploy tool, or unattended autonomous-agent runtime.

## Current Product Boundary

- Rust `engine/` is the sole runtime/API/storage implementation.
- `dashboard/` is the local operations console with guarded app-owned controls. Its API client understands V2-3 output, but the product workflow/control is deferred to V2-5; release/deploy/apply actions remain unavailable.
- TypeScript and Python SDKs cover REST access to dispatch, workflow, config, team, cost, audit, backup/export, supervised patches, and V2-3 target output.
- Provider execution is off unless `ACP_ENABLE_PROVIDER_EXECUTION=1`.
- CLI execution is off unless `ACP_ENABLE_CLI_EXECUTION=1`.
- V2-3 target output is default-off. On the V2-3 branch it can create an app-owned git worktree and, only after scoped confirmation plus artifact approval/integrity checks, export a patch or push an `acp/*` branch. It never writes the registered target working tree or `main`.
- No hard process/container/VM sandbox is implemented; V2-1 is scoped to app-owned workspace confinement unless separately approved.
- No hosted/cloud/multi-tenant deployment is implemented.
- No unattended autonomous-agent loop is approved.
- Target-repo output, provider/CLI execution, supervised workers, and product UX are approved only through the V2 phase plan in `docs/NEXT_DECISION.md`; until each phase lands, the old limitation remains active.
- Cloud SaaS, multi-tenant hosting, direct release/tag/deploy/apply authority, provider failover, default-on real execution, and unattended autonomous-agent loops remain out of scope.

## Last Recorded Verification

- Branch: `main`.
- Tests: **1534 Rust tests pass**, 0 failures, recorded 2026-06-12.
- CI: latest `tests` workflow on `main` is green as of 2026-06-17 after P0-P3.
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

## Active Track

| Track | Status |
|---|---|
| V2 Real Production Output Track | Authorized; V2-0 documentation PR opened; V2-1 execution safety base implemented on `codex/v2-1-execution-safety-base`; V2-2 provider/CLI output path implemented on `codex/v2-2-provider-cli-output`; V2-3 target repo output implemented on `codex/v2-3-target-repo-pr-flow`; V2-4 and V2-5 pending |

Historical phase plans, closeouts, and long-form validation reports are retained under `docs/archive/`.

## Active Capability

- Deterministic dispatch pipeline: task analysis, model tier selection, budget reservation, executor selection, evaluation, and ledger persistence.
- Workflow runtime: persisted workflow runs, nodes, edges, events, approvals, queue/backpressure state, executor-pool binding, and opt-in dynamic graph mutation.
- Supervised execution primitives: app-owned workspace lifecycle, `NodeExecutor` trait, allowlisted `CommandNodeExecutor`, workflow tick endpoint, artifact capture, secret scan, integrity validation, approval binding, and export gate.
- V2-1 safety base: workspace IDs are path-safe, workspace copies stay under the app-owned workspace root, symlinks are skipped, copy file/byte ceilings are enforced, secret findings are redacted, secret-hit diffs are suppressed, command cwd is validated, command env is cleared except `PATH`, and command output is capped.
- V2-2 provider/CLI output path: workflow ticks can run provider nodes only when `ACP_ENABLE_PROVIDER_EXECUTION=1` and a provider is configured; Claude/Codex CLI ticks remain `ACP_ENABLE_CLI_EXECUTION=1` gated; provider/CLI outputs are redacted/capped, provider ticks record provider audit events, provider cost gates block before execution, and CLI subprocess env is restricted to `PATH` plus `ACP_CLI_ENV_ALLOWLIST`.
- V2-3 target repo output: `git_worktree` workspace creation and real output require `dispatch:execute` plus `ACP_ENABLE_TARGET_REPO_OUTPUT=1`; artifact hashes bind actual patch content; output requires completed workflow verification evidence, same-run approval binding, integrity, redaction, explicit confirmation, bounded text-only changed files, remote/host allowlists, and an HTTPS token referenced by env; branch names are restricted to `acp/*`; `ACP_TARGET_REPO_OUTPUT_KILL_SWITCH=1` stops new output.
- Local storage: SQLite default with PostgreSQL optional via `ACP_DATABASE_URL`; schema version is documented in `docs/ARCHITECTURE_BOOK.md`.
- Operations: health, metrics, backups, restore smoke, circuit breaker state, audit log, and release-readiness checks.
- Dashboard: local operations console with guarded app-owned controls for workflow runs, scheduler state, proposals, patches, config/team/costs, and app-owned actions.
- Dashboard product-polish closeout: boundary lint checks dashboard app/components/lib for forbidden boundary controls; runtime gates are visible; Mission Control exposes a primary workflow path from run selection through tick, failure/status inspection, retry/fix path, approval, and export readiness.

## Current Gaps

- Engine/API/SDK V2-3 output is end to end for a supplied local git repo path: controlled worktree, execution artifact, verification evidence, approval, patch export, and branch push. V2-5 must expose this as one product workflow instead of low-level calls.
- Product fit is stronger for local operations/research than for public-facing production UX.
- UI is functional and operator-oriented; V2-5 must turn existing Mission Control, supervised patch, runtime gate, run, and audit components into one clear output workflow.
- Security posture is suitable for local/small-team self-hosting only; hosted/multi-tenant use would require a new threat model and approved implementation plan.
- V2-1 alone does not authorize target output; V2-3 adds only controlled worktree/branch output and still does not add provider/CLI default-on execution or sandbox/process/container/VM isolation.
- V2-4 must add bounded supervised workers behind explicit gates.
- Cloud SaaS, multi-tenant hosting, direct release/tag/deploy/apply authority, provider failover, default-on real execution, and unattended autonomous-agent loops remain out of scope.

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
