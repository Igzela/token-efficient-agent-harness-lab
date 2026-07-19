use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};
use serde_json::{json, Value};

use super::{append_audit_locked, DatabaseConnection, LocalProductStore};
use crate::recursive_execution::{
    RecursiveBudget, RecursiveFailureReason, RecursiveNode, RecursiveScope, RecursiveTree,
    MAX_ACCEPTED_CHILDREN_PER_NODE, MAX_RECURSIVE_DECISION_EVIDENCE_REFS, MAX_RECURSIVE_DEPTH,
    MAX_RECURSIVE_EVIDENCE_REF_BYTES, MAX_RECURSIVE_LEASES, MAX_RECURSIVE_NODES_PER_ROOT,
    MAX_RECURSIVE_TREE_BYTES, MAX_SCOPE_ITEMS, MAX_SCOPE_VALUE_BYTES, RECURSIVE_SCHEMA_VERSION,
};

fn node_values(node: &RecursiveNode) -> (&str, Option<&str>, Option<&str>, i64, &str, &str, i64) {
    (
        &node.node_id,
        node.parent_node_id.as_deref(),
        node.proposal_id.as_deref(),
        i64::from(node.depth),
        &node.objective_fingerprint,
        &node.status,
        node.version as i64,
    )
}

fn validate_tree_for_persistence(tree: &RecursiveTree) -> Result<String, String> {
    if tree.schema_version != RECURSIVE_SCHEMA_VERSION {
        return Err("recursive tree schema version mismatch".to_string());
    }
    if tree.nodes.len() > MAX_RECURSIVE_NODES_PER_ROOT
        || tree.active_leases.len() > MAX_RECURSIVE_LEASES
    {
        return Err("recursive tree exceeds bounded node or lease limit".to_string());
    }
    validate_scope(&tree.root_scope)?;
    if tree.root_run_id.is_empty() || tree.workflow_id.is_empty() || tree.root_node_id.is_empty() {
        return Err("recursive tree identity is empty".to_string());
    }
    if tree.root_agent_id.is_empty()
        || tree.root_recursive_marker != tree.root_node_id
        || tree.root_creation_receipt_sha256.is_empty()
    {
        return Err("recursive root creation identity is incomplete".to_string());
    }
    if let Some(limit) = tree.root_budget_limit.as_ref() {
        let mut total = tree.spent_budget.clone();
        total.add(&tree.reserved_budget);
        let recorded_overrun = tree.execution_state
            == crate::recursive_execution::RecursiveExecutionState::BudgetExhausted
            && tree
                .usage_receipts
                .values()
                .any(|receipt| receipt.starts_with("0:"));
        if !limit.can_spend(&tree.root_budget) || (!limit.can_spend(&total) && !recorded_overrun) {
            return Err("recursive tree aggregate budget is inconsistent".to_string());
        }
    }
    let root = tree
        .nodes
        .get(&tree.root_node_id)
        .ok_or_else(|| "recursive tree root node is missing".to_string())?;
    if root.root_run_id != tree.root_run_id
        || root.parent_node_id.is_some()
        || root.depth != 0
        || root.scope != tree.root_scope
        || root.capabilities != tree.root_capabilities
        || root.tenant_id != tree.root_tenant_id
        || root.workspace_id != tree.root_workspace_id
    {
        return Err("recursive tree root identity is inconsistent".to_string());
    }
    let mut child_counts = std::collections::BTreeMap::<String, usize>::new();
    let mut fingerprints = std::collections::BTreeSet::new();
    let mut proposal_ids = std::collections::BTreeSet::new();
    for node in tree.nodes.values() {
        if node.depth > MAX_RECURSIVE_DEPTH
            || node.accepted_children > MAX_ACCEPTED_CHILDREN_PER_NODE
            || node.objective_fingerprint.len() != 64
            || node.ancestor_fingerprints.len() > MAX_RECURSIVE_DEPTH as usize
        {
            return Err("recursive node exceeds bounded shape".to_string());
        }
        if node.root_run_id != tree.root_run_id
            || node.node_id.is_empty()
            || (node.node_id != tree.root_node_id && node.parent_node_id.is_none())
            || node.scope.capabilities != node.capabilities
        {
            return Err("recursive node identity is inconsistent".to_string());
        }
        if !fingerprints.insert(node.objective_fingerprint.clone()) {
            return Err("recursive objective fingerprint is duplicated".to_string());
        }
        if node.node_id == tree.root_node_id && !node.ancestor_fingerprints.is_empty() {
            return Err("recursive root lineage is not empty".to_string());
        }
        if let Some(parent_node_id) = node.parent_node_id.as_deref() {
            let Some(parent) = tree.nodes.get(parent_node_id) else {
                return Err("recursive node parent is missing".to_string());
            };
            if node.depth != parent.depth.saturating_add(1)
                || node.ancestor_fingerprints
                    != parent
                        .ancestor_fingerprints
                        .iter()
                        .cloned()
                        .chain(std::iter::once(parent.objective_fingerprint.clone()))
                        .collect::<Vec<_>>()
                || !node.capabilities.is_subset(&parent.capabilities)
                || !node.scope.is_subset_of(&parent.scope)
                || node.tenant_id != parent.tenant_id
                || node.workspace_id != parent.workspace_id
            {
                return Err("recursive node lineage or authority is inconsistent".to_string());
            }
            *child_counts.entry(parent_node_id.to_string()).or_default() += 1;
        }
        if let Some(proposal_id) = node.proposal_id.as_deref() {
            if node.node_id
                != crate::recursive_execution::derived_node_id_for_persistence(
                    &tree.root_run_id,
                    proposal_id,
                    &node.objective_fingerprint,
                )
            {
                return Err("recursive child node identity is not deterministic".to_string());
            }
            if !proposal_ids.insert(proposal_id.to_string())
                || !tree.accepted_proposals.contains(proposal_id)
                || !tree.receipts.contains_key(proposal_id)
            {
                return Err("recursive proposal identity is inconsistent".to_string());
            }
        } else if node.node_id != tree.root_node_id {
            return Err("recursive child proposal identity is missing".to_string());
        }
        match (node.status.as_str(), node.lease_id.as_deref()) {
            ("leased", Some(lease_id)) if tree.active_leases.contains(lease_id) => {}
            ("leased", _) => return Err("recursive lease identity is inconsistent".to_string()),
            (_, Some(_)) => return Err("non-leased recursive node has a lease".to_string()),
            _ => {}
        }
        validate_scope(&node.scope)?;
        if node.evidence_refs.len() > MAX_RECURSIVE_DECISION_EVIDENCE_REFS
            || node
                .evidence_refs
                .iter()
                .any(|reference| reference.len() > MAX_RECURSIVE_EVIDENCE_REF_BYTES)
        {
            return Err("recursive node evidence exceeds bounded shape".to_string());
        }
    }
    if tree
        .nodes
        .values()
        .any(|node| node.accepted_children != child_counts.get(&node.node_id).copied().unwrap_or(0))
    {
        return Err("recursive child count is inconsistent".to_string());
    }
    if tree.active_leases.iter().any(|lease_id| {
        !tree
            .nodes
            .values()
            .any(|node| node.lease_id.as_deref() == Some(lease_id))
    }) {
        return Err("recursive active lease index is inconsistent".to_string());
    }
    if tree.accepted_proposals.len() != tree.receipts.len()
        || tree.accepted_proposals.iter().any(|proposal_id| {
            !proposal_ids.contains(proposal_id)
                || tree.receipts.get(proposal_id).is_none_or(String::is_empty)
        })
        || tree
            .accepted_proposals
            .iter()
            .any(|proposal_id| tree.rejected_proposals.contains_key(proposal_id))
    {
        return Err("recursive accepted proposal receipt set is inconsistent".to_string());
    }
    if tree.usage_receipts.len() > MAX_RECURSIVE_NODES_PER_ROOT * 2
        || tree.usage_receipts.keys().any(String::is_empty)
    {
        return Err("recursive usage receipt set exceeds bounded shape".to_string());
    }
    if tree.rejected_proposals.len() > MAX_RECURSIVE_NODES_PER_ROOT
        || tree.rejected_proposals.values().any(|decision| {
            decision.evidence_refs.len() > MAX_RECURSIVE_DECISION_EVIDENCE_REFS
                || decision
                    .evidence_refs
                    .iter()
                    .any(|reference| reference.len() > MAX_RECURSIVE_EVIDENCE_REF_BYTES)
                || decision.reason_code.len() > 128
        })
    {
        return Err("recursive decision evidence exceeds bounded shape".to_string());
    }
    let tree_json = serde_json::to_string(tree).map_err(|error| error.to_string())?;
    if tree_json.len() > MAX_RECURSIVE_TREE_BYTES {
        return Err("recursive tree exceeds persistence byte cap".to_string());
    }
    Ok(tree_json)
}

fn validate_scope(scope: &RecursiveScope) -> Result<(), String> {
    if scope.allowed_paths.len() > MAX_SCOPE_ITEMS || scope.capabilities.len() > MAX_SCOPE_ITEMS {
        return Err("recursive scope exceeds bounded item cap".to_string());
    }
    let mut values = scope
        .repository
        .iter()
        .chain(scope.allowed_paths.iter())
        .chain(scope.capabilities.iter());
    if values.any(|value| value.len() > MAX_SCOPE_VALUE_BYTES) {
        return Err("recursive scope value exceeds byte cap".to_string());
    }
    Ok(())
}

fn normalize_loaded_tree(tree: &mut RecursiveTree) {
    // Older v1 snapshots did not carry aggregate usage accounting. Rebind
    // them to their persisted remaining authority so restart stays fail-closed.
    if tree.root_budget_limit.is_none() {
        tree.root_budget_limit = Some(tree.root_budget.clone());
    }
}

