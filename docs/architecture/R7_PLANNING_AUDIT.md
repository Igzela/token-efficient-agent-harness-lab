# R7 Planning Audit

Audit date: 2026-05-30.
Auditor: Claude (read-only, no source changes).

## Audit Table

| Criterion | A: concurrency.rs | B: checkpoint.rs | C: dispatch_decision.rs | D: app_layer/ dir | E: model_gateway.rs |
|---|---|---|---|---|---|
| Line count | 674 | 652 | 887 | 6330 (16 files) | 488 |
| Structure | Single file | Single file | Single file | Module directory (already split) | Single file |
| External imports | **0** — only `pub mod` in workflow/mod.rs | **0** — only `pub mod` in workflow/mod.rs | **15** — budget_manager, executor_adapter, dispatch_ledger, evaluation_stub, dispatch_engine, task_analyzer/mod, task_analyzer/risk, model_selector, cli/claude_code, cli/codex, cli/multi_executor, provider/executor, routing/auto_policies, routing/dynamic_tier_selector | **0** — only `pub mod` in lib.rs | **0** — only `pub mod` in harness/mod.rs |
| Wire contract coupling | None | None | **HIGH** — `build_dispatch_bundle` golden-parity-tested in `dispatch_parity.rs` | None | None |
| Test coverage | 22 inline tests, no external test file | 16 inline tests, no external test file | 16 inline tests + 2 external test files (`test_dispatch_decision.rs`, `dispatch_parity.rs`) | No test file (inline tests only) | 0 inline tests, no external test file |
| Internal boundaries | Clean: DAG types → concurrency types → ConcurrencyController → helpers → tests | Moderate: data types → CheckpointManager (save/load/list/recovery/compensate) → tests | Mixed: constants + 7 data structs + tier selection + budget + gates + evaluation + bundle builder | 16 files, each self-contained, zero inter-module deps | Clean: types → gateway config → gateway logic → tests |
| Behavior drift risk | None (zero consumers) | None (zero consumers) | **High** (15 import sites + golden parity) | None (dead code) | None (zero consumers) |
| Schema/wire drift risk | None | None | **High** (bundle builder output is golden-parity-tested) | None | None |
| Persistence/recovery drift risk | None | **Moderate** — CheckpointManager mixes filesystem I/O, SHA256 hashing, JSON serialization, path traversal prevention, recovery planning, and compensating event generation | None | None | None |
| Visibility churn risk | Zero | Zero | **High** (15 import paths must be updated) | Zero (already a directory) | Zero |
| Diff reviewability | **High** — pure file moves | **High** — pure file moves | Low — import path changes + parity verification | Medium — reorg of dead code adds no value | **High** — pure file moves |
| Pure file move/split? | Yes | Yes | No (import paths change) | Yes (already a directory) | Yes |
| R-series fit (one-axis, behavior-preserving) | Yes | Yes | Risky — import churn + parity coupling | Yes (but no benefit) | Yes |

## Scores

| Candidate | Benefit | Risk | Reviewability | Recommended Now |
|---|---|---|---|---|
| A: concurrency.rs | Medium | Low | High | **Yes — R7 target** |
| B: checkpoint.rs | Medium | Medium | High | Deferred — persistence semantics add complexity |
| C: dispatch_decision.rs | Medium | **High** | Low | **No** — wire contract coupling + 15 import paths |
| D: app_layer/ dir | Low | Low | Medium | **No** — 6330 lines of dead code; reorganization adds no runtime value |
| E: model_gateway.rs | Low | Low | High | Deferred — below threshold at 488 lines; parity code with zero consumers |

## Recommended R7 Target: Candidate A — `engine/src/workflow/concurrency.rs`

### Why A

1. **674 lines, zero external consumers** — the only reference is `pub mod concurrency;` in `workflow/mod.rs`. Splitting changes no import paths.
2. **Clean internal boundaries** — the file already has clear section markers: DAG types (lines 1–81), concurrency types (lines 83–127), ConcurrencyController (lines 129–244), helpers (lines 246–398), and tests (lines 400–674).
3. **No wire contract coupling** — the module is purely about DAG-based concurrency scheduling and file-overlap detection. No golden fixture parity.
4. **No persistence/serialization complexity** — the only I/O is serde derives on structs. No filesystem, no SHA256 hashing, no path traversal logic.
5. **Pure file move** — the split is mechanical: extract types, controller, and helpers into separate files within a `concurrency/` directory. Public API re-exported from `mod.rs` unchanged.
6. **Fits R-series pattern exactly** — same as R3–R6: monolith → module directory, one axis, behavior-preserving, public API unchanged.
7. **22 inline tests** — more than R5 (17) and R6 (16), providing strong regression coverage.

### Why Not the Others

