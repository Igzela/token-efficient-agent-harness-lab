# Real-World Testing Playbook

Operational execution guide for real branches, commits, PRs, CI, review, rollback, and gated autonomous merge.

## Mode Summary

The project validates changes through real repository work. Full Agent Autonomy Mode permits repo-scoped planning and execution when changes are testable, observable, reviewed, CI-gated, and rollbackable.

Execution-ready packets are the default work units:

```text
Observe → Select or repair packet → Decide → Implement → Verify → Review → Document → PR/merge decision → Continue or report
```

The coding agent may resolve bounded missing decisions from current code, merged history, tests, and authoritative documents. Material architecture, authority, schema, migration, security, release, or recovery decisions must be recorded before dependent behavior is merged.

## Model Selection

The user or execution tool selects the model and reasoning effort. PRs may record the selected model for operational traceability, but model identity is not an eligibility gate and is not validated by repository CI.

The packet in `docs/NEXT_DECISION.md`, current code, merged history, and verified contracts jointly define implementation authority. A broad user goal may authorize a bounded multi-packet objective when the agent keeps each PR coherent and refreshes `main` between merges.

## Action Permission Matrix

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
| low | docs, tests, deterministic CI fix, small isolated code | focused check, handoff guard, reviewable diff |
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
| CI | every required job completed successfully |
| handoff guard | pass |
| review | diff reviewed against architecture, authority, compatibility, security, audit, and rollback |
| rollback | clear and sufficient |
| external authority | no unapproved irreversible operation or missing required human approval |
| human objection | none unresolved |

Green CI alone is not permission to merge a misleading, incompatible, or unreviewed change.

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
| `ci_result` | pass/fail/queued with run ID |
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
5. required human approval, external credentials, or unavailable access blocks validation;
6. another agent owns conflicting in-progress work that cannot be reconciled safely;
7. materially contradictory requirements cannot be resolved from code, merged history, tests, and authoritative documents;
8. required CI remains failed, queued, in progress, or unexpectedly skipped at merge time.

Do not stop merely because a packet is stale, a bounded decision is missing, or an initial implementation failed. Audit, update the contract, repair the root cause, and continue while work remains evidence-driven and rollbackable.

## Execution Checklist

For each coherent packet or slice:

- [ ] Start from latest `main` or audit the current owned PR
- [ ] Inspect open PRs, recent merges, branch state, and CI
- [ ] Read `AGENTS.md`, `CURRENT_STATUS`, `NEXT_DECISION`, and `MODULE_MAP`
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
- [ ] Wait for every required CI job to complete
- [ ] Repair failures at their root cause; do not weaken tests or guards
- [ ] Merge only when the auto-merge classifier passes
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

Add browser, Docker, migration, release, signing, backup/restore, concurrency, compensation, or fault-specific checks when the change touches those surfaces.

## PR and Merge Policy

Agents may autonomously create and merge scoped PRs when the classifier passes. Do not combine unrelated packets or risk surfaces merely to reduce PR count.

A bounded objective may span multiple PRs in one session. After each merge, refresh `main`, reconcile active docs and open work, and continue only from the new repository state.

Documentation-only corrections should use a branch/PR by default. Direct-to-main documentation changes are reserved for explicit user authorization and must pass handoff/diff validation.

CI must be completely green. Queued, in-progress, unexpectedly skipped, action-required, or failed CI is not success.

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
- focused tests and full CI run
- compatibility, authority, security, and audit result
- residual risk and rollback
- merge decision
- next eligible packet, prerequisite repair, or evidence-backed blocker
- confirmation that no external tag/release/deploy occurred unless separately authorized