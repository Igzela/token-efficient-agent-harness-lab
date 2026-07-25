#![allow(dead_code)]

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Dialect {
    Sqlite,
    Postgres,
}

pub(super) const CURRENT_SQLITE_SCHEMA_VERSION: i64 = 34;
pub(super) const CURRENT_POSTGRES_SCHEMA_VERSION: i64 = 34;

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
    SchemaMigration {
        version: 26,
        description: "add bounded recursive execution tree and node identity state",
    },
    SchemaMigration {
        version: 27,
        description:
            "add harness evolution laboratory candidate/proposal/receipt evidence foundation",
    },
    SchemaMigration {
        version: 28,
        description:
            "add harness evolution evaluation, sealed holdout, and Pareto archive evidence",
    },
    SchemaMigration {
        version: 29,
        description: "add harness evolution PR_READY candidate bundles and finalizer receipts",
    },
    SchemaMigration {
        version: 30,
        description: "add product golden path canonical task identity and worktree binding state",
    },
    SchemaMigration {
        version: 31,
        description: "add canonical product task terminal evidence",
    },
    SchemaMigration {
        version: 32,
        description: "add managed acceptance decision, authorization, attempt admission, and hash-linked transition receipts",
    },
    SchemaMigration {
        version: 33,
        description: "add managed acceptance stable logical spend authorization, lease, and receipt identity",
    },
    SchemaMigration {
        version: 34,
        description: "add RWE run authorization, runs, and task attempt evidence",
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
    operation_kind TEXT NOT NULL CHECK (operation_kind IN ('memory_version','retrieval_query')),
    target_memory_id TEXT NOT NULL,
    target_version BIGINT NOT NULL,
    tenant_id TEXT NOT NULL,
    workspace_id TEXT NOT NULL,
    agent_id TEXT,
    run_id TEXT,
    task_id TEXT,
    source_id TEXT NOT NULL,
    source_sha256 TEXT NOT NULL CHECK (length(source_sha256) = 64),
    node_id TEXT,
    query_sha256 TEXT CHECK (query_sha256 IS NULL OR length(query_sha256) = 64),
    request_identity_sha256 TEXT NOT NULL CHECK (length(request_identity_sha256) = 64),
    operation_binding_sha256 TEXT NOT NULL CHECK (length(operation_binding_sha256) = 64),
    content_sha256 TEXT NOT NULL CHECK (length(content_sha256) = 64),
    contract_json TEXT NOT NULL,
    contract_sha256 TEXT NOT NULL CHECK (length(contract_sha256) = 64),
    receipt_sha256 TEXT NOT NULL CHECK (length(receipt_sha256) = 64),
    provider_id TEXT NOT NULL,
    requested_model_id TEXT NOT NULL,
    resolved_model_id TEXT NOT NULL,
    dimensions BIGINT NOT NULL CHECK (dimensions > 0),
    reservation_event_id TEXT NOT NULL,
    send_event_id TEXT,
    outcome_event_id TEXT,
    result_kind TEXT CHECK (result_kind IS NULL OR result_kind IN ('memory_version','retrieval_event')),
    result_id TEXT,
    result_sha256 TEXT CHECK (result_sha256 IS NULL OR length(result_sha256) = 64),
    state TEXT NOT NULL CHECK (state IN ('preflight_reserved','reserved','sending','network_succeeded','succeeded','result_erased','failed_before_send','failed_known_outcome','outcome_unknown','outcome_unknown_acknowledged','retry_authorized')),
    attempt_count BIGINT NOT NULL DEFAULT 1 CHECK (attempt_count BETWEEN 1 AND 4),
    vector_json TEXT,
    metadata_json TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (target_memory_id, target_version),
    FOREIGN KEY (reservation_event_id) REFERENCES provider_audit_events(event_id),
    FOREIGN KEY (send_event_id) REFERENCES provider_audit_events(event_id),
    FOREIGN KEY (outcome_event_id) REFERENCES provider_audit_events(event_id),
    CHECK ((result_kind IS NULL AND result_id IS NULL AND result_sha256 IS NULL)
        OR (result_kind IS NOT NULL AND result_id IS NOT NULL AND result_sha256 IS NOT NULL)),
    CHECK ((operation_kind='memory_version' AND node_id IS NULL AND query_sha256 IS NULL)
        OR (operation_kind='retrieval_query' AND run_id IS NOT NULL AND node_id IS NOT NULL
            AND query_sha256 IS NOT NULL AND query_sha256=source_sha256))
);
CREATE INDEX IF NOT EXISTS idx_provider_embedding_operations_state
    ON provider_embedding_operations(state, updated_at);
CREATE UNIQUE INDEX IF NOT EXISTS idx_provider_embedding_operations_retrieval_identity
    ON provider_embedding_operations(tenant_id,workspace_id,run_id,node_id,query_sha256,provider_id,
        requested_model_id,resolved_model_id,dimensions,request_identity_sha256)
    WHERE operation_kind='retrieval_query';
";

