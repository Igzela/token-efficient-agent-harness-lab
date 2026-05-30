# R5 Planning Audit

Audit date: 2026-05-30.
Auditor: Claude (read-only, no source changes).

## Audit Table

| Criterion | A: app_layer/ dir reorg | B: context_pack.rs | C: dispatch_decision.rs | D: model_profiles.rs |
|---|---|---|---|---|
| Line count | 6330 (16 files) | 1003 | 887 | 840 |
| Structure | Module directory (already split) | Single file | Single file | Single file |
| External imports | **0** — entirely self-contained | **0** — no consumers outside workflow/ | **19** — executor_adapter, evaluation_stub, budget_manager, dispatch_ledger, dispatch_engine, model_selector, provider/executor, task_analyzer, cli/, routing/ | **0** — no consumers outside harness/ |
| Wire contract coupling | None | None | **HIGH** — golden fixture parity test (`dispatch_parity.rs`) depends on `build_dispatch_bundle` | None |
| Test coverage | No test file (inline tests only) | 18 inline tests (self-contained) | 198-line test file + golden fixture tests | 16 inline tests (self-contained) |
| Internal boundaries | 8 files >400 lines; each self-contained | Clean: constants → structs → validators → budget/prune | Mixed: constants + 6 data structs + tier selection + budget + gates + evaluation | Mixed: constants + structs + validation + shadow helpers |
| Behavior drift risk | None (dead code) | None (no consumers) | **High** (any split changes import paths for 19 call sites) | None (no consumers) |
| Schema/wire drift risk | None | None | **High** (bundle builder output is golden-parity-tested) | None |
| Visibility churn risk | Zero (no external imports) | Zero (no external imports) | **High** (19 import paths must be updated) | Zero (no external imports) |
| Diff reviewability | High per file, but 6330 lines of dead code | **High** — pure file moves, mechanical | Low — import path changes + parity verification | **High** — pure file moves, mechanical |
| Pure file move/split? | Yes (already a directory) | Yes | No (import paths change) | Yes |
| R-series fit (one-axis, behavior-preserving) | Yes | Yes | Risky — import churn + parity coupling | Yes |

## Scores

| Candidate | Benefit | Risk | Reviewability | Recommended Now |
|---|---|---|---|---|
| A: app_layer/ dir reorg | Low | Low | Medium | **No** — 6330 lines of dead code; reorganization adds no runtime value |
| B: context_pack.rs | **High** | **Low** | **High** | **Yes** |
| C: dispatch_decision.rs | Medium | **High** | Low | **No** — wire contract coupling + 19 import paths |
| D: model_profiles.rs | Medium | Low | High | **Deferred** — good R6 candidate |

## Recommended R5 Target: Candidate B — `engine/src/workflow/context_pack.rs`

### Why B

