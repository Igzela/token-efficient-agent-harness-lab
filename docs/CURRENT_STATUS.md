# Current Status

Last updated: 2026-07-21.

## Verified Repository State

- Canonical repository: `Igzela/token-efficient-agent-harness-lab`.
- Audited remote `main`: `178d020e91d359a71a6bc272a12bfeebdc9dc505` (squash merge of PR #268 G1 golden path).
- Prior baseline: PR #267 docs audit at `641679c1`; Level-1 acceptance PR #265 at `6b4091e8`.
- Open PR coordination: PR #225 remains presentation-only Dashboard work. Golden Path G2–G4 follow-up is on `codex/pe7-product-golden-path-g2-g4`.
- Open research coordination: Issue #266 contains only the initial Level-2 multi-generation proposal. No matching branch, commit, PR, or issue comment was found during this audit.
- Parked external acceptance: Issue #254 remains the repository-agent replacement smoke parking issue. Issue #208 remains the emergency-stop authority.

Repository evidence, CI, and current source remain authoritative. Prior chat summaries and stale document text are not implementation evidence.

## Current Product Verdict

The repository contains a substantial set of implemented and tested control-plane owners, but it does not yet expose one canonical user-task transaction that binds natural-language intent, target repository, source revision, executable graph, worktree, verification, artifact, approval, and output.

The ordinary product-facing path is therefore not yet a usable end-to-end coding-agent workflow:

1. `/dispatch` is a deterministic dispatch/evaluation ledger path whose default executor is `noop`.
2. `/plans` produces a read-only advisory graph unless the caller explicitly supplies `agent_steps` or `adaptive_execution`.
3. `/workflow-runs` creates and advances persisted runs, but normal plan nodes do not carry the task prompt, target repository, verification contract, or worktree binding needed by a coding executor.
4. supervised-patch workspaces, verification, patch capture, approval, and target output are real owners, but they are created through separate calls after the run exists.
5. Mission Control places these controls on one screen, but its “Create plan + run” action does not bind the repository fields shown beside it and requires manual workspace creation, tick, verification, capture, approval, and output.

This is an integration gap, not proof that the individual owners are fake.

## Capability Status

| Capability | State | Current truth |
|---|---|---|
| Rust API, scheduler, workflow store | implemented / active | `engine/` is the sole runtime, scheduler, policy, audit, and application-owned storage authority. |
| Plain `/dispatch` | implemented / default-noop | TaskAnalyzer, ModelSelector, BudgetManager, executor, evaluator, ledger, persisted routing decision, and registered offline-replay producer participate. A real provider is not called by default. |
| Read-only planner and DAG | implemented / advisory by default | Task decomposition, dependency validation, DAG metadata, context budget, and routing advice exist. Generic nodes contain task types and dependencies, not an executable coding-task contract. |
| Scheduler and executor pool | implemented / default-off | When explicitly enabled, supervised workers enumerate active runs, lease nodes, select an executor route, and continue ticking. Default scheduler/worker gates are off and default executor is `noop`. |
| Agent Runtime | implemented / explicit / default-off | Caller must construct `agent_steps`; provider/runtime/auth/pricing/cost gates must pass. One leased node produces one typed action. |
| Bounded recursive task execution | implemented / default-off | PR #239 adds bounded persisted child-task admission through existing run/scheduler owners. It does not create autonomous root-goal authority or recursive self-improvement. |
| Durable memory | implemented / scoped | Versioned store and scheduler context injection exist for compatible Agent Runtime paths; ordinary read-only plans do not opt into memory automatically. Provider embeddings remain guarded and are not live acceptance evidence. |
| Adaptive Fusion | implemented / explicit or trusted-local default-routing | Guarded completion, candidate selection, observation, experiment, promotion, replay, and rollback owners exist. Ordinary plan creation does not automatically compile these into an executable graph. |
| Dynamic workflow control | implemented / explicit | Scheduler/controller logic can mutate and recover bounded runs, but ordinary plans are not automatically converted into an adaptive coding workflow. |
| Supervised patch workspaces | implemented / separate API | Copy and controlled git-worktree creation, verification/repair, capture, scan/redaction, integrity, approval binding, patch export, `acp/*` branch push, and optional Draft PR creation exist. |
| Target repository output | implemented / default-off | Requires controlled git worktree, verification evidence, passed secret scan, redaction, current approval binding, integrity, output gate, and confirmation. Target `main` and merge authority remain protected. |
| OpenCode adapter | fixture-only / default-off | The fixture adapter and honesty repair are merged. No real OpenCode binary is admitted by the current `PIN.json`. |
| Harness Evolution Level-1 | implemented laboratory / default-off / fixture | Store-owned active identity, real app-owned candidate workspace, evaluator-owned sealed fixture tasks, Pareto archive, operator acknowledgement, and PR_READY receipt path are accepted through PR #265. It makes no production improvement claim and does not mutate active Harness. |
| Harness Evolution Level-2 | planning only | Issue #266 only. No implementation branch, commit, PR, or accepted active-lane status. |
| Meta Improver | blocked experiment | No production recursive self-improvement, provider evolution, evaluator co-evolution, active-policy mutation, or continuous self-update is authorized. |
| Repository-agent orchestration | implemented / production-disabled / parked | GitHub/Vader maintenance owners exist, but the replacement live smoke is parked on Issue #254 and must not be enabled by product work. |
| Dashboard | active / manual composition | Read-mostly operations plus guarded controls. PR #225 remains an independent presentation-only lane. |
| SDKs and wire contracts | active | Typed API clients and codegen/drift checks mirror existing endpoints; they do not yet provide a one-call canonical task intake. |
| PE-5 Release Provenance | implemented / no release authority | Provenance and verification owners exist; no release, tag, publication, deployment, or installation is authorized by this audit. |
| PE-6 Fault Injection and Recovery Drills | implemented / disposable only | Fixed, bounded recovery drills cover existing owners; they do not authorize production faults or destructive operations. |
| Post-R7 wire/type governance | implemented | `scripts/check_wire_codegen_drift.sh` remains the required cross-language drift guard. |

## Top-Level Directory Truth

| Path | Role | Active? | In the ordinary user task path? |
|---|---|---:|---:|
| `engine/` | Canonical Rust API, dispatch, planning, workflow, scheduler, execution, store, evidence, and output owners | yes | partially; owners exist but are not composed into one transaction |
| `dashboard/` | Mission Control, evidence views, and guarded operator actions | yes | manual composition only |
| `sdk/` | TypeScript and Python clients | yes | mirrors fragmented APIs; no canonical intake |
| `scripts/` | validation, pilots, scorecards, importers, release/ops, repository-agent control | yes | mostly operator/test paths, not the core runtime |
| `tools/` | security, fault, packaging, provenance, and maintenance checks | yes | verification/support only |
| `adapters/` | bounded external-runtime adapters, including OpenCode fixture | yes, guarded | only explicit fixture/external nodes |
| `wire_contract/`, `codegen/` | cross-language schemas and generated types | yes | contract support |
| `.github/` | CI, exact-head checks, release workflows, and default-off repository-agent workflows | yes | delivery/verification, not product runtime |
| `docs/` | operating policy, architecture, runbook, and active packet state | yes | governance, not execution |
| `tests/`, `engine/tests/`, fixtures, benchmarks | deterministic and integration evidence | yes | validation only; fixture success is not live usability |
| `deploy/` | local/deployment packaging support | bounded | not a production deployment authority |
| `site/` | public presentation | bounded | no runtime participation |
| release/provenance assets | build and trust evidence | bounded | release-time only; no active release authority |

## Confirmed Integration Gaps

1. No canonical root task identity spans dispatch, plan, run, supervised workspace, artifact, approval, output receipt, replay, and scorecard.
2. No single intake binds `raw_request`, target repository, source revision, objective, allowed paths, verification commands, output intent, budget, and execution confirmation.
3. Generic TaskDecomposer graphs are advisory/task-type graphs, not executable coding graphs. The prompt is stored at plan level but not carried into generic node execution metadata.
4. A git worktree is created after the workflow run and is not automatically bound to each executable node before lease.
5. Manual tick selects an executor from the request; scheduler selection works only after explicit startup gates and suitable node metadata. The default path remains `noop`.
6. Adaptive Fusion, durable memory, recursive execution, dynamic workflow, Agent Runtime, and external runtimes are composable owners, not automatically selected product capabilities.
7. supervised-patch verification/repair uses an API-owned run boundary and is not one atomic transaction with the ordinary plan/run.
8. Dispatch replay and adaptive observations can feed routing owners, and Level-1 can consume owner-backed fixture evidence, but there is no proven real-workload loop from a successful natural-language repository task into later Harness candidate generation.
9. Mission Control's “Task, run, workspace, patch, approval, and output controls in one path” is a UI sequence, not a backend orchestrator.
10. The first usability blocker is orchestration and identity binding. Real provider/OpenCode admission affects live execution quality, but enabling it before the task transaction is connected would expose the same fragmented path with more external risk.

## Active Tracks

- `PE7-PRODUCT-GOLDEN-PATH-1`: `IN_PROGRESS` — G1 merged (PR #268). G2 graph/scheduler, G3 verify/artifact/approval, SDK/Dashboard surfaces are on the follow-up branch; full packet acceptance (including Draft PR E2E when gated) is not sealed.
- `PE7-REAL-WORKLOAD-EVIDENCE-1`: blocked until the Golden Path emits trustworthy end-to-end evidence.
- `PE7-HARNESS-EVOLUTION-LEVEL2-GENERATIONAL-CONTROLLER-1`: Issue #266 remains open but blocked on real-workload evidence and an explicit activation review.
- `PE7-META-IMPROVER-EXPERIMENT-1`: blocked on a stable, independently reviewed Level-2 result and separate authority decision.
- `PE7-OPENCODE-BINARY-ADMISSION-1`: deferred and not a Golden Path prerequisite; fixture or an already-supported managed executor is sufficient for initial acceptance.
- `PR3-EXTERNAL-RUNTIME-LIVE-SEAL-1`: parked on Issue #254.
- PR #225: independent presentation-only Dashboard work.

## Open Work Coordination

PR #268 (G1) is merged on `main`. Follow-up G2–G4 work is on `codex/pe7-product-golden-path-g2-g4`. PR #225 remains independent presentation-only work; do not modify its theme files. Issue #266 remains Level-2 proposal only.

## Safety Boundary

No provider, Vader, Issue #208, real OpenCode binary, target-repository write, release, auto-merge, or deployment was enabled or exercised by this audit. The repository does not currently claim to be autonomous, production-recursive, recursively self-improving, or generally usable for an unassisted natural-language coding task.
