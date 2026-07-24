# Current Status

Last updated: 2026-07-24.

## Verified Repository State

- Repository: `Igzela/token-efficient-agent-harness-lab`; refreshed `origin/main`: `29262bce793a1a3563ab7e486be68c2ecd5c3789` after PE7 Codex session-usage / loopback budget mediation (#293).
- Open PRs: #225 (presentation-only Dashboard). Auto-merge is disabled.
- Issue #266 is Level-2 proposal-only; Issue #254 is parked; Issue #208 is emergency-stopped.
- Disposable target `Igzela/pe7-golden-path-acceptance-20260722`: Draft PR #1 remains open/draft; target `main` is `926f3d47a2a11e1cdcf05c3a960a5c89cd80679d`.
- Rust `engine/` and `LocalProductStore` remain the sole runtime and application-owned persistence authorities.

## Current Product Verdict

Product Golden Path is default-off and `IN_PROGRESS`.

Fixture evidence proves the existing intake → worktree → graph → scheduler → verification → artifact → approval → output → `acp/*` Draft PR path, but is not managed acceptance. The target default branch remains unchanged.

Managed-executor classification (provider-free, Codex CLI **0.145.0**):

| Executor | Class | Exact residual blocker |
|---|---|---|
| Codex workspace-write | **Partially admissible / evidence-capable** | Exact session JSONL usage (`event_msg`/`token_count` with `total_token_usage` + `last_token_usage`) and optional account-window rate-limit fields are parseable; a loopback budget gateway can inject `max_output_tokens` and count mediated API-key requests. **Not fully admissible:** request-ordering proof fails—real rollouts show further `response_item` provider rounds after `token_count` without an external gate; native `features.rollout_budget.limit_tokens` is not a hard pre/during-call cap; ChatGPT-auth bypass of the loopback gateway is not closed without API-key-only mediation + no child credential. |
| Claude Code | Blocked | Provider-independent worktree-only FS confinement unproved; configured subscription path has known API 404. |
| OpenCode | Blocked | No admitted upstream artifact/checksum. |

Do **not** claim “Codex has no usage authority.” The narrower blockers are: (1) hard cross-call budget interposition before the next provider request, and (2) hard single-request output bounding for non-mediated (ChatGPT-auth) paths. Live managed Golden Path acceptance remains ineligible until full admission is proved.

## Active Work

`PE7-MANAGED-CLI-PROCESS-BOUNDARY-REPAIR-2` is complete through PR #281 squash merge `54b5a430…`. It added versioned bounded stdout/stderr/combined capture, descendant cleanup, typed process failures, a hardened version probe, and non-retryable post-start failures. It did not authorize a provider call or change Golden Path admission.

Phase 2 is complete via PR #282. The audit found no provider-independent worktree-only filesystem mediation for Claude 2.1.217, so managed Claude admission is fail-closed; no model request is permitted unless a separately reviewed mediation boundary proves the packet contract.

Phase 3 is complete via PR #283. Product and Dynamic Workflow previews now truncate only at valid UTF-8 boundaries under a documented byte limit; objective fingerprints remain based on the full objective.

CI cache budgeting is complete through PRs #285, #287, and #289. The final inventory is four main caches totaling `3,870,843,444` bytes; cutover has no full-target restore, PG disables incremental compilation, and RustSec data is not cached.

Downstream order: known repairs → managed-executor Golden Path → frozen first RWE → Architecture Convergence → same-corpus RWE rerun → Level-2 GO/NO-GO → separately authorized Meta decision. PR #225 is independent and last. Architecture Convergence and RWE are not yet eligible.

## Capability Status

| Capability | State | Truth |
|---|---|---|
| Managed CLI process boundary | complete | PR #281; exact-head CI and full applicable checks passed. |
| UTF-8 bounded previews | complete | PR #283; shared byte-boundary helper and Unicode tests passed. |
| Product Golden Path | `IN_PROGRESS`, default-off | Fixture path accepted; managed-executor E2E remains open. |
| Rust runtime/store | active | `engine/` and `LocalProductStore` are sole authorities. |
| Supervised patch/output | default-off | Reused for worktree, verification, artifact, approval, export, and `acp/*` output. |
| Harness Evolution Level-1 | accepted fixture lab | Active Harness immutable; no self-improvement claim. |
| Harness Evolution Level-2 | blocked | Requires post-convergence RWE decision; #266 is proposal-only. |
| Meta Improver | blocked | Requires accepted Level-2 and separate authority. |
| Repository-agent path | parked | #254 / #208. |
| OpenCode binary | deferred | No admitted upstream identity/checksum. |
| Dashboard PR #225 | independent | Presentation-only; handle last. |

## Confirmed Integration Gaps

1. Disposable Draft PR #1 proves real branch/output plumbing only; it binds `acp/product-ptask-20260722135332-18c4a108f1d4e757` at `6c70195c…` to target `main`, which remains `926f3d47…`.
2. Managed coding-executor E2E is not proved. Concurrent non-network output authority is repaired via PR #292 (`234def24…`). Codex is evidence-capable (session usage importer + optional loopback mediation) but not fully admitted for live Golden Path; residual blockers are hard cross-call interposition and single-request output authority on non-mediated paths.
3. RWE has no accepted baseline; Architecture Convergence is blocked until that baseline is frozen.
4. Level-2 and Meta remain blocked.

## Supporting Programs

- **PE-5 Release Provenance:** implemented; no release authority.
- **PE-6 Fault Injection and Recovery Drills:** implemented; disposable only.
- **Post-R7 wire/type governance:** implemented; `scripts/check_wire_codegen_drift.sh` remains required.

## Active Tracks

- `PE7-MANAGED-CLI-PROCESS-BOUNDARY-REPAIR-2`: `COMPLETE` via PR #281 → `54b5a430`.
- `PE7-CLAUDE-ADMISSION-AUTHORITY-REPAIR-2`: `COMPLETE` via PR #282 → `95c3528d`; admission remains disabled pending provider-independent confinement/model authority.
- `PE7-UTF8-BOUNDARY-REPAIR-1`: `COMPLETE` via PR #283 → `9ee5544c`.
- `PE7-CI-ACCELERATION-1`: `COMPLETE` via PR #284 → `456092fb`; exact-head/full cache-hit CI passed and main push CI `30006429193` passed.
- `PE7-CI-CACHE-BUDGET-1`: `COMPLETE` via PR #285 → `9c8c3a42`, #287 → `1bd17d7a`, and #289 → `9db4845c`; final docs-sync main run `30029185064` passed on attempt 2 after the same pre-existing concurrent-test failure on attempt 1. No runtime, gate, provider, or target-branch change.
- Independent PR #288 → `a08d0e28` repaired the newly published PostCSS advisory; its audit and full CI passed. PR #290/#291 are external/contribution maintenance and preserved.
- `PE7-PRODUCT-OUTPUT-AUTHORITY-CONCURRENCY-REPAIR-1`: `COMPLETE` via PR #292 → `234def24`; exact-head/full CI green without concurrency-job retry.
- `PE7-CODEX-TASK-BUDGET-AUTHORITY-1` / `PE7-CODEX-SESSION-USAGE-AUTHORITY-1`: `COMPLETE` via PR #293 → `29262bce` (partial admission only; live E2E not eligible).
- `PE7-PRODUCT-GOLDEN-PATH-RESIDUAL-SEAL-2`: `IN_PROGRESS` through managed acceptance.
- `PE7-PRODUCT-GOLDEN-PATH-1`: `IN_PROGRESS` until the residual seal closes.
- `PE7-REAL-WORKLOAD-EVIDENCE-1`: `BLOCKED_PREREQUISITE` until Golden Path completion.
- `PE7-ARCHITECTURE-CONVERGENCE-1`: `BLOCKED_PREREQUISITE` until the first RWE baseline.
- `PE7-REAL-WORKLOAD-EVIDENCE-REPLAY-1`: `BLOCKED_PREREQUISITE` until convergence.
- `PE7-HARNESS-EVOLUTION-LEVEL2-GENERATIONAL-CONTROLLER-1`: blocked until replay and GO decision.
- `PE7-META-IMPROVER-EXPERIMENT-1`: blocked.
- `PE7-OPENCODE-BINARY-ADMISSION-1`: deferred; `PR3-EXTERNAL-RUNTIME-LIVE-SEAL-1`: parked.
- PR #225: independent presentation-only Dashboard work.

## Open Work Coordination

PR #281–#292 are merged (including external #290/#291 and concurrency repair #292); PR #225 remains separate and last. Concurrent product output authority is repaired. Product Golden Path remains `IN_PROGRESS`: Codex is partially admissible (exact session usage evidence; loopback mediation design) but not fully admitted for live managed acceptance because cross-call hard gates and non-mediated single-request caps are unproved. Claude confinement and OpenCode artifact admission remain blocked. RWE, Architecture Convergence, Level-2, Meta, Vader, and Issue #208 remain blocked.

## Safety Boundary

Default-off product gates; no provider call in CI; no target `main` write, merge, auto-merge, release, or deployment authority. No secret, raw prompt/output/transcript, or fixture-only result may become durable acceptance evidence.
