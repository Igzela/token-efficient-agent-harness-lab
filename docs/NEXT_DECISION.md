# Next Decision

## Core Plan COMPLETE

**Phase 8 DONE. Core plan COMPLETE.** No future core-completion phase remains. Future work is maintenance, bugfixes, pilots, or v2 proposals only. Historical gap inventory and seal evidence are archived at `docs/archive/phase-closeouts/PHASE8_FINAL_COMPLETION_PLAN.md`.

**Completed phases:** Phase 1-7 (including 6A, 6B-1/2/3, Gates 1-3), Operator Surface (PRs #49-#52), Dynamic Workflow Batches 1-7, Macro-Orchestrator Phases 1-5, Self-Hosted GA Readiness SG-1 through SG-5, HA Hardening HA-1 through HA-6, Dynamic Regulator MVP Phases 1-5.

**Agent Autonomous Maintenance Mode is active.** Agents autonomously maintain repo health, docs hygiene, CI correctness, and low-risk PR flow. CI green is the merge/success standard. Documentation maintenance means update/prune/archive, not accumulate. See playbook section "Agent Autonomous Maintenance Mode" for the full loop and rules.

**Strategic background:** `docs/archive/strategy/DYNAMIC_GLOBAL_REGULATOR_PLAN.md` (read only when drafting a v2 strategic proposal, not at every session start).

## Architecture refactor (R-series)

**Architecture Refactor R-series**: **SEALED AT R7.** R8 is not approved. No further R-series file splitting is approved.

## V2 Real Production Output Track — AUTHORIZED

Human approval on 2026-06-17 authorizes a new V2 productization track: turn the local control plane into a system that can produce auditable patches or PR branches for real repositories.

This approval does **not** remove safety limits. It turns selected old limits into production guardrails that must land phase by phase, behind explicit gates, with audit evidence, tests, and rollback/kill paths. Until a V2 phase merges, current v1 behavior remains authoritative.

Target V2 user flow:

```text
connect real repo -> create task -> isolated app-owned workspace execution
-> code changes -> verification -> evidence/diff -> human approval
-> push PR branch or export patch
```

Current hard constraints for the V2 track:

- Provider API execution remains default-off. Installed local Claude/Codex CLIs are discovered by default for explicit workflow ticks; set `ACP_ENABLE_CLI_EXECUTION=0` to disable them.
- V2-3 target output is merged; target writes are allowed only through its controlled worktree plus `acp/*` branch push or patch export, never direct target working-tree or `main` writes.
- V2-1 may harden app-owned workspace isolation; process/container/VM sandboxing remains a separate approval item unless explicitly added to a future plan.
- V2-4 may add bounded supervised workers with lease/heartbeat/kill controls; unattended autonomous-agent loops remain disallowed.
- Hosted/cloud/multi-tenant SaaS, app-runtime release/deploy controls, provider failover, and default-on provider API execution remain out of scope for this track.
- Secrets must not appear in logs, diffs, artifacts, dashboard output, or PR bodies.

## Disallowed by Default

Outside explicitly merged V2 phases, the following remain disabled:

- Cloud SaaS, hosted/cloud deployment, and multi-tenant service.
- Process/container/VM sandbox isolation.
- Uncontrolled target working-tree writes, direct `main` writes, apply/merge/deploy authority, and release/tag controls.
- Default-on provider API execution.
- Unattended autonomous-agent loops.
- Provider failover.
- Production worker concurrency outside the V2-4 supervised lease/heartbeat model.

## Safety Gates

| Gate | Rule |
|---|---|
| No secrets committed | `scripts/acp_secret_scan.py` enforced |
| No merge on failing CI | all jobs must pass |
| No unlogged execution | ledger records all dispatches |
| Rollback path required | `git revert` sufficient for low-risk |
| Provider execution | env-gated (`ACP_ENABLE_PROVIDER_EXECUTION=1`) |
| CLI execution | local CLI discovery defaults on; `ACP_ENABLE_CLI_EXECUTION=0` disables it |
| Target repo output | V2 branch/worktree/PR flow only; no direct `main` writes |
| V2 real output | explicit phase gate, audit event, tests, rollback/kill path |
| No auto release/tag/deploy | explicit approval required |
| High-risk changes | auth, security, provider, deploy, DB — explicit approval |
| YAML/rubric/policy mutation | explicit approval |
| Destructive operations | explicit approval |

## Auto-Merge Policy

Auto-merge eligible: docs-only, tests-only, CI fix, small low-risk code fix (< 50 lines), all CI green, handoff guard pass, `git revert` rollback.

Not auto-merge eligible: auth/security/provider/deploy/DB changes, release/tag/deploy, policy mutation, failing CI, unclear rollback. PR #31 is not auto-merge eligible (DB schema v12 migration + active policy override routing path).

Full classifier: `docs/REAL_WORLD_TESTING_PLAYBOOK.md`

## Allowed Next Paths

Autonomously maintain repo health and fix CI/docs/test drift. No future core-completion phase remains. The following paths are allowed:

- Autonomous maintenance: repair stale docs, CI breakage, test drift, wire-codegen drift
- Regression hardening: add/repair tests for existing behavior
- Docs/CI/test drift repair
- Pilots: real-world task validation
- V2 Real Production Output PRs that follow the phase plan below

## Real Output Closeout — COMPLETE

Human approval on 2026-06-20 authorizes the final local-product closeout. This is not a new runtime kernel or unattended-agent track. It completes the existing V2 path in this order:

1. Preserve the workflow plan `raw_request` as the default CLI/provider node prompt.
2. Add a bounded supervised task loop in the existing workspace/executor/artifact path: one app-owned git worktree, explicit CLI executor, detected or supplied verification commands, at most two repair attempts, recorded verification evidence, and existing pause/kill/time/cost gates.
3. After an approved `acp/*` branch push, optionally create a GitHub pull request through an explicit GitHub token environment reference and repository/host allowlist. No merge authority is added.
4. Align release workflow, installer, and README artifact naming, then publish the first verified release.
5. Validate the flow against three independent disposable real git repositories and record compact evidence in the existing status/runbook surfaces.
6. Make the dashboard task-first: task prompt, repository, executor, verification, diff/evidence, approval, and PR result are primary; operations/admin views remain available as secondary navigation.

Implementation status:

- Items 1-6 are merged through PRs #79-#81.
- Release naming, package layout, installer behavior, and the local 16-check release smoke are complete.
- `v0.1.0` was published on 2026-06-21 with verified checksums for all release archives.
- The README online installer downloaded the published x86_64 Linux asset, verified its checksum, installed it into an isolated home, started the runtime, and passed health, dashboard API, and dashboard HTML smoke checks.

Acceptance:

- Chinese and English prompts reach the selected CLI/provider unchanged unless an explicit command override is supplied.
- Failed verification can trigger no more than two audited repair attempts; exhausted verification blocks approval-bound output.
- Verification output, exit status, command, attempt, and timestamp are bound to the captured artifact.
- GitHub PR creation is default-off, explicit, audited, and returns the real PR URL; direct `main`, merge, release, deploy, and apply authority remain unavailable.
- The local package/installer smoke and published-asset online installation both pass.
- Three pilots produce distinct verified `acp/*` branches or PRs while each target `main` remains unchanged.

## Product Boundary Repair Track — COMPLETE

The product-boundary repair track closed the gap between product wording, dashboard behavior, and practical usability. This was a maintenance/product-polish track, not a new runtime authority track.

Completed PRs:

| Slice | Branch | Goal | Scope |
|---|---|---|---|
| P0 | `codex/p0-boundary-lint` / PR #64 | Align dashboard boundary wording and checks | Replaced read-only dashboard lint with boundary lint across dashboard app/components/lib; updated live E2E dashboard assertion |
| P3 | `codex/p3-out-of-scope-docs` / PR #65 | Make non-goals explicit | Documented cloud/SaaS, multi-tenant, hard sandbox, target writes/apply/merge/deploy, default-on provider, unattended workers, provider failover, and production worker concurrency as v2/out-of-scope |
| P1 | `codex/p1-runtime-gates` / PR #66 | Make local gates understandable | Added runtime-gate visibility and shortest local operator path for provider/CLI/auth/workspace/export gates |
| P2 | `codex/p2-primary-workflow` / PR #67 | Add a clear dashboard main workflow | Surfaced create/select run, tick, inspect failure/status, retry/fix, approve, and export readiness as a guided path using existing APIs |

Latest `main` CI after P0-P3 is green. No further Product Boundary Repair slices are planned.

## V2 Phase Plan

Use this as the single forward plan. Do not create new roadmap/status docs for V2. If a phase grows too large, split by vertical acceptance criteria while preserving the same phase order.

| Phase | Branch | Goal | Required acceptance |
|---|---|---|---|
| V2-0 | `codex/v2-real-production-output` | Authorize and document the track | Merged in PR #69 |
| V2-1 | `codex/v2-1-execution-safety-base` | Real execution safety base | Merged in PR #70 |
| V2-2 | `codex/v2-2-provider-cli-output` | Real provider/CLI output path | Merged in PR #71 |
| V2-3 | `codex/v2-3-target-repo-pr-flow` | Target repo branch/PR output | Merged in PR #72 |
| V2-4 | `codex/v2-4-supervised-worker-queue` | Bounded production worker queue | Merged in PR #73: dual env gate, bounded worker count, atomic lease claim, worker heartbeat, stale recovery audit, pause/resume/kill API, auth scope, kill switch, SDK/tests |
| V2-5 | `codex/v2-5-product-output-ux` | Product-grade main workflow | Merged in PR #75: Mission Control path for task/run creation, tick, workspace, capture, approval binding, export/target output, scheduler control, visible gates, responsive layout |

V2 implementation routing:

- V2-1 starts in `engine/src/storage/local_product_store/supervised_patch.rs`, `engine/src/http_server/handlers/supervised_patch.rs`, `engine/src/node_executor.rs`, and focused storage/API tests.
- V2-2 starts in `engine/src/provider/`, `engine/src/cli/`, `engine/src/executor/`, `engine/src/dispatch_engine.rs`, and provider/CLI tests.
- V2-3 is owned by `engine/src/target_repo_output.rs`, supervised patch storage/API, and matching SDK/dashboard API contracts. Runtime gate: `ACP_ENABLE_TARGET_REPO_OUTPUT=1`; emergency kill: `ACP_TARGET_REPO_OUTPUT_KILL_SWITCH=1`.
- V2-4 starts in `engine/src/scheduler.rs`, `engine/src/workflow/run_queue.rs`, `engine/src/executor_pool.rs`, and `engine/src/storage/local_product_store/heartbeat.rs`.
- V2-5 starts in `dashboard/src/components/MissionControl.tsx`, `SupervisedPatch.tsx`, `RuntimeGates.tsx`, and `dashboard/src/lib/api-client.ts`.

Every V2 PR must list: completed phase, intentionally unfinished phases, verification, residual risk, rollback path, and next PR.

## Adaptive Fusion Routing Track — AUTHORIZED

Human approval on 2026-06-21 authorizes an adaptive multi-provider/model routing track inspired by Auto Router, Fusion deliberation, provider performance routing, and this repository's existing feedback/regulator loop.

Canonical terms:

- **Model endpoint** — one concrete provider plus model combination. Credentials remain environment/keyring/vault references and never enter routing records.
- **Objective profile** — an operator-selected multi-objective policy. `efficient` prioritizes acceptable quality per dollar and latency; `quality` prioritizes correctness and reliability while retaining hard budget limits.
- **Fusion plan** — a bounded panel of model endpoints, a judge, and a synthesizer. It is an execution plan, not an autonomous worker swarm.
- **Recursive improvement** — evidence-driven routing-policy updates from task context, outcomes, quality, cost, latency, tool success, and human feedback. It does not mean self-modifying code.
- **Exploration** — bounded traffic assigned to uncertain candidates to learn performance. Exploration must be explicit, budgeted, audited, reversible, and disabled by default until AF-4.

Target flow:

```text
task context + objective profile + hard constraints
-> eligible model endpoints
-> single / fallback / fusion plan
-> bounded execution
-> judge + deterministic evidence
-> quality/cost/latency/tool/human outcome
-> shadow policy update
-> gated promotion or rollback
```

Global gates:

- Real provider execution remains default-off and requires the existing provider/auth/cost gates plus phase-specific adaptive-routing gates.
- AF-0 through AF-2 are shadow/offline only and cannot influence live executor selection.
- No provider credential, binary path, prompt secret, raw sensitive output, or target-repository content may enter portfolio metadata.
- Fusion calls require per-request call, token, dollar, timeout, and concurrency ceilings.
- A judge score alone cannot promote a policy. Promotion requires minimum samples, confidence, regression checks, audit evidence, a persistent snapshot, and rollback.
- Exploration has a hard traffic ceiling, excludes high/critical-risk tasks by default, and has an emergency kill switch.
- Until a phase merges, the previous routing behavior and provider-failover prohibition remain authoritative.

| Phase | Goal | Required acceptance |
|---|---|---|
| AF-0 | Shadow portfolio planning contract | Deterministic pure planner; `efficient`/`quality`; capability and hard-budget filtering; single/fusion plan; panel/judge/synthesizer identities; normalized score evidence; always `shadow_only`; no network, storage, or live routing effect |
| AF-1 | Model endpoint registry | Multiple provider/model endpoints with capability, pricing, context, tool, health, and credential-reference metadata; no secret persistence; hot add/disable; deterministic snapshot |
| AF-2 | Offline evaluation and replay | Reuse run traces and evaluator evidence to compare endpoint/portfolio outcomes by task class; calibrate judge bias; produce shadow recommendations and Pareto frontiers |
| AF-3 | Bounded adaptive execution | Explicit env/auth gate; single, ordered fallback, and bounded panel/judge/synthesizer execution; max calls/cost/time/concurrency; full audit; kill switch; no default-on provider calls |
| AF-4 | Contextual-bandit improvement | Shadow-first contextual policy, bounded exploration, non-stationary decay, minimum samples/confidence, dual-gated promotion, snapshot/rollback, high-risk exclusion |
| AF-5 | Product/operator UX | Objective selection, portfolio evidence, model/provider scorecards, cost/quality frontier, exploration/promotion controls, rollback visibility |

AF-0 implementation routing:

- Extend `engine/src/feedback/`; do not create a parallel dispatch, policy, scheduler, or storage kernel.
- Inputs are immutable model-endpoint observations with normalized quality, success, cost-efficiency, and latency-efficiency scores plus capabilities.
- Required capabilities, minimum quality, and per-plan cost constraints filter before scoring. Ties are deterministic by endpoint ID.
- AF-0 uses explicit, serialized bootstrap weights: `efficient` = quality 0.25, success 0.25, cost efficiency 0.35, latency efficiency 0.15; `quality` = quality 0.65, success 0.25, cost efficiency 0.05, latency efficiency 0.05. These defaults are not learned policy and cannot change live routing.
- `efficient` defaults to one endpoint. `quality` in auto mode may emit a three-endpoint fusion plan only for sufficiently complex or high-impact tasks and only when enough eligible endpoints exist.
- AF-0 output is advisory evidence only. It must state that it cannot influence selected tier, executor, retry path, or active routing policy.

AF-0 implementation status:

- Implemented on `codex/adaptive-fusion-af0`.
- The pure planner and seven focused tests cover objective semantics, bounded fusion, full-plan budget fallback, capability/score/budget validation, duplicate endpoint rejection, audit scorecards, deterministic tie-breaking, and zero live influence.

AF-1 implementation status:

- Implemented on `codex/adaptive-fusion-af1`, stacked on AF-0.
- The bounded in-memory registry supports idempotent upsert and disable, deterministic sorted snapshots and content hashes, capability/context/tool/pricing/health metadata, and symbolic credential references.
- Validation rejects malformed identities, unbounded metadata, invalid pricing/health/context, raw secret patterns, non-symbolic credential references, and registry capacity overflow before mutation.
- AF-1 adds no database migration, HTTP surface, credential resolution, provider call, or live-routing influence.

AF-2 implementation status:

- Implemented on `codex/adaptive-fusion-af2`, stacked on AF-1.
- Existing `RunTrace` quality, success, cost, latency, task-class, and evidence IDs can be adapted into endpoint or portfolio replay observations; tool success and explicit judge/reference evidence remain typed inputs.
- The bounded offline engine aggregates candidate metrics by task class, computes deterministic multi-dimensional Pareto frontiers, emits `efficient` and `quality` shadow recommendations using AF-0 objective weights, and calibrates judge signed bias/absolute error after at least three samples.
- Inputs are capped at 10,000 observations, 512 candidates per task class, and $1,000,000 cost per observation; malformed, duplicate, inconsistent, overflow-prone, or secret-shaped evidence is rejected without identifier disclosure.
- AF-2 adds no database query, persistence, HTTP surface, provider call, or live-routing influence.

AF-3 implementation status:

- Implemented on `codex/adaptive-fusion-af3`, stacked on AF-2.
- Startup accepts up to eight fixed provider/model endpoints from `ACP_ADAPTIVE_PROVIDER_ENDPOINTS_JSON`; real endpoint credentials remain symbolic environment references and remote base URLs require HTTPS, with loopback HTTP allowed for local adapters.
- Live execution requires `ACP_ENABLE_PROVIDER_EXECUTION=1`, `ACP_ENABLE_ADAPTIVE_FUSION_EXECUTION=1`, `ACP_REQUIRE_AUTH=1`, an authenticated `dispatch:execute` request, an explicit node `adaptive_execution` plan, and `executor=adaptive_provider`.
- Single, ordered fallback, and serial two/three-member panel plus judge and synthesizer execution reuse the existing `Provider`, circuit-breaker, provider-audit, workflow tick, and `NodeExecutor` boundaries.
- Hard limits cover at most 8 calls, $1,000 admitted cost, 300 seconds, 1,000,000 total reserved tokens, panel size 2-3, and concurrency 1. Existing per-dispatch/daily cost gates also apply; workflow-level retries are rejected because the plan owns fallback.
- Endpoint IDs are fixed to configured provider/model bindings. Calls and terminal outcomes are audited without prompts or raw outputs; outputs are redacted/capped; provider cost/token/identity overruns stop the plan. `ACP_ADAPTIVE_FUSION_KILL_SWITCH=1` blocks startup execution and the shared runtime kill handle stops subsequent calls.
- AF-3 does not automatically apply AF-0/AF-2 recommendations, explore traffic, promote policy, persist learned policy, or add unattended workers.

AF-4 implementation status:

- Implemented on `codex/adaptive-fusion-af4`, stacked on AF-3.
- Contextual scoring lives in `feedback/` and uses task class plus `efficient`/`quality` objective profiles, normalized outcome metrics, human score blending, and sequence decay for non-stationary behavior.
- Promotion requires `ACP_ENABLE_ADAPTIVE_POLICY_PROMOTION=1`, `ACP_ADAPTIVE_POLICY_PROMOTION_ACTIVE=1`, explicit human confirmation, at least 30 samples, confidence >= 0.85, no quality/cost/failure regression, low/medium risk, unique local evidence run IDs, and configured auth with `team:admin` on the HTTP API.
- Active policies and rollback snapshots are persisted in existing `local_config` keys with hash validation; no database migration or new storage kernel is added. Rollback requires confirmation and a matching active snapshot.
- Exploration requires `ACP_ENABLE_ADAPTIVE_EXPLORATION=1` plus `ACP_ADAPTIVE_EXPLORATION_ACTIVE=1`, is killed by `ACP_ADAPTIVE_EXPLORATION_KILL_SWITCH=1`, is capped at 5%, and excludes high/critical-risk contexts.
- Promoted policies do not carry live execution authority. They can affect `adaptive_provider` only when a workflow node supplies `adaptive_policy_execution` with explicit candidate plans that each still pass AF-3 provider/model/call/token/cost/time/concurrency gates.
AF-5 implementation status:

- Implemented on `codex/adaptive-fusion-af5`, stacked on AF-4.
- The dashboard adds an Adaptive Fusion operator tab for active policy review, snapshot visibility, safety flags, explicit promotion request submission, and snapshot rollback.
- The TypeScript SDK exposes the AF-4 policy list, promotion, and rollback endpoints with explicit confirmation fields.
- AF-5 adds no execution authority, provider calls, default-on routing, provider failover, unattended workers, merge, release, deploy, or apply controls.

Design references:

- OpenRouter Auto Router: <https://openrouter.ai/docs/guides/routing/routers/auto-router>
- OpenRouter Fusion Router: <https://openrouter.ai/docs/guides/routing/routers/fusion-router>
- OpenRouter Auto Exacto: <https://openrouter.ai/docs/guides/routing/auto-exacto>
- Adaptive LLM Routing under Budget Constraints: <https://aclanthology.org/2025.findings-emnlp.1301/>

Every Adaptive Fusion Routing PR must list: completed AF phase, intentionally unfinished phases, live-influence status, provider/cost/concurrency gates, verification, residual risk, rollback, and next phase.

## Before Starting Autonomous Work

1. Read `docs/CURRENT_STATUS.md` only when status facts are unclear or the task updates status.
2. Read `docs/REAL_WORLD_TESTING_PLAYBOOK.md` for PR/merge/CI work, docs cleanup, and real-world pilot tasks.
3. Confirm the proposed task is allowed under the safety gates above.
4. Keep the change commit-sized and run the relevant verification.
5. Run `uv run --no-project python scripts/check_agent_handoff.py`.
6. Update handoff docs before committing and pushing.
