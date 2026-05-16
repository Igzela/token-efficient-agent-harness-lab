# Stage 4 Sandbox and Concurrency Spec

## Purpose

Stage 4 uses a sandbox abstraction for file claims and a concurrency controller for scheduling decisions. These components do not run processes, create containers, create VMs, or spawn concurrent workers. They produce deterministic, auditable control data that later stages may use.

## Sandbox Manager

`SandboxManager` tracks logical work areas and write ownership.

### SandboxHandle

A sandbox handle identifies a logical sandbox:

- `sandbox_id`
- `task_id`
- `status`: `created`, `active`, `released`, or `failed`
- `claimed_files`
- `created_at`
- `released_at`

The implementation may call this record `Sandbox`, but the handle semantics are the same.

### WriteClaim

A write claim records one claimed file path:

- `claim_id`
- `sandbox_id`
- `file_path`
- `claimed_at`
- `released`

The implementation may call this record `FileClaim`; it must behave as an auditable write claim.

### ConflictDetection

Conflict detection returns a structured report:

- whether a conflict exists
- conflicting sandbox id, if any
- conflicting file, if any
- deterministic message

### API

Required behavior:

- `create`: create a logical sandbox handle and optionally acquire initial file claims.
- `claim_files`: claim one or more files atomically; if any requested file conflicts, no partial claim is made.
- `release_claims`: release claims for a sandbox without deleting the historical claim records.
- `detect_conflicts`: determine whether requested files overlap active claims.
- `export_artifacts`: produce descriptive artifact references only; no promotion outside approved temp/test locations.
- `cleanup`: mark sandbox resources released/cleaned in records; do not perform destructive filesystem cleanup outside controlled temp dirs.

Stage 4 may expose narrower method names such as `create_sandbox`, `release_sandbox`, and `is_file_claimed` if they preserve these behaviors.

## Sandbox Rules

- No real process, container, VM, chroot, namespace, or filesystem sandboxing.
- File isolation is claim tracking only.
- Active write claims conflict when the same file path is requested by another sandbox.
- Same-sandbox duplicate claims are idempotent.
- Released claims no longer block future claims.
- Tests may use `tempfile.TemporaryDirectory` only.
- Every state-changing sandbox operation must be representable as an auditable event or record.

## Concurrency Controller

`ConcurrencyController` creates scheduling batches. It does not execute work.

### ScheduleBatch

`ScheduleBatch` describes the selected runnable group:

- scheduled items
- blocked items
- detected file overlaps
- deterministic warnings

### FileOverlap

`FileOverlap` records a conflict between two items:

- item A id
- item B id
- overlapping files

### API

Required behavior:

- `schedule(ready_items, dag, active_claims) -> ScheduleBatch`
- `detect_file_overlaps(items) -> tuple[FileOverlap, ...]`
- `can_run_parallel(item_a, item_b, overlaps) -> bool`

## Scheduling Rules

- Default `max_concurrent` is 4.
- `max_concurrent` is an upper bound on scheduled items in a single batch.
- File overlap blocks parallel grouping when either side writes the overlapping file.
- Active write claims block conflicting scheduled items.
- Hard dependencies block downstream scheduling until the upstream node is completed or the edge is satisfied.
- Artifact dependencies block scheduling until the required artifact condition is verified or promoted.
- Soft dependencies do not block scheduling.
- The controller must not enforce a global single-builder limit unless actual conflicts require it.
- Empty ready lists return an empty batch.

## No Real Concurrency

The controller returns deterministic schedule data. It must not:

- start threads
- start processes
- use async worker pools
- execute task commands
- claim to have completed work

## Determinism

- Items are considered in sorted id order.
- Overlap pairs are sorted deterministically.
- Scheduling decisions are reproducible for the same DAG, ready items, active claims, and `max_concurrent`.
