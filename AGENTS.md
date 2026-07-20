# Agent Instructions

This repository is the Token-Efficient Agent Harness Lab: a local deterministic Harness and self-hosted workflow control plane for studying token-efficient, auditable Agent systems.

## Current State

Rust `engine/` is the sole runtime, API, scheduler, policy, and application-owned storage implementation. The dispatch kernel, V2 real output, Adaptive Fusion through AF-7, Agent Runtime through AR-6, Trusted Local Autonomous Execution through IAE-3, durable memory, the managed external-runtime path, and PE-1 through PE-6 implementation are present.

The production-integration program and provider/target-output safety repairs are merged. Controlled staging drills and disposable target-repository acceptance passed. Two external acceptance paths remain incomplete:

- the GitHub Issues/Actions -> Vader repository-maintenance orchestrator still needs the named runner restored and one replacement smoke through PR creation, exact-head CI, and independent review;
- provider-backed embedding and benchmarking remain fail-closed until current catalog evidence establishes the exact admitted model identity and every modeled applicable price dimension.

A new forward lane is open:

- `PE7-BOUNDED-RECURSIVE-EXECUTION-1` — merged via PR #239: the AR7 runtime-extension slice for bounded persistent recursive task trees using the existing Agent Runtime and scheduler, default-off behind its feature gate and independent kill switch;
- `PE7-HARNESS-EVOLUTION-LAB-1` — documented but not implemented: default-off, fixture/local, evidence-gated candidate Harness evolution;
- `PE7-META-IMPROVER-EXPERIMENT-1` — later second-order `Improvement@K` experiment, blocked until a stable Level-1 result exists.

Do not claim that Harness evolution, recursive self-improvement, or an evolution gate is implemented until the corresponding packet is merged with verified evidence. Later work is governed by `docs/NEXT_DECISION.md`.

Post-R7 wire/type governance hardening implemented: `scripts/check_wire_codegen_drift.sh`.

## Autonomous Operating Model

The coding agent may act as planner, implementer, reviewer, and maintainer for repository-scoped work. It may inspect current code, resolve bounded design gaps, update authoritative contracts, implement code, add tests, create branches and PRs, repair CI, merge eligible changes, and continue to the next approved slice.

Execution-ready packets in `docs/NEXT_DECISION.md` are the default work units. They preserve sequence, scope, authority, acceptance, compatibility, rollback, and evidence; they do not prohibit agent judgment. When repository evidence is sufficient, choose the smallest compatible, testable, observable, and rollbackable design rather than stopping for an external planner.

Material architecture, authority, schema, migration, security, evaluator-integrity, release, or recovery decisions must be recorded in an existing authoritative document before or with implementation. Do not silently create parallel runtimes, schedulers, queues, stores, policy owners, evaluator authorities, target-output owners, or rollback systems.

## GPT Web Repository-Agent Entry

A user working in GPT Web should not need to remember workflow names, Issue numbers, dispatch IDs, PR numbers, head SHAs, CI run IDs, or retry parameters. When the repository-agent path is operational, a normal-language request is sufficient.

The GPT Web assistant then owns the internal translation:

1. refresh actual `main`, open PRs, CI, Issue #208, runner readiness, and relevant active documents;
2. create one bounded Agent Task Issue with measurable acceptance and an exact `agent-orchestrator-scope:v1` allowed-path list;
3. keep auto-merge disabled unless the user explicitly authorizes it;
4. enable only the authority required for the bounded task;
5. observe worker, artifact finalizer, branch/PR binding, exact-head CI, repair/review state, and terminal labels;
6. independently inspect the final diff and evidence;
7. merge only when the standing authority, repository classifier, exact-head CI, independent review, and rollback requirements permit it;
8. restore emergency stop immediately on scope drift, secret exposure, contradictory state, duplicate dispatch, stale binding, unexpected mutation, or a worker that fails to reach a bounded terminal state.

Current state: PR #237 repaired the CI cancellation/capacity-leak and CI-observation race. The uniquely named Vader runner is currently offline on its GitHub token-exchange TLS path; restoring the existing runner service and egress is operational repair work, not a permission blocker (see Standing Operational Authorization). Issue #208 remains emergency-stopped until a bounded replacement smoke begins; the agent may temporarily replace the emergency stop with the normal enabled control for one bounded smoke or approved repository task and restore it immediately after. A replacement documentation-only smoke is the remaining acceptance step before normal use resumes.

## Standing Operational Authorization

