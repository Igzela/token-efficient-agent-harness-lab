use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};
use serde_json::Value;

use super::{DatabaseConnection, LocalProductStore};
use crate::recursive_execution::{RecursiveNode, RecursiveTree, RECURSIVE_SCHEMA_VERSION};

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
    serde_json::to_string(tree).map_err(|error| error.to_string())
}

pub(crate) fn persist_recursive_tree_sqlite(
    tx: &rusqlite::Connection,
    tree: &RecursiveTree,
    now: &str,
) -> Result<(), String> {
    let tree_json = validate_tree_for_persistence(tree)?;
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

#[cfg(feature = "pg")]
pub(crate) fn persist_recursive_tree_pg(
    client: &mut impl postgres::GenericClient,
    tree: &RecursiveTree,
    now: &str,
) -> Result<(), String> {
    let tree_json = validate_tree_for_persistence(tree)?;
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

impl LocalProductStore {
    /// Persist one complete tree snapshot and its bounded node identity index.
    /// The snapshot is the restart authority; the index is query/evidence aid.
    pub fn save_recursive_tree(&self, tree: &RecursiveTree) -> Result<(), String> {
        if tree.schema_version != RECURSIVE_SCHEMA_VERSION {
            return Err("recursive tree schema version mismatch".to_string());
        }
        let tree_json = serde_json::to_string(tree).map_err(|error| error.to_string())?;
        let now = self.now();
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                    .map_err(|error| error.to_string())?;
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
        let tree_json = match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                conn.query_row(
                    "SELECT tree_json FROM recursive_execution_trees WHERE root_run_id=?1",
                    params![root_run_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(|error| error.to_string())
            })?,
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                client
                    .query_opt(
                        "SELECT tree_json FROM recursive_execution_trees WHERE root_run_id=$1",
                        &[&root_run_id],
                    )
                    .map(|row| row.map(|row| row.get::<_, String>(0)))
                    .map_err(|error| error.to_string())
            })?,
        };
        tree_json
            .map(|value| {
                let tree: RecursiveTree =
                    serde_json::from_str(&value).map_err(|error| error.to_string())?;
                if tree.root_run_id != root_run_id
                    || tree.schema_version != RECURSIVE_SCHEMA_VERSION
                {
                    return Err("recursive tree identity or schema conflict".to_string());
                }
                Ok(tree)
            })
            .transpose()
    }

    pub fn recursive_tree_operator_evidence(&self, root_run_id: &str) -> Result<Value, String> {
        self.load_recursive_tree(root_run_id)?
            .map(|tree| tree.redacted_read_model())
            .ok_or_else(|| "recursive tree not found".to_string())
    }
}
