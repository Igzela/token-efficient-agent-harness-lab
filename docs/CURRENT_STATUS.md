# Current Status

Last updated: 2026-07-16.

## Summary

This repository is a local/small-team self-hosted agent workflow control plane. Rust `engine/` remains the sole runtime, API, and application-owned storage implementation. Active documents describe current facts and forward execution; merged PRs and repository history retain detailed stage history.

PR #220, PR #221, and PR #222 closed the local production-call gaps for Agent Runtime, durable memory, PE-2/PE-4, the managed external runtime, and fixture/guarded-live efficiency measurement. PR #223 then merged the provider-backed durable-memory receipt and embedding repair. External live acceptance is still incomplete. PR #224 is the active provider live-safety repair; PR #225 is a presentation-only Dashboard redesign. The GitHub Issues/Actions → Vader Codex repository-maintenance orchestrator remains unaccepted for production task use.

## Verified Repository State

- PR #214 (`PE56-POST-SEAL-REPAIR-1`) is merged at `0d8127e3d779e54c58caf5d93e7589dd1a6df616`;
- PR #214 exact final head `ed5e033a5206d2ddfea2d48381217d0a04b4ceb3` passed exact-head CI run `29250861586`;
- PR #207 merged the disabled-by-default event-driven repository-maintenance orchestrator at `23187bb83dc32165d8982c79be1a1f7f818380a0`;
- PR #216 repaired Codex last-message handling and runner-readiness validation and merged at `2a42c011164765ba6c2dbe940c5a73900a7bb4b1`;
- PR #216 exact head `7210cd1943b075ef07c561f4804bca8230cffd60` passed canonical CI run `29308693744` with all seven required jobs successful;
- PR #220 merged Agent Runtime integration as `936b05c226ab64576c0e2d4146d3f8ca3d0c3e47` after exact-head CI passed all seven required jobs;
- PR #221 merged durable memory plus the PE-2/PE-4 production loop as `f821d366359e1b68376df6dd1eae7a10c9519058`; exact PR-head CI run `29383755592` and post-merge CI run `29384216781` each passed all seven required jobs;
- PR #222 merged the managed external runtime, benchmark, orchestrator repair, and local acceptance seal as `49a5948c4527ba741569f673696cf462db7ac092`; exact head `646118bffa1e2c9c56aead2616cb9526a8457032` passed all seven jobs in CI run `29390757467`;
- PR #223 (`DURABLE-MEMORY-PROVIDER-EMBEDDING-REPAIR-1`) merged exact head `2c31912c4e07e182667d68b14fa20472866d01fe` as `f33b7bb0b49ec902c66b170406efa9d8ee60f9a2`; exact-head CI run `29424971021` passed all seven required jobs;
- PR #224 (`PROVIDER-EMBEDDING-LIVE-SAFETY-REPAIR-1`) is open and mergeable at exact head `861990a6c52c91f640af8db028f7172459abd0c3`; current CI run `29485833293` is in progress, independent review and merge are not complete, and no live provider POST has been made;
- PR #225 (`style(dashboard): adopt a warm Claude-inspired workspace`) is open at head `edc6048be17716a7de5d5949295877f30eaf9249`; it changes only Dashboard presentation files and does not modify runtime, API, storage, permissions, or provider behavior;
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

The task then transitioned to `agent-blocked`. Bounded Actions diagnostics established that Vader produced the artifact, the GitHub-hosted finalizer validated it, and the branch push completed; PR creation then failed with HTTP 403 because the repository does not permit GitHub Actions to create or approve pull requests. The Issue comments do not retain that terminal detail. The control Issue was returned to emergency stop and orchestration was disabled.

The demonstrated failure cause is established from Actions diagnostics, but the required repository setting is still disabled, the Vader runner is currently offline, and Issue comments do not contain the workflow run/job identity or bounded terminal reason. A repaired preflight now checks the setting through a separate least-privilege administration-read credential before dispatch and records bounded terminal evidence, but a replacement smoke has not run.

Operational consequence:

- do not dispatch a production repository task through this orchestrator yet;
- keep Issue #208 emergency-stopped;
- configure the documented administration-read preflight credential, enable the repository PR-creation setting, and restore the named disposable Vader runner;
- run a new bounded smoke through PR creation, exact-head CI, and independent review before declaring the GPT Web path operational.

The intended user interface remains natural language in GPT Web. The assistant, not the user, owns creation of the bounded Issue and the internal workflow parameters. This contract is documented in `README.md` and `AGENTS.md`, but activation remains blocked by the failed smoke.

## Capability Status

