# Trial 2 Onboarding Followup

Date: 2026-05-27

## Trial 2 Result Summary

Trial 2 executed against `/home/igzela/Projects/hermes-gateway-lab`. Verdict: `ACCEPTABLE_WITH_NOTES`.

The harness app correctly identified that hermes-gateway-lab lacks required harness control files and returned audit verdict `BLOCKED`. All 5 plans were correctly blocked with `audit_blocked` blocker. Review guidance and triage correctly pointed to the audit failure. The target repo remained completely unchanged.

**The BLOCKED audit was correct behavior, not a failure.** The app does not plan for repos it cannot audit.

## What This Means

The app generalizes correctly to operational repos. The gap is not in the app — it is in the target repo's governance metadata. Non-harness-managed repos need minimal control files before the app can audit and plan against them.

## Next Possible Action

**Target repo onboarding.** Add minimal harness control files to hermes-gateway-lab so the app can audit and plan.

This is documented in `docs/onboarding/TARGET_REPO_ONBOARDING_PLAN.md`.

## Important Constraint

Applying onboarding to hermes-gateway-lab is a **target repo write**. It requires explicit user approval. Do not proceed without approval.

## Recommendation

No Trial 2 rerun until onboarding is applied or the user chooses another target. The current Trial 2 result is sufficient evidence that the app generalizes correctly.

## Replay Completed

The onboarded replay has been executed. Result: `ACCEPTABLE_FOR_ONBOARDED_SECOND_PROJECT_TRIAL`. Full report: `docs/trials/TRIAL_2_ONBOARDED_REPLAY_REPORT.md`.

## Target Onboarding Complete

- Onboarding plan applied to hermes-gateway-lab
- Branch `harness-onboarding` pushed and PR #1 opened
- PR #1 reviewed (APPROVE TARGET PR FOR HUMAN MERGE) and merged (commit `77cf282`)
- Audit after merge: PASS_WITH_NOTES, blockers []
- hermes-gateway-lab is now harness-managed enough for audit and planning
- Future trials may use target main, not local onboarding branch
