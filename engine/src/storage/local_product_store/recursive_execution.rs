use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};
use serde_json::{json, Value};

use super::{append_audit_locked, DatabaseConnection, LocalProductStore};
use crate::recursive_execution::{
    RecursiveFailureReason, RecursiveNode, RecursiveScope, RecursiveTree,
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
    if let Some(limit) = tree.root_budget_limit.as_ref() {
        if !limit.can_spend(&tree.root_budget) {
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
    let Some(workflow_id) = workflow_id else {
        return Ok(());
    };
    if workflow_id != tree.workflow_id {
        return Err("recursive workflow identity is not bound to workflow run".to_string());
    }
    let root_exists: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM workflow_run_nodes WHERE run_id=?1 AND node_id=?2",
            params![tree.root_run_id, tree.root_node_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if root_exists != 1 {
        return Err("recursive root node is not bound to workflow run".to_string());
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
    let Some(workflow_id) = workflow_id else {
        return Ok(());
    };
    if workflow_id != tree.workflow_id {
        return Err("recursive workflow identity is not bound to workflow run".to_string());
    }
    let root_exists: i64 = client
        .query_one(
            "SELECT COUNT(*) FROM workflow_run_nodes WHERE run_id=$1 AND node_id=$2",
            &[&tree.root_run_id, &tree.root_node_id],
        )
        .map_err(|error| error.to_string())?
        .get(0);
    if root_exists != 1 {
        return Err("recursive root node is not bound to workflow run".to_string());
    }
    Ok(())
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
) -> Result<Vec<String>, String> {
    let Some(mut current) = load_recursive_tree_sqlite(conn, &candidate.root_run_id)? else {
        return Err("recursive_tree_missing".to_string());
    };
    let expected_version = current.version;
    let rejected: Vec<String> = candidate
        .accepted_proposals
        .difference(&current.accepted_proposals)
        .cloned()
        .collect();
    for proposal_id in &rejected {
        current.record_rejection(proposal_id, RecursiveFailureReason::StaleParent);
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
) -> Result<Vec<String>, String> {
    let Some(mut current) = load_recursive_tree_pg(client, &candidate.root_run_id)? else {
        return Err("recursive_tree_missing".to_string());
    };
    let expected_version = current.version;
    let rejected: Vec<String> = candidate
        .accepted_proposals
        .difference(&current.accepted_proposals)
        .cloned()
        .collect();
    for proposal_id in &rejected {
        current.record_rejection(proposal_id, RecursiveFailureReason::StaleParent);
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

    pub(crate) fn record_recursive_rejection(
        &self,
        root_run_id: &str,
        proposal_id: &str,
        reason: RecursiveFailureReason,
    ) -> Result<(), String> {
        let now = self.now();
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                    .map_err(|error| error.to_string())?;
                let Some(mut tree) = load_recursive_tree_sqlite(&tx, root_run_id)? else {
                    return Err("recursive_tree_missing".to_string());
                };
                let expected_version = tree.version;
                tree.record_rejection(proposal_id, reason);
                persist_recursive_tree_sqlite(&tx, &tree, &now, Some(expected_version))?;
                tx.commit().map_err(|error| error.to_string())
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|error| error.to_string())?;
                let Some(mut tree) = load_recursive_tree_pg(&mut tx, root_run_id)? else {
                    return Err("recursive_tree_missing".to_string());
                };
                let expected_version = tree.version;
                tree.record_rejection(proposal_id, reason);
                persist_recursive_tree_pg(&mut tx, &tree, &now, Some(expected_version))?;
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
                    if tree.paused != paused {
                        tree.paused = paused;
                        tree.version += 1;
                    }
                    if let Some(reason) = terminal_reason {
                        for node in tree.nodes.values_mut() {
                            if node.lease_id.take().is_some() {
                                terminated.push(node.node_id.clone());
                                node.status = "failed".to_string();
                                node.failure_reason = Some(reason.to_string());
                                node.version += 1;
                            }
                        }
                        if !terminated.is_empty() {
                            tree.active_leases.clear();
                            tree.version += 1;
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
                        if paused {
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
                                "reason": terminal_reason.unwrap_or("recursive_execution_paused"),
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
                        let Some((_status, _leased_at, node_json_text)) =
                            workflow_row.filter(|(status, leased_at, node_json)| {
                                status == "running"
                                    && leased_at.is_some()
                                    && serde_json::from_str::<Value>(node_json)
                                        .ok()
                                        .and_then(|value| {
                                            value
                                                .get("recursive_node_id")
                                                .and_then(Value::as_str)
                                                .map(|id| id == workflow_node_id)
                                        })
                                        .unwrap_or(false)
                            })
                        else {
                            return Err("recursive workflow node binding is missing".to_string());
                        };
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
                                 WHERE run_id=?4 AND node_id=?5 AND status='running'
                                   AND leased_at IS NOT NULL",
                                params![
                                    now,
                                    terminal_reason.unwrap_or("recursive_execution_paused"),
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
                    if tree.paused != paused {
                        tree.paused = paused;
                        tree.version += 1;
                    }
                    if let Some(reason) = terminal_reason {
                        for node in tree.nodes.values_mut() {
                            if node.lease_id.take().is_some() {
                                terminated.push(node.node_id.clone());
                                node.status = "failed".to_string();
                                node.failure_reason = Some(reason.to_string());
                                node.version += 1;
                            }
                        }
                        if !terminated.is_empty() {
                            tree.active_leases.clear();
                            tree.version += 1;
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
                        if paused {
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
                                "reason": terminal_reason.unwrap_or("recursive_execution_paused"),
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
                        let Some((_status, _leased_at, node_json_text)) =
                            workflow_row.filter(|(status, leased_at, node_json)| {
                                status == "running"
                                    && leased_at.is_some()
                                    && serde_json::from_str::<Value>(node_json)
                                        .ok()
                                        .and_then(|value| {
                                            value
                                                .get("recursive_node_id")
                                                .and_then(Value::as_str)
                                                .map(|id| id == workflow_node_id)
                                        })
                                        .unwrap_or(false)
                            })
                        else {
                            return Err("recursive workflow node binding is missing".to_string());
                        };
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
                                 WHERE run_id=$4 AND node_id=$5 AND status='running'
                                   AND leased_at IS NOT NULL",
                                &[
                                    &now,
                                    &terminal_reason.unwrap_or("recursive_execution_paused"),
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

fn load_recursive_tree_sqlite(
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

pub(crate) fn sync_recursive_completion_sqlite(
    conn: &rusqlite::Connection,
    root_run_id: &str,
    recursive_node_id: &str,
    lease_id: &str,
    success: bool,
    retry: bool,
    usage: &crate::recursive_execution::RecursiveBudget,
    now: &str,
) -> Result<(), String> {
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
    }
    persist_recursive_tree_sqlite(conn, &tree, now, Some(expected_version))
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
    persist_recursive_tree_sqlite(conn, &tree, now, Some(expected_version))?;
    Ok(within_tree_budget)
}

#[cfg(feature = "pg")]
fn load_recursive_tree_pg(
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
pub(crate) fn sync_recursive_completion_pg(
    client: &mut impl postgres::GenericClient,
    root_run_id: &str,
    recursive_node_id: &str,
    lease_id: &str,
    success: bool,
    retry: bool,
    usage: &crate::recursive_execution::RecursiveBudget,
    now: &str,
) -> Result<(), String> {
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
    }
    persist_recursive_tree_pg(client, &tree, now, Some(expected_version))
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
    persist_recursive_tree_pg(client, &tree, now, Some(expected_version))?;
    Ok(within_tree_budget)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recursive_execution::{RecursiveBudget, RecursiveScope};
    use std::collections::BTreeSet;

    #[test]
    fn scheduler_lifecycle_sync_is_restart_safe_and_retry_bounded() {
        let _guard = crate::recursive_execution::test_env_lock().lock().unwrap();
        std::env::set_var("ACP_RECURSIVE_EXECUTION_ENABLED", "1");
        std::env::remove_var("ACP_RECURSIVE_EXECUTION_KILL_SWITCH");
        let store = LocalProductStore::new(":memory:").expect("store");
        let tree = RecursiveTree::new(
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
        store.save_recursive_tree(&tree).expect("save");
        store
            .with_conn(|conn| {
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

    #[test]
    fn pause_terminalizes_recursive_leases_and_late_usage_is_idempotent() {
        let _guard = crate::recursive_execution::test_env_lock().lock().unwrap();
        std::env::set_var("ACP_RECURSIVE_EXECUTION_ENABLED", "1");
        let store = LocalProductStore::new(":memory:").expect("store");
        let tree = RecursiveTree::new(
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
        store.save_recursive_tree(&tree).expect("save");
        store
            .with_conn(|conn| {
                sync_recursive_lease_sqlite(
                    conn,
                    "recursive-pause-run",
                    &tree.root_node_id,
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
        let root = paused.nodes.get(&paused.root_node_id).expect("root");
        assert!(paused.paused);
        assert_eq!(root.status, "failed");
        assert!(paused.active_leases.is_empty());

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
                    &tree.root_node_id,
                    "late-attempt-1",
                    &usage,
                    "2026-07-18T00:00:01Z",
                )?);
                assert!(record_recursive_late_usage_sqlite(
                    conn,
                    "recursive-pause-run",
                    &tree.root_node_id,
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
        assert_eq!(loaded.usage_receipts.len(), 1);
        std::env::remove_var("ACP_RECURSIVE_EXECUTION_ENABLED");
    }

    #[cfg(feature = "pg-tests")]
    #[test]
    fn postgres_scheduler_lifecycle_sync_is_restart_safe() {
        let _guard = crate::recursive_execution::test_env_lock().lock().unwrap();
        let Ok(url) = std::env::var("ACP_TEST_DATABASE_URL") else {
            eprintln!("ACP_TEST_DATABASE_URL not set; skipping recursive PostgreSQL test");
            return;
        };
        std::env::set_var("ACP_RECURSIVE_EXECUTION_ENABLED", "1");
        std::env::remove_var("ACP_RECURSIVE_EXECUTION_KILL_SWITCH");
        let store = LocalProductStore::new_postgres(&url, || "2026-07-18T00:00:00Z".to_string())
            .expect("store");
        let root_run_id = format!("recursive-pg-{}", uuid::Uuid::new_v4());
        let tree = RecursiveTree::new(
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
        store.save_recursive_tree(&tree).expect("save");
        store
            .with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|error| error.to_string())?;
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
        let paused_tree = RecursiveTree::new(
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
        store
            .save_recursive_tree(&paused_tree)
            .expect("save paused tree");
        store
            .with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|error| error.to_string())?;
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
        assert!(paused_loaded.paused);
        assert!(paused_loaded.active_leases.is_empty());
        assert_eq!(paused_loaded.spent_budget.tokens_remaining, 7);
        assert_eq!(paused_loaded.usage_receipts.len(), 1);
        std::env::remove_var("ACP_RECURSIVE_EXECUTION_ENABLED");
    }
}
