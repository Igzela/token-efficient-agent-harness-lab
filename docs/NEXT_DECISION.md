# Next Decision

## Default Recommendation

**Stop unless the user chooses a new path.** The completed Stage 0–4 task-book scope, CA-7 sealed baseline, Harness App MVP0–MVP8, Trials 0–1, and Reliability Hardening 1 are all complete. No next track is automatic.

## Allowed Next Paths

The user may choose any of the following. Each requires explicit approval before work begins.

| Path | Description |
|---|---|
| Trial 2 final verification on target main | Run harness audit and planning against hermes-gateway-lab target main to verify onboarding is effective post-merge. |
| Trial 3 on another repo | Run Trial 3 against a third target repo (requires target onboarding or BLOCKED handling). |
| Demo polish | Refine demo docs only if user feedback requires it. |
| Future production PRD | Draft a product requirements document for a future production track (requires new approval). |
| Additional reliability hardening | Only if backed by new trial evidence that identifies specific issues. |

## Disallowed by Default

The following are **not** allowed without explicit human approval and a new implementation plan:

- **MVP9** — no MVP9 scope has been defined.
- **CA-8** — CA-7 is sealed. No CA-8 exists.
- **Stage 5** — no Stage 5 implementation has been started.
- **Provider/model integration** — real API calls to model providers.
- **Sandbox/process/container/VM execution** — real isolation beyond logical file claims.
- **Autonomous workers** — real concurrent worker processes.
- **Target repo writes** — any mutation of registered target repositories.
- **Approval/run/execute/deploy/merge controls** — any execution or deployment mechanism.

## Before Proposing a New Track

1. Read `docs/CURRENT_STATUS.md` to confirm the latest state.
2. Confirm the proposed track is not in the disallowed list above.
3. Present the track to the user for explicit approval.
4. Do not begin implementation until approved.