| Capability | Current status |
|---|---|
| Dispatch kernel and V2 output authority | Complete |
| Adaptive Fusion through AF-7 | Implemented; real adaptive completion and scheduler paths can execute single, ordered-fallback, and Fusion candidates. The legacy shadow planner remains separate and complexity-driven automatic Fusion selection is deferred until after live acceptance. |
| Agent Runtime through AR-6 | Production-managed through typed `agent_step` plans, the Rust scheduler/executor pool, bounded provider decisions, atomic action receipts, and operator evidence |
| Trusted Local Autonomous Execution through IAE-3 | Complete |
| PE-1 Token Efficiency Regression Lab | Complete and connected through scorecard persistence, read APIs, Dashboard, reports, batches, and trends |
| Durable cross-run memory | Versioned scoped memory, bounded retrieval, runtime context injection, audit, API/SDK, backup, and SQLite/PostgreSQL parity are connected; provider live-safety hardening remains active in PR #224 |
| PE-2 Budget Intelligence and Anomaly Auto-Pause | Normalized usage production, immutable forecast/anomaly artifacts, operator decisions, pause, and recovery are connected |
| PE-3 Operator Decision Center | Complete and connected to typed approval, inspect, acknowledge, rollback, workflow, retry, pause, and recovery owners |
| PE-4 Trace-backed Policy Replay | Eligible dispatch traces produce immutable replay artifacts; explicit evidence-chain promotion and exact-snapshot rollback are connected |
| PE-5 Release Provenance | Post-seal repair merged in PR #214; exact-head CI passed; no real public release or production installation was exercised |
| PE-6 Fault Injection and Recovery Drills | Post-seal repair merged in PR #214; owner-emitted drills are wired into existing test/CI paths; no destructive external testing is authorized |
| Managed LangGraph external runtime | Production-managed by one Rust-leased node with scoped checkpoint/receipt storage, fixture/live gates, automatic scorecard persistence, and bounded inspection; live mode remains default-off |
| Native/LangGraph efficiency benchmark | Canonical four-strategy and tool-discovery contract is connected in fixture and guarded-live modes; deterministic fixture evidence passed, but no provider-backed result is verified |
| GitHub/Vader repository orchestrator | Repair merged in PR #222; smoke #217 failed after branch push because Actions PR creation is disabled; replacement smoke remains blocked and production use is disabled |
| Dashboard product surface | Functional production surface is connected; PR #225 is an in-progress presentation-only redesign toward a warmer, calmer workspace visual system |
| Post-R7 wire/type governance | Implemented through `scripts/check_wire_codegen_drift.sh` |

## Connected Production Chains

### Agent Runtime and tool execution policy

An authenticated `dispatch:execute` caller can create a confirmed typed `agent_step` plan, create its workflow run, and advance exactly one leased step through either the existing scheduler or the explicit tick API. The Rust scheduler remains the sole admission, lease, retry, cooldown, pause/resume, restart, and concurrency owner. Its normal and dynamic background paths now share bounded `ACP_SCHEDULER_MAX_RETRIES` (`0..=10`, default `0`) instead of hard-coding no retries. Lease startup bounds and attempt-CAS completion prevent reclaim while a valid worker is still active and prevent a stale worker from overwriting a recovered attempt. Provider decisions are default-off, require the node model to equal the configured provider model before reservation, use existing provider/cost/audit gates, expose only capability-authorized typed actions, return only the bounded `agent_action.v1` union, and cannot create an internal loop. Atomic `(run_id, node_id)` action receipts make mailbox, memory-digest, child-task, handoff, review, and debate mutations idempotent across retries, process restart, and concurrent claims.

Command and installed-CLI nodes now pass through the same app-owned tool-policy wrapper in scheduler, executor-pool, explicit tick, and supervised-patch verification paths. Direct `cli`, `auto`, and multi/CLI dispatch are retired; scheduler `auto`/`pool` owns hybrid provider/CLI workflow routing. A configured allowlist, including an explicitly empty one, is authoritative. Pre-hook block/error fails closed; bounded enrichment is hash- and audit-bound; approval-required tools enter the existing workflow/operator decision flow and receive only a single exact-action execution authorization; non-approval tools atomically claim an implicit consumed receipt before invocation; post hooks preserve authoritative usage fields. Failures after a claimed effect are explicit non-retryable outcome-unknown results, not automatic retries. Supervised-patch verification has one canonical exact-binding run per workspace/operation/attempt across restart and concurrent requests. Those runs are atomically marked as API-owned and excluded from both scheduler queue modes; the API-owned tick remains the only executor. When a background scheduler is mounted, the handler refuses command or CLI execution unless the durable lease exceeds the exact bound executor timeout by the scheduling margin. An absent allowlist profile preserves the previous unconfigured behavior and is not described as a configured deny policy.

