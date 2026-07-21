# Current Status

Last updated: 2026-07-22.

## Verified Repository State

- Canonical repository: `Igzela/token-efficient-agent-harness-lab`.
- Audited remote `main` at the start of Residual Seal 2: `364a2ad24fb494653ccbe3ff2e8b038e9e40d095`.
- That commit is the documentation synchronization after PR #271. Commits `f7293548`, `fe742052`, and `364a2ad2` are all present in order.
- Prior golden-path merges on this line:
  - PR #268 G1 → `178d020e`
  - PR #269 G2–G4 → `8fa85c15`
  - PR #270 authority repair (real verification, scheduler-only advance, live executor pool, fixture honesty, recovery matrix) → `f7293548`
  - PR #271 evidence/output → `fe742052`
- Exact-head CI: PR #270 head `70f883a4` run `29837940355` green; PR #271 head `73f025bc` run `29839301704` green (required jobs).
- Open PR coordination: PR #225 remains presentation-only Dashboard work (theme files only).
- Open research coordination: Issue #266 remains Level-2 proposal only (not the active lane).
- Parked external acceptance: Issue #254 remains repository-agent smoke parking. Issue #208 remains emergency-stopped.

Repository evidence, CI, and current source remain authoritative.

## Current Product Verdict

**Product Golden Path (default-off)** remains `IN_PROGRESS`. The implemented path is:

`intake → worktree bind → executable graph → existing scheduler advance → verification → artifact capture → awaiting_approval → separately authorized approval → explicit output confirmation → durable non-network receipt or phased branch/PR operation`

Gate: `ACP_PRODUCT_GOLDEN_PATH=1` (and existing `ACP_ENABLE_TARGET_REPO_OUTPUT` for git worktrees). Network draft/push also requires `ACP_PRODUCT_GOLDEN_PATH_ALLOW_NETWORK_OUTPUT=1` plus existing target-output remote allowlists.

Authority repairs from PR #270:

- Declared verification commands execute via `CommandNodeExecutor` and are recorded through supervised-patch verification; fabricated `result: pass` is gone.
- `/finalize` does not tick executors; it observes scheduler-owned run state then post-processes.
- Compile stays at `graph_ready` until scheduler leases/advances.
- Executor availability comes from the live pool / registration snapshot.
- Fixture apply helper is labeled `fixture_deterministic` and is not managed-agent evidence.

The Residual Seal 2 audit found that PR #271's terminal/output summary was overstated:

- its combined endpoint could manufacture approval with ordinary execution permission;
- missing output confirmation could still create approval state;
- any JSON output, including network-unavailable or branch-only output, could complete the task;
- the `draft_pr` path pushed a branch but did not create a GitHub Draft PR;
- terminal evidence was dynamically assembled, selected broad-scan records, mislabeled replay/executor facts, and mutated audit state during reads.

The output-authority slice corrects the first four defects through separate `team:admin` approval and `dispatch:execute` output endpoints, exact approval/evidence binding, zero-side-effect confirmation rejection, durable artifact/export receipts, and a progressive branch-push/Draft-PR operation. A branch receipt is not PR evidence; known failure remains non-terminal and outcome-unknown is reconciled through the same operation. The legacy combined endpoint is compatibility-only and invokes both authorities.

The fragmented manual plan/run/workspace/tick/verify/capture path remains available for compatibility. Legacy `/dispatch` default remains `noop`.

### Residual (packet not fully COMPLETE)

1. Canonical persisted terminal evidence, pure reads, exact owner references, and owner-backed replay/scorecard/usage/cost remain for the next slice.
2. Actual process outcome and verification-time pause/kill/lease/version/late-write authority remain for the next slice.
3. Real GitHub Draft PR acceptance remains to be proved in a disposable repository after the output-authority slice merges.
4. Managed coding-executor E2E is mandatory under the current acceptance contract. Fixture evidence cannot substitute for it; an audited unavailable managed executor is an explicit blocker, not an acceptance exception.

## Capability Status

| Capability | State | Current truth |
|---|---|---|
| Product Golden Path | in progress / default-off / Residual Seal 2 | Schema v30 root tasks, scheduler path, separate approval/output authority, and phased Draft PR operation; canonical terminal evidence and final E2E are not yet sealed. |
| Rust API, scheduler, workflow store | implemented / active | `engine/` sole runtime/store authority. |
| Plain `/dispatch` | implemented / default-noop | Unchanged compatibility path. |
| Plans / workflow runs | implemented | Unchanged; golden path reuses them with product bindings. |
| Supervised patch / target output | implemented / default-off | Reused for worktree, verification, capture, approval, export, `acp/*` push. |
| Harness Evolution Level-1 | accepted fixture laboratory | PR #265; not recursive self-improvement. |
| Harness Evolution Level-2 | planning only | Issue #266. |
| Meta Improver | blocked | Requires Level-2 + separate authority. |
| Repository-agent orchestration | parked | Issue #254 / #208. |
| SDKs | active | Product task client methods after PR #269. |
| Dashboard | active | Mission Control product golden path button; PR #225 theme independent. |

## Confirmed Integration Gaps

1. Canonical terminal evidence and authoritative process/verification outcomes remain open.
2. Real GitHub Draft PR acceptance remains under existing credential/host/repository gates.
3. Managed coding-executor E2E remains mandatory and not yet proved.
4. Real-workload evidence remains blocked until the full Golden Path contract passes; there is no implicit fixture-only exception.
5. Level-2 and Meta remain blocked.

## Supporting Programs

- **PE-5 Release Provenance**: implemented / no release authority.
- **PE-6 Fault Injection and Recovery Drills**: implemented / disposable only.
- **Post-R7 wire/type governance**: implemented. `scripts/check_wire_codegen_drift.sh` remains the required cross-language drift guard.

## Active Tracks

- `PE7-PRODUCT-GOLDEN-PATH-RESIDUAL-SEAL-2`: `IN_PROGRESS` (output authority first; terminal evidence/process outcome and final recovery/E2E slices follow).
- `PE7-PRODUCT-GOLDEN-PATH-1`: `IN_PROGRESS` until Residual Seal 2 satisfies the full acceptance contract.
- `PE7-REAL-WORKLOAD-EVIDENCE-1`: `BLOCKED_PREREQUISITE`.
- `PE7-HARNESS-EVOLUTION-LEVEL2-GENERATIONAL-CONTROLLER-1`: blocked; Issue #266 proposal only.
- `PE7-META-IMPROVER-EXPERIMENT-1`: blocked.
- `PE7-OPENCODE-BINARY-ADMISSION-1`: deferred.
- `PR3-EXTERNAL-RUNTIME-LIVE-SEAL-1`: parked on Issue #254.
- PR #225: independent presentation-only Dashboard work.

## Open Work Coordination

PRs #268–#271 are merged. Residual Seal 2 owns current Golden Path implementation; PR #225 remains an independent presentation-only lane. Do not activate Real Workload Evidence, Level-2, Meta Improver, Vader, or Issue #208 yet.

## Safety Boundary

Default-off product gate; no target `main` write, merge, auto-merge, release, or deployment authority. Provider calls remain forbidden in CI. Network output remains double-gated.
