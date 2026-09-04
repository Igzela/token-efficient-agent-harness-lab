# Project Roadmap

Last updated: 2026-08-31.

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
| RWE and contemporary comparison | Identity, corpus, evaluator, protocol, lifecycle-cost, and reconstructable old/new inputs are accepted (`common_rwe_evidence_basis`: `COMPLETE`). Contemporary old/new replay was not executed to a decision-grade result due to absent live provider transport and credentials (`contemporary_rwe_replay`: `INSUFFICIENT`). | Common evidence basis; frozen contracts verified by tests; live replay remains evidence-limited. |
| Context Working Set (CWS) | Projection, residency, source-bound rehydration, tool-result reduction, and default-off analysis boundaries are accepted. Live treatment evidence was evaluated via `cws_benchmark_analyze` and found insufficient without live provider requests (`cws_strategy_evidence`: `INSUFFICIENT`); default-off is maintained. | Registered Strategy component; derived context only; default-off maintained. |
| MX1 / Harness Evolution CORE | Provider-free `Harness × Model × Strategy` descriptors, adapters, and deterministic matrix planning are accepted capability. Live provider-free projection yields `INCOMPARABLE` for unexecuted cells (`mx1_c1_1x2x1`: `INCOMPARABLE`). Downstream rungs are halted (`mx1_c1_1x2x3` and `mx1_c1_2x2x3`: `NOT_JUSTIFIED_BY_PRECEDING_GATE`). | Evaluated against hard comparability gates; halts on lower-rung incomparability. |
| Architecture Convergence | Freeze → converge → reconstruct old/new → same-corpus compare methodology is accepted historical design evidence. | Methodology, not an AC0–AC7 packet train to rerun. |
| Level-1, transfer, and memory+skill | Downstream research halted by upstream lower-rung matrix incomparability (`level_1`, `transfer`, `replication`, `memory`, `skill`: `NOT_JUSTIFIED_BY_PRECEDING_GATE`). | Candidate programs remain unadmitted pending comparable lower-rung evidence. |
| Level-2, adoption, Meta, and later recursion | Halted by Level-1 prerequisites (`level_2`, `adoption_decision`, `meta`, `r4`, `r5`, `r6`: `NOT_JUSTIFIED_BY_PRECEDING_GATE`). No autonomous self-adoption or production replacement is permitted. | Future intent; gated by lower-rung evidence and explicit human authority. |

Historical contracts and accepted capability remain useful even when their
packet execution route was superseded. The old packet route and
`FUTURE_ROUTE.md` are not current governance; no 93-packet train is restored.

## Research Mission Closeout and Terminal Dispositions

The previous research Mission **`MISSION-RESEARCH-20260901`** reached a false-positive
`MISSION_COMPLETED` event in the Steward journal when its six preplanned repository-maintenance
Stages finished. Under repaired Mission completion semantics, a complex research Mission cannot
become `COMPLETE` merely because its preplanned Stage list is exhausted.

To resolve this defect and conclude the closed-loop research mainline truthfully, successor
Research Mission **`MISSION-RESEARCH-20260901-SUCCESSOR`** (bound to predecessor
`MISSION-RESEARCH-20260901` and accepted main `0136218d9a4517a2fac99f9f42ddf648a29c85fd`)
established a machine-verifiable 18-node acceptance ledger. Every preserved scientific node
has been evaluated against first-party engine tests, specifications, and evidence, reaching
a genuine terminal disposition:

| Obligation ID | Category | Dependencies | Terminal Disposition | Evidence Summary |
|---|---|---|---|---|
| `common_rwe_evidence_basis` | basis | none | **`COMPLETE`** | Reconciled and validated frozen RWE corpus, protocol, schedule, task bindings, and baseline seeds in `engine/src/rwe`. All 103 canonical unit/integration tests pass (`cargo test --lib rwe`). |
| `contemporary_rwe_replay` | evaluation | `common_rwe_evidence_basis` | **`INSUFFICIENT`** | `ProductGoldenPathCellDriver` fails closed without live provider transport/credentials. No contemporary old/new replay was executed to a decision-grade result; campaign remains evidence-limited. |
| `mx1_c1_1x2x1` | ladder | `common_rwe_evidence_basis` | **`INCOMPARABLE`** | Provider-free matrix projection in `engine/src/harness_evolution.rs` yields `Incomparable("outcome_unknown")` for unexecuted cells; Model effects cannot be isolated without live execution. |
| `mx1_c1_1x2x3` | ladder | `mx1_c1_1x2x1` | **`NOT_JUSTIFIED_BY_PRECEDING_GATE`** | Gated by Rung 1 comparability; halted because upstream `mx1_c1_1x2x1` produced `INCOMPARABLE`. |
| `mx1_c1_2x2x3` | ladder | `mx1_c1_1x2x3` | **`NOT_JUSTIFIED_BY_PRECEDING_GATE`** | Gated by Rung 2 comparability; halted by upstream `mx1_c1_1x2x3`. |
| `cws_strategy_evidence` | evaluation | `common_rwe_evidence_basis` | **`INSUFFICIENT`** | `cws_benchmark_analyze` in `engine/src/context_working_set.rs` yields `CwsAnalysisDisposition::InsufficientDefaultOff` when live arms are absent; default-off is maintained. |
| `harness_evolution` | evaluation | `mx1_c1_2x2x3` | **`NOT_JUSTIFIED_BY_PRECEDING_GATE`** | Candidate Pareto archive and prediction outcomes halted by upstream MX1 matrix ladder failure. |
| `level_1` | gate | `harness_evolution` | **`NOT_JUSTIFIED_BY_PRECEDING_GATE`** | Level-1 candidate eligibility halted by upstream `harness_evolution`. |
| `transfer` | transfer | `level_1` | **`NOT_JUSTIFIED_BY_PRECEDING_GATE`** | Cross-task/domain transfer evaluation halted by upstream `level_1`. |
| `replication` | replication | `level_1` | **`NOT_JUSTIFIED_BY_PRECEDING_GATE`** | Multi-seed replication evaluation halted by upstream `level_1`. |
| `memory` | capability | `level_1` | **`NOT_JUSTIFIED_BY_PRECEDING_GATE`** | Memory retention/eviction evaluation halted by upstream `level_1`. |
| `skill` | capability | `level_1` | **`NOT_JUSTIFIED_BY_PRECEDING_GATE`** | Skill reuse evaluation halted by upstream `level_1`. |
| `level_2` | gate | `level_1`, `transfer`, `replication` | **`NOT_JUSTIFIED_BY_PRECEDING_GATE`** | Level-2 evaluation halted by upstream Level-1/transfer/replication gates. |
| `adoption_decision` | adoption | `level_2` | **`NOT_JUSTIFIED_BY_PRECEDING_GATE`** | No candidate reached Level-2; explicit human adoption review halted. No autonomous self-adoption. |
| `meta` | meta | `level_2` | **`NOT_JUSTIFIED_BY_PRECEDING_GATE`** | Meta program evaluation halted by upstream `level_2`. |
| `r4` | meta | `meta` | **`NOT_JUSTIFIED_BY_PRECEDING_GATE`** | R4 atomic journal concurrency evaluation halted by upstream `meta`. |
| `r5` | meta | `meta` | **`NOT_JUSTIFIED_BY_PRECEDING_GATE`** | R5 distributed observer evaluation halted by upstream `meta`. |
| `r6` | meta | `meta` | **`NOT_JUSTIFIED_BY_PRECEDING_GATE`** | R6 recursive decomposition evaluation halted by upstream `meta`. |

### Summary of Evidence and Completion Invariant

1. **Accepted Capability**: Frozen RWE contracts, deterministic matrix planning, and CWS projection are verified and sound.
2. **Executed Experiments**: Deterministic frozen RWE basis suite (`cargo test --lib rwe`) and harness evolution contract suite (`cargo test --lib harness_evolution`).
3. **Scientific Dispositions**: 1 basis obligation `COMPLETE`, 2 obligations `INSUFFICIENT`, 1 obligation `INCOMPARABLE`, and 14 downstream obligations `NOT_JUSTIFIED_BY_PRECEDING_GATE`.
4. **No Fabricated Success**: The campaign honestly records evidence-limited results. No live provider executions or model adoptions are claimed.
5. **Enforced Dual Completion**: Steward requires both stage settlement AND complete terminal disposition of the acceptance ledger before any Mission can report `COMPLETE`.