This Agent Runtime is an engine workflow capability. It is independent from the disabled GitHub Issues/Actions → Vader repository-maintenance orchestrator, which remains subject to its separate live-smoke restriction below.

### Durable memory and bounded retrieval

Migration v23 adds app-owned versioned durable memory, retrieval events, normalized usage, fenced production jobs, replay bindings, and operator acknowledgements for SQLite and PostgreSQL. Memory identity and updates bind tenant, workspace, optional agent/task, source ID/hash, version, state, freshness, expiry, confidence, conflict, supersession, tombstone, actor, and record hash. Create, revise, supersede, invalidate, forget, prune, inspect, and retrieve are authenticated production APIs with SDK coverage; every call is rebound to an authoritative run and exact stored tenant/workspace/scope before access. Conflicting, superseded, stale, expired, invalid, and tombstoned records are excluded from current truth; concurrent revisions serialize by memory identity and stale versions fail explicitly.

The scheduler assembles each real node context from durable run state, bounded recent history, the run-scoped digest, and immutable retrieved references before execution. Top-K, bytes, estimated tokens, candidate evidence, selection scores, truncation, and source hashes are bounded and audited without copying raw memory into scorecards. `local_hash_v1` vector generation is default-off, prohibited in CI, explicitly gated, and labeled `harness_derived`; deterministic fixture vectors are test-only. Migration v25 connects the durable-memory owner to a default-off OpenRouter embedding adapter pinned to the currently verified free NVIDIA Nemotron embedding model at 1,536 dimensions. It requires provider execution (legacy gate or a ready trusted-local profile) plus authenticated runtime mode, rejects secret-shaped outbound content before the send claim, and revalidates current catalog identity, text/context capability, and zero pricing before each POST. Catalog reads have bounded retry; each embedding POST is sent once. An app-owned, hash-bound operation receipt atomically binds scope/source, complete catalog evidence, cost reservation, send evidence, attempt count, and one memory/version target. Completed vector/metadata receipts survive restart and are revalidated before reuse without another POST. Definitive pre-effect failures and redacted error audit commit atomically. Every transport, 5xx, or validation failure after a POST is typed as outcome-unknown and cannot be retried. Authenticated reconciliation supports bounded retries only for definitive failures; an unknown outcome can be source/hash-acknowledged for audit but remains blocked. A confirmed re-embedding API creates a new immutable version under the current contract, while an append-only identity-and-pricing registry keeps historical rows and receipts readable during model rotation. The catalog price values and the harness-pinned selection effective date have distinct combined provenance and remain stable across UTC date changes. Target uniqueness prevents different concurrent revisions from both calling the provider. The symbolic `OPENROUTER_API_KEY`, timeout, separate catalog/POST circuit breakers, kill switch, redacted provider audit, and app-owned cost owner remain authoritative. Exact model/pricing/vector hashes bind tenant/workspace/optional agent/task/run, memory/version, and source ID/hash. Provider failure never silently degrades. Lexical retrieval runs only when the request explicitly permits the labeled degradation. SQLite v25 schema and marker changes commit in one immediate transaction and repair only empty partial operation tables; occupied partial tables fail closed. PostgreSQL retains the same transaction/advisory-lock semantics. Integrity scans and backup/storage include binding metadata and operation receipts; the older export/import snapshot does not claim durable-memory coverage. PR #224 is tightening ambiguous post-send classification, tenant-scoped receipt visibility, full documented zero-price validation, reusable bounded provider transport, free-model benchmark admission, and PostgreSQL startup behavior before any controlled live provider POST.

### Automatic budget evidence and trace-backed policy operations

Terminal workflow production and the explicit authenticated recompute API normalize owner-backed native scorecards, provider audit, dispatch/CLI history, adaptive observations, and workflow execution evidence into deduplicated source-ID/hash-bound usage observations. A run aggregate scorecard suppresses its component workflow events so one execution is not counted twice. Provenance distinguishes provider-reported, tokenizer-exact, harness-derived, estimated, and unavailable fields; missing provider, model, pricing, token, or billed-cost dimensions remain absent rather than fabricated as zero. Fenced, restart-safe jobs derive immutable forecast/anomaly artifacts, make them visible through existing API/SDK/Dashboard surfaces, and feed the existing operator queue. Scheduler recovery uses a persisted bounded ascending run cursor plus rotating retry set, so work beyond the first page and failures before restart are revisited without creating a second queue. Only supported fresh evidence can reach the existing explicit budget pause owner; recovery remains audited and compensating.

