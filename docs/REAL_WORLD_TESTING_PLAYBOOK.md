# Real-World Testing Playbook

Operational execution guide for real branches, commits, PRs, CI, review, rollback, and gated autonomous merge.

## Mode Summary

The project validates changes through real repository work. Full Agent Autonomy Mode permits repo-scoped planning and execution when changes are testable, observable, reviewed, verification-gated, and rollbackable. Code, runtime, configuration, schema, workflow, release, authority, and external-action changes remain full-CI-gated. Strictly documentation-only changes may use the targeted exception defined below.

### Default Ship PR path

The default daily development path is local Agent → focused branch → PR → exact-head CI → independent complete-diff review → manual squash merge. Auto-merge stays off. The GitHub Issue / Vader orchestrator is an optional unattended entry and is currently emergency-stopped until its replacement smoke is accepted; both paths meet at the same PR / exact-head CI / review / manual-merge boundary. Do not add a second Ship PR workflow or parallel merge owner.

Execution-ready packets are the default work units:

```text
Observe → Select or repair packet → Decide → Implement → Verify → Review → Document → PR/merge decision → Continue or report
```

The coding agent may resolve bounded missing decisions from current code, merged history, tests, and authoritative documents. Material architecture, authority, schema, migration, security, release, or recovery decisions must be recorded before dependent behavior is merged.

## Model Selection

The user or execution tool selects the model and reasoning effort. PRs may record the selected model for operational traceability, but model identity is not an eligibility gate and is not validated by repository CI.

The packet in `docs/NEXT_DECISION.md`, current code, merged history, and verified contracts jointly define implementation authority. A broad user goal may authorize a bounded multi-packet objective when the agent keeps each PR coherent and refreshes `main` between merges.

## Action Permission Matrix

Standing authority permits normal reversible repository work: branch and commit creation, PR creation and repair, CI repair, independent review, eligible manual merge, runner/service/egress recovery, bounded Issue #208 enable/disable for one smoke, audit evidence, and continuation across `READY_FOR_EXECUTION` packets. Agents must not ask again when the exact-head, review, scope, audit, rollback, and merge classifier requirements pass. Existing configured credentials may be used normally; creating, rotating, disclosing, or copying credential values remains confirmation-required.

| Action | Default | Gate |
|---|---|---|
| Branch creation | allowed | latest `main` or current owned PR; clean scope |
| Commit and push | allowed | intended files only; focused verification |
| PR creation | allowed | task/slice ID, scope, risk, tests, rollback |
| CI repair | allowed | evidence-backed root-cause repair; do not weaken guards |
| Docs/test/small code maintenance | allowed | accurate, bounded, reviewable |
| New endpoint or multi-module implementation | allowed | execution-ready packet or documented bounded design; focused tests |
| Auth/security/provider boundary | allowed | explicit threat/authority boundary, audit, tests, rollback |
| DB migration | allowed | schema, compatibility, rollback, SQLite and PostgreSQL tests |
| Release/signing/install changes | allowed | explicit trust contract, dry-run evidence, rollback |
| Existing mutation endpoint integration | allowed | permission, audit, idempotency, compensation, fail-closed tests |
| New architecture/authority/recovery decision | allowed | smallest compatible design, authoritative documentation, separate contract/decision PR when risk warrants |
| Reorder packet or activate independent lane | allowed with evidence | prerequisites, user objective, conflicts, and residual risk recorded |
| Irreversible external operation | stop by default | explicit authority and tested recovery required |

## Risk Classification

| Risk | Typical scope | Required evidence |
|---|---|---|
| low | docs, tests, deterministic CI fix, small isolated code | focused check, handoff guard, reviewable diff; docs-only PRs may use the targeted merge exception |
| medium | new endpoint/UI/SDK, multi-module read model, bounded behavior change | focused integration tests, full stack, compatibility, rollback |
| high | auth, provider authority, schema migration, automatic pause/promotion, release trust, recovery | explicit contract, threat/failure review, concurrency/fault tests, full CI, audit, compensation/rollback |

Risk changes the depth of evidence and review, not the allowed model.

## Auto-Merge Classifier

A PR is autonomously merge-eligible only when all are true:

| Field | Required value |
|---|---|
| `task_packet_id` or `task_slice_id` | eligible packet, prerequisite repair, or explicit bounded-maintenance ID |
| packet/slice state | represented truthfully in active docs when state changes |
| scope | matches goal, owners, decisions, allowed changes, and non-goals |
| risk | classified with matching focused evidence |
| CI | every required job completed successfully, unless the final diff qualifies for the strict documentation-only merge exception |
| handoff guard | pass |
| review | diff reviewed against architecture, authority, compatibility, security, audit, and rollback |
| rollback | clear and sufficient |
| external authority | no unapproved irreversible operation or action remaining on the confirmation list |
| human objection | none unresolved |

Green CI alone is not permission to merge a misleading, incompatible, or unreviewed change.