fn check_expected_sqlite(
    conn: &rusqlite::Connection,
    root_run_id: &str,
    expected_version: u64,
) -> Result<(), String> {
    let current: Option<i64> = conn
        .query_row(
            "SELECT version FROM recursive_execution_trees WHERE root_run_id=?1",
            [root_run_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let matches = if expected_version == 0 {
        current.is_none()
    } else {
        current == Some(expected_version as i64)
    };
    if matches {
        Ok(())
    } else {
        Err("stale_parent".to_string())
    }
}

fn check_node_index_sqlite(
    conn: &rusqlite::Connection,
    node_id: &str,
    root_run_id: &str,
) -> Result<(), String> {
    let existing: Option<String> = conn
        .query_row(
            "SELECT root_run_id FROM recursive_execution_nodes WHERE node_id=?1",
            [node_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    if existing.is_some_and(|existing| existing != root_run_id) {
        return Err("recursive node identity is already bound to another root".to_string());
    }
    Ok(())
}

#[cfg(feature = "pg")]
fn check_node_index_pg(
    client: &mut impl postgres::GenericClient,
    node_id: &str,
    root_run_id: &str,
) -> Result<(), String> {
    let existing: Option<String> = client
        .query_opt(
            "SELECT root_run_id FROM recursive_execution_nodes WHERE node_id=$1",
            &[&node_id],
        )
        .map_err(|error| error.to_string())?
        .map(|row| row.get(0));
    if existing.is_some_and(|existing| existing != root_run_id) {
        return Err("recursive node identity is already bound to another root".to_string());
    }
    Ok(())
}

fn check_workflow_binding_sqlite(
    conn: &rusqlite::Connection,
    tree: &RecursiveTree,
) -> Result<(), String> {
    let workflow_id: Option<String> = conn
        .query_row(
            "SELECT workflow_id FROM workflow_runs WHERE run_id=?1",
            [tree.root_run_id.as_str()],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let workflow_id =
        workflow_id.ok_or_else(|| "recursive workflow run binding is missing".to_string())?;
    if workflow_id != tree.workflow_id {
        return Err("recursive workflow identity is not bound to workflow run".to_string());
    }
    let boundaries_json: String = conn
        .query_row(
            "SELECT boundaries_json FROM workflow_runs WHERE run_id=?1",
            [tree.root_run_id.as_str()],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    let boundaries: Value = serde_json::from_str(&boundaries_json)
        .map_err(|_| "recursive workflow boundaries are malformed".to_string())?;
    if boundaries.get("tenant_id").and_then(Value::as_str) != tree.root_tenant_id.as_deref()
        || boundaries.get("workspace_id").and_then(Value::as_str)
            != tree.root_workspace_id.as_deref()
    {
        return Err(
            "recursive tenant or workspace identity is not bound to workflow run".to_string(),
        );
    }
    let root_metadata: String = conn
        .query_row(
            "SELECT node_json FROM workflow_run_nodes
             WHERE run_id=?1 AND node_id=?2 AND task_type='agent_step'",
            params![tree.root_run_id, tree.root_node_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    let metadata: Value = serde_json::from_str(&root_metadata)
        .map_err(|_| "recursive root node identity is malformed".to_string())?;
    let root_marker = metadata
        .get("recursive_root_node_id")
        .or_else(|| metadata.get("recursive_node_id"))
        .and_then(Value::as_str);
    if metadata.get("agent_id").and_then(Value::as_str) != Some(tree.root_agent_id.as_str())
        || root_marker != Some(tree.root_recursive_marker.as_str())
        || metadata
            .get("creation_receipt_sha256")
            .and_then(Value::as_str)
            != Some(tree.root_creation_receipt_sha256.as_str())
    {
        return Err("recursive root node identity is not bound to workflow run".to_string());
    }
    Ok(())
}

#[cfg(feature = "pg")]
fn check_workflow_binding_pg(
    client: &mut impl postgres::GenericClient,
    tree: &RecursiveTree,
) -> Result<(), String> {
    let workflow_id: Option<String> = client
        .query_opt(
            "SELECT workflow_id FROM workflow_runs WHERE run_id=$1",
            &[&tree.root_run_id],
        )
        .map_err(|error| error.to_string())?
        .map(|row| row.get(0));
    let workflow_id =
        workflow_id.ok_or_else(|| "recursive workflow run binding is missing".to_string())?;
    if workflow_id != tree.workflow_id {
        return Err("recursive workflow identity is not bound to workflow run".to_string());
    }
    let boundaries_json: String = client
        .query_one(
            "SELECT boundaries_json FROM workflow_runs WHERE run_id=$1",
            &[&tree.root_run_id],
        )
        .map_err(|error| error.to_string())?
        .get(0);
    let boundaries: Value = serde_json::from_str(&boundaries_json)
        .map_err(|_| "recursive workflow boundaries are malformed".to_string())?;
    if boundaries.get("tenant_id").and_then(Value::as_str) != tree.root_tenant_id.as_deref()
        || boundaries.get("workspace_id").and_then(Value::as_str)
            != tree.root_workspace_id.as_deref()
    {
        return Err(
            "recursive tenant or workspace identity is not bound to workflow run".to_string(),
        );
    }
    let root_metadata: String = client
        .query_one(
            "SELECT node_json FROM workflow_run_nodes
             WHERE run_id=$1 AND node_id=$2 AND task_type='agent_step'",
            &[&tree.root_run_id, &tree.root_node_id],
        )
        .map_err(|error| error.to_string())?
        .get(0);
    let metadata: Value = serde_json::from_str(&root_metadata)
        .map_err(|_| "recursive root node identity is malformed".to_string())?;
    let root_marker = metadata
        .get("recursive_root_node_id")
        .or_else(|| metadata.get("recursive_node_id"))
        .and_then(Value::as_str);
    if metadata.get("agent_id").and_then(Value::as_str) != Some(tree.root_agent_id.as_str())
        || root_marker != Some(tree.root_recursive_marker.as_str())
        || metadata
            .get("creation_receipt_sha256")
            .and_then(Value::as_str)
            != Some(tree.root_creation_receipt_sha256.as_str())
    {
        return Err("recursive root node identity is not bound to workflow run".to_string());
    }
    Ok(())
}

fn validate_recursive_workflow_payload(
    tree: &RecursiveTree,
    node: &Value,
    edge: &Value,
    agent_id: &str,
) -> Result<(), String> {
    let workflow_node_id = node
        .get("node_id")
        .and_then(Value::as_str)
        .ok_or_else(|| "recursive workflow node identity is missing".to_string())?;
    let recursive_node_id = node
        .get("recursive_node_id")
        .and_then(Value::as_str)
        .ok_or_else(|| "recursive workflow recursive_node_id is missing".to_string())?;
    let proposal_id = node
        .get("proposal_id")
        .and_then(Value::as_str)
        .ok_or_else(|| "recursive workflow proposal identity is missing".to_string())?;
    if workflow_node_id != recursive_node_id
        || !tree.accepted_proposals.contains(proposal_id)
        || !tree.receipts.contains_key(proposal_id)
        || node.get("task_type").and_then(Value::as_str) != Some("agent_step")
        || node.get("status").and_then(Value::as_str) != Some("pending")
        || node.get("attempt_count").and_then(Value::as_i64) != Some(0)
        || node.get("agent_id").and_then(Value::as_str) != Some(agent_id)
        || node.get("acceptance_reason").and_then(Value::as_str) != Some("accepted")
    {
        return Err("recursive workflow node is not bound to accepted tree proposal".to_string());
    }
    let recursive_node = tree
        .nodes
        .get(recursive_node_id)
        .ok_or_else(|| "recursive workflow node is missing from tree".to_string())?;
    if recursive_node.proposal_id.as_deref() != Some(proposal_id)
        || node.get("parent_node_id").and_then(Value::as_str)
            != recursive_node.parent_node_id.as_deref()
        || node.get("objective_fingerprint").and_then(Value::as_str)
            != Some(recursive_node.objective_fingerprint.as_str())
        || node.get("evidence_refs") != Some(&json!(recursive_node.evidence_refs))
        || node.get("recursive_capabilities") != Some(&json!(recursive_node.capabilities))
        || node.get("recursive_scope") != Some(&json!(recursive_node.scope))
        || node.get("recursive_tenant_id") != Some(&json!(recursive_node.tenant_id))
        || node.get("recursive_workspace_id") != Some(&json!(recursive_node.workspace_id))
    {
        return Err("recursive workflow node does not match accepted tree snapshot".to_string());
    }
    let edge_id = edge
        .get("edge_id")
        .and_then(Value::as_str)
        .ok_or_else(|| "recursive workflow edge identity is missing".to_string())?;
    if edge_id != format!("recursive-edge-{workflow_node_id}")
        || edge.get("from_node_id").and_then(Value::as_str)
            != recursive_node.parent_node_id.as_deref()
        || edge.get("to_node_id").and_then(Value::as_str) != Some(workflow_node_id)
        || edge.get("edge_type").and_then(Value::as_str) != Some("dependency")
        || edge.get("recursive") != Some(&Value::Bool(true))
    {
        return Err("recursive workflow edge does not match accepted tree snapshot".to_string());
    }
    Ok(())
}

pub(crate) fn validate_recursive_workflow_mutation_sqlite(
    conn: &rusqlite::Connection,
    root_run_id: &str,
    node: &Value,
    edge: &Value,
    agent_id: &str,
) -> Result<(), String> {
    let tree = load_recursive_tree_sqlite(conn, root_run_id)?
        .ok_or_else(|| "recursive_tree_missing".to_string())?;
    validate_recursive_workflow_payload(&tree, node, edge, agent_id)
}

#[cfg(feature = "pg")]
pub(crate) fn validate_recursive_workflow_mutation_pg(
    client: &mut impl postgres::GenericClient,
    root_run_id: &str,
    node: &Value,
    edge: &Value,
    agent_id: &str,
) -> Result<(), String> {
    let tree = load_recursive_tree_pg(client, root_run_id)?
        .ok_or_else(|| "recursive_tree_missing".to_string())?;
    validate_recursive_workflow_payload(&tree, node, edge, agent_id)
}

pub(crate) fn persist_recursive_tree_sqlite(
    tx: &rusqlite::Connection,
    tree: &RecursiveTree,
    now: &str,
    expected_version: Option<u64>,
) -> Result<(), String> {
    let tree_json = validate_tree_for_persistence(tree)?;
    if let Some(expected_version) = expected_version {
        check_expected_sqlite(tx, &tree.root_run_id, expected_version)?;
    }
    check_workflow_binding_sqlite(tx, tree)?;
    tx.execute(
        "INSERT INTO recursive_execution_trees
         (root_run_id, workflow_id, root_node_id, tree_schema_version, tree_json, version, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)
         ON CONFLICT(root_run_id) DO UPDATE SET
           workflow_id=excluded.workflow_id,
           root_node_id=excluded.root_node_id,
           tree_schema_version=excluded.tree_schema_version,
           tree_json=excluded.tree_json,
           version=excluded.version,
           updated_at=excluded.updated_at",
        params![
            tree.root_run_id,
            tree.workflow_id,
            tree.root_node_id,
            tree.schema_version,
            tree_json,
            tree.version as i64,
            now,
        ],
    )
    .map_err(|error| error.to_string())?;
    for node in tree.nodes.values() {
        check_node_index_sqlite(tx, &node.node_id, &tree.root_run_id)?;
        let (node_id, parent_node_id, proposal_id, depth, fingerprint, status, version) =
            node_values(node);
        tx.execute(
            "INSERT INTO recursive_execution_nodes
             (node_id, root_run_id, parent_node_id, proposal_id, depth, objective_fingerprint, status, version, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)
             ON CONFLICT(node_id) DO UPDATE SET
               root_run_id=excluded.root_run_id,
               parent_node_id=excluded.parent_node_id,
               proposal_id=excluded.proposal_id,
               depth=excluded.depth,
               objective_fingerprint=excluded.objective_fingerprint,
               status=excluded.status,
               version=excluded.version,
               updated_at=excluded.updated_at",
            params![
                node_id,
                tree.root_run_id,
                parent_node_id,
                proposal_id,
                depth,
                fingerprint,
                status,
                version,
                now,
            ],
        )
        .map_err(|error| error.to_string())?;
    }
    Ok(())
}

pub(crate) fn record_recursive_cas_rejection_sqlite(
    conn: &rusqlite::Connection,
    candidate: &RecursiveTree,
    now: &str,
) -> Result<Vec<(String, String)>, String> {
    let Some(mut current) = load_recursive_tree_sqlite(conn, &candidate.root_run_id)? else {
        return Err("recursive_tree_missing".to_string());
    };
    let expected_version = current.version;
    let mut rejected = candidate
        .accepted_proposals
        .difference(&current.accepted_proposals)
        .map(|proposal_id| {
            (
                proposal_id.clone(),
                RecursiveFailureReason::StaleParent.as_str().to_string(),
            )
        })
        .collect::<Vec<_>>();
    for (proposal_id, _) in &rejected {
        current.record_rejection(proposal_id, RecursiveFailureReason::StaleParent);
    }
    for (proposal_id, evidence) in &candidate.rejected_proposals {
        if !current.rejected_proposals.contains_key(proposal_id)
            && !current.accepted_proposals.contains(proposal_id)
        {
            current.record_rejection_evidence(proposal_id, evidence.clone());
            rejected.push((proposal_id.clone(), evidence.reason_code.clone()));
        }
    }
    if !rejected.is_empty() {
        persist_recursive_tree_sqlite(conn, &current, now, Some(expected_version))?;
    }
    Ok(rejected)
}

#[cfg(feature = "pg")]
pub(crate) fn persist_recursive_tree_pg(
    client: &mut impl postgres::GenericClient,
    tree: &RecursiveTree,
    now: &str,
    expected_version: Option<u64>,
) -> Result<(), String> {
    let tree_json = validate_tree_for_persistence(tree)?;
    if let Some(expected_version) = expected_version {
        client
            .execute(
                "SELECT pg_advisory_xact_lock(hashtext($1))",
                &[&tree.root_run_id],
            )
            .map_err(|error| error.to_string())?;
        let current: Option<i64> = client
            .query_opt(
                "SELECT version FROM recursive_execution_trees WHERE root_run_id=$1 FOR UPDATE",
                &[&tree.root_run_id],
            )
            .map_err(|error| error.to_string())?
            .map(|row| row.get(0));
        let matches = if expected_version == 0 {
            current.is_none()
        } else {
            current == Some(expected_version as i64)
        };
        if !matches {
            return Err("stale_parent".to_string());
        }
    }
    client
        .execute(
            "INSERT INTO recursive_execution_trees
             (root_run_id, workflow_id, root_node_id, tree_schema_version, tree_json, version, created_at, updated_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$7)
             ON CONFLICT(root_run_id) DO UPDATE SET
               workflow_id=EXCLUDED.workflow_id,
               root_node_id=EXCLUDED.root_node_id,
               tree_schema_version=EXCLUDED.tree_schema_version,
               tree_json=EXCLUDED.tree_json,
               version=EXCLUDED.version,
               updated_at=EXCLUDED.updated_at",
            &[
                &tree.root_run_id,
                &tree.workflow_id,
                &tree.root_node_id,
                &tree.schema_version,
                &tree_json,
                &(tree.version as i64),
                &now,
            ],
        )
        .map_err(|error| error.to_string())?;
    for node in tree.nodes.values() {
        check_node_index_pg(client, &node.node_id, &tree.root_run_id)?;
        let (node_id, parent_node_id, proposal_id, depth, fingerprint, status, version) =
            node_values(node);
        client
            .execute(
                "INSERT INTO recursive_execution_nodes
                 (node_id, root_run_id, parent_node_id, proposal_id, depth, objective_fingerprint, status, version, created_at, updated_at)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$9)
                 ON CONFLICT(node_id) DO UPDATE SET
                   root_run_id=EXCLUDED.root_run_id,
                   parent_node_id=EXCLUDED.parent_node_id,
                   proposal_id=EXCLUDED.proposal_id,
                   depth=EXCLUDED.depth,
                   objective_fingerprint=EXCLUDED.objective_fingerprint,
                   status=EXCLUDED.status,
                   version=EXCLUDED.version,
                   updated_at=EXCLUDED.updated_at",
                &[
                    &node_id,
                    &tree.root_run_id,
                    &parent_node_id,
                    &proposal_id,
                    &depth,
                    &fingerprint,
                    &status,
                    &version,
                    &now,
                ],
            )
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[cfg(feature = "pg")]
pub(crate) fn record_recursive_cas_rejection_pg(
    client: &mut impl postgres::GenericClient,
    candidate: &RecursiveTree,
    now: &str,
) -> Result<Vec<(String, String)>, String> {
    let Some(mut current) = load_recursive_tree_pg(client, &candidate.root_run_id)? else {
        return Err("recursive_tree_missing".to_string());
    };
    let expected_version = current.version;
    let mut rejected = candidate
        .accepted_proposals
        .difference(&current.accepted_proposals)
        .map(|proposal_id| {
            (
                proposal_id.clone(),
                RecursiveFailureReason::StaleParent.as_str().to_string(),
            )
        })
        .collect::<Vec<_>>();
    for (proposal_id, _) in &rejected {
        current.record_rejection(proposal_id, RecursiveFailureReason::StaleParent);
    }
    for (proposal_id, evidence) in &candidate.rejected_proposals {
        if !current.rejected_proposals.contains_key(proposal_id)
            && !current.accepted_proposals.contains(proposal_id)
        {
            current.record_rejection_evidence(proposal_id, evidence.clone());
            rejected.push((proposal_id.clone(), evidence.reason_code.clone()));
        }
    }
    if !rejected.is_empty() {
        persist_recursive_tree_pg(client, &current, now, Some(expected_version))?;
    }
    Ok(rejected)
}

impl LocalProductStore {
    /// Persist one complete tree snapshot and its bounded node identity index.
    /// The snapshot is the restart authority; the index is query/evidence aid.
    pub fn save_recursive_tree(&self, tree: &RecursiveTree) -> Result<(), String> {
        self.save_recursive_tree_with_expected_version(tree, tree.version.saturating_sub(1))
    }

    pub(crate) fn save_recursive_tree_with_expected_version(
        &self,
        tree: &RecursiveTree,
        expected_version: u64,
    ) -> Result<(), String> {
        if tree.schema_version != RECURSIVE_SCHEMA_VERSION {
            return Err("recursive tree schema version mismatch".to_string());
        }
        let tree_json = validate_tree_for_persistence(tree)?;
        let now = self.now();
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                    .map_err(|error| error.to_string())?;
                check_expected_sqlite(
                    &tx,
                    &tree.root_run_id,
                    expected_version,
                )?;
                check_workflow_binding_sqlite(&tx, tree)?;
                tx.execute(
                    "INSERT INTO recursive_execution_trees
                     (root_run_id, workflow_id, root_node_id, tree_schema_version, tree_json, version, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)
                     ON CONFLICT(root_run_id) DO UPDATE SET
                       workflow_id=excluded.workflow_id,
                       root_node_id=excluded.root_node_id,
                       tree_schema_version=excluded.tree_schema_version,
                       tree_json=excluded.tree_json,
                       version=excluded.version,
                       updated_at=excluded.updated_at",
                    params![
                        tree.root_run_id,
                        tree.workflow_id,
                        tree.root_node_id,
                        tree.schema_version,
                        tree_json,
                        tree.version as i64,
                        now,
                    ],
                )
                .map_err(|error| error.to_string())?;
                for node in tree.nodes.values() {
                    check_node_index_sqlite(&tx, &node.node_id, &tree.root_run_id)?;
                    let (node_id, parent_node_id, proposal_id, depth, fingerprint, status, version) =
                        node_values(node);
                    tx.execute(
                        "INSERT INTO recursive_execution_nodes
                         (node_id, root_run_id, parent_node_id, proposal_id, depth, objective_fingerprint, status, version, created_at, updated_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)
                         ON CONFLICT(node_id) DO UPDATE SET
                           root_run_id=excluded.root_run_id,
                           parent_node_id=excluded.parent_node_id,
                           proposal_id=excluded.proposal_id,
                           depth=excluded.depth,
                           objective_fingerprint=excluded.objective_fingerprint,
                           status=excluded.status,
                           version=excluded.version,
                           updated_at=excluded.updated_at",
                        params![
                            node_id,
                            tree.root_run_id,
                            parent_node_id,
                            proposal_id,
                            depth,
                            fingerprint,
                            status,
                            version,
                            now,
                        ],
                    )
                    .map_err(|error| error.to_string())?;
                }
                tx.commit().map_err(|error| error.to_string())
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|error| error.to_string())?;
                tx.execute(
                    "SELECT pg_advisory_xact_lock(hashtext($1))",
                    &[&tree.root_run_id],
                )
                .map_err(|error| error.to_string())?;
                let current: Option<i64> = tx
                    .query_opt(
                        "SELECT version FROM recursive_execution_trees WHERE root_run_id=$1 FOR UPDATE",
                        &[&tree.root_run_id],
                    )
                    .map_err(|error| error.to_string())?
                    .map(|row| row.get(0));
                let matches = if expected_version == 0 {
                    current.is_none()
                } else {
                    current == Some(expected_version as i64)
                };
                if !matches {
                    return Err("stale_parent".to_string());
                }
                check_workflow_binding_pg(&mut tx, tree)?;
                tx.execute(
                    "INSERT INTO recursive_execution_trees
                     (root_run_id, workflow_id, root_node_id, tree_schema_version, tree_json, version, created_at, updated_at)
                     VALUES ($1,$2,$3,$4,$5,$6,$7,$7)
                     ON CONFLICT(root_run_id) DO UPDATE SET
                       workflow_id=EXCLUDED.workflow_id,
                       root_node_id=EXCLUDED.root_node_id,
                       tree_schema_version=EXCLUDED.tree_schema_version,
                       tree_json=EXCLUDED.tree_json,
                       version=EXCLUDED.version,
                       updated_at=EXCLUDED.updated_at",
                    &[
                        &tree.root_run_id,
                        &tree.workflow_id,
                        &tree.root_node_id,
                        &tree.schema_version,
                        &tree_json,
                        &(tree.version as i64),
                        &now,
                    ],
                )
                .map_err(|error| error.to_string())?;
                for node in tree.nodes.values() {
                    check_node_index_pg(&mut tx, &node.node_id, &tree.root_run_id)?;
                    let (node_id, parent_node_id, proposal_id, depth, fingerprint, status, version) =
                        node_values(node);
                    tx.execute(
                        "INSERT INTO recursive_execution_nodes
                         (node_id, root_run_id, parent_node_id, proposal_id, depth, objective_fingerprint, status, version, created_at, updated_at)
                         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$9)
                         ON CONFLICT(node_id) DO UPDATE SET
                           root_run_id=EXCLUDED.root_run_id,
                           parent_node_id=EXCLUDED.parent_node_id,
                           proposal_id=EXCLUDED.proposal_id,
                           depth=EXCLUDED.depth,
                           objective_fingerprint=EXCLUDED.objective_fingerprint,
                           status=EXCLUDED.status,
                           version=EXCLUDED.version,
                           updated_at=EXCLUDED.updated_at",
                        &[
                            &node_id,
                            &tree.root_run_id,
                            &parent_node_id,
                            &proposal_id,
                            &depth,
                            &fingerprint,
                            &status,
                            &version,
                            &now,
                        ],
                    )
                    .map_err(|error| error.to_string())?;
                }
                tx.commit().map_err(|error| error.to_string())
            }),
        }
    }

    pub fn load_recursive_tree(&self, root_run_id: &str) -> Result<Option<RecursiveTree>, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => {
                self.with_conn(|conn| load_recursive_tree_sqlite(conn, root_run_id))
            }
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => {
                self.with_pg_conn(|client| load_recursive_tree_pg(client, root_run_id))
            }
        }
    }

    pub fn recursive_tree_operator_evidence(&self, root_run_id: &str) -> Result<Value, String> {
        self.load_recursive_tree(root_run_id)?
            .map(|tree| tree.redacted_read_model())
            .ok_or_else(|| "recursive tree not found".to_string())
    }

    /// Pause recursive admission and terminally block any currently leased
    /// recursive nodes. This is called by the existing scheduler operator
    /// controls so recursive leases cannot outlive a pause/kill transition.
    pub(crate) fn set_recursive_execution_paused(
        &self,
        paused: bool,
        terminal_reason: Option<&str>,
    ) -> Result<usize, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                    .map_err(|error| error.to_string())?;
                let now = self.now();
                let rows: Vec<(String, String, String, String, String)> = {
                    let mut stmt = tx
                        .prepare(
                            "SELECT root_run_id, workflow_id, root_node_id,
                             tree_schema_version, tree_json FROM recursive_execution_trees",
                        )
                        .map_err(|error| error.to_string())?;
                    let rows = stmt
                        .query_map([], |row| {
                            Ok((
                                row.get(0)?,
                                row.get(1)?,
                                row.get(2)?,
                                row.get(3)?,
                                row.get(4)?,
                            ))
                        })
                        .map_err(|error| error.to_string())?
                        .collect::<Result<Vec<_>, _>>()
                        .map_err(|error| error.to_string())?;
                    rows
                };
                let mut changed = 0;
                for (root_run_id, workflow_id, root_node_id, schema_version, tree_json) in rows {
                    let mut tree: RecursiveTree =
                        serde_json::from_str(&tree_json).map_err(|error| error.to_string())?;
                    if tree.root_run_id != root_run_id
                        || tree.workflow_id != workflow_id
                        || tree.root_node_id != root_node_id
                        || tree.schema_version != schema_version
                    {
                        return Err("recursive tree identity or schema conflict".to_string());
                    }
                    normalize_loaded_tree(&mut tree);
                    validate_tree_for_persistence(&tree)?;
                    let expected_version = tree.version;
                    let mut terminated = Vec::new();
                    let budget_exhausted = tree.execution_state
                        == crate::recursive_execution::RecursiveExecutionState::BudgetExhausted
                        || tree.nodes.values().any(|node| {
                            node.failure_reason.as_deref()
                                == Some(RecursiveFailureReason::TreeBudgetExhausted.as_str())
                        });
                    let effective_state = if budget_exhausted {
                        crate::recursive_execution::RecursiveExecutionState::BudgetExhausted
                    } else if matches!(
                        tree.execution_state,
                        crate::recursive_execution::RecursiveExecutionState::KillStopped
                            | crate::recursive_execution::RecursiveExecutionState::TerminalFailed
                    ) {
                        tree.execution_state
                    } else if paused
                        && terminal_reason
                            == Some(RecursiveFailureReason::RecursiveKillSwitchActive.as_str())
                    {
                        crate::recursive_execution::RecursiveExecutionState::KillStopped
                    } else if paused {
                        crate::recursive_execution::RecursiveExecutionState::OperatorPaused
                    } else if tree.execution_state
                        == crate::recursive_execution::RecursiveExecutionState::OperatorPaused
                    {
                        crate::recursive_execution::RecursiveExecutionState::Running
                    } else {
                        tree.execution_state
                    };
                    let terminal_node_reason = match effective_state {
                        crate::recursive_execution::RecursiveExecutionState::KillStopped => {
                            RecursiveFailureReason::RecursiveKillSwitchActive.as_str()
                        }
                        crate::recursive_execution::RecursiveExecutionState::BudgetExhausted => {
                            RecursiveFailureReason::TreeBudgetExhausted.as_str()
                        }
                        crate::recursive_execution::RecursiveExecutionState::TerminalFailed => {
                            RecursiveFailureReason::TerminalFailed.as_str()
                        }
                        _ => RecursiveFailureReason::OperatorPaused.as_str(),
                    };
                    if tree.execution_state != effective_state {
                        tree.execution_state = effective_state;
                        tree.version += 1;
                    }
                    if terminal_reason.is_some() {
                        for node in tree.nodes.values_mut() {
                            if (node.parent_node_id.is_some() || node.lease_id.is_some())
                                && matches!(node.status.as_str(), "ready" | "leased")
                            {
                                if let Some(lease_id) = node.lease_id.take() {
                                    tree.active_leases.remove(&lease_id);
                                }
                                terminated.push(node.node_id.clone());
                                node.status = "failed".to_string();
                                node.failure_reason = Some(terminal_node_reason.to_string());
                                node.version += 1;
                            }
                        }
                        if !terminated.is_empty() {
                            tree.version += 1;
                            for node_id in &terminated {
                                tree.release_node_reservation_for_persistence(node_id);
                            }
                        }
                    }
                    if tree.version == expected_version {
                        continue;
                    }
                    persist_recursive_tree_sqlite(&tx, &tree, &now, Some(expected_version))?;
                    changed += 1;
                    append_audit_locked(
                        &tx,
                        &now,
                        "scheduler",
                        if effective_state
                            != crate::recursive_execution::RecursiveExecutionState::Running
                        {
                            "recursive.execution.paused"
                        } else {
                            "recursive.execution.resumed"
                        },
                        &root_run_id,
                        &json!({
                            "root_run_id": root_run_id,
                            "workflow_id": workflow_id,
                            "terminalized_node_count": terminated.len(),
                        }),
                    )?;
                    if !terminated.is_empty() {
                        append_audit_locked(
                            &tx,
                            &now,
                            "scheduler",
                            "recursive.execution.leases_terminalized",
                            &root_run_id,
                            &json!({
                                "root_run_id": root_run_id,
                                "reason": terminal_node_reason,
                                "node_count": terminated.len(),
                            }),
                        )?;
                    }
                    for workflow_node_id in terminated {
                        let workflow_exists: i64 = tx
                            .query_row(
                                "SELECT COUNT(*) FROM workflow_runs WHERE run_id=?1",
                                [root_run_id.as_str()],
                                |row| row.get(0),
                            )
                            .map_err(|error| error.to_string())?;
                        if workflow_exists == 0 {
                            continue;
                        }
                        let workflow_row: Option<(String, Option<String>, String)> = tx
                            .query_row(
                                "SELECT status, leased_at, node_json FROM workflow_run_nodes
                                 WHERE run_id=?1 AND node_id=?2",
                                params![root_run_id, workflow_node_id],
                                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                            )
                            .optional()
                            .map_err(|error| error.to_string())?;
                        let Some((status, leased_at, node_json_text)) = workflow_row else {
                            continue;
                        };
                        if !(((status == "running" && leased_at.is_some())
                            || (status == "pending" && leased_at.is_none()))
                            && serde_json::from_str::<Value>(&node_json_text)
                                .ok()
                                .and_then(|value| {
                                    value
                                        .get("recursive_node_id")
                                        .or_else(|| value.get("recursive_root_node_id"))
                                        .and_then(Value::as_str)
                                        .map(|id| id == workflow_node_id)
                                })
                                .unwrap_or(false))
                        {
                            return Err("recursive workflow node binding is missing".to_string());
                        }
                        let mut node_json: Value = serde_json::from_str(&node_json_text)
                            .map_err(|error| error.to_string())?;
                        if let Some(object) = node_json.as_object_mut() {
                            object.insert("status".to_string(), json!("failed"));
                            object.insert("completed_at".to_string(), json!(now));
                        }
                        let updated = tx
                            .execute(
                                "UPDATE workflow_run_nodes SET status='failed', completed_at=?1,
                                 leased_at=NULL, blocked_reason=?2, node_json=?3
                                 WHERE run_id=?4 AND node_id=?5
                                   AND ((status='running' AND leased_at IS NOT NULL)
                                     OR (status='pending' AND leased_at IS NULL))",
                                params![
                                    now,
                                    terminal_node_reason,
                                    node_json.to_string(),
                                    root_run_id,
                                    workflow_node_id,
                                ],
                            )
                            .map_err(|error| error.to_string())?;
                        if updated != 1 {
                            return Err("recursive workflow node terminalization raced".to_string());
                        }
                    }
                }
                tx.commit().map_err(|error| error.to_string())?;
                Ok(changed)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|error| error.to_string())?;
                let now = self.now();
                // Claiming a recursive workflow node locks the authoritative
                // workflow row before taking the per-root tree lock. Lock all
                // currently running recursive workflow rows first so pause/
                // kill cannot form the opposite claim↔tree lock cycle.
                tx.query(
                    "SELECT run_id, node_id FROM workflow_run_nodes
                     WHERE task_type='agent_step' AND status='running'
                       AND (node_json::jsonb ? 'recursive_node_id'
                            OR node_json::jsonb ? 'recursive_root_node_id')
                     ORDER BY run_id, node_id FOR UPDATE",
                    &[],
                )
                .map_err(|error| error.to_string())?;
                let rows = tx
                    .query(
                        "SELECT root_run_id FROM recursive_execution_trees ORDER BY root_run_id",
                        &[],
                    )
                    .map_err(|error| error.to_string())?;
                let mut changed = 0;
                for row in rows {
                    let root_run_id: String = row.get(0);
                    tx.execute(
                        "SELECT pg_advisory_xact_lock(hashtext($1))",
                        &[&root_run_id],
                    )
                    .map_err(|error| error.to_string())?;
                    let authoritative = tx
                        .query_opt(
                            "SELECT workflow_id, root_node_id, tree_schema_version, tree_json
                             FROM recursive_execution_trees WHERE root_run_id=$1 FOR UPDATE",
                            &[&root_run_id],
                        )
                        .map_err(|error| error.to_string())?
                        .ok_or_else(|| "recursive_tree_missing".to_string())?;
                    let workflow_id: String = authoritative.get(0);
                    let root_node_id: String = authoritative.get(1);
                    let schema_version: String = authoritative.get(2);
                    let tree_json: String = authoritative.get(3);
                    let mut tree: RecursiveTree =
                        serde_json::from_str(&tree_json).map_err(|error| error.to_string())?;
                    if tree.root_run_id != root_run_id
                        || tree.workflow_id != workflow_id
                        || tree.root_node_id != root_node_id
                        || tree.schema_version != schema_version
                    {
                        return Err("recursive tree identity or schema conflict".to_string());
                    }
                    normalize_loaded_tree(&mut tree);
                    validate_tree_for_persistence(&tree)?;
                    let expected_version = tree.version;
                    let mut terminated = Vec::new();
                    let budget_exhausted = tree.execution_state
                        == crate::recursive_execution::RecursiveExecutionState::BudgetExhausted
                        || tree.nodes.values().any(|node| {
                            node.failure_reason.as_deref()
                                == Some(RecursiveFailureReason::TreeBudgetExhausted.as_str())
                        });
                    let effective_state = if budget_exhausted {
                        crate::recursive_execution::RecursiveExecutionState::BudgetExhausted
                    } else if matches!(
                        tree.execution_state,
                        crate::recursive_execution::RecursiveExecutionState::KillStopped
                            | crate::recursive_execution::RecursiveExecutionState::TerminalFailed
                    ) {
                        tree.execution_state
                    } else if paused
                        && terminal_reason
                            == Some(RecursiveFailureReason::RecursiveKillSwitchActive.as_str())
                    {
                        crate::recursive_execution::RecursiveExecutionState::KillStopped
                    } else if paused {
                        crate::recursive_execution::RecursiveExecutionState::OperatorPaused
                    } else if tree.execution_state
                        == crate::recursive_execution::RecursiveExecutionState::OperatorPaused
                    {
                        crate::recursive_execution::RecursiveExecutionState::Running
                    } else {
                        tree.execution_state
                    };
                    let terminal_node_reason = match effective_state {
                        crate::recursive_execution::RecursiveExecutionState::KillStopped => {
                            RecursiveFailureReason::RecursiveKillSwitchActive.as_str()
                        }
                        crate::recursive_execution::RecursiveExecutionState::BudgetExhausted => {
                            RecursiveFailureReason::TreeBudgetExhausted.as_str()
                        }
                        crate::recursive_execution::RecursiveExecutionState::TerminalFailed => {
                            RecursiveFailureReason::TerminalFailed.as_str()
                        }
                        _ => RecursiveFailureReason::OperatorPaused.as_str(),
                    };
                    if tree.execution_state != effective_state {
                        tree.execution_state = effective_state;
                        tree.version += 1;
                    }
                    if terminal_reason.is_some() {
                        for node in tree.nodes.values_mut() {
                            if (node.parent_node_id.is_some() || node.lease_id.is_some())
                                && matches!(node.status.as_str(), "ready" | "leased")
                            {
                                if let Some(lease_id) = node.lease_id.take() {
                                    tree.active_leases.remove(&lease_id);
                                }
                                terminated.push(node.node_id.clone());
                                node.status = "failed".to_string();
                                node.failure_reason = Some(terminal_node_reason.to_string());
                                node.version += 1;
                            }
                        }
                        if !terminated.is_empty() {
                            tree.version += 1;
                            for node_id in &terminated {
                                tree.release_node_reservation_for_persistence(node_id);
                            }
                        }
                    }
                    if tree.version == expected_version {
                        continue;
                    }
                    persist_recursive_tree_pg(&mut tx, &tree, &now, Some(expected_version))?;
                    changed += 1;
                    super::workflow_runs::pg_append_audit(
                        &mut tx,
                        &now,
                        "scheduler",
                        if effective_state
                            != crate::recursive_execution::RecursiveExecutionState::Running
                        {
                            "recursive.execution.paused"
                        } else {
                            "recursive.execution.resumed"
                        },
                        &root_run_id,
                        &json!({
                            "root_run_id": root_run_id,
                            "workflow_id": workflow_id,
                            "terminalized_node_count": terminated.len(),
                        }),
                    )?;
                    if !terminated.is_empty() {
                        super::workflow_runs::pg_append_audit(
                            &mut tx,
                            &now,
                            "scheduler",
                            "recursive.execution.leases_terminalized",
                            &root_run_id,
                            &json!({
                                "root_run_id": root_run_id,
                                "reason": terminal_node_reason,
                                "node_count": terminated.len(),
                            }),
                        )?;
                    }
                    for workflow_node_id in terminated {
                        let workflow_exists: i64 = tx
                            .query_one(
                                "SELECT COUNT(*) FROM workflow_runs WHERE run_id=$1",
                                &[&root_run_id],
                            )
                            .map_err(|error| error.to_string())?
                            .get(0);
                        if workflow_exists == 0 {
                            continue;
                        }
                        let workflow_row: Option<(String, Option<String>, String)> = tx
                            .query_opt(
                                "SELECT status, leased_at, node_json FROM workflow_run_nodes
                                 WHERE run_id=$1 AND node_id=$2",
                                &[&root_run_id, &workflow_node_id],
                            )
                            .map_err(|error| error.to_string())?
                            .map(|row| (row.get(0), row.get(1), row.get(2)));
                        let Some((status, leased_at, node_json_text)) = workflow_row else {
                            continue;
                        };
                        if !(((status == "running" && leased_at.is_some())
                            || (status == "pending" && leased_at.is_none()))
                            && serde_json::from_str::<Value>(&node_json_text)
                                .ok()
                                .and_then(|value| {
                                    value
                                        .get("recursive_node_id")
                                        .or_else(|| value.get("recursive_root_node_id"))
                                        .and_then(Value::as_str)
                                        .map(|id| id == workflow_node_id)
                                })
                                .unwrap_or(false))
                        {
                            return Err("recursive workflow node binding is missing".to_string());
                        }
                        let mut node_json: Value = serde_json::from_str(&node_json_text)
                            .map_err(|error| error.to_string())?;
                        if let Some(object) = node_json.as_object_mut() {
                            object.insert("status".to_string(), json!("failed"));
                            object.insert("completed_at".to_string(), json!(now));
                        }
                        let updated = tx
                            .execute(
                                "UPDATE workflow_run_nodes SET status='failed', completed_at=$1,
                                 leased_at=NULL, blocked_reason=$2, node_json=$3
                                 WHERE run_id=$4 AND node_id=$5
                                   AND ((status='running' AND leased_at IS NOT NULL)
                                     OR (status='pending' AND leased_at IS NULL))",
                                &[
                                    &now,
                                    &terminal_node_reason,
                                    &node_json.to_string(),
                                    &root_run_id,
                                    &workflow_node_id,
                                ],
                            )
                            .map_err(|error| error.to_string())?;
                        if updated != 1 {
                            return Err("recursive workflow node terminalization raced".to_string());
                        }
                    }
                }
                tx.commit().map_err(|error| error.to_string())?;
                Ok(changed)
            }),
        }
    }
}

