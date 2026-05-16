# Stage 1 Week 3 Acceptance Report

## 1. Acceptance Summary

Stage 1 Week 3 is accepted.

- Test result: `PYTHONPATH=src python3 -m unittest discover -s tests` passed with 83 tests.
- Branch at acceptance: `stage1-week3`.
- Working tree state before acceptance report commit: clean except this report.
- Scope: Kernel and BatchRunner skeleton only.

## 2. Components Implemented

### Kernel

- Validates event logs with replay preflight before projection or append.
- Replays project state, task queue state, and full projection bundles.
- Appends project events through `EventStore`.
- Rejects protected source and fixture event logs as append targets.
- Keeps behavior deterministic and local.

### BatchRunner

- Lists ready project items from projection state.
- Excludes items that already have handoff records.
- Runs one deterministic ready-item step.
- Pre-builds and validates all planned events before appending.
- Appends `ready -> running`, handoff creation, and `running -> review` events.
- Generates a post-run digest from updated projections.

## 3. Commits

- `056b1b2` Plan Stage 1 Week 3 kernel runner
- `ad75493` Implement Stage 1 Week 3 kernel skeleton
- `834f17d` Implement Stage 1 Week 3 batch runner skeleton
- `6059735` Harden Stage 1 Week 3 batch runner projection test

## 4. Test Summary

- Total tests: 83
- Kernel rejects the preserved bad line 17 fixture.
- Kernel accepts the sanitized fixture and projects five done items.
- Kernel append uses temporary event logs and preserves JSONL validity.
- Protected source and fixture event logs cannot be appended to.
- BatchRunner lists ready items, excludes handed-off items, and rejects invalid logs.
- BatchRunner appends the planned running, handoff, and review event sequence.
- BatchRunner validates all planned events before append.
- Post-run projection confirms the item status is `review`.

## 5. Scope Boundaries Preserved

- No modification to `docs/stage0/events.jsonl`.
- No CLI mutation command.
- No model calls.
- No real agents.
- No Web UI.
- No provider failover.
- No concurrency.
- No dynamic DAG mutation.
- No sandbox execution.
- No arbitrary shell command execution from tasks.
- No Stage 2 expansion.

## 6. Known Gaps Not To Fix Yet

- No CLI `run-one` command.
- No real worker execution.
- No sandbox.
- No model calls.
- No transaction rollback beyond prevalidation.
- No persistent projection store.
- No real scheduler/concurrency.

## 7. Recommendation

Week 3 is accepted.

Week 4 should focus on Task Record integration and a Final Gate runtime skeleton. It should not add agents, model calls, shell command execution, sandbox execution, Web UI, provider failover, concurrency, or Stage 2 behavior.
