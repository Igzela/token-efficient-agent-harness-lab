# Next Decision

Last updated: 2026-07-20.

## Current Direction

The bounded production-integration program is merged. Rust `engine/` remains the sole runtime, API, scheduler, policy, audit, and application-owned storage authority. Disposable staging drills and the repaired target-repository output path passed. External live acceptance is still incomplete:

- the GitHub/Vader repository-maintenance path needs one final bounded replacement smoke after authorized restoration of the named runner to online/idle (empty-workspace defect fixed, dispatch-trigger gap repaired by PR #233, CI-observation and cancellation/capacity-leak repaired by PR #237);
- provider-backed embedding and the native/LangGraph benchmark remain fail-closed because the current OpenRouter catalog proves the configured embedding identity but omits the potentially chargeable embedding `request` price;
- no provider POST, public release, production installation, or protected-branch write is authorized;
- deletion of the disposable no-value target repository still requires interactive GitHub sudo-mode/2FA.

A separate research lane is now approved for **bounded recursive execution and evidence-gated Harness evolution**. This lane is not an online self-update mechanism and does not establish recursive self-improvement. It must remain default-off, fixture/local-first, isolated from the active Harness, and subordinate to the existing budget, tool-policy, target-output, review, promotion, and rollback owners.

A controlled OpenCode integration is planned between bounded recursive execution and Harness evolution. OpenCode may be introduced only as a pinned, default-off external coding executor under the existing Rust scheduler and finalizer. It may not become a second runtime, scheduler, permission owner, session store, provider router, evaluator, promotion owner, or release authority.

Do not create another roadmap, phase, status, policy, or closeout document. This file is the single forward plan. Current facts belong in `docs/CURRENT_STATUS.md`; durable architecture belongs in `docs/ARCHITECTURE_BOOK.md`; ownership belongs in `docs/MODULE_MAP.md`; only proven operator procedures belong in `docs/RUNBOOK.md`.

## Active Routing

1. `OSS-CONTRIBUTOR-SURFACE-1` — `IN_PROGRESS` / `READY_FOR_EXECUTION` (bounded maintenance). Contributor and community surface from the open-source growth audit: issue forms, layered PR template, SUPPORT response policy, CHANGELOG, CITATION, CODEOWNERS, and eight `good first issue` seeds. No Dependabot enablement without a separate cost review. No second roadmap file. Prerequisite: `OSS-PUBLIC-TRUST-1` merged (PR #241).
2. `OSS-PUBLIC-TRUST-1` — `COMPLETE` via PR #241.
3. `PR3-EXTERNAL-RUNTIME-LIVE-SEAL-1` — `READY_FOR_EXECUTION`. Restore the existing runner, pass readiness, run the replacement smoke. Independent operational lane.
4. `PE7-OPENCODE-EXTERNAL-ADAPTER-1` — `READY_FOR_EXECUTION`. Lower priority than public/contributor readiness for external adoption.
5. Later open-source slices: no-provider five-minute demo; monorepo `actions/exact-head-check/` wedge.
6. `PE7-HARNESS-EVOLUTION-LAB-1` — `BLOCKED_PREREQUISITE` on both bounded recursive execution and the controlled OpenCode adapter.
7. `PE7-META-IMPROVER-EXPERIMENT-1` — `BLOCKED_PREREQUISITE` on a stable PE7 Level-1 result.
8. PR #225 remains an independent presentation-only Dashboard PR.

The recursive/evolution lane is independent of the external-acceptance lane only while it remains deterministic, local/fixture-only, and free of external mutations. PR3 acceptance work and PE7 local/fixture work may proceed without repeated permission, using separate branches/sessions when they run concurrently. Any provider call, networked OpenCode tool, repository mutation outside isolated candidate workspaces, protected-branch write, public release, deployment, or destructive operation remains subject to its existing owner and hard-stop contract.

## Verified Baseline

The following merged work is the implementation baseline:

- PR #214 repaired PE-5 release provenance and PE-6 fault/recovery evidence;
- PR #207 and PR #216 delivered and repaired the disabled-by-default GitHub/Vader repository-maintenance orchestrator;
- PR #220 connected production Agent Runtime and tool-policy routing;
- PR #221 connected durable memory, budget evidence production, and trace-backed replay/promotion;
- PR #222 connected the managed external runtime, canonical efficiency benchmark, orchestrator evidence repair, and local acceptance seal;
- PR #223 and PR #224 repaired provider embedding receipts, transport, authorization, identity, and pricing safety;
- PR #226 repaired target-output duplicate delivery and restart idempotency;
- PR #227 added endpoint-specific Applicable / NotApplicable / Unknown embedding-pricing evidence while preserving historical receipt reads; its authenticated GET-only evidence still rejects the current row because `request` pricing is absent;
- PR #230 repaired the empty-workspace defect and split terminal attribution in the repository-agent orchestrator;
- PR #233 repaired the dispatch-trigger gap in `agent-ci-monitor` (added `workflow_dispatch` trigger with explicit `issue`/`pr`/`head_sha`/`ci_run_id` inputs and a worker finalizer step that dispatches it after CI is green);
- PR #237 repaired the repository-agent CI cancellation/capacity-leak and CI-observation race (reverted all seven orchestrator workflows to per-resource concurrency groups with `cancel-in-progress: false` so emergency-stop no longer cancels unrelated workflows; split `ci_verifier` into `acquire_exact_run` + `wait_for_run_completion` with per-cycle binding revalidation; made production CI identity fail-closed on every missing field; added durable `claimed` → `dispatched` claim lifecycle records written before label mutation; added `reconcile_claimed_dispatch` plus idempotent `release_and_record_ci_terminal` terminal compensation preserving `dispatched` claims for their own child-workflow compensation; squash-merged into `main` at `1947d4b555bd14b7f104c1fc9aba31747099cb88` after all seven CI jobs passed on exact head `068b2e9ac4bde16daea25bcb4846f7e26ba6cca9`, run `29628449688`; two independent complete-diff reviews passed);
- staging recovery drills and disposable target-repository acceptance passed without moving target `main`;
- the current repository-maintenance orchestrator remains emergency-stopped and no provider-backed benchmark is verified.

Every implementation session must refresh actual `main`, open PRs, CI, Issue #208, runner readiness, provider catalog evidence, and overlapping file ownership before relying on these facts.

## Packet States

- `READY_FOR_EXECUTION` — prerequisites and contract are sufficient to begin.
- `BLOCKED_PREREQUISITE` — defined but waiting for an earlier packet or external condition.
- `DECISION_REQUIRED` — a material authority or product decision cannot be derived safely.
- `IN_PROGRESS` — one branch or PR owns the packet.
- `COMPLETE` — implementation is merged, required evidence is verified, and active documents are synchronized.

## Common Execution Protocol

Every implementation packet must:

- start from the latest actual `main` or the exact owned PR head;
- use one focused branch and PR;
- preserve the existing Rust runtime, scheduler, `LocalProductStore`, auth, budget, audit, tool-policy, pause, target-output, release, and rollback owners;
- define versioned bounded inputs, outputs, reason codes, identity bindings, limits, and failure states;
- reject caller-supplied authority where facts must be derived from persisted owners;
- fail closed on missing, stale, conflicting, tampered, oversized, cyclic, over-budget, or incompatible evidence;
- add real call-path, restart, concurrency, idempotency, and negative tests where applicable;
- run focused checks, the applicable full local baseline, exact-head GitHub CI, and independent complete-diff review;
- keep auto-merge disabled unless separately authorized;
- update only the smallest necessary active documents;
- retain an exact rollback path and leave authoritative evidence inert rather than deleting it by default.

Strictly documentation-only factual changes may be committed directly to `main` under the existing user authorization when the final diff is documentation-only, reviewed, rollbackable, and passes the handoff and whitespace checks. Documentation cannot claim implementation or CI evidence that does not exist.

## Hard Stops

Stop and report `BLOCKED` rather than improvising when:

- a secret, raw prompt/output/transcript, private path, or unredacted sensitive payload would enter version control or an artifact;
- required tests, CI, exact-head identity, review evidence, or a known failure would be hidden or fabricated;
- an existing runtime, scheduler, storage, permission, evaluator, promotion, target-output, release, audit, compensation, or rollback owner would be bypassed or duplicated;
- a candidate can read or modify sealed test labels, evaluator source, promotion thresholds, permissions, credentials, budgets, audit history, or active-version bindings;
- an irreversible external action lacks explicit authority and tested recovery;
- another active PR owns conflicting code that cannot be safely reconciled;
- a recursive run can exceed a deterministic tree, call, token, cost, time, concurrency, retry, or lease bound;
- an OpenCode invocation can broaden scope, enable an unapproved provider or network tool, escape its workspace, change its own permission contract, or leave descendant processes unbounded;
- a worker or external effect cannot be proven terminal and late writes cannot be ruled out;
- exact-head CI is failed, queued, in progress, cancelled, action-required, or unexpectedly skipped at merge time.

## Packet PR3-EXTERNAL-RUNTIME-LIVE-SEAL-1 — Remaining external acceptance

**State:** `READY_FOR_EXECUTION`

**Goal:** Complete the already-merged external acceptance chain by restoring the named Vader runner and running one bounded replacement smoke. Provider catalog admission remains a separate fail-closed concern and is not a prerequisite for this documentation-only smoke.

**Next actions:**

1. restore the existing Vader service and egress route;
2. pass the repository-owned runner readiness checker with the runner uniquely registered, online, idle, and free of stale capacity;
3. temporarily replace Issue #208 emergency stop with the normal enabled control and run one fresh documentation-only smoke through intake, Vader, artifact, branch, PR, exact-head seven-job CI, independent review, and terminal capacity release;
4. restore the emergency stop immediately after terminal review or unexpected behavior;
5. record the exact Issue/PR/head/run/review/evidence bindings and acceptance, then continue future work.

The prior smoke history remains as recorded in `CURRENT_STATUS.md`; the current runner offline observation is a repair target, not a governance hard stop. No provider POST is active.

**Completion:** one replacement repository-agent smoke reaches PR creation, exact-head CI, and independent review; any provider-backed acceptance separately proves current identity, pricing, audit, and cost evidence.

## Packet PE7-BOUNDED-RECURSIVE-EXECUTION-1 — Recursive task-tree contract

**State:** `COMPLETE`

**Completion evidence:** PR #239 squash-merged into `main` at `d554c5630c0347e99840067a216c772d5a2377ca` on the exact reviewed head `1c78413a866f81b3bb3340f84a673886f9c1b228`. Exact-head seven-job CI passed on explicitly dispatched run `29717160879` (nonce `pr239-final-1c78413a-20260720T043250Z`) with the exact-head verification step executed in all seven jobs; the parallel `pull_request` run `29717141726` on the same head also passed. Two independent complete-diff reviews (standards and spec axes) ran on the full `main...head` diff; their material findings were repaired before merge (non-heritable capability honesty at both derivation points and admission, root late-usage terminalization parity with terminal workflow-node sync on both backends, single-sourced failure reason codes, `MAX_RECURSIVE_RETRIES` reuse instead of a restated literal, unreachable v26 migration arms removed on both backends, AGENTS.md/NEXT_DECISION duplicate-refusal wording reconciled with the implemented versioned deterministic lexical-equivalence contract). Recursive execution is implemented but remains default-off; Harness evolution, OpenCode integration, and any evolution gate remain unavailable.

**Goal:** Extend the existing Agent Runtime child-task proposal mechanism into a persistent, bounded task tree without adding a second runtime, scheduler, queue, mailbox, or storage authority. This packet is the AR7 runtime-extension slice.

**Existing owners to reuse:**

- `AgentStepExecutor`, `AgentAction::ProposeChildTask`, `ChildTaskProposal`, and `agent_proposals`;
- `WorkflowGraph`, workflow nodes/edges, scheduler leases, executor pool, and AR-4 concurrency caps;
- `agent_action_receipts` for exactly-once mutation;
- existing provider/tool-policy gates, Agent Runtime kill switch, operator evidence, audit, and pause/recovery controls;
- `LocalProductStore` SQLite/PostgreSQL transactions and integrity coverage;
- existing HTTP, SDK, and Dashboard read surfaces where bounded operator visibility is required.

**Required contract:**

Each accepted recursive node must derive and persist a versioned identity containing at least:

- root run and workflow identity;
- parent run/node/proposal identity;
- recursion depth;
- deterministic task/objective fingerprint;
- inherited-and-reduced capability profile;
- per-node and remaining tree budgets;
- ancestor fingerprints needed for cycle/duplicate detection;
- accepted/rejected/blocked reason and exact evidence references.

The control plane, not the model, derives depth, remaining budgets, scope, authority, and ancestry. A model may only submit a bounded proposal. Acceptance creates ordinary persisted workflow nodes and edges through the existing owners.

**Initial hard limits:**

- default-off feature gate and independent kill switch;
- maximum depth 2;
- maximum 3 accepted children per node;
- maximum 12 recursive nodes per root run;
- maximum 3 concurrently leased recursive nodes globally within the feature;
- maximum one retry per recursive node, subject to existing retryability and outcome-unknown rules;
- child permissions may only equal or be a strict subset of the parent permissions;
- no recursive node may create a new root goal, broaden tenant/workspace/repository scope, or authorize an external mutation.

These defaults may be changed only through a later evidence-backed packet and versioned configuration contract.

**Required failure states:**

`recursive_disabled`, `depth_exceeded`, `child_limit_exceeded`, `tree_budget_exhausted`, `duplicate_objective`, `ancestor_cycle`, `capability_escalation`, `scope_mismatch`, `stale_parent`, `proposal_conflict`, `receipt_conflict`, `scheduler_capacity_exhausted`, and `recursive_kill_switch_active` must remain distinguishable and auditable.

**Verification:**

- deterministic tree construction and ordering;
- depth, child, total-node, token, cost, time, concurrency, retry, and lease bounds;
- ancestor-cycle refusal and versioned deterministic lexical-equivalence duplicate refusal; this local/fixture contract canonicalizes only its declared normalization and synonym vocabulary and does not claim provider-grade semantic equivalence;
- capability and scope reduction;
- concurrent proposal acceptance races;
- restart and exact receipt replay;
- stale worker completion and late-write refusal;
- kill/pause/recovery behavior;
- SQLite/PostgreSQL parity and integrity coverage;
- bounded read-model output with no raw prompt/output/transcript persistence;
- full exact-head CI and independent review.

**Non-goals:**

- no persistent self-modification;
- no candidate Harness archive;
- no automatic generation of new root objectives;
- no online modification of active code, evaluator, permissions, budget policy, provider policy, target-output policy, or release policy;
- no claim of recursive self-improvement.

**Rollback:** Activate the recursive kill switch, pause new admission, drain or terminally block active recursive leases, preserve a verified backup and lineage evidence, then revert the implementation. Additive schema rows remain inert by default. Any destructive downgrade must be explicit, backend-aligned, audited, and refuse non-empty live authority.

**Completion:** One reviewed PR is merged with the bounded tree contract, production call-path ownership, SQLite/PostgreSQL tests, operator evidence, default-off gates, and exact-head CI. Active docs must state that recursive execution is implemented but Harness evolution remains unavailable.

## Packet PE7-OPENCODE-EXTERNAL-ADAPTER-1 — Controlled coding executor

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** `PE7-BOUNDED-RECURSIVE-EXECUTION-1` is merged and accepted.

**Goal:** Integrate a pinned release or exact commit of the active MIT-licensed `anomalyco/opencode` project as one default-off external coding executor. The Rust scheduler remains the sole admission, lease, retry, timeout, pause, concurrency, authority, and state owner. OpenCode receives one bounded task in one isolated worktree and may return only bounded analysis evidence or a candidate patch for existing finalizer validation.

**Architecture:**

```text
Rust scheduler and tool-policy owner
        -> one leased external-executor invocation
        -> pinned OpenCode binary in an isolated worktree
        -> deny-by-default generated permission profile
        -> bounded analysis or allowed-path patch
        -> app-owned receipt and output validation
        -> existing finalizer / evaluation path
        -> at most PR_READY
```

**Version and supply-chain contract:**

- pin an exact upstream release or commit plus artifact/source checksum; mutable `latest`, an unpinned installer, or an unreviewed automatic upgrade is forbidden;
- record the upstream repository identity, license attribution, version, build source, checksum, and local adapter version;
- upgrades are explicit compatibility packets with fixture replay and changed-permission/tool review;
- no fork becomes an authoritative parallel product runtime merely because its code is vendored or invoked.

**Initial authority and confinement:**

- default-off and local/fixture-only;
- execute as an ordinary bounded workflow node under the existing scheduler, never through a second queue or autonomous session owner;
- one app-owned disposable worktree with exact base identity and allowed-path set;
- explicit deny-by-default OpenCode permission configuration; do not use `--auto` or any equivalent implicit approval mode;
- disable provider fallback, web search, web fetch, MCP servers, remote agents, networked plugins, background agents, and OpenCode-managed repository mutations;
- no provider call, external network access, GitHub mutation, target-repository delivery, protected-branch write, merge, release, deployment, or new root objective;
- bound process-tree lifetime, descendant termination, stdout/stderr bytes, files read/written, patch size, tool calls, retries, wall time, tokens, and trustworthy cost where applicable;
- OpenCode's internal permission result never overrides the Harness allowlist, approval, budget, secret, audit, or target-output owners.

**Required invocation evidence:**

Each invocation must bind at least the root run/node/lease, adapter and OpenCode versions, binary/source checksum, generated permission-profile hash, task-input hash, base commit/worktree identity, allowed paths, environment allowlist, process start/end, timeout and termination result, exit status, bounded tool summary, changed paths, patch/output hash and size, validation result, retry identity, and final reason code. Raw prompts, model outputs, transcripts, secrets, private paths, and unrestricted repository contents must not enter durable product evidence.

**Fixture-first acceptance:**

- one deterministic read-only code-analysis fixture and one deterministic allowed-path patch fixture;
- path traversal, symlink escape, forbidden file, unexpected binary, undeclared environment, network attempt, MCP/plugin activation, permission escalation, background descendant, malformed output, oversized patch, timeout, cancellation, kill-switch, stale lease, duplicate delivery, restart, and receipt-replay tests;
- prove that finalizer validation independently rejects scope, base/head, secret-scan, test-evidence, or output-contract mismatch;
- compare the adapter against the existing fixed executor on representative fixtures under equal declared calls/time/output bounds, but make no quality or efficiency claim from adapter acceptance alone;
- full applicable local suites, exact-head seven-job CI, and independent complete-diff review.

**Non-goals:**

- no OpenCode web search, web fetch, MCP, remote agents, background-agent autonomy, internal recursive subagent tree, provider routing, or production external calls;
- no replacement of Rust runtime, scheduler, storage, auth, audit, budget, evaluator, target-output, promotion, merge, release, or rollback owners;
- no automatic use in production routing and no claim that OpenCode is superior to Codex or the existing executor;
- no direct participation in Harness evolution until this adapter packet is complete.

**Later staged capabilities:** LSP/symbol assistance, bounded internal subagents, `webfetch`, and `websearch` require separate evidence-backed packets in that order or an explicitly justified alternative. Each networked capability must add source provenance, freshness, prompt-injection defenses, permission, cost, timeout, caching, and audit contracts before activation.

**Rollback:** Disable the adapter gate, terminate and drain adapter-owned processes, discard unpromoted worktrees, preserve bounded receipts, remove the pinned binary/configuration, and revert the adapter PR. No external or production state should require compensation because initial acceptance forbids it.

**Completion:** One reviewed PR is merged with a pinned and attributed OpenCode adapter, deterministic fixture acceptance, confinement and process-tree evidence, default-off gates, no network/provider activity, exact-head CI, and active-document synchronization. The adapter may produce bounded evidence or `PR_READY` candidates only.

## Packet PE7-HARNESS-EVOLUTION-LAB-1 — Evidence-gated candidate evolution

**State:** `BLOCKED_PREREQUISITE`

**Prerequisites:** `PE7-BOUNDED-RECURSIVE-EXECUTION-1` and `PE7-OPENCODE-EXTERNAL-ADAPTER-1`.

**Goal:** Add a default-off laboratory path that proposes, evaluates, archives, and optionally promotes isolated Harness candidates while the active Harness and the complete control plane remain immutable.

**Architecture:**

```text
posted traces / scorecards / failures
        -> bounded failure miner
        -> structured mutation proposal
        -> isolated candidate worktree or app-owned workspace
        -> fixed executor baselines, including the controlled OpenCode adapter
        -> static checks and deterministic fixture evaluation
        -> equal-budget validation and sealed holdout
        -> lineage/Pareto archive
        -> explicit operator decision and PR-only output
```

**Immutable control plane:**

The candidate and evolver may not modify or supply authority for:

- auth, credentials, tool allowlists, approval requirements, kill switches, budgets, scheduler/lease ownership, or audit;
- evaluator implementation, sealed task labels, holdout membership, scoring thresholds, statistical gates, or final test results;
- target-output, branch/PR binding, review, merge, release, deployment, backup, restore, or rollback owners;
- active Harness version, production policy snapshots, provider pricing/catalog evidence, or evidence hashes.

**Initially mutable candidate surface:**

- prompts and bounded behavioral rules;
- context selection, summarization, and retrieval configuration;
- tool descriptions and deterministic tool-selection policy, not tool authority;
- retry and stop policy within existing limits;
- model-routing policy within the existing admitted provider/model set;
- recursive subtask decomposition policy within the preceding packet's limits.

Harness source-code mutation is a later sub-stage and may begin only after the component-level candidate path has stable lineage, equal-budget baselines, sealed evaluation, and rollback evidence. Model weights, evaluator code, permissions, and production release logic remain out of scope.

**Required persistent evidence:**

Each candidate must bind candidate, parent, lineage, base commit, active-version identity, exact proposal/evidence references, patch/content hash, mutable-component declaration, generator/evaluator metadata, task-split identities, seed, calls, tokens, trustworthy cost, latency, failures, retries, tool use, wall time, correctness, safety, regression, integrity, compatibility, and terminal reason. Raw prompts, outputs, transcripts, repository content, credentials, and private paths must not be persisted in product evidence.

**Evaluation protocol:**

Compare under a predeclared equal total-call or total-token budget:

- static Harness single pass;
- static Harness plus bounded reflection/retry;
- parallel or sequential best-of-N;
- prompt-only optimization;
- greedy current-best Harness mutation;
- random candidate generation with the same candidate count;
- the lineage/archive experiment;
- existing fixed executor baselines and the controlled OpenCode adapter, without allowing executor choice to change the evaluator or budget owner.

Use task-family splits, not only random instance splits. Search may use development and validation tasks. Only 1–3 preselected candidates may enter the sealed set, and sealed results may not be fed back into further mutation. Primary promotion evidence is sealed task success/non-regression; secondary evidence includes tokens per pass, cost per pass, latency, invalid tool calls, crash rate, permission incidents, old-task sentinel regression, and lineage-average performance rather than best-so-far alone.

**Initial selection model:**

- hard constraints for correctness, safety, integrity, scope, and budget;
- Pareto archive over quality, token/cost, latency, robustness, and behavior diversity;
- conservative AIDE²-style sequential promotion first: the active laboratory parent changes only when a candidate clears every hard gate and improves the declared objective under equal budget;
- DGM/GEA-style multi-lineage parent selection and cross-branch recombination remain disabled until the sequential baseline is measured and a later packet revision defines the archive policy.

**Output authority:** A candidate may become `PR_READY` only. Existing target-output or repository-orchestrator finalizers must independently validate the exact patch, scope, base/head identity, secret scan, verification evidence, and live controls. Auto-merge stays off. No candidate may write active `main`, deploy, publish, or change its own evaluator.

**Fixture-first acceptance:**

- deterministic candidate-generation fixtures;
- lineage/archive persistence and restart/idempotency tests;
- evaluator and sealed-set access-denial tests;
- equal-budget accounting and incomplete-cost refusal;
- candidate workspace confinement and target-output non-authority;
- tamper, duplicate, stale-parent, changed-active-version, and rollback tests;
- at least three deterministic seeds or equivalent repeated fixture runs;
- no claim of performance improvement until a separately authorized guarded-live experiment is completed.

**Rollback:** Disable the evolution gate and kill switch, stop candidate admission, preserve candidate and evaluation evidence, discard unpromoted workspaces, and revert code. Promoted code is rolled back only through its ordinary commit/PR/release rollback path.

**Completion:** The laboratory can produce auditable fixture candidates and evaluation bundles, but remains default-off and cannot mutate the active Harness. A later guarded-live packet is required before any Level-1 efficiency claim.

## Packet PE7-META-IMPROVER-EXPERIMENT-1 — Bounded second-order test

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** `PE7-HARNESS-EVOLUTION-LAB-1`

**Goal:** Test whether an evolved proposer/parent selector improves the rate or quality of future improvement under a fixed candidate budget.

**Primary metric:**

```text
Improvement@K = sealed-task net improvement produced within exactly K candidate proposals,
                subject to the same safety, cost, and non-regression gates.
```

Compare a fixed human-designed proposer/selector, the initial Meta Agent, the evolved Meta Agent, random parent selection, and the best fixed archive selector. Use unseen improvement tasks and repeated seeds. Merely changing meta-level code or obtaining a higher best-so-far task score is not evidence that the improvement mechanism became stronger.

**Still immutable:** evaluator implementation, sealed set, permissions, budget owner, audit, release/merge authority, and final promotion gate.

**Completion threshold:** statistically supported improvement in `Improvement@K` on unseen improvement tasks, with no safety or cost regression. Without that evidence the project must describe the result as Harness evolution, not recursive self-improvement or ignition.

## Historical Compatibility Packets

These headings remain in the active handoff surface because repository governance checks and older automation use them as stable anchors. Their implementation is merged; they are not active work.

## Packet PR207-REPAIR-1 — Repository-maintenance orchestrator baseline

**State:** `COMPLETE`

PR #207 and PR #216 established the disabled-by-default orchestrator and repaired its Codex/output/readiness compatibility. Production acceptance remains separately blocked under `PR3-EXTERNAL-RUNTIME-LIVE-SEAL-1`.

## Packet PE2-RUNTIME-PRODUCER-1 — Budget evidence production

**State:** `COMPLETE`

Owner-backed workflow/provider/scorecard usage produces bounded forecast and anomaly evidence through the existing fenced jobs and typed pause/recovery owners.

## Packet PE4-EVIDENCE-ENTRY-1 — Trace-backed replay and promotion

**State:** `COMPLETE`

Recorder-owned dispatch traces produce immutable replay evidence; promotion remains explicit, current-state-bound, permissioned, confirmed, snapshot-backed, and rollbackable.

## Packet TOOL-DISCOVERY-BENCH-1 — Static-all versus deterministic Top-K

**State:** `COMPLETE`

The canonical benchmark compares static-all and deterministic Top-K tool discovery through existing scorecard owners and grants no production tool-loading authority.

## Deferred Work

The following remain deferred unless separately activated:

- production provider-backed evolution;
- automatic multi-lineage recombination;
- model-weight training or self-training;
- evaluator or task-generator co-evolution;
- autonomous generation of new root goals;
- production continuous self-update;
- automatic merge, deploy, release, or protected-branch mutation;
- OpenCode networked tools, MCP, remote/background agents, autonomous provider routing, and production executor routing beyond the accepted adapter;
- A2A remote-agent execution;
- complexity/risk-driven Adaptive Fusion unification;
- behavior-preserving cleanup of oversized provider/integrity modules until current external acceptance is stable.

## Preserved Boundaries

- Rust `engine/` remains the sole runtime/API/storage authority.
- Existing scheduler, workflow, pause/recovery, budget, provider, tool-policy, audit, target-output, promotion, release, and rollback owners remain authoritative.
- Recursive nodes are ordinary bounded workflow work, not a new autonomous runtime.
- OpenCode, if admitted, remains a pinned external worker under Rust-owned leases, permissions, receipts, confinement, finalization, and kill controls.
- Evolution candidates execute only in isolated app-owned workspaces or controlled worktrees.
- Candidate evidence never grants execution, target-output, merge, deploy, release, or evaluator authority.
- Vader remains artifact-only; GitHub-hosted finalizers own GitHub mutations when that path is accepted and enabled.
- Real provider calls, public releases, production installation, destructive faults, and external mutations require separate explicit authorization.

## Final Reporting Contract

Every packet report must include:

- actual starting `main`, branch/PR, and exact final head;
- files and existing owners reused;
- schema, authority, and threat-model decisions;
- focused and full local tests;
- exact-head CI and every required job result;
- independent complete-diff review and repairs;
- compatibility, cost, security, evaluator-integrity, rollback, and residual risk;
- whether Issue #208 or any provider/evolution/OpenCode gate was enabled;
- whether any external call or repository mutation occurred;
- truthful terminal state: `COMPLETE`, `BLOCKED`, `MERGE_READY`, or `PR_READY`.