**B (checkpoint.rs):** At 652 lines with zero external consumers, this is a safe split candidate. However, it has higher inherent caution than concurrency.rs because: (a) it mixes filesystem I/O (`fs::create_dir_all`, `fs::write`, `fs::read_to_string`, `fs::read_dir`), (b) it includes SHA256-based checkpoint ID generation (`sha2::Sha256` + `hex::encode`), (c) its `path_for()` method implements path traversal prevention with canonicalization, (d) `create_recovery_plan()` mixes state-dependent branching (resume/restart/compensate/skip) with compensating event generation, and (e) splitting recovery/persistence logic from data types requires care to preserve the deterministic state behavior. The R-series rule of "one axis only" is harder to satisfy cleanly when persistence, recovery, and data types are interleaved. **Recommend deferral to R8** when the project is ready for a more careful split.

**C (dispatch_decision.rs):** This is the highest-risk candidate. It has **15 external import paths** across 12 modules, and its `build_dispatch_bundle` function is golden-parity-tested (`dispatch_parity.rs` asserts exact JSON output match against 20 fixtures). Any split that changes the module path requires updating all 15 import sites and re-verifying golden fixture parity. This is not a pure file move — it's a cross-module refactoring with schema drift risk. **Strongly recommend deferring** to a future R-series round when the wire contract is more stable. See separate recommendation below.

**D (app_layer/ dir reorg):** The app_layer directory is already split into 16 files. It has **zero external consumers** — `pub mod app_layer;` is declared in `lib.rs` but no code anywhere imports from it. The entire module is implemented-but-unwired parity code from the original Python reference implementation. Reorganizing dead code provides no runtime or reviewability benefit. See separate recommendation below.

**E (model_gateway.rs):** At 488 lines with zero external consumers, this is a potential candidate. However, it is: (a) below the 400-line threshold that typically warrants a split, (b) parity code from the original Python reference implementation (like app_layer), (c) has zero inline tests (low regression safety net), and (d) has lower benefit than concurrency.rs because it's smaller and has no test coverage to catch drift. **Recommend deferring** — if a future R-round targets harness/, this would be a secondary candidate alongside sandbox.rs (395 lines).

## Proposed R7 Implementation Structure

### Current: `engine/src/workflow/concurrency.rs` (674 lines, single file)

### Target: `engine/src/workflow/concurrency/` (module directory)

```
engine/src/workflow/concurrency/
├── mod.rs          (~70 lines)  — module declarations, re-exports of all public items,
│                                  inline test module with all 22 existing tests
├── dag_types.rs    (~85 lines)  — DagNode, DagEdge, DagState structs with Default impls
│                                  and default_edge_status helper
├── types.rs        (~50 lines)  — FileOverlap, ScheduleBatch structs with Default impls
│                                  and ScheduleBatch::item_ids()
├── controller.rs   (~120 lines) — ConcurrencyController struct with new(),
│                                  schedule(), detect_file_overlaps(), can_run_parallel()
├── helpers.rs      (~155 lines) — item_id(), metadata(), read_files(), write_files(),
│                                  conflicting_files(), blocking_reason(), edge_blocks()
└── (tests stay in mod.rs)       — all 22 existing tests
```

**Test module:** Stays in `mod.rs` (inline `#[cfg(test)] mod tests`) — same as R3/R4/R5/R6 pattern. All 22 existing tests pass unchanged because the public API is re-exported from `mod.rs`.

### Public API (unchanged)

```rust
// Re-exported from crate::workflow::concurrency
pub use dag_types::{DagNode, DagEdge, DagState};
pub use types::{FileOverlap, ScheduleBatch};
pub use controller::ConcurrencyController;
pub use helpers::{
    item_id, metadata, read_files, write_files,
    conflicting_files, blocking_reason, edge_blocks,
};
```

### Import path changes

**Zero.** The only declaration is `pub mod concurrency;` in `engine/src/workflow/mod.rs`. Since `concurrency` stays at the same path (`crate::workflow::concurrency`), no external code changes.

## Hard Boundaries for R7 Implementation Session

1. No source code behavior changes — pure file reorganization.
2. No API/schema/status-code changes.
3. No wire contract modifications.
4. No type unification or trait abstraction.
5. No new dependencies.
6. No workspace crate changes.
7. No dashboard/SDK changes.
8. No test additions beyond what's needed to verify the split.
9. Public API re-exported from `mod.rs` must be identical to the current public API.
10. All 1140+ existing Rust tests must pass.

## Verification Commands

```bash
# R-series full verification
cargo fmt --check
cargo clippy -p engine -- -D warnings
cargo test -p engine
bash scripts/verify_rust_typescript_stack.sh
uv run --no-project python scripts/check_agent_handoff.py
```

## R7 Implementation Prompt (Ready to Copy)

