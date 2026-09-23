# Project Roadmap

Last updated: 2026-09-23.

This document owns the product-development and repository-maintenance
direction. The active mainline is an autonomous, evidence-backed engineering
loop. Research and model-comparison programs are separate optional work and
must not intercept ordinary development or select the next task.

## Mainline: Autonomous Development and Maintenance

The outcome is a repository an agent can enter, understand, change, verify,
and maintain through ordinary files, tools, tests, Git, and review:

```text
START_HERE.md
  → current user request or next unblocked roadmap objective
  → inspect accepted main, current checkout, relevant owners, tests, and CI
  → implement one coherent change and update its canonical documentation
  → run focused checks and the applicable verification baseline
  → obtain independent review against the exact diff and head
  → satisfy canonical CI and the guarded GitHub merge path
  → read back accepted main and select the next unblocked objective
```

The loop must remain recoverable: preserve unrelated work, keep changing PRs
Draft, bind review and CI evidence to the exact head, retain rollback paths,
and treat uncertain external effects as unresolved until read-only evidence
settles them. Details and hard gates belong to `docs/AUTONOMY.md`; module
ownership belongs to `docs/ARCHITECTURE.md`; proven operator procedures belong
to `docs/RUNBOOK.md`.

### Development priorities

1. Keep `START_HERE.md` sufficient to route an agent into ordinary repository
   work without generated assignment state, an external task card, or a
   lifecycle service.
2. Keep one canonical owner for each runtime, persistence, policy, review,
   merge, and documentation responsibility; remove obsolete parallel control
   paths rather than routing around them.
3. Make the smallest end-to-end product or maintenance change verifiable by
   focused tests, the required verification baseline, independent exact-head
   review, and canonical CI.
4. Close failures at their root cause and retain observable evidence,
   recovery, and rollback; never turn missing evidence into success.
5. After accepted-main readback, refresh the repository and choose the next
   unblocked user or roadmap objective. Do not resume from stale branches,
   reviews, CI, or generated context.

### Completion criteria

- An agent can start from `START_HERE.md` and determine the relevant source of
  truth without first acquiring a task-card, Mission, Stage, or journal record.
- Normal repository edits and verification do not require an orchestration
  service, special hook lifecycle, or generated scope declaration.
- Repository safety checks, exact-head review, required CI, and guarded merge
  remain intact and independent of task assignment.
- Canonical documents describe the live owners and procedures rather than
  historical orchestration as an active or fallback execution path.
- A delivered change has live GitHub and accepted-main readback before it is
  reported as merged or complete.

## Optional Research Program (Parked)

The repository contains a separate finite-experiment program around Real
Workload Evidence (RWE), Harness × Model × Strategy comparisons, Context
Working Set, and Harness Evolution. Its contracts and implementation remain
available as product capabilities and historical research design. They are
not the repository's development mainline, are not prerequisites for coding,
and are not authorized for execution merely by appearing in this document.

Any future research execution must first refresh the current canonical
experiment package, evaluator, corpus, runtime/provider availability, finite
budget, and effect authorization. Historical status snapshots in older notes
are not live evidence. Until research is directly selected and its existing
authority is verified, keep provider-backed experiments and live effects
parked; continue ordinary product development and maintenance.

Research architecture and immutable product contracts are documented in
`docs/ARCHITECTURE.md` and implemented under `engine/src/rwe/` and
`engine/src/harness_evolution*.rs`. They do not own repository task selection,
review, CI, merge, release, or deployment authority.
