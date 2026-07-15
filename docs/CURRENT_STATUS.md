# Current Status

Last updated: 2026-07-14.

## Summary

This repository is a local/small-team self-hosted agent workflow control plane. Rust `engine/` remains the sole runtime, API, and application-owned storage implementation. Active documents describe current facts and forward execution; merged PRs and repository history retain detailed stage history.

The repository has broad feature coverage, but several existing capabilities still require integration repair. In addition, the GitHub Issues/Actions → Vader Codex repository-maintenance orchestrator is merged but is not yet accepted for production task use: the first live GPT Web smoke reached the Vader worker and then failed closed before branch or PR creation.

## Verified Repository State

- PR #214 (`PE56-POST-SEAL-REPAIR-1`) is merged at `0d8127e3d779e54c58caf5d93e7589dd1a6df616`;
- PR #214 exact final head `ed5e033a5206d2ddfea2d48381217d0a04b4ceb3` passed exact-head CI run `29250861586`;
- PR #207 merged the disabled-by-default event-driven repository-maintenance orchestrator at `23187bb83dc32165d8982c79be1a1f7f818380a0`;
- PR #216 repaired Codex last-message handling and runner-readiness validation and merged at `2a42c011164765ba6c2dbe940c5a73900a7bb4b1`;
- PR #216 exact head `7210cd1943b075ef07c561f4804bca8230cffd60` passed canonical CI run `29308693744` with all seven required jobs successful;
- Issue #208 currently has only `agent-control` and `agent-emergency-stop`; orchestration and auto-merge enable labels are absent.

## Repository-Agent Smoke Status

GPT Web created bounded smoke Issue #217 with the sole allowed path `docs/agent-smoke-test.md`. The live chain successfully performed:

```text
GPT Web request
→ bounded Agent Task Issue
→ agent-ready intake
→ dispatcher claim
→ agent-worker workflow dispatch
→ agent-running / Vader worker entry
```

The task then transitioned to `agent-blocked` without creating `agent/issue-217`, a pull request, or exact-head CI evidence. The control Issue was returned to emergency stop and orchestration was disabled.

The exact worker failure cause is not yet established from durable repository evidence. Issue comments record only the claimed and dispatched states; they do not contain the workflow run/job identity or a bounded terminal failure reason. Do not infer a Codex, runner, network, token, or finalizer cause without reading the actual Actions and runner diagnostics.

Operational consequence:

- do not dispatch a production repository task through this orchestrator yet;
- keep Issue #208 emergency-stopped;
- repair timeout/failure observability and the demonstrated worker failure through `PR207-SMOKE-REPAIR-1`;
- run `PR207-SMOKE-VERIFY-1` through PR creation, exact-head CI, and independent review before declaring the GPT Web path operational.

The intended user interface remains natural language in GPT Web. The assistant, not the user, owns creation of the bounded Issue and the internal workflow parameters. This contract is documented in `README.md` and `AGENTS.md`, but activation remains blocked by the failed smoke.

## Capability Status

| Capability | Current status |
|---|---|
| Dispatch kernel and V2 output authority | Complete |
| Adaptive Fusion through AF-7 | Implemented; trace-backed replay and explicit evidence-chain promotion now have production owners |
| Agent Runtime through AR-6 | Production-managed through typed `agent_step` plans, the Rust scheduler/executor pool, bounded provider decisions, atomic action receipts, and operator evidence |
| Trusted Local Autonomous Execution through IAE-3 | Complete |
| PE-1 Token Efficiency Regression Lab | Complete and connected through scorecard persistence, read APIs, Dashboard, reports, batches, and trends |
| Durable cross-run memory | Versioned scoped memory, bounded retrieval, runtime context injection, audit, API/SDK, backup, and SQLite/PostgreSQL parity are connected |
| PE-2 Budget Intelligence and Anomaly Auto-Pause | Normalized usage production, immutable forecast/anomaly artifacts, operator decisions, pause, and recovery are connected |
| PE-3 Operator Decision Center | Complete and connected to typed approval, inspect, acknowledge, rollback, workflow, retry, pause, and recovery owners |
| PE-4 Trace-backed Policy Replay | Eligible dispatch traces produce immutable replay artifacts; explicit evidence-chain promotion and exact-snapshot rollback are connected |
| PE-5 Release Provenance | Post-seal repair merged in PR #214; exact-head CI passed; no real public release or production installation was exercised |
| PE-6 Fault Injection and Recovery Drills | Post-seal repair merged in PR #214; owner-emitted drills are wired into existing test/CI paths; no destructive external testing is authorized |
| GitHub/Vader repository orchestrator | Code merged and runner path reached; first live smoke #217 blocked before branch/PR creation, so production use is disabled |
| Post-R7 wire/type governance | Implemented through `scripts/check_wire_codegen_drift.sh` |

