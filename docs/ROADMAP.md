# Project Roadmap

Last updated: 2026-09-04.

This document owns the high-level milestones and research direction for the
Token-Efficient Agent Harness Lab. Research programs are gated Missions, not a
second execution or governance system.

## Autonomous Steward Migration Milestones

| Milestone | Objective | Status |
|---|---|---|
| **M0: Baseline Recovery** | Green canonical CI, branch ruleset enforcement, and repository hygiene | Complete |
| **M1: Core Contracts** | Define `MaintenanceMission`, `Stage`, and `WorkCard` schemas; short-term compatibility layer | Complete |
| **M2: Shadow Steward** | Natural language intake, proposal digest compilation, shadow replay, and owner-approval binding | Complete |
| **M3: Provider-Free Executor** | SQLite journal, heartbeat, reconciliation loop, K=2 isolated worktree dispatch, and Stage PR integration | Complete |
| **M4: Canary Cutover** | Fault injection drills, single-writer cutover, guarded merge verification, and emergency stop compensation | Complete |
| **M5: Limited Effect Autonomy** | Managed parent effect envelopes, one-use child authorization derivation, and single effect persistence owner in Rust engine | Complete |
| **M6: Control Plane Simplification** | Remove legacy loop/supervisor/dispatcher, delete obsolete workflows, consolidate governance docs to ≤ 7 | Complete |
| **M7: Final Non-Regression Acceptance** | End-to-end mission verification, comprehensive fault and rollback drills, final architecture mapping, and closeout | Complete |
| **M8: Autonomous Steward Closure** | Autonomous control loop, GitHub-authenticated non-replayable approval, single merge owner workflow delegation, and fault matrix | Complete — production Mission `MISSION-9E042A35652D3D4A` completed two autonomously merged Stages (#667 and #674), followed by accepted-main documentation readback (#675) on 2026-08-31 |

Steward is enabling repository-maintenance infrastructure: user-approved
Mission → Steward Stage planning/replanning → bounded WorkCards →
implementation → verification → independent review → exact-head CI → guarded
merge → accepted-main readback. It is not the research objective, an evaluator,
or an authority to replace the active Harness.

## Durable Optimization Objective

> Maximize verifiable and reusable task delivery per total lifecycle cost,
> subject to non-negotiable correctness, safety, evidence, compatibility,
> recovery, and rollback gates.

Token reduction is only one possible optimization. A lower-token result is not
an improvement when quality, reliability, safety, evidence completeness,
comparability, or recovery is weaker. Lifecycle cost includes provider calls and tokens,
monetary cost or explicit unavailability, elapsed time, retries, review and
repair, CI and recovery, human burden, maintenance, compatibility, and rollback.

## Research Horizons: One Closed Evidence and Evolution Loop

The durable research direction is one feedback loop:

```text
accepted Harness H_n + registered Models + registered Strategies
  → common Real Workload Evidence tasks, corpus, evaluator, and budgets
  → controlled Harness × Model × Strategy cells
  → correctness, reliability, safety, evidence, and comparability hard gates
  → causal, interaction, lifecycle-cost, and Pareto analysis
  → candidate or policy decision
  → Harness Evolution and transfer/replication
  → explicitly authorized human adoption
  → accepted Harness H_(n+1)
  → the same common evidence basis
```

Real Workload Evidence (RWE) is the common comparison substrate, not a peer
destination. A valid comparison binds task/corpus identity, evaluator and
verification identity, provider/model identity, comparable budgets and seeds,
quality, reliability, and safety gates, complete lifecycle evidence, and
missingness. Missing tokens, cost, latency, calls, retries, review/repair, CI, recovery, or human
burden remain unavailable; no scalar efficiency score can override a failed
hard gate.

The main experimental factorization is `Harness × Model × Strategy`:

1. `1 × 2 × 1` isolates Model effects.
2. `1 × 2 × 3` adds baseline/no-projection, memory-only, and skill-only
   Strategies, isolating Strategy and Model×Strategy effects.
3. `2 × 2 × 3` introduces a second Harness last, exposing Harness and higher-
   order interactions only after the lower-rung basis is comparable. Candidate `ledger-orchestrated:provider-independent:v1` provides the second Harness candidate for this rung once lower rungs resolve.

Hard-gate filtering precedes efficiency and Pareto analysis. Failed, unknown,
missing, unsupported, or `INCOMPARABLE` cells remain explicit evidence in the
ledger and are never coerced into success. A cell with missing or invalid
required inputs is excluded from a numerical contrast; the affected contrast
remains explicitly `INCOMPARABLE` rather than being silently removed from the
evidence record.

## Accepted Capability, Parked Input, and Future Intent

| Capability | Current truth | Role in the loop |
|---|---|---|
| RWE and contemporary comparison | Identity, corpus, evaluator, protocol, lifecycle-cost, and reconstructable old/new inputs are accepted (`common_rwe_evidence_basis`: `COMPLETE`). Contemporary old/new replay was not executed due to absent live provider transport and credentials; unexecuted runs cannot be mapped to scientific `INSUFFICIENT` and remain operationally unresolved (`contemporary_rwe_replay`: unresolved, pending execution). | Common evidence basis; frozen contracts verified by tests; live replay remains evidence-limited. |
| Context Working Set (CWS) | Projection, residency, source-bound rehydration, tool-result reduction, and default-off analysis boundaries are accepted. Live treatment evidence requires live provider requests; default-off is maintained; obligation remains open pending live execution. | Registered Strategy component; derived context only; default-off maintained. |
| MX1 / Harness Evolution CORE | Provider-free `Harness × Model × Strategy` descriptors, adapters, and deterministic matrix planning are accepted capability. Live cell execution is unexecuted; unexecuted cells cannot be classified as scientific `INCOMPARABLE` and remain operationally unresolved (`mx1_c1_1x2x1`: unresolved, pending execution). Downstream rungs remain pending comparable lower-rung evidence. | Evaluated against hard comparability gates; halts on lower-rung incomparability. |
| Ledger-Orchestrated Harness Candidate (`H_ledger`) | Provider-independent candidate `ledger-orchestrated:provider-independent:v1` implemented and verified under `engine/src/harness_evolution/ledger_orchestration.rs` with deterministic tests (decomposition, self-repair, no-progress guard, truncation recovery, state bounds, cell isolation, fresh-context projection, adapter normalization). R1–R8 audit-repair integration accepted on main (`5db67201`, PR #706): executable provider-free RWE bridge with producer-owned matrix provenance, real lifecycle metrics and usage conservation, canonical contract identity, robust no-progress novelty, typed terminal evidence, and receipt-gated retry safety. Default-off; strictly preserves `Harness × Model × Strategy` matrix factorization. Live cells remain unexecuted for lack of authorized/funded provider effects (no credential, no live-run token, no AGY owner approval as of 2026-09-05); operational absence is not a scientific verdict and the `1×2×1` / `1×2×3` / `2×2×3` ladder stays pending execution, not failed. | Candidate Harness implementation; implemented capability distinct from executed evidence; unexecuted live cells remain unresolved without scientific claims. |
| Architecture Convergence | Freeze → converge → reconstruct old/new → same-corpus compare methodology is accepted historical design evidence. | Methodology, not an AC0–AC7 packet train to rerun. |
| Level-1, transfer, and memory+skill | Downstream research halted pending resolved lower-rung matrix evidence (`level_1`, `transfer`, `replication`, `memory`, `skill`: unresolved / pending upstream). | Candidate programs remain unadmitted pending comparable lower-rung evidence. |
| Level-2, adoption, Meta, and later recursion | Gated by Level-1 prerequisites (`level_2`, `adoption_decision`, `meta`, `r4`, `r5`, `r6`: unresolved / pending upstream). No autonomous self-adoption or production replacement is permitted. | Future intent; gated by lower-rung evidence and explicit human authority. |

Historical contracts and accepted capability remain useful even when their
packet execution route was superseded. The old packet route and
`FUTURE_ROUTE.md` are not current governance; no 93-packet train is restored.

## Research Mission Closeout and Real Evidence Acquisition Status

The previous research Mission **`MISSION-RESEARCH-20260901`** reached a false-positive
`MISSION_COMPLETED` event in the Steward journal when its six preplanned repository-maintenance
Stages finished. PR #693 introduced a vital dual-completion root-fix requiring both Stage settlement
and terminal acceptance ledger disposition, but attempted an invalid synthetic closeout via:
1. Manufactured synthetic owner approval bypassing authentic GitHub Issue #208 controls;
2. A predetermined 18-node disposition answer key;
3. Conflation of operational absence (`lack_of_provider_execution`, absent credentials, unexecuted cells) with scientific inadequacy (`INSUFFICIENT` / `INCOMPARABLE`);
4. Unvalidated dispositions lacking cryptographic provenance receipts;
5. Direct journal injection bypassing the Steward's mission completion eligibility checks.

### Audit Invalidation and Corrective Continuation

To restore scientific integrity while strictly preserving the dual-completion invariant:
- The historical fake closeout events (sequences 1373–1392 in the Steward journal) have been audited and explicitly invalidated (`MISSION_CLOSEOUT_INVALIDATED`).
- The original mission `MISSION-RESEARCH-20260901` is continued correctively (`MISSION_CORRECTIVE_CONTINUATION`) under authentic Issue #208 authority.
- The dual-completion rule remains active: a Mission cannot reach `COMPLETE` while its acceptance ledger has unresolved obligations.
- Dispositions must carry verified provenance receipts (`make_provenance_receipt`) bound to accepted main SHA, evidence producer/evaluator identities, and explicit missingness checks. Operational absence is strictly prohibited from being recorded as scientific failure.

### Frozen Campaign Package Seam (`rwe_campaign_package.v1`)

To prevent silent provider substitution, unverified execution, or configuration drift during the research campaign:
- A Store-owned, immutable campaign descriptor schema `rwe_campaign_package.v1` (`FrozenCampaignPackage` in `engine/src/rwe/campaign_package.rs`) binds the complete execution context:
  - `package_id`, `provider_kind`, `models`, `binary_path`, `binary_sha256`, `four_cell_count`, `budgets` (per-call token budget, maximum spend ceiling, timeout seconds), and `rollback_strategy`.
- **Canonical DeepSeek v2 Freeze** (`canonical_deepseek_v2_package()`):
  - Binds the frozen Decision B DeepSeek v2 models (`deepseek-chat` and `deepseek-coder`).
  - DeepSeek balance is currently 0: live execution is blocked by operational absence (`lack_of_provider_execution=true`). Under non-evidence and integrity rules, this is strictly operational absence and MUST NOT be recorded as scientific failure (`INSUFFICIENT` or `INCOMPARABLE`).
- **Candidate AGY v1 Package** (`canonical_agy_v1_candidate_package()`):
  - Binds the local `agy` CLI binary (`/usr/local/bin/agy` or `~/.local/bin/agy`).
  - Marked with `requires_owner_approval: true` and `live_authorization_required: true`.
  - Prohibits silent substitution: AGY can only execute cells if a Store-owned package is explicitly approved by repository authority and granted live authorization.
- **Fail-Closed Driver Enforcement**:
  - `ProductGoldenPathCellDriver` verifies `campaign_package` in `ensure_effects_ready()`.
  - Unapproved or unauthorized candidate packages fail closed before any authority consumption or execution attempt.
  - Package identity and canonical SHA256 hashes are recorded in `LocalProductStore` audit entries via `record_campaign_package_audit()`.

### Frontier Status and 6-Dimensional Accounting of Acceptance Obligations

Each of the 18 research acceptance obligations is accounted for across six mandatory dimensions:
1. **Confirmed Facts**: Provable invariants established by current code, contracts, or journals.
2. **Missing Evidence**: Empirical data or execution outputs currently absent.
3. **Code & Test Evidence**: Verified repository implementations and passing regression suites.
4. **Real Provider Status**: Current transport availability, account balance, and execution reality.
5. **Target Write Status**: Whether any target write has occurred or is permitted (`target_write_performed=false`).
6. **Remaining Blockers**: Specific prerequisites, authorizations, or dependencies blocking terminal resolution.

| Obligation ID | Category | Dependencies | Current Status | 1. Confirmed Facts | 2. Missing Evidence | 3. Code & Test Evidence | 4. Real Provider Status | 5. Target Write Status | 6. Remaining Blockers |
|---|---|---|---|---|---|---|---|---|---|
| `common_rwe_evidence_basis` | basis | none | **`COMPLETE`** | Frozen operator corpus, protocol, schedule, task bindings, baseline seeds v1/v2, and campaign package seam deterministically bound. | None for static basis. | `engine/src/rwe/operator_corpus.rs`, `engine/src/rwe/campaign_package.rs`, `engine/src/rwe/live_baseline_coordinator.rs`; 109 unit/integration tests pass (`cargo test --lib rwe`). | Provider-free basis; no live provider required for static contracts. | `target_write_performed=false`; zero mutation. | None (fully resolved and accepted). |
| `contemporary_rwe_replay` | evaluation | `common_rwe_evidence_basis` | **`UNRESOLVED`** | Golden-path prerequisite and driver scaffolding verified; fail-closed guards tested. | Real provider responses from 4-cell golden path replay under live transport. | `engine/src/rwe/live_baseline_coordinator.rs`; preflight and simulated transport verified. | DeepSeek v2 balance = 0 (`lack_of_provider_execution=true`); AGY candidate package unapproved/unauthorized. | `target_write_performed=false`; no target write permitted. | Lack of funded/authorized live provider transport. Operational absence must remain open, never mapped to `INSUFFICIENT`. |
| `mx1_c1_1x2x1` | ladder | `common_rwe_evidence_basis` | **`UNRESOLVED`** | 1x2x1 matrix configuration and seed progression modeled in harness evolution. | Live cell execution evidence for 1x2x1 matrix slice. | `engine/src/harness_evolution.rs`; deterministic matrix planner tests pass. | Operational absence (zero live provider executions). | `target_write_performed=false`. | Live provider execution required; unexecuted cells cannot be classified as `INCOMPARABLE`. |
| `mx1_c1_1x2x3` | ladder | `mx1_c1_1x2x1` | **`UNRESOLVED`** | 1x2x3 matrix configuration and seed progression modeled. | 3-seed live matrix execution outputs. | `engine/src/harness_evolution.rs`. | Operational absence (zero live provider executions). | `target_write_performed=false`. | Blocked by upstream `mx1_c1_1x2x1` terminal resolution. |
| `mx1_c1_2x2x3` | ladder | `mx1_c1_1x2x3` | **`UNRESOLVED`** | Full 2x2x3 candidate matrix ladder contract modeled. | 2x2x3 matrix execution results across models and seeds. | `engine/src/harness_evolution.rs`. | Operational absence (zero live provider executions). | `target_write_performed=false`. | Blocked by upstream `mx1_c1_1x2x3` terminal resolution. |
| `cws_strategy_evidence` | evaluation | `common_rwe_evidence_basis` | **`UNRESOLVED`** | CWS strategy projection logic verified; production default-off invariant enforced. | Live request-level CWS evaluation empirical data. | `engine/src/harness_evolution.rs`. | Operational absence (zero live requests executed). | `target_write_performed=false`; default-off preserved. | Blocked by upstream live execution. |
| `harness_evolution` | evaluation | `mx1_c1_2x2x3` | **`UNRESOLVED`** | Pareto front calculation and evolutionary candidate scoring implemented. | Real empirical Pareto archive derived from executed matrix ladders. | `engine/src/harness_evolution.rs`. | Operational absence. | `target_write_performed=false`. | Blocked by upstream `mx1_c1_2x2x3` completion. |
| `level_1` | gate | `harness_evolution` | **`UNRESOLVED`** | Level-1 candidate qualification criteria specified. | Evaluated candidate metrics meeting Level-1 threshold. | `engine/src/harness_evolution.rs`. | Operational absence. | `target_write_performed=false`. | Blocked by upstream `harness_evolution`. |
| `transfer` | transfer | `level_1` | **`UNRESOLVED`** | Cross-domain/cross-task transfer evaluation protocol specified. | Empirical transfer performance scores across domains. | `engine/src/rwe` and evolution specs. | Operational absence. | `target_write_performed=false`. | Blocked by upstream `level_1` gate. |
| `replication` | replication | `level_1` | **`UNRESOLVED`** | Replication protocol across independent seeds specified. | Multi-seed replication run outputs. | `engine/src/rwe/operator_corpus.rs`. | Operational absence. | `target_write_performed=false`. | Blocked by upstream `level_1` gate. |
| `memory` | capability | `level_1` | **`UNRESOLVED`** | Memory retention and eviction metrics defined. | Live harness memory bench data. | Memory telemetry interfaces in `engine/`. | Operational absence. | `target_write_performed=false`. | Blocked by upstream `level_1` gate. |
| `skill` | capability | `level_1` | **`UNRESOLVED`** | Skill library reuse metrics defined. | Live skill invocation and reuse empirical results. | Skill evaluation interfaces in `engine/`. | Operational absence. | `target_write_performed=false`. | Blocked by upstream `level_1` gate. |
| `level_2` | gate | `level_1`, `transfer`, `replication` | **`UNRESOLVED`** | Level-2 qualification gate requirements defined. | Candidate passing Level-1, Transfer, and Replication composite gates. | Level-2 gate evaluation contracts in `engine/`. | Operational absence. | `target_write_performed=false`. | Blocked by upstream `level_1`, `transfer`, and `replication` gates. |
| `adoption_decision` | adoption | `level_2` | **`UNRESOLVED`** | Policy mandates explicit human authority; autonomous self-adoption is strictly prohibited. | Human review and signed adoption authority decision. | Policy guards in `docs/ARCHITECTURE.md`, `docs/AUTONOMY.md`, `engine/src/rwe/runner.rs`. | Operational absence. | No production writes or replacements permitted. | Blocked by upstream `level_2` and explicit human governance approval. |
| `meta` | meta | `level_2` | **`UNRESOLVED`** | Meta-agent optimization framework and safety bounds specified. | Meta-optimization execution trace and validated performance gains. | Meta-evaluation scaffolding in `engine/src/meta`. | Operational absence. | `target_write_performed=false`. | Blocked by upstream `level_2` resolution. |
| `r4` | meta | `meta` | **`UNRESOLVED`** | R4 atomic journal concurrency architecture specified. | Concurrent multi-agent journal stress test empirical telemetry. | Journal concurrency models in `engine/`. | Operational absence. | `target_write_performed=false`. | Blocked by upstream `meta`. |
| `r5` | meta | `meta` | **`UNRESOLVED`** | R5 distributed observer architecture specified. | Distributed consensus and observation empirical validation. | Observer interfaces in `engine/`. | Operational absence. | `target_write_performed=false`. | Blocked by upstream `meta`. |
| `r6` | meta | `meta` | **`UNRESOLVED`** | R6 recursive decomposition boundary and rollback invariants specified. | Multi-level recursive decomposition execution receipts. | Recursive dispatch safety limits in `engine/`. | Operational absence. | `target_write_performed=false`. | Blocked by upstream `meta`. |

### Summary of Evidence and Resumption Posture

1. **Current Mission State**: Truthfully held in **`RESEARCH_PENDING`** under dual-completion invariant.
2. **Accepted Capability**: Frozen RWE contracts, deterministic matrix planning, CWS projection, and campaign package seam are verified and sound.
3. **Executed Experiments**: Deterministic frozen RWE basis suite (`cargo test --lib rwe`, 109 passed) validates `common_rwe_evidence_basis`.
4. **No Fabricated Evidence**: Operational absence is never mapped to scientific failure. No live provider executions or model adoptions are claimed without verifiable receipts.
5. **Campaign Package Seam**: `rwe_campaign_package.v1` guards against silent provider substitution; AGY candidate package is explicitly defined as requiring owner approval and live authorization.
6. **Enforced Dual Completion**: Steward requires both stage settlement AND complete terminal disposition of the acceptance ledger before any Mission can report `COMPLETE`. Direct journal injection of `MISSION_COMPLETED` is rejected.