1. **Largest single file in workflow/** (1003 lines) with four distinct internal responsibilities: schema constants, data structs, five public validators, and budget compliance/prune logic.
2. **Zero external consumers** — no module outside `workflow/context_pack.rs` imports from it. The only reference in `app_layer/` is a string literal `"context_pack_id"`, not a Rust import. Splitting changes no import paths.
3. **Clean internal boundaries** — the file already has clear section markers: schema versions (lines 10–68), constant sets (lines 20–68), required-field arrays (lines 74–144), data structs (lines 150–256), validation helpers (lines 262–603), budget/prune logic (lines 609–723), tests (lines 729–1003).
4. **No wire contract coupling** — context pack schemas are self-contained; they don't participate in golden fixture parity.
5. **Pure file move** — the split is mechanical: extract types, validators, and budget into separate files within a `context_pack/` directory. Public API re-exported from `mod.rs` unchanged.
6. **Fits R-series pattern exactly** — same as R3 (task_analyzer split) and R4 (dag_manager split): monolith → module directory, one axis, behavior-preserving, public API unchanged.

### Why Not the Others

**A (app_layer/ dir reorg):** The app_layer directory is already split into 16 files. While some files are large (instance_audit 954, policy_candidate 897), the entire module has zero external consumers — it's implemented-but-unwired parity code. Reorganizing dead code provides no runtime or reviewability benefit. If app_layer is ever wired in, a split at that point will be motivated by actual usage patterns.

**C (dispatch_decision.rs):** This is the highest-risk candidate. It has 19 external import paths across 11 modules, and its `build_dispatch_bundle` function is golden-parity-tested (`dispatch_parity.rs` asserts exact JSON output match against 20 fixtures). Any split that changes the module path (e.g., `dispatch_decision/` directory) requires updating all 19 import sites and re-verifying golden fixture parity. This is not a pure file move — it's a cross-module refactoring with schema drift risk. **Strongly recommend deferring to a future R-series round** when the wire contract is more stable.

**D (model_profiles.rs):** At 840 lines with zero external consumers, this is a safe split candidate. However, it has lower benefit than context_pack because: (a) it's smaller, (b) its internal structure is less clearly segmented (validation + shadow helpers + credential detection are interleaved), and (c) context_pack has more distinct responsibilities to separate. Recommended as the **R6 candidate** after R5 validates the pattern.

## Proposed R5 Implementation Structure

### Current: `engine/src/workflow/context_pack.rs` (1003 lines, single file)

### Target: `engine/src/workflow/context_pack/` (module directory)

```
engine/src/workflow/context_pack/
├── mod.rs          (~120 lines) — re-exports, module declarations
├── types.rs        (~120 lines) — ContextBudget, RetrievalPolicy, MemoryDigest, ContextLayers structs + Default impls + to_value()
├── validation.rs   (~350 lines) — check_fields, validate_budget, validate_context_budget, validate_retrieval_policy, validate_memory_digest, 5 public validators
└── budget.rs       (~120 lines) — check_budget_compliance, apply_prune_policy + constant arrays + required-field arrays
```

**Test module:** Stays in `mod.rs` (inline `#[cfg(test)] mod tests`) — same as R3/R4 pattern. All 18 existing tests pass unchanged because the public API is re-exported from `mod.rs`.

### Public API (unchanged)

```rust
// Re-exported from crate::workflow::context_pack
pub use types::{ContextBudget, RetrievalPolicy, MemoryDigest, ContextLayers};
pub use validation::{
    validate_advisor_context_pack_v2, validate_model_context_pack_v2,
    validate_context_retrieval_request, validate_context_retrieval_result,
    validate_context_layers,
};
pub use budget::{check_budget_compliance, apply_prune_policy};
// All constant arrays re-exported from mod.rs
```

### Import path changes

**Zero.** The only declaration is `pub mod context_pack;` in `engine/src/workflow/mod.rs`. Since `context_pack` stays at the same path (`crate::workflow::context_pack`), no external code changes.

## Hard Boundaries for R5 Implementation Session

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
# Primary: all engine tests pass
cargo test -p engine

# Verify public API unchanged (no import errors)
cargo build -p engine

# Handoff doc consistency
uv run --no-project python scripts/check_agent_handoff.py
```

## R5 Implementation Prompt (Ready to Copy)

```
Architecture Refactor R5 — context_pack split

Split the 1003-line `engine/src/workflow/context_pack.rs` monolith into
`engine/src/workflow/context_pack/` module directory (4 files).

Structure:
- `mod.rs`: module declarations, constant arrays (CALL_TYPES, MODEL_ROLES,
  CONTENT_MODES, etc.), required-field arrays (ADVISOR_PACK_REQUIRED,
  MODEL_PACK_REQUIRED, etc.), re-exports of all public items from
  submodules, inline test module with all 18 existing tests.
- `types.rs`: ContextBudget, RetrievalPolicy, MemoryDigest, ContextLayers
  structs with Default impls and to_value() methods.
- `validation.rs`: check_fields, validate_budget, validate_context_budget,
  validate_retrieval_policy, validate_memory_digest internal helpers;
  validate_advisor_context_pack_v2, validate_model_context_pack_v2,
  validate_context_retrieval_request, validate_context_retrieval_result,
  validate_context_layers public validators.
- `budget.rs`: check_budget_compliance, apply_prune_policy public functions.

Rules:
- Public API re-exported from `crate::workflow::context_pack` must be identical.
- All 18 existing inline tests must pass (moved to mod.rs).
- No behavior changes, no schema changes, no new tests needed.
- Follow the same pattern as R3 (task_analyzer) and R4 (dag_manager).

Verify: `cargo test -p engine` (1140+ tests pass)
```
