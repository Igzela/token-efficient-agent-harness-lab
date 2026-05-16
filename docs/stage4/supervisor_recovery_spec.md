# Stage 4 Supervisor and Recovery Spec

## Purpose

Runtime supervision in Stage 4 is data-driven. It summarizes supplied worker health records, detects stuck or crashed workers deterministically, coordinates checkpoints, and creates descriptive recovery plans. It does not monitor real OS processes, kill workers, restart workers, or execute recovery actions.

## RuntimeSupervisor

`RuntimeSupervisor` coordinates task checkpoint state and supplied health records.

Required behavior:

- Start a task by writing a running checkpoint.
- Record step checkpoints.
- Mark a task completed or failed through checkpoint records.
- Return task status from latest checkpoint.
- Create recovery plans through `CheckpointManager`.
- Assess supplied `WorkerHealth` records using supplied timestamps.

## Health Records

### WorkerHealth

`WorkerHealth` describes a worker as reported by tests or callers:

- `worker_id`
- `task_id`
- `status`: `idle`, `running`, `completed`, `failed`, or `crashed`
- `last_heartbeat`
- `started_at`
- `error`

### ComponentHealth

`ComponentHealth` summarizes one component:

- `component_id`
- `status`: `healthy`, `degraded`, or `failed`
- `message`
- `checked_at`

### SupervisorReport

`SupervisorReport` aggregates runtime health:

- `checked_at`
- `healthy`
- `stuck_workers`
- `crashed_workers`
- `component_health`
- `recovery_plans`

## Health Checks

Health is based only on supplied records. Stage 4 must not poll processes or inspect live worker state.

Stuck detection:

- A worker is stuck when `status == "running"` and `now - last_heartbeat` exceeds the configured heartbeat timeout.
- `now` is supplied by tests/callers, not read from wall-clock time.

Crashed worker recovery:

- A worker with `status == "crashed"` or `status == "failed"` is reported as crashed.
- Recovery output is a descriptive action plan, not execution.

## Event Log Integrity Checks

Checkpoint/recovery code may run preflight checks against event logs using existing Stage 1 validation:

- JSONL must be valid.
- Event ids must be unique.
- Required schema fields must exist.
- Replay preflight errors block replay-dependent recovery decisions.

Projection consistency checks may use existing Stage 1 projection replay. Unsupported event types may produce warnings but must not mutate the event log.

## CheckpointManager

`CheckpointManager` persists deterministic JSON checkpoint files in caller-provided directories, typically temp dirs in tests.

Required behavior:

- `save_checkpoint`
- `load_checkpoint`
- `latest_checkpoint`
- `list_checkpoints`
- `create_recovery_plan`
- `check_event_log_integrity`
- `check_projection_consistency`

## Checkpoint

`Checkpoint` is a mid-execution snapshot:

- `checkpoint_id`
- `task_id`
- `node_id`
- `dag_version`
- `status`: `running`, `completed`, or `failed`
- `current_step`
- `completed_steps`
- `pending_steps`
- `input_hash`
- `artifact_refs`
- `model_call_refs`
- `tool_call_refs`
- `resumable`
- `resume_strategy`
- `created_at`
- optional failure reason

## Checkpoint vs Run Log

A checkpoint is not the canonical run log.

- Checkpoints support recovery by capturing the latest resumable task state.
- Run logs/events remain append-only audit records.
- Checkpoint overwrite idempotency is allowed for identical checkpoint ids/content.
- Recovery must not delete or rewrite run logs or event history.

## RecoveryPlan

`RecoveryPlan` is descriptive only:

- `task_id`
- `checkpoint_id`
- `strategy`: `resume`, `restart`, `skip`, or `compensate`
- `compensating_events`
- `resumed_from_step`
- `warnings`

Strategy rules:

- No checkpoint: `skip`
- Latest checkpoint `running` and resumable: `resume`
- Latest checkpoint `running` and not resumable: `restart`
- Latest checkpoint `failed`: `compensate`
- Latest checkpoint `completed`: `skip`

Compensating events are forward-only descriptions such as `task_cancelled` or `claim_released`. They do not perform process recovery.

## Determinism

- Checkpoint JSON is sorted and newline-terminated.
- Latest checkpoint lookup sorts deterministically.
- Recovery plans are deterministic for the same checkpoint set.
- No wall-clock reads, network calls, model calls, process inspection, or process execution.
