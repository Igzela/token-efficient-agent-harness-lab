/// Full PostgreSQL DDL for all local_product_store tables.
///
/// Translation from SQLite:
/// - INTEGER PRIMARY KEY AUTOINCREMENT -> BIGSERIAL PRIMARY KEY
/// - INTEGER PRIMARY KEY (rowid alias sequences) -> BIGSERIAL PRIMARY KEY
/// - REAL -> DOUBLE PRECISION
/// - TEXT NOT NULL UNIQUE stays
/// - INTEGER NOT NULL DEFAULT 0 for booleans stays INTEGER
/// - CREATE INDEX IF NOT EXISTS works in PG
pub const PG_DDL: &str = "
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
    latency_ms INTEGER
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
";
