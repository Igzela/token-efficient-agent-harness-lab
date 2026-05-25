# Trial 1 Report: Multi-Task Queue and Token Budget Efficiency Validation

## Executive Summary

| Field | Result |
| --- | --- |
| Verdict | `ACCEPTABLE_WITH_NOTES` |
| Technical execution | successful |
| Target repository mutation | none |
| Harness repository mutation during trial | none |
| App-owned state | `/tmp/harness-trial1-registry.json`, `/tmp/harness-trial1-plans.json` |

Trial 1 successfully exercised the existing Harness App MVP0-MVP8 read-only
control plane against a real local project instance. The app registered the
target repository, confirmed a clean audit, created multiple non-executable
plans, compared a lower-budget variant, generated review guidance, produced
portfolio triage, and returned final diagnostics with all components `ok`.

The main product finding is that planning, review guidance, and portfolio triage
are over-gated when task text contains negated risk phrases. Low-risk read-only
tasks became `needs_approval` because constraints such as `no target repo
writes` still contain the keyword `writes`. That made triage less useful because
all plans landed in the same review bucket and priority band.

## Metadata

| Field | Result |
| --- | --- |
| Trial date/time | 2026-05-25 18:16-18:17 Asia/Shanghai |
| Harness commit | `23cf353e2e01de3675be29aa5e448cea4d2e7fc9` |
| Target repository | `/home/igzela/Projects/alters-lab` |
| Target commit | `af86b90923eb87291f0b4fcf2a1079383361ba45` |
| App URL | `http://127.0.0.1:8769/` |
| Registry path | `/tmp/harness-trial1-registry.json` |
| Plans path | `/tmp/harness-trial1-plans.json` |

## Preflight

| Check | Result |
| --- | --- |
| Harness status | `main...origin/main`, clean |
| Target status | `main...origin/main`, clean |
| Security checker | `PASS` |
| Unit tests | `898 OK` |
| Dashboard JavaScript syntax | `PASS` |
| Git diff whitespace check | `PASS` |

## Audit Result

Audit result for `/home/igzela/Projects/alters-lab`:

- verdict: `PASS`
- warnings: `[]`
- blockers: `[]`
- `agents_policy`: `PASS`
- `project_board`: `PASS`

## Diagnostics

Initial diagnostics after repository registration had warnings only because the
plan store was still empty. After plan creation and review derivation, final
diagnostics were clean:

- final status: `ok`
- component count: `10`
- warning components: `0`
- blocked components: `0`
- recent errors: `[]`
- registry storage: `/tmp/harness-trial1-registry.json`, exists, repo count `1`
- plan store storage: `/tmp/harness-trial1-plans.json`, exists, plan count `6`
- data flow: all steps `ok`

The final diagnostics boundary evidence continued to state:

- provider: `no_calls`
- sandbox: `no_execution`
- target repository: `read_only`

## Plans Created

All plans were written only to the app-owned `/tmp` plan store. Every plan
remained `executable=false`.

| Plan ID | Task ID | Total Budget | Context Budget | Execution Budget | Status | Gates | Executable |
| --- | --- | ---: | ---: | ---: | --- | ---: | --- |
| `plan-db02cc1abebfd315` | `trial1-docs-governance` | 1600 | 800 | 800 | `needs_approval` | 2 | `false` |
| `plan-3aee4204d617c646` | `trial1-audit-health` | 1400 | 800 | 600 | `needs_approval` | 2 | `false` |
| `plan-f9942510ef98941b` | `trial1-small-code-review` | 2000 | 800 | 1200 | `needs_approval` | 2 | `false` |
| `plan-50e927830d409eb9` | `trial1-provider-boundary` | 1700 | 800 | 900 | `needs_approval` | 3 | `false` |
| `plan-4d455734f8e56ac2` | `trial1-budget-pressure` | 7800 | 6000 | 1800 | `needs_approval` | 2 | `false` |
| `plan-034c43a3574b446c` | `trial1-budget-pressure-low` | 1700 | 800 | 900 | `needs_approval` | 2 | `false` |

Aggregate plan summary:

