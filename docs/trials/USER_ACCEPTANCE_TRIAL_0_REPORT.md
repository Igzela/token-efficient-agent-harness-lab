# User Acceptance Trial 0 Report

## Trial Summary

| Field | Result |
| --- | --- |
| Verdict | `ACCEPTABLE_WITH_NOTES` |
| Trial date | 2026-05-25 |
| Harness repository | `/home/igzela/Projects/token-efficient-agent-harness-lab` |
| Harness commit | `c27380b88711aacf881e13b5113b7cf7d277c00e` |
| Harness status | `main...origin/main`, clean |
| Target repository | `/home/igzela/Projects/alters-lab` |
| Target commit | `ce86497d8a607a5f4ac3db281ab5a30bd798b8db` |
| Target status | `main...origin/main`, clean before and after trial |
| Trial app URL | `http://127.0.0.1:8769/` |
| Registry path | `/tmp/harness-app-trial-registry.json` |
| Plan store path | `/tmp/harness-app-trial-plans.json` |

The trial used the sealed Harness App MVP0-MVP8 local operations/control-plane
prototype against the real local `alters-lab` repository. Claude Code was used
as a read-only observer, while Codex monitored repository status and process
state from outside the trial.

No residual Claude or Harness app server processes remained after the trial.
The only writes observed were app-owned trial state under `/tmp` for the local
registry and plan store.

## Boundary Confirmation

The trial preserved the sealed MVP0-MVP8 boundaries:

- no target repository writes
- no provider or model API calls by the Harness app
- no sandbox, process, container, or VM execution
- no autonomous workers
- no approval, run, execute, deploy, or merge controls
- no CA-8
- no Stage 5
- no production runtime behavior

The `alters-lab` working tree remained clean before and after the trial.

## Observed Results

### API Health

```json
{"mode": "local_read_only_control_plane", "status": "ok"}
```

### Repository Registration

The app-owned registry contained one local target:

```json
{
  "id": "alters-lab",
  "kind": "local",
  "name": "Alters Lab",
  "path": "/home/igzela/Projects/alters-lab"
}
```

### Audit

Audit result for `/home/igzela/Projects/alters-lab`:

- verdict: `PASS_WITH_NOTES`
- blockers: `[]`
- checks: `8`
- warnings:
  - `AGENTS.md does not explicitly mention main/master push restrictions`
  - `PROJECT_BOARD.md has structurally suspicious table rows: line 32: P1-004 | Controlled Snapshot YAML Write | done |; line 33: P1-005 | Controlled Branches YAML Write | done |; line 34: P1-006 | Controlled Alter YAML Write | done |; line 36: P1-008 | Controlled Dialogue YAML Write | done |; line 37: P1-009 | Reality Trace / Weekly Evidence Controlled Write | done |`

Codex follow-up inspection found that `alters-lab/AGENTS.md` already contains
the phrase `git push to the current branch (NOT main/master directly)`. The
main/master push warning is therefore likely a false positive caused by
conservative wording recognition.

The `PROJECT_BOARD.md` table warning appears valid. The listed rows look like
Markdown table rows missing a leading pipe.

### Plan Generation

One deterministic non-executable plan was generated in the app-owned `/tmp`
plan store:

| Field | Result |
| --- | --- |
| Plan ID | `plan-f3469d2dc1d47f63` |
| Repo ID | `alters-lab` |
| Task ID | `trial-001` |
| Task type | `bug_fix` |
| Objective | `Fix date display in weekly review header` |
| Status | `ready_for_review` |
| Executable | `false` |
| Context budget | `2500` |
| Execution budget | `3000` |
| Total budget | `5500` |
| Next review action | `review_token_budget` |
| Approval gates | `0` |
| Blockers | `0` |

Plan-store summary:

- total plans: `1`
- ready for review: `1`
- blocked: `0`
- needs approval: `0`
- average token budget: `5500`

### Review Guidance

Review guidance for `plan-f3469d2dc1d47f63`:

- preview only: `true`
- executable: `false`
- status: `ready_for_review`
- recommended option: `reduce_budget`
- next review action: `review_token_budget`
- allowed effect: `human_review_only`
- evidence requirements:
  - `plan_boundary`, required
  - `audit_result`, required
  - `token_budget_review`, optional
