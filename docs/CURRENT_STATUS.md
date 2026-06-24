# Current Status

Last updated: 2026-06-23. V2-0 through V2-5, the Real Output Closeout, and Adaptive Fusion AF-0 through AF-7 are complete; `v0.1.0` is published and its online installer path is verified.

## Summary

The core plan, V2 implementation, Real Output Closeout, Adaptive Fusion Routing track, and Trusted Local Autonomous Execution IAE-0 through IAE-3 are complete. Full Agent Autonomy Mode is active for repo-scoped, testable, observable, CI-gated, rollbackable architecture and authority evolution. The system includes prompt-to-CLI execution, bounded verification/repair evidence, optional real GitHub PR creation, guarded automatic provider/model routing with safe evidence feedback, bounded background advancement of already-created adaptive workflow runs, and an integrated operator control/evidence surface.

The system is useful as an operations/control-plane lab for deterministic dispatch, workflow state, app-owned execution metadata, guarded local controls, SDKs, and audit evidence. It is not a cloud SaaS, hosted multi-tenant service, direct-deploy tool, or unattended autonomous-agent runtime.

## Current Product Boundary

- Rust `engine/` is the sole runtime/API/storage implementation.
- `dashboard/` is the local operations console with guarded app-owned controls. Mission Control exposes the V2-5 product output path and Adaptive Fusion exposes AF-4 policy evidence, AF-6 completion testing, routing metadata, gate status, experiment/promotion status, kill cues, rollback controls, and secret-safe adaptive provider endpoint configuration over existing guarded APIs; release/deploy/apply actions remain unavailable.
- TypeScript and Python SDKs cover REST access to dispatch, workflow, config, team, cost, audit, backup/export, supervised patches, and V2-3 target output. The TypeScript SDK also covers Adaptive Fusion policy controls, the guarded completion endpoint, provider endpoint configuration, and dashboard operator status typing.
- Provider execution is available through the recommended ready `ACP_TRUSTED_LOCAL_PROFILE=1` profile, with legacy `ACP_ENABLE_PROVIDER_EXECUTION=1` still supported for standalone operation.
- Adaptive Fusion AF-0 can produce deterministic `efficient` or `quality` single/fusion plans from normalized endpoint observations, but cannot influence live routing or call providers.
- Adaptive Fusion AF-1 can hot-add/update/disable bounded endpoint metadata and emit deterministic secret-safe snapshots; it has no database, HTTP, credential-resolution, network, or execution path.
- Adaptive Fusion AF-2 can adapt existing run traces into bounded offline endpoint/portfolio observations, aggregate evidence by task class, compute Pareto frontiers, calibrate judge bias, and emit `efficient`/`quality` shadow recommendations without changing routing.
- Adaptive Fusion AF-3/AF-6 can execute single, ordered fallback, or bounded parallel-panel fusion plans through fixed provider/model endpoints. Judge and synthesizer remain serial. Live calls require legacy provider+adaptive gates or a ready trusted-local profile, configured auth, `dispatch:execute`, call/token/cost/time/concurrency limits, audit, redaction, circuit breakers, and a kill switch.
- Adaptive Fusion AF-4 can promote contextual policies from local evidence behind dual env gates and human confirmation, persist hash-bound snapshots in `local_config`, roll back promoted policies, and optionally assign at most 5% bounded exploration for low/medium-risk contexts. Promoted policies still have no live execution authority without an explicit bounded candidate plan.
- Adaptive Fusion AF-5 exposes active policies, snapshots, safety flags, explicit promotion JSON submission, and snapshot rollback in the dashboard and TypeScript SDK. It adds no provider execution authority, default-on routing, provider failover, or unattended workers.
- Adaptive Fusion AF-6 generates deterministic candidates, persists only safe observation summaries, supports deterministic experiments and evidence-driven auto promotion through the ready trusted-local profile or standalone legacy gates, and exposes `POST /api/v1/adaptive-fusion/completions`. Routing metadata is hidden by default. Ordinary `/dispatch` delegation is enabled by a ready trusted-local profile or `ACP_ADAPTIVE_DEFAULT_LIVE_ROUTING=1`.
- Adaptive Fusion AF-7 exposes AF-6 in the dashboard through a completion test panel, optional routing metadata display, read-only gate/experiment/promotion/default-routing/kill status, rollback snapshot cues, and an endpoint config panel for `stub`, `openai_compatible`, and `anthropic` endpoint metadata. A protected legacy adaptive runtime may start fail-closed without endpoint metadata so operators can bootstrap through the dashboard/API; completion remains unavailable until config succeeds. Endpoint config stores only symbolic credential environment names, rejects raw-secret-shaped values, requires present credential environment variables for real providers, and applies validated local config to the adaptive completion API without restart. The same config restores the startup-bound adaptive executor and registry on the next process start. Explicit `ACP_ADAPTIVE_PROVIDER_ENDPOINTS_JSON` remains authoritative while present; saved local config stays persisted but inactive until the env override is removed. It adds no target-output authority, DB migration, or default-on live execution.
- Installed local Claude/Codex CLIs are discovered by default for explicit workflow ticks. `ACP_ENABLE_CLI_EXECUTION=0` disables local CLI execution.
- V2-3 target output is default-off. It can create an app-owned git worktree and, only after scoped confirmation plus artifact approval/integrity checks, export a patch or push an `acp/*` branch. It never writes the registered target working tree or `main`.
- No hard process/container/VM sandbox is implemented; V2-1 is scoped to app-owned workspace confinement unless separately approved.
- No hosted/cloud/multi-tenant deployment is implemented.
- Bounded supervised workers remain available through the legacy `ACP_ENABLE_SCHEDULER=1` plus `ACP_ENABLE_SUPERVISED_WORKERS=1` gates. A ready trusted-local profile may instead use `ACP_TRUSTED_LOCAL_TASK_ADVANCEMENT=1`, which enables one bounded scheduler path only after worker configuration validation.
- IAE-1 implements `ACP_TRUSTED_LOCAL_PROFILE=1`. It fails closed without protected auth, valid endpoint metadata, available symbolic credentials, strictly positive endpoint pricing, and positive per-dispatch/daily cost caps. When ready it activates provider execution, adaptive execution, default routing, experiments, and auto promotion while retaining existing token/call/time/concurrency, identity, redaction, audit, pause, kill, snapshot, and rollback controls.
- IAE-2 implements `ACP_TRUSTED_LOCAL_TASK_ADVANCEMENT=1`. It requires a ready IAE-1 profile, accepts only the pinned `adaptive_provider` executor, rejects invalid or excessive worker/interval/lease configuration, and advances only already-created queued workflow runs. It does not create goals or tasks, select CLI/command/noop workers, persist raw model content, or expand target-output, merge, release, or deploy authority.
- IAE-3 extends the existing dashboard snapshot with effective provider/adaptive/default-routing/experiment/promotion/task-advancement authority; configured cost, traffic, token, call, time, concurrency, rollout, and worker bounds; safe observation aggregates; and secret-free scheduler state. Live completion readiness requires provider/adaptive/auth gates, executor, registry, local storage, and a clear fusion kill switch. Experiment and auto-promotion authority fail closed when completion is not ready or their runtime policy/rollout configuration is invalid, and the dashboard exposes only safe readiness and validation evidence. The Adaptive Fusion page reuses existing authenticated scheduler pause/resume/kill and policy rollback endpoints and loads recent adaptive/scheduler actions through the existing `audit:read` API with `redact=true`. It never renders audit details.
- Cloud SaaS, multi-tenant hosting, app-runtime release/deploy/apply authority, direct target-repository `main` writes, unbounded provider spending, and unbounded autonomous loops remain out of scope.

