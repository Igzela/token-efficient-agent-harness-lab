# Real-World Eval Fixture Expansion Report

## Purpose

This expands the post-closeout Real-World Read-Only Evaluation Track for the
completed Stage 0-4 harness. It is not Stage 5.

The expansion adds synthetic but real-world-like copied fixtures and read-only
tests that exercise existing validators, projections, digest generation, task
record loading, final gate evaluation, artifact checks, deterministic scoring,
quality gates, and quality digest generation.

## Fixtures Added

| Fixture | Path | Purpose |
| --- | --- | --- |
| `doc_update` | `tests/fixtures/real_world_eval/doc-update-project/` | Documentation-only task shape with README-shaped artifact evidence. |
| `bugfix` | `tests/fixtures/real_world_eval/bugfix-project/` | Bugfix task shape with patch-shaped evidence, regression notes, artifact refs, and scoring. |
| `config_rule` | `tests/fixtures/real_world_eval/config-rule-project/` | Config/rule-change task shape with allowed and forbidden file policy evidence. |
| `failure_fix_loop` | `tests/fixtures/real_world_eval/failure-fix-loop-project/` | Retry/fix-loop evidence with canonical `failure_code`, freeform `failure_subcode`, and advisor-like records. |
| `cross_task_dependency` | `tests/fixtures/real_world_eval/cross-task-dependency-project/` | Multi-item project event stream with dependency resolution and quality digest coverage across related tasks. |

The original first-pass `project-alpha` fixture remains covered.

## What Each Fixture Validates

- `doc_update`: project event replay, batch digest, task bundle validation,
  final gate, artifact gate, scoring, and quality gate for documentation-only
  work.
- `bugfix`: task bundle loading, final gate pass, artifact existence checks,
  patch-shaped evidence, and deterministic scoring.
- `config_rule`: file policy representation through `allowed_files`,
  `forbidden_files`, allowed-files completeness, artifact gate, and scoring.
- `failure_fix_loop`: canonical failure code acceptance, freeform subcode
  tolerance, advisor-protocol event validation, and existing scoring penalty
  behavior.
- `cross_task_dependency`: multi-item projection replay, handoff projection,
  dependency resolution projection, batch digest, run scoring, and quality
  digest generation.

## APIs Exercised

- `validate_replay_preflight_check`
- `replay_all`
- `generate_batch_digest`
- `TaskRecordStore`
- `FinalGateRunner`
- `ArtifactGate`
- `ScoringEngine`
- `QualityGateManager`
- `QualityDigestGenerator`
- `validate_allowed_files_completeness`
- `validate_failure_code`
- `validate_advisor_protocol_events`

## Safety Boundaries Preserved

- No model calls.
- No model API integration.
- No real agents.
- No sandbox execution.
- No task execution.
- No real external project mutation.
- No dependency installation.
- No Web UI.
- No provider failover.
- No runtime module changes.
- No `docs/stage0/events.jsonl` modification.
- No Stage 5 work started.

## Test Result

Command:

```bash
PYTHONPATH=src python3 -m unittest discover -s tests
```

Result after fixture expansion: 350 tests pass.

## Known Gaps

- Fixtures are synthetic and realistic in shape, not live production projects.
- No real provider integration is included.
- No task execution is included.
- No sandbox execution is included.
- No external issue tracker or project checkout is accessed.

## Recommended Next Step

If a human provides approved copied project fixtures, add 2-3 actual sanitized
fixtures under `tests/fixtures/real_world_eval/` and run the same read-only API
path.

After that, an Advisor-only real model test could be considered as a separate
explicitly approved track. It should remain outside this fixture expansion and
must not be treated as Stage 5.