- token-efficiency guidance:
  - inspect whether summary or excerpt context is sufficient before increasing context budget
  - compare with lower-budget variants when available

### Portfolio Triage

Portfolio triage result:

- non-executable: `true`
- persistent: `false`
- generated from store only: `true`
- returned items: `1`
- blocked: `0`
- ready for review: `1`
- token hotspot count: `1`
- budget pressure count: `1`

The trial plan was classified as:

- bottleneck: `token_hotspot`
- review bucket: `token_budget_review`
- review priority: `60`
- recommended human focus: `Review token hotspots before requesting more context.`

### Diagnostics

App status and diagnostics:

- overall status: `ok`
- mode: `local_read_only_control_plane`
- component count: `10`
- all components: `ok`
- registry storage: `ok`, record count `1`
- plan store storage: `ok`, record count `1`
- recent errors: `[]`
- recommended debug action: `All app diagnostics are readable; continue review through non-executable panels.`

The diagnostics boundary notice stated that diagnostics are read-only and do not
approve, execute, mutate, assign, call providers, launch workers or sandboxes, or
write target repositories.

## Interpretation

The MVP0-MVP8 Harness app is acceptable for local real-project read-only trials.
It successfully registered a real local repository, audited it, generated a
non-executable plan, derived review guidance, triaged the plan, and reported
clean app diagnostics without modifying the target repository.

The operations/debug dashboard and diagnostics layer are useful enough to
surface component health, storage state, data flow, and recent-error state. The
plan/guidance/triage chain produced a meaningful token-budget signal:
`review_token_budget`, `reduce_budget`, and `token_hotspot`.

The audit warning about `AGENTS.md` is probably a false positive. The target
repo already states that `git push` should go to the current branch and not
directly to `main/master`.

The `PROJECT_BOARD.md` table warning appears valid and belongs to target repo
cleanup, not Harness app mutation.

## Follow-Up Decisions

Recommended sequence:

1. Persist this Trial 0 report as the evidence anchor.
2. Then consider a small Harness reliability hardening PR for the `AGENTS.md`
   push-restriction false positive.
3. Consider `alters-lab` `PROJECT_BOARD.md` table cleanup only after explicit
   approval for target repository writes.
4. Defer lower-budget variant comparison to a later Trial 0.1.
5. Do not start MVP9 by default.

## Final Recommendation

Stop feature expansion for now.

Use this report as the evidence baseline that MVP0-MVP8 can support a local,
real-project, read-only trial. The next Harness-side improvement, if approved,
should be reliability hardening for audit wording detection. The next
`alters-lab` improvement, if approved, should be a small documentation cleanup
for the `PROJECT_BOARD.md` table rows.

## Final Closeout Addendum

| Field | Result |
| --- | --- |
| Final closeout date | 2026-05-25 |
| Harness repository | `/home/igzela/Projects/token-efficient-agent-harness-lab` |
| Harness status | `main...origin/main`, clean |
| Target repository | `/home/igzela/Projects/alters-lab` |
| Target HEAD | `af86b90923eb87291f0b4fcf2a1079383361ba45` |
| Target origin/main | `af86b90923eb87291f0b4fcf2a1079383361ba45` |
| Target cleanup commit | `af86b90 Fix project board markdown table formatting` |

After the initial Trial 0 report was captured, the target repository cleanup
was performed with explicit human approval. The cleanup fixed the Phase 1
Markdown table leading-pipe formatting in `docs/harness/PROJECT_BOARD.md`.

The cleanup made no task status changes, no phase semantic changes, no
`AGENTS.md` changes, and no target source-code changes.

Final audit result for `/home/igzela/Projects/alters-lab`:

- verdict: `PASS`
- warnings: `[]`
- blockers: `[]`
- `agents_policy`: `PASS`
- `project_board`: `PASS`

The closeout preserved the project boundaries:

- no provider or model API calls
- no sandbox, process, container, or VM execution
- no autonomous workers
- no target repository source mutation
- no CA-8
- no Stage 5

Final interpretation: Trial 0 is closed as successful. Harness App MVP0-MVP8
can audit and guide a real local project instance from an initial
`ACCEPTABLE_WITH_NOTES` finding to a clean `PASS` state without crossing the
sealed execution, provider, sandbox, autonomous-worker, CA-8, or Stage 5
boundaries.
