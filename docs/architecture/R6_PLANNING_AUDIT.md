# R6 Planning Audit

Audit date: 2026-05-30.
Auditor: Claude (read-only, no source changes).

## Audit Table

| Criterion | A: model_profiles.rs | B: concurrency.rs | C: checkpoint.rs | D: app_layer/ dir | E: dispatch_decision.rs |
|---|---|---|---|---|---|
| Line count | 840 | 674 | 652 | 6330 (16 files) | 887 |
| Structure | Single file | Single file | Single file | Module directory (already split) | Single file |
| External imports | **0** — only `pub mod` in harness/mod.rs | **0** — only `pub mod` in workflow/mod.rs | **0** — only `pub mod` in workflow/mod.rs | **0** — only `pub mod` in lib.rs | **14** — budget_manager, executor_adapter, dispatch_ledger, evaluation_stub, dispatch_engine, task_analyzer/risk, cli/claude_code, cli/codex, provider/executor, cli/multi_executor, routing/dynamic_tier_selector, routing/auto_policies |
| Wire contract coupling | None | None | None | None | **HIGH** — `build_dispatch_bundle` golden-parity-tested in `dispatch_parity.rs` |
| Test coverage | 16 inline tests, no external test file | 22 inline tests, no external test file | 16 inline tests, no external test file | No test file (inline tests only) | 16 inline tests + 2 external test files (`test_dispatch_decision.rs`, `dispatch_parity.rs`) |
| Internal boundaries | Clean: constants → required fields → structs → validation → helpers → shadow helpers → tests | Clean: DAG types → concurrency types → controller → helpers → tests | Moderate: data types → CheckpointManager → tests (persistence/recovery interleaved) | 8 files >400 lines; each self-contained, zero inter-module deps | Mixed: constants + 6 data structs + tier selection + budget + gates + evaluation |
| Behavior drift risk | None (zero consumers) | None (zero consumers) | None (zero consumers) | None (dead code) | **High** (14 import sites + golden parity) |
| Schema/wire drift risk | None | None | None | None | **High** (bundle builder output is golden-parity-tested) |
| Visibility churn risk | Zero | Zero | Zero | Zero (already a directory) | **High** (14 import paths must be updated) |
| Diff reviewability | **High** — pure file moves | **High** — pure file moves | **High** — pure file moves | Medium — reorg of dead code adds no value | Low — import path changes + parity verification |
| Pure file move/split? | Yes | Yes | Yes | Yes (already a directory) | No (import paths change) |
| R-series fit (one-axis, behavior-preserving) | Yes | Yes | Yes | Yes (but no benefit) | Risky — import churn + parity coupling |

## Scores

| Candidate | Benefit | Risk | Reviewability | Recommended Now |
|---|---|---|---|---|
| A: model_profiles.rs | Medium | Low | High | **Yes** |
| B: concurrency.rs | Medium | Low | High | Deferred — good R7 candidate |
| C: checkpoint.rs | Medium | Low | High | Deferred — persistence semantics need higher caution |
| D: app_layer/ dir | Low | Low | Medium | **No** — 6330 lines of dead code; reorganization adds no runtime value |
| E: dispatch_decision.rs | Medium | **High** | Low | **No** — wire contract coupling + 14 import paths |

## Recommended R6 Target: Candidate A — `engine/src/harness/model_profiles.rs`

### Why A

