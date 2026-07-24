# Next Decision

Last updated: 2026-07-24.

## Current Direction

Finish known correctness/security repairs, complete the default-off managed-executor Product Golden Path, run and freeze the first bounded Real Workload Evidence (RWE) corpus, then run small compatibility-preserving Architecture Convergence packets. Rerun the same corpus after convergence before making a Level-2 GO/NO-GO decision. Meta Improver is later and separately authorized; PR #225 is independent and last.

Rust remains the sole authority for state transitions, workflow execution, permissions, budgets, leases, approvals, evidence, output reconciliation, and persistence. TypeScript remains interaction/projection; Python remains a bounded adapter/evaluation/research layer. No second runtime, scheduler, store, evaluator, workspace, output, audit, or rollback owner.

The managed process-boundary repair is complete via PR #281 squash merge `54b5a430…`; Claude authority repair is complete via PR #282 squash merge `95c3528d…`. The audit found no provider-independent worktree-only filesystem mediation for Claude 2.1.217, so managed Claude admission remains fail-closed. No model request, target output, OpenCode admission, Vader/Issue #208 use, or active-Harness mutation is authorized.

## Active Routing

1. `PE7-PRODUCT-GOLDEN-PATH-1` / `PE7-PRODUCT-GOLDEN-PATH-RESIDUAL-SEAL-2` — `IN_PROGRESS`.
2. `PE7-REAL-WORKLOAD-EVIDENCE-1` — `BLOCKED_PREREQUISITE`.
3. `PE7-ARCHITECTURE-CONVERGENCE-1` — `BLOCKED_PREREQUISITE`.
4. `PE7-REAL-WORKLOAD-EVIDENCE-REPLAY-1` — `BLOCKED_PREREQUISITE`.
5. `PE7-HARNESS-EVOLUTION-LEVEL2-GENERATIONAL-CONTROLLER-1` — `BLOCKED_PREREQUISITE`.
6. `PE7-META-IMPROVER-EXPERIMENT-1` — `BLOCKED_PREREQUISITE`.
7. `PE7-OPENCODE-BINARY-ADMISSION-1` and `PR3-EXTERNAL-RUNTIME-LIVE-SEAL-1` remain deferred/parked; PR #225 remains presentation-only and last.

## Packet States

`READY_FOR_EXECUTION` means prerequisites and acceptance are sufficient. `BLOCKED_PREREQUISITE` means a named earlier condition is incomplete. `DECISION_REQUIRED` means safe authority cannot be derived. `IN_PROGRESS` means one branch/PR owns the work. `COMPLETE` means merged, verified, reviewed, and documented.

## Common Execution Protocol

Historical labels retained for handoff compatibility: Packet PR207-REPAIR-1; Packet PE2-RUNTIME-PRODUCER-1; Packet PE4-EVIDENCE-ENTRY-1; Packet TOOL-DISCOVERY-BENCH-1. They are not active routing.

- Refresh actual `main`, PRs/issues, CI, controls, documents, and overlapping ownership.
- Use one focused branch/PR per coherent risk surface; a new head invalidates old CI and review.
- Reuse current scheduler, executor, worktree, verification, artifact, approval, output, replay, scorecard, audit, and `LocalProductStore` owners.
- Bind authority from persisted current owners, not caller assertions; fail closed on stale, conflicting, tampered, duplicate, late, oversized, killed, paused, over-budget, or outcome-unknown state.
- Preserve SQLite/PostgreSQL parity, atomicity, restart, concurrency, idempotency, compensation, and rollback.
- Keep provider execution off in CI, target `main` unchanged, auto-merge disabled, and merge/release/deploy outside the runtime.
- Run focused/full verification, exact-head CI, complete-diff correctness/security review, handoff checks, and explicit rollback review before manual squash merge.

## Hard Stops

- Secret, credential, raw prompt/output/transcript, private-path, or unredacted repository-content exposure.
- A second authority, weakened allowlist, bypassed approval, target-default-branch write, provider call in CI, auto-merge, release, deployment, or production installation.
- Missing proof of process containment, filesystem confinement, model/usage authority, atomicity, restart, concurrency, idempotency, late-write refusal, rollback, or external-effect status.
- Hidden failure, falsified CI/review/evaluator/cost evidence, unresolved objection, overlapping ownership, or required CI not green for the exact head.

## Packet PE7-MANAGED-CLI-PROCESS-BOUNDARY-REPAIR-2 — bounded managed process owner