An app-owned replay production profile lets normal dispatch persistence automatically evaluate eligible traces without provider calls and record immutable offline replay artifacts; an authenticated generate endpoint provides deterministic operator recomputation. The scheduler also owns a persisted bounded dispatch-history cursor and rotating retry set, so an immediate producer failure or restart cannot strand a trace and later rows cannot be starved by a permanently bad row. Replay remains shadow-only. Evidence-chain promotion requires the exact replay binding, candidate and active-policy identity, freshness, current-state rebinding, confirmation, and permission before snapshot-backed mutation. The caller-asserted observation-summary promotion shortcut and its Dashboard mutation control are removed. Typed operator `inspect` is read-only, `acknowledge` records no approval and resolves only the exact source kind/ID/hash, and `rollback` binds the exact adaptive-policy snapshot and current state.

### Managed external runtime and benchmark boundary

Migration v24 adds app-owned, scope-bound external-runtime invocation receipts and LangGraph checkpoint metadata for SQLite and PostgreSQL. The Rust scheduler remains the only owner of workflow admission, lease, retry, pause/resume, concurrency, and authority. A `langgraph_external` node launches exactly one bounded adapter invocation. The Python package owns no queue or product store, receives a content-free typed provider exchange from Rust, and returns only a versioned summary. Fixture mode is network-free. Live mode is default-off and requires authentication, symbolic credentials, exact provider/model identity, pricing and cost gates, timeout, token cap, kill switch, and non-CI execution. `provider_outcome_unknown` is blocked and non-retryable.

The canonical efficiency benchmark compares exactly `full_history`, `summary_memory`, `retrieval_memory`, and `durable_state_bounded_recent` for native and LangGraph runtimes, plus static-all versus deterministic Top-K tool discovery. Every material metric carries provenance, completeness, and confidence; unavailable provider fields remain null. Fixture execution is CI-safe. Provider-backed evidence is not yet verified and must not be inferred from fixture output.

## Confirmed Integration Gaps

### Repository-agent worker completion and evidence

The control, intake, dispatcher, Vader implementation, artifact finalizer, and branch push were exercised, but the first bounded task could not create a PR because the repository Actions PR-creation setting is disabled. The repair adds preflight and bounded failure evidence; external acceptance still requires the repository setting, an online disposable Vader runner, and a replacement smoke.

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

### External live acceptance

No live provider POST, disposable-target repository acceptance, replacement orchestrator smoke, staging-only external fault drill, public tag, or public release has been completed. OpenRouter catalog reads identified a currently free Hy3 endpoint, but catalog inspection is not provider execution and is not live acceptance. These states remain `BLOCKED`, `IN_PROGRESS`, or `RELEASE_READY_NOT_PUBLISHED` according to their actual prerequisites; fixture evidence is not live evidence.

### Local runner boundary

The workflow-owned `LocalRunnerValidationExecutor` intentionally uses the Stub provider and persists bounded scorecards. Its live provider mode remains an explicit local CLI/operator path. The separate `agent_step` provider decision path is production-managed only when provider execution, Agent Runtime, authentication, pricing/cost gates, and kill-switch boundaries are all satisfied; it does not make the local benchmark runner live or authorize provider calls in CI.

## Open Work Coordination

PR #224 owns the active provider live-safety repair. It must complete exact-head CI, two independent reviews, exact reviewed-head merge, and post-merge verification before controlled live provider acceptance begins. PR #225 is independent presentation-only Dashboard work and must not be mixed into #224 or used as evidence for live acceptance.

## Active Execution Order

1. `PR1-AR-RUNTIME-INTEGRATION-1` — production Agent Runtime and tool-policy routing (merged as PR #220).
2. `PR2-MEMORY-BUDGET-POLICY-LOOP-1` — merged as PR #221.
3. `PR3-EXTERNAL-RUNTIME-LIVE-SEAL-1` — merged as PR #222; local/fixture implementation is complete.
4. `DURABLE-MEMORY-PROVIDER-EMBEDDING-REPAIR-1` — merged as PR #223.
5. `PROVIDER-EMBEDDING-LIVE-SAFETY-REPAIR-1` — active in PR #224; CI/review/merge are not complete.
6. Controlled external acceptance — provider embedding, native/LangGraph benchmark, replacement Vader smoke, disposable target repository, and staging drills; not started.

Deferred until the external acceptance sequence is complete:

- converge Adaptive Fusion on one production candidate-selection authority and add explicit complexity/risk-driven automatic Fusion selection without reviving the shadow planner as a second router;
- evaluate an outbound-only `A2A-REMOTE-AGENT-ADAPTER-1` while retaining the Rust scheduler as the sole admission, lease, retry, pause, and state authority;
- perform behavior-preserving modular cleanup of oversized provider/integrity files only after live evidence is stable.

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