1. **Largest single file in harness/** (840 lines) with four distinct responsibilities: schema constants and enum sets (lines 1–69), required-field arrays (lines 70–110), data structs with Default impls (lines 111–262), validation logic for both profile and shadow routing schemas (lines 264–449), internal helpers including credential detection (lines 451–568), shadow routing query helpers (lines 570–625), and tests (lines 631–840).
2. **Zero external consumers** — no module outside `harness/model_profiles.rs` imports from it. The only reference is `pub mod model_profiles;` in `harness/mod.rs`. Splitting changes no import paths.
3. **Clean internal boundaries** — the file already has clear section markers separating constants, types, validation, helpers, and shadow routing logic.
4. **No wire contract coupling** — model profiles and shadow routing schemas are self-contained; they don't participate in golden fixture parity.
5. **Pure file move** — the split is mechanical: extract types, validation, and helpers into separate files within a `model_profiles/` directory. Public API re-exported from `mod.rs` unchanged.
6. **Fits R-series pattern exactly** — same as R3 (task_analyzer), R4 (dag_manager), R5 (context_pack): monolith → module directory, one axis, behavior-preserving, public API unchanged.

### Why Not the Others

**B (concurrency.rs):** At 674 lines with zero external consumers, this is a safe split candidate. However, it has slightly lower benefit than model_profiles because: (a) it's smaller, (b) its internal structure is already relatively clean (DAG types, concurrency types, controller, helpers), and (c) it would only produce 2–3 subfiles. Recommended as the **R7 candidate**.

**C (checkpoint.rs):** At 652 lines with zero external consumers, this is also safe. However, it has higher inherent caution because: (a) it touches persistence semantics (filesystem I/O, JSON serialization, path traversal prevention), (b) its CheckpointManager mixes creation, loading, listing, recovery planning, and compensating event generation, and (c) splitting recovery/persistence logic from data types requires care to preserve the deterministic state behavior. Recommended as a **future R-series candidate** when the project is ready for a more careful split.

**D (app_layer/ dir reorg):** The app_layer directory is already split into 16 files. It has **zero external consumers** — `pub mod app_layer;` is declared in `lib.rs` but no code anywhere imports from it. The entire module is implemented-but-unwired parity code from the original Python reference implementation. Reorganizing dead code provides no runtime or reviewability benefit. See separate recommendation below.

**E (dispatch_decision.rs):** This is the highest-risk candidate. It has **14 external import paths** across 11 modules, and its `build_dispatch_bundle` function is golden-parity-tested (`dispatch_parity.rs` asserts exact JSON output match against 20 fixtures). Any split that changes the module path (e.g., `dispatch_decision/` directory) requires updating all 14 import sites and re-verifying golden fixture parity. This is not a pure file move — it's a cross-module refactoring with schema drift risk. **Strongly recommend deferring** to a future R-series round when the wire contract is more stable.

## Proposed R6 Implementation Structure

### Current: `engine/src/harness/model_profiles.rs` (840 lines, single file)

### Target: `engine/src/harness/model_profiles/` (module directory)

```
engine/src/harness/model_profiles/
├── mod.rs          (~80 lines)  — module declarations, re-exports of all public items,
│                                  inline test module with all 16 existing tests
├── constants.rs    (~110 lines) — schema versions (MODEL_PROFILE_SCHEMA_VERSION,
│                                  SHADOW_ROUTING_SCHEMA_VERSION), enum sets (TIERS,
│                                  TOOL_STRICTNESS, JSON_TOLERANCE, etc.),
│                                  required-field arrays (PROFILE_REQUIRED,
│                                  SHADOW_ROUTING_REQUIRED)
├── types.rs        (~155 lines) — CostMetadata, ForbiddenPreviousTool,
│                                  ModelHarnessProfile, ShadowRoutingRecommendation
│                                  structs with Default impls and to_value()
├── validation.rs   (~300 lines) — validate_model_harness_profile,
│                                  validate_shadow_routing_recommendation public
│                                  validators; validate_cost_metadata,
│                                  validate_forbidden_previous_tools,
│                                  extract_tool_ids, check_tool_conflict,
│                                  detect_credentials internal helpers
└── shadow.rs       (~65 lines)  — is_shadow_only, can_compare_with_usage_ledger
                                  public functions
```

**Test module:** Stays in `mod.rs` (inline `#[cfg(test)] mod tests`) — same as R3/R4/R5 pattern. All 16 existing tests pass unchanged because the public API is re-exported from `mod.rs`.

### Public API (unchanged)

```rust
// Re-exported from crate::harness::model_profiles
pub use constants::{
    MODEL_PROFILE_SCHEMA_VERSION, SHADOW_ROUTING_SCHEMA_VERSION,
    TIERS, TOOL_STRICTNESS, JSON_TOLERANCE, REASONING_EFFORT,
    PARALLEL_TOOL_PREFERENCE, CACHE_STRATEGY, FALLBACK_POLICY,
    ENFORCEMENT_SCOPES, RECOMMENDATION_VALUES, RISK_LEVELS,
    CREDENTIAL_KEYWORDS, PROFILE_REQUIRED, SHADOW_ROUTING_REQUIRED,
};
pub use types::{
    CostMetadata, ForbiddenPreviousTool,
    ModelHarnessProfile, ShadowRoutingRecommendation,
};
pub use validation::{
    validate_model_harness_profile,
    validate_shadow_routing_recommendation,
};
pub use shadow::{is_shadow_only, can_compare_with_usage_ledger};
```

### Import path changes

**Zero.** The only declaration is `pub mod model_profiles;` in `engine/src/harness/mod.rs`. Since `model_profiles` stays at the same path (`crate::harness::model_profiles`), no external code changes.

## Hard Boundaries for R6 Implementation Session

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

## R6 Implementation Prompt (Ready to Copy)

```
Architecture Refactor R6 — model_profiles split

Split the 840-line `engine/src/harness/model_profiles.rs` monolith into
`engine/src/harness/model_profiles/` module directory (5 files).

Structure:
- `mod.rs`: module declarations, re-exports of all public items from
  submodules, inline test module with all 16 existing tests.
- `constants.rs`: schema versions (MODEL_PROFILE_SCHEMA_VERSION,
  SHADOW_ROUTING_SCHEMA_VERSION), enum sets (TIERS, TOOL_STRICTNESS,
  JSON_TOLERANCE, REASONING_EFFORT, PARALLEL_TOOL_PREFERENCE,
  CACHE_STRATEGY, FALLBACK_POLICY, ENFORCEMENT_SCOPES,
  RECOMMENDATION_VALUES, RISK_LEVELS, CREDENTIAL_KEYWORDS),
  required-field arrays (PROFILE_REQUIRED, SHADOW_ROUTING_REQUIRED).
- `types.rs`: CostMetadata, ForbiddenPreviousTool, ModelHarnessProfile,
  ShadowRoutingRecommendation structs with Default impls and to_value().
- `validation.rs`: validate_model_harness_profile,
  validate_shadow_routing_recommendation public validators;
  validate_cost_metadata, validate_forbidden_previous_tools,
  extract_tool_ids, check_tool_conflict, detect_credentials
  internal helpers.
- `shadow.rs`: is_shadow_only, can_compare_with_usage_ledger
  public functions.

Rules:
- Public API re-exported from `crate::harness::model_profiles` must be
  identical.
- All 16 existing inline tests must pass (moved to mod.rs).
- No behavior changes, no schema changes, no new tests needed.
- Follow the same pattern as R3 (task_analyzer), R4 (dag_manager),
  and R5 (context_pack).

Verify: `cargo fmt --check && cargo clippy -p engine -- -D warnings && cargo test -p engine && bash scripts/verify_rust_typescript_stack.sh && uv run --no-project python scripts/check_agent_handoff.py`
```

## Separate Recommendation: `app_layer/`

### Status: Dead/Unwired Parity Code

The `app_layer/` directory contains 6,330 lines across 16 files implementing the original Python reference application layer in Rust. Key findings:

- **Zero external consumers** — `pub mod app_layer;` is declared in `lib.rs` but no code anywhere in the engine imports from it.
- **Zero inter-module dependencies** — each file is self-contained; only `use super::*` appears in test modules.
- **Not part of the active runtime** — the HTTP server, dispatch engine, storage, and CLI modules do not use any app_layer types.
- **Originally parity-migrated** — these files were created during Language Migration Phases 1–7 to mirror the Python reference implementation, which has since been retired.

### Recommendation: Defer, Do Not Delete or Reorganize

1. **Do not reorganize now** — reorganizing dead code adds no runtime, reviewability, or test value.
2. **Do not delete now** — the code may serve as a reference if a future phase needs to wire application-layer concepts (plan workbench, triage, governance) into the active runtime. Premature deletion risks losing that reference.
3. **Mark as optional in future planning** — when a future phase needs application-layer functionality, evaluate whether to wire existing code or rewrite from scratch.
4. **No action needed in R6** — the app_layer recommendation is independent of the model_profiles split.

## Unresolved Risks

1. **model_profiles regex usage** — `detect_credentials` uses `std::sync::LazyLock<Regex>` with `regex` crate. After split, `validation.rs` will be the only file importing `regex`. Verify that `regex` remains in `Cargo.toml` (it is used elsewhere in the engine).
2. **Harness module size** — after R6, the harness/ module will have one fewer large file, but `model_gateway.rs` (488 lines) and `sandbox.rs` (395 lines) remain. Future R-series rounds may target these.
3. **R7 candidate selection** — concurrency.rs (674 lines, zero consumers) is the recommended next target after R6.
