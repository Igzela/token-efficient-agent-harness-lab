# Real-World Testing Playbook

Operational execution guide for Real-World Testing Mode.

---

## Mode Summary

The project validates the Dynamic Global Regulator through real work: real tasks, real branches, real commits, real PRs, real CI, and gated auto-merge. Full Agent Autonomy Mode permits low- and high-risk repo changes when scoped, testable, observable, reviewed, and rollbackable.

---

## Pilot Matrix — First Real-World Tasks

| # | Task Class | Example | Expected Outcome |
|---|---|---|---|
| 1 | docs-only cleanup | Fix stale references, update inventory | Auto-merge eligible |
| 2 | test-only repair | Fix flaky test, add missing regression test | Auto-merge eligible |
| 3 | CI correctness fix | Fix retry loop, update Docker cache | Auto-merge eligible |
| 4 | small low-risk code fix | Fix typo in error message, add missing enum variant | Auto-merge eligible if diff < 50 lines |
| 5 | context instrumentation | Add structured logging to dispatch decision points | Requires CI green |
| 6 | feedback trace schema | Define trace fields for outcome attribution | Requires CI green |
| 7 | shadow routing stub | Log "what regulator would choose" alongside real decision | Requires CI green |
| 8 | dashboard metrics tab | Add dispatch outcome distribution view | Requires CI green |
| 9 | policy proposal schema | Define proposal structure with evidence fields | Requires CI green |
| 10 | cross-node context assembly | Pass node outputs as inputs to downstream nodes | Requires CI green + review |

---

## Action Permission Matrix

| Action | Default | Gate |
|---|---|---|
| Branch creation | ✅ allowed | none |
| Commit | ✅ allowed | must pass fmt/clippy |
| PR creation | ✅ allowed | none |
| CI trigger | ✅ allowed | none |
| CI repair | ✅ allowed | must fix before merge |
| Docs change | ✅ allowed | auto-merge eligible |
| Tests change | ✅ allowed | auto-merge eligible |
| Small code fix | ✅ allowed | auto-merge if low-risk |
| Target repo edit (via PR) | ✅ allowed | CI must pass |
| Dynamic workflow node injection | ✅ allowed | within existing bounds |
| Green PR merge | ✅ allowed | see classifier below |
| Provider/CLI boundary change | ✅ allowed | tests + audit + rollback |
| Auth/security change | ✅ allowed | threat model + tests + rollback |
| DB migration | ✅ allowed | forward/backward test + rollback |
| Release/tag/deploy workflow change | ✅ allowed | dry-run + rollback; external action separately gated |
| YAML/rubric/policy mutation | ✅ allowed | validation + rollback |
| Irreversible external operation | ❌ stop | recovery path or human decision required |

---

## Auto-Merge Classifier

A PR is auto-merge eligible when ALL conditions are met:

| Field | Required Value |
|---|---|
| `change_type` | documented and scoped |
| `risk_class` | low, medium, or high with matching evidence |
| `touched_paths` | reviewed; risk paths include focused tests and rollback |
| `ci_status` | `success` (all jobs) |
| `handoff_guard_status` | `pass` |
| `rollback_path` | `git revert` is sufficient |
| `hard_stop` | `no` |

### Risk Classification

| Risk Class | Criteria | Auto-Merge |
|---|---|---|
| `low` | docs, tests, CI config, small code fix (< 50 lines, no auth/provider/deploy paths) | ✅ yes |
| `medium` | code change > 50 lines, touches multiple modules, new endpoint | ✅ after review + green CI |
| `high` | auth, security, provider boundary, DB schema, deploy config | ✅ after threat review + focused tests + green CI + rollback |

---

## Feedback Trace Fields

Every real-world test task must produce a feedback trace with these fields:

| Field | Type | Description |
|---|---|---|
| `task_id` | string | Unique task identifier |
| `task_class` | string | One of: `docs`, `tests`, `ci-fix`, `small-code-fix`, `instrumentation`, `schema`, `stub`, `dashboard`, `assembly` |
| `selected_executor` | string | `noop`, `provider`, `claude_code_cli`, `codex_cli` |
| `changed_files` | list[string] | Files modified |
| `touched_risk_paths` | list[string] | Paths matching risk patterns (auth, security, provider, deploy, migration) |
| `ci_result` | string | `pass`, `fail`, `not-triggered` |
| `handoff_guard_result` | string | `pass`, `fail`, `skipped` |
| `retry_count` | int | Number of CI retries needed |
| `merge_result` | string | `merged`, `blocked`, `pending` |
| `rollback_plan` | string | How to undo (e.g., `git revert <sha>`) |
| `human_override_reason` | string | Why human intervened (empty if autonomous) |

---

## Stop Conditions

The system MUST stop when ANY of these are detected:

1. **Real-secret commit** — real credentials or secrets would enter version control
2. **Falsified evidence** — test or CI evidence would be fabricated
3. **Hidden failure** — a known failure would be intentionally concealed
4. **Removed rollback** — an existing rollback path would be removed
5. **Irreversible external destruction** — no recovery path exists

---

## First 10 Real-World Test Tasks

These are the concrete tasks to execute in order:

### Task 1: Active Docs Cleanup
- **Class:** `docs`
- **Goal:** Archive unreferenced docs and keep the active docs set small
- **Auto-merge:** yes
- **Risk:** low

### Task 2: NEXT_DECISION.md Simplification
- **Class:** `docs`
- **Goal:** Remove duplicate planning text, link to playbook
- **Auto-merge:** yes
- **Risk:** low

### Task 3: Agent Entrypoint Update
- **Class:** `docs`
- **Goal:** Keep `AGENTS.md`, `CLAUDE.md`, and the six active docs aligned
- **Auto-merge:** yes
- **Risk:** low

### Task 4: CURRENT_STATUS.md Real-World Mode Note
- **Class:** `docs`
- **Goal:** Record PR #27 merged, Real-World Testing Mode active
- **Auto-merge:** yes
- **Risk:** low

### Task 5: Dispatch Observability Logging
- **Class:** `instrumentation`
- **Goal:** Add structured logging for tier selection, complexity score, constraint matches
- **Auto-merge:** yes (if < 50 lines, no risk paths)
- **Risk:** low-medium

### Task 6: Feedback Trace Schema Definition
- **Class:** `schema`
- **Goal:** Define Rust struct for feedback trace with all mandatory fields
- **Auto-merge:** yes (if tests included)
- **Risk:** low

### Task 7: Shadow Routing Log Stub
- **Class:** `stub`
- **Goal:** Log "regulator would choose X" alongside real dispatch decision
- **Auto-merge:** no (new code, needs review)
- **Risk:** medium

### Task 8: Dashboard Dispatch Metrics Tab
- **Class:** `dashboard`
- **Goal:** Show tier distribution and success rate from dispatch history
- **Auto-merge:** no (new endpoint + UI)
- **Risk:** medium

### Task 9: Policy Proposal Schema
- **Class:** `schema`
- **Goal:** Define proposal structure with evidence, impact, rollback fields
- **Auto-merge:** yes (if tests included)
- **Risk:** low

### Task 10: Cross-Node Context Assembly
- **Class:** `assembly`
- **Goal:** Pass completed node outputs as context to downstream nodes via DAG edges
- **Auto-merge:** no (new behavior, needs review)
- **Risk:** medium

---

## Execution Checklist

For each task:

- [ ] Create branch from latest main
- [ ] Implement change
- [ ] Run `cargo test -p engine` (if code changed)
- [ ] Run `cargo fmt --check` and `cargo clippy` (if code changed)
- [ ] Run `uv run --no-project python scripts/check_agent_handoff.py`
- [ ] Classify risk using Auto-Merge Classifier
- [ ] Create PR with feedback trace fields in body
- [ ] If auto-merge eligible: merge after CI green
- [ ] If not auto-merge eligible: request human review
- [ ] Record feedback trace

---

## Agent Autonomous Maintenance Mode

Agents operate autonomously under Real-World Testing Mode. The loop:

```
Observe → Classify → Act → Verify → Document → PR/Merge Decision → Report
```

### A. Docs Maintenance Rules

Docs maintenance is mandatory but not additive-by-default.

- Every agent task must check whether docs need updating.
- If docs need updating, update the smallest authoritative doc.
- Do not create a new doc unless the topic is operationally distinct and will be referenced by an authoritative doc.
- Prefer pruning, replacing, archiving, or linking over adding more prose.
- If a doc becomes stale or duplicated, mark it archive-candidate or move it to `docs/archive/`.
- Keep active docs limited to the six-file set in `docs/CURRENT_STATUS.md`; archive low-frequency or historical docs under `docs/archive/`.

### B. PR Creation Policy

Agents may autonomously open PRs for:

- docs cleanup
- stale doc repair
- test-only repair
- CI correctness fix
- small low-risk code fix
- wire/codegen drift repair
- handoff guard repair
- feedback trace schema docs
- playbook/classifier updates

Agents must not autonomously open broad PRs that combine unrelated code, docs, and strategy changes.

### C. Merge Policy

Documentation-only changes may be committed directly to `main` after local handoff and diff validation; they do not require a CI wait.

Agents may autonomously merge PRs when ALL are true:

- scope and risk evidence are documented
- CI is green
- handoff guard passed
- focused tests cover changed boundaries
- rollback path is clear
- diff is reviewable
- no explicit human objection

Agents must request human approval before merge when:

- a real secret would need to be committed
- test or CI evidence cannot be reported truthfully
- a known failure would need to be hidden
- the change removes its rollback path
- an irreversible external action has no tested recovery path

### D. Success Standard

- Green CI is the default success standard.
- For docs-only PRs, handoff guard must pass.
- For code PRs, relevant tests must pass.
- Do not claim success if CI is queued, in progress, skipped unexpectedly, or failed.
- If CI fails, fix or report blocker; do not merge.

### E. Report Format

Every autonomous run must report:

| Field | Required |
|---|---|
| classification | task class (docs, tests, ci-fix, code-fix, etc.) |
| changed files | list of files modified |
| docs updated/pruned/archived | what changed in docs and why |
| tests/CI status | pass/fail/queued with run ID |
| PR opened or not | yes + URL, or no + reason |
| merge decision | merged, or approval needed + reason |
| rollback path | how to undo (e.g., `git revert <sha>`) |
| tag/release status | confirmed: no tag/release created |
