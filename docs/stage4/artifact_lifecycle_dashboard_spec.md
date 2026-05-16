# Stage 4 Artifact Lifecycle, Health, and Dashboard Spec

## Purpose

Stage 4 records artifact lifecycle state, unlocks dependencies when artifacts are verified or promoted, aggregates runtime health, and builds a dashboard data model. This is planning/data-model work only and does not implement a Web UI or move artifacts through production systems.

## ArtifactLifecycleManager

`ArtifactLifecycleManager` tracks artifact records and transition records in memory or in a temp/event-backed store. It does not perform real artifact promotion outside approved temp test locations.

Required behavior:

- Produce artifact records.
- Verify produced artifacts.
- Reject produced artifacts.
- Promote verified artifacts.
- Archive promoted artifacts.
- Reject invalid transitions.
- Return dependency unlock records only when required artifacts are verified or promoted.

## ArtifactRecord

`ArtifactRecord` describes one artifact:

- `artifact_id`
- `task_id`
- `artifact_type`
- `path`
- `sha256`
- `status`
- `created_at`
- `updated_at`
- `metadata`

Allowed statuses:

- `draft`
- `produced`
- `verified`
- `rejected`
- `promoted`
- `archived`

## ArtifactTransition

`ArtifactTransition` is the audit record for state movement:

- `artifact_id`
- `from_status`
- `to_status`
- `timestamp`
- `reason`

Allowed transitions:

- `draft -> produced`
- `produced -> verified`
- `produced -> rejected`
- `verified -> promoted`
- `promoted -> archived`

No other transitions are valid in Stage 4.

## DependencyUnlock

`DependencyUnlock` describes whether an artifact dependency is now satisfied:

- `artifact_id`
- `dependency_id`
- `unlocked`
- `reason`

Dependencies unlock only when the artifact status is `verified` or `promoted`. `produced`, `rejected`, `archived`, and missing artifacts do not unlock downstream work.

## Integration With Stage 2 Artifact Gate

Stage 2 Artifact Gate remains the validation boundary for artifact quality checks. Stage 4 lifecycle records should consume gate outcomes as inputs:

- passing gate result -> artifact may transition from `produced` to `verified`
- failing gate result -> artifact may transition from `produced` to `rejected`

Stage 4 must not broaden Stage 2 validation semantics or bypass the existing gate.

## HealthMonitor and HealthReport

`HealthMonitor` aggregates supplied component health from Stage 4 abstractions.

`HealthReport` includes:

- `checked_at`
- `overall_status`: `healthy`, `degraded`, or `failed`
- `components`
- `warnings`

Aggregation rules:

- Any failed component makes the overall status `failed`.
- Otherwise, any degraded component makes the overall status `degraded`.
- All healthy components make the overall status `healthy`.
- The monitor does not poll real processes or perform live infrastructure checks.

## DashboardSnapshot

`DashboardSnapshot` is a read-only data model for future UI consumption.

It may include:

- generation timestamp
- DAG id and version
- node and edge counts
- supervisor report
- health report
- artifact records
- recovery plans
- schedule batches

## No UI Implementation

Stage 4 does not build a dashboard UI, frontend route, server endpoint, or browser experience. It only defines and tests the data model that a later stage may render.

## Determinism

- Artifact lists are sorted by id where ordering matters.
- Health aggregation depends only on supplied records.
- Dashboard snapshots are built from explicit inputs.
- No filesystem promotion, model calls, network calls, process calls, real workers, or Web UI behavior.