## Connected Production Chains

### Agent Runtime and tool execution policy

An authenticated `dispatch:execute` caller can create a confirmed typed `agent_step` plan, create its workflow run, and advance exactly one leased step through either the existing scheduler or the explicit tick API. The Rust scheduler remains the sole admission, lease, retry, cooldown, pause/resume, restart, and concurrency owner. Its normal and dynamic background paths now share bounded `ACP_SCHEDULER_MAX_RETRIES` (`0..=10`, default `0`) instead of hard-coding no retries. Lease startup bounds and attempt-CAS completion prevent reclaim while a valid worker is still active and prevent a stale worker from overwriting a recovered attempt. Provider decisions are default-off, require the node model to equal the configured provider model before reservation, use existing provider/cost/audit gates, expose only capability-authorized typed actions, return only the bounded `agent_action.v1` union, and cannot create an internal loop. Atomic `(run_id, node_id)` action receipts make mailbox, memory-digest, child-task, handoff, review, and debate mutations idempotent across retries, process restart, and concurrent claims.

Command and installed-CLI nodes now pass through the same app-owned tool-policy wrapper in scheduler, executor-pool, explicit tick, and supervised-patch verification paths. Direct `cli`, `auto`, and multi/CLI dispatch are retired; scheduler `auto`/`pool` owns hybrid provider/CLI workflow routing. A configured allowlist, including an explicitly empty one, is authoritative. Pre-hook block/error fails closed; bounded enrichment is hash- and audit-bound; approval-required tools enter the existing workflow/operator decision flow and receive only a single exact-action execution authorization; non-approval tools atomically claim an implicit consumed receipt before invocation; post hooks preserve authoritative usage fields. Failures after a claimed effect are explicit non-retryable outcome-unknown results, not automatic retries. Supervised-patch verification has one canonical exact-binding run per workspace/operation/attempt across restart and concurrent requests. Those runs are atomically marked as API-owned and excluded from both scheduler queue modes; the API-owned tick remains the only executor. When a background scheduler is mounted, the handler refuses command or CLI execution unless the durable lease exceeds the exact bound executor timeout by the scheduling margin. An absent allowlist profile preserves the previous unconfigured behavior and is not described as a configured deny policy.

This Agent Runtime is an engine workflow capability. It is independent from the disabled GitHub Issues/Actions → Vader repository-maintenance orchestrator, which remains subject to its separate live-smoke restriction below.

### Durable memory and bounded retrieval

Migration v23 adds app-owned versioned durable memory, retrieval events, normalized usage, fenced production jobs, replay bindings, and operator acknowledgements for SQLite and PostgreSQL. Memory identity and updates bind tenant, workspace, optional agent/task, source ID/hash, version, state, freshness, expiry, confidence, conflict, supersession, tombstone, actor, and record hash. Create, revise, supersede, invalidate, forget, prune, inspect, and retrieve are authenticated production APIs with SDK coverage; every call is rebound to an authoritative run and exact stored tenant/workspace/scope before access. Conflicting, superseded, stale, expired, invalid, and tombstoned records are excluded from current truth; concurrent revisions serialize by memory identity and stale versions fail explicitly.

The scheduler assembles each real node context from durable run state, bounded recent history, the run-scoped digest, and immutable retrieved references before execution. Top-K, bytes, estimated tokens, candidate evidence, selection scores, truncation, and source hashes are bounded and audited without copying raw memory into scorecards. `local_hash_v1` vector generation is default-off, prohibited in CI, explicitly gated, and labeled `harness_derived`; deterministic fixture vectors are test-only. Provider embedding mode is explicitly unavailable until the managed external-provider adapter exists. Lexical retrieval runs only when the request explicitly permits the labeled degradation. The canonical SQLite online backup includes all app tables while the source connection remains open; the older export/import snapshot does not claim durable-memory coverage.

### Automatic budget evidence and trace-backed policy operations

Terminal workflow production and the explicit authenticated recompute API normalize owner-backed native scorecards, provider audit, dispatch/CLI history, adaptive observations, and workflow execution evidence into deduplicated source-ID/hash-bound usage observations. A run aggregate scorecard suppresses its component workflow events so one execution is not counted twice. Provenance distinguishes provider-reported, tokenizer-exact, harness-derived, estimated, and unavailable fields; missing provider, model, pricing, token, or billed-cost dimensions remain absent rather than fabricated as zero. Fenced, restart-safe jobs derive immutable forecast/anomaly artifacts, make them visible through existing API/SDK/Dashboard surfaces, and feed the existing operator queue. Scheduler recovery uses a persisted bounded ascending run cursor plus rotating retry set, so work beyond the first page and failures before restart are revisited without creating a second queue. Only supported fresh evidence can reach the existing explicit budget pause owner; recovery remains audited and compensating.

