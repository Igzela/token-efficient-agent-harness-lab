# Module Map

Last updated: 2026-07-20.

This file maps current ownership and approved future connection points. It is not a phase history. Detailed implementation evidence remains in merged commits and PRs; forward execution is governed by `docs/NEXT_DECISION.md`.

Full Agent Autonomy Mode remains active for repository-scoped work that is testable, observable, reviewable, verification-gated, compatible, and rollbackable. External mutations, release, deployment, evaluator changes, and other authority-critical actions retain their separate gates.

## Core Ownership

| Module | State | Purpose | Verification |
|---|---|---|---|
| `AGENTS.md`, `CLAUDE.md`, `docs/NEXT_DECISION.md`, `docs/REAL_WORLD_TESTING_PLAYBOOK.md`, `scripts/check_agent_handoff.py` | active | autonomous authority, packet routing, evidence discipline, and governance-drift prevention | handoff guard, focused documentation checks, CI where required |
| `engine/src/main.rs`, `engine/src/http_server/` | active | sole Rust runtime and API | HTTP and engine tests |
| `engine/src/trusted_local.rs` | active | trusted-local readiness and execution gates | trusted-local tests |
| `dispatch_engine.rs`, `task_analyzer/`, `model_selector.rs`, `budget_manager.rs` | active | deterministic dispatch, routing, and bounded budget authority | dispatch and budget tests |
| `engine/src/provider/`, `engine/src/provider/fake.rs` | active | provider adapters, strict Agent Runtime decisions, redaction, audit, identity/pricing/cost evidence, and deterministic fake-provider testing | provider, audit, embedding, dependency, and full-stack tests |
| `engine/src/local_runner_provider.rs`, `engine/src/bin/local_runner_exec.rs` | active/manual-live | bounded Stub/Fake/explicit local live runner | local runner/provider tests and binaries |
| `engine/src/agent_memory.rs`, `engine/src/durable_memory.rs` | active | run-scoped memory policy and versioned durable memory/retrieval | memory, context, retrieval, integrity, and PostgreSQL tests |
| `engine/src/orchestration/schemas.rs`, `engine/src/workflow/`, `engine/src/scheduler.rs`, `engine/src/scheduler/`, `engine/src/node_executor.rs`, `engine/src/executor_pool.rs` | active | workflow graph, sole scheduler, bounded Agent Runtime, child proposals, concurrency, leases, retries, and executor accounting | workflow, scheduler, executor, restart, and concurrency tests |
| `engine/src/tool_policy_executor.rs`, tool-policy HTTP/store owners | active | configured allowlists, hooks, exact-action approval, and one-use effect authorization | tool-policy, approval, idempotency, and outcome-unknown tests |
| `engine/src/storage/local_product_store/` | active | sole application-owned SQLite/PostgreSQL-compatible store, audit, integrity, migrations, backups, artifacts, and evidence | local-store, migration, integrity, backup/restore, and PostgreSQL tests |
| `engine/src/target_repo_output.rs`, `engine/src/target_repo_output/authority.rs` | active | isolated target-workspace and approved patch/branch output authority | target-output, duplicate/restart, compensation, and acceptance tests |
| `engine/src/feedback/`, `engine/src/storage/local_product_store/policy_replay_producer.rs`, adaptive-policy store owners | active | offline replay, shadow/canary evidence, explicit promotion, snapshots, compensation, and rollback | replay, promotion, stale/tamper, and concurrency tests |
| `scripts/token_efficiency_scorecard.py`, benchmark/import scripts, scorecard store/API | active | token/cost/quality evidence, regression reports, batches, trends, and runtime comparisons | deterministic fixture, importer, store, API, SDK, and Dashboard tests |
| `dashboard/` | active/read-mostly | local operator UI; mutation remains in explicit backend owners | lint, typecheck, build, static export, and browser checks where applicable |
| `sdk/typescript/`, `sdk/python/` | active | typed API clients | SDK tests |
| `wire_contract/v1/`, `codegen/` | active | cross-language contracts and generated types | `scripts/check_wire_codegen_drift.sh` |
| `scripts/`, `tools/`, `.github/workflows/` | active | CI, pilots, packaging, release provenance, install/upgrade, backup/restore, and bounded drills | focused script tests, workflow checks, security baseline, and CI |
| `scripts/external_validation.{sh,py}`, `.github/workflows/external-validation.yml`, `tests/test_external_validation.py` | active | clean-environment stranger validation (demo + doctor + exact-head self-check); `external_validation_report.v1` | unit/self-test; hosted Ubuntu/macOS matrix; not external adoption evidence |
| `scripts/demo.sh`, `scripts/demo_no_provider.py`, `actions/exact-head-check/`, `tools/check_readme_public_surface.py` | active | no-provider public demo and exact-head growth wedge (OSS #241–#253) | demo unit tests, action contract tests, public-surface drift checks |
| `scripts/agent-control/`, `.github/workflows/agent-*.yml`, `tests/test_agent_control_*.py`, `tests/test_agent_orchestrator_*.py` | active/default-off | GitHub Issue-controlled maintenance orchestration, Vader artifact workers, GitHub-hosted finalization, exact-head CI/review/merge gates | orchestrator suite, YAML/action-pin/security/handoff checks, replacement live smoke pending |

## Current Capability Ownership

| Capability | Primary owners | Current boundary |
|---|---|---|
| Agent Runtime execution | typed plan/run HTTP handlers; `AgentStepExecutor`; scheduler/executor pool; `agent_action_receipts`; provider `agent_action.v1` source | connected; one leased node produces at most one typed action; default-off provider/runtime gates; restart/concurrency idempotency |
| Child tasks, handoff, review, debate | `ChildTaskProposal`, `agent_proposals`, `AgentAction` variants, workflow graph, scheduler, action receipts, `recursive_execution`, recursive store tables | flat actions remain compatible; bounded recursive admission/persistence is implemented (PR #239, default-off with independent kill switch); no autonomous root-goal authority |
| Command/CLI tool policy | capability/allowlist/hook stores; `ToolPolicyNodeExecutor`; workflow approvals/operator actions | connected; configured allowlists authoritative; exact-action authorization consumed once; post-effect failures remain non-retryable outcome-unknown |
| Durable memory and retrieval | durable-memory store, provider embedding adapter, provider audit, scheduler context injection, HTTP/SDK/Dashboard | connected; exact scope/version/source/provenance; guarded provider mode remains fail-closed without admissible current catalog evidence |
| PE-1 regression lab | scorecard scripts/store/read APIs/Dashboard | connected, report-only, and non-mutating |
| PE-2 budget intelligence | usage normalization, forecast/anomaly owners, fenced production jobs, operator pause/recovery | connected; only supported fresh evidence can reach typed pause/recovery owners |
| PE-3 operator decision center | derived queue, typed HTTP handlers, SDKs, Dashboard, existing mutation owners | connected; no generic executor and no new source of truth |
| PE-4 replay and promotion | dispatch-history provenance, offline replay, shadow/canary, evidence-chain promotion, snapshots/rollback | connected; replay remains non-authorizing until explicit current-state-bound promotion |
| Managed external runtime | Rust-leased `langgraph_external` node, v24 receipts/checkpoints, locked adapter package | connected in fixture/guarded-live modes; Python owns no queue, authority, or product store |
| OpenCode external coding adapter | default-off `opencode_external` node (`engine/src/opencode_runtime.rs`), fixture adapter `adapters/opencode/` | PE7 fixture-first; deny-by-default; reserved exact-capability routing; fixture identity in `FIXTURE_ADAPTER_MANIFEST.json`; `PIN.json` does not admit a real binary (`PE7-OPENCODE-BINARY-ADMISSION-1` deferred) |
| Efficiency/tool-discovery benchmark | native/LangGraph runtime binaries, benchmark script, scorecard matrix API/Dashboard | deterministic fixture evidence connected; provider-backed result not verified |
| Target repository output | app-owned workspaces/worktrees, approvals, receipts, branch/patch output, compensation | connected and externally accepted on a disposable target after PR #226; no direct target `main` or merge authority |
| GitHub/Vader repository maintenance | control Issue, Actions workflows, Vader artifact worker, GitHub-hosted finalizer | implemented but production-disabled; replacement smoke blocked by offline runner/TLS token-exchange path |

## Approved Recursive-Execution Ownership

`PE7-BOUNDED-RECURSIVE-EXECUTION-1` is the approved AR7 runtime-extension packet and is merged via PR #239 (default-off); it extends existing owners rather than creating parallel infrastructure.

| Required capability | Existing owner to extend | Boundary |
|---|---|---|
| Recursive identity and ancestry | `ChildTaskProposal`, workflow node/edge identities, `agent_proposals`, `AgentState` | control plane derives root, parent, depth, task fingerprint, and ancestry; model cannot assert authority |
| Tree admission | scheduler admission, workflow graph mutation, Agent Runtime capability checks | default-off; depth, children, total nodes, budget, concurrency, retry, scope, and cycle limits fail closed |
| Exactly-once child acceptance | `agent_action_receipts`, backend transactions, audit | one accepted proposal creates ordinary workflow state atomically; changed hashes or stale parents conflict |
| Capability inheritance | Agent Runtime capability profiles and tool-policy owner | children receive only an equal or reduced scope; escalation is rejected |
| Recursive execution | existing scheduler, executor pool, `AgentStepExecutor`, leases and retries | no recursive function/runtime loop; each child remains one ordinary bounded leased node |
| Recursive evidence | operator evidence read model, audit, scorecards, storage integrity | metadata-only ancestry/budget/result evidence; no raw prompt/output/transcript persistence |
| Pause, kill, rollback | existing scheduler pause, Agent Runtime kill switch plus a new narrower recursive gate/kill switch | disable admission, drain/block leases, preserve evidence, revert code; no destructive cleanup by default |

No source file or schema addition is considered implemented until its packet PR is merged with exact-head CI. The implementation PR must audit actual current file boundaries before choosing new filenames.

## Approved Harness-Evolution Ownership

`PE7-HARNESS-EVOLUTION-LAB-1` is a default-off laboratory. B1–B3 scaffolding (PRs #258–#260) is merged but not Level-1 complete (merged-repair-required residual defects). Active repair sequence: R1 authority/workspace → R2 evaluator/sealed-set → R3 finalizer integration → Level-1 acceptance. Extend existing owners below; do not treat synthetic fixtures or caller-supplied identity as authority.

| Laboratory function | Existing owner to reuse | Boundary |
|---|---|---|
| Failure/trace input | dispatch history, workflow/audit evidence, scorecards, PE-2 usage artifacts, PE-4 replay bindings | only owner-backed redacted evidence; no caller-invented failures or raw trace persistence |
| Mutation proposal | bounded Agent Runtime decision/proposal contracts | proposal only; no direct code, evaluator, active-policy, or authority mutation |
| Candidate workspace | `ACP_HARNESS_EVOLUTION_WORKSPACE_ROOT` + `materialize_candidate_workspace` / `revalidate_workspace_content` / `discard_candidate_workspace` under `harness_evolution.rs`; supervised-patch/app-owned workspace and target-output worktree owners for later finalizer path | isolated candidate state; registered target tree and active `main` remain protected; content hash from actual bounded surface |
| Active identity epoch | `LocalProductStore::register_harness_evolution_active_identity` (insert-only, actor-audited) | caller cannot supply authoritative active identity; optimistic expected-id only |
| Static and task evaluation | existing verification commands, benchmark registry, scorecards, replay/shadow/canary | equal-budget, versioned, hash-bound, sealed-task-aware evaluation; no provider call in initial fixture stage |
| Candidate lineage/archive | `LocalProductStore` + `engine/src/harness_evolution.rs` + `engine/src/harness_evolution_eval.rs` + `engine/src/harness_evolution_pr_ready.rs` + v27–v29 tables (`harness_evolution_*`), migrations, integrity, backup/restore, audit | app-owned versioned evidence through PR_READY bundles; laboratory never creates/merges PRs |
| Cost and budget comparison | scorecard and PE-2 usage/cost owners | missing or untrustworthy cost remains unavailable, never fabricated as zero |
| Promotion | operator decision center, evidence-chain promotion patterns, target-output finalizer, PR/CI/review gates | candidate may become `PR_READY`; no direct active-version change, auto-merge, deployment, or release |
| Rollback | ordinary Git/PR/release rollback plus lab gate/kill switch | lab never invents a second rollback authority |

The authoritative evaluator, sealed set, permissions, credentials, budget owner, audit, promotion thresholds, target-output authority, merge/release owner, and active-version binding are part of the immutable control plane. The initial mutable surface is limited to prompts/rules, context selection, tool descriptions/selection policy, bounded retry/stop policy, admitted model routing, and recursive decomposition policy. Source-code mutation follows only after component-level evaluation is stable.

## Integration and External-Acceptance Ownership

### Repository-maintenance orchestrator

The orchestrator remains disabled and emergency-stopped. `control_state.py` owns setup and control transitions; GitHub-hosted finalizers own branch/PR/label/comment mutations; Vader remains artifact-only. No recursive or evolution packet may enable Issue #208, consume the offline runner, or use this path until the replacement smoke is accepted.

### Provider/live benchmark

Provider adapters, catalog validation, pricing/cost gates, receipts, audit, circuit breakers, kill switches, and symbolic credentials remain the only live-call owners. Recursive/evolution fixture work may not infer live evidence from fixture output or from a `free` label.

## PE-5 Release Provenance Ownership

PR #214 merged the active `release_provenance.v2` repair. Existing release workflow, package/container builders, lockfiles, signed SLSA/SPDX/custom-manifest bundles, installer/upgrader verification, and transactional rollback remain authoritative. Recursive/evolution work adds no signing, publication, install, or release authority.

## PE-6 Fault Injection and Recovery Ownership

PR #214 merged the active v2 owner-evidence repair. Existing fixed scenario registry, disposable fault harness, SQLite/PostgreSQL recovery tests, provider/fake fault owners, release rollback drills, and cleanup evidence remain authoritative. Recursive/evolution work may use only separately allowlisted disposable fixtures and cannot target production resources.

## Active Routing

1. Active PE7 Ship PR lane: `PE7-HARNESS-EVOLUTION-B1-AUTHORITY-REPAIR-1` (then R2, R3, Level-1 acceptance). B1–B3 scaffolding is merged but not Level-1 complete.
2. `PR3-EXTERNAL-RUNTIME-LIVE-SEAL-1` is parked on Issue #254 (runner online/idle; Codex HTTP 403).
3. `PE7-OPENCODE-FIXTURE-ADAPTER-REPAIR-1` is complete via PR #257; binary admission remains deferred.
4. `PE7-META-IMPROVER-EXPERIMENT-1` remains blocked until a stable, independently reviewed Level-1 lab result exists.
5. Extend existing owners; do not create another runtime, scheduler, queue, storage layer, evaluator authority, release pipeline, signing authority, recovery authority, artifact truth source, tool registry, or Dashboard mutation model without an explicit replacement decision, compatibility evidence, and rollback.

## Active Documents

- `README.md`
- `AGENTS.md`
- `CLAUDE.md`
- `docs/ARCHITECTURE_BOOK.md`
- `docs/CURRENT_STATUS.md`
- `docs/NEXT_DECISION.md`
- `docs/MODULE_MAP.md`
- `docs/REAL_WORLD_TESTING_PLAYBOOK.md`
- `docs/RUNBOOK.md`

Prefer editing, shortening, and reconciling these surfaces over adding another policy, roadmap, status, packet, or closeout document.