**State:** `COMPLETE`

PR #281 merged as `54b5a430…`. It proved versioned per-stream/combined limits, bounded concurrent capture, descendant cleanup, typed spawn/wait/timeout/reader/limit/cleanup failures, hardened probing, non-retry after effect, focused fault tests, full applicable verification, exact-head CI, review, and rollback. No provider request or live acceptance was included.

## Packet PE7-CLAUDE-ADMISSION-AUTHORITY-REPAIR-2 — Claude authority

**State:** `COMPLETE`

**Prerequisite:** PE7-MANAGED-CLI-PROCESS-BOUNDARY-REPAIR-2

PR #282 merged as `95c3528d…`; exact-head `30001196729`, full tests `30001196738`, and external validation `30001196749` passed. Exact binary/version/SHA and bounded process/probe controls exist, but provider-independent worktree-only confinement is not proved and real `HOME` must not be inherited. Runtime Claude admission is disabled; no model request was made. Reopen only with a separately reviewed mediation boundary and provider-free probes for admitted, out-of-scope, parent/sibling, `/etc`, `/proc/self/environ`, home/secret paths, symlink escape, Bash, WebFetch, WebSearch, MCP, Agent, Task/subagent, and notebook tools.

## Packet PE7-UTF8-BOUNDARY-REPAIR-1 — deterministic UTF-8 previews

**State:** `COMPLETE`

PR #283 squash-merged as `9ee5544c…`; exact head `472c6608…`; full CI run `30003155716` and exact-head run `30003155742` passed. The shared helper includes the ellipsis in the byte limit, preserves valid UTF-8 boundaries, and keeps hashes based on complete objectives. No external validation run was applicable.

## Packet PE7-CI-ACCELERATION-1 — bounded CI speed maintenance

**State:** `COMPLETE`

PR #284 squash-merged as `456092fb…`; exact head `52ae7720…`; exact-head run `30004445550`, cache-hit full run `30004445554`, and post-merge main run `30006429193` passed. Baseline → cache-hit: Rust 12m28s → 11m05s, PG 11m00s → 11m02s, cutover 10m28s → 8m14s, Docker 6m27s → 5m02s, native 4m22s → 2m34s, TypeScript 50s → 51s, Python 25s → 22s. Main caches are present; no gate was removed. Cargo-audit (~2m09s), full-test duplication, Docker BuildKit, and cache-size/eviction policy remain unoptimized follow-ups.

## Packet PE7-CI-CACHE-BUDGET-1 — bounded cache budget

**State:** `COMPLETE`

PR #285 (`9c8c3a42…`) established the bounded layout; PR #287 (`1bd17d7a…`) removed cutover's full-target restore; PR #289 (`9db4845c…`, exact head `01e67048…`) disabled incremental PG compilation. The initial eight caches used `12,567,992,986` bytes; the final four main caches use `3,870,843,444` bytes. The limit endpoint returned HTTP 402, so the configured account limit is unknown; use a 10 GB reference ceiling and 8 GB operating budget. No repeated-key thrashing or advisory-database cache was found. Old closed-PR caches and the superseded Bun cache were removed only after classification.

Exact-head/full/post-merge verification passed: #285 `30014629247`/`30014629441`/`30017458718`; #287 `30020044817`/`30020044848`; #289 `30025724951`/`30025726379`/main `30026711865`; final docs-sync main `30029185064` passed on attempt 2 after attempt 1 exposed the pre-existing concurrent output-authority test failure. RustSec data still refreshes during `cargo audit`; no security gate or check was removed. This was workflow-only; rollback is revert of #285/#287/#289 plus the docs commits.

## Packet PE7-PRODUCT-OUTPUT-AUTHORITY-CONCURRENCY-REPAIR-1 — concurrent output CAS

**State:** `COMPLETE`

PR #292 squash-merged as `234def24…`; exact head `799674df…`; exact-head run `30082115186` and full tests `30082115131` passed on first attempt. Concurrent identical non-network output callers now reconstruct the winner’s canonical receipt and terminal evidence after the ProductTask version advances; create-path expected-current authority remains strict; conflicting identities fail closed. SQLite and PostgreSQL concurrent coverage retained. Do not start RWE from this packet.

## Packet PE7-MANAGED-EXECUTOR-USAGE-EVIDENCE-1 — multi-executor usage evidence

**State:** `COMPLETE`