## Documentation-Only Merge Exception

A PR may merge before the repository's full CI matrix completes only when its final diff is strictly documentation-only.

All of the following are required:

1. Changed paths are limited to Markdown or plain-text documentation entrypoints and files, such as `README.md`, `AGENTS.md`, `CLAUDE.md`, and `docs/**/*.md`.
2. The PR changes no code, tests, scripts, GitHub Actions/workflows, configuration, schema, migration, generated artifact, executable file, dependency manifest or lockfile, release artifact, or runtime data.
3. The PR performs no tag, release, deployment, provider call, target-repository write, or other external mutation.
4. The final diff has been reviewed and `git diff --check` plus `uv run --no-project python scripts/check_agent_handoff.py` pass. Run any applicable documentation/link validator when the touched surface has one.
5. The PR has a clear rollback by revert, no unresolved human objection, and no branch-protection rule prevents merging.
6. Any factual claim about code, runtime behavior, migration success, release state, or prior CI is backed by already verified underlying repository evidence. The docs-only PR itself cannot create that evidence.

This exception is intended for factual synchronization, stale-state cleanup, clarification, pruning, typo/link repair, and documentation-governance changes. If the diff is mixed, generated, executable, security-sensitive outside prose, or uncertain, the normal complete-green-CI gate applies. If later CI reports a docs-specific failure, repair it promptly; do not reinterpret the early merge as proof of unrelated implementation correctness.

## Feedback Trace Fields

Every product or governance PR should record:

| Field | Description |
|---|---|
| `task_packet_id` or `task_slice_id` | packet, prerequisite repair, decision, or bounded-maintenance identifier |
| `task_class` | docs, tests, ci-fix, code-fix, schema, API, SDK, Dashboard, migration, security, release, recovery |
| `selected_executor` | execution environment when useful |
| `execution_model` | optional operational trace only; not a repository gate |
| `changed_files` | exact intended file list |
| `touched_risk_paths` | auth, security, provider, migration, release, authority, recovery paths |
| `decision_record` | material design decision and authoritative location, when applicable |
| `packet_contract_check` | pass/fail with deviations explained |
| `focused_tests` | commands and results |
| `ci_result` | pass/fail/queued with run ID, or docs-only exception plus targeted check results |
| `handoff_guard_result` | pass/fail |
| `repair_summary` | root causes and coherent repair attempts |
| `merge_result` | merged/blocked/pending |
| `compatibility` | existing data/API/SDK/runtime behavior preserved or intentionally versioned |
| `rollback_plan` | exact revert and cleanup procedure |
| `residual_risk` | remaining bounded risk |
| `human_override_reason` | user-directed scope, lane, or policy change when applicable |

## Stop Conditions

Stop and report evidence when any occurs:

1. a real secret would enter version control;
2. evidence would be falsified or a known failure hidden;
3. a rollback or recovery path would be removed without a tested replacement;
4. an irreversible external action lacks explicit authority and tested recovery;
5. an action requiring confirmation, missing credentials that cannot be repaired through an existing configured interface, or unavailable access after bounded recovery blocks validation;
6. another agent owns conflicting in-progress work that cannot be reconciled safely;
7. materially contradictory requirements cannot be resolved from code, merged history, tests, and authoritative documents;
8. required CI remains failed, queued, in progress, or unexpectedly skipped at merge time, unless the final diff meets every requirement of the strict documentation-only merge exception.

Do not stop merely because a packet is stale, a bounded decision is missing, or an initial implementation failed. Audit, update the contract, repair the root cause, and continue while work remains evidence-driven and rollbackable.

## Execution Checklist

An unavailable but repairable local runner, stopped service, broken existing proxy route, expired local session, or bounded first-attempt failure is a repair task. Diagnose and repair the existing path before reporting a blocker, without weakening TLS, scope, credential-redaction, exact-head, CI, review, or emergency-stop controls.

For each coherent packet or slice:

- [ ] Start from latest `main` or audit the current owned PR
- [ ] Inspect open PRs, recent merges, branch state, and CI
- [ ] Read `AGENTS.md`, `CURRENT_STATUS`, `NEXT_DECISION`, and `MODULE_MAP`
- [ ] For a repository-agent smoke, verify `AGENT_SETTINGS_READ_TOKEN` has Administration read only, Actions PR creation is enabled, the named disposable runner is online/idle, and emergency stop remains authoritative until dispatch
- [ ] Select the highest-value eligible packet, prerequisite repair, or bounded decision
- [ ] Audit existing code and recent merged work before assuming capability is absent
- [ ] Restate goal, prerequisites, owners, allowed/forbidden changes, risk, acceptance, rollback, and hard stops
- [ ] Record material decisions in an authoritative document
- [ ] Add or update focused tests before behavior changes when practical
- [ ] Implement one coherent reviewable slice
- [ ] Run focused checks and applicable full verification
- [ ] Review the diff against packet, architecture, module ownership, authority, security, compatibility, audit, and rollback
- [ ] Run `uv run --no-project python scripts/check_agent_handoff.py`
- [ ] Open or update a PR with accurate feedback trace fields
- [ ] Wait for every required CI job to complete, unless the final diff qualifies for the strict documentation-only merge exception
- [ ] Repair failures at their root cause; do not weaken tests or guards
- [ ] Merge only when the auto-merge classifier or documentation-only exception passes
- [ ] Refresh `main`, update active state, and continue if the bounded objective includes later packets