An app-owned replay production profile lets normal dispatch persistence automatically evaluate eligible traces without provider calls and record immutable offline replay artifacts; an authenticated generate endpoint provides deterministic operator recomputation. The scheduler also owns a persisted bounded dispatch-history cursor and rotating retry set, so an immediate producer failure or restart cannot strand a trace and later rows cannot be starved by a permanently bad row. Replay remains shadow-only. Evidence-chain promotion requires the exact replay binding, candidate and active-policy identity, freshness, current-state rebinding, confirmation, and permission before snapshot-backed mutation. The caller-asserted observation-summary promotion shortcut and its Dashboard mutation control are removed. Typed operator `inspect` is read-only, `acknowledge` records no approval and resolves only the exact source kind/ID/hash, and `rollback` binds the exact adaptive-policy snapshot and current state.

## Confirmed Integration Gaps

### Repository-agent worker completion and evidence

The control, intake, dispatcher, and worker-entry path are live, but the first bounded task did not reach a branch or PR. The repair must identify the actual failed workflow step and add enough bounded evidence to distinguish queue delay, runner loss, Codex timeout/nonzero exit, artifact rejection, finalizer failure, and control-state interruption without exposing raw prompts, model output, credentials, or unbounded logs.

Required chain:

```text
bounded task
→ durable workflow/run identity
→ bounded worker timeout and terminal reason
→ capacity release
→ validated artifact
→ branch and PR
→ exact-head CI
→ independent review
```

### Tool discovery benchmark

The tool registry and PE-1 regression evidence exist independently. There is no deterministic static-all versus retrieve-Top-K tool selection benchmark, no required-tool recall/selection precision evidence, and no bridge from tool discovery results into PE-1 scorecards and regression reports.

Required chain:

```text
existing tool descriptors
→ deterministic bounded retrieval/Top-K selection
→ paired static-all and retrieved-tool runs
→ quality, recall, precision, token, latency, and cost evidence
→ existing PE-1 scorecard/regression owners
```

This is a benchmark and evidence feature first. It does not authorize dynamic production tool execution.

### Local runner boundary

The workflow-owned `LocalRunnerValidationExecutor` intentionally uses the Stub provider and persists bounded scorecards. Its live provider mode remains an explicit local CLI/operator path. The separate `agent_step` provider decision path is production-managed only when provider execution, Agent Runtime, authentication, pricing/cost gates, and kill-switch boundaries are all satisfied; it does not make the local benchmark runner live or authorize provider calls in CI.

## Open Work Coordination

The explicitly authorized bounded three-PR production-integration program consolidates the original seven capability packets without dropping any acceptance requirement or safety boundary. The GitHub/Vader repair and replacement smoke are owned by the final `PR3-EXTERNAL-RUNTIME-LIVE-SEAL-1` slice; they remain disabled until that slice and do not block the local Rust control-plane integration PRs.

## Active Execution Order

1. `PR1-AR-RUNTIME-INTEGRATION-1` — production Agent Runtime and tool-policy routing (merged as PR #220).
2. `PR2-MEMORY-BUDGET-POLICY-LOOP-1` — durable memory, automatic normalized usage/budget evidence, and trace-replay/evidence-chain policy operations (current slice).
3. `PR3-EXTERNAL-RUNTIME-LIVE-SEAL-1` — managed LangGraph adapter, comparable native/LangGraph benchmark, orchestrator/target-output/recovery, and guarded live acceptance.

Keep real release publication, destructive production faults, persistent signing secrets, and unguarded provider execution unauthorized. Without separate publication authority the terminal release state is `RELEASE_READY_NOT_PUBLISHED`, or `BLOCKED` when an external prerequisite cannot be verified.

The normative packet definitions and acceptance gates are in `docs/NEXT_DECISION.md`.

## Active Documentation

- `README.md`
- `AGENTS.md`
- `CLAUDE.md`
- `docs/ARCHITECTURE_BOOK.md`
- `docs/CURRENT_STATUS.md`
- `docs/NEXT_DECISION.md`
- `docs/MODULE_MAP.md`
- `docs/REAL_WORLD_TESTING_PLAYBOOK.md`
- `docs/RUNBOOK.md`

Do not create another roadmap, status, policy, packet, or closeout document by default. Current direction belongs in `docs/NEXT_DECISION.md`; current facts belong here.
