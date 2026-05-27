# Trial 2 Final State Index

Trial 2 verified that the Harness App generalizes to a second operational repo after minimal target onboarding.

## Final Status

| Field | Value |
|-------|-------|
| Initial candidate | hermes-gateway-lab |
| Initial audit | BLOCKED |
| Cause | Missing harness control files |
| Onboarding protocol | Created and applied |
| Target PR | hermes-gateway-lab PR #1 merged |
| Target main | Onboarding files present |
| Final verification | PASS |
| Final verdict | `TRIAL_2_FINAL_VERIFICATION_PASS` |
| Target repo writes | Only approved onboarding control files |
| Source/runtime changes | None |
| Provider/model/sandbox/worker | None |
| MVP9 / Stage 5 / CA-8 | Not started |

## Evidence Chain

1. `docs/trials/TRIAL_2_CANDIDATE_SELECTION.md` — Candidate scoring and selection
2. `docs/trials/TRIAL_2_PLAN.md` — Execution plan
3. `docs/trials/TRIAL_2_REPORT.md` — Initial execution (BLOCKED)
4. `docs/onboarding/TARGET_REPO_ONBOARDING_PLAN.md` — Onboarding protocol
5. `docs/onboarding/TARGET_REPO_ONBOARDING_TEMPLATE.md` — Onboarding templates
6. `docs/trials/TRIAL_2_ONBOARDING_FOLLOWUP.md` — Onboarding followup
7. `docs/trials/TRIAL_2_ONBOARDED_REPLAY_REPORT.md` — Replay after onboarding
8. `docs/trials/TRIAL_2_FINAL_VERIFICATION_REPORT.md` — Final verification from target main

## Target Repo Final State

| Field | Value |
|-------|-------|
| Repo | `/home/igzela/Projects/hermes-gateway-lab` |
| Branch | `main` |
| Merged onboarding commit | `77cf282` |
| Audit | `PASS_WITH_NOTES`, blockers `[]` |
| Warnings | 17 structural/non-blocking |
| Pre-existing dirty draft | Preserved |
| Future trials | Use target main, not `harness-onboarding` branch |

## Interpretation

- BLOCKED before onboarding was correct behavior.
- Onboarding made the repo harness-managed enough for audit/planning.
- Final verification from target main passed.
- The system generalized beyond alters-lab to an operational repo.
- Planning/guidance/triage remained non-executable and human-review-only.

## Do Not Do Next by Default

- Do not start Trial 3 automatically.
- Do not start MVP9.
- Do not start Stage 5.
- Do not add provider/model/sandbox/worker behavior.
- Do not write target repos without explicit approval.

## Possible Next Choices

- Stop
- Trial 3 on another repo
- Reliability hardening only if backed by new evidence
- Future production PRD
- Docs/demo polish only if user feedback requires it
