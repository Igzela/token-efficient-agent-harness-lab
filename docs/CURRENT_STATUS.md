# Current Status

Last verified: 2026-05-27.

## Repository State

- Branch: `main` synced after `592803f` (Phase 1 dispatch kernel stable).
- Tests: **1074 pass**, 0 failures.
- Security baseline: ALL CHECKS PASSED.

## Completed Tracks

| Track | Status |
|---|---|
| Stage 0 — Foundation | Complete |
| Stage 1 — Deterministic Harness Core | Complete |
| Stage 2 — Quality Runtime | Complete |
| Stage 3 — Controlled Intelligence Stubs | Complete |
| Stage 4 — Advanced Runtime Abstractions | Complete |
| CA-7 Sealed Baseline | Complete — policy baseline sealed |
| Post-closeout hardening/design | Complete |
| Harness App MVP0–MVP8 | Complete |
| Trial 0 — Real target acceptance | Closed — `PASS` |
| Trial 1 — Multi-task budget validation | Closed — `ACCEPTABLE_FOR_MULTI_TASK_TRIAL_AFTER_HARDENING` |
| Reliability Hardening 1 — Negated risk and triage | Complete |
| Demo packaging | Complete |
| Demo verification | Complete — all docs accurate and runnable |
| Trial 2 candidate selection | Planned — hermes-gateway-lab recommended |
| Trial 2 execution | Closed — `ACCEPTABLE_WITH_NOTES` (audit BLOCKED on target, generalization finding) |
| Target repo onboarding plan | Complete — plan and templates ready, awaiting user approval for target writes |
| Target repo onboarding (hermes-gateway-lab) | Complete — PR #1 merged (commit `77cf282`), onboarding files on target main, audit PASS_WITH_NOTES / blockers [] |
| Trial 2 onboarded replay | Closed — `ACCEPTABLE_FOR_ONBOARDED_SECOND_PROJECT_TRIAL` (audit PASS_WITH_NOTES, 0 blockers, 5 plans created) |
| Trial 2 final verification | Closed — `TRIAL_2_FINAL_VERIFICATION_PASS` (audit PASS_WITH_NOTES from target main, 5 plans, all boundary confirmed) |
| Trial 3 multi-repo generalization | Closed — `TRIAL_3_MULTI_REPO_GENERALIZATION_PASS` (3 repos: API/CLI/infra, all BLOCKED→PASS_WITH_NOTES, 6 plans, triage working) |
| Trial 3 target merge | Closed — all 3 target PRs merged, audit PASS_WITH_NOTES, blockers [] |
| Global Architecture Book v1 | Approved — 3-round Claude+GPT collaborative review, Phase 1 implementation-ready |
| Phase 1 — Dispatch Kernel | **STABLE** — 8 source files, 20 fixtures, 1074 total tests, commits `a4227e9`→`aed213b`→`592803f` |

Trial 2 complete evidence chain: [`docs/trials/TRIAL_2_FINAL_STATE_INDEX.md`](trials/TRIAL_2_FINAL_STATE_INDEX.md).
Trial 3 report: [`docs/trials/TRIAL_3_REPORT.md`](trials/TRIAL_3_REPORT.md).
Trial 3 target merge closeout: [`docs/trials/TRIAL_3_TARGET_MERGE_CLOSEOUT.md`](trials/TRIAL_3_TARGET_MERGE_CLOSEOUT.md).

## Phase 1 Dispatch Kernel — Closeout

**Stable commit:** `592803f`
**P0 fixes:** `aed213b` (5 P0 blockers from GPT review)
**P1 evidence precision:** `592803f` (flag-specific negative evidence)
**Tests:** 1074 pass (was 914 at Phase 0 end)
**GPT verdict:** Phase 1 Stable — approved for Phase 2 planning

**Phase 1 boundaries (sacred):** no real provider calls, no sandbox execution, no target repo writes, no autonomous workers.

**Accepted limitations (non-blocking, Phase 2/3 refinement):**
- Compound "or" negations ("without any X or Y") only match first phrase
- Evidence spans use placeholder (0, 0) instead of exact phrase position
- Budget pressure is diagnostic, not a selector-changing mechanism
- fallback_tier mixes fallback/escalation semantics

**Next eligible path:** Phase 2 Manual Execution Bridge planning

## Current App Capability

The local Harness App (MVP0–MVP8) provides:

- **Repo registry** — register local or remote target repositories.
- **Local target audit** — read-only inspection of harness control files in a target repo.
- **Non-executable planning** — deterministic resource plans with steps, budgets, approval gates, and blockers. Plans are never executed.
- **App-owned plan store** — plans persist in a local JSON file owned by the app.
- **Plan review workbench** — plan history, summary, comparison, and advisory review actions.
- **Review guidance** — non-persistent advisory guidance derived from stored plans.
- **Portfolio triage** — read-only ranking of stored plans by risk, budget, and bottleneck.
- **Operations diagnostics** — component health, data flow, storage status, recent errors.

## State Boundary

| State | Owner | Writable | Description |
|---|---|---|---|
| Target repositories | User | No (read-only by app) | The app never writes to target repos. |
| App registry | App | Yes | Stores registered repo metadata. |
| Plan store | App | Yes | Stores non-executable resource plans. |
| Diagnostics | Derived | No | Computed on each request from app state. |
| Review guidance | Derived | No | Computed from plan store. Not persisted. |
| Portfolio triage | Derived | No | Computed from plan store. Not persisted. |

No app output constitutes execution authority. The human operator remains the final decision-maker.