PR #294 squash-merged; exact head `58e549ba…`; exact-head `30089192304` and full tests `30089192331` passed first attempt. Unified `execution_usage_event.v1` with Codex/Claude/OpenCode/provider adapters and cross-source reconcile. Evidence only; no second budget owner; live admission still separately blocked per executor.

One Rust-owned `execution_usage_event.v1` contract with adapters for Codex JSONL, Claude Code JSONL, OpenCode read-only SQLite, and provider/proxy responses. Cross-source reconcile prefers higher-precedence sources when counters agree and fails closed on contradictions. Importers produce evidence only; ProductTask budget remains the sole authority. Exact post-call usage evidence does **not** grant pre/cross-call hard budget authority. Live managed admission remains separately gated (Codex mediation/ordering, Claude confinement, OpenCode binary admission).

## Packet PE7-CODEX-TASK-BUDGET-AUTHORITY-1 / PE7-CODEX-SESSION-USAGE-AUTHORITY-1 — Codex budget + session usage

**State:** `COMPLETE` (superseded for product admission by PE7-CODEX-FULL-MEDIATION-ADMISSION-1)

PR #293 squash-merged as `29262bce…`. Established loopback gateway, session usage importer, and partial admission evidence. JSONL alone is not a hard cross-call gate.

## Packet PE7-CODEX-FULL-MEDIATION-ADMISSION-1 — Codex full mediation admission

**State:** `COMPLETE`

**Prerequisite:** PE7-CODEX-TASK-BUDGET-AUTHORITY-1, PE7-MANAGED-EXECUTOR-USAGE-EVIDENCE-1

PR #295 squash-merged as `381571bf…`; exact head `7e3c1f70…`; exact-head run `30091019486` and full tests run `30091019491` passed first attempt on the merge head.

Closes product-managed Codex admission for the **API-key-mediated** path only:

1. Every provider request/retry through Rust `CodexBudgetGateway` (model pin, unforgeable session token, request count, injected/enforced `max_output_tokens`, cumulative residual check before the next call, usage journal restart recovery).
2. Child launch via `/usr/bin/bwrap` filesystem isolation: real operator `HOME`/`.codex` hidden; task-scoped empty `auth.json`; only loopback gateway base URL + session token; real upstream key never in child env.
3. Session JSONL remains corroborating evidence + reconcile; gateway is the cross-call gate. Contradictory counters fail closed.
4. Official ChatGPT-auth Codex path remains **excluded** from Product Golden Path.
5. Network posture: shared host network with credential non-bypass (unprivileged loopback-only netns that still reaches the host gateway is not available on this host profile).

**Admission class (mediated API-key path, when bwrap present):** `fully_admitted_mediated_api_key` (provider-free mediation proof). Live managed Golden Path acceptance is a separate packet.

## Packet PE7-PRODUCT-GOLDEN-PATH-MANAGED-ACCEPTANCE-1 — live managed acceptance

**State:** `BLOCKED_PREREQUISITE` (operator credential + authorization)

**Prerequisite:** PE7-CODEX-FULL-MEDIATION-ADMISSION-1

Provider-free mediation is accepted. A live call is still required for residual-seal completion and is **not** started in this environment because:

* `ACP_CODEX_UPSTREAM_API_KEY` / `OPENAI_API_KEY` are not present in the agent process environment;
* product admission must not scrape or repurpose ChatGPT OAuth from the operator `~/.codex/auth.json`;
* no explicit operator authorization for a bounded live provider spend was provided in-session.

**Exact operator action to unblock (do not paste secrets into chat, git, CI, or evidence):**

```bash
# One-time identity (use real absolute canonical binary path + sha256):
export ACP_ENABLE_CLI_EXECUTION=1
export ACP_CODEX_BIN="$(readlink -f "$(command -v codex)")"
export ACP_CODEX_VERSION=0.145.0
export ACP_CODEX_SHA256="$(sha256sum "$ACP_CODEX_BIN" | awk '{print $1}')"
export ACP_CODEX_MODEL="<admitted-model-id>"
# Parent-only API key for the gateway (never given to the child):
export ACP_CODEX_UPSTREAM_API_KEY="<third-party-or-api-key>"
# Optional OpenAI-compatible base:
# export ACP_CODEX_UPSTREAM_BASE_URL="https://api.openai.com/v1"
```

Then authorize one tightly bounded ordinary coding task on the disposable target (predeclare max tokens/cost; Draft PR only; no target-main write; no auto-merge). After that authorization, re-run `PE7-PRODUCT-GOLDEN-PATH-MANAGED-ACCEPTANCE-1` as its own branch/PR.

