# Project Roadmap

Last updated: 2026-08-31.

This document owns the high-level milestones, strategic directions, and research horizons for the Token-Efficient Agent Harness Lab.

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
| **M8: Autonomous Steward Closure** | Autonomous control loop, GitHub-authenticated non-replayable approval, single merge owner workflow delegation, and fault matrix | Complete — sealed by production Mission MISSION-9E042A35652D3D4A with two autonomously merged Stages, canonical guarded merges, and authoritative accepted-main readback on 2026-08-31 (PRs #667 and #674; final accepted main `a9406abca1afad4e7217c59a928b19dccbc7aa4d`) |

## Research Horizons

1. **Real Workload Evidence (RWE v2)**:
   - Provider-free operator corpus evaluation.
   - Deterministic economic protocol and total lifecycle cost estimation.
2. **Context Working Set (CWS)**:
   - Token efficiency via structured state projection and selective rehydration.
   - Multi-turn compression without loss of reasoning or verification fidelity.
3. **Harness Evolution**:
   - Level-1 laboratory for mutation generation and holdout evaluation.
   - Pareto-optimal candidate selection under fixed evaluator constraints.