- total plans: `6`
- blocked: `0`
- needs approval: `6`
- ready for review: `0`
- plans with approval gates: `6`
- plans with blockers: `0`
- total token budget: `16200`
- average token budget: `2700`
- most common next review action: `review_approval_gates`

## Lower-Budget Variant Comparison

| Field | Result |
| --- | --- |
| Original plan | `plan-4d455734f8e56ac2` |
| Lower-budget plan | `plan-034c43a3574b446c` |
| Token budget delta | `-6100` |
| Context budget delta | `-5200` |
| Execution budget delta | `-900` |
| Status delta | `same` |
| Next review action delta | `same` |
| Approval gate delta | `0` |
| Blocker delta | `0` |

The comparison was understandable and useful. It clearly showed that the lower
variant moved planner, advisor, and executor context mode from `full` to
`summary` while preserving the same status and gate count.

The comparison also exposed a limitation: because both plans were over-gated,
the next review action stayed `review_approval_gates` instead of emphasizing the
budget decision.

## Review Guidance

Original budget-pressure plan guidance:

- plan: `plan-4d455734f8e56ac2`
- preview only: `true`
- executable: `false`
- recommended option: `inspect_gates`
- allowed effect: `human_review_only`
- token guidance: `Total planned token budget is 7800; compare with lower-budget variants when available.`

Lower-budget variant guidance:

- plan: `plan-034c43a3574b446c`
- preview only: `true`
- executable: `false`
- recommended option: `inspect_gates`
- allowed effect: `human_review_only`
- token guidance:
  - inspect whether summary or excerpt context is sufficient before increasing
    context budget
  - execution budget is tight; prioritize verifier and gate review before
    optional detail
  - total planned token budget is `1700`; compare with lower-budget variants
    when available

The guidance preserved the boundary: advisory only, no approval, no execution,
and no mutation.

## Portfolio Triage

Portfolio triage result:

- total plans: `6`
- returned items: `6`
- needs approval: `6`
- blocked: `0`
- ready for review: `0`
- token hotspot count: `6`
- budget pressure count: `6`
- top plan: `plan-034c43a3574b446c`
- all review buckets: `review_gates`
- all bottlenecks: `approval_gates`

The top plan was weakly selected. All plans had `review_priority=80`, so the
sort favored later stored items rather than a semantically stronger priority.
Triage still surfaced token budgets and hotspot labels, but it did not provide
enough differentiation between normal gated work, high-risk provider-boundary
work, and budget-pressure work.

## Important Finding

Trial 1 exposed over-gating:

- Low-risk read-only tasks became `needs_approval`.
- The likely cause is keyword matching over negated risk phrases such as `no
  target repo writes`.
- The planner interpreted `writes` as mutation risk despite the negation.
- Review guidance then prioritized `inspect_gates`.
- Portfolio triage gave every plan the same `review_gates` bucket and
  `approval_gates` bottleneck.

This means the app can technically manage a multi-plan portfolio, but its
review-priority signal is not yet semantically strong enough for real
token-efficiency optimization when task constraints contain explicit negative
boundaries.

## Boundary Confirmation

The trial preserved all required boundaries:

- `/home/igzela/Projects/alters-lab` remained clean
- target repository `git diff --stat` was empty
- `/home/igzela/Projects/token-efficient-agent-harness-lab` remained clean
- harness repository `git diff --stat` was empty during execution
- no provider or model calls
- no sandbox, process, container, or VM execution beyond the local Harness App
  server
- no autonomous workers
- no Stage 5
- no MVP9
- no plan execution
- no target repository writes

## Final Verdict

`ACCEPTABLE_WITH_NOTES`

Trial 1 succeeded as a read-only multi-task validation. It proved that the app
can create and inspect a portfolio of non-executable plans and compare token
budgets while preserving target repository immutability.

The notes are material: over-gating and weak triage differentiation should be
treated as targeted reliability hardening before claiming that the system
optimizes token efficiency across multiple real tasks.

## Recommended Next Decision

Persist this report first, then perform targeted reliability hardening for:

1. negated-risk phrase handling
2. over-gating
3. triage ranking differentiation when all plans share `needs_approval`

Do not start MVP9, Stage 5, provider integration, sandbox execution, or
additional target repository work from this report alone.