Do not start RWE until live managed acceptance completes.

## Packet PE7-PRODUCT-GOLDEN-PATH-1 — canonical user-task orchestration

**State:** `IN_PROGRESS`

The fixture path and output authority (including concurrent CAS repair) are accepted; the managed coding-executor disposable E2E remains open. Do not start RWE until the residual seal is closed or explicitly accepted under its existing contract.

## Packet PE7-PRODUCT-GOLDEN-PATH-RESIDUAL-SEAL-2 — managed acceptance

**State:** `IN_PROGRESS`

Existing PRs #268–#280 provide the intake, graph, scheduler, verification, artifact, approval, output, evidence, model-binding, and managed-process foundations. Codex full mediation (`PE7-CODEX-FULL-MEDIATION-ADMISSION-1`) proves the API-key-mediated path when bwrap is present. The remaining residual-seal gate is one **live** managed coding-executor run through verification, current approval, separate output confirmation, `acp/*` Draft PR, unchanged target `main`, exact terminal evidence, and all live-provider/security requirements — as a separate packet after mediation merges. If live credentials/authorization are absent, complete provider-free admission and document the exact operator action.

## Packet PE7-REAL-WORKLOAD-EVIDENCE-1 — first bounded baseline

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-PRODUCT-GOLDEN-PATH-1

Use one versioned, hash-bound, disposable corpus with multiple ordinary task classes and accepted executor modes where trustworthy. Persist bounded identities/hashes/counters/references through existing evidence owners; record success/failure, retries, usage/cost availability, timeout/cancel, pause/kill, restart, SQLite/PostgreSQL parity, approval/output/terminal evidence, and cleanup.

## Packet PE7-ARCHITECTURE-CONVERGENCE-1 — compatibility convergence

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-REAL-WORKLOAD-EVIDENCE-1

Implement small reviewed packets in order: AC1 Unified ProcessSupervisor, AC2 typed execution boundary, AC3 Golden Path responsibility split, AC4 transaction-scoped domain views, AC5 runtime composition, AC6 authoritative API/SDK/Dashboard convergence, AC7 obsolete-abstraction cleanup. Preserve Rust authority, one database/transaction owner, persisted compatibility, provider-free CI, cancellation/restart/atomicity, and the frozen corpus.

## Packet PE7-REAL-WORKLOAD-EVIDENCE-REPLAY-1 — post-convergence comparison

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-ARCHITECTURE-CONVERGENCE-1

Rerun the identical bounded corpus and compare classifications, retries, usage/cost evidence, recovery, approvals, output reconciliation, terminal evidence, target-main identity, and Draft PR behavior. Do not claim performance or token improvement without comparable evidence.

## Packet PE7-HARNESS-EVOLUTION-LEVEL2-GENERATIONAL-CONTROLLER-1 — Level-2 decision

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-REAL-WORKLOAD-EVIDENCE-REPLAY-1

Issue #266 is proposal-only. First record an evidence-backed GO/NO-GO using Golden Path stability, both RWE results, contamination risk, deterministic budgets, and Level-1 composition. Implement Level-2 only on GO, reusing existing owners, default-off gates, immutable active Harness, sealed evaluator, exact receipts, SQLite/PostgreSQL parity, and no automatic PR/merge/deploy/adoption.

## Packet PE7-META-IMPROVER-EXPERIMENT-1 — separate meta decision

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HARNESS-EVOLUTION-LEVEL2-GENERATIONAL-CONTROLLER-1

Require a pre-registered unseen improvement-task set, immutable evaluator/labels, contamination controls, baselines, `Improvement@K`, statistical test, effect/error thresholds, seeds, budgets, stop/rollback, and immutable active Harness. Otherwise record NO-GO.

## Packet PE7-OPENCODE-BINARY-ADMISSION-1 — real binary admission

**State:** `BLOCKED_PREREQUISITE`

Real OpenCode remains excluded until exact upstream artifact/source identity, checksum, supply-chain evidence, confinement, and review exist. Fixture evidence is not binary admission.

## Packet PR3-EXTERNAL-RUNTIME-LIVE-SEAL-1 — repository-agent smoke

**State:** `BLOCKED_PREREQUISITE`

Issue #254 remains parked and Issue #208 emergency-stopped. Do not run the replacement smoke or use Vader as product runtime.
