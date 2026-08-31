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
an improvement when quality, safety, evidence completeness, comparability, or
recovery is weaker. Lifecycle cost includes provider calls and tokens,
monetary cost or explicit unavailability, elapsed time, retries, review and
repair, CI and recovery, human burden, maintenance, compatibility, and rollback.

## Research Horizons: One Closed Evidence and Evolution Loop

The durable research direction is one feedback loop:

```text
accepted Harness H_n + registered Models + registered Strategies
  → common Real Workload Evidence tasks, corpus, evaluator, and budgets
  → controlled Harness × Model × Strategy cells
  → correctness, safety, evidence, and comparability hard gates
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
quality and safety gates, complete lifecycle evidence, and missingness. Missing
tokens, cost, latency, calls, retries, review/repair, CI, recovery, or human
burden remain unavailable; no scalar efficiency score can override a failed
hard gate.

The main experimental factorization is `Harness × Model × Strategy`:

1. `1 × 2 × 1` isolates Model effects.
2. `1 × 2 × 3` adds baseline/no-projection, memory-only, and skill-only
   Strategies, isolating Strategy and Model×Strategy effects.
3. `2 × 2 × 3` introduces a second Harness last, exposing Harness and higher-
   order interactions only after the lower-rung basis is comparable.

Hard-gate filtering precedes efficiency and Pareto analysis. Failed, unknown,
missing, unsupported, or `INCOMPARABLE` cells remain explicit evidence and are
never coerced into success or silently removed from a contrast.

## Accepted Capability, Parked Input, and Future Intent

| Capability | Current truth | Role in the loop |
|---|---|---|
| RWE and contemporary comparison | Identity, corpus, evaluator, protocol, lifecycle-cost, and reconstructable old/new inputs are accepted. The historical contemporary replay did not produce a decision-grade baseline. | Common evidence basis; architecture changes are compared against it when the missing gates are repaired. |
| Context Working Set (CWS) | The projection, residency, source-bound rehydration, tool-result reduction, and default-off analysis boundaries are accepted. Live treatment evidence was insufficient and no enablement is claimed. | An optional, registered Strategy component; it is derived/rebuildable model-visible context, never truth, memory, Store, evaluator, scheduler, or authority. |
| MX1 / Harness Evolution CORE | The provider-free `Harness × Model × Strategy` descriptors, adapters, deterministic matrix planning, baseline/memory-only/skill-only Strategies, and `INCOMPARABLE` projections are accepted capability. | Parked input for a controlled C1 evaluation; the live pilot/effects and any winner adoption are not accepted results. |
| Architecture Convergence | The freeze → converge → reconstruct old/new → same-corpus compare methodology is accepted historical design evidence. | Methodology, not an AC0–AC7 packet train to rerun. |
| Level-1, transfer, and memory+skill | Future research intent gated by common evidence, replication, and transfer evidence. | Candidate programs decomposed into Steward Stages only after a Mission is approved. |
| Level-2, adoption, Meta, and later recursion | Future intent only; no automatic controller, self-update, model-weight change, evaluator co-evolution, or production replacement is authorized. | Requires evidence-backed GO/NO-GO, explicit adoption authority, and return to the common loop. |

Historical contracts and accepted capability remain useful even when their
packet execution route was superseded. The old packet route and
`FUTURE_ROUTE.md` are not current governance; no 93-packet train is restored.

## Research Mission Gates and Next Frontier

No research Mission is currently active or authorized on accepted `main`. The
next permitted research Mission, if separately owner-approved, is an **MX1 C1
controlled-evaluation Mission on the common RWE basis**, beginning with fresh
descriptor, corpus/evaluator, preflight, authority, and parked-input
reconciliation. This names a research frontier, not a live Provider or spend
authorization. The accepted CORE does not authorize the pilot, and the
emergency-stop control remains untouched.

Advancement requires, in order:

- a complete common evidence basis and exact immutable Harness, Model, and
  Strategy descriptors;
- separate finite authority for any effect, with no Provider call, target
  write, or adoption implied by a contract or preflight;
- complete lower-rung blocks and hard quality/safety/comparability gates before
  the next matrix rung or any efficiency/Pareto claim;
- transfer/replication evidence and explicit human or otherwise authorized
  adoption before changing the active Harness; and
- a new accepted Harness identity that re-enters the same common evidence loop.

Level-2, Meta, and later recursion may be considered only after those gates
produce evidence sufficient for an explicit decision. A better score alone
does not grant merge, deploy, release, evaluator, or adoption authority.