Normal reversible, repository-scoped, evidence-gated work is pre-authorized. No additional confirmation is required to inspect, start, stop, restart, or repair the existing Vader runner service; repair its existing Mihomo/Clash egress route or switch to another already-configured working route; use already-configured authenticated GitHub, Codex, Actions, runner, and local-service interfaces; inspect logs; run bounded diagnostics; create or update Issues, branches, commits, PRs, workflow runs, reviews, and audit evidence; repair CI; continue `READY_FOR_EXECUTION` packets; or refresh `main` and continue after an eligible merge.

The agent may temporarily replace Issue #208 `agent-emergency-stop` with the normal enabled control for one bounded smoke or approved repository task, and must restore the emergency stop immediately after terminal review or unexpected behavior. Eligible PRs may be manually squash-merged without asking again when exact-head required CI and independent review pass, no unresolved objection exists, and the repository merge classifier permits it. Auto-merge remains disabled by default.

An offline runner, stopped service, failed process, broken existing proxy route, expired local session, bounded CI failure, failed first attempt, or repairable documentation conflict is prerequisite-repair work, not a permission blocker. Agents must attempt bounded recovery and evidence collection before reporting `BLOCKED`; they must not invent permission gates absent from the true hard-stop list.

Confirmation remains required only for irreversible external destruction, repository or durable production-data deletion, public release or production deployment, a new paid provider POST without an approved identity/pricing/budget/receipt contract, credential creation/rotation/disclosure/copying of secret values, protected-branch force push, or an external effect that cannot be bounded, compensated, or rolled back through existing owners.

## Repository-Agent Repair Contract

Continue this path under `PR3-EXTERNAL-RUNTIME-LIVE-SEAL-1`; PR #219 is a closed historical parking PR and is not an implementation branch. First restore the uniquely named Vader runner to registered, online, and idle state and pass the repository-owned readiness checker without weakening TLS or control gates.

Preserve the useful security split during the first replacement smoke: Vader remains artifact-only and receives no repository write credential; the GitHub-hosted finalizer owns branch push and PR creation; exact allowed paths, base/head binding, secret suppression, emergency stop, and auto-merge-off remain mandatory.

The replacement smoke must change exactly one disposable Markdown path and prove one bounded execution identity through intake, controller, worker, artifact, finalizer, PR, exact-head CI, independent review, and terminal capacity release. Any failure must retain the exact workflow run/attempt, failed job, failed phase, fixed reason code, and release result. Do not infer a cause from labels alone.

Do not pre-emptively rewrite the orchestrator merely because it has multiple workflows. If the restored current chain completes the smoke, retain it and limit later work to evidence-backed cleanup. If the smoke fails because of cross-workflow handoff, duplicate dispatch, contradictory state, or missing terminal ownership, open one focused architecture-repair PR that may collapse intake/controller/worker coordination into one top-level workflow with ordered jobs and one run identity. Such simplification must preserve the artifact-only Vader boundary, GitHub-hosted finalization, fail-closed controls, exact-head evidence, manual merge default, and rollback.

Before the replacement smoke is accepted, use this path only for that bounded acceptance work. After acceptance, normal bounded repository tasks may use it under the same scope, identity, CI, review, audit, compensation, rollback, and emergency-stop controls.

## Recursive and Evolution Boundary

The recursive/evolution lane must obey these additional rules:

- Recursive tasks are ordinary bounded workflow nodes under the existing scheduler, not a recursive function loop or second Agent runtime.
- A model may propose a child task; the control plane derives root, parent, depth, ancestry, remaining budgets, scope, and authority.
- Child capabilities may only remain equal or become narrower; capability or repository-scope escalation fails closed.
- Whole-tree depth, child count, node count, call, token, cost, time, concurrency, retry, and lease limits are mandatory.
- Ancestor cycles, duplicate objectives under the versioned deterministic lexical-equivalence contract (declared normalization and synonym vocabulary only; no provider-grade semantic equivalence is claimed), stale parents, changed active versions, and receipt conflicts must remain explicit failures.
- Evolution candidates run only in isolated app-owned workspaces/worktrees and may become `PR_READY` only.
- The evaluator, sealed set, permissions, credentials, budgets, audit, active-version binding, promotion thresholds, target-output, merge, release, deployment, and rollback owners are immutable to the candidate and evolver.
- Initial evolution work is deterministic fixture/local-only. It may not enable Issue #208, use the unavailable Vader runner, call a provider, mutate a real target repository, or infer live evidence from fixtures.
- No result may be called recursive self-improvement unless an independently evaluated meta-improver shows statistically supported improvement in `Improvement@K` on unseen improvement tasks.