## Verification Baseline

Use focused checks plus applicable commands:

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

Add browser, Docker, migration, release, signing, backup/restore, concurrency, compensation, or fault-specific checks when the change touches those surfaces. Strictly documentation-only PRs use the targeted checks in the exception instead of the full baseline unless an additional documentation-specific check is applicable.

## PR and Merge Policy

Standing authority covers branch creation, commits, PRs, CI repair, independent review, eligible manual squash-merge, and refresh/continuation across ready packets. Automatic merge remains disabled. Manual merge requires the exact reviewed head, all required CI successful, no unresolved objection, a clean scope/path binding, and the normal classifier result; no repeated user confirmation is needed once those conditions pass.

Agents may autonomously create and merge scoped PRs when the classifier or documentation-only exception passes. Do not combine unrelated packets or risk surfaces merely to reduce PR count. When `docs/NEXT_DECISION.md` declares a grouped boundary, its ordered internal packets are one coherent risk surface and may share one branch and PR.

The historical PE-5/PE-6 implementation and closeout boundaries remain in repository history. `PE56-POST-SEAL-REPAIR-1` is one coherent post-seal implementation, independent standards/spec review, documentation, and acceptance PR; it must not be split into PE-5, PE-6, prerequisite, or closeout PRs. Its independent reviews are separate review passes over the same final diff, not separate branches or PRs. Full repository validation and exact-head GitHub CI run only on the complete reviewed head. If a CI repair changes that head, the complete diff is reviewed again and the exact new head receives the full matrix. The final acceptance state is recorded only after those results and post-merge `main` verification exist.

A bounded objective may span multiple PRs in one session. After each merge, refresh `main`, reconcile active docs and open work, and continue only from the new repository state.

### Selective rebase

Do not rebase a focused PR only because `main` advanced with unrelated documentation-only commits. Rebase when the branch conflicts with `main`, when relevant path/schema/authority/workflow surfaces overlap recent `main` changes, when freshness is explicitly required, or when evidence shows a real integration risk. Any head change invalidates prior CI and complete-diff review for that head; re-verify the new head before merge.

### Exact-head CI evidence

Canonical CI evidence must prove which commit was checked out and tested. The `tests` workflow resolves `EXPECTED_SHA` from `inputs.expected_sha` (required on `workflow_dispatch`), the pull-request head SHA, or `github.sha` on push to `main`; checkout uses that commit; every required job must execute the exact-head verification step successfully. A pull-request run that skips that step is not acceptable exact-head evidence. The orchestrator fallback `workflow_dispatch` path continues to pass `expected_sha` and `dispatch_nonce` and must not be weakened.

### Direct-main documentation coordination

While an implementation PR is in final exact-head CI or independent review, defer non-urgent direct-to-`main` documentation changes that would force unnecessary rebases or base drift. Urgent factual corrections on `main` remain allowed under explicit user authorization and the documentation-only direct-main rule, but must be reported as potentially invalidating related base evidence for open PRs.

Documentation-only corrections should use a branch/PR by default. Direct-to-main documentation changes are reserved for explicit user authorization and must pass handoff/diff validation.

For any PR that is not strictly documentation-only, CI must be completely green. Queued, in-progress, unexpectedly skipped, action-required, or failed CI is not success. A qualifying docs-only PR may merge after its targeted checks and final review without waiting for the full CI matrix.

## Documentation Maintenance

Docs maintenance is mandatory but not additive-by-default.

- Update the smallest authoritative surface.
- Put packet direction in `docs/NEXT_DECISION.md`.
- Put current facts in `docs/CURRENT_STATUS.md`.
- Put ownership in `docs/MODULE_MAP.md`.
- Put durable architecture in `docs/ARCHITECTURE_BOOK.md`.
- Put only proven operator procedures in `docs/RUNBOOK.md`.
- Keep stale/historical material under `docs/archive/` when retention is useful.
- Do not create a second roadmap, status, policy, packet, or closeout document.

## Completion Report

Every run reports:

- packet or slice ID and starting/ending state
- material decisions made and where they were recorded
- exact files and behavior changed
- focused tests and full CI run, or docs-only targeted checks and the observed CI state
- compatibility, authority, security, and audit result
- residual risk and rollback
- merge decision
- next eligible packet, prerequisite repair, or evidence-backed blocker
- confirmation that no external tag/release/deploy occurred unless separately authorized