## Last Recorded Verification

- Branch: `main` includes persisted trusted-local adaptive feedback gate repair merge `d390a4a` from PR #117 and status refresh commit `19f7a4d`; this status was refreshed after green main CI run `28116952179`.
- Tests: full Rust + TypeScript stack verification, 52 Python SDK tests, 159 HTTP server tests, 10 adaptive completion API tests, focused trusted-local/adaptive/provider-executor tests, rustfmt, Clippy, handoff, wire drift, dashboard boundary lint, production-like persisted-config preflight, and `git diff --check` passed with 0 failures; recent full main CI evidence includes run `28116952179`. The supervised patch CLI repair test and Trial 4/5 CLI pilot stubs remain hardened for noexec `/tmp` environments by keeping fake CLI executables under `target/`.
- Browser/runtime: authenticated stub runtime showed effective authority, 3% experiment traffic, configured cost/worker ceilings, one safe observation, redacted adaptive/scheduler audit actions, and functional confirmed pause/resume controls. Separate runtime checks verified invalid policy fail-closed blockers, storage-aware completion readiness, and raw requested adaptive gates remaining visually distinct from effective fail-closed authority; desktop and 390px mobile had no horizontal overflow or console errors.
- Security: repository secret scan returned 0 findings; handoff, rustfmt, Clippy, dashboard boundary lint, TypeScript typecheck/build, and `git diff --check` passed.
- CI: persisted trusted-local adaptive feedback gate repair PR #117 run `28115981080`, post-merge main run `28116439730`, and status-refresh main run `28116952179` were green across all seven jobs. The preceding scheduler gate main run `28114701272`, persisted workflow gate main run `28011294700`, autonomy-policy main run `28010547977`, and persisted provider-config main run `27999545331` were also green.
- Release: the `v0.1.0` release workflow run `27891104370` is green; all eight published assets passed checksum/archive inspection.
- Online install: the README installer fetched `v0.1.0` into an isolated home, verified the checksum, installed the runtime/dashboard, and passed health, dashboard API, and HTML smoke checks on 2026-06-21.
- PostgreSQL integration tests are gated behind `cargo test -p engine --features pg-tests` with `ACP_TEST_DATABASE_URL`.
- Live E2E validation evidence is archived at `docs/archive/validation/LIVE_E2E_VALIDATION_REPORT.md` with 48 PASS, 0 FAIL, 1 SKIP on 2026-06-12.

