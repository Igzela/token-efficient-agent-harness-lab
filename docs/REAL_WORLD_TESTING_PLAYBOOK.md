# Real-World Testing Playbook

Operational execution guide for Real-World Testing Mode.

---

## Mode Summary

The project validates the Dynamic Global Regulator through real work: real tasks, real branches, real commits, real PRs, real CI, and gated auto-merge. This is controlled autonomy — low-risk work proceeds autonomously; high-risk work requires human approval.

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
| Low-risk auto-merge | ✅ allowed | see classifier below |
| Provider/CLI boundary change | ❌ requires approval | explicit user approval |
| Auth/security change | ❌ requires approval | explicit user approval |
| DB migration | ❌ requires approval | explicit user approval |
| Release/tag/deploy | ❌ requires approval | explicit user approval |
| YAML/rubric/policy mutation | ❌ requires approval | explicit user approval |
| Destructive operation | ❌ requires approval | explicit user approval |

---

## Auto-Merge Classifier

A PR is auto-merge eligible when ALL conditions are met:

| Field | Required Value |
|---|---|
| `change_type` | `docs` OR `tests` OR `ci-fix` OR `small-code-fix` |
| `risk_class` | `low` |
| `touched_paths` | no paths matching `auth`, `security`, `provider`, `deploy`, `migration`, `.env` |
| `ci_status` | `success` (all jobs) |
| `handoff_guard_status` | `pass` |
| `rollback_path` | `git revert` is sufficient |
| `approval_required` | `no` |

### Risk Classification

| Risk Class | Criteria | Auto-Merge |
|---|---|---|
| `low` | docs, tests, CI config, small code fix (< 50 lines, no auth/provider/deploy paths) | ✅ yes |
| `medium` | code change > 50 lines, touches multiple modules, new endpoint | ❌ no — needs review |
| `high` | auth, security, provider boundary, DB schema, deploy config | ❌ no — needs explicit approval |

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

The system MUST stop and request human approval when ANY of these are detected:

1. **Secrets risk** — API keys, tokens, or passwords in changed files
2. **Auth/security boundary change** — modifications to auth middleware, key management, or security controls
3. **Provider/CLI execution boundary expansion** — changes to `ACP_ENABLE_PROVIDER_EXECUTION`, `ACP_ENABLE_CLI_EXECUTION`, or `ACP_EXECUTION_MODE` defaults
4. **Database migration** — schema changes, new tables, column modifications
5. **Deploy/release/tag** — release scripts, version tags, deployment config
6. **Destructive or irreversible operation** — data deletion, credential rotation, schema drops
7. **Active YAML/rubric/policy mutation** — CI workflow changes, governance rule changes, routing policy changes
8. **CI failure after retry** — CI fails 3 times on the same change
9. **Unclear rollback path** — cannot describe how to undo the change in one sentence
10. **Large or hard-to-review diff** — > 200 lines changed, or diff touches > 5 files across modules

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

Agents may autonomously merge low-risk PRs only when ALL are true:

- PR is docs-only, tests-only, CI fix, or small low-risk code fix
- CI is green
- handoff guard passed
- no secrets/auth/security/provider/deploy/db/rubric/policy boundary changes
- rollback path is clear
- diff is reviewable
- no explicit human objection

Agents must request human approval before merge when:

- release/tag/deploy involved
- auth/security/db/provider/CLI execution boundary involved
- active YAML/rubric/policy mutation involved
- destructive or irreversible operation involved
- CI is failing or missing
- diff is large or cross-cutting
- rollback path is unclear

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
