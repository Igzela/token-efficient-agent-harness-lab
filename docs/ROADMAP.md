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
   order interactions only after the lower-rung basis is comparable.

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

### Frontier Status of Research Acceptance Obligations

| Obligation ID | Category | Dependencies | Current Status | Provenance & Evidence Basis |
|---|---|---|---|---|
| `common_rwe_evidence_basis` | basis | none | **`COMPLETE`** | Provenance: `ACCEPTED_STATIC_BASIS`. Reconciled and validated frozen RWE corpus, protocol, schedule, task bindings, and baseline seeds in `engine/src/rwe`. All 103 canonical unit/integration tests pass (`cargo test --lib rwe`). |
| `contemporary_rwe_replay` | evaluation | `common_rwe_evidence_basis` | **`UNRESOLVED`** | Provenance: `EXECUTED_EVIDENCE`. `ProductGoldenPathCellDriver` requires live provider transport and credentials. Operational absence (`lack_of_provider_execution=True`) is not a scientific failure; obligation remains open pending live execution. |
| `mx1_c1_1x2x1` | ladder | `common_rwe_evidence_basis` | **`UNRESOLVED`** | Provenance: `EXECUTED_EVIDENCE`. Provider-free matrix projection in `engine/src/harness_evolution.rs` cannot evaluate Model effects without live execution. Unexecuted cells cannot be classified as `INCOMPARABLE`; obligation remains open. |
| `mx1_c1_1x2x3` | ladder | `mx1_c1_1x2x1` | **`UNRESOLVED`** | Pending upstream `mx1_c1_1x2x1` terminal resolution with valid provenance. |
| `mx1_c1_2x2x3` | ladder | `mx1_c1_1x2x3` | **`UNRESOLVED`** | Pending upstream `mx1_c1_1x2x3` terminal resolution. |
| `cws_strategy_evidence` | evaluation | `common_rwe_evidence_basis` | **`UNRESOLVED`** | Pending live provider requests. CWS remains default-off in production. |
| `harness_evolution` | evaluation | `mx1_c1_2x2x3` | **`UNRESOLVED`** | Candidate Pareto archive and prediction outcomes pending upstream MX1 matrix ladder resolution. |
| `level_1` | gate | `harness_evolution` | **`UNRESOLVED`** | Level-1 candidate eligibility pending upstream `harness_evolution`. |
| `transfer` | transfer | `level_1` | **`UNRESOLVED`** | Cross-task/domain transfer evaluation pending upstream `level_1`. |
| `replication` | replication | `level_1` | **`UNRESOLVED`** | Multi-seed replication evaluation pending upstream `level_1`. |
| `memory` | capability | `level_1` | **`UNRESOLVED`** | Memory retention/eviction evaluation pending upstream `level_1`. |
| `skill` | capability | `level_1` | **`UNRESOLVED`** | Skill reuse evaluation pending upstream `level_1`. |
| `level_2` | gate | `level_1`, `transfer`, `replication` | **`UNRESOLVED`** | Level-2 evaluation pending upstream Level-1/transfer/replication gates. |
| `adoption_decision` | adoption | `level_2` | **`UNRESOLVED`** | No candidate reached Level-2; explicit human adoption review gated. No autonomous self-adoption. |
| `meta` | meta | `level_2` | **`UNRESOLVED`** | Meta program evaluation pending upstream `level_2`. |
| `r4` | meta | `meta` | **`UNRESOLVED`** | R4 atomic journal concurrency evaluation pending upstream `meta`. |
| `r5` | meta | `meta` | **`UNRESOLVED`** | R5 distributed observer evaluation pending upstream `meta`. |
| `r6` | meta | `meta` | **`UNRESOLVED`** | R6 recursive decomposition evaluation pending upstream `meta`. |

### Summary of Evidence and Resumption Posture

1. **Current Mission State**: Truthfully held in **`RESEARCH_PENDING`** under dual-completion invariant.
2. **Accepted Capability**: Frozen RWE contracts, deterministic matrix planning, and CWS projection are verified and sound.
3. **Executed Experiments**: Deterministic frozen RWE basis suite (`cargo test --lib rwe`, 103 passed) validates `common_rwe_evidence_basis`.
4. **No Fabricated Evidence**: Operational absence is never mapped to scientific failure. No live provider executions or model adoptions are claimed without verifiable receipts.
5. **Enforced Dual Completion**: Steward requires both stage settlement AND complete terminal disposition of the acceptance ledger before any Mission can report `COMPLETE`. Direct journal injection of `MISSION_COMPLETED` is rejected.