```
Architecture Refactor R7 — concurrency split

Split the 674-line `engine/src/workflow/concurrency.rs` monolith into
`engine/src/workflow/concurrency/` module directory (5 files).

Structure:
- `mod.rs`: module declarations, re-exports of all public items from
  submodules, inline test module with all 22 existing tests.
- `dag_types.rs`: DagNode, DagEdge, DagState structs with Default impls
  and default_edge_status helper function.
- `types.rs`: FileOverlap, ScheduleBatch structs with Default impls
  and ScheduleBatch::item_ids() method.
- `controller.rs`: ConcurrencyController struct with new(),
  schedule(), detect_file_overlaps(), can_run_parallel() methods.
- `helpers.rs`: item_id(), metadata(), read_files(), write_files(),
  conflicting_files(), blocking_reason(), edge_blocks() public
  helper functions.

Rules:
- Public API re-exported from `crate::workflow::concurrency` must be
  identical.
- All 22 existing inline tests must pass (moved to mod.rs).
- No behavior changes, no schema changes, no new tests needed.
- Follow the same pattern as R3 (task_analyzer), R4 (dag_manager),
  R5 (context_pack), and R6 (model_profiles).

Verify: `cargo fmt --check && cargo clippy -p engine -- -D warnings && cargo test -p engine && bash scripts/verify_rust_typescript_stack.sh && uv run --no-project python scripts/check_agent_handoff.py`
```

## Separate Recommendation: `dispatch_decision.rs`

### Status: Deferred — Wire Contract Coupling

The `dispatch_decision.rs` file is 887 lines with 15 external import paths across 12 modules. Key findings:

- **Golden-parity coupling** — `build_dispatch_bundle` is tested against 20 Python golden fixtures in `dispatch_parity.rs`. The exact JSON output must match.
- **15 import sites** — budget_manager, executor_adapter, dispatch_ledger, evaluation_stub, dispatch_engine, task_analyzer/mod, task_analyzer/risk, model_selector, cli/claude_code, cli/codex, cli/multi_executor, provider/executor, routing/auto_policies, routing/dynamic_tier_selector.
- **Mixed responsibilities** — constants (10 static arrays), 7 data structs (Evidence, ShadowRoute, BudgetReservation, ExecutionGate, RejectedCandidate, DispatchDecision), tier selection logic, budget reservation, execution gates, evaluation, and the bundle builder.
- **Schema drift risk** — any path change (`dispatch_decision/` directory) requires updating all 15 imports and re-verifying golden parity.

### Recommendation: Defer Until Wire Contract Stabilizes

1. **Do not split now** — the import churn + parity coupling makes this the highest-risk R-series candidate.
2. **Type-governance prerequisite** — before splitting, the wire contract types (Evidence, BudgetReservation, etc.) should be stabilized or extracted to a shared `wire_types` module. This is a separate work item.
3. **Future split opportunity** — when the wire contract is stable, the file could split into: `constants.rs`, `types.rs`, `selection.rs` (tier selection + budget), `gates.rs` (execution gates + evaluation), `bundle.rs` (build_dispatch_bundle).
4. **No action needed in R7** — the dispatch_decision recommendation is independent of the concurrency split.

## Separate Recommendation: `app_layer/`

### Status: Dead/Unwired Parity Code

The `app_layer/` directory contains 6,330 lines across 16 files implementing the original Python reference application layer in Rust. Key findings:

- **Zero external consumers** — `pub mod app_layer;` is declared in `lib.rs` but no code anywhere in the engine imports from it.
- **Zero inter-module dependencies** — each file is self-contained; only `use super::*` appears in test modules.
- **Not part of the active runtime** — the HTTP server, dispatch engine, storage, and CLI modules do not use any app_layer types.
- **Originally parity-migrated** — these files were created during Language Migration Phases 1–7 to mirror the Python reference implementation, which has since been retired.

### Recommendation: Keep As-Is, Do Not Delete or Reorganize

1. **Do not reorganize now** — reorganizing dead code adds no runtime, reviewability, or test value.
2. **Do not delete now** — the code may serve as a reference if a future phase needs to wire application-layer concepts (plan workbench, triage, governance) into the active runtime. Premature deletion risks losing that reference.
3. **Mark as optional in future planning** — when a future phase needs application-layer functionality, evaluate whether to wire existing code or rewrite from scratch.
4. **No action needed in R7** — the app_layer recommendation is independent of the concurrency split.

## Unresolved Risks

1. **concurrency.rs DAG types overlap** — `DagNode`, `DagEdge`, `DagState` in concurrency.rs are a subset of the types in `dag_manager/types.rs`. This duplication exists because concurrency.rs was written as a standalone module. The R7 refactor should NOT unify these types (that violates the one-axis rule), but the split should clearly label the concurrency DAG types as "local subset for scheduling" to avoid future confusion.
2. **Harness module size** — after R7, harness/ still has `model_gateway.rs` (488 lines) and `sandbox.rs` (395 lines) as potential future candidates, but both are below the 400-line threshold and are parity code with zero consumers.
3. **dispatch_decision.rs remains deferred** — 15 import sites + golden parity coupling. Needs wire-contract stabilization before any split.
