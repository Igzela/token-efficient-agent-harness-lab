#![allow(dead_code)]

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Dialect {
    Sqlite,
    Postgres,
}

pub(super) const CURRENT_SQLITE_SCHEMA_VERSION: i64 = 25;
pub(super) const CURRENT_POSTGRES_SCHEMA_VERSION: i64 = 25;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct SchemaMigration {
    pub version: i64,
    pub description: &'static str,
}

pub(super) const SQLITE_MIGRATIONS: &[SchemaMigration] = &[
    SchemaMigration {
        version: 1,
        description: "add last_used_at and expires_at to api_key_metadata",
    },
    SchemaMigration {
        version: 2,
        description: "add inert workflow run state tables",
    },
    SchemaMigration {
        version: 3,
        description: "add supervised patch metadata tables",
    },
    SchemaMigration {
        version: 4,
        description: "add scheduler feedback table for feedback-driven routing",
    },
    SchemaMigration {
        version: 5,
        description: "add agent_profiles table and profile_id column on workflow_run_nodes",
    },
    SchemaMigration {
        version: 6,
        description: "add tool_capabilities, tool_allowlists, and tool_hooks tables",
    },
    SchemaMigration {
        version: 7,
        description: "add orchestration_decisions table for policy decision trace",
    },
    SchemaMigration {
        version: 8,
        description: "add executor_pool table for resource/executor pool tracking",
    },
    SchemaMigration {
        version: 9,
        description: "add queue/priority/backpressure columns to workflow_runs",
    },
    SchemaMigration {
        version: 10,
        description: "add policy signal columns to orchestration_decisions",
    },
    SchemaMigration {
        version: 11,
        description: "add scheduler_heartbeat table for persistent heartbeat",
    },
    SchemaMigration {
        version: 12,
        description: "add controlled loop policy proposal table",
    },
    SchemaMigration {
        version: 13,
        description: "add controlled loop policy snapshot table",
    },
    SchemaMigration {
        version: 14,
        description: "add agent_state and agent_mailbox tables",
    },
    SchemaMigration {
        version: 15,
        description: "add agent_proposals table for AR-3 child task proposals and handoff",
    },
    SchemaMigration {
        version: 16,
        description: "add native scorecard artifact evidence table",
    },
    SchemaMigration {
        version: 17,
        description: "add token-efficiency regression report artifact table",
    },
    SchemaMigration {
        version: 18,
        description: "add immutable budget evidence artifact table",
    },
    SchemaMigration {
        version: 19,
        description: "add policy-gated budget auto-pause decisions",
    },
    SchemaMigration {
        version: 20,
        description: "add offline replay evidence artifacts",
    },
    SchemaMigration {
        version: 21,
        description: "bind dispatch history to immutable recorder-owned trace hashes",
    },
    SchemaMigration {
        version: 22,
        description: "add agent action receipts and authoritative tool policy state",
    },
    SchemaMigration {
        version: 23,
        description: "add durable memory, retrieval evidence, and production job ownership",
    },
    SchemaMigration {
        version: 24,
        description: "add scoped external runtime checkpoints and invocation receipts",
    },
    SchemaMigration {
        version: 25,
        description:
            "bind provider embedding identity, pricing, and restart-safe operation receipts",
    },
];

pub(super) const POSTGRES_MIGRATIONS: &[SchemaMigration] = SQLITE_MIGRATIONS;

pub(super) fn ddl_for(dialect: Dialect) -> &'static str {
    match dialect {
        Dialect::Sqlite => SQLITE_DDL,
        Dialect::Postgres => POSTGRES_DDL,
    }
}

pub(super) const V23_DDL: &str = "
CREATE TABLE IF NOT EXISTS durable_memory_versions (
    memory_id TEXT NOT NULL,
    version BIGINT NOT NULL,
    tenant_id TEXT NOT NULL,
    workspace_id TEXT NOT NULL,
    agent_id TEXT,
    run_id TEXT,
    task_id TEXT,
    source_id TEXT NOT NULL,
    source_sha256 TEXT NOT NULL CHECK (length(source_sha256) = 64),
    conflict_key TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('current','superseded','conflicting','invalid','tombstoned','expired')),
    confidence DOUBLE PRECISION NOT NULL,
    fresh_until TEXT,
    expires_at TEXT,
    supersedes_memory_id TEXT,
    content_json TEXT NOT NULL,
    embedding_json TEXT,
    embedding_provenance TEXT NOT NULL,
    record_sha256 TEXT NOT NULL CHECK (length(record_sha256) = 64),
    created_at TEXT NOT NULL,
    created_by TEXT NOT NULL,
    PRIMARY KEY (memory_id, version)
);
CREATE INDEX IF NOT EXISTS idx_durable_memory_scope
    ON durable_memory_versions(tenant_id, workspace_id, agent_id, task_id, state);
CREATE INDEX IF NOT EXISTS idx_durable_memory_source
    ON durable_memory_versions(source_id, source_sha256);
CREATE INDEX IF NOT EXISTS idx_durable_memory_conflict
    ON durable_memory_versions(tenant_id, workspace_id, conflict_key, state);