pub(super) const V26_DDL: &str = "
CREATE TABLE IF NOT EXISTS recursive_execution_trees (
    root_run_id TEXT PRIMARY KEY,
    workflow_id TEXT NOT NULL,
    root_node_id TEXT NOT NULL,
    tree_schema_version TEXT NOT NULL,
    tree_json TEXT NOT NULL,
    version BIGINT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_recursive_execution_trees_workflow
    ON recursive_execution_trees(workflow_id, updated_at);

CREATE TABLE IF NOT EXISTS recursive_execution_nodes (
    node_id TEXT NOT NULL,
    root_run_id TEXT NOT NULL,
    parent_node_id TEXT,
    proposal_id TEXT,
    depth BIGINT NOT NULL CHECK (depth BETWEEN 0 AND 2),
    objective_fingerprint TEXT NOT NULL CHECK (length(objective_fingerprint) = 64),
    status TEXT NOT NULL,
    version BIGINT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY(root_run_id, node_id),
    UNIQUE(root_run_id, objective_fingerprint),
    FOREIGN KEY(root_run_id) REFERENCES recursive_execution_trees(root_run_id)
);
CREATE INDEX IF NOT EXISTS idx_recursive_execution_nodes_root
    ON recursive_execution_nodes(root_run_id, depth, node_id);
CREATE INDEX IF NOT EXISTS idx_recursive_execution_nodes_parent
    ON recursive_execution_nodes(root_run_id, parent_node_id, status, node_id);
";

pub(super) const V27_DDL: &str = "
CREATE TABLE IF NOT EXISTS harness_evolution_active_identity (
    active_version_id TEXT PRIMARY KEY,
    active_version_hash TEXT NOT NULL CHECK (length(active_version_hash) = 64),
    evaluator_identity_hash TEXT NOT NULL CHECK (length(evaluator_identity_hash) = 64),
    body_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS harness_evolution_proposals (
    proposal_id TEXT PRIMARY KEY,
    parent_candidate_id TEXT,
    active_version_id TEXT NOT NULL,
    active_version_hash TEXT NOT NULL CHECK (length(active_version_hash) = 64),
    evaluator_identity_hash TEXT NOT NULL CHECK (length(evaluator_identity_hash) = 64),
    proposal_body_sha256 TEXT NOT NULL CHECK (length(proposal_body_sha256) = 64),
    body_json TEXT NOT NULL,
    seed BIGINT NOT NULL,
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_harness_evolution_proposals_active
    ON harness_evolution_proposals(active_version_id, created_at);

CREATE TABLE IF NOT EXISTS harness_evolution_candidates (
    candidate_id TEXT PRIMARY KEY,
    lineage_id TEXT NOT NULL,
    parent_candidate_id TEXT,
    proposal_id TEXT NOT NULL,
    active_version_id TEXT NOT NULL,
    active_version_hash TEXT NOT NULL CHECK (length(active_version_hash) = 64),
    evaluator_identity_hash TEXT NOT NULL CHECK (length(evaluator_identity_hash) = 64),
    content_hash TEXT NOT NULL CHECK (length(content_hash) = 64),
    status TEXT NOT NULL,
    terminal_reason TEXT NOT NULL,
    workspace_id TEXT NOT NULL,
    workspace_rel_path TEXT NOT NULL,
    body_json TEXT NOT NULL,
    seed BIGINT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(lineage_id, content_hash),
    FOREIGN KEY(proposal_id) REFERENCES harness_evolution_proposals(proposal_id)
);
CREATE INDEX IF NOT EXISTS idx_harness_evolution_candidates_lineage
    ON harness_evolution_candidates(lineage_id, created_at, candidate_id);
CREATE INDEX IF NOT EXISTS idx_harness_evolution_candidates_parent
    ON harness_evolution_candidates(parent_candidate_id, status);

CREATE TABLE IF NOT EXISTS harness_evolution_receipts (
    receipt_id TEXT PRIMARY KEY,
    candidate_id TEXT NOT NULL,
    proposal_id TEXT NOT NULL,
    lineage_id TEXT NOT NULL,
    active_version_id TEXT NOT NULL,
    terminal_reason TEXT NOT NULL,
    content_hash TEXT NOT NULL CHECK (length(content_hash) = 64),
    body_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY(candidate_id) REFERENCES harness_evolution_candidates(candidate_id)
);
CREATE INDEX IF NOT EXISTS idx_harness_evolution_receipts_candidate
    ON harness_evolution_receipts(candidate_id, created_at);
";

pub(super) const V29_DDL: &str = "
CREATE TABLE IF NOT EXISTS harness_evolution_pr_ready_bundles (
    bundle_id TEXT PRIMARY KEY,
    candidate_id TEXT NOT NULL,
    lineage_id TEXT NOT NULL,
    active_version_id TEXT NOT NULL,
    evaluation_id TEXT NOT NULL,
    patch_sha256 TEXT NOT NULL CHECK (length(patch_sha256) = 64),
    base_commit_sha TEXT NOT NULL CHECK (length(base_commit_sha) = 64),
    head_commit_sha TEXT NOT NULL CHECK (length(head_commit_sha) = 64),
    bundle_sha256 TEXT NOT NULL CHECK (length(bundle_sha256) = 64),
    terminal TEXT NOT NULL,
    body_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    UNIQUE(candidate_id, evaluation_id, patch_sha256)
);
CREATE INDEX IF NOT EXISTS idx_harness_evolution_pr_ready_candidate
    ON harness_evolution_pr_ready_bundles(candidate_id, created_at);

CREATE TABLE IF NOT EXISTS harness_evolution_pr_ready_receipts (
    receipt_id TEXT PRIMARY KEY,
    bundle_id TEXT NOT NULL,
    candidate_id TEXT NOT NULL,
    terminal TEXT NOT NULL,
    bundle_sha256 TEXT NOT NULL CHECK (length(bundle_sha256) = 64),
    body_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY(bundle_id) REFERENCES harness_evolution_pr_ready_bundles(bundle_id)
);
CREATE INDEX IF NOT EXISTS idx_harness_evolution_pr_ready_receipts_bundle
    ON harness_evolution_pr_ready_receipts(bundle_id, created_at);
";

pub(super) const V30_DDL: &str = "
CREATE TABLE IF NOT EXISTS product_tasks (
    task_id TEXT PRIMARY KEY,
    schema_version TEXT NOT NULL,
    tenant_id TEXT NOT NULL,
    workspace_id TEXT NOT NULL,
    idempotency_key TEXT NOT NULL,
    status TEXT NOT NULL,
    version BIGINT NOT NULL,
    objective_fingerprint TEXT NOT NULL CHECK (length(objective_fingerprint) = 64),
    target_id TEXT NOT NULL,
    target_repo_path TEXT NOT NULL,
    source_revision TEXT NOT NULL,
    source_tree_hash TEXT,
    output_intent TEXT NOT NULL,
    risk_class TEXT NOT NULL,
    approval_required INTEGER NOT NULL,
    confirm_execution INTEGER NOT NULL,
    confirm_output INTEGER NOT NULL,
    intake_contract_sha256 TEXT NOT NULL CHECK (length(intake_contract_sha256) = 64),
    intake_json TEXT NOT NULL,
    workspace_binding_json TEXT,
    plan_id TEXT,
    run_id TEXT,
    workspace_record_id TEXT,
    failure_code TEXT,
    failure_detail TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    created_by TEXT NOT NULL,
    UNIQUE (tenant_id, workspace_id, idempotency_key)
);
CREATE INDEX IF NOT EXISTS idx_product_tasks_status
    ON product_tasks(status, updated_at);
CREATE INDEX IF NOT EXISTS idx_product_tasks_target
    ON product_tasks(target_id, source_revision);
CREATE INDEX IF NOT EXISTS idx_product_tasks_run
    ON product_tasks(run_id);
CREATE INDEX IF NOT EXISTS idx_product_tasks_workspace_record
    ON product_tasks(workspace_record_id);
";

pub(super) const V31_DDL: &str = "
CREATE TABLE IF NOT EXISTS product_task_terminal_evidence (
    evidence_id TEXT PRIMARY KEY,
    product_task_id TEXT NOT NULL,
    tenant_id TEXT NOT NULL,
    workspace_id TEXT NOT NULL,
    task_version BIGINT NOT NULL,
    output_result_sha256 TEXT NOT NULL CHECK (length(output_result_sha256) = 64),
    artifact_id TEXT NOT NULL,
    approval_id TEXT NOT NULL,
    output_operation_id TEXT,
    output_receipt_id TEXT,
    audit_id BIGINT NOT NULL,
    content_sha256 TEXT NOT NULL CHECK (length(content_sha256) = 64),
    evidence_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    created_by TEXT NOT NULL,
    UNIQUE (product_task_id, task_version, output_result_sha256),
    FOREIGN KEY(product_task_id) REFERENCES product_tasks(task_id),
    FOREIGN KEY(audit_id) REFERENCES audit_log(audit_id)
);
CREATE INDEX IF NOT EXISTS idx_product_terminal_evidence_task
    ON product_task_terminal_evidence(product_task_id, task_version);
";

pub(super) const V32_DDL: &str = "
CREATE TABLE IF NOT EXISTS managed_acceptance_decisions (
    decision_id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    decision_body_sha256 TEXT NOT NULL CHECK (length(decision_body_sha256) = 64),
    residual_finding_sha256 TEXT NOT NULL CHECK (length(residual_finding_sha256) = 64),
    status TEXT NOT NULL CHECK (status IN ('draft_pending_operator','operator_accepted','operator_rejected','invalidated','revoked','expired')),
    principal_kind TEXT NOT NULL CHECK (principal_kind IN ('operator_api_key','fixture_principal')),
    principal_id TEXT,
    body_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    expires_at TEXT,
    revoked_at TEXT,
    UNIQUE (tenant_id, decision_body_sha256)
);
CREATE INDEX IF NOT EXISTS idx_managed_acceptance_decisions_tenant
    ON managed_acceptance_decisions(tenant_id, status, updated_at);

CREATE TABLE IF NOT EXISTS managed_acceptance_authorizations (
    authorization_id TEXT PRIMARY KEY,
    decision_id TEXT NOT NULL,
    tenant_id TEXT NOT NULL,
    principal_kind TEXT NOT NULL CHECK (principal_kind IN ('operator_api_key','fixture_principal')),
    principal_id TEXT NOT NULL,
    decision_body_sha256 TEXT NOT NULL CHECK (length(decision_body_sha256) = 64),
    residual_finding_sha256 TEXT NOT NULL CHECK (length(residual_finding_sha256) = 64),
    authorization_sha256 TEXT NOT NULL CHECK (length(authorization_sha256) = 64),
    scope_json TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('active','revoked','expired','consumed')),
    mutation_authority TEXT NOT NULL CHECK (mutation_authority = 'authorization_receipt_only'),
    execution_granted INTEGER NOT NULL CHECK (execution_granted = 0),
    body_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    revoked_at TEXT,
    UNIQUE (decision_id, principal_id, decision_body_sha256),
    FOREIGN KEY(decision_id) REFERENCES managed_acceptance_decisions(decision_id)
);
CREATE INDEX IF NOT EXISTS idx_managed_acceptance_auth_tenant
    ON managed_acceptance_authorizations(tenant_id, status, expires_at);

CREATE TABLE IF NOT EXISTS managed_acceptance_attempts (
    attempt_id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    product_task_id TEXT,
    workflow_node_id TEXT,
    execution_id TEXT NOT NULL,
    decision_id TEXT NOT NULL,
    authorization_id TEXT NOT NULL,
    manifest_sha256 TEXT NOT NULL CHECK (length(manifest_sha256) = 64),
    attempt_body_sha256 TEXT NOT NULL CHECK (length(attempt_body_sha256) = 64),
    status TEXT NOT NULL CHECK (status IN ('admitted','in_flight','succeeded','failed','cancelled','outcome_unknown','replayed')),
    terminal_class TEXT,
    body_json TEXT NOT NULL,
    receipt_json TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (tenant_id, attempt_id),
    UNIQUE (tenant_id, attempt_body_sha256),
    FOREIGN KEY(decision_id) REFERENCES managed_acceptance_decisions(decision_id),
    FOREIGN KEY(authorization_id) REFERENCES managed_acceptance_authorizations(authorization_id)
);
CREATE INDEX IF NOT EXISTS idx_managed_acceptance_attempts_status
    ON managed_acceptance_attempts(tenant_id, status, updated_at);

CREATE TABLE IF NOT EXISTS managed_acceptance_decision_transition_receipts (
    transition_receipt_id TEXT PRIMARY KEY,
    decision_id TEXT NOT NULL,
    tenant_id TEXT NOT NULL,
    decision_body_sha256 TEXT NOT NULL CHECK (length(decision_body_sha256) = 64),
    sequence INTEGER NOT NULL DEFAULT 1 CHECK (sequence >= 1),
    previous_transition_sequence INTEGER CHECK (previous_transition_sequence IS NULL OR previous_transition_sequence >= 1),
    previous_transition_sha256 TEXT,
    transition_sha256 TEXT NOT NULL CHECK (length(transition_sha256) = 64),
    from_status TEXT NOT NULL,
    to_status TEXT NOT NULL,
    actor_principal_kind TEXT NOT NULL,
    actor_principal_id TEXT NOT NULL,
    receipt_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    UNIQUE (decision_id, transition_sha256),
    FOREIGN KEY(decision_id) REFERENCES managed_acceptance_decisions(decision_id)
);
CREATE INDEX IF NOT EXISTS idx_managed_acceptance_transition_decision
    ON managed_acceptance_decision_transition_receipts(decision_id, created_at);
CREATE UNIQUE INDEX IF NOT EXISTS idx_managed_acceptance_transition_one_child
    ON managed_acceptance_decision_transition_receipts(decision_id, previous_transition_sha256)
    WHERE previous_transition_sha256 IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS idx_managed_acceptance_transition_one_genesis
    ON managed_acceptance_decision_transition_receipts(decision_id)
    WHERE previous_transition_sha256 IS NULL;
CREATE UNIQUE INDEX IF NOT EXISTS idx_managed_acceptance_transition_sequence
    ON managed_acceptance_decision_transition_receipts(decision_id, sequence);
";

pub(super) const V33_DDL: &str = "
CREATE TABLE IF NOT EXISTS managed_acceptance_spend_authorizations (
    spend_authorization_id TEXT PRIMARY KEY,
    decision_id TEXT NOT NULL,
    risk_authorization_id TEXT NOT NULL,
    tenant_id TEXT NOT NULL,
    principal_kind TEXT NOT NULL CHECK (principal_kind IN ('operator_api_key','fixture_principal')),
    principal_id TEXT NOT NULL,
    spend_body_sha256 TEXT NOT NULL CHECK (length(spend_body_sha256) = 64),
    logical_authorization_sha256 TEXT CHECK (logical_authorization_sha256 IS NULL OR length(logical_authorization_sha256) = 64),
    risk_authorization_sha256 TEXT NOT NULL CHECK (length(risk_authorization_sha256) = 64),
    decision_body_sha256 TEXT NOT NULL CHECK (length(decision_body_sha256) = 64),
    residual_finding_sha256 TEXT NOT NULL CHECK (length(residual_finding_sha256) = 64),
    fixture_only INTEGER NOT NULL CHECK (fixture_only IN (0,1)),
    status TEXT NOT NULL CHECK (status IN ('active','consumed','revoked','expired')),
    body_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    consumed_at TEXT,
    consumed_by_attempt_id TEXT,
    revoked_at TEXT,
    CHECK (status <> 'active' OR logical_authorization_sha256 IS NOT NULL),
    UNIQUE (tenant_id, spend_body_sha256),
    FOREIGN KEY(decision_id) REFERENCES managed_acceptance_decisions(decision_id),
    FOREIGN KEY(risk_authorization_id) REFERENCES managed_acceptance_authorizations(authorization_id)
);
CREATE INDEX IF NOT EXISTS idx_managed_acceptance_spend_tenant
    ON managed_acceptance_spend_authorizations(tenant_id, status, expires_at);
CREATE UNIQUE INDEX IF NOT EXISTS idx_managed_acceptance_spend_active_logical
    ON managed_acceptance_spend_authorizations(tenant_id, logical_authorization_sha256)
    WHERE status = 'active';
";

pub(super) const V34_DDL: &str = "
CREATE TABLE IF NOT EXISTS rwe_run_authorizations (
    authorization_id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    principal_id TEXT NOT NULL,
    principal_kind TEXT NOT NULL,
    corpus_sha256 TEXT NOT NULL CHECK (length(corpus_sha256) = 64),
    body_sha256 TEXT NOT NULL CHECK (length(body_sha256) = 64),
    body_json TEXT NOT NULL,
    fixture_only INTEGER NOT NULL CHECK (fixture_only IN (0,1)),
    status TEXT NOT NULL CHECK (status IN ('active','consumed','revoked','expired')),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    consumed_at TEXT,
    consumed_by_run_id TEXT,
    revoked_at TEXT,
    UNIQUE (tenant_id, body_sha256)
);
CREATE INDEX IF NOT EXISTS idx_rwe_run_auth_tenant ON rwe_run_authorizations(tenant_id, status, expires_at);

CREATE TABLE IF NOT EXISTS rwe_runs (
    run_id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    authorization_id TEXT NOT NULL,
    corpus_sha256 TEXT NOT NULL CHECK (length(corpus_sha256) = 64),
    principal_id TEXT NOT NULL,
    status TEXT NOT NULL,
    evidence_json TEXT,
    evidence_sha256 TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY(authorization_id) REFERENCES rwe_run_authorizations(authorization_id)
);
CREATE INDEX IF NOT EXISTS idx_rwe_runs_tenant ON rwe_runs(tenant_id, status, updated_at);

CREATE TABLE IF NOT EXISTS rwe_task_attempts (
    task_attempt_id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL,
    task_id TEXT NOT NULL,
    definition_sha256 TEXT NOT NULL CHECK (length(definition_sha256) = 64),
    classification TEXT NOT NULL,
    evidence_json TEXT NOT NULL,
    evidence_sha256 TEXT NOT NULL CHECK (length(evidence_sha256) = 64),
    created_at TEXT NOT NULL,
    FOREIGN KEY(run_id) REFERENCES rwe_runs(run_id)
);
CREATE INDEX IF NOT EXISTS idx_rwe_task_attempts_run ON rwe_task_attempts(run_id, created_at);
";

pub(super) const V28_DDL: &str = "
CREATE TABLE IF NOT EXISTS harness_evolution_sealed_holdouts (
    vault_sha256 TEXT PRIMARY KEY CHECK (length(vault_sha256) = 64),
    family_id TEXT NOT NULL,
    preselected_entrant_limit INTEGER NOT NULL CHECK (preselected_entrant_limit BETWEEN 1 AND 3),
    body_json TEXT NOT NULL,
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_harness_evolution_sealed_family
    ON harness_evolution_sealed_holdouts(family_id, created_at);

CREATE TABLE IF NOT EXISTS harness_evolution_evaluations (
    evaluation_id TEXT PRIMARY KEY,
    candidate_id TEXT NOT NULL,
    lineage_id TEXT NOT NULL,
    active_version_id TEXT NOT NULL,
    active_version_hash TEXT NOT NULL CHECK (length(active_version_hash) = 64),
    evaluator_identity_hash TEXT NOT NULL CHECK (length(evaluator_identity_hash) = 64),
    family_id TEXT NOT NULL,
    budget_seed BIGINT NOT NULL,
    bundle_sha256 TEXT NOT NULL CHECK (length(bundle_sha256) = 64),
    sealed_entrant_count INTEGER NOT NULL,
    claims_improvement INTEGER NOT NULL CHECK (claims_improvement = 0),
    sealed_feedback_into_mutation INTEGER NOT NULL CHECK (sealed_feedback_into_mutation = 0),
    body_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    UNIQUE(candidate_id, budget_seed, family_id)
);
CREATE INDEX IF NOT EXISTS idx_harness_evolution_evaluations_lineage
    ON harness_evolution_evaluations(lineage_id, created_at);

CREATE TABLE IF NOT EXISTS harness_evolution_pareto_archive (
    archive_id TEXT PRIMARY KEY,
    evaluation_id TEXT NOT NULL,
    candidate_id TEXT NOT NULL,
    lineage_id TEXT NOT NULL,
    baseline TEXT NOT NULL,
    sequential_rank INTEGER NOT NULL,
    dominated INTEGER NOT NULL,
    entry_sha256 TEXT NOT NULL CHECK (length(entry_sha256) = 64),
    body_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY(evaluation_id) REFERENCES harness_evolution_evaluations(evaluation_id)
);
CREATE INDEX IF NOT EXISTS idx_harness_evolution_pareto_eval
    ON harness_evolution_pareto_archive(evaluation_id, sequential_rank);

CREATE TABLE IF NOT EXISTS harness_evolution_eval_receipts (
    receipt_id TEXT PRIMARY KEY,
    evaluation_id TEXT NOT NULL,
    candidate_id TEXT NOT NULL,
    terminal TEXT NOT NULL,
    bundle_sha256 TEXT NOT NULL CHECK (length(bundle_sha256) = 64),
    body_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY(evaluation_id) REFERENCES harness_evolution_evaluations(evaluation_id)
);
CREATE INDEX IF NOT EXISTS idx_harness_evolution_eval_receipts_eval
    ON harness_evolution_eval_receipts(evaluation_id, created_at);
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

CREATE TABLE IF NOT EXISTS recursive_execution_trees (
    root_run_id TEXT PRIMARY KEY,
    workflow_id TEXT NOT NULL,
    root_node_id TEXT NOT NULL,
    tree_schema_version TEXT NOT NULL,
    tree_json TEXT NOT NULL,
    version BIGINT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_recursive_execution_trees_workflow
    ON recursive_execution_trees(workflow_id, updated_at);

CREATE TABLE IF NOT EXISTS recursive_execution_nodes (
    node_id TEXT NOT NULL,
    root_run_id TEXT NOT NULL,
    parent_node_id TEXT,
    proposal_id TEXT,
    depth BIGINT NOT NULL CHECK (depth BETWEEN 0 AND 2),
    objective_fingerprint TEXT NOT NULL CHECK (length(objective_fingerprint) = 64),
    status TEXT NOT NULL,
    version BIGINT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY(root_run_id, node_id),
    UNIQUE(root_run_id, objective_fingerprint),
    FOREIGN KEY(root_run_id) REFERENCES recursive_execution_trees(root_run_id)
);
CREATE INDEX IF NOT EXISTS idx_recursive_execution_nodes_root
    ON recursive_execution_nodes(root_run_id, depth, node_id);
CREATE INDEX IF NOT EXISTS idx_recursive_execution_nodes_parent
    ON recursive_execution_nodes(root_run_id, parent_node_id, status, node_id);

CREATE TABLE IF NOT EXISTS harness_evolution_active_identity (
    active_version_id TEXT PRIMARY KEY,
    active_version_hash TEXT NOT NULL CHECK (length(active_version_hash) = 64),
    evaluator_identity_hash TEXT NOT NULL CHECK (length(evaluator_identity_hash) = 64),
    body_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS harness_evolution_proposals (
    proposal_id TEXT PRIMARY KEY,
    parent_candidate_id TEXT,
    active_version_id TEXT NOT NULL,
    active_version_hash TEXT NOT NULL CHECK (length(active_version_hash) = 64),
    evaluator_identity_hash TEXT NOT NULL CHECK (length(evaluator_identity_hash) = 64),
    proposal_body_sha256 TEXT NOT NULL CHECK (length(proposal_body_sha256) = 64),
    body_json TEXT NOT NULL,
    seed BIGINT NOT NULL,
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_harness_evolution_proposals_active
    ON harness_evolution_proposals(active_version_id, created_at);

CREATE TABLE IF NOT EXISTS harness_evolution_candidates (
    candidate_id TEXT PRIMARY KEY,
    lineage_id TEXT NOT NULL,
    parent_candidate_id TEXT,
    proposal_id TEXT NOT NULL,
    active_version_id TEXT NOT NULL,
    active_version_hash TEXT NOT NULL CHECK (length(active_version_hash) = 64),
    evaluator_identity_hash TEXT NOT NULL CHECK (length(evaluator_identity_hash) = 64),
    content_hash TEXT NOT NULL CHECK (length(content_hash) = 64),
    status TEXT NOT NULL,
    terminal_reason TEXT NOT NULL,
    workspace_id TEXT NOT NULL,
    workspace_rel_path TEXT NOT NULL,
    body_json TEXT NOT NULL,
    seed BIGINT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(lineage_id, content_hash),
    FOREIGN KEY(proposal_id) REFERENCES harness_evolution_proposals(proposal_id)
);
CREATE INDEX IF NOT EXISTS idx_harness_evolution_candidates_lineage
    ON harness_evolution_candidates(lineage_id, created_at, candidate_id);
CREATE INDEX IF NOT EXISTS idx_harness_evolution_candidates_parent
    ON harness_evolution_candidates(parent_candidate_id, status);

CREATE TABLE IF NOT EXISTS harness_evolution_receipts (
    receipt_id TEXT PRIMARY KEY,
    candidate_id TEXT NOT NULL,
    proposal_id TEXT NOT NULL,
    lineage_id TEXT NOT NULL,
    active_version_id TEXT NOT NULL,
    terminal_reason TEXT NOT NULL,
    content_hash TEXT NOT NULL CHECK (length(content_hash) = 64),
    body_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY(candidate_id) REFERENCES harness_evolution_candidates(candidate_id)
);
CREATE INDEX IF NOT EXISTS idx_harness_evolution_receipts_candidate
    ON harness_evolution_receipts(candidate_id, created_at);

CREATE TABLE IF NOT EXISTS harness_evolution_sealed_holdouts (
    vault_sha256 TEXT PRIMARY KEY CHECK (length(vault_sha256) = 64),
    family_id TEXT NOT NULL,
    preselected_entrant_limit INTEGER NOT NULL CHECK (preselected_entrant_limit BETWEEN 1 AND 3),
    body_json TEXT NOT NULL,
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_harness_evolution_sealed_family
    ON harness_evolution_sealed_holdouts(family_id, created_at);

CREATE TABLE IF NOT EXISTS harness_evolution_evaluations (
    evaluation_id TEXT PRIMARY KEY,
    candidate_id TEXT NOT NULL,
    lineage_id TEXT NOT NULL,
    active_version_id TEXT NOT NULL,
    active_version_hash TEXT NOT NULL CHECK (length(active_version_hash) = 64),
    evaluator_identity_hash TEXT NOT NULL CHECK (length(evaluator_identity_hash) = 64),
    family_id TEXT NOT NULL,
    budget_seed BIGINT NOT NULL,
    bundle_sha256 TEXT NOT NULL CHECK (length(bundle_sha256) = 64),
    sealed_entrant_count INTEGER NOT NULL,
    claims_improvement INTEGER NOT NULL CHECK (claims_improvement = 0),
    sealed_feedback_into_mutation INTEGER NOT NULL CHECK (sealed_feedback_into_mutation = 0),
    body_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    UNIQUE(candidate_id, budget_seed, family_id)
);
CREATE INDEX IF NOT EXISTS idx_harness_evolution_evaluations_lineage
    ON harness_evolution_evaluations(lineage_id, created_at);

CREATE TABLE IF NOT EXISTS harness_evolution_pareto_archive (
    archive_id TEXT PRIMARY KEY,
    evaluation_id TEXT NOT NULL,
    candidate_id TEXT NOT NULL,
    lineage_id TEXT NOT NULL,
    baseline TEXT NOT NULL,
    sequential_rank INTEGER NOT NULL,
    dominated INTEGER NOT NULL,
    entry_sha256 TEXT NOT NULL CHECK (length(entry_sha256) = 64),
    body_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY(evaluation_id) REFERENCES harness_evolution_evaluations(evaluation_id)
);
CREATE INDEX IF NOT EXISTS idx_harness_evolution_pareto_eval
    ON harness_evolution_pareto_archive(evaluation_id, sequential_rank);

CREATE TABLE IF NOT EXISTS harness_evolution_eval_receipts (
    receipt_id TEXT PRIMARY KEY,
    evaluation_id TEXT NOT NULL,
    candidate_id TEXT NOT NULL,
    terminal TEXT NOT NULL,
    bundle_sha256 TEXT NOT NULL CHECK (length(bundle_sha256) = 64),
    body_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY(evaluation_id) REFERENCES harness_evolution_evaluations(evaluation_id)
);
CREATE INDEX IF NOT EXISTS idx_harness_evolution_eval_receipts_eval
    ON harness_evolution_eval_receipts(evaluation_id, created_at);


CREATE TABLE IF NOT EXISTS harness_evolution_pr_ready_bundles (
    bundle_id TEXT PRIMARY KEY,
    candidate_id TEXT NOT NULL,
    lineage_id TEXT NOT NULL,
    active_version_id TEXT NOT NULL,
    evaluation_id TEXT NOT NULL,
    patch_sha256 TEXT NOT NULL CHECK (length(patch_sha256) = 64),
    base_commit_sha TEXT NOT NULL CHECK (length(base_commit_sha) = 64),
    head_commit_sha TEXT NOT NULL CHECK (length(head_commit_sha) = 64),
    bundle_sha256 TEXT NOT NULL CHECK (length(bundle_sha256) = 64),
    terminal TEXT NOT NULL,
    body_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    UNIQUE(candidate_id, evaluation_id, patch_sha256)
);
CREATE INDEX IF NOT EXISTS idx_harness_evolution_pr_ready_candidate
    ON harness_evolution_pr_ready_bundles(candidate_id, created_at);

CREATE TABLE IF NOT EXISTS harness_evolution_pr_ready_receipts (
    receipt_id TEXT PRIMARY KEY,
    bundle_id TEXT NOT NULL,
    candidate_id TEXT NOT NULL,
    terminal TEXT NOT NULL,
    bundle_sha256 TEXT NOT NULL CHECK (length(bundle_sha256) = 64),
    body_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY(bundle_id) REFERENCES harness_evolution_pr_ready_bundles(bundle_id)
);
CREATE INDEX IF NOT EXISTS idx_harness_evolution_pr_ready_receipts_bundle
    ON harness_evolution_pr_ready_receipts(bundle_id, created_at);

CREATE TABLE IF NOT EXISTS product_tasks (
    task_id TEXT PRIMARY KEY,
    schema_version TEXT NOT NULL,
    tenant_id TEXT NOT NULL,
    workspace_id TEXT NOT NULL,
    idempotency_key TEXT NOT NULL,
    status TEXT NOT NULL,
    version BIGINT NOT NULL,
    objective_fingerprint TEXT NOT NULL CHECK (length(objective_fingerprint) = 64),
    target_id TEXT NOT NULL,
    target_repo_path TEXT NOT NULL,
    source_revision TEXT NOT NULL,
    source_tree_hash TEXT,
    output_intent TEXT NOT NULL,
    risk_class TEXT NOT NULL,
    approval_required INTEGER NOT NULL,
    confirm_execution INTEGER NOT NULL,
    confirm_output INTEGER NOT NULL,
    intake_contract_sha256 TEXT NOT NULL CHECK (length(intake_contract_sha256) = 64),
    intake_json TEXT NOT NULL,
    workspace_binding_json TEXT,
    plan_id TEXT,
    run_id TEXT,
    workspace_record_id TEXT,
    failure_code TEXT,
    failure_detail TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    created_by TEXT NOT NULL,
    UNIQUE (tenant_id, workspace_id, idempotency_key)
);
CREATE INDEX IF NOT EXISTS idx_product_tasks_status
    ON product_tasks(status, updated_at);
CREATE INDEX IF NOT EXISTS idx_product_tasks_target
    ON product_tasks(target_id, source_revision);
CREATE INDEX IF NOT EXISTS idx_product_tasks_run
    ON product_tasks(run_id);
CREATE INDEX IF NOT EXISTS idx_product_tasks_workspace_record
    ON product_tasks(workspace_record_id);

CREATE TABLE IF NOT EXISTS product_task_terminal_evidence (
    evidence_id TEXT PRIMARY KEY,
    product_task_id TEXT NOT NULL,
    tenant_id TEXT NOT NULL,
    workspace_id TEXT NOT NULL,
    task_version BIGINT NOT NULL,
    output_result_sha256 TEXT NOT NULL CHECK (length(output_result_sha256) = 64),
    artifact_id TEXT NOT NULL,
    approval_id TEXT NOT NULL,
    output_operation_id TEXT,
    output_receipt_id TEXT,
    audit_id BIGINT NOT NULL,
    content_sha256 TEXT NOT NULL CHECK (length(content_sha256) = 64),
    evidence_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    created_by TEXT NOT NULL,
    UNIQUE (product_task_id, task_version, output_result_sha256),
    FOREIGN KEY(product_task_id) REFERENCES product_tasks(task_id),
    FOREIGN KEY(audit_id) REFERENCES audit_log(audit_id)
);
CREATE INDEX IF NOT EXISTS idx_product_terminal_evidence_task
    ON product_task_terminal_evidence(product_task_id, task_version);

CREATE TABLE IF NOT EXISTS managed_acceptance_decisions (
    decision_id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    decision_body_sha256 TEXT NOT NULL CHECK (length(decision_body_sha256) = 64),
    residual_finding_sha256 TEXT NOT NULL CHECK (length(residual_finding_sha256) = 64),
    status TEXT NOT NULL CHECK (status IN ('draft_pending_operator','operator_accepted','operator_rejected','invalidated','revoked','expired')),
    principal_kind TEXT NOT NULL CHECK (principal_kind IN ('operator_api_key','fixture_principal')),
    principal_id TEXT,
    body_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    expires_at TEXT,
    revoked_at TEXT,
    UNIQUE (tenant_id, decision_body_sha256)
);
CREATE INDEX IF NOT EXISTS idx_managed_acceptance_decisions_tenant
    ON managed_acceptance_decisions(tenant_id, status, updated_at);

CREATE TABLE IF NOT EXISTS managed_acceptance_authorizations (
    authorization_id TEXT PRIMARY KEY,
    decision_id TEXT NOT NULL,
    tenant_id TEXT NOT NULL,
    principal_kind TEXT NOT NULL CHECK (principal_kind IN ('operator_api_key','fixture_principal')),
    principal_id TEXT NOT NULL,
    decision_body_sha256 TEXT NOT NULL CHECK (length(decision_body_sha256) = 64),
    residual_finding_sha256 TEXT NOT NULL CHECK (length(residual_finding_sha256) = 64),
    authorization_sha256 TEXT NOT NULL CHECK (length(authorization_sha256) = 64),
    scope_json TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('active','revoked','expired','consumed')),
    mutation_authority TEXT NOT NULL CHECK (mutation_authority = 'authorization_receipt_only'),
    execution_granted INTEGER NOT NULL CHECK (execution_granted = 0),
    body_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    revoked_at TEXT,
    UNIQUE (decision_id, principal_id, decision_body_sha256),
    FOREIGN KEY(decision_id) REFERENCES managed_acceptance_decisions(decision_id)
);
CREATE INDEX IF NOT EXISTS idx_managed_acceptance_auth_tenant
    ON managed_acceptance_authorizations(tenant_id, status, expires_at);

CREATE TABLE IF NOT EXISTS managed_acceptance_spend_authorizations (
    spend_authorization_id TEXT PRIMARY KEY,
    decision_id TEXT NOT NULL,
    risk_authorization_id TEXT NOT NULL,
    tenant_id TEXT NOT NULL,
    principal_kind TEXT NOT NULL CHECK (principal_kind IN ('operator_api_key','fixture_principal')),
    principal_id TEXT NOT NULL,
    spend_body_sha256 TEXT NOT NULL CHECK (length(spend_body_sha256) = 64),
    logical_authorization_sha256 TEXT CHECK (logical_authorization_sha256 IS NULL OR length(logical_authorization_sha256) = 64),
    risk_authorization_sha256 TEXT NOT NULL CHECK (length(risk_authorization_sha256) = 64),
    decision_body_sha256 TEXT NOT NULL CHECK (length(decision_body_sha256) = 64),
    residual_finding_sha256 TEXT NOT NULL CHECK (length(residual_finding_sha256) = 64),
    fixture_only INTEGER NOT NULL CHECK (fixture_only IN (0,1)),
    status TEXT NOT NULL CHECK (status IN ('active','consumed','revoked','expired')),
    body_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    consumed_at TEXT,
    consumed_by_attempt_id TEXT,
    revoked_at TEXT,
    CHECK (status <> 'active' OR logical_authorization_sha256 IS NOT NULL),
    UNIQUE (tenant_id, spend_body_sha256),
    FOREIGN KEY(decision_id) REFERENCES managed_acceptance_decisions(decision_id),
    FOREIGN KEY(risk_authorization_id) REFERENCES managed_acceptance_authorizations(authorization_id)
);
CREATE INDEX IF NOT EXISTS idx_managed_acceptance_spend_tenant
    ON managed_acceptance_spend_authorizations(tenant_id, status, expires_at);

CREATE TABLE IF NOT EXISTS managed_acceptance_attempts (
    attempt_id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    product_task_id TEXT,
    workflow_node_id TEXT,
    execution_id TEXT NOT NULL,
    decision_id TEXT NOT NULL,
    authorization_id TEXT NOT NULL,
    spend_authorization_id TEXT,
    manifest_sha256 TEXT NOT NULL CHECK (length(manifest_sha256) = 64),
    attempt_body_sha256 TEXT NOT NULL CHECK (length(attempt_body_sha256) = 64),
    status TEXT NOT NULL CHECK (status IN ('admitted','in_flight','succeeded','failed','cancelled','outcome_unknown','replayed')),
    terminal_class TEXT,
    body_json TEXT NOT NULL,
    receipt_json TEXT,
    receipt_sha256 TEXT,
    lease_token TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (tenant_id, attempt_id),
    UNIQUE (tenant_id, attempt_body_sha256),
    FOREIGN KEY(decision_id) REFERENCES managed_acceptance_decisions(decision_id),
    FOREIGN KEY(authorization_id) REFERENCES managed_acceptance_authorizations(authorization_id)
);
CREATE INDEX IF NOT EXISTS idx_managed_acceptance_attempts_status
    ON managed_acceptance_attempts(tenant_id, status, updated_at);

CREATE UNIQUE INDEX IF NOT EXISTS idx_managed_acceptance_spend_active_logical
    ON managed_acceptance_spend_authorizations(tenant_id, logical_authorization_sha256)
    WHERE status = 'active';

CREATE TABLE IF NOT EXISTS managed_acceptance_decision_transition_receipts (
    transition_receipt_id TEXT PRIMARY KEY,
    decision_id TEXT NOT NULL,
    tenant_id TEXT NOT NULL,
    decision_body_sha256 TEXT NOT NULL CHECK (length(decision_body_sha256) = 64),
    sequence INTEGER NOT NULL DEFAULT 1 CHECK (sequence >= 1),
    previous_transition_sequence INTEGER CHECK (previous_transition_sequence IS NULL OR previous_transition_sequence >= 1),
    previous_transition_sha256 TEXT,
    transition_sha256 TEXT NOT NULL CHECK (length(transition_sha256) = 64),
    from_status TEXT NOT NULL,
    to_status TEXT NOT NULL,
    actor_principal_kind TEXT NOT NULL,
    actor_principal_id TEXT NOT NULL,
    receipt_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    UNIQUE (decision_id, transition_sha256),
    FOREIGN KEY(decision_id) REFERENCES managed_acceptance_decisions(decision_id)
);
CREATE INDEX IF NOT EXISTS idx_managed_acceptance_transition_decision
    ON managed_acceptance_decision_transition_receipts(decision_id, created_at);
CREATE UNIQUE INDEX IF NOT EXISTS idx_managed_acceptance_transition_one_child
    ON managed_acceptance_decision_transition_receipts(decision_id, previous_transition_sha256)
    WHERE previous_transition_sha256 IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS idx_managed_acceptance_transition_one_genesis
    ON managed_acceptance_decision_transition_receipts(decision_id)
    WHERE previous_transition_sha256 IS NULL;

CREATE TABLE IF NOT EXISTS rwe_run_authorizations (
    authorization_id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    principal_id TEXT NOT NULL,
    principal_kind TEXT NOT NULL,
    corpus_sha256 TEXT NOT NULL CHECK (length(corpus_sha256) = 64),
    body_sha256 TEXT NOT NULL CHECK (length(body_sha256) = 64),
    body_json TEXT NOT NULL,
    fixture_only INTEGER NOT NULL CHECK (fixture_only IN (0,1)),
    status TEXT NOT NULL CHECK (status IN ('active','consumed','revoked','expired')),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    consumed_at TEXT,
    consumed_by_run_id TEXT,
    revoked_at TEXT,
    UNIQUE (tenant_id, body_sha256)
);
CREATE INDEX IF NOT EXISTS idx_rwe_run_auth_tenant ON rwe_run_authorizations(tenant_id, status, expires_at);

CREATE TABLE IF NOT EXISTS rwe_runs (
    run_id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    authorization_id TEXT NOT NULL,
    corpus_sha256 TEXT NOT NULL CHECK (length(corpus_sha256) = 64),
    principal_id TEXT NOT NULL,
    status TEXT NOT NULL,
    evidence_json TEXT,
    evidence_sha256 TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY(authorization_id) REFERENCES rwe_run_authorizations(authorization_id)
);
CREATE INDEX IF NOT EXISTS idx_rwe_runs_tenant ON rwe_runs(tenant_id, status, updated_at);

CREATE TABLE IF NOT EXISTS rwe_task_attempts (
    task_attempt_id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL,
    task_id TEXT NOT NULL,
    definition_sha256 TEXT NOT NULL CHECK (length(definition_sha256) = 64),
    classification TEXT NOT NULL,
    evidence_json TEXT NOT NULL,
    evidence_sha256 TEXT NOT NULL CHECK (length(evidence_sha256) = 64),
    created_at TEXT NOT NULL,
    FOREIGN KEY(run_id) REFERENCES rwe_runs(run_id)
);
CREATE INDEX IF NOT EXISTS idx_rwe_task_attempts_run ON rwe_task_attempts(run_id, created_at);
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

CREATE TABLE IF NOT EXISTS recursive_execution_trees (
    root_run_id TEXT PRIMARY KEY,
    workflow_id TEXT NOT NULL,
    root_node_id TEXT NOT NULL,
    tree_schema_version TEXT NOT NULL,
    tree_json TEXT NOT NULL,
    version BIGINT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_recursive_execution_trees_workflow
    ON recursive_execution_trees(workflow_id, updated_at);

CREATE TABLE IF NOT EXISTS recursive_execution_nodes (
    node_id TEXT NOT NULL,
    root_run_id TEXT NOT NULL,
    parent_node_id TEXT,
    proposal_id TEXT,
    depth BIGINT NOT NULL CHECK (depth BETWEEN 0 AND 2),
    objective_fingerprint TEXT NOT NULL CHECK (length(objective_fingerprint) = 64),
    status TEXT NOT NULL,
    version BIGINT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY(root_run_id, node_id),
    UNIQUE(root_run_id, objective_fingerprint),
    FOREIGN KEY(root_run_id) REFERENCES recursive_execution_trees(root_run_id)
);
CREATE INDEX IF NOT EXISTS idx_recursive_execution_nodes_root
    ON recursive_execution_nodes(root_run_id, depth, node_id);
CREATE INDEX IF NOT EXISTS idx_recursive_execution_nodes_parent
    ON recursive_execution_nodes(root_run_id, parent_node_id, status, node_id);

CREATE TABLE IF NOT EXISTS harness_evolution_active_identity (
    active_version_id TEXT PRIMARY KEY,
    active_version_hash TEXT NOT NULL CHECK (length(active_version_hash) = 64),
    evaluator_identity_hash TEXT NOT NULL CHECK (length(evaluator_identity_hash) = 64),
    body_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS harness_evolution_proposals (
    proposal_id TEXT PRIMARY KEY,
    parent_candidate_id TEXT,
    active_version_id TEXT NOT NULL,
    active_version_hash TEXT NOT NULL CHECK (length(active_version_hash) = 64),
    evaluator_identity_hash TEXT NOT NULL CHECK (length(evaluator_identity_hash) = 64),
    proposal_body_sha256 TEXT NOT NULL CHECK (length(proposal_body_sha256) = 64),
    body_json TEXT NOT NULL,
    seed BIGINT NOT NULL,
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_harness_evolution_proposals_active
    ON harness_evolution_proposals(active_version_id, created_at);

CREATE TABLE IF NOT EXISTS harness_evolution_candidates (
    candidate_id TEXT PRIMARY KEY,
    lineage_id TEXT NOT NULL,
    parent_candidate_id TEXT,
    proposal_id TEXT NOT NULL,
    active_version_id TEXT NOT NULL,
    active_version_hash TEXT NOT NULL CHECK (length(active_version_hash) = 64),
    evaluator_identity_hash TEXT NOT NULL CHECK (length(evaluator_identity_hash) = 64),
    content_hash TEXT NOT NULL CHECK (length(content_hash) = 64),
    status TEXT NOT NULL,
    terminal_reason TEXT NOT NULL,
    workspace_id TEXT NOT NULL,
    workspace_rel_path TEXT NOT NULL,
    body_json TEXT NOT NULL,
    seed BIGINT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(lineage_id, content_hash),
    FOREIGN KEY(proposal_id) REFERENCES harness_evolution_proposals(proposal_id)
);
CREATE INDEX IF NOT EXISTS idx_harness_evolution_candidates_lineage
    ON harness_evolution_candidates(lineage_id, created_at, candidate_id);
CREATE INDEX IF NOT EXISTS idx_harness_evolution_candidates_parent
    ON harness_evolution_candidates(parent_candidate_id, status);

CREATE TABLE IF NOT EXISTS harness_evolution_receipts (
    receipt_id TEXT PRIMARY KEY,
    candidate_id TEXT NOT NULL,
    proposal_id TEXT NOT NULL,
    lineage_id TEXT NOT NULL,
    active_version_id TEXT NOT NULL,
    terminal_reason TEXT NOT NULL,
    content_hash TEXT NOT NULL CHECK (length(content_hash) = 64),
    body_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY(candidate_id) REFERENCES harness_evolution_candidates(candidate_id)
);
CREATE INDEX IF NOT EXISTS idx_harness_evolution_receipts_candidate
    ON harness_evolution_receipts(candidate_id, created_at);

CREATE TABLE IF NOT EXISTS harness_evolution_sealed_holdouts (
    vault_sha256 TEXT PRIMARY KEY CHECK (length(vault_sha256) = 64),
    family_id TEXT NOT NULL,
    preselected_entrant_limit INTEGER NOT NULL CHECK (preselected_entrant_limit BETWEEN 1 AND 3),
    body_json TEXT NOT NULL,
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_harness_evolution_sealed_family
    ON harness_evolution_sealed_holdouts(family_id, created_at);

CREATE TABLE IF NOT EXISTS harness_evolution_evaluations (
    evaluation_id TEXT PRIMARY KEY,
    candidate_id TEXT NOT NULL,
    lineage_id TEXT NOT NULL,
    active_version_id TEXT NOT NULL,
    active_version_hash TEXT NOT NULL CHECK (length(active_version_hash) = 64),
    evaluator_identity_hash TEXT NOT NULL CHECK (length(evaluator_identity_hash) = 64),
    family_id TEXT NOT NULL,
    budget_seed BIGINT NOT NULL,
    bundle_sha256 TEXT NOT NULL CHECK (length(bundle_sha256) = 64),
    sealed_entrant_count INTEGER NOT NULL,
    claims_improvement INTEGER NOT NULL CHECK (claims_improvement = 0),
    sealed_feedback_into_mutation INTEGER NOT NULL CHECK (sealed_feedback_into_mutation = 0),
    body_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    UNIQUE(candidate_id, budget_seed, family_id)
);
CREATE INDEX IF NOT EXISTS idx_harness_evolution_evaluations_lineage
    ON harness_evolution_evaluations(lineage_id, created_at);

CREATE TABLE IF NOT EXISTS harness_evolution_pareto_archive (
    archive_id TEXT PRIMARY KEY,
    evaluation_id TEXT NOT NULL,
    candidate_id TEXT NOT NULL,
    lineage_id TEXT NOT NULL,
    baseline TEXT NOT NULL,
    sequential_rank INTEGER NOT NULL,
    dominated INTEGER NOT NULL,
    entry_sha256 TEXT NOT NULL CHECK (length(entry_sha256) = 64),
    body_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY(evaluation_id) REFERENCES harness_evolution_evaluations(evaluation_id)
);
CREATE INDEX IF NOT EXISTS idx_harness_evolution_pareto_eval
    ON harness_evolution_pareto_archive(evaluation_id, sequential_rank);

CREATE TABLE IF NOT EXISTS harness_evolution_eval_receipts (
    receipt_id TEXT PRIMARY KEY,
    evaluation_id TEXT NOT NULL,
    candidate_id TEXT NOT NULL,
    terminal TEXT NOT NULL,
    bundle_sha256 TEXT NOT NULL CHECK (length(bundle_sha256) = 64),
    body_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY(evaluation_id) REFERENCES harness_evolution_evaluations(evaluation_id)
);
CREATE INDEX IF NOT EXISTS idx_harness_evolution_eval_receipts_eval
    ON harness_evolution_eval_receipts(evaluation_id, created_at);

CREATE TABLE IF NOT EXISTS harness_evolution_pr_ready_bundles (
    bundle_id TEXT PRIMARY KEY,
    candidate_id TEXT NOT NULL,
    lineage_id TEXT NOT NULL,
    active_version_id TEXT NOT NULL,
    evaluation_id TEXT NOT NULL,
    patch_sha256 TEXT NOT NULL CHECK (length(patch_sha256) = 64),
    base_commit_sha TEXT NOT NULL CHECK (length(base_commit_sha) = 64),
    head_commit_sha TEXT NOT NULL CHECK (length(head_commit_sha) = 64),
    bundle_sha256 TEXT NOT NULL CHECK (length(bundle_sha256) = 64),
    terminal TEXT NOT NULL,
    body_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    UNIQUE(candidate_id, evaluation_id, patch_sha256)
);
CREATE INDEX IF NOT EXISTS idx_harness_evolution_pr_ready_candidate
    ON harness_evolution_pr_ready_bundles(candidate_id, created_at);

CREATE TABLE IF NOT EXISTS harness_evolution_pr_ready_receipts (
    receipt_id TEXT PRIMARY KEY,
    bundle_id TEXT NOT NULL,
    candidate_id TEXT NOT NULL,
    terminal TEXT NOT NULL,
    bundle_sha256 TEXT NOT NULL CHECK (length(bundle_sha256) = 64),
    body_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY(bundle_id) REFERENCES harness_evolution_pr_ready_bundles(bundle_id)
);
CREATE INDEX IF NOT EXISTS idx_harness_evolution_pr_ready_receipts_bundle
    ON harness_evolution_pr_ready_receipts(bundle_id, created_at);

CREATE TABLE IF NOT EXISTS product_tasks (
    task_id TEXT PRIMARY KEY,
    schema_version TEXT NOT NULL,
    tenant_id TEXT NOT NULL,
    workspace_id TEXT NOT NULL,
    idempotency_key TEXT NOT NULL,
    status TEXT NOT NULL,
    version BIGINT NOT NULL,
    objective_fingerprint TEXT NOT NULL CHECK (length(objective_fingerprint) = 64),
    target_id TEXT NOT NULL,
    target_repo_path TEXT NOT NULL,
    source_revision TEXT NOT NULL,
    source_tree_hash TEXT,
    output_intent TEXT NOT NULL,
    risk_class TEXT NOT NULL,
    approval_required INTEGER NOT NULL,
    confirm_execution INTEGER NOT NULL,
    confirm_output INTEGER NOT NULL,
    intake_contract_sha256 TEXT NOT NULL CHECK (length(intake_contract_sha256) = 64),
    intake_json TEXT NOT NULL,
    workspace_binding_json TEXT,
    plan_id TEXT,
    run_id TEXT,
    workspace_record_id TEXT,
    failure_code TEXT,
    failure_detail TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    created_by TEXT NOT NULL,
    UNIQUE (tenant_id, workspace_id, idempotency_key)
);
CREATE INDEX IF NOT EXISTS idx_product_tasks_status
    ON product_tasks(status, updated_at);
CREATE INDEX IF NOT EXISTS idx_product_tasks_target
    ON product_tasks(target_id, source_revision);
CREATE INDEX IF NOT EXISTS idx_product_tasks_run
    ON product_tasks(run_id);
CREATE INDEX IF NOT EXISTS idx_product_tasks_workspace_record
    ON product_tasks(workspace_record_id);

CREATE TABLE IF NOT EXISTS product_task_terminal_evidence (
    evidence_id TEXT PRIMARY KEY,
    product_task_id TEXT NOT NULL,
    tenant_id TEXT NOT NULL,
    workspace_id TEXT NOT NULL,
    task_version BIGINT NOT NULL,
    output_result_sha256 TEXT NOT NULL CHECK (length(output_result_sha256) = 64),
    artifact_id TEXT NOT NULL,
    approval_id TEXT NOT NULL,
    output_operation_id TEXT,
    output_receipt_id TEXT,
    audit_id BIGINT NOT NULL,
    content_sha256 TEXT NOT NULL CHECK (length(content_sha256) = 64),
    evidence_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    created_by TEXT NOT NULL,
    UNIQUE (product_task_id, task_version, output_result_sha256),
    FOREIGN KEY(product_task_id) REFERENCES product_tasks(task_id),
    FOREIGN KEY(audit_id) REFERENCES audit_log(audit_id)
);
CREATE INDEX IF NOT EXISTS idx_product_terminal_evidence_task
    ON product_task_terminal_evidence(product_task_id, task_version);

CREATE TABLE IF NOT EXISTS managed_acceptance_decisions (
    decision_id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    decision_body_sha256 TEXT NOT NULL CHECK (length(decision_body_sha256) = 64),
    residual_finding_sha256 TEXT NOT NULL CHECK (length(residual_finding_sha256) = 64),
    status TEXT NOT NULL CHECK (status IN ('draft_pending_operator','operator_accepted','operator_rejected','invalidated','revoked','expired')),
    principal_kind TEXT NOT NULL CHECK (principal_kind IN ('operator_api_key','fixture_principal')),
    principal_id TEXT,
    body_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    expires_at TEXT,
    revoked_at TEXT,
    UNIQUE (tenant_id, decision_body_sha256)
);
CREATE INDEX IF NOT EXISTS idx_managed_acceptance_decisions_tenant
    ON managed_acceptance_decisions(tenant_id, status, updated_at);

CREATE TABLE IF NOT EXISTS managed_acceptance_authorizations (
    authorization_id TEXT PRIMARY KEY,
    decision_id TEXT NOT NULL,
    tenant_id TEXT NOT NULL,
    principal_kind TEXT NOT NULL CHECK (principal_kind IN ('operator_api_key','fixture_principal')),
    principal_id TEXT NOT NULL,
    decision_body_sha256 TEXT NOT NULL CHECK (length(decision_body_sha256) = 64),
    residual_finding_sha256 TEXT NOT NULL CHECK (length(residual_finding_sha256) = 64),
    authorization_sha256 TEXT NOT NULL CHECK (length(authorization_sha256) = 64),
    scope_json TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('active','revoked','expired','consumed')),
    mutation_authority TEXT NOT NULL CHECK (mutation_authority = 'authorization_receipt_only'),
    execution_granted INTEGER NOT NULL CHECK (execution_granted = 0),
    body_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    revoked_at TEXT,
    UNIQUE (decision_id, principal_id, decision_body_sha256),
    FOREIGN KEY(decision_id) REFERENCES managed_acceptance_decisions(decision_id)
);
CREATE INDEX IF NOT EXISTS idx_managed_acceptance_auth_tenant
    ON managed_acceptance_authorizations(tenant_id, status, expires_at);

CREATE TABLE IF NOT EXISTS managed_acceptance_spend_authorizations (
    spend_authorization_id TEXT PRIMARY KEY,
    decision_id TEXT NOT NULL,
    risk_authorization_id TEXT NOT NULL,
    tenant_id TEXT NOT NULL,
    principal_kind TEXT NOT NULL CHECK (principal_kind IN ('operator_api_key','fixture_principal')),
    principal_id TEXT NOT NULL,
    spend_body_sha256 TEXT NOT NULL CHECK (length(spend_body_sha256) = 64),
    logical_authorization_sha256 TEXT CHECK (logical_authorization_sha256 IS NULL OR length(logical_authorization_sha256) = 64),
    risk_authorization_sha256 TEXT NOT NULL CHECK (length(risk_authorization_sha256) = 64),
    decision_body_sha256 TEXT NOT NULL CHECK (length(decision_body_sha256) = 64),
    residual_finding_sha256 TEXT NOT NULL CHECK (length(residual_finding_sha256) = 64),
    fixture_only INTEGER NOT NULL CHECK (fixture_only IN (0,1)),
    status TEXT NOT NULL CHECK (status IN ('active','consumed','revoked','expired')),
    body_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    consumed_at TEXT,
    consumed_by_attempt_id TEXT,
    revoked_at TEXT,
    CHECK (status <> 'active' OR logical_authorization_sha256 IS NOT NULL),
    UNIQUE (tenant_id, spend_body_sha256),
    FOREIGN KEY(decision_id) REFERENCES managed_acceptance_decisions(decision_id),
    FOREIGN KEY(risk_authorization_id) REFERENCES managed_acceptance_authorizations(authorization_id)
);
CREATE INDEX IF NOT EXISTS idx_managed_acceptance_spend_tenant
    ON managed_acceptance_spend_authorizations(tenant_id, status, expires_at);

CREATE TABLE IF NOT EXISTS managed_acceptance_attempts (
    attempt_id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    product_task_id TEXT,
    workflow_node_id TEXT,
    execution_id TEXT NOT NULL,
    decision_id TEXT NOT NULL,
    authorization_id TEXT NOT NULL,
    spend_authorization_id TEXT,
    manifest_sha256 TEXT NOT NULL CHECK (length(manifest_sha256) = 64),
    attempt_body_sha256 TEXT NOT NULL CHECK (length(attempt_body_sha256) = 64),
    status TEXT NOT NULL CHECK (status IN ('admitted','in_flight','succeeded','failed','cancelled','outcome_unknown','replayed')),
    terminal_class TEXT,
    body_json TEXT NOT NULL,
    receipt_json TEXT,
    receipt_sha256 TEXT,
    lease_token TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (tenant_id, attempt_id),
    UNIQUE (tenant_id, attempt_body_sha256),
    FOREIGN KEY(decision_id) REFERENCES managed_acceptance_decisions(decision_id),
    FOREIGN KEY(authorization_id) REFERENCES managed_acceptance_authorizations(authorization_id)
);
CREATE INDEX IF NOT EXISTS idx_managed_acceptance_attempts_status
    ON managed_acceptance_attempts(tenant_id, status, updated_at);

CREATE UNIQUE INDEX IF NOT EXISTS idx_managed_acceptance_spend_active_logical
    ON managed_acceptance_spend_authorizations(tenant_id, logical_authorization_sha256)
    WHERE status = 'active';

CREATE TABLE IF NOT EXISTS managed_acceptance_decision_transition_receipts (
    transition_receipt_id TEXT PRIMARY KEY,
    decision_id TEXT NOT NULL,
    tenant_id TEXT NOT NULL,
    decision_body_sha256 TEXT NOT NULL CHECK (length(decision_body_sha256) = 64),
    sequence BIGINT NOT NULL DEFAULT 1 CHECK (sequence >= 1),
    previous_transition_sequence BIGINT CHECK (previous_transition_sequence IS NULL OR previous_transition_sequence >= 1),
    previous_transition_sha256 TEXT,
    transition_sha256 TEXT NOT NULL CHECK (length(transition_sha256) = 64),
    from_status TEXT NOT NULL,
    to_status TEXT NOT NULL,
    actor_principal_kind TEXT NOT NULL,
    actor_principal_id TEXT NOT NULL,
    receipt_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    UNIQUE (decision_id, transition_sha256),
    FOREIGN KEY(decision_id) REFERENCES managed_acceptance_decisions(decision_id)
);
CREATE INDEX IF NOT EXISTS idx_managed_acceptance_transition_decision
    ON managed_acceptance_decision_transition_receipts(decision_id, created_at);
CREATE UNIQUE INDEX IF NOT EXISTS idx_managed_acceptance_transition_one_child
    ON managed_acceptance_decision_transition_receipts(decision_id, previous_transition_sha256)
    WHERE previous_transition_sha256 IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS idx_managed_acceptance_transition_one_genesis
    ON managed_acceptance_decision_transition_receipts(decision_id)
    WHERE previous_transition_sha256 IS NULL;

CREATE TABLE IF NOT EXISTS rwe_run_authorizations (
    authorization_id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    principal_id TEXT NOT NULL,
    principal_kind TEXT NOT NULL,
    corpus_sha256 TEXT NOT NULL CHECK (length(corpus_sha256) = 64),
    body_sha256 TEXT NOT NULL CHECK (length(body_sha256) = 64),
    body_json TEXT NOT NULL,
    fixture_only INTEGER NOT NULL CHECK (fixture_only IN (0,1)),
    status TEXT NOT NULL CHECK (status IN ('active','consumed','revoked','expired')),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    consumed_at TEXT,
    consumed_by_run_id TEXT,
    revoked_at TEXT,
    UNIQUE (tenant_id, body_sha256)
);
CREATE INDEX IF NOT EXISTS idx_rwe_run_auth_tenant ON rwe_run_authorizations(tenant_id, status, expires_at);

CREATE TABLE IF NOT EXISTS rwe_runs (
    run_id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    authorization_id TEXT NOT NULL,
    corpus_sha256 TEXT NOT NULL CHECK (length(corpus_sha256) = 64),
    principal_id TEXT NOT NULL,
    status TEXT NOT NULL,
    evidence_json TEXT,
    evidence_sha256 TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY(authorization_id) REFERENCES rwe_run_authorizations(authorization_id)
);
CREATE INDEX IF NOT EXISTS idx_rwe_runs_tenant ON rwe_runs(tenant_id, status, updated_at);

CREATE TABLE IF NOT EXISTS rwe_task_attempts (
    task_attempt_id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL,
    task_id TEXT NOT NULL,
    definition_sha256 TEXT NOT NULL CHECK (length(definition_sha256) = 64),
    classification TEXT NOT NULL,
    evidence_json TEXT NOT NULL,
    evidence_sha256 TEXT NOT NULL CHECK (length(evidence_sha256) = 64),
    created_at TEXT NOT NULL,
    FOREIGN KEY(run_id) REFERENCES rwe_runs(run_id)
);
CREATE INDEX IF NOT EXISTS idx_rwe_task_attempts_run ON rwe_task_attempts(run_id, created_at);
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
            "recursive_execution_trees",
            "recursive_execution_nodes",
            "idx_recursive_execution_nodes_parent",
            "harness_evolution_active_identity",
            "harness_evolution_proposals",
            "harness_evolution_candidates",
            "harness_evolution_receipts",
            "idx_harness_evolution_candidates_lineage",
            "harness_evolution_sealed_holdouts",
            "harness_evolution_evaluations",
            "harness_evolution_pareto_archive",
            "harness_evolution_eval_receipts",
            "harness_evolution_pr_ready_bundles",
            "harness_evolution_pr_ready_receipts",
            "product_tasks",
            "idx_product_tasks_status",
            "idx_product_tasks_target",
            "idx_product_tasks_run",
            "idx_product_tasks_workspace_record",
            "product_task_terminal_evidence",
            "idx_product_terminal_evidence_task",
            "managed_acceptance_decisions",
            "managed_acceptance_authorizations",
            "managed_acceptance_attempts",
            "managed_acceptance_spend_authorizations",
            "idx_managed_acceptance_spend_active_logical",
            "managed_acceptance_decision_transition_receipts",
            "idx_managed_acceptance_transition_decision",
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
