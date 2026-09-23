# Autonomy and Testing Contract

Last updated: 2026-09-23.

This document owns repository-development permissions, verification, independent review, exact-head CI, guarded merge, and recovery. The normal loop begins at `START_HERE.md` and is driven by the user's request or the next unblocked roadmap item.

## Autonomous repository work

Within the user's request, an agent may inspect, plan, resolve bounded design details, implement, test, document, repair CI, update a focused pull request, and complete the repository's review and merge process. Use one coherent branch/PR and preserve unrelated work. Do not write directly to protected `main`.

Normal reversible repository work and already-configured GitHub services are allowed. A new paid-provider request, target-repository write, production release/deployment, credential change, protected force-push, irreversible destruction, or materially broader scope requires current explicit authority. Do not weaken security, verification, audit, compatibility, recovery, or rollback to make work pass.

The product's runtime, scheduler, policy, persistence, provider effects, research acceptance, and release boundaries remain owned by the Rust engine and `docs/ARCHITECTURE.md`. A repository change or a passing test does not authorize those effects.

## Verification and evidence

Run focused checks for the changed behavior, then every applicable check in `AGENTS.md`'s Verification Baseline and the canonical CI matrix. Add negative tests for relevant rejection and recovery paths. `git diff --check`, the security baseline, agent handoff, and wire-codegen drift checks remain required where applicable.

Report local tests, hosted CI, independent review, merge/readback, and any external effect as separate evidence. A passing test proves only its assertions. Never infer success from a command being started, a capsule, aggregate approval, a worker's self-report, or a local branch that was not read back from GitHub.

## Review Convergence Protocol

Review the complete `base...head` diff on one stable Draft candidate. Standards and Spec are independent axes. Review receipts must bind the repository, base, exact head, reviewer identity/session, outcome, and unresolved blockers. A later head invalidates earlier review conclusions.

```text
stable Draft candidate + local checks passed
        ↓
R1: independent full-diff review
     ├─ no open blockers → exact PASS (deferred notes permitted)
     └─ blockers → one bounded repair batch
                ↓
R2: independent verification of the complete current diff
     ├─ no open blockers → exact PASS
     └─ blockers remain → stop and report the exact blocker and decision needed
```

- At most two substantive review rounds and one autonomous repair batch per candidate head.
- Severity and disposition are separate. Only an open `block_current_head` finding blocks; a documented deferred note may remain on a `PASS` receipt.
- Exact `PASS` is the only review verdict eligible for merge. It is not a substitute for tests, CI, or delivery readback.
- Review-state projections and generated capsules do not decide severity, disposition, or acceptance.

The canonical normalization and round budget live in `scripts/agent-control/review_convergence.py`; this document owns the procedure and gate.

## Exact-head CI and guarded merge

All review receipts, required checks, and merge authorization must identify the same exact head SHA. The canonical matrix is:

- `exact-head-check`
- `rust-tests`
- `pg-integration-tests`
- `typescript-tests`
- `native-runtime`
- `docker-build`
- `python-tests`
- `rust-typescript-cutover`
- `context-capsule`

Keep a changing PR Draft. Mark it Ready only after local verification and complete-diff independent review. Merge only through `.github/workflows/agent-merge.yml` after the branch ruleset, current exact-head CI, exact `PASS`, zero open blockers, and squash-merge eligibility are confirmed. Direct `gh pr merge` is not the repository merge path.

After merge, read the PR and `main` from GitHub and verify the merged PR number, expected head, and exact merge commit. A successful workflow dispatch or local `HEAD` alone is not proof of merge.

## Recovery and rollback

Preserve the branch and evidence when a check, review, or merge operation fails. A changed head requires fresh review and CI. If a merge dispatch or other external mutation has an unknown outcome, do not retry it; reconcile its exact remote identity read-only using the recovery contract in `docs/ARCHITECTURE.md`.

Every change needs a recoverable Git point. Prefer a focused revert of the exact accepted commit. Do not overwrite unrelated work or remove a rollback path without a tested replacement. Stop when resolution requires new authority, an unprovable external-effect status, or a decision outside the user's accepted scope; report evidence, consequence, and the smallest decision needed.