CREATE TABLE IF NOT EXISTS memory_retrieval_events (
    retrieval_id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    workspace_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    node_id TEXT NOT NULL,
    agent_id TEXT,
    request_sha256 TEXT NOT NULL CHECK (length(request_sha256) = 64),
    result_sha256 TEXT NOT NULL CHECK (length(result_sha256) = 64),
    mode TEXT NOT NULL,
    candidate_count BIGINT NOT NULL,
    selected_count BIGINT NOT NULL,
    estimated_tokens BIGINT NOT NULL,
    read_bytes BIGINT NOT NULL,
    truncated BIGINT NOT NULL,
    evidence_json TEXT NOT NULL,
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_memory_retrieval_run
    ON memory_retrieval_events(run_id, node_id, created_at);
CREATE INDEX IF NOT EXISTS idx_memory_retrieval_scope
    ON memory_retrieval_events(tenant_id, workspace_id, created_at);

CREATE TABLE IF NOT EXISTS production_jobs (
    job_key TEXT PRIMARY KEY,
    job_kind TEXT NOT NULL,
    scope_sha256 TEXT NOT NULL CHECK (length(scope_sha256) = 64),
    input_sha256 TEXT NOT NULL CHECK (length(input_sha256) = 64),
    state TEXT NOT NULL CHECK (state IN ('running','completed','failed')),
    lease_owner TEXT,
    lease_token TEXT,
    lease_expires_at TEXT,
    result_json TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_production_jobs_kind
    ON production_jobs(job_kind, state, updated_at);

CREATE TABLE IF NOT EXISTS normalized_usage_observations (
    observation_id TEXT PRIMARY KEY,
    dedupe_key TEXT NOT NULL UNIQUE,
    source_kind TEXT NOT NULL,
    source_id TEXT NOT NULL,
    source_sha256 TEXT NOT NULL CHECK (length(source_sha256) = 64),
    occurred_at TEXT NOT NULL,
    tenant_id TEXT NOT NULL,
    workspace_id TEXT,
    run_id TEXT,
    dispatch_id TEXT,
    provider_id TEXT,
    model_id TEXT,
    pricing_identity TEXT,
    pricing_effective_date TEXT,
    currency TEXT,
    provenance_json TEXT NOT NULL,
    usage_json TEXT NOT NULL,
    completeness TEXT NOT NULL,
    confidence DOUBLE PRECISION NOT NULL,
    record_sha256 TEXT NOT NULL CHECK (length(record_sha256) = 64),
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_normalized_usage_run
    ON normalized_usage_observations(run_id, occurred_at, observation_id);
CREATE INDEX IF NOT EXISTS idx_normalized_usage_dispatch
    ON normalized_usage_observations(dispatch_id, occurred_at, observation_id);

CREATE TABLE IF NOT EXISTS replay_producer_bindings (
    artifact_id TEXT PRIMARY KEY,
    input_sha256 TEXT NOT NULL CHECK (length(input_sha256) = 64),
    dispatch_ids_json TEXT NOT NULL,
    maximum_trace_age_seconds BIGINT NOT NULL,
    scope_json TEXT NOT NULL,
    current_policy_json TEXT NOT NULL,
    candidate_policies_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    created_by TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_replay_producer_bindings_created
    ON replay_producer_bindings(created_at, artifact_id);

CREATE TABLE IF NOT EXISTS operator_acknowledgements (
    acknowledgement_id TEXT PRIMARY KEY,
    decision_id TEXT NOT NULL,
    source_type TEXT NOT NULL,
    source_id TEXT NOT NULL,
    source_sha256 TEXT NOT NULL CHECK (length(source_sha256) = 64),
    reason TEXT,
    actor TEXT NOT NULL,
    created_at TEXT NOT NULL,
    UNIQUE(decision_id, source_type, source_id, source_sha256)
);
CREATE INDEX IF NOT EXISTS idx_operator_acknowledgements_source
    ON operator_acknowledgements(source_type, source_id, created_at);
";

pub(super) const V24_DDL: &str = "
CREATE TABLE IF NOT EXISTS external_runtime_checkpoints (
    checkpoint_id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    workspace_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    node_id TEXT NOT NULL,
    thread_id TEXT NOT NULL,
    runtime_kind TEXT NOT NULL CHECK (runtime_kind = 'langgraph'),
    adapter_version TEXT NOT NULL,
    runtime_version TEXT NOT NULL,
    memory_strategy TEXT NOT NULL,
    checkpoint_summary_json TEXT NOT NULL,
    state_sha256 TEXT NOT NULL CHECK (length(state_sha256) = 64),
    status TEXT NOT NULL CHECK (status IN ('active','completed','tombstoned')),
    version BIGINT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (tenant_id, workspace_id, run_id, node_id, thread_id)
);
CREATE INDEX IF NOT EXISTS idx_external_runtime_checkpoints_scope
    ON external_runtime_checkpoints(tenant_id, workspace_id, run_id, node_id, updated_at);

CREATE TABLE IF NOT EXISTS external_runtime_invocations (
    invocation_id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    workspace_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    node_id TEXT NOT NULL,
    thread_id TEXT NOT NULL,
    idempotency_sha256 TEXT NOT NULL CHECK (length(idempotency_sha256) = 64),
    checkpoint_id TEXT NOT NULL,
    lease_token TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('started','completed','failed','blocked')),
    result_summary_json TEXT,
    artifact_id TEXT,
    failure_code TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    completed_at TEXT,
    UNIQUE (tenant_id, workspace_id, run_id, node_id, idempotency_sha256)
);
CREATE INDEX IF NOT EXISTS idx_external_runtime_invocations_scope
    ON external_runtime_invocations(tenant_id, workspace_id, run_id, node_id, updated_at);
";

pub(super) const V25_DDL: &str = "
ALTER TABLE durable_memory_versions ADD COLUMN embedding_metadata_json TEXT;
ALTER TABLE durable_memory_versions ADD COLUMN embedding_binding_sha256 TEXT;
CREATE TABLE IF NOT EXISTS provider_embedding_operations (
    operation_id TEXT PRIMARY KEY,
    target_memory_id TEXT NOT NULL,
    target_version BIGINT NOT NULL,
    operation_binding_sha256 TEXT NOT NULL CHECK (length(operation_binding_sha256) = 64),
    content_sha256 TEXT NOT NULL CHECK (length(content_sha256) = 64),
    receipt_sha256 TEXT NOT NULL CHECK (length(receipt_sha256) = 64),
    provider_id TEXT NOT NULL,
    model_id TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('request_sent','completed','failed','outcome_unknown')),
    vector_json TEXT,
    metadata_json TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (target_memory_id, target_version)
);
CREATE INDEX IF NOT EXISTS idx_provider_embedding_operations_state
    ON provider_embedding_operations(state, updated_at);
";

pub(super) const SQLITE_DDL: &str = "
CREATE TABLE IF NOT EXISTS dispatch_history (
    history_id INTEGER PRIMARY KEY AUTOINCREMENT,
    dispatch_id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    raw_request TEXT NOT NULL,
    request_source TEXT NOT NULL,
    final_status TEXT NOT NULL,
    selected_tier TEXT NOT NULL,
    risk_level TEXT NOT NULL,
    reserved_cost REAL NOT NULL DEFAULT 0.0,
    bundle_json TEXT NOT NULL,
    input_tokens INTEGER,
    output_tokens INTEGER,
    estimated_cost_usd REAL,
    executor_type TEXT NOT NULL DEFAULT 'noop',
    latency_ms INTEGER,
    trace_schema_version TEXT,
    trace_content_sha256 TEXT
);
CREATE INDEX IF NOT EXISTS idx_dispatch_history_created ON dispatch_history(created_at);
CREATE INDEX IF NOT EXISTS idx_dispatch_history_dispatch_id ON dispatch_history(dispatch_id);

CREATE TABLE IF NOT EXISTS local_config (
    key TEXT PRIMARY KEY,
    value_json TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    updated_by TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS team_members (
    user_id TEXT PRIMARY KEY,
    display_name TEXT NOT NULL,
    role TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS api_key_metadata (
    key_id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    role TEXT NOT NULL,
    scopes_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    created_by TEXT NOT NULL,
    revoked_at TEXT,
    last_used_at TEXT,
    expires_at TEXT
);

CREATE TABLE IF NOT EXISTS audit_log (
    audit_id INTEGER PRIMARY KEY AUTOINCREMENT,
    created_at TEXT NOT NULL,
    actor TEXT NOT NULL,
    action TEXT NOT NULL,
    resource TEXT NOT NULL,
    details_json TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_audit_created ON audit_log(created_at);

CREATE TABLE IF NOT EXISTS provider_audit_events (
    event_id TEXT PRIMARY KEY,
    dispatch_id TEXT NOT NULL,
    provider_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    input_token_count INTEGER,
    output_token_count INTEGER,
    cost REAL,
    currency TEXT,
    latency_ms INTEGER,
    error_domain TEXT,
    redaction_status TEXT NOT NULL,
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_provider_audit_dispatch ON provider_audit_events(dispatch_id);
CREATE INDEX IF NOT EXISTS idx_provider_audit_created ON provider_audit_events(created_at);

CREATE TABLE IF NOT EXISTS workflow_plans (
    plan_sequence INTEGER PRIMARY KEY,
    plan_id TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    raw_request TEXT NOT NULL,
    request_source TEXT NOT NULL,
    status TEXT NOT NULL,
    workflow_id TEXT NOT NULL,
    dispatch_id TEXT NOT NULL,
    graph_json TEXT NOT NULL,
    analysis_json TEXT NOT NULL,
    boundaries_json TEXT NOT NULL,
    plan_json TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_workflow_plans_created ON workflow_plans(created_at);
CREATE INDEX IF NOT EXISTS idx_workflow_plans_status ON workflow_plans(status);

CREATE TABLE IF NOT EXISTS workflow_runs (
    run_sequence INTEGER PRIMARY KEY,
    run_id TEXT NOT NULL UNIQUE,
    plan_id TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    status TEXT NOT NULL,
    workflow_id TEXT NOT NULL,
    dispatch_id TEXT,
    started_at TEXT,
    completed_at TEXT,
    last_heartbeat_at TEXT,
    result_json TEXT,
    boundaries_json TEXT NOT NULL,
    run_json TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_workflow_runs_created ON workflow_runs(created_at);
CREATE INDEX IF NOT EXISTS idx_workflow_runs_status ON workflow_runs(status);
CREATE INDEX IF NOT EXISTS idx_workflow_runs_plan ON workflow_runs(plan_id);

CREATE TABLE IF NOT EXISTS workflow_run_nodes (
    run_id TEXT NOT NULL,
    node_id TEXT NOT NULL,
    task_type TEXT NOT NULL,
    status TEXT NOT NULL,
    node_json TEXT NOT NULL,
    started_at TEXT,
    completed_at TEXT,
    attempt_count INTEGER NOT NULL DEFAULT 0,
    timeout_ms INTEGER,
    blocked_reason TEXT,
    leased_at TEXT,
    profile_id TEXT,
    PRIMARY KEY (run_id, node_id)
);
CREATE INDEX IF NOT EXISTS idx_workflow_run_nodes_run ON workflow_run_nodes(run_id);

CREATE TABLE IF NOT EXISTS workflow_run_edges (
    run_id TEXT NOT NULL,
    edge_id TEXT NOT NULL,
    from_node_id TEXT NOT NULL,
    to_node_id TEXT NOT NULL,
    edge_type TEXT NOT NULL,
    edge_json TEXT NOT NULL,
    PRIMARY KEY (run_id, edge_id)
);
CREATE INDEX IF NOT EXISTS idx_workflow_run_edges_run ON workflow_run_edges(run_id);

CREATE TABLE IF NOT EXISTS workflow_run_events (
    event_sequence INTEGER PRIMARY KEY,
    event_id TEXT NOT NULL UNIQUE,
    run_id TEXT NOT NULL,
    node_id TEXT,
    event_type TEXT NOT NULL,
    actor TEXT NOT NULL,
    created_at TEXT NOT NULL,
    details_json TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_workflow_run_events_run ON workflow_run_events(run_id);
CREATE INDEX IF NOT EXISTS idx_workflow_run_events_created ON workflow_run_events(created_at);

CREATE TABLE IF NOT EXISTS workflow_run_approvals (
    approval_sequence INTEGER PRIMARY KEY,
    approval_id TEXT NOT NULL UNIQUE,
    run_id TEXT NOT NULL,
    node_id TEXT NOT NULL,
    decision TEXT NOT NULL,
    actor TEXT NOT NULL,
    reason TEXT,
    created_at TEXT NOT NULL,
    approval_json TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_workflow_run_approvals_run ON workflow_run_approvals(run_id);

CREATE TABLE IF NOT EXISTS supervised_patch_workspaces (
    workspace_sequence INTEGER PRIMARY KEY,
    workspace_id TEXT NOT NULL UNIQUE,
    plan_id TEXT,
    run_id TEXT NOT NULL,
    target_id TEXT NOT NULL,
    target_repo_path TEXT NOT NULL,
    target_repo_canonical_path TEXT NOT NULL,
    workspace_path TEXT NOT NULL,
    workspace_canonical_path TEXT NOT NULL,
    source_revision TEXT NOT NULL,
    source_tree_hash TEXT,
    status TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    boundary_json TEXT NOT NULL,
    workspace_json TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_supervised_patch_workspaces_run ON supervised_patch_workspaces(run_id);
CREATE INDEX IF NOT EXISTS idx_supervised_patch_workspaces_target ON supervised_patch_workspaces(target_id, source_revision);
CREATE INDEX IF NOT EXISTS idx_supervised_patch_workspaces_status ON supervised_patch_workspaces(status);

CREATE TABLE IF NOT EXISTS supervised_patch_artifacts (
    artifact_sequence INTEGER PRIMARY KEY,
    artifact_id TEXT NOT NULL UNIQUE,
    workspace_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    plan_id TEXT,
    target_id TEXT NOT NULL,
    source_revision TEXT NOT NULL,
    artifact_type TEXT NOT NULL,
    patch_hash TEXT NOT NULL,
    changed_files_json TEXT NOT NULL,
    redaction_status TEXT NOT NULL,
    created_at TEXT NOT NULL,
    artifact_json TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_supervised_patch_artifacts_workspace ON supervised_patch_artifacts(workspace_id);
CREATE INDEX IF NOT EXISTS idx_supervised_patch_artifacts_run ON supervised_patch_artifacts(run_id);
CREATE INDEX IF NOT EXISTS idx_supervised_patch_artifacts_created ON supervised_patch_artifacts(created_at);

CREATE TABLE IF NOT EXISTS native_scorecard_artifacts (
    artifact_sequence INTEGER PRIMARY KEY,
    artifact_id TEXT NOT NULL UNIQUE,
    run_id TEXT NOT NULL,
    dispatch_id TEXT,
    scorecard_schema_version TEXT NOT NULL,
    content_sha256 TEXT NOT NULL,
    read_only INTEGER NOT NULL DEFAULT 1,
    redaction_status TEXT NOT NULL,
    created_at TEXT NOT NULL,
    artifact_json TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_native_scorecard_artifacts_run ON native_scorecard_artifacts(run_id);
CREATE INDEX IF NOT EXISTS idx_native_scorecard_artifacts_dispatch ON native_scorecard_artifacts(dispatch_id);
CREATE INDEX IF NOT EXISTS idx_native_scorecard_artifacts_created ON native_scorecard_artifacts(created_at);

CREATE TABLE IF NOT EXISTS regression_report_artifacts (
    artifact_sequence INTEGER PRIMARY KEY,
    artifact_id TEXT NOT NULL UNIQUE,
    artifact_kind TEXT NOT NULL,
    report_schema_version TEXT NOT NULL,
    registry_id TEXT NOT NULL,
    registry_sha256 TEXT NOT NULL,
    scenario_id TEXT,
    content_sha256 TEXT NOT NULL,
    created_at TEXT NOT NULL,
    artifact_json TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_regression_report_artifacts_registry ON regression_report_artifacts(registry_id, artifact_sequence);
CREATE INDEX IF NOT EXISTS idx_regression_report_artifacts_scenario ON regression_report_artifacts(scenario_id, artifact_sequence);
CREATE INDEX IF NOT EXISTS idx_regression_report_artifacts_created ON regression_report_artifacts(created_at);

CREATE TABLE IF NOT EXISTS budget_evidence_artifacts (
    artifact_sequence INTEGER PRIMARY KEY,
    artifact_id TEXT NOT NULL UNIQUE,
    artifact_kind TEXT NOT NULL,
    evidence_id TEXT NOT NULL,
    evidence_sha256 TEXT NOT NULL,
    created_at TEXT NOT NULL,
    artifact_json TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_budget_evidence_artifacts_kind ON budget_evidence_artifacts(artifact_kind, artifact_sequence);
CREATE INDEX IF NOT EXISTS idx_budget_evidence_artifacts_created ON budget_evidence_artifacts(created_at);

CREATE TABLE IF NOT EXISTS budget_pause_decisions (
    decision_id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL,
    artifact_id TEXT NOT NULL,
    evidence_sha256 TEXT NOT NULL,
    state TEXT NOT NULL,
    cause TEXT NOT NULL,
    policy_json TEXT NOT NULL,
    actor TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    recovery_reason TEXT,
    UNIQUE(run_id, artifact_id)
);
CREATE INDEX IF NOT EXISTS idx_budget_pause_decisions_run ON budget_pause_decisions(run_id, created_at);

CREATE TABLE IF NOT EXISTS offline_replay_artifacts (
    artifact_sequence INTEGER PRIMARY KEY,
    artifact_id TEXT NOT NULL UNIQUE,
    report_schema_version TEXT NOT NULL,
    status TEXT NOT NULL,
    eligibility_content_sha256 TEXT NOT NULL,
    content_sha256 TEXT NOT NULL,
    created_at TEXT NOT NULL,
    artifact_json TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_offline_replay_artifacts_status ON offline_replay_artifacts(status, artifact_sequence);
CREATE INDEX IF NOT EXISTS idx_offline_replay_artifacts_created ON offline_replay_artifacts(created_at);

CREATE TABLE IF NOT EXISTS durable_memory_versions (
    memory_id TEXT NOT NULL,
    version BIGINT NOT NULL,
    tenant_id TEXT NOT NULL,
    workspace_id TEXT NOT NULL,
    agent_id TEXT,
    run_id TEXT,
    task_id TEXT,
    source_id TEXT NOT NULL,
    source_sha256 TEXT NOT NULL CHECK (length(source_sha256) = 64),
    conflict_key TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('current','superseded','conflicting','invalid','tombstoned','expired')),
    confidence REAL NOT NULL,
    fresh_until TEXT,
    expires_at TEXT,
    supersedes_memory_id TEXT,
    content_json TEXT NOT NULL,
    embedding_json TEXT,
    embedding_provenance TEXT NOT NULL,
    record_sha256 TEXT NOT NULL CHECK (length(record_sha256) = 64),
    created_at TEXT NOT NULL,
    created_by TEXT NOT NULL,
    PRIMARY KEY (memory_id, version)
);
CREATE INDEX IF NOT EXISTS idx_durable_memory_scope ON durable_memory_versions(tenant_id, workspace_id, agent_id, task_id, state);
CREATE INDEX IF NOT EXISTS idx_durable_memory_source ON durable_memory_versions(source_id, source_sha256);
CREATE INDEX IF NOT EXISTS idx_durable_memory_conflict ON durable_memory_versions(tenant_id, workspace_id, conflict_key, state);

CREATE TABLE IF NOT EXISTS memory_retrieval_events (
    retrieval_id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    workspace_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    node_id TEXT NOT NULL,
    agent_id TEXT,
    request_sha256 TEXT NOT NULL CHECK (length(request_sha256) = 64),
    result_sha256 TEXT NOT NULL CHECK (length(result_sha256) = 64),
    mode TEXT NOT NULL,
    candidate_count BIGINT NOT NULL,
    selected_count BIGINT NOT NULL,
    estimated_tokens BIGINT NOT NULL,
    read_bytes BIGINT NOT NULL,
    truncated BIGINT NOT NULL,
    evidence_json TEXT NOT NULL,
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_memory_retrieval_run ON memory_retrieval_events(run_id, node_id, created_at);
CREATE INDEX IF NOT EXISTS idx_memory_retrieval_scope ON memory_retrieval_events(tenant_id, workspace_id, created_at);

CREATE TABLE IF NOT EXISTS production_jobs (
    job_key TEXT PRIMARY KEY,
    job_kind TEXT NOT NULL,
    scope_sha256 TEXT NOT NULL CHECK (length(scope_sha256) = 64),
    input_sha256 TEXT NOT NULL CHECK (length(input_sha256) = 64),
    state TEXT NOT NULL CHECK (state IN ('running','completed','failed')),
    lease_owner TEXT,
    lease_token TEXT,
    lease_expires_at TEXT,
    result_json TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_production_jobs_kind ON production_jobs(job_kind, state, updated_at);

CREATE TABLE IF NOT EXISTS normalized_usage_observations (
    observation_id TEXT PRIMARY KEY,
    dedupe_key TEXT NOT NULL UNIQUE,
    source_kind TEXT NOT NULL,
    source_id TEXT NOT NULL,
    source_sha256 TEXT NOT NULL CHECK (length(source_sha256) = 64),
    occurred_at TEXT NOT NULL,
    tenant_id TEXT NOT NULL,
    workspace_id TEXT,
    run_id TEXT,
    dispatch_id TEXT,
    provider_id TEXT,
    model_id TEXT,
    pricing_identity TEXT,
    pricing_effective_date TEXT,
    currency TEXT,
    provenance_json TEXT NOT NULL,
    usage_json TEXT NOT NULL,
    completeness TEXT NOT NULL,
    confidence DOUBLE PRECISION NOT NULL,
    record_sha256 TEXT NOT NULL CHECK (length(record_sha256) = 64),
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_normalized_usage_run
    ON normalized_usage_observations(run_id, occurred_at, observation_id);
CREATE INDEX IF NOT EXISTS idx_normalized_usage_dispatch
    ON normalized_usage_observations(dispatch_id, occurred_at, observation_id);

CREATE TABLE IF NOT EXISTS replay_producer_bindings (
    artifact_id TEXT PRIMARY KEY,
    input_sha256 TEXT NOT NULL CHECK (length(input_sha256) = 64),
    dispatch_ids_json TEXT NOT NULL,
    maximum_trace_age_seconds BIGINT NOT NULL,
    scope_json TEXT NOT NULL,
    current_policy_json TEXT NOT NULL,
    candidate_policies_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    created_by TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_replay_producer_bindings_created
    ON replay_producer_bindings(created_at, artifact_id);

CREATE TABLE IF NOT EXISTS operator_acknowledgements (
    acknowledgement_id TEXT PRIMARY KEY,
    decision_id TEXT NOT NULL,
    source_type TEXT NOT NULL,
    source_id TEXT NOT NULL,
    source_sha256 TEXT NOT NULL CHECK (length(source_sha256) = 64),
    reason TEXT,
    actor TEXT NOT NULL,
    created_at TEXT NOT NULL,
    UNIQUE(decision_id, source_type, source_id, source_sha256)
);
CREATE INDEX IF NOT EXISTS idx_operator_acknowledgements_source
    ON operator_acknowledgements(source_type, source_id, created_at);


CREATE TABLE IF NOT EXISTS scheduler_feedback (
    feedback_id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL,
    node_id TEXT,
    executor_type TEXT NOT NULL,
    task_group TEXT NOT NULL,
    task_domain TEXT NOT NULL,
    task_intent TEXT NOT NULL,
    success INTEGER NOT NULL DEFAULT 0,
    latency_ms INTEGER NOT NULL DEFAULT 0,
    retry_count INTEGER NOT NULL DEFAULT 0,
    quality_score REAL NOT NULL DEFAULT 0.0,
    cost REAL NOT NULL DEFAULT 0.0,
    error_domain TEXT,
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_scheduler_feedback_run ON scheduler_feedback(run_id);
CREATE INDEX IF NOT EXISTS idx_scheduler_feedback_task_group ON scheduler_feedback(task_group);
CREATE INDEX IF NOT EXISTS idx_scheduler_feedback_created ON scheduler_feedback(created_at);

CREATE TABLE IF NOT EXISTS agent_profiles (
    profile_id TEXT PRIMARY KEY,
    role TEXT NOT NULL,
    tools_json TEXT NOT NULL,
    model_hint TEXT,
    context_budget_tokens INTEGER,
    workspace_scope TEXT NOT NULL DEFAULT 'task',
    executor_preference TEXT,
    max_retries INTEGER NOT NULL DEFAULT 3,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS tool_capabilities (
    tool_name TEXT PRIMARY KEY,
    description TEXT NOT NULL,
    input_schema_json TEXT,
    output_schema_json TEXT,
    requires_approval INTEGER NOT NULL DEFAULT 0,
    risk_level TEXT NOT NULL DEFAULT 'low',
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS tool_allowlists (
    profile_id TEXT NOT NULL,
    tool_name TEXT NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY (profile_id, tool_name)
);

CREATE TABLE IF NOT EXISTS tool_hooks (
    hook_id TEXT PRIMARY KEY,
    hook_type TEXT NOT NULL,
    tool_name TEXT,
    condition_json TEXT,
    action TEXT NOT NULL,
    action_config_json TEXT,
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS orchestration_decisions (
    decision_id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL,
    node_id TEXT,
    action TEXT NOT NULL,
    action_reason TEXT NOT NULL,
    selected_executor TEXT NOT NULL,
    blocked_reason TEXT,
    confidence TEXT NOT NULL,
    confidence_score REAL NOT NULL,
    input_signals_json TEXT NOT NULL,
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_orchestration_decisions_run ON orchestration_decisions(run_id);
CREATE INDEX IF NOT EXISTS idx_orchestration_decisions_action ON orchestration_decisions(action);
CREATE INDEX IF NOT EXISTS idx_orchestration_decisions_created ON orchestration_decisions(created_at);

CREATE TABLE IF NOT EXISTS controlled_loop_policy_proposals (
    proposal_sequence INTEGER PRIMARY KEY,
    proposal_id TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    status TEXT NOT NULL,
    title TEXT NOT NULL,
    summary TEXT,
    task_domain TEXT NOT NULL,
    task_intent TEXT NOT NULL,
    target_tier TEXT NOT NULL,
    evidence_json TEXT NOT NULL,
    approval_json TEXT,
    proposal_json TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_policy_proposals_status ON controlled_loop_policy_proposals(status);
CREATE INDEX IF NOT EXISTS idx_policy_proposals_key ON controlled_loop_policy_proposals(task_domain, task_intent, status);

CREATE TABLE IF NOT EXISTS controlled_loop_policy_snapshots (
    snapshot_sequence INTEGER PRIMARY KEY,
    adjustment_id TEXT NOT NULL UNIQUE,
    snapshot_id TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    status TEXT NOT NULL,
    actor TEXT NOT NULL,
    created_by TEXT NOT NULL,
    source TEXT NOT NULL,
    candidate_id TEXT NOT NULL,
    proposal_id TEXT NOT NULL,
    policy_key TEXT NOT NULL,
    target_tier TEXT NOT NULL,
    active_policy_before_json TEXT NOT NULL,
    rollback_target_json TEXT NOT NULL,
    evidence_ids_json TEXT NOT NULL,
    safety_hash TEXT NOT NULL,
    snapshot_json TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_policy_snapshots_status ON controlled_loop_policy_snapshots(status);
CREATE INDEX IF NOT EXISTS idx_policy_snapshots_proposal ON controlled_loop_policy_snapshots(proposal_id);
CREATE INDEX IF NOT EXISTS idx_policy_snapshots_adjustment ON controlled_loop_policy_snapshots(adjustment_id);
CREATE INDEX IF NOT EXISTS idx_policy_snapshots_policy_key ON controlled_loop_policy_snapshots(policy_key);
CREATE UNIQUE INDEX IF NOT EXISTS idx_policy_snapshots_active_policy_key
    ON controlled_loop_policy_snapshots(policy_key)
    WHERE status = 'active';

CREATE TABLE IF NOT EXISTS agent_state (
    agent_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    role TEXT NOT NULL,
    capability_profile_json TEXT NOT NULL DEFAULT '[]',
    objective TEXT,
    status TEXT NOT NULL DEFAULT 'idle',
    scratchpad_summary TEXT,
    redaction_filter TEXT,
    metadata_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (agent_id, run_id)
);

CREATE TABLE IF NOT EXISTS agent_mailbox (
    message_sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    message_id TEXT NOT NULL UNIQUE,
    correlation_id TEXT,
    from_agent_id TEXT NOT NULL,
    to_agent_id TEXT NOT NULL,
    run_id TEXT,
    node_id TEXT,
    message_type TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    body TEXT,
    body_summary TEXT,
    redaction_status TEXT NOT NULL DEFAULT 'none',
    created_at TEXT NOT NULL,
    read_at TEXT,
    ack_at TEXT,
    reply_to_message_id TEXT,
    metadata_json TEXT NOT NULL DEFAULT '{}'
);
CREATE INDEX IF NOT EXISTS idx_agent_mailbox_to ON agent_mailbox(to_agent_id);
CREATE INDEX IF NOT EXISTS idx_agent_mailbox_run ON agent_mailbox(run_id);
CREATE INDEX IF NOT EXISTS idx_agent_mailbox_status ON agent_mailbox(status);
CREATE INDEX IF NOT EXISTS idx_agent_mailbox_correlation ON agent_mailbox(correlation_id);

CREATE TABLE IF NOT EXISTS agent_proposals (
    proposal_id TEXT NOT NULL,
    correlation_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    parent_node_id TEXT NOT NULL,
    agent_id TEXT NOT NULL,
    target_agent_id TEXT,
    proposed_node_id TEXT,
    proposed_edge_id TEXT,
    proposal_type TEXT NOT NULL DEFAULT 'child_task',
    objective TEXT NOT NULL,
    context_summary TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'pending',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (proposal_id)
);
CREATE INDEX IF NOT EXISTS idx_agent_proposals_run ON agent_proposals(run_id);
CREATE INDEX IF NOT EXISTS idx_agent_proposals_correlation ON agent_proposals(correlation_id);
CREATE INDEX IF NOT EXISTS idx_agent_proposals_status ON agent_proposals(status);
CREATE INDEX IF NOT EXISTS idx_agent_proposals_agent ON agent_proposals(agent_id);

CREATE TABLE IF NOT EXISTS agent_action_receipts (
    run_id TEXT NOT NULL,
    node_id TEXT NOT NULL,
    agent_id TEXT NOT NULL,
    action_sha256 TEXT NOT NULL CHECK (length(action_sha256) = 64),
    action_type TEXT NOT NULL,
    result_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY (run_id, node_id)
);
CREATE INDEX IF NOT EXISTS idx_agent_action_receipts_agent
    ON agent_action_receipts(agent_id, run_id);

CREATE TABLE IF NOT EXISTS tool_allowlist_profiles (
    profile_id TEXT PRIMARY KEY,
    configured_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS tool_execution_authorizations (
    run_id TEXT NOT NULL,
    node_id TEXT NOT NULL,
    action_sha256 TEXT NOT NULL CHECK (length(action_sha256) = 64),
    tool_name TEXT NOT NULL,
    profile_id TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('requested', 'approved', 'rejected', 'consumed')),
    requested_approval_id TEXT NOT NULL UNIQUE,
    resolved_by TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (run_id, node_id)
);
CREATE INDEX IF NOT EXISTS idx_tool_execution_authorizations_status
    ON tool_execution_authorizations(status, run_id);

CREATE TABLE IF NOT EXISTS external_runtime_checkpoints (
    checkpoint_id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    workspace_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    node_id TEXT NOT NULL,
    thread_id TEXT NOT NULL,
    runtime_kind TEXT NOT NULL CHECK (runtime_kind = 'langgraph'),
    adapter_version TEXT NOT NULL,
    runtime_version TEXT NOT NULL,
    memory_strategy TEXT NOT NULL,
    checkpoint_summary_json TEXT NOT NULL,
    state_sha256 TEXT NOT NULL CHECK (length(state_sha256) = 64),
    status TEXT NOT NULL CHECK (status IN ('active','completed','tombstoned')),
    version INTEGER NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (tenant_id, workspace_id, run_id, node_id, thread_id)
);
CREATE INDEX IF NOT EXISTS idx_external_runtime_checkpoints_scope
    ON external_runtime_checkpoints(tenant_id, workspace_id, run_id, node_id, updated_at);

CREATE TABLE IF NOT EXISTS external_runtime_invocations (
    invocation_id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    workspace_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    node_id TEXT NOT NULL,
    thread_id TEXT NOT NULL,
    idempotency_sha256 TEXT NOT NULL CHECK (length(idempotency_sha256) = 64),
    checkpoint_id TEXT NOT NULL,
    lease_token TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('started','completed','failed','blocked')),
    result_summary_json TEXT,
    artifact_id TEXT,
    failure_code TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    completed_at TEXT,
    UNIQUE (tenant_id, workspace_id, run_id, node_id, idempotency_sha256)
);
CREATE INDEX IF NOT EXISTS idx_external_runtime_invocations_scope
    ON external_runtime_invocations(tenant_id, workspace_id, run_id, node_id, updated_at);
";

pub(crate) const POSTGRES_DDL: &str = "
CREATE TABLE IF NOT EXISTS dispatch_history (
    history_id BIGSERIAL PRIMARY KEY,
    dispatch_id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    raw_request TEXT NOT NULL,
    request_source TEXT NOT NULL,
    final_status TEXT NOT NULL,
    selected_tier TEXT NOT NULL,
    risk_level TEXT NOT NULL,
    reserved_cost DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    bundle_json TEXT NOT NULL,
    input_tokens INTEGER,
    output_tokens INTEGER,
    estimated_cost_usd DOUBLE PRECISION,
    executor_type TEXT NOT NULL DEFAULT 'noop',
    latency_ms INTEGER,
    trace_schema_version TEXT,
    trace_content_sha256 TEXT
);
CREATE INDEX IF NOT EXISTS idx_dispatch_history_created ON dispatch_history(created_at);
CREATE INDEX IF NOT EXISTS idx_dispatch_history_dispatch_id ON dispatch_history(dispatch_id);

CREATE TABLE IF NOT EXISTS local_config (
    key TEXT PRIMARY KEY,
    value_json TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    updated_by TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS team_members (
    user_id TEXT PRIMARY KEY,
    display_name TEXT NOT NULL,
    role TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS api_key_metadata (
    key_id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    role TEXT NOT NULL,
    scopes_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    created_by TEXT NOT NULL,
    revoked_at TEXT,
    last_used_at TEXT,
    expires_at TEXT
);

CREATE TABLE IF NOT EXISTS audit_log (
    audit_id BIGSERIAL PRIMARY KEY,
    created_at TEXT NOT NULL,
    actor TEXT NOT NULL,
    action TEXT NOT NULL,
    resource TEXT NOT NULL,
    details_json TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_audit_created ON audit_log(created_at);

CREATE TABLE IF NOT EXISTS provider_audit_events (
    event_id TEXT PRIMARY KEY,
    dispatch_id TEXT NOT NULL,
    provider_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    input_token_count INTEGER,
    output_token_count INTEGER,
    cost DOUBLE PRECISION,
    currency TEXT,
    latency_ms INTEGER,
    error_domain TEXT,
    redaction_status TEXT NOT NULL,
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_provider_audit_dispatch ON provider_audit_events(dispatch_id);
CREATE INDEX IF NOT EXISTS idx_provider_audit_created ON provider_audit_events(created_at);

CREATE TABLE IF NOT EXISTS workflow_plans (
    plan_sequence BIGSERIAL PRIMARY KEY,
    plan_id TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    raw_request TEXT NOT NULL,
    request_source TEXT NOT NULL,
    status TEXT NOT NULL,
    workflow_id TEXT NOT NULL,
    dispatch_id TEXT NOT NULL,
    graph_json TEXT NOT NULL,
    analysis_json TEXT NOT NULL,
    boundaries_json TEXT NOT NULL,
    plan_json TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_workflow_plans_created ON workflow_plans(created_at);
CREATE INDEX IF NOT EXISTS idx_workflow_plans_status ON workflow_plans(status);

CREATE TABLE IF NOT EXISTS workflow_runs (
    run_sequence BIGSERIAL PRIMARY KEY,
    run_id TEXT NOT NULL UNIQUE,
    plan_id TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    status TEXT NOT NULL,
    workflow_id TEXT NOT NULL,
    dispatch_id TEXT,
    started_at TEXT,
    completed_at TEXT,
    last_heartbeat_at TEXT,
    result_json TEXT,
    boundaries_json TEXT NOT NULL,
    run_json TEXT NOT NULL,
    priority INTEGER NOT NULL DEFAULT 5,
    deadline_at TEXT,
    sla_ms INTEGER,
    tenant_id TEXT,
    queue_position INTEGER,
    pause_reason TEXT,
    degrade_mode TEXT
);
CREATE INDEX IF NOT EXISTS idx_workflow_runs_created ON workflow_runs(created_at);
CREATE INDEX IF NOT EXISTS idx_workflow_runs_status ON workflow_runs(status);
CREATE INDEX IF NOT EXISTS idx_workflow_runs_plan ON workflow_runs(plan_id);
CREATE INDEX IF NOT EXISTS idx_workflow_runs_priority ON workflow_runs(priority, created_at);

CREATE TABLE IF NOT EXISTS workflow_run_nodes (
    run_id TEXT NOT NULL,
    node_id TEXT NOT NULL,
    task_type TEXT NOT NULL,
    status TEXT NOT NULL,
    node_json TEXT NOT NULL,
    started_at TEXT,
    completed_at TEXT,
    attempt_count INTEGER NOT NULL DEFAULT 0,
    timeout_ms INTEGER,
    blocked_reason TEXT,
    leased_at TEXT,
    profile_id TEXT,
    PRIMARY KEY (run_id, node_id)
);
CREATE INDEX IF NOT EXISTS idx_workflow_run_nodes_run ON workflow_run_nodes(run_id);

CREATE TABLE IF NOT EXISTS workflow_run_edges (
    run_id TEXT NOT NULL,
    edge_id TEXT NOT NULL,
    from_node_id TEXT NOT NULL,
    to_node_id TEXT NOT NULL,
    edge_type TEXT NOT NULL,
    edge_json TEXT NOT NULL,
    PRIMARY KEY (run_id, edge_id)
);
CREATE INDEX IF NOT EXISTS idx_workflow_run_edges_run ON workflow_run_edges(run_id);

CREATE TABLE IF NOT EXISTS workflow_run_events (
    event_sequence BIGSERIAL PRIMARY KEY,
    event_id TEXT NOT NULL UNIQUE,
    run_id TEXT NOT NULL,
    node_id TEXT,
    event_type TEXT NOT NULL,
    actor TEXT NOT NULL,
    created_at TEXT NOT NULL,
    details_json TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_workflow_run_events_run ON workflow_run_events(run_id);
CREATE INDEX IF NOT EXISTS idx_workflow_run_events_created ON workflow_run_events(created_at);

CREATE TABLE IF NOT EXISTS workflow_run_approvals (
    approval_sequence BIGSERIAL PRIMARY KEY,
    approval_id TEXT NOT NULL UNIQUE,
    run_id TEXT NOT NULL,
    node_id TEXT NOT NULL,
    decision TEXT NOT NULL,
    actor TEXT NOT NULL,
    reason TEXT,
    created_at TEXT NOT NULL,
    approval_json TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_workflow_run_approvals_run ON workflow_run_approvals(run_id);

CREATE TABLE IF NOT EXISTS supervised_patch_workspaces (
    workspace_sequence BIGSERIAL PRIMARY KEY,
    workspace_id TEXT NOT NULL UNIQUE,
    plan_id TEXT,
    run_id TEXT NOT NULL,
    target_id TEXT NOT NULL,
    target_repo_path TEXT NOT NULL,
    target_repo_canonical_path TEXT NOT NULL,
    workspace_path TEXT NOT NULL,
    workspace_canonical_path TEXT NOT NULL,
    source_revision TEXT NOT NULL,
    source_tree_hash TEXT,
    status TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    boundary_json TEXT NOT NULL,
    workspace_json TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_supervised_patch_workspaces_run ON supervised_patch_workspaces(run_id);
CREATE INDEX IF NOT EXISTS idx_supervised_patch_workspaces_target ON supervised_patch_workspaces(target_id, source_revision);
CREATE INDEX IF NOT EXISTS idx_supervised_patch_workspaces_status ON supervised_patch_workspaces(status);

CREATE TABLE IF NOT EXISTS supervised_patch_artifacts (
    artifact_sequence BIGSERIAL PRIMARY KEY,
    artifact_id TEXT NOT NULL UNIQUE,
    workspace_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    plan_id TEXT,
    target_id TEXT NOT NULL,
    source_revision TEXT NOT NULL,
    artifact_type TEXT NOT NULL,
    patch_hash TEXT NOT NULL,
    changed_files_json TEXT NOT NULL,
    redaction_status TEXT NOT NULL,
    created_at TEXT NOT NULL,
    artifact_json TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_supervised_patch_artifacts_workspace ON supervised_patch_artifacts(workspace_id);
CREATE INDEX IF NOT EXISTS idx_supervised_patch_artifacts_run ON supervised_patch_artifacts(run_id);
CREATE INDEX IF NOT EXISTS idx_supervised_patch_artifacts_created ON supervised_patch_artifacts(created_at);

CREATE TABLE IF NOT EXISTS native_scorecard_artifacts (
    artifact_sequence BIGSERIAL PRIMARY KEY,
    artifact_id TEXT NOT NULL UNIQUE,
    run_id TEXT NOT NULL,
    dispatch_id TEXT,
    scorecard_schema_version TEXT NOT NULL,
    content_sha256 TEXT NOT NULL,
    read_only BOOLEAN NOT NULL DEFAULT TRUE,
    redaction_status TEXT NOT NULL,
    created_at TEXT NOT NULL,
    artifact_json TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_native_scorecard_artifacts_run ON native_scorecard_artifacts(run_id);
CREATE INDEX IF NOT EXISTS idx_native_scorecard_artifacts_dispatch ON native_scorecard_artifacts(dispatch_id);
CREATE INDEX IF NOT EXISTS idx_native_scorecard_artifacts_created ON native_scorecard_artifacts(created_at);

CREATE TABLE IF NOT EXISTS regression_report_artifacts (
    artifact_sequence BIGSERIAL PRIMARY KEY,
    artifact_id TEXT NOT NULL UNIQUE,
    artifact_kind TEXT NOT NULL,
    report_schema_version TEXT NOT NULL,
    registry_id TEXT NOT NULL,
    registry_sha256 TEXT NOT NULL,
    scenario_id TEXT,
    content_sha256 TEXT NOT NULL,
    created_at TEXT NOT NULL,
    artifact_json TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_regression_report_artifacts_registry ON regression_report_artifacts(registry_id, artifact_sequence);
CREATE INDEX IF NOT EXISTS idx_regression_report_artifacts_scenario ON regression_report_artifacts(scenario_id, artifact_sequence);
CREATE INDEX IF NOT EXISTS idx_regression_report_artifacts_created ON regression_report_artifacts(created_at);

CREATE TABLE IF NOT EXISTS budget_evidence_artifacts (
    artifact_sequence BIGSERIAL PRIMARY KEY,
    artifact_id TEXT NOT NULL UNIQUE,
    artifact_kind TEXT NOT NULL,
    evidence_id TEXT NOT NULL,
    evidence_sha256 TEXT NOT NULL,
    created_at TEXT NOT NULL,
    artifact_json TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_budget_evidence_artifacts_kind ON budget_evidence_artifacts(artifact_kind, artifact_sequence);
CREATE INDEX IF NOT EXISTS idx_budget_evidence_artifacts_created ON budget_evidence_artifacts(created_at);

CREATE TABLE IF NOT EXISTS budget_pause_decisions (
    decision_id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL,
    artifact_id TEXT NOT NULL,
    evidence_sha256 TEXT NOT NULL,
    state TEXT NOT NULL,
    cause TEXT NOT NULL,
    policy_json TEXT NOT NULL,
    actor TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    recovery_reason TEXT,
    UNIQUE(run_id, artifact_id)
);
CREATE INDEX IF NOT EXISTS idx_budget_pause_decisions_run ON budget_pause_decisions(run_id, created_at);

CREATE TABLE IF NOT EXISTS offline_replay_artifacts (
    artifact_sequence BIGSERIAL PRIMARY KEY,
    artifact_id TEXT NOT NULL UNIQUE,
    report_schema_version TEXT NOT NULL,
    status TEXT NOT NULL,
    eligibility_content_sha256 TEXT NOT NULL,
    content_sha256 TEXT NOT NULL,
    created_at TEXT NOT NULL,
    artifact_json TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_offline_replay_artifacts_status ON offline_replay_artifacts(status, artifact_sequence);
CREATE INDEX IF NOT EXISTS idx_offline_replay_artifacts_created ON offline_replay_artifacts(created_at);

CREATE TABLE IF NOT EXISTS durable_memory_versions (
    memory_id TEXT NOT NULL,
    version BIGINT NOT NULL,
    tenant_id TEXT NOT NULL,
    workspace_id TEXT NOT NULL,
    agent_id TEXT,
    run_id TEXT,
    task_id TEXT,
    source_id TEXT NOT NULL,
    source_sha256 TEXT NOT NULL CHECK (length(source_sha256) = 64),
    conflict_key TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('current','superseded','conflicting','invalid','tombstoned','expired')),
    confidence DOUBLE PRECISION NOT NULL,
    fresh_until TEXT,
    expires_at TEXT,
    supersedes_memory_id TEXT,
    content_json TEXT NOT NULL,
    embedding_json TEXT,
    embedding_provenance TEXT NOT NULL,
    record_sha256 TEXT NOT NULL CHECK (length(record_sha256) = 64),
    created_at TEXT NOT NULL,
    created_by TEXT NOT NULL,
    PRIMARY KEY (memory_id, version)
);
CREATE INDEX IF NOT EXISTS idx_durable_memory_scope ON durable_memory_versions(tenant_id, workspace_id, agent_id, task_id, state);
CREATE INDEX IF NOT EXISTS idx_durable_memory_source ON durable_memory_versions(source_id, source_sha256);
CREATE INDEX IF NOT EXISTS idx_durable_memory_conflict ON durable_memory_versions(tenant_id, workspace_id, conflict_key, state);

CREATE TABLE IF NOT EXISTS memory_retrieval_events (
    retrieval_id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    workspace_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    node_id TEXT NOT NULL,
    agent_id TEXT,
    request_sha256 TEXT NOT NULL CHECK (length(request_sha256) = 64),
    result_sha256 TEXT NOT NULL CHECK (length(result_sha256) = 64),
    mode TEXT NOT NULL,
    candidate_count BIGINT NOT NULL,
    selected_count BIGINT NOT NULL,
    estimated_tokens BIGINT NOT NULL,
    read_bytes BIGINT NOT NULL,
    truncated BIGINT NOT NULL,
    evidence_json TEXT NOT NULL,
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_memory_retrieval_run ON memory_retrieval_events(run_id, node_id, created_at);
CREATE INDEX IF NOT EXISTS idx_memory_retrieval_scope ON memory_retrieval_events(tenant_id, workspace_id, created_at);

CREATE TABLE IF NOT EXISTS production_jobs (
    job_key TEXT PRIMARY KEY,
    job_kind TEXT NOT NULL,
    scope_sha256 TEXT NOT NULL CHECK (length(scope_sha256) = 64),
    input_sha256 TEXT NOT NULL CHECK (length(input_sha256) = 64),
    state TEXT NOT NULL CHECK (state IN ('running','completed','failed')),
    lease_owner TEXT,
    lease_token TEXT,
    lease_expires_at TEXT,
    result_json TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_production_jobs_kind ON production_jobs(job_kind, state, updated_at);

CREATE TABLE IF NOT EXISTS normalized_usage_observations (
    observation_id TEXT PRIMARY KEY,
    dedupe_key TEXT NOT NULL UNIQUE,
    source_kind TEXT NOT NULL,
    source_id TEXT NOT NULL,
    source_sha256 TEXT NOT NULL CHECK (length(source_sha256) = 64),
    occurred_at TEXT NOT NULL,
    tenant_id TEXT NOT NULL,
    workspace_id TEXT,
    run_id TEXT,
    dispatch_id TEXT,
    provider_id TEXT,
    model_id TEXT,
    pricing_identity TEXT,
    pricing_effective_date TEXT,
    currency TEXT,
    provenance_json TEXT NOT NULL,
    usage_json TEXT NOT NULL,
    completeness TEXT NOT NULL,
    confidence DOUBLE PRECISION NOT NULL,
    record_sha256 TEXT NOT NULL CHECK (length(record_sha256) = 64),
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_normalized_usage_run
    ON normalized_usage_observations(run_id, occurred_at, observation_id);
CREATE INDEX IF NOT EXISTS idx_normalized_usage_dispatch
    ON normalized_usage_observations(dispatch_id, occurred_at, observation_id);

CREATE TABLE IF NOT EXISTS replay_producer_bindings (
    artifact_id TEXT PRIMARY KEY,
    input_sha256 TEXT NOT NULL CHECK (length(input_sha256) = 64),
    dispatch_ids_json TEXT NOT NULL,
    maximum_trace_age_seconds BIGINT NOT NULL,
    scope_json TEXT NOT NULL,
    current_policy_json TEXT NOT NULL,
    candidate_policies_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    created_by TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_replay_producer_bindings_created
    ON replay_producer_bindings(created_at, artifact_id);

CREATE TABLE IF NOT EXISTS operator_acknowledgements (
    acknowledgement_id TEXT PRIMARY KEY,
    decision_id TEXT NOT NULL,
    source_type TEXT NOT NULL,
    source_id TEXT NOT NULL,
    source_sha256 TEXT NOT NULL CHECK (length(source_sha256) = 64),
    reason TEXT,
    actor TEXT NOT NULL,
    created_at TEXT NOT NULL,
    UNIQUE(decision_id, source_type, source_id, source_sha256)
);
CREATE INDEX IF NOT EXISTS idx_operator_acknowledgements_source
    ON operator_acknowledgements(source_type, source_id, created_at);


CREATE TABLE IF NOT EXISTS scheduler_feedback (
    feedback_id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL,
    node_id TEXT,
    executor_type TEXT NOT NULL,
    task_group TEXT NOT NULL,
    task_domain TEXT NOT NULL,
    task_intent TEXT NOT NULL,
    success INTEGER NOT NULL DEFAULT 0,
    latency_ms INTEGER NOT NULL DEFAULT 0,
    retry_count INTEGER NOT NULL DEFAULT 0,
    quality_score DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    cost DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    error_domain TEXT,
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_scheduler_feedback_run ON scheduler_feedback(run_id);
CREATE INDEX IF NOT EXISTS idx_scheduler_feedback_task_group ON scheduler_feedback(task_group);
CREATE INDEX IF NOT EXISTS idx_scheduler_feedback_created ON scheduler_feedback(created_at);

CREATE TABLE IF NOT EXISTS agent_profiles (
    profile_id TEXT PRIMARY KEY,
    role TEXT NOT NULL,
    tools_json TEXT NOT NULL,
    model_hint TEXT,
    context_budget_tokens INTEGER,
    workspace_scope TEXT NOT NULL DEFAULT 'task',
    executor_preference TEXT,
    max_retries INTEGER NOT NULL DEFAULT 3,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS tool_capabilities (
    tool_name TEXT PRIMARY KEY,
    description TEXT NOT NULL,
    input_schema_json TEXT,
    output_schema_json TEXT,
    requires_approval INTEGER NOT NULL DEFAULT 0,
    risk_level TEXT NOT NULL DEFAULT 'low',
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS tool_allowlists (
    profile_id TEXT NOT NULL,
    tool_name TEXT NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY (profile_id, tool_name)
);

CREATE TABLE IF NOT EXISTS tool_hooks (
    hook_id TEXT PRIMARY KEY,
    hook_type TEXT NOT NULL,
    tool_name TEXT,
    condition_json TEXT,
    action TEXT NOT NULL,
    action_config_json TEXT,
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS orchestration_decisions (
    decision_id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL,
    node_id TEXT,
    action TEXT NOT NULL,
    action_reason TEXT NOT NULL,
    selected_executor TEXT NOT NULL,
    blocked_reason TEXT,
    confidence TEXT NOT NULL,
    confidence_score DOUBLE PRECISION NOT NULL,
    input_signals_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    quality_signal_json TEXT,
    routing_signal_json TEXT,
    cost_signal_json TEXT,
    approval_signal_json TEXT,
    queue_signal_json TEXT,
    executor_pool_signal_json TEXT,
    candidate_executors_json TEXT,
    degraded_reason TEXT
);
CREATE INDEX IF NOT EXISTS idx_orchestration_decisions_run ON orchestration_decisions(run_id);
CREATE INDEX IF NOT EXISTS idx_orchestration_decisions_action ON orchestration_decisions(action);
CREATE INDEX IF NOT EXISTS idx_orchestration_decisions_created ON orchestration_decisions(created_at);

CREATE TABLE IF NOT EXISTS executor_pool (
    executor_type TEXT PRIMARY KEY,
    capabilities_json TEXT NOT NULL DEFAULT '{}',
    status_json TEXT NOT NULL DEFAULT '{}',
    cost_profile_json TEXT NOT NULL DEFAULT '{}',
    metrics_json TEXT NOT NULL DEFAULT '{}',
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS scheduler_heartbeat (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    last_heartbeat_at TEXT NOT NULL DEFAULT '',
    tick_count INTEGER NOT NULL DEFAULT 0,
    error_count INTEGER NOT NULL DEFAULT 0,
    uptime_seconds DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    metadata_json TEXT NOT NULL DEFAULT '{}',
    updated_at TEXT NOT NULL DEFAULT ''
);

CREATE TABLE IF NOT EXISTS controlled_loop_policy_proposals (
    proposal_sequence BIGSERIAL PRIMARY KEY,
    proposal_id TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    status TEXT NOT NULL,
    title TEXT NOT NULL,
    summary TEXT,
    task_domain TEXT NOT NULL,
    task_intent TEXT NOT NULL,
    target_tier TEXT NOT NULL,
    evidence_json TEXT NOT NULL,
    approval_json TEXT,
    proposal_json TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_policy_proposals_status ON controlled_loop_policy_proposals(status);
CREATE INDEX IF NOT EXISTS idx_policy_proposals_key ON controlled_loop_policy_proposals(task_domain, task_intent, status);

CREATE TABLE IF NOT EXISTS controlled_loop_policy_snapshots (
    snapshot_sequence BIGSERIAL PRIMARY KEY,
    adjustment_id TEXT NOT NULL UNIQUE,
    snapshot_id TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    status TEXT NOT NULL,
    actor TEXT NOT NULL,
    created_by TEXT NOT NULL,
    source TEXT NOT NULL,
    candidate_id TEXT NOT NULL,
    proposal_id TEXT NOT NULL,
    policy_key TEXT NOT NULL,
    target_tier TEXT NOT NULL,
    active_policy_before_json TEXT NOT NULL,
    rollback_target_json TEXT NOT NULL,
    evidence_ids_json TEXT NOT NULL,
    safety_hash TEXT NOT NULL,
    snapshot_json TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_policy_snapshots_status ON controlled_loop_policy_snapshots(status);
CREATE INDEX IF NOT EXISTS idx_policy_snapshots_proposal ON controlled_loop_policy_snapshots(proposal_id);
CREATE INDEX IF NOT EXISTS idx_policy_snapshots_adjustment ON controlled_loop_policy_snapshots(adjustment_id);
CREATE INDEX IF NOT EXISTS idx_policy_snapshots_policy_key ON controlled_loop_policy_snapshots(policy_key);
CREATE UNIQUE INDEX IF NOT EXISTS idx_policy_snapshots_active_policy_key
    ON controlled_loop_policy_snapshots(policy_key)
    WHERE status = 'active';

CREATE TABLE IF NOT EXISTS agent_state (
    agent_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    role TEXT NOT NULL,
    capability_profile_json TEXT NOT NULL DEFAULT '[]',
    objective TEXT,
    status TEXT NOT NULL DEFAULT 'idle',
    scratchpad_summary TEXT,
    redaction_filter TEXT,
    metadata_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (agent_id, run_id)
);

CREATE TABLE IF NOT EXISTS agent_mailbox (
    message_sequence BIGSERIAL PRIMARY KEY,
    message_id TEXT NOT NULL UNIQUE,
    correlation_id TEXT,
    from_agent_id TEXT NOT NULL,
    to_agent_id TEXT NOT NULL,
    run_id TEXT,
    node_id TEXT,
    message_type TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    body TEXT,
    body_summary TEXT,
    redaction_status TEXT NOT NULL DEFAULT 'none',
    created_at TEXT NOT NULL,
    read_at TEXT,
    ack_at TEXT,
    reply_to_message_id TEXT,
    metadata_json TEXT NOT NULL DEFAULT '{}'
);
CREATE INDEX IF NOT EXISTS idx_agent_mailbox_to ON agent_mailbox(to_agent_id);
CREATE INDEX IF NOT EXISTS idx_agent_mailbox_run ON agent_mailbox(run_id);
CREATE INDEX IF NOT EXISTS idx_agent_mailbox_status ON agent_mailbox(status);
CREATE INDEX IF NOT EXISTS idx_agent_mailbox_correlation ON agent_mailbox(correlation_id);

CREATE TABLE IF NOT EXISTS agent_proposals (
    proposal_id TEXT NOT NULL,
    correlation_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    parent_node_id TEXT NOT NULL,
    agent_id TEXT NOT NULL,
    target_agent_id TEXT,
    proposed_node_id TEXT,
    proposed_edge_id TEXT,
    proposal_type TEXT NOT NULL DEFAULT 'child_task',
    objective TEXT NOT NULL,
    context_summary TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'pending',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (proposal_id)
);
CREATE INDEX IF NOT EXISTS idx_agent_proposals_run ON agent_proposals(run_id);
CREATE INDEX IF NOT EXISTS idx_agent_proposals_correlation ON agent_proposals(correlation_id);
CREATE INDEX IF NOT EXISTS idx_agent_proposals_status ON agent_proposals(status);
CREATE INDEX IF NOT EXISTS idx_agent_proposals_agent ON agent_proposals(agent_id);

CREATE TABLE IF NOT EXISTS agent_action_receipts (
    run_id TEXT NOT NULL,
    node_id TEXT NOT NULL,
    agent_id TEXT NOT NULL,
    action_sha256 TEXT NOT NULL CHECK (length(action_sha256) = 64),
    action_type TEXT NOT NULL,
    result_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY (run_id, node_id)
);
CREATE INDEX IF NOT EXISTS idx_agent_action_receipts_agent
    ON agent_action_receipts(agent_id, run_id);

CREATE TABLE IF NOT EXISTS tool_allowlist_profiles (
    profile_id TEXT PRIMARY KEY,
    configured_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS tool_execution_authorizations (
    run_id TEXT NOT NULL,
    node_id TEXT NOT NULL,
    action_sha256 TEXT NOT NULL CHECK (length(action_sha256) = 64),
    tool_name TEXT NOT NULL,
    profile_id TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('requested', 'approved', 'rejected', 'consumed')),
    requested_approval_id TEXT NOT NULL UNIQUE,
    resolved_by TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (run_id, node_id)
);
CREATE INDEX IF NOT EXISTS idx_tool_execution_authorizations_status
    ON tool_execution_authorizations(status, run_id);

CREATE TABLE IF NOT EXISTS external_runtime_checkpoints (
    checkpoint_id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    workspace_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    node_id TEXT NOT NULL,
    thread_id TEXT NOT NULL,
    runtime_kind TEXT NOT NULL CHECK (runtime_kind = 'langgraph'),
    adapter_version TEXT NOT NULL,
    runtime_version TEXT NOT NULL,
    memory_strategy TEXT NOT NULL,
    checkpoint_summary_json TEXT NOT NULL,
    state_sha256 TEXT NOT NULL CHECK (length(state_sha256) = 64),
    status TEXT NOT NULL CHECK (status IN ('active','completed','tombstoned')),
    version BIGINT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (tenant_id, workspace_id, run_id, node_id, thread_id)
);
CREATE INDEX IF NOT EXISTS idx_external_runtime_checkpoints_scope
    ON external_runtime_checkpoints(tenant_id, workspace_id, run_id, node_id, updated_at);

CREATE TABLE IF NOT EXISTS external_runtime_invocations (
    invocation_id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    workspace_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    node_id TEXT NOT NULL,
    thread_id TEXT NOT NULL,
    idempotency_sha256 TEXT NOT NULL CHECK (length(idempotency_sha256) = 64),
    checkpoint_id TEXT NOT NULL,
    lease_token TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('started','completed','failed','blocked')),
    result_summary_json TEXT,
    artifact_id TEXT,
    failure_code TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    completed_at TEXT,
    UNIQUE (tenant_id, workspace_id, run_id, node_id, idempotency_sha256)
);
CREATE INDEX IF NOT EXISTS idx_external_runtime_invocations_scope
    ON external_runtime_invocations(tenant_id, workspace_id, run_id, node_id, updated_at);
";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_catalogs_are_sorted_contiguous_and_match_current_versions() {
        assert_catalog(SQLITE_MIGRATIONS);
        assert_catalog(POSTGRES_MIGRATIONS);
        assert_eq!(
            CURRENT_SQLITE_SCHEMA_VERSION,
            SQLITE_MIGRATIONS.last().unwrap().version
        );
        assert_eq!(
            CURRENT_POSTGRES_SCHEMA_VERSION,
            POSTGRES_MIGRATIONS.last().unwrap().version
        );
    }

    #[test]
    fn current_schema_ddl_contains_policy_snapshot_surface_for_both_dialects() {
        for expected in [
            "controlled_loop_policy_snapshots",
            "idx_policy_snapshots_status",
            "idx_policy_snapshots_proposal",
            "idx_policy_snapshots_adjustment",
            "idx_policy_snapshots_policy_key",
            "idx_policy_snapshots_active_policy_key",
            "agent_state",
            "agent_mailbox",
            "idx_agent_mailbox_to",
            "idx_agent_mailbox_run",
            "idx_agent_mailbox_status",
            "idx_agent_mailbox_correlation",
            "agent_proposals",
            "idx_agent_proposals_run",
            "idx_agent_proposals_correlation",
            "idx_agent_proposals_status",
            "idx_agent_proposals_agent",
            "native_scorecard_artifacts",
            "idx_native_scorecard_artifacts_run",
            "idx_native_scorecard_artifacts_dispatch",
            "idx_native_scorecard_artifacts_created",
            "regression_report_artifacts",
            "idx_regression_report_artifacts_registry",
            "idx_regression_report_artifacts_scenario",
            "idx_regression_report_artifacts_created",
        ] {
            assert!(
                SQLITE_DDL.contains(expected),
                "missing SQLite schema item {expected}"
            );
            assert!(
                POSTGRES_DDL.contains(expected),
                "missing PostgreSQL schema item {expected}"
            );
        }
    }

    #[test]
    fn ddl_renderer_routes_to_catalog_owned_sql() {
        assert_eq!(ddl_for(Dialect::Sqlite), SQLITE_DDL);
        assert_eq!(ddl_for(Dialect::Postgres), POSTGRES_DDL);
    }

    #[test]
    fn action_receipt_and_tool_authorization_constraints_fail_closed() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(SQLITE_DDL).unwrap();

        let short_hash = conn.execute(
            "INSERT INTO agent_action_receipts
             (run_id, node_id, agent_id, action_sha256, action_type, result_json, created_at)
             VALUES ('run-1', 'node-1', 'agent-1', 'short', 'wait', '{}', 'now')",
            [],
        );
        assert!(short_hash.is_err());

        let invalid_status = conn.execute(
            "INSERT INTO tool_execution_authorizations
             (run_id, node_id, action_sha256, tool_name, profile_id, status,
              requested_approval_id, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 'unexpected', ?6, ?7, ?7)",
            rusqlite::params![
                "run-1",
                "node-1",
                "a".repeat(64),
                "echo",
                "bounded",
                "approval-1",
                "now",
            ],
        );
        assert!(invalid_status.is_err());

        for ddl in [SQLITE_DDL, POSTGRES_DDL] {
            assert!(ddl.contains("CHECK (length(action_sha256) = 64)"));
            assert!(
                ddl.contains("CHECK (status IN ('requested', 'approved', 'rejected', 'consumed'))")
            );
        }
    }

    fn assert_catalog(catalog: &[SchemaMigration]) {
        for (i, migration) in catalog.iter().enumerate() {
            assert_eq!(migration.version, (i + 1) as i64);
            assert!(!migration.description.is_empty());
        }
    }
}