## Model Selection

Model and reasoning-effort selection are user/tool settings, not repository policy. Do not change model configuration merely to satisfy repository instructions. Model choice never weakens testing, review, CI, audit, cost, evaluator-integrity, compatibility, compensation, or rollback requirements.

## Execution-Ready Task Packets

Packet states are:

- `READY_FOR_EXECUTION` — prerequisites and contract are sufficient to begin.
- `BLOCKED_PREREQUISITE` — an earlier dependency or external condition must complete first.
- `DECISION_REQUIRED` — a material decision cannot be derived safely.
- `IN_PROGRESS` — one branch or PR owns the packet.
- `COMPLETE` — implementation is merged, required evidence is verified, and active documents are synchronized.

Every packet should state:

- packet ID and state;
- goal and observable result;
- prerequisites;
- owning paths and existing owners to reuse;
- allowed changes, forbidden changes, and non-goals;
- versioned input/output, schema, identity, budget, and failure contracts;
- focused and full verification;
- compatibility, migration, and rollback requirements;
- completion evidence;
- stop triggers.

Prefer the earliest eligible packet in the normative sequence. An explicitly independent lane may proceed only when its prerequisites and isolation are documented. Do not begin later product behavior before its predecessor is accepted.

## Full Agent Autonomy Mode

Full Agent Autonomy Mode is active for repository-scoped, testable, observable, verification-gated, and rollbackable work. It permits:

- execute and close ready packets;
- audit current code before assuming a capability is absent;
- resolve bounded missing decisions from repository evidence;
- update existing authoritative architecture, authority, schema, migration, security, evaluator, release, or recovery contracts;
- implement cross-module code, migrations, APIs, SDKs, Dashboard changes, policy adapters, release tooling, and recovery tests when boundaries are explicit;
- repair deterministic tests, CI, lint, security-baseline, action-pin, handoff, or wire-codegen failures at the root cause;
- create branches, commits, PRs, reviews, and eligible green merges;
- continue across packets after refreshing `main`;
- perform narrow maintenance and remove stale or misleading documentation.

Autonomy does not authorize inventing evidence, weakening fail-closed behavior, bypassing existing authority, silently changing external state, modifying sealed evaluation data, or creating parallel owners.

## Minimal Reading Model

Use progressive disclosure rather than reading every active document in full.

Always read `AGENTS.md`. Then read only these sections before selecting work:

1. `docs/CURRENT_STATUS.md`: `Summary`, `Verified Repository State`, `Confirmed Integration Gaps`, and `Open Work Coordination`;
2. `docs/NEXT_DECISION.md`: `Current Direction`, `Active Routing`, `Common Execution Protocol`, `Hard Stops`, and the one selected packet; do not read unrelated packet bodies unless scope or prerequisites overlap;
3. `docs/MODULE_MAP.md`: `Core Ownership`, the capability row relevant to the request, and the selected packet's approved ownership section.

Read additional surfaces only when triggered:

- `docs/REAL_WORLD_TESTING_PLAYBOOK.md` for branch, PR, CI, review, and merge work;
- the relevant section of `docs/ARCHITECTURE_BOOK.md` for architecture, storage, authority, security, evaluator, release, or recovery changes;
- the relevant procedure in `docs/RUNBOOK.md` only for an actual operator action;
- `README.md` and `CLAUDE.md` only when changing entrypoints, installation, public usage, or compatibility instructions.

Use targeted search and line-range reads. Do not load an entire large document merely because it is active. Expand reading only when the selected section references a dependency, conflict, or authority boundary that can change the conclusion.

Use current code, merged history, tests, and CI as authoritative evidence. Reconcile stale documentation explicitly rather than silently following it.

## Hard Stops

Stop rather than work around any of these:

- do not commit real secrets, raw prompts/outputs/transcripts, private paths, or unredacted sensitive payloads;
- do not falsify test or CI evidence; benchmark, cost, evaluator, lineage, and implementation evidence are subject to the same rule;
- do not intentionally hide failures, rejected candidates, outcome-unknown effects, or safety regressions;
- do not remove rollback paths without a tested replacement; recovery paths are subject to the same rule;
- do not perform irreversible external destruction without a recovery path and explicit authority;
- do not bypass required approval for an action that remains on the confirmation list, create or disclose credentials, or cross a sealed evaluation boundary; already-configured credentials and services may be used and repaired normally;
- do not modify or expose evaluator source, sealed labels, promotion thresholds, permissions, budgets, or audit to a candidate/evolver;
- do not merge code, tests, scripts, workflows, configuration, schemas, migrations, generated artifacts, dependencies, runtime behavior, authority, evaluator, release, or external-action changes while required CI is failed, queued, in progress, cancelled, action-required, or unexpectedly skipped;
- do not overwrite another agent's in-progress work without reconciling ownership;
- stop when materially contradictory requirements cannot be resolved from repository evidence;
- stop any recursive/evolution run whose complete bounded terminal state or external-effect status cannot be proven.

A difficult implementation, failed first attempt, or bounded missing design detail is not by itself a hard stop. Diagnose, revise, and continue while each repair remains evidence-driven and inside the documented boundaries.

## Documentation-Only Direct-Main Rule

Standing user authorization permits a commit directly to `main` when the final diff is limited to Markdown or plain-text documentation and changes no code, tests, scripts, workflows, configuration, schema, migration, generated artifact, dependency, executable, runtime behavior, release artifact, credential, tag, deployment, provider call, target-repository write, or other external state.

A qualifying direct-main documentation change still requires final diff review, `git diff --check`, `uv run --no-project python scripts/check_agent_handoff.py`, any applicable documentation/link check, a clear revert rollback, and no fabricated implementation or CI claim. If the scope is mixed, executable, generated, security-sensitive outside prose, or uncertain, use a branch, PR, and complete required CI.

## Autonomous Advancement Loop

For every autonomous session:

1. inspect latest `main`, open PRs, recent merges, CI, and external control state;
2. read active documents and select the highest-value eligible packet or prerequisite repair;
3. audit existing code and tests;
4. restate scope, non-goals, owners, risk, acceptance, rollback, and hard stops;
5. resolve bounded decisions from evidence and record material ones;
6. add or update focused tests before behavior changes where practical;
7. implement one coherent reviewable slice;
8. run focused checks and applicable full verification;
9. review architecture, authority, evaluator integrity, compatibility, security, audit, cost, and rollback;
10. repair failures at the root cause without weakening guards;
11. update only the smallest necessary active documents;
12. run `uv run --no-project python scripts/check_agent_handoff.py`;
13. commit in English; documentation-only direct-main changes require explicit user authority, all other changes require a branch and PR;
14. merge only when the playbook permits it and no unresolved human objection exists;
15. refresh `main`, update packet states, and continue only within the authorized objective;
16. report exact packet/slice, files, decisions, tests, CI or docs-only checks, compatibility, residual risk, rollback, external effects, and next state.

## Verification Baseline

Use focused checks plus applicable repository checks:

```bash
cargo fmt --all -- --check
cargo clippy -p engine --all-targets --all-features -- -D warnings
cargo test -p engine
cargo test -p engine --features pg-tests -- --test-threads=1
PYTHONPATH=src uv run --no-project python -m unittest discover -s tests
bash scripts/verify_rust_typescript_stack.sh
bash scripts/check_wire_codegen_drift.sh
uv run --no-project python tools/check_security_baseline.py
uv run --no-project python scripts/check_agent_handoff.py
git diff --check
```

Add migration, evaluator-integrity, benchmark, browser, Docker, release, backup/restore, concurrency, compensation, or fault-specific checks when the change touches those surfaces. Strictly documentation-only changes use the targeted checks required by the direct-main/merge exception.

## Documentation Maintenance Rule

Keep the documentation set small. Authoritative surfaces are:

- `docs/ARCHITECTURE_BOOK.md` — implemented architecture, data ownership, and durable boundaries;
- `docs/CURRENT_STATUS.md` — current facts, limitations, and implementation status;
- `docs/NEXT_DECISION.md` — single forward plan and packet definitions;
- `docs/MODULE_MAP.md` — source/test ownership and approved connection points;
- `docs/REAL_WORLD_TESTING_PLAYBOOK.md` — branch, PR, CI, evidence, and merge discipline;
- `docs/RUNBOOK.md` — proven operator procedures only;
- `README.md`, `CLAUDE.md`, `AGENTS.md` — entrypoints and agent boundaries.

Prefer editing, shortening, or deleting stale text over adding another roadmap, policy, status, packet, or closeout document. Planned behavior belongs in `NEXT_DECISION`; do not place unproven PE7 commands in the runbook or describe them as current architecture.
