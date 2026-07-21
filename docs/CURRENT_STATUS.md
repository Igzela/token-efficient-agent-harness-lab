# Current Status

Last updated: 2026-07-21.

## Verified Repository State

- Canonical repository: `Igzela/token-efficient-agent-harness-lab`.
- Audited remote `main`: `fe742052152d3189651570b44b2a1c67a4b112d5`.
- That commit is the squash merge of PR #271 (terminal evidence + `acp/*` output path).
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

**Product Golden Path (default-off)** is implemented for the canonical user-task transaction:

`intake → worktree bind → executable graph → existing scheduler advance → real verification receipts → artifact capture → current approval → export_patch or gated acp/* push → terminal evidence`

Gate: `ACP_PRODUCT_GOLDEN_PATH=1` (and existing `ACP_ENABLE_TARGET_REPO_OUTPUT` for git worktrees). Network draft/push also requires `ACP_PRODUCT_GOLDEN_PATH_ALLOW_NETWORK_OUTPUT=1` plus existing target-output remote allowlists.

Authority repairs from PR #270:

- Declared verification commands execute via `CommandNodeExecutor` and are recorded through supervised-patch verification; fabricated `result: pass` is gone.
- `/finalize` does not tick executors; it observes scheduler-owned run state then post-processes.
- Compile stays at `graph_ready` until scheduler leases/advances.
- Executor availability comes from the live pool / registration snapshot.
- Fixture apply helper is labeled `fixture_deterministic` and is not managed-agent evidence.

Evidence/output from PR #271:

- Task-rooted terminal evidence links plan/run/workspace/verification/artifact/approval with explicit unavailable usage/cost (no fabrication).
- `export_patch` writes approved patches under app-owned `exports/`.
- `draft_pr` default is explicit `network_output_unavailable`; with network gate + local remote allow, tests prove `acp/*` branch push without target `main` mutation.

The fragmented manual plan/run/workspace/tick/verify/capture path remains available for compatibility. Legacy `/dispatch` default remains `noop`.

### Residual (packet not fully COMPLETE)

1. **GitHub Draft PR HTTP create** is not CI-proven; local bare-origin proves `acp/*` branch push only. Real Draft PR still needs configured GitHub credentials/host allowlists outside CI.
2. **Managed coding-executor E2E** (CLI/OpenCode/provider) remains blocked on admitted live capability without gate weakening; fixture `command` path is the accepted disposable-repo proof.
3. **Provider-grade scorecard quality / usage / cost** remain explicitly unavailable for the fixture path (correct fail-closed behavior).

## Capability Status

| Capability | State | Current truth |
|---|---|---|
| Product Golden Path | implemented / default-off / residual seal | Schema v30 `product_tasks`; real verification; scheduler-owned advance; terminal evidence; export_patch; gated `acp/*` push. |
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

1. Real GitHub Draft PR HTTP create is residual under existing credential/host gates.
2. Managed non-command coding executors are not CI-proven for golden path.
3. Real-workload evidence corpus (`PE7-REAL-WORKLOAD-EVIDENCE-1`) remains blocked until golden path residual is closed or explicitly accepted with recorded exception.
4. Level-2 and Meta remain blocked.

## Supporting Programs

- **PE-5 Release Provenance**: implemented / no release authority.
- **PE-6 Fault Injection and Recovery Drills**: implemented / disposable only.
- **Post-R7 wire/type governance**: implemented. `scripts/check_wire_codegen_drift.sh` remains the required cross-language drift guard.

## Active Tracks

- `PE7-PRODUCT-GOLDEN-PATH-1`: `IN_PROGRESS` (authority + evidence/output merged via PRs #268–#271; residual GitHub Draft PR HTTP + managed-executor E2E).
- `PE7-REAL-WORKLOAD-EVIDENCE-1`: `BLOCKED_PREREQUISITE`.
- `PE7-HARNESS-EVOLUTION-LEVEL2-GENERATIONAL-CONTROLLER-1`: blocked; Issue #266 proposal only.
- `PE7-META-IMPROVER-EXPERIMENT-1`: blocked.
- `PE7-OPENCODE-BINARY-ADMISSION-1`: deferred.
- `PR3-EXTERNAL-RUNTIME-LIVE-SEAL-1`: parked on Issue #254.
- PR #225: independent presentation-only Dashboard work.

## Open Work Coordination

PRs #268–#271 are merged. No open golden-path implementation PR. Do not activate Level-2, Meta Improver, Vader, or Issue #208 for this packet.

## Safety Boundary

Default-off product gate; no target `main` write, merge, auto-merge, release, or deployment authority. Provider calls remain forbidden in CI. Network output remains double-gated.