pub(crate) fn load_recursive_tree_sqlite(
    conn: &rusqlite::Connection,
    root_run_id: &str,
) -> Result<Option<RecursiveTree>, String> {
    let stored: Option<(String, String, String, String)> = conn
        .query_row(
            "SELECT workflow_id, root_node_id, tree_schema_version, tree_json
             FROM recursive_execution_trees WHERE root_run_id=?1",
            params![root_run_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    stored
        .map(|(workflow_id, root_node_id, schema_version, value)| {
            let mut tree: RecursiveTree =
                serde_json::from_str(&value).map_err(|error| error.to_string())?;
            if tree.root_run_id != root_run_id
                || tree.workflow_id != workflow_id
                || tree.root_node_id != root_node_id
                || tree.schema_version != schema_version
                || tree.schema_version != RECURSIVE_SCHEMA_VERSION
            {
                return Err("recursive tree identity or schema conflict".to_string());
            }
            normalize_loaded_tree(&mut tree);
            check_workflow_binding_sqlite(conn, &tree)?;
            validate_tree_for_persistence(&tree)?;
            Ok(tree)
        })
        .transpose()
}

pub(crate) fn sync_recursive_lease_sqlite(
    conn: &rusqlite::Connection,
    root_run_id: &str,
    recursive_node_id: &str,
    lease_id: &str,
    now: &str,
) -> Result<(), String> {
    let Some(mut tree) = load_recursive_tree_sqlite(conn, root_run_id)? else {
        return Err("recursive_tree_missing".to_string());
    };
    if !tree.nodes.contains_key(recursive_node_id) {
        return Err("recursive_node_missing".to_string());
    }
    let expected_version = tree.version;
    tree.lease_node(recursive_node_id, lease_id)
        .map_err(|reason| reason.as_str().to_string())?;
    persist_recursive_tree_sqlite(conn, &tree, now, Some(expected_version))
}

pub(crate) fn recursive_retry_allowed_sqlite(
    conn: &rusqlite::Connection,
    root_run_id: &str,
    recursive_node_id: &str,
    usage: &crate::recursive_execution::RecursiveBudget,
) -> Result<bool, String> {
    let Some(tree) = load_recursive_tree_sqlite(conn, root_run_id)? else {
        return Err("recursive_tree_missing".to_string());
    };
    if !tree.nodes.contains_key(recursive_node_id) {
        return Err("recursive_node_missing".to_string());
    }
    tree.retry_allowed(recursive_node_id, usage)
        .map_err(|reason| reason.as_str().to_string())
}

pub(crate) fn sync_recursive_stale_recovery_sqlite(
    conn: &rusqlite::Connection,
    root_run_id: &str,
    recursive_node_id: &str,
    lease_id: &str,
    retry: bool,
    now: &str,
) -> Result<String, String> {
    let Some(mut tree) = load_recursive_tree_sqlite(conn, root_run_id)? else {
        return Err("recursive_tree_missing".to_string());
    };
    let expected_version = tree.version;
    let status = tree
        .recover_stale_lease(recursive_node_id, lease_id, retry)
        .map_err(|reason| reason.as_str().to_string())?;
    if !retry {
        tree.release_node_reservation_for_persistence(recursive_node_id);
    }
    persist_recursive_tree_sqlite(conn, &tree, now, Some(expected_version))?;
    Ok(status)
}

fn sync_terminal_workflow_nodes_sqlite(
    conn: &rusqlite::Connection,
    tree: &RecursiveTree,
    now: &str,
) -> Result<(), String> {
    for node in tree.nodes.values().filter(|node| {
        node.parent_node_id.is_some()
            && node.status == "failed"
            && matches!(
                node.failure_reason.as_deref(),
                Some("tree_budget_exhausted" | "terminal_failed" | "recursive_usage_unavailable")
            )
    }) {
        let row: Option<(String, String)> = conn
            .query_row(
                "SELECT status, node_json FROM workflow_run_nodes
                 WHERE run_id=?1 AND node_id=?2",
                params![tree.root_run_id, node.node_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        let Some((status, node_json_text)) = row else {
            continue;
        };
        if !matches!(status.as_str(), "pending" | "running") {
            continue;
        }
        let mut node_json: Value = serde_json::from_str(&node_json_text)
            .map_err(|_| "recursive workflow node binding is malformed".to_string())?;
        if node_json.get("recursive_node_id").and_then(Value::as_str) != Some(node.node_id.as_str())
        {
            return Err("recursive workflow node binding is missing".to_string());
        }
        if let Some(object) = node_json.as_object_mut() {
            object.insert("status".to_string(), json!("failed"));
            object.insert("completed_at".to_string(), json!(now));
        }
        conn.execute(
            "UPDATE workflow_run_nodes SET status='failed', completed_at=?1,
             leased_at=NULL, blocked_reason=?2, node_json=?3
             WHERE run_id=?4 AND node_id=?5 AND status IN ('pending','running')",
            params![
                now,
                node.failure_reason.as_deref(),
                node_json.to_string(),
                tree.root_run_id,
                node.node_id,
            ],
        )
        .map_err(|error| error.to_string())?;
    }
    Ok(())
}

pub(crate) fn sync_recursive_completion_sqlite(
    conn: &rusqlite::Connection,
    root_run_id: &str,
    recursive_node_id: &str,
    lease_id: &str,
    success: bool,
    retry: bool,
    usage: &crate::recursive_execution::RecursiveBudget,
    now: &str,
) -> Result<String, String> {
    let Some(mut tree) = load_recursive_tree_sqlite(conn, root_run_id)? else {
        return Err("recursive_tree_missing".to_string());
    };
    if !tree.nodes.contains_key(recursive_node_id) {
        return Err("recursive_node_missing".to_string());
    }
    let expected_version = tree.version;
    let retry_allowed = retry
        && tree
            .retry_allowed(recursive_node_id, usage)
            .map_err(|reason| reason.as_str().to_string())?;
    tree.complete_node_with_usage(recursive_node_id, lease_id, success, usage)
        .map_err(|reason| reason.as_str().to_string())?;
    if retry_allowed {
        tree.retry_node(recursive_node_id)
            .map_err(|reason| reason.as_str().to_string())?;
    } else {
        tree.release_node_reservation_for_persistence(recursive_node_id);
    }
    sync_terminal_workflow_nodes_sqlite(conn, &tree, now)?;
    let status = tree
        .nodes
        .get(recursive_node_id)
        .map(|node| node.status.clone())
        .ok_or_else(|| "recursive_node_missing".to_string())?;
    persist_recursive_tree_sqlite(conn, &tree, now, Some(expected_version))?;
    Ok(status)
}

pub(crate) fn record_recursive_late_usage_sqlite(
    conn: &rusqlite::Connection,
    root_run_id: &str,
    recursive_node_id: &str,
    attempt_receipt: &str,
    usage: &crate::recursive_execution::RecursiveBudget,
    now: &str,
) -> Result<bool, String> {
    let Some(mut tree) = load_recursive_tree_sqlite(conn, root_run_id)? else {
        return Err("recursive_tree_missing".to_string());
    };
    if !tree.nodes.contains_key(recursive_node_id) {
        return Err("recursive_node_missing".to_string());
    }
    let expected_version = tree.version;
    let within_tree_budget = tree
        .record_late_usage(recursive_node_id, attempt_receipt, usage)
        .map_err(|reason| reason.as_str().to_string())?;
    sync_terminal_workflow_nodes_sqlite(conn, &tree, now)?;
    persist_recursive_tree_sqlite(conn, &tree, now, Some(expected_version))?;
    Ok(within_tree_budget)
}

pub(crate) fn sync_recursive_usage_unavailable_sqlite(
    conn: &rusqlite::Connection,
    root_run_id: &str,
    recursive_node_id: &str,
    lease_id: &str,
    now: &str,
) -> Result<String, String> {
    sync_recursive_completion_sqlite(
        conn,
        root_run_id,
        recursive_node_id,
        lease_id,
        false,
        false,
        &RecursiveBudget::default(),
        now,
    )?;
    let mut tree = load_recursive_tree_sqlite(conn, root_run_id)?
        .ok_or_else(|| "recursive_tree_missing".to_string())?;
    let expected_version = tree.version;
    let node = tree.nodes.get_mut(recursive_node_id).ok_or_else(|| {
        RecursiveFailureReason::RecursiveNodeMissing
            .as_str()
            .to_string()
    })?;
    node.failure_reason = Some(
        RecursiveFailureReason::RecursiveUsageUnavailable
            .as_str()
            .to_string(),
    );
    node.status = "failed".to_string();
    node.version += 1;
    tree.execution_state = crate::recursive_execution::RecursiveExecutionState::TerminalFailed;
    tree.terminalize_remaining_nodes(
        Some(recursive_node_id),
        RecursiveFailureReason::TerminalFailed,
    );
    tree.version += 1;
    sync_terminal_workflow_nodes_sqlite(conn, &tree, now)?;
    persist_recursive_tree_sqlite(conn, &tree, now, Some(expected_version))?;
    Ok("failed".to_string())
}

#[cfg(feature = "pg")]
pub(crate) fn load_recursive_tree_pg(
    client: &mut impl postgres::GenericClient,
    root_run_id: &str,
) -> Result<Option<RecursiveTree>, String> {
    client
        .execute(
            "SELECT pg_advisory_xact_lock(hashtext($1))",
            &[&root_run_id],
        )
        .map_err(|error| error.to_string())?;
    let stored: Option<(String, String, String, String)> = client
        .query_opt(
            "SELECT workflow_id, root_node_id, tree_schema_version, tree_json
             FROM recursive_execution_trees WHERE root_run_id=$1 FOR UPDATE",
            &[&root_run_id],
        )
        .map_err(|error| error.to_string())?
        .map(|row| (row.get(0), row.get(1), row.get(2), row.get(3)));
    stored
        .map(|(workflow_id, root_node_id, schema_version, value)| {
            let mut tree: RecursiveTree =
                serde_json::from_str(&value).map_err(|error| error.to_string())?;
            if tree.root_run_id != root_run_id
                || tree.workflow_id != workflow_id
                || tree.root_node_id != root_node_id
                || tree.schema_version != schema_version
                || tree.schema_version != RECURSIVE_SCHEMA_VERSION
            {
                return Err("recursive tree identity or schema conflict".to_string());
            }
            normalize_loaded_tree(&mut tree);
            check_workflow_binding_pg(client, &tree)?;
            validate_tree_for_persistence(&tree)?;
            Ok(tree)
        })
        .transpose()
}

#[cfg(feature = "pg")]
pub(crate) fn sync_recursive_lease_pg(
    client: &mut impl postgres::GenericClient,
    root_run_id: &str,
    recursive_node_id: &str,
    lease_id: &str,
    now: &str,
) -> Result<(), String> {
    let Some(mut tree) = load_recursive_tree_pg(client, root_run_id)? else {
        return Err("recursive_tree_missing".to_string());
    };
    if !tree.nodes.contains_key(recursive_node_id) {
        return Err("recursive_node_missing".to_string());
    }
    let expected_version = tree.version;
    tree.lease_node(recursive_node_id, lease_id)
        .map_err(|reason| reason.as_str().to_string())?;
    persist_recursive_tree_pg(client, &tree, now, Some(expected_version))
}

#[cfg(feature = "pg")]
pub(crate) fn recursive_retry_allowed_pg(
    client: &mut impl postgres::GenericClient,
    root_run_id: &str,
    recursive_node_id: &str,
    usage: &crate::recursive_execution::RecursiveBudget,
) -> Result<bool, String> {
    let Some(tree) = load_recursive_tree_pg(client, root_run_id)? else {
        return Err("recursive_tree_missing".to_string());
    };
    if !tree.nodes.contains_key(recursive_node_id) {
        return Err("recursive_node_missing".to_string());
    }
    tree.retry_allowed(recursive_node_id, usage)
        .map_err(|reason| reason.as_str().to_string())
}

#[cfg(feature = "pg")]
pub(crate) fn sync_recursive_stale_recovery_pg(
    client: &mut impl postgres::GenericClient,
    root_run_id: &str,
    recursive_node_id: &str,
    lease_id: &str,
    retry: bool,
    now: &str,
) -> Result<String, String> {
    let Some(mut tree) = load_recursive_tree_pg(client, root_run_id)? else {
        return Err("recursive_tree_missing".to_string());
    };
    let expected_version = tree.version;
    let status = tree
        .recover_stale_lease(recursive_node_id, lease_id, retry)
        .map_err(|reason| reason.as_str().to_string())?;
    if !retry {
        tree.release_node_reservation_for_persistence(recursive_node_id);
    }
    persist_recursive_tree_pg(client, &tree, now, Some(expected_version))?;
    Ok(status)
}

#[cfg(feature = "pg")]
fn sync_terminal_workflow_nodes_pg(
    client: &mut impl postgres::GenericClient,
    tree: &RecursiveTree,
    now: &str,
) -> Result<(), String> {
    for node in tree.nodes.values().filter(|node| {
        node.parent_node_id.is_some()
            && node.status == "failed"
            && matches!(
                node.failure_reason.as_deref(),
                Some("tree_budget_exhausted" | "terminal_failed" | "recursive_usage_unavailable")
            )
    }) {
        let row = client
            .query_opt(
                "SELECT status, node_json FROM workflow_run_nodes
                 WHERE run_id=$1 AND node_id=$2",
                &[&tree.root_run_id, &node.node_id],
            )
            .map_err(|error| error.to_string())?;
        let Some(row) = row else {
            continue;
        };
        let status: String = row.get(0);
        if !matches!(status.as_str(), "pending" | "running") {
            continue;
        }
        let node_json_text: String = row.get(1);
        let mut node_json: Value = serde_json::from_str(&node_json_text)
            .map_err(|_| "recursive workflow node binding is malformed".to_string())?;
        if node_json.get("recursive_node_id").and_then(Value::as_str) != Some(node.node_id.as_str())
        {
            return Err("recursive workflow node binding is missing".to_string());
        }
        if let Some(object) = node_json.as_object_mut() {
            object.insert("status".to_string(), json!("failed"));
            object.insert("completed_at".to_string(), json!(now));
        }
        client
            .execute(
                "UPDATE workflow_run_nodes SET status='failed', completed_at=$1,
                 leased_at=NULL, blocked_reason=$2, node_json=$3
                 WHERE run_id=$4 AND node_id=$5 AND status IN ('pending','running')",
                &[
                    &now,
                    &node.failure_reason.as_deref(),
                    &node_json.to_string(),
                    &tree.root_run_id,
                    &node.node_id,
                ],
            )
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[cfg(feature = "pg")]
pub(crate) fn sync_recursive_completion_pg(
    client: &mut impl postgres::GenericClient,
    root_run_id: &str,
    recursive_node_id: &str,
    lease_id: &str,
    success: bool,
    retry: bool,
    usage: &crate::recursive_execution::RecursiveBudget,
    now: &str,
) -> Result<String, String> {
    let Some(mut tree) = load_recursive_tree_pg(client, root_run_id)? else {
        return Err("recursive_tree_missing".to_string());
    };
    if !tree.nodes.contains_key(recursive_node_id) {
        return Err("recursive_node_missing".to_string());
    }
    let expected_version = tree.version;
    let retry_allowed = retry
        && tree
            .retry_allowed(recursive_node_id, usage)
            .map_err(|reason| reason.as_str().to_string())?;
    tree.complete_node_with_usage(recursive_node_id, lease_id, success, usage)
        .map_err(|reason| reason.as_str().to_string())?;
    if retry_allowed {
        tree.retry_node(recursive_node_id)
            .map_err(|reason| reason.as_str().to_string())?;
    } else {
        tree.release_node_reservation_for_persistence(recursive_node_id);
    }
    sync_terminal_workflow_nodes_pg(client, &tree, now)?;
    let status = tree
        .nodes
        .get(recursive_node_id)
        .map(|node| node.status.clone())
        .ok_or_else(|| "recursive_node_missing".to_string())?;
    persist_recursive_tree_pg(client, &tree, now, Some(expected_version))?;
    Ok(status)
}

#[cfg(feature = "pg")]
pub(crate) fn record_recursive_late_usage_pg(
    client: &mut impl postgres::GenericClient,
    root_run_id: &str,
    recursive_node_id: &str,
    attempt_receipt: &str,
    usage: &crate::recursive_execution::RecursiveBudget,
    now: &str,
) -> Result<bool, String> {
    let Some(mut tree) = load_recursive_tree_pg(client, root_run_id)? else {
        return Err("recursive_tree_missing".to_string());
    };
    if !tree.nodes.contains_key(recursive_node_id) {
        return Err("recursive_node_missing".to_string());
    }
    let expected_version = tree.version;
    let within_tree_budget = tree
        .record_late_usage(recursive_node_id, attempt_receipt, usage)
        .map_err(|reason| reason.as_str().to_string())?;
    sync_terminal_workflow_nodes_pg(client, &tree, now)?;
    persist_recursive_tree_pg(client, &tree, now, Some(expected_version))?;
    Ok(within_tree_budget)
}

#[cfg(feature = "pg")]
pub(crate) fn sync_recursive_usage_unavailable_pg(
    client: &mut impl postgres::GenericClient,
    root_run_id: &str,
    recursive_node_id: &str,
    lease_id: &str,
    now: &str,
) -> Result<String, String> {
    sync_recursive_completion_pg(
        client,
        root_run_id,
        recursive_node_id,
        lease_id,
        false,
        false,
        &RecursiveBudget::default(),
        now,
    )?;
    let mut tree = load_recursive_tree_pg(client, root_run_id)?
        .ok_or_else(|| "recursive_tree_missing".to_string())?;
    let expected_version = tree.version;
    let node = tree.nodes.get_mut(recursive_node_id).ok_or_else(|| {
        RecursiveFailureReason::RecursiveNodeMissing
            .as_str()
            .to_string()
    })?;
    node.failure_reason = Some(
        RecursiveFailureReason::RecursiveUsageUnavailable
            .as_str()
            .to_string(),
    );
    node.status = "failed".to_string();
    node.version += 1;
    tree.execution_state = crate::recursive_execution::RecursiveExecutionState::TerminalFailed;
    tree.terminalize_remaining_nodes(
        Some(recursive_node_id),
        RecursiveFailureReason::TerminalFailed,
    );
    tree.version += 1;
    sync_terminal_workflow_nodes_pg(client, &tree, now)?;
    persist_recursive_tree_pg(client, &tree, now, Some(expected_version))?;
    Ok("failed".to_string())
}

#[cfg(test)]
mod tests {
    use super::super::{AgentActionMutation, AgentMutationOp};
    use super::*;
    use crate::recursive_execution::{RecursiveBudget, RecursiveProposal, RecursiveScope};
    use std::collections::BTreeSet;

    fn bind_test_workflow(
        store: &LocalProductStore,
        run_id: &str,
        workflow_id: &str,
        root_node_id: &str,
    ) {
        store
            .import_workflow_run(&json!({
                "run_id": run_id,
                "workflow_id": workflow_id,
                "status": "created",
                "boundaries": {"execution_authority": "disabled"},
                "nodes": [{
                    "node_id": root_node_id,
                    "task_type": "agent_step",
                    "status": "completed",
                    "recursive_root_node_id": root_node_id,
                    "agent_id": "test-agent",
                    "creation_receipt_sha256": "test-root-receipt"
                }],
                "edges": [],
                "events": [],
                "approvals": []
            }))
            .expect("bind test workflow");
    }

    fn bind_test_tree_identity(tree: &mut RecursiveTree) {
        let root_node_id = tree.root_node_id.clone();
        tree.bind_root_identity("test-agent", &root_node_id, "test-root-receipt")
            .expect("bind test tree identity");
    }

    #[cfg(feature = "pg-tests")]
    fn postgres_test_url() -> Option<String> {
        match std::env::var("ACP_TEST_DATABASE_URL") {
            Ok(url) => Some(url),
            Err(_) if std::env::var("CI").as_deref() == Ok("true") => {
                panic!("ACP_TEST_DATABASE_URL is required for PostgreSQL CI evidence")
            }
            Err(_) => {
                eprintln!(
                    "ACP_TEST_DATABASE_URL not set; PostgreSQL recursive test is explicitly skipped"
                );
                None
            }
        }
    }

    #[test]
    fn scheduler_lifecycle_sync_is_restart_safe_and_retry_bounded() {
        let _guard = crate::recursive_execution::test_env_lock().lock().unwrap();
        std::env::set_var("ACP_RECURSIVE_EXECUTION_ENABLED", "1");
        std::env::remove_var("ACP_RECURSIVE_EXECUTION_KILL_SWITCH");
        let store = LocalProductStore::new(":memory:").expect("store");
        let mut tree = RecursiveTree::new(
            "recursive-sync-run",
            "recursive-sync-workflow",
            "root objective",
            RecursiveScope {
                repository: Some("fixture".to_string()),
                allowed_paths: BTreeSet::from(["docs/".to_string()]),
                capabilities: BTreeSet::from(["read".to_string()]),
            },
            BTreeSet::from(["read".to_string()]),
            RecursiveBudget {
                calls_remaining: 12,
                tokens_remaining: 120,
                cost_micros_remaining: 120,
                time_ms_remaining: 1200,
            },
        );
        bind_test_tree_identity(&mut tree);
        bind_test_workflow(
            &store,
            &tree.root_run_id,
            &tree.workflow_id,
            &tree.root_node_id,
        );
        store.save_recursive_tree(&tree).expect("save");
        store
            .with_conn(|conn| {
                conn.execute(
                    "UPDATE workflow_run_nodes SET status='running', leased_at=?1
                     WHERE run_id=?2 AND node_id=?3",
                    params![
                        "2026-07-18T00:00:00Z",
                        "recursive-sync-run",
                        tree.root_node_id
                    ],
                )
                .map_err(|error| error.to_string())?;
                sync_recursive_lease_sqlite(
                    conn,
                    "recursive-sync-run",
                    &tree.root_node_id,
                    "lease-1",
                    "2026-07-18T00:00:00Z",
                )?;
                sync_recursive_completion_sqlite(
                    conn,
                    "recursive-sync-run",
                    &tree.root_node_id,
                    "lease-1",
                    false,
                    true,
                    &RecursiveBudget {
                        calls_remaining: 0,
                        tokens_remaining: 0,
                        cost_micros_remaining: 0,
                        time_ms_remaining: 0,
                    },
                    "2026-07-18T00:00:01Z",
                )
            })
            .expect("sync lifecycle");
        let loaded = store
            .load_recursive_tree("recursive-sync-run")
            .expect("load")
            .expect("tree");
        let root = loaded.nodes.get(&loaded.root_node_id).expect("root");
        assert_eq!(root.status, "ready");
        assert_eq!(root.retry_count, 1);
        assert!(loaded.active_leases.is_empty());
        std::env::remove_var("ACP_RECURSIVE_EXECUTION_ENABLED");
    }

    #[test]
    fn persisted_tree_rejects_forged_lineage() {
        let store = LocalProductStore::new(":memory:").expect("store");
        let mut tree = RecursiveTree::new(
            "recursive-forged-run",
            "recursive-forged-workflow",
            "root objective",
            RecursiveScope {
                repository: Some("fixture".to_string()),
                allowed_paths: BTreeSet::from(["docs/".to_string()]),
                capabilities: BTreeSet::from(["read".to_string()]),
            },
            BTreeSet::from(["read".to_string()]),
            RecursiveBudget {
                calls_remaining: 2,
                tokens_remaining: 20,
                cost_micros_remaining: 20,
                time_ms_remaining: 200,
            },
        );
        bind_test_tree_identity(&mut tree);
        tree.nodes
            .get_mut(&tree.root_node_id)
            .expect("root")
            .ancestor_fingerprints
            .push("forged".to_string());
        let error = store
            .save_recursive_tree(&tree)
            .expect_err("forged lineage must fail closed");
        assert!(error.contains("root lineage"));
    }

    fn assert_depth_two_tree_round_trips(store: LocalProductStore, suffix: &str) {
        let run_id = format!("recursive-depth-two-run-{suffix}");
        let workflow_id = format!("recursive-depth-two-workflow-{suffix}");
        let root_id = format!("recursive-depth-two-root-{suffix}");
        let mut tree = RecursiveTree::new_with_root_node_id(
            &run_id,
            &workflow_id,
            &root_id,
            "root objective",
            RecursiveScope {
                repository: Some("fixture".to_string()),
                allowed_paths: BTreeSet::from(["docs/".to_string()]),
                capabilities: BTreeSet::from(["read".to_string()]),
            },
            BTreeSet::from(["read".to_string()]),
            RecursiveBudget {
                calls_remaining: 10,
                tokens_remaining: 100,
                cost_micros_remaining: 100,
                time_ms_remaining: 1000,
            },
        );
        bind_test_tree_identity(&mut tree);
        bind_test_workflow(&store, &run_id, &workflow_id, &root_id);
        let child = tree
            .admit_child(&RecursiveProposal {
                proposal_id: format!("recursive-depth-two-child-{suffix}"),
                parent_node_id: root_id.clone(),
                parent_version: tree.nodes[&root_id].version,
                objective: "review child docs".to_string(),
                context_summary: "fixture".to_string(),
                requested_scope: tree.root_scope.clone(),
                requested_capabilities: tree.root_capabilities.clone(),
                budget: RecursiveBudget {
                    calls_remaining: 3,
                    tokens_remaining: 30,
                    cost_micros_remaining: 30,
                    time_ms_remaining: 300,
                },
                receipt_sha256: format!("recursive-depth-two-child-receipt-{suffix}"),
            })
            .expect("child");
        tree.lease_node(&child.node.node_id, "depth-two-child-lease")
            .expect("child lease");
        let grandchild = tree
            .admit_child(&RecursiveProposal {
                proposal_id: format!("recursive-depth-two-grandchild-{suffix}"),
                parent_node_id: child.node.node_id.clone(),
                parent_version: tree.nodes[&child.node.node_id].version,
                objective: "review grandchild docs".to_string(),
                context_summary: "fixture".to_string(),
                requested_scope: child.node.scope.clone(),
                requested_capabilities: child.node.capabilities.clone(),
                budget: RecursiveBudget {
                    calls_remaining: 2,
                    tokens_remaining: 20,
                    cost_micros_remaining: 20,
                    time_ms_remaining: 200,
                },
                receipt_sha256: format!("recursive-depth-two-grandchild-receipt-{suffix}"),
            })
            .expect("grandchild");
        tree.complete_node_with_usage(
            &child.node.node_id,
            "depth-two-child-lease",
            true,
            &RecursiveBudget {
                calls_remaining: 1,
                tokens_remaining: 10,
                cost_micros_remaining: 10,
                time_ms_remaining: 100,
            },
        )
        .expect("child completion");
        tree.release_node_reservation_for_persistence(&child.node.node_id);
        tree.lease_node(&grandchild.node.node_id, "depth-two-grandchild-lease")
            .expect("grandchild lease");
        tree.complete_node(&grandchild.node.node_id, "depth-two-grandchild-lease", true)
            .expect("grandchild completion");
        tree.release_node_reservation_for_persistence(&grandchild.node.node_id);
        store
            .save_recursive_tree_with_expected_version(&tree, 0)
            .expect("persist depth-two tree");
        let loaded = store
            .load_recursive_tree(&run_id)
            .expect("load")
            .expect("tree");
        assert_eq!(loaded.nodes[&child.node.node_id].status, "completed");
        assert_eq!(loaded.nodes[&grandchild.node.node_id].status, "completed");
        assert_eq!(loaded.reserved_budget, RecursiveBudget::default());
        assert_eq!(loaded.spent_budget.tokens_remaining, 10);
    }

    #[test]
    fn sqlite_depth_two_tree_round_trips() {
        let _guard = crate::recursive_execution::test_env_lock().lock().unwrap();
        std::env::set_var("ACP_RECURSIVE_EXECUTION_ENABLED", "1");
        assert_depth_two_tree_round_trips(
            LocalProductStore::new(":memory:").expect("store"),
            "sqlite",
        );
        std::env::remove_var("ACP_RECURSIVE_EXECUTION_ENABLED");
    }

    fn assert_terminal_state_resume_guards(store: LocalProductStore, suffix: &str) {
        let make_tree = |kind: &str| {
            let run_id = format!("recursive-terminal-{kind}-run-{suffix}");
            let workflow_id = format!("recursive-terminal-{kind}-workflow-{suffix}");
            let root_id = format!("recursive-terminal-{kind}-root-{suffix}");
            let mut tree = RecursiveTree::new_with_root_node_id(
                &run_id,
                &workflow_id,
                &root_id,
                "terminal root objective",
                RecursiveScope {
                    repository: Some("fixture".to_string()),
                    allowed_paths: BTreeSet::from(["docs/".to_string()]),
                    capabilities: BTreeSet::from(["read".to_string()]),
                },
                BTreeSet::from(["read".to_string()]),
                RecursiveBudget {
                    calls_remaining: 4,
                    tokens_remaining: 40,
                    cost_micros_remaining: 40,
                    time_ms_remaining: 400,
                },
            );
            bind_test_tree_identity(&mut tree);
            bind_test_workflow(&store, &run_id, &workflow_id, &root_id);
            tree
        };
        let mut operator = make_tree("operator");
        operator.pause();
        store
            .save_recursive_tree_with_expected_version(&operator, 0)
            .expect("operator tree");

        let mut budget = make_tree("budget");
        let budget_root_id = budget.root_node_id.clone();
        budget
            .lease_node(&budget_root_id, "recursive-terminal-budget-lease")
            .expect("budget lease");
        budget.execution_state =
            crate::recursive_execution::RecursiveExecutionState::BudgetExhausted;
        budget.version += 1;
        store
            .save_recursive_tree_with_expected_version(&budget, 0)
            .expect("budget tree");
        match &store.db {
            DatabaseConnection::Sqlite(_) => store
                .with_conn(|conn| {
                    conn.execute(
                        "UPDATE workflow_run_nodes SET status='running', leased_at=?1
                         WHERE run_id=?2 AND node_id=?3",
                        params!["2026-07-18T00:00:00Z", budget.root_run_id, budget_root_id],
                    )
                    .map(|_| ())
                    .map_err(|error| error.to_string())
                })
                .expect("bind budget workflow lease"),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => store
                .with_pg_conn(|client| {
                    client
                        .execute(
                            "UPDATE workflow_run_nodes SET status='running', leased_at=$1
                             WHERE run_id=$2 AND node_id=$3",
                            &[
                                &"2026-07-18T00:00:00Z",
                                &budget.root_run_id,
                                &budget_root_id,
                            ],
                        )
                        .map(|_| ())
                        .map_err(|error| error.to_string())
                })
                .expect("bind budget PostgreSQL workflow lease"),
        }

        let mut killed = make_tree("kill");
        killed.execution_state = crate::recursive_execution::RecursiveExecutionState::KillStopped;
        killed.version += 1;
        store
            .save_recursive_tree_with_expected_version(&killed, 0)
            .expect("kill tree");

        store
            .set_recursive_execution_paused(true, Some("recursive_execution_paused"))
            .expect("terminalize budget lease");
        let budget_after_pause = store
            .load_recursive_tree(&budget.root_run_id)
            .expect("budget load")
            .expect("budget tree");
        assert_eq!(
            budget_after_pause.nodes[&budget_root_id]
                .failure_reason
                .as_deref(),
            Some(RecursiveFailureReason::TreeBudgetExhausted.as_str())
        );
        assert_eq!(
            budget_after_pause.reserved_budget,
            RecursiveBudget::default()
        );

        store
            .set_recursive_execution_paused(false, None)
            .expect("ordinary resume");
        assert_eq!(
            store
                .load_recursive_tree(&operator.root_run_id)
                .expect("operator load")
                .expect("operator tree")
                .execution_state,
            crate::recursive_execution::RecursiveExecutionState::Running
        );
        assert_eq!(
            store
                .load_recursive_tree(&budget.root_run_id)
                .expect("budget load")
                .expect("budget tree")
                .execution_state,
            crate::recursive_execution::RecursiveExecutionState::BudgetExhausted
        );
        assert_eq!(
            store
                .load_recursive_tree(&killed.root_run_id)
                .expect("kill load")
                .expect("kill tree")
                .execution_state,
            crate::recursive_execution::RecursiveExecutionState::KillStopped
        );
    }

    #[test]
    fn sqlite_terminal_state_resume_guards_are_persistent() {
        let _guard = crate::recursive_execution::test_env_lock().lock().unwrap();
        std::env::set_var("ACP_RECURSIVE_EXECUTION_ENABLED", "1");
        assert_terminal_state_resume_guards(
            LocalProductStore::new(":memory:").expect("store"),
            "sqlite",
        );
        std::env::remove_var("ACP_RECURSIVE_EXECUTION_ENABLED");
    }

    #[cfg(feature = "pg-tests")]
    #[test]
    fn postgres_terminal_state_resume_guards_are_persistent() {
        let _guard = crate::recursive_execution::test_env_lock().lock().unwrap();
        let Some(url) = postgres_test_url() else {
            return;
        };
        std::env::set_var("ACP_RECURSIVE_EXECUTION_ENABLED", "1");
        assert_terminal_state_resume_guards(
            LocalProductStore::new_postgres(&url, || "2026-07-18T00:00:00Z".to_string())
                .expect("store"),
            &uuid::Uuid::new_v4().to_string(),
        );
        std::env::remove_var("ACP_RECURSIVE_EXECUTION_ENABLED");
    }

    #[cfg(feature = "pg-tests")]
    #[test]
    fn postgres_depth_two_tree_round_trips() {
        let _guard = crate::recursive_execution::test_env_lock().lock().unwrap();
        let Some(url) = postgres_test_url() else {
            return;
        };
        std::env::set_var("ACP_RECURSIVE_EXECUTION_ENABLED", "1");
        assert_depth_two_tree_round_trips(
            LocalProductStore::new_postgres(&url, || "2026-07-18T00:00:00Z".to_string())
                .expect("store"),
            &uuid::Uuid::new_v4().to_string(),
        );
        std::env::remove_var("ACP_RECURSIVE_EXECUTION_ENABLED");
    }

    #[test]
    fn rejected_proposal_cas_race_preserves_original_reason_and_receipt() {
        let _guard = crate::recursive_execution::test_env_lock().lock().unwrap();
        std::env::set_var("ACP_RECURSIVE_EXECUTION_ENABLED", "1");
        let store = LocalProductStore::new(":memory:").expect("store");
        let mut tree = RecursiveTree::new(
            "recursive-rejection-cas-run",
            "recursive-rejection-cas-workflow",
            "root objective",
            RecursiveScope {
                repository: Some("fixture".to_string()),
                allowed_paths: BTreeSet::from(["docs/".to_string()]),
                capabilities: BTreeSet::from(["read".to_string()]),
            },
            BTreeSet::from(["read".to_string()]),
            RecursiveBudget {
                calls_remaining: 4,
                tokens_remaining: 40,
                cost_micros_remaining: 40,
                time_ms_remaining: 400,
            },
        );
        bind_test_tree_identity(&mut tree);
        bind_test_workflow(
            &store,
            &tree.root_run_id,
            &tree.workflow_id,
            &tree.root_node_id,
        );
        store.save_recursive_tree(&tree).expect("initial tree");

        let proposal_id = "recursive-cas-rejected-proposal";
        let mut candidate = tree.clone();
        let admission = candidate
            .admit_child(&crate::recursive_execution::RecursiveProposal {
                proposal_id: proposal_id.to_string(),
                parent_node_id: tree.root_node_id.clone(),
                parent_version: tree.nodes[&tree.root_node_id].version,
                objective: "independent child objective".to_string(),
                context_summary: "fixture".to_string(),
                requested_scope: tree.root_scope.clone(),
                requested_capabilities: tree.root_capabilities.clone(),
                budget: RecursiveBudget {
                    calls_remaining: 1,
                    tokens_remaining: 10,
                    cost_micros_remaining: 10,
                    time_ms_remaining: 100,
                },
                receipt_sha256: "c".repeat(64),
            })
            .expect("candidate admission");
        let mut advanced = tree.clone();
        advanced.pause();
        store
            .save_recursive_tree_with_expected_version(&advanced, tree.version)
            .expect("concurrent tree update");

        let mutation = AgentActionMutation {
            run_id: tree.root_run_id.clone(),
            node_id: tree.root_node_id.clone(),
            agent_id: "test-agent".to_string(),
            action_sha256: "c".repeat(64),
            action_type: "propose_child_task".to_string(),
            result_json: json!({
                "decision": "accepted",
                "recursive_node_id": admission.node.node_id,
            })
            .to_string(),
            operations: vec![
                AgentMutationOp::InsertProposal {
                    proposal_id: proposal_id.to_string(),
                    correlation_id: "recursive-cas-correlation".to_string(),
                    parent_node_id: tree.root_node_id.clone(),
                    proposal_type: "child_task".to_string(),
                    objective: "duplicate ancestor objective".to_string(),
                    context_summary: "fixture".to_string(),
                    target_agent_id: None,
                    proposed_node_id: None,
                    proposed_edge_id: None,
                },
                AgentMutationOp::PersistRecursiveTree {
                    tree: Box::new(candidate),
                    expected_version: Some(tree.version),
                },
            ],
        };
        let first = store
            .apply_agent_action_once(&mutation)
            .expect("persist rejected CAS decision");
        let persisted = store
            .load_recursive_tree(&tree.root_run_id)
            .expect("load tree")
            .expect("tree");
        assert_eq!(
            persisted.rejected_proposals[proposal_id].reason_code,
            RecursiveFailureReason::StaleParent.as_str()
        );
        assert!(first.contains("\"decision\":\"rejected\""));
        assert!(first.contains("\"reason_code\":\"stale_parent\""));
        assert!(first.contains("\"recursive_node_id\":null"));
        assert_eq!(
            store
                .get_proposal_in_run(proposal_id, &tree.root_run_id)
                .expect("proposal")
                .expect("proposal exists")["status"],
            "rejected"
        );
        let version = persisted.version;
        let audit_count = store
            .audit_events(100)
            .expect("audit")
            .iter()
            .filter(|event| event["action"] == "agent_step.recursive_proposal_rejected")
            .count();
        assert_eq!(
            store
                .apply_agent_action_once(&mutation)
                .expect("exact replay"),
            first
        );
        assert_eq!(
            store
                .load_recursive_tree(&tree.root_run_id)
                .expect("load replayed tree")
                .expect("tree")
                .version,
            version
        );
        assert_eq!(
            store
                .audit_events(100)
                .expect("audit")
                .iter()
                .filter(|event| event["action"] == "agent_step.recursive_proposal_rejected")
                .count(),
            audit_count
        );
        std::env::remove_var("ACP_RECURSIVE_EXECUTION_ENABLED");
    }

    #[test]
    fn budget_overrun_snapshot_persists_as_explicit_terminal_evidence() {
        let _guard = crate::recursive_execution::test_env_lock().lock().unwrap();
        std::env::set_var("ACP_RECURSIVE_EXECUTION_ENABLED", "1");
        let store = LocalProductStore::new(":memory:").expect("store");
        let mut tree = RecursiveTree::new(
            "recursive-budget-overrun-run",
            "recursive-budget-overrun-workflow",
            "root objective",
            RecursiveScope {
                repository: Some("fixture".to_string()),
                allowed_paths: BTreeSet::from(["docs/".to_string()]),
                capabilities: BTreeSet::from(["read".to_string()]),
            },
            BTreeSet::from(["read".to_string()]),
            RecursiveBudget {
                calls_remaining: 2,
                tokens_remaining: 20,
                cost_micros_remaining: 20,
                time_ms_remaining: 200,
            },
        );
        bind_test_tree_identity(&mut tree);
        bind_test_workflow(
            &store,
            &tree.root_run_id,
            &tree.workflow_id,
            &tree.root_node_id,
        );
        store
            .save_recursive_tree_with_expected_version(&tree, 0)
            .expect("initial tree");
        let expected_version = tree.version;
        let child = tree
            .admit_child(&RecursiveProposal {
                proposal_id: "overrun-proposal".to_string(),
                parent_node_id: tree.root_node_id.clone(),
                parent_version: tree.nodes[&tree.root_node_id].version,
                objective: "bounded child".to_string(),
                context_summary: "fixture".to_string(),
                requested_scope: tree.root_scope.clone(),
                requested_capabilities: tree.root_capabilities.clone(),
                budget: RecursiveBudget {
                    calls_remaining: 1,
                    tokens_remaining: 10,
                    cost_micros_remaining: 10,
                    time_ms_remaining: 100,
                },
                receipt_sha256: "overrun-receipt".to_string(),
            })
            .expect("child");
        tree.lease_node(&child.node.node_id, "overrun-lease")
            .expect("lease");
        tree.complete_node_with_usage(
            &child.node.node_id,
            "overrun-lease",
            true,
            &RecursiveBudget {
                calls_remaining: 3,
                tokens_remaining: 21,
                cost_micros_remaining: 21,
                time_ms_remaining: 201,
            },
        )
        .expect("overrun recorded");
        tree.release_node_reservation_for_persistence(&child.node.node_id);
        store
            .save_recursive_tree_with_expected_version(&tree, expected_version)
            .expect("terminal overrun snapshot persists");
        let loaded = store
            .load_recursive_tree(&tree.root_run_id)
            .expect("load")
            .expect("tree");
        assert_eq!(
            loaded.execution_state,
            crate::recursive_execution::RecursiveExecutionState::BudgetExhausted
        );
        assert!(loaded
            .usage_receipts
            .values()
            .any(|receipt| receipt.starts_with("0:")));
        std::env::remove_var("ACP_RECURSIVE_EXECUTION_ENABLED");
    }

    #[test]
    fn persisted_tree_requires_authoritative_workflow_binding() {
        let store = LocalProductStore::new(":memory:").expect("store");
        let mut tree = RecursiveTree::new(
            "recursive-unbound-run",
            "recursive-unbound-workflow",
            "root objective",
            RecursiveScope {
                repository: Some("fixture".to_string()),
                allowed_paths: BTreeSet::from(["docs/".to_string()]),
                capabilities: BTreeSet::from(["read".to_string()]),
            },
            BTreeSet::from(["read".to_string()]),
            RecursiveBudget {
                calls_remaining: 1,
                tokens_remaining: 10,
                cost_micros_remaining: 10,
                time_ms_remaining: 100,
            },
        );
        bind_test_tree_identity(&mut tree);
        let error = store
            .save_recursive_tree(&tree)
            .expect_err("orphan recursive tree must fail closed");
        assert!(error.contains("workflow run binding"));
    }

    #[test]
    fn pause_terminalizes_recursive_leases_and_late_usage_is_idempotent() {
        let _guard = crate::recursive_execution::test_env_lock().lock().unwrap();
        std::env::set_var("ACP_RECURSIVE_EXECUTION_ENABLED", "1");
        let store = LocalProductStore::new(":memory:").expect("store");
        let mut tree = RecursiveTree::new(
            "recursive-pause-run",
            "recursive-pause-workflow",
            "root objective",
            RecursiveScope {
                repository: Some("fixture".to_string()),
                allowed_paths: BTreeSet::from(["docs/".to_string()]),
                capabilities: BTreeSet::from(["read".to_string()]),
            },
            BTreeSet::from(["read".to_string()]),
            RecursiveBudget {
                calls_remaining: 4,
                tokens_remaining: 40,
                cost_micros_remaining: 40,
                time_ms_remaining: 400,
            },
        );
        bind_test_tree_identity(&mut tree);
        let admission = tree
            .admit_child(&RecursiveProposal {
                proposal_id: "recursive-pause-proposal".to_string(),
                parent_node_id: tree.root_node_id.clone(),
                parent_version: tree.nodes[&tree.root_node_id].version,
                objective: "pause child".to_string(),
                context_summary: "fixture".to_string(),
                requested_scope: tree.root_scope.clone(),
                requested_capabilities: tree.root_capabilities.clone(),
                budget: RecursiveBudget {
                    calls_remaining: 1,
                    tokens_remaining: 10,
                    cost_micros_remaining: 10,
                    time_ms_remaining: 100,
                },
                receipt_sha256: "recursive-pause-receipt".to_string(),
            })
            .expect("admit child");
        let child_id = admission.node.node_id;
        store
            .import_workflow_run(&json!({
                "run_id": tree.root_run_id,
                "workflow_id": tree.workflow_id,
                "status": "running",
                "boundaries": {"execution_authority": "disabled"},
                "nodes": [
                    {
                        "node_id": tree.root_node_id,
                        "task_type": "agent_step",
                        "status": "completed",
                        "recursive_node_id": tree.root_node_id,
                        "agent_id": "test-agent",
                        "creation_receipt_sha256": "test-root-receipt"
                    },
                    {
                        "node_id": child_id,
                        "task_type": "agent_step",
                        "status": "running",
                        "leased_at": "2026-07-18T00:00:00Z",
                        "recursive_node_id": child_id,
                        "agent_id": "test-agent",
                        "recursive_capabilities": admission.node.capabilities,
                        "recursive_scope": admission.node.scope,
                        "recursive_tenant_id": admission.node.tenant_id,
                        "recursive_workspace_id": admission.node.workspace_id
                    }
                ],
                "edges": [],
                "events": [],
                "approvals": []
            }))
            .expect("bind pause workflow");
        store
            .save_recursive_tree_with_expected_version(&tree, 0)
            .expect("save");
        store
            .with_conn(|conn| {
                sync_recursive_lease_sqlite(
                    conn,
                    "recursive-pause-run",
                    &child_id,
                    "pause-lease",
                    "2026-07-18T00:00:00Z",
                )
            })
            .expect("lease");
        store
            .set_recursive_execution_paused(true, Some("recursive_execution_paused"))
            .expect("pause");
        let paused = store
            .load_recursive_tree("recursive-pause-run")
            .expect("load")
            .expect("tree");
        let child = paused.nodes.get(&child_id).expect("child");
        assert_eq!(
            paused.execution_state,
            crate::recursive_execution::RecursiveExecutionState::OperatorPaused
        );
        assert_eq!(child.status, "failed");
        assert!(paused.active_leases.is_empty());
        assert_eq!(paused.reserved_budget, RecursiveBudget::default());

        store
            .with_conn(|conn| {
                let usage = RecursiveBudget {
                    calls_remaining: 1,
                    tokens_remaining: 7,
                    cost_micros_remaining: 2,
                    time_ms_remaining: 9,
                };
                assert!(record_recursive_late_usage_sqlite(
                    conn,
                    "recursive-pause-run",
                    &child_id,
                    "late-attempt-1",
                    &usage,
                    "2026-07-18T00:00:01Z",
                )?);
                assert!(record_recursive_late_usage_sqlite(
                    conn,
                    "recursive-pause-run",
                    &child_id,
                    "late-attempt-1",
                    &usage,
                    "2026-07-18T00:00:02Z",
                )?);
                Ok(())
            })
            .expect("late usage");
        let loaded = store
            .load_recursive_tree("recursive-pause-run")
            .expect("load")
            .expect("tree");
        assert_eq!(loaded.spent_budget.tokens_remaining, 7);
        assert_eq!(loaded.nodes[&child_id].actual_usage.tokens_remaining, 7);
        assert_eq!(loaded.nodes[&child_id].budget.tokens_remaining, 3);
        assert_eq!(loaded.usage_receipts.len(), 1);
        std::env::remove_var("ACP_RECURSIVE_EXECUTION_ENABLED");
    }

    #[cfg(feature = "pg-tests")]
    #[test]
    fn postgres_scheduler_lifecycle_sync_is_restart_safe() {
        let _guard = crate::recursive_execution::test_env_lock().lock().unwrap();
        let Some(url) = postgres_test_url() else {
            return;
        };
        std::env::set_var("ACP_RECURSIVE_EXECUTION_ENABLED", "1");
        std::env::remove_var("ACP_RECURSIVE_EXECUTION_KILL_SWITCH");
        let store = LocalProductStore::new_postgres(&url, || "2026-07-18T00:00:00Z".to_string())
            .expect("store");
        let root_run_id = format!("recursive-pg-{}", uuid::Uuid::new_v4());
        let mut tree = RecursiveTree::new(
            &root_run_id,
            "recursive-pg-workflow",
            "root objective",
            RecursiveScope {
                repository: Some("fixture".to_string()),
                allowed_paths: BTreeSet::from(["docs/".to_string()]),
                capabilities: BTreeSet::from(["read".to_string()]),
            },
            BTreeSet::from(["read".to_string()]),
            RecursiveBudget {
                calls_remaining: 2,
                tokens_remaining: 120,
                cost_micros_remaining: 120,
                time_ms_remaining: 1200,
            },
        );
        bind_test_tree_identity(&mut tree);
        assert!(store
            .save_recursive_tree(&tree)
            .expect_err("orphan PostgreSQL tree must fail closed")
            .contains("workflow run binding"));
        bind_test_workflow(
            &store,
            &tree.root_run_id,
            &tree.workflow_id,
            &tree.root_node_id,
        );
        store.save_recursive_tree(&tree).expect("save");
        store
            .with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|error| error.to_string())?;
                tx.execute(
                    "UPDATE workflow_run_nodes SET status='running', leased_at=$1
                     WHERE run_id=$2 AND node_id=$3",
                    &[&"2026-07-18T00:00:01Z", &root_run_id, &tree.root_node_id],
                )
                .map_err(|error| error.to_string())?;
                sync_recursive_lease_pg(
                    &mut tx,
                    &root_run_id,
                    &tree.root_node_id,
                    "lease-pg-1",
                    "2026-07-18T00:00:01Z",
                )?;
                sync_recursive_completion_pg(
                    &mut tx,
                    &root_run_id,
                    &tree.root_node_id,
                    "lease-pg-1",
                    false,
                    false,
                    &RecursiveBudget {
                        calls_remaining: 0,
                        tokens_remaining: 0,
                        cost_micros_remaining: 0,
                        time_ms_remaining: 0,
                    },
                    "2026-07-18T00:00:02Z",
                )?;
                tx.execute(
                    "UPDATE workflow_run_nodes SET status='failed', leased_at=NULL,
                     completed_at=$1 WHERE run_id=$2 AND node_id=$3",
                    &[&"2026-07-18T00:00:02Z", &root_run_id, &tree.root_node_id],
                )
                .map_err(|error| error.to_string())?;
                tx.commit().map_err(|error| error.to_string())
            })
            .expect("sync lifecycle");
        let loaded = store
            .load_recursive_tree(&root_run_id)
            .expect("load")
            .expect("tree");
        let root = loaded.nodes.get(&loaded.root_node_id).expect("root");
        assert_eq!(root.status, "failed");
        assert!(loaded.active_leases.is_empty());

        let paused_run_id = format!("recursive-pg-paused-{}", uuid::Uuid::new_v4());
        let mut paused_tree = RecursiveTree::new(
            &paused_run_id,
            "recursive-pg-paused-workflow",
            "paused root objective",
            RecursiveScope {
                repository: Some("fixture".to_string()),
                allowed_paths: BTreeSet::from(["docs/".to_string()]),
                capabilities: BTreeSet::from(["read".to_string()]),
            },
            BTreeSet::from(["read".to_string()]),
            RecursiveBudget {
                calls_remaining: 4,
                tokens_remaining: 40,
                cost_micros_remaining: 40,
                time_ms_remaining: 400,
            },
        );
        bind_test_tree_identity(&mut paused_tree);
        bind_test_workflow(
            &store,
            &paused_tree.root_run_id,
            &paused_tree.workflow_id,
            &paused_tree.root_node_id,
        );
        store
            .save_recursive_tree(&paused_tree)
            .expect("save paused tree");
        store
            .with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|error| error.to_string())?;
                tx.execute(
                    "UPDATE workflow_run_nodes SET status='running', leased_at=$1
                     WHERE run_id=$2 AND node_id=$3",
                    &[
                        &"2026-07-18T00:00:03Z",
                        &paused_run_id,
                        &paused_tree.root_node_id,
                    ],
                )
                .map_err(|error| error.to_string())?;
                sync_recursive_lease_pg(
                    &mut tx,
                    &paused_run_id,
                    &paused_tree.root_node_id,
                    "lease-pg-pause",
                    "2026-07-18T00:00:03Z",
                )?;
                tx.commit().map_err(|error| error.to_string())
            })
            .expect("pause lease");
        store
            .set_recursive_execution_paused(true, Some("recursive_execution_paused"))
            .expect("pause postgres trees");
        store
            .with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|error| error.to_string())?;
                let usage = RecursiveBudget {
                    calls_remaining: 1,
                    tokens_remaining: 7,
                    cost_micros_remaining: 2,
                    time_ms_remaining: 9,
                };
                assert!(record_recursive_late_usage_pg(
                    &mut tx,
                    &paused_run_id,
                    &paused_tree.root_node_id,
                    "late-pg-attempt-1",
                    &usage,
                    "2026-07-18T00:00:04Z",
                )?);
                assert!(record_recursive_late_usage_pg(
                    &mut tx,
                    &paused_run_id,
                    &paused_tree.root_node_id,
                    "late-pg-attempt-1",
                    &usage,
                    "2026-07-18T00:00:05Z",
                )?);
                tx.commit().map_err(|error| error.to_string())
            })
            .expect("late postgres usage");
        let paused_loaded = store
            .load_recursive_tree(&paused_run_id)
            .expect("load paused tree")
            .expect("paused tree");
        assert_eq!(
            paused_loaded.execution_state,
            crate::recursive_execution::RecursiveExecutionState::OperatorPaused
        );
        assert!(paused_loaded.active_leases.is_empty());
        assert_eq!(paused_loaded.reserved_budget, RecursiveBudget::default());
        assert_eq!(paused_loaded.spent_budget.tokens_remaining, 7);
        assert_eq!(
            paused_loaded.nodes[&paused_tree.root_node_id]
                .actual_usage
                .tokens_remaining,
            7
        );
        assert_eq!(paused_loaded.usage_receipts.len(), 1);
        std::env::remove_var("ACP_RECURSIVE_EXECUTION_ENABLED");
    }

    #[cfg(feature = "pg-tests")]
    #[test]
    fn postgres_recursive_workflow_snapshot_binding_is_enforced() {
        let _guard = crate::recursive_execution::test_env_lock().lock().unwrap();
        let Some(url) = postgres_test_url() else {
            return;
        };
        std::env::set_var("ACP_RECURSIVE_EXECUTION_ENABLED", "1");
        let store = LocalProductStore::new_postgres(&url, || "2026-07-18T00:00:00Z".to_string())
            .expect("store");
        let run_id = format!("recursive-pg-binding-{}", uuid::Uuid::new_v4());
        let mut tree = RecursiveTree::new(
            &run_id,
            "recursive-pg-binding-workflow",
            "root objective",
            RecursiveScope {
                repository: Some("fixture".to_string()),
                allowed_paths: BTreeSet::from(["docs/".to_string()]),
                capabilities: BTreeSet::from(["read".to_string()]),
            },
            BTreeSet::from(["read".to_string()]),
            RecursiveBudget {
                calls_remaining: 2,
                tokens_remaining: 120,
                cost_micros_remaining: 120,
                time_ms_remaining: 1200,
            },
        );
        bind_test_tree_identity(&mut tree);
        assert!(store
            .save_recursive_tree(&tree)
            .expect_err("orphan tree must fail closed")
            .contains("workflow run binding"));
        bind_test_workflow(&store, &run_id, &tree.workflow_id, &tree.root_node_id);
        store.save_recursive_tree(&tree).expect("save root");

        let proposal = RecursiveProposal {
            proposal_id: "proposal-pg-binding".to_string(),
            parent_node_id: tree.root_node_id.clone(),
            parent_version: tree.nodes[&tree.root_node_id].version,
            objective: "child objective".to_string(),
            context_summary: "bounded context".to_string(),
            requested_scope: tree.root_scope.clone(),
            requested_capabilities: tree.root_capabilities.clone(),
            budget: RecursiveBudget {
                calls_remaining: 1,
                tokens_remaining: 10,
                cost_micros_remaining: 10,
                time_ms_remaining: 100,
            },
            receipt_sha256: "a".repeat(64),
        };
        let mut accepted_tree = tree.clone();
        let admission = accepted_tree.admit_child(&proposal).expect("admit child");
        store
            .save_recursive_tree_with_expected_version(&accepted_tree, tree.version)
            .expect("save accepted tree");
        let node = json!({
            "node_id": admission.node.node_id,
            "task_type": "agent_step",
            "status": "pending",
            "attempt_count": 0,
            "agent_id": "agent-pg-binding",
            "recursive_node_id": admission.node.node_id,
            "parent_node_id": tree.root_node_id,
            "objective_fingerprint": admission.node.objective_fingerprint,
            "proposal_id": proposal.proposal_id,
            "acceptance_reason": "accepted",
            "evidence_refs": admission.node.evidence_refs,
            "recursive_capabilities": admission.node.capabilities,
            "recursive_scope": serde_json::to_value(&admission.node.scope)
                .expect("scope json"),
            "recursive_tenant_id": admission.node.tenant_id,
            "recursive_workspace_id": admission.node.workspace_id,
        });
        let edge = json!({
            "edge_id": format!("recursive-edge-{}", admission.node.node_id),
            "from_node_id": tree.root_node_id,
            "to_node_id": admission.node.node_id,
            "edge_type": "dependency",
            "recursive": true,
        });
        store
            .with_pg_conn(|client| {
                validate_recursive_workflow_mutation_pg(
                    client,
                    &run_id,
                    &node,
                    &edge,
                    "agent-pg-binding",
                )?;
                let mut tampered_edge = edge.clone();
                tampered_edge["edge_type"] = json!("control");
                assert!(validate_recursive_workflow_mutation_pg(
                    client,
                    &run_id,
                    &node,
                    &tampered_edge,
                    "agent-pg-binding",
                )
                .is_err());
                assert_eq!(
                    validate_recursive_workflow_mutation_pg(
                        client,
                        "recursive-pg-binding-missing",
                        &node,
                        &edge,
                        "agent-pg-binding",
                    )
                    .expect_err("missing tree must fail closed"),
                    "recursive_tree_missing"
                );
                Ok(())
            })
            .expect("validate PostgreSQL snapshot binding");
        store
            .create_proposal(
                &proposal.proposal_id,
                "corr-pg-binding",
                &run_id,
                &tree.root_node_id,
                "agent-pg-binding",
                "child_task",
                &proposal.objective,
                &proposal.context_summary,
                None,
                node.get("node_id").and_then(Value::as_str),
                edge.get("edge_id").and_then(Value::as_str),
            )
            .expect("create PostgreSQL proposal");
        store
            .apply_agent_action_once(&AgentActionMutation {
                run_id: run_id.clone(),
                node_id: tree.root_node_id.clone(),
                agent_id: "agent-pg-binding".to_string(),
                action_sha256: "b".repeat(64),
                action_type: "propose_child_task".to_string(),
                result_json: json!({"action": "propose_child_task"}).to_string(),
                operations: vec![AgentMutationOp::PersistRecursiveWorkflow {
                    node: node.clone(),
                    edge: edge.clone(),
                }],
            })
            .expect("persist PostgreSQL workflow node");
        let persisted_proposal = store
            .get_proposal_in_run(&proposal.proposal_id, &run_id)
            .expect("load PostgreSQL proposal")
            .expect("PostgreSQL proposal exists");
        assert_eq!(persisted_proposal["status"], "accepted");
        let persisted_run = store
            .get_workflow_run(&run_id)
            .expect("load PostgreSQL workflow run")
            .expect("PostgreSQL workflow run exists");
        assert!(persisted_run["nodes"].as_array().is_some_and(|nodes| {
            nodes.iter().any(|persisted| {
                persisted["node_id"] == node["node_id"]
                    && persisted["recursive_node_id"] == node["recursive_node_id"]
            })
        }));
        assert!(persisted_run["edges"]
            .as_array()
            .is_some_and(|edges| { edges.iter().any(|persisted| persisted == &edge) }));
        std::env::remove_var("ACP_RECURSIVE_EXECUTION_ENABLED");
    }
}