Handoff guard facts:

- Phase 4 is complete and historical as part of the sealed dispatch-kernel sequence.
- Full Agent Autonomy Mode: R7 remains the baseline; an explicit documented, tested, observable, rollbackable decision may supersede it.
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
| Agent Autonomous Maintenance Mode | Active for implementation, docs, CI, tests, review, and shipping |
| Full Agent Autonomy Mode | Active for repo-scoped architecture, authority, auth/security, migration, release-workflow, default-profile, and target-output evolution with tests, evidence, review, and rollback |
| Adaptive Fusion Routing Track | AF-0 through AF-7 implemented; existing runtime gates remain active |
| Trusted Local Autonomous Execution Track | Complete through IAE-3 |

Historical phase plans, closeouts, and long-form validation reports are retained under `docs/archive/`.

## Active Capability

- Deterministic dispatch pipeline: task analysis, model tier selection, budget reservation, executor selection, evaluation, and ledger persistence.
- Workflow runtime: persisted workflow runs, nodes, edges, events, approvals, queue/backpressure state, executor-pool binding, and opt-in dynamic graph mutation.
- Supervised execution primitives: app-owned workspace lifecycle, `NodeExecutor` trait, allowlisted `CommandNodeExecutor`, workflow tick endpoint, artifact capture, secret scan, integrity validation, approval binding, and export gate.
- V2-1 safety base: workspace IDs are path-safe, workspace copies stay under the app-owned workspace root, symlinks are skipped, copy file/byte ceilings are enforced, secret findings are redacted, secret-hit diffs are suppressed, command cwd is validated, command env is cleared except `PATH`, and command output is capped.
- V2-2 provider/CLI output path: provider nodes require either `ACP_ENABLE_PROVIDER_EXECUTION=1` or a ready `ACP_TRUSTED_LOCAL_PROFILE=1` with valid single-provider configuration; installed Claude/Codex CLIs are discovered by default and run only on explicit workflow ticks; plan `raw_request` becomes the node prompt unless a command override is supplied; outputs are redacted/capped and subprocess env remains restricted.
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
- IAE-1 trusted-local profile: `engine/src/trusted_local.rs` resolves a fail-closed readiness status and effective gates from protected auth, endpoint configuration, symbolic credential availability, positive pricing, and positive cost caps. The engine, persisted-config-aware explicit provider/adaptive workflow admission, adaptive completion/default routing, experiments, auto promotion, dashboard, and TypeScript SDK consume the same effective status. Runtime kill/pause controls remain independent and recoverable.
- IAE-2 bounded task advancement: an explicit acknowledgement composes the existing scheduler with a pinned `PersistingAdaptiveProviderNodeExecutor`. Each node refreshes active policy/evidence/cost context, applies the existing adaptive execution gates, persists only safe observation summaries, and fails closed when policy or cost context cannot be read. Dashboard and TypeScript status expose requested/ready/blocker state plus worker and concurrency bounds.
- IAE-3 operator control and evidence: the dashboard/API expose effective authority, completion prerequisites, budget and experiment ceilings, daily remaining cost, safe observation counts, scheduler runtime state, and existing rollback availability. Default routing, experiment, and auto-promotion authority require complete live readiness, including local storage; experiment and promotion policy validation fails closed while returning only stable blocker codes. The Adaptive Fusion UI provides confirmed pause/resume/kill controls and redacted recent audit action/resource/timestamp evidence without returning audit details, raw prompts/outputs/transcripts, credentials, repository content, or private paths.
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
- Cloud SaaS, multi-tenant hosting, app-runtime release/deploy/apply authority, direct target `main` writes, and unbounded autonomous loops remain out of scope.

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
