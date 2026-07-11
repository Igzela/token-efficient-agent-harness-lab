# Real-World Testing Playbook

Operational execution guide for real branches, commits, PRs, CI, review, rollback, and gated autonomous merge.

## Mode Summary

The project validates changes through real repository work. Full Agent Autonomy Mode permits repo-scoped changes only when they are inside an approved Terra-ready packet or are narrow maintenance, and when they are testable, observable, reviewed, CI-gated, and rollbackable.

Agent Autonomous Maintenance Mode uses this loop:

```text
Observe → Select packet → Implement → Verify → Review → Document → PR/merge decision → Report
```

## Mandatory Execution Profile

Codex execution must declare:

- `execution_model: gpt-5.6-terra`
- `reasoning_effort: medium`
- `review_model: gpt-5.6-terra`
- `task_packet_id: <READY_FOR_TERRA packet>`

`.codex/config.toml` supplies the local default. A session using another model or effort must stop with `model_profile_mismatch`. Do not solve failure by switching to Sol or increasing reasoning effort.

The packet in `docs/NEXT_DECISION.md` is the implementation authority. Stage prose, a broad user goal, or an inferred architecture direction is not enough.

## Action Permission Matrix

| Action | Default | Gate |
|---|---|---|
| Branch creation | allowed | latest `main`, clean scope |
| Commit and push | allowed | intended files only, local verification |
| PR creation | allowed | packet/profile declared |
| CI repair | allowed | same packet contract, at most two coherent repair cycles |
| Docs/test/small code maintenance | allowed | no new product decision |
| New endpoint or multi-module implementation | allowed | `READY_FOR_TERRA` packet + focused tests |
| Auth/security/provider boundary | allowed | packet fixes threat boundary, tests, audit, rollback |
| DB migration | allowed | packet fixes schema/compatibility/rollback; SQLite and PostgreSQL tests |
| Release/signing/install changes | allowed | packet fixes trust policy, dry-run evidence, rollback |
| Existing mutation endpoint integration | allowed | packet fixes authority, permission, audit, idempotency, compensation |
| New architecture/authority/recovery decision | stop | external planner must update the packet |
| Irreversible external operation | stop | separate authority and tested recovery required |

## Risk Classification

| Risk | Typical scope | Required evidence |
|---|---|---|
| low | docs, tests, deterministic CI fix, small isolated code | focused check, handoff guard, reviewable diff |
| medium | new endpoint/UI/SDK, multi-module read model, bounded behavior change | focused integration tests, full stack, compatibility, rollback |
| high | auth, provider authority, schema migration, automatic pause/promotion, release trust, recovery | complete packet contract, threat/failure review, focused fault/concurrency tests, full CI, rollback/compensation |

Risk does not change the model. Low, medium, and high packets are all implemented with Terra Medium; high risk requires more complete planner decisions and stronger machine evidence.

## Auto-Merge Classifier

A PR is auto-merge eligible only when all are true:

| Field | Required value |
|---|---|
| `task_packet_id` | earliest eligible packet, or explicit bounded-maintenance ID |
| `packet_state` | `READY_FOR_TERRA` at start; moved to `IN_PROGRESS`/`COMPLETE` truthfully |
| `execution_model` | `gpt-5.6-terra` |
| `reasoning_effort` | `medium` |
| scope | matches goal, owners, allowed changes, and non-goals |
| risk | classified with matching focused evidence |
| CI | every required job completed successfully |
| handoff guard | pass |
| review | diff reviewed against packet, authority, compatibility, and rollback |
| repair cycles | no more than two for the same root cause |
| rollback | clear and sufficient |
| hard stop | none |
| human objection | none |

Green CI alone is not permission to merge an out-of-packet change.

## Feedback Trace Fields

Every packet PR must record:

| Field | Description |
|---|---|
| `task_packet_id` | packet or bounded-maintenance identifier |
| `task_class` | docs, tests, ci-fix, code-fix, schema, API, SDK, Dashboard, migration, security, release, recovery |
| `execution_model` | `gpt-5.6-terra` |
| `reasoning_effort` | `medium` |
| `selected_executor` | normally `codex_cli` |
| `changed_files` | exact intended file list |
| `touched_risk_paths` | auth, security, provider, migration, release, authority, recovery paths |
| `packet_contract_check` | pass/fail with any deviations |
| `focused_tests` | commands and results |
| `ci_result` | pass/fail/queued with run ID |
| `handoff_guard_result` | pass/fail |
| `repair_cycles` | coherent root-cause repair attempts |
| `merge_result` | merged/blocked/pending |
| `compatibility` | existing data/API/SDK/runtime behavior preserved or intentionally versioned |
| `rollback_plan` | exact revert and cleanup procedure |
| `residual_risk` | remaining bounded risk |
| `human_override_reason` | empty unless the user explicitly changed the packet/lane |

## Stop Conditions

Stop and report evidence when any occurs:

1. current execution profile is not Terra Medium
2. no eligible `READY_FOR_TERRA` packet exists
3. packet prerequisites are not complete
4. code or authoritative docs contradict the packet
5. an unspecified architecture, authority, schema, migration, security, trust, signing, or recovery decision is needed
6. the same root cause remains after two coherent repair cycles
7. a real secret would enter version control
8. evidence would be falsified or a known failure hidden
9. a rollback/recovery path would be removed
10. an irreversible external action lacks tested recovery

A stop is not failure. It is the correct output when planner input is incomplete.

## Execution Checklist

For each packet:

- [ ] Confirm `.codex/config.toml` and the active session indicate Terra Medium
- [ ] Start from latest `main`; inspect branch and working tree
- [ ] Read `AGENTS.md`, `CURRENT_STATUS`, `NEXT_DECISION`, and `MODULE_MAP`
- [ ] Select the earliest eligible packet and record its ID
- [ ] Audit existing code and recent merged work
- [ ] Restate goal, prerequisites, owners, allowed/forbidden changes, acceptance, rollback, and stop triggers
- [ ] Add/update focused tests before behavior changes when practical
- [ ] Implement the smallest coherent packet
- [ ] Run focused checks and applicable full verification
- [ ] Review the diff against packet and module ownership
- [ ] Run `uv run --no-project python scripts/check_agent_handoff.py`
- [ ] Open a PR with the feedback trace fields
- [ ] Wait for every required CI job to complete
- [ ] Repair ordinary failures within two coherent cycles or report a blocker
- [ ] Merge only when the auto-merge classifier passes
- [ ] Update packet/current status and report the next packet state

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

Add browser, Docker, migration, release, signing, backup/restore, or fault-specific checks when the packet touches those surfaces.

## PR and Merge Policy

Agents may autonomously create and merge scoped packet PRs when the classifier passes. Do not combine unrelated packets. One active product packet is the default; the PE-5 lane requires explicit activation.

Documentation-only corrections may still use a branch/PR by default. Direct-to-main documentation changes are reserved for explicit user authorization and must pass handoff/diff validation.

CI must be completely green. Queued, in-progress, unexpectedly skipped, or failed CI is not success.

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

- packet ID and starting/ending packet state
- Terra Medium profile declaration
- exact files and behavior changed
- focused tests and full CI run
- compatibility and authority-boundary result
- residual risk and rollback
- merge decision
- next eligible packet or evidence-backed blocker
- confirmation that no external tag/release/deploy occurred unless separately authorized