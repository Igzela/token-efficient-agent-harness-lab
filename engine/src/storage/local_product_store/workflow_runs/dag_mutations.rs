use rusqlite::params;
use serde_json::{json, Value};

use super::super::{DatabaseConnection, LocalProductStore};
use super::{
    ensure_run_exists_locked, insert_workflow_run_event_locked, workflow_run_edges_locked,
    workflow_run_events_locked, workflow_run_nodes_locked,
};
#[cfg(feature = "pg")]
use super::{
    pg_ensure_run_exists, pg_insert_workflow_run_event, pg_workflow_run_edges,
    pg_workflow_run_events, pg_workflow_run_nodes,
};
use crate::workflow::dag_manager::{types::DAGMutationProposal, DAGManager};

fn insert_workflow_run_node_with_mode(
    conn: &rusqlite::Connection,
    run_id: &str,
    node: &Value,
    strict: bool,
) -> Result<(), String> {
    let node_id = node
        .get("node_id")
        .and_then(Value::as_str)
        .ok_or_else(|| "workflow node missing node_id".to_string())?;
    let task_type = node
        .get("task_type")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let status = node
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("pending");
    let profile_id = node.get("profile_id").and_then(Value::as_str);
    let statement = if strict {
        "INSERT INTO workflow_run_nodes
         (run_id, node_id, task_type, status, node_json,
          started_at, completed_at, attempt_count, timeout_ms, blocked_reason, leased_at, profile_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)"
    } else {
        "INSERT OR REPLACE INTO workflow_run_nodes
         (run_id, node_id, task_type, status, node_json,
          started_at, completed_at, attempt_count, timeout_ms, blocked_reason, leased_at, profile_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)"
    };
    conn.execute(
        statement,
        params![
            run_id,
            node_id,
            task_type,
            status,
            node.to_string(),
            node.get("started_at").and_then(Value::as_str),
            node.get("completed_at").and_then(Value::as_str),
            node.get("attempt_count")
                .and_then(Value::as_i64)
                .unwrap_or(0),
            node.get("timeout_ms").and_then(Value::as_i64),
            node.get("blocked_reason").and_then(Value::as_str),
            node.get("leased_at").and_then(Value::as_str),
            profile_id,
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub(crate) fn insert_workflow_run_node_locked(
    conn: &rusqlite::Connection,
    run_id: &str,
    node: &Value,
) -> Result<(), String> {
    insert_workflow_run_node_with_mode(conn, run_id, node, false)
}

fn insert_workflow_run_edge_with_mode(
    conn: &rusqlite::Connection,
    run_id: &str,
    edge: &Value,
    strict: bool,
) -> Result<(), String> {
    let edge_id = edge
        .get("edge_id")
        .and_then(Value::as_str)
        .ok_or_else(|| "workflow edge missing edge_id".to_string())?;
    let from_node_id = edge
        .get("from_node_id")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("workflow edge {edge_id} missing from_node_id"))?;
    let to_node_id = edge
        .get("to_node_id")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("workflow edge {edge_id} missing to_node_id"))?;
    let edge_type = edge
        .get("edge_type")
        .and_then(Value::as_str)
        .unwrap_or("dependency");
    let statement = if strict {
        "INSERT INTO workflow_run_edges
         (run_id, edge_id, from_node_id, to_node_id, edge_type, edge_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)"
    } else {
        "INSERT OR REPLACE INTO workflow_run_edges
         (run_id, edge_id, from_node_id, to_node_id, edge_type, edge_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)"
    };
    conn.execute(
        statement,
        params![
            run_id,
            edge_id,
            from_node_id,
            to_node_id,
            edge_type,
            edge.to_string(),
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub(crate) fn insert_workflow_run_edge_locked(
    conn: &rusqlite::Connection,
    run_id: &str,
    edge: &Value,
) -> Result<(), String> {
    insert_workflow_run_edge_with_mode(conn, run_id, edge, false)
}

#[cfg(feature = "pg")]
fn pg_insert_workflow_run_node_with_mode(
    client: &mut impl postgres::GenericClient,
    run_id: &str,
    node: &Value,
    strict: bool,
) -> Result<(), String> {
    let node_id = node
        .get("node_id")
        .and_then(Value::as_str)
        .ok_or_else(|| "workflow node missing node_id".to_string())?;
    let task_type = node
        .get("task_type")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let status = node
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("pending");
    let profile_id = node.get("profile_id").and_then(Value::as_str);
    let node_json = node.to_string();
    let started_at = node.get("started_at").and_then(Value::as_str);
    let completed_at = node.get("completed_at").and_then(Value::as_str);
    let attempt_count: i32 = node
        .get("attempt_count")
        .and_then(Value::as_i64)
        .unwrap_or(0) as i32;
    let timeout_ms: Option<i32> = node
        .get("timeout_ms")
        .and_then(Value::as_i64)
        .map(|v| v as i32);
    let blocked_reason = node.get("blocked_reason").and_then(Value::as_str);
    let leased_at = node.get("leased_at").and_then(Value::as_str);
    let params: Vec<&(dyn postgres::types::ToSql + Sync)> = vec![
        &run_id,
        &node_id,
        &task_type,
        &status,
        &node_json,
        &started_at,
        &completed_at,
        &attempt_count,
        &timeout_ms,
        &blocked_reason,
        &leased_at,
        &profile_id,
    ];
    let statement = if strict {
        "INSERT INTO workflow_run_nodes
         (run_id, node_id, task_type, status, node_json,
          started_at, completed_at, attempt_count, timeout_ms, blocked_reason, leased_at, profile_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)"
    } else {
        "INSERT INTO workflow_run_nodes
             (run_id, node_id, task_type, status, node_json,
              started_at, completed_at, attempt_count, timeout_ms, blocked_reason, leased_at, profile_id)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
             ON CONFLICT (run_id, node_id) DO UPDATE SET
              task_type = EXCLUDED.task_type, status = EXCLUDED.status, node_json = EXCLUDED.node_json,
              started_at = EXCLUDED.started_at, completed_at = EXCLUDED.completed_at,
              attempt_count = EXCLUDED.attempt_count, timeout_ms = EXCLUDED.timeout_ms,
              blocked_reason = EXCLUDED.blocked_reason, leased_at = EXCLUDED.leased_at,
              profile_id = EXCLUDED.profile_id"
    };
    client
        .execute(statement, &params)
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(feature = "pg")]
pub(crate) fn pg_insert_workflow_run_node(
    client: &mut impl postgres::GenericClient,
    run_id: &str,
    node: &Value,
) -> Result<(), String> {
    pg_insert_workflow_run_node_with_mode(client, run_id, node, false)
}

#[cfg(feature = "pg")]
fn pg_insert_workflow_run_edge_with_mode(
    client: &mut impl postgres::GenericClient,
    run_id: &str,
    edge: &Value,
    strict: bool,
) -> Result<(), String> {
    let edge_id = edge
        .get("edge_id")
        .and_then(Value::as_str)
        .ok_or_else(|| "workflow edge missing edge_id".to_string())?;
    let from_node_id = edge
        .get("from_node_id")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("workflow edge {edge_id} missing from_node_id"))?;
    let to_node_id = edge
        .get("to_node_id")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("workflow edge {edge_id} missing to_node_id"))?;
    let edge_type = edge
        .get("edge_type")
        .and_then(Value::as_str)
        .unwrap_or("dependency");
    let edge_json = edge.to_string();
    let params: Vec<&(dyn postgres::types::ToSql + Sync)> = vec![
        &run_id,
        &edge_id,
        &from_node_id,
        &to_node_id,
        &edge_type,
        &edge_json,
    ];
    let statement = if strict {
        "INSERT INTO workflow_run_edges
         (run_id, edge_id, from_node_id, to_node_id, edge_type, edge_json)
         VALUES ($1, $2, $3, $4, $5, $6)"
    } else {
        "INSERT INTO workflow_run_edges
             (run_id, edge_id, from_node_id, to_node_id, edge_type, edge_json)
             VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT (run_id, edge_id) DO UPDATE SET
              from_node_id = EXCLUDED.from_node_id, to_node_id = EXCLUDED.to_node_id,
              edge_type = EXCLUDED.edge_type, edge_json = EXCLUDED.edge_json"
    };
    client
        .execute(statement, &params)
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(feature = "pg")]
pub(crate) fn pg_insert_workflow_run_edge(
    client: &mut impl postgres::GenericClient,
    run_id: &str,
    edge: &Value,
) -> Result<(), String> {
    pg_insert_workflow_run_edge_with_mode(client, run_id, edge, false)
}

pub(crate) fn insert_recursive_workflow_run_node_locked(
    conn: &rusqlite::Connection,
    run_id: &str,
    node: &Value,
) -> Result<(), String> {
    let node_id = node
        .get("node_id")
        .and_then(Value::as_str)
        .ok_or_else(|| "workflow node missing node_id".to_string())?;
    let existing: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM workflow_run_nodes WHERE run_id=?1 AND node_id=?2",
            params![run_id, node_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if existing != 0 {
        return Err(format!("duplicate node_id: {node_id}"));
    }
    insert_workflow_run_node_with_mode(conn, run_id, node, true)
}

pub(crate) fn insert_recursive_workflow_run_edge_locked(
    conn: &rusqlite::Connection,
    run_id: &str,
    edge: &Value,
) -> Result<(), String> {
    let edge_id = edge
        .get("edge_id")
        .and_then(Value::as_str)
        .ok_or_else(|| "workflow edge missing edge_id".to_string())?;
    let from_node_id = edge
        .get("from_node_id")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("workflow edge {edge_id} missing from_node_id"))?;
    let to_node_id = edge
        .get("to_node_id")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("workflow edge {edge_id} missing to_node_id"))?;
    let (from_count, to_count, edge_count): (i64, i64, i64) = conn
        .query_row(
            "SELECT
                (SELECT COUNT(*) FROM workflow_run_nodes WHERE run_id=?1 AND node_id=?2),
                (SELECT COUNT(*) FROM workflow_run_nodes WHERE run_id=?1 AND node_id=?3),
                (SELECT COUNT(*) FROM workflow_run_edges WHERE run_id=?1 AND edge_id=?4)",
            params![run_id, from_node_id, to_node_id, edge_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(|error| error.to_string())?;
    if (from_count, to_count) != (1, 1) {
        return Err(format!(
            "workflow edge {edge_id} references missing endpoint"
        ));
    }
    if edge_count != 0 {
        return Err(format!("duplicate edge_id: {edge_id}"));
    }
    insert_workflow_run_edge_with_mode(conn, run_id, edge, true)
}

#[cfg(feature = "pg")]
pub(crate) fn pg_insert_recursive_workflow_run_node(
    client: &mut impl postgres::GenericClient,
    run_id: &str,
    node: &Value,
) -> Result<(), String> {
    let node_id = node
        .get("node_id")
        .and_then(Value::as_str)
        .ok_or_else(|| "workflow node missing node_id".to_string())?;
    let existing: i64 = client
        .query_one(
            "SELECT COUNT(*) FROM workflow_run_nodes WHERE run_id=$1 AND node_id=$2",
            &[&run_id, &node_id],
        )
        .map_err(|error| error.to_string())?
        .get(0);
    if existing != 0 {
        return Err(format!("duplicate node_id: {node_id}"));
    }
    pg_insert_workflow_run_node_with_mode(client, run_id, node, true)
}

#[cfg(feature = "pg")]
pub(crate) fn pg_insert_recursive_workflow_run_edge(
    client: &mut impl postgres::GenericClient,
    run_id: &str,
    edge: &Value,
) -> Result<(), String> {
    let edge_id = edge
        .get("edge_id")
        .and_then(Value::as_str)
        .ok_or_else(|| "workflow edge missing edge_id".to_string())?;
    let from_node_id = edge
        .get("from_node_id")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("workflow edge {edge_id} missing from_node_id"))?;
    let to_node_id = edge
        .get("to_node_id")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("workflow edge {edge_id} missing to_node_id"))?;
    let row = client
        .query_one(
            "SELECT
                (SELECT COUNT(*) FROM workflow_run_nodes WHERE run_id=$1 AND node_id=$2),
                (SELECT COUNT(*) FROM workflow_run_nodes WHERE run_id=$1 AND node_id=$3),
                (SELECT COUNT(*) FROM workflow_run_edges WHERE run_id=$1 AND edge_id=$4)",
            &[&run_id, &from_node_id, &to_node_id, &edge_id],
        )
        .map_err(|error| error.to_string())?;
    let identity = (
        row.get::<_, i64>(0),
        row.get::<_, i64>(1),
        row.get::<_, i64>(2),
    );
    if (identity.0, identity.1) != (1, 1) {
        return Err(format!(
            "workflow edge {edge_id} references missing endpoint"
        ));
    }
    if identity.2 != 0 {
        return Err(format!("duplicate edge_id: {edge_id}"));
    }
    pg_insert_workflow_run_edge_with_mode(client, run_id, edge, true)
}

impl LocalProductStore {
    pub fn insert_workflow_node(
        &self,
        run_id: &str,
        node: &Value,
        actor: &str,
        reason: &str,
    ) -> Result<Value, String> {
        let node_id = node
            .get("node_id")
            .and_then(Value::as_str)
            .ok_or_else(|| "workflow node missing node_id".to_string())?
            .to_string();
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                ensure_run_exists_locked(conn, run_id)?;
                let existing: i64 = conn
                    .query_row(
                        "SELECT COUNT(*) FROM workflow_run_nodes WHERE run_id = ?1 AND node_id = ?2",
                        params![run_id, node_id],
                        |row| row.get(0),
                    )
                    .map_err(|e| e.to_string())?;
                if existing > 0 {
                    return Err(format!("duplicate node_id: {node_id}"));
                }
                insert_workflow_run_node_locked(conn, run_id, node)?;
                let created_at = self.now();
                insert_workflow_run_event_locked(
                    conn,
                    run_id,
                    Some(&node_id),
                    "dag.mutation.node_added",
                    actor,
                    &json!({"node_id": node_id, "reason": reason, "metadata_only": true}),
                    &created_at,
                )?;
                Ok(())
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                pg_ensure_run_exists(client, run_id)?;
                let existing: i64 = client
                    .query_one(
                        "SELECT COUNT(*) FROM workflow_run_nodes WHERE run_id = $1 AND node_id = $2",
                        &[&run_id, &node_id],
                    )
                    .map_err(|e| e.to_string())?
                    .get(0);
                if existing > 0 {
                    return Err(format!("duplicate node_id: {node_id}"));
                }
                pg_insert_workflow_run_node(client, run_id, node)?;
                let created_at = self.now();
                pg_insert_workflow_run_event(
                    client,
                    run_id,
                    Some(&node_id),
                    "dag.mutation.node_added",
                    actor,
                    &json!({"node_id": node_id, "reason": reason, "metadata_only": true}),
                    &created_at,
                )?;
                Ok(())
            }),
        }?;
        Ok(json!({
            "action": "node_inserted",
            "node_id": node_id,
            "run_id": run_id,
            "metadata_only": true,
        }))
    }

    pub fn remove_workflow_node(
        &self,
        run_id: &str,
        node_id: &str,
        actor: &str,
        reason: &str,
    ) -> Result<Value, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                ensure_run_exists_locked(conn, run_id)?;
                let connected_edges: i64 = conn
                    .query_row(
                        "SELECT COUNT(*) FROM workflow_run_edges WHERE run_id = ?1 AND (from_node_id = ?2 OR to_node_id = ?2)",
                        params![run_id, node_id],
                        |row| row.get(0),
                    )
                    .map_err(|e| e.to_string())?;
                conn.execute(
                    "DELETE FROM workflow_run_edges WHERE run_id = ?1 AND (from_node_id = ?2 OR to_node_id = ?2)",
                    params![run_id, node_id],
                )
                .map_err(|e| e.to_string())?;
                conn.execute(
                    "DELETE FROM workflow_run_nodes WHERE run_id = ?1 AND node_id = ?2",
                    params![run_id, node_id],
                )
                .map_err(|e| e.to_string())?;
                let created_at = self.now();
                insert_workflow_run_event_locked(
                    conn,
                    run_id,
                    Some(node_id),
                    "dag.mutation.node_removed",
                    actor,
                    &json!({"node_id": node_id, "removed_edges": connected_edges, "reason": reason, "metadata_only": true}),
                    &created_at,
                )?;
                Ok(json!({
                    "action": "node_removed",
                    "node_id": node_id,
                    "run_id": run_id,
                    "removed_edges": connected_edges,
                    "metadata_only": true,
                }))
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                pg_ensure_run_exists(client, run_id)?;
                let connected_edges: i64 = client
                    .query_one(
                        "SELECT COUNT(*) FROM workflow_run_edges WHERE run_id = $1 AND (from_node_id = $2 OR to_node_id = $2)",
                        &[&run_id, &node_id],
                    )
                    .map_err(|e| e.to_string())?
                    .get(0);
                client.execute(
                    "DELETE FROM workflow_run_edges WHERE run_id = $1 AND (from_node_id = $2 OR to_node_id = $2)",
                    &[&run_id, &node_id],
                )
                .map_err(|e| e.to_string())?;
                client.execute(
                    "DELETE FROM workflow_run_nodes WHERE run_id = $1 AND node_id = $2",
                    &[&run_id, &node_id],
                )
                .map_err(|e| e.to_string())?;
                let created_at = self.now();
                pg_insert_workflow_run_event(
                    client,
                    run_id,
                    Some(node_id),
                    "dag.mutation.node_removed",
                    actor,
                    &json!({"node_id": node_id, "removed_edges": connected_edges, "reason": reason, "metadata_only": true}),
                    &created_at,
                )?;
                Ok(json!({
                    "action": "node_removed",
                    "node_id": node_id,
                    "run_id": run_id,
                    "removed_edges": connected_edges,
                    "metadata_only": true,
                }))
            }),
        }
    }

    pub fn update_workflow_node_status(
        &self,
        run_id: &str,
        node_id: &str,
        new_status: &str,
        actor: &str,
        reason: &str,
    ) -> Result<Value, String> {
        let valid_statuses = [
            "pending",
            "running",
            "completed",
            "failed",
            "cancelled",
            "blocked",
            "waiting_human",
            "recovered",
        ];
        if !valid_statuses.contains(&new_status) {
            return Err(format!("invalid node status: {new_status}"));
        }
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                ensure_run_exists_locked(conn, run_id)?;
                let now = self.now();
                conn.execute(
                    "UPDATE workflow_run_nodes SET status = ?1 WHERE run_id = ?2 AND node_id = ?3",
                    params![new_status, run_id, node_id],
                )
                .map_err(|e| e.to_string())?;
                let node_json_text: String = conn
                    .query_row(
                        "SELECT node_json FROM workflow_run_nodes WHERE run_id = ?1 AND node_id = ?2",
                        params![run_id, node_id],
                        |row| row.get(0),
                    )
                    .map_err(|e| e.to_string())?;
                let mut node_json: Value =
                    serde_json::from_str(&node_json_text).unwrap_or(Value::Null);
                if let Some(obj) = node_json.as_object_mut() {
                    obj.insert("status".to_string(), json!(new_status));
                }
                conn.execute(
                    "UPDATE workflow_run_nodes SET node_json = ?1 WHERE run_id = ?2 AND node_id = ?3",
                    params![node_json.to_string(), run_id, node_id],
                )
                .map_err(|e| e.to_string())?;
                insert_workflow_run_event_locked(
                    conn,
                    run_id,
                    Some(node_id),
                    "dag.mutation.node_status_updated",
                    actor,
                    &json!({"node_id": node_id, "new_status": new_status, "reason": reason, "metadata_only": true}),
                    &now,
                )?;
                Ok(json!({
                    "action": "node_status_updated",
                    "node_id": node_id,
                    "run_id": run_id,
                    "new_status": new_status,
                    "metadata_only": true,
                }))
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                pg_ensure_run_exists(client, run_id)?;
                let now = self.now();
                client.execute(
                    "UPDATE workflow_run_nodes SET status = $1 WHERE run_id = $2 AND node_id = $3",
                    &[&new_status, &run_id, &node_id],
                )
                .map_err(|e| e.to_string())?;
                let node_json_text: String = client
                    .query_one(
                        "SELECT node_json FROM workflow_run_nodes WHERE run_id = $1 AND node_id = $2",
                        &[&run_id, &node_id],
                    )
                    .map_err(|e| e.to_string())?
                    .get(0);
                let mut node_json: Value =
                    serde_json::from_str(&node_json_text).unwrap_or(Value::Null);
                if let Some(obj) = node_json.as_object_mut() {
                    obj.insert("status".to_string(), json!(new_status));
                }
                client.execute(
                    "UPDATE workflow_run_nodes SET node_json = $1 WHERE run_id = $2 AND node_id = $3",
                    &[&node_json.to_string(), &run_id, &node_id],
                )
                .map_err(|e| e.to_string())?;
                pg_insert_workflow_run_event(
                    client,
                    run_id,
                    Some(node_id),
                    "dag.mutation.node_status_updated",
                    actor,
                    &json!({"node_id": node_id, "new_status": new_status, "reason": reason, "metadata_only": true}),
                    &now,
                )?;
                Ok(json!({
                    "action": "node_status_updated",
                    "node_id": node_id,
                    "run_id": run_id,
                    "new_status": new_status,
                    "metadata_only": true,
                }))
            }),
        }
    }

    pub fn insert_workflow_edge(
        &self,
        run_id: &str,
        edge: &Value,
        actor: &str,
        reason: &str,
    ) -> Result<Value, String> {
        let edge_id = edge
            .get("edge_id")
            .and_then(Value::as_str)
            .ok_or_else(|| "workflow edge missing edge_id".to_string())?
            .to_string();
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                ensure_run_exists_locked(conn, run_id)?;
                let existing: i64 = conn
                    .query_row(
                        "SELECT COUNT(*) FROM workflow_run_edges WHERE run_id = ?1 AND edge_id = ?2",
                        params![run_id, edge_id],
                        |row| row.get(0),
                    )
                    .map_err(|e| e.to_string())?;
                if existing > 0 {
                    return Err(format!("duplicate edge_id: {edge_id}"));
                }
                insert_workflow_run_edge_locked(conn, run_id, edge)?;
                let created_at = self.now();
                insert_workflow_run_event_locked(
                    conn,
                    run_id,
                    None,
                    "dag.mutation.edge_added",
                    actor,
                    &json!({"edge_id": edge_id, "reason": reason, "metadata_only": true}),
                    &created_at,
                )?;
                Ok(())
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                pg_ensure_run_exists(client, run_id)?;
                let existing: i64 = client
                    .query_one(
                        "SELECT COUNT(*) FROM workflow_run_edges WHERE run_id = $1 AND edge_id = $2",
                        &[&run_id, &edge_id],
                    )
                    .map_err(|e| e.to_string())?
                    .get(0);
                if existing > 0 {
                    return Err(format!("duplicate edge_id: {edge_id}"));
                }
                pg_insert_workflow_run_edge(client, run_id, edge)?;
                let created_at = self.now();
                pg_insert_workflow_run_event(
                    client,
                    run_id,
                    None,
                    "dag.mutation.edge_added",
                    actor,
                    &json!({"edge_id": edge_id, "reason": reason, "metadata_only": true}),
                    &created_at,
                )?;
                Ok(())
            }),
        }?;
        Ok(json!({
            "action": "edge_inserted",
            "edge_id": edge_id,
            "run_id": run_id,
            "metadata_only": true,
        }))
    }

    pub fn remove_workflow_edge(
        &self,
        run_id: &str,
        edge_id: &str,
        actor: &str,
        reason: &str,
    ) -> Result<Value, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                ensure_run_exists_locked(conn, run_id)?;
                conn.execute(
                    "DELETE FROM workflow_run_edges WHERE run_id = ?1 AND edge_id = ?2",
                    params![run_id, edge_id],
                )
                .map_err(|e| e.to_string())?;
                let created_at = self.now();
                insert_workflow_run_event_locked(
                    conn,
                    run_id,
                    None,
                    "dag.mutation.edge_removed",
                    actor,
                    &json!({"edge_id": edge_id, "reason": reason, "metadata_only": true}),
                    &created_at,
                )?;
                Ok(json!({
                    "action": "edge_removed",
                    "edge_id": edge_id,
                    "run_id": run_id,
                    "metadata_only": true,
                }))
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                pg_ensure_run_exists(client, run_id)?;
                client
                    .execute(
                        "DELETE FROM workflow_run_edges WHERE run_id = $1 AND edge_id = $2",
                        &[&run_id, &edge_id],
                    )
                    .map_err(|e| e.to_string())?;
                let created_at = self.now();
                pg_insert_workflow_run_event(
                    client,
                    run_id,
                    None,
                    "dag.mutation.edge_removed",
                    actor,
                    &json!({"edge_id": edge_id, "reason": reason, "metadata_only": true}),
                    &created_at,
                )?;
                Ok(json!({
                    "action": "edge_removed",
                    "edge_id": edge_id,
                    "run_id": run_id,
                    "metadata_only": true,
                }))
            }),
        }
    }

    pub fn rewire_workflow_edge(
        &self,
        run_id: &str,
        edge_id: &str,
        new_from: Option<&str>,
        new_to: Option<&str>,
        actor: &str,
        reason: &str,
    ) -> Result<Value, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                ensure_run_exists_locked(conn, run_id)?;
                let edge_json_text: String = conn
                    .query_row(
                        "SELECT edge_json FROM workflow_run_edges WHERE run_id = ?1 AND edge_id = ?2",
                        params![run_id, edge_id],
                        |row| row.get(0),
                    )
                    .map_err(|e| e.to_string())?;
                let mut edge_json: Value =
                    serde_json::from_str(&edge_json_text).unwrap_or(Value::Null);
                let current_from = edge_json
                    .get("from_node_id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let current_to = edge_json
                    .get("to_node_id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let final_from = new_from.unwrap_or(&current_from);
                let final_to = new_to.unwrap_or(&current_to);
                if let Some(obj) = edge_json.as_object_mut() {
                    obj.insert("from_node_id".to_string(), json!(final_from));
                    obj.insert("to_node_id".to_string(), json!(final_to));
                }
                conn.execute(
                    "UPDATE workflow_run_edges SET from_node_id = ?1, to_node_id = ?2, edge_json = ?3 WHERE run_id = ?4 AND edge_id = ?5",
                    params![final_from, final_to, edge_json.to_string(), run_id, edge_id],
                )
                .map_err(|e| e.to_string())?;
                let created_at = self.now();
                insert_workflow_run_event_locked(
                    conn,
                    run_id,
                    None,
                    "dag.mutation.edge_rewired",
                    actor,
                    &json!({"edge_id": edge_id, "old_from": current_from, "old_to": current_to, "new_from": final_from, "new_to": final_to, "reason": reason, "metadata_only": true}),
                    &created_at,
                )?;
                Ok(json!({
                    "action": "edge_rewired",
                    "edge_id": edge_id,
                    "run_id": run_id,
                    "old_from": current_from,
                    "old_to": current_to,
                    "new_from": final_from,
                    "new_to": final_to,
                    "metadata_only": true,
                }))
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                pg_ensure_run_exists(client, run_id)?;
                let edge_json_text: String = client
                    .query_one(
                        "SELECT edge_json FROM workflow_run_edges WHERE run_id = $1 AND edge_id = $2",
                        &[&run_id, &edge_id],
                    )
                    .map_err(|e| e.to_string())?
                    .get(0);
                let mut edge_json: Value =
                    serde_json::from_str(&edge_json_text).unwrap_or(Value::Null);
                let current_from = edge_json
                    .get("from_node_id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let current_to = edge_json
                    .get("to_node_id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let final_from = new_from.unwrap_or(&current_from);
                let final_to = new_to.unwrap_or(&current_to);
                if let Some(obj) = edge_json.as_object_mut() {
                    obj.insert("from_node_id".to_string(), json!(final_from));
                    obj.insert("to_node_id".to_string(), json!(final_to));
                }
                client.execute(
                    "UPDATE workflow_run_edges SET from_node_id = $1, to_node_id = $2, edge_json = $3 WHERE run_id = $4 AND edge_id = $5",
                    &[&final_from, &final_to, &edge_json.to_string(), &run_id, &edge_id],
                )
                .map_err(|e| e.to_string())?;
                let created_at = self.now();
                pg_insert_workflow_run_event(
                    client,
                    run_id,
                    None,
                    "dag.mutation.edge_rewired",
                    actor,
                    &json!({"edge_id": edge_id, "old_from": current_from, "old_to": current_to, "new_from": final_from, "new_to": final_to, "reason": reason, "metadata_only": true}),
                    &created_at,
                )?;
                Ok(json!({
                    "action": "edge_rewired",
                    "edge_id": edge_id,
                    "run_id": run_id,
                    "old_from": current_from,
                    "old_to": current_to,
                    "new_from": final_from,
                    "new_to": final_to,
                    "metadata_only": true,
                }))
            }),
        }
    }

    pub fn apply_dag_mutations_batch(
        &self,
        run_id: &str,
        proposals: &[DAGMutationProposal],
        actor: &str,
    ) -> Result<Vec<Value>, String> {
        let (base_nodes, base_edges) = match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                ensure_run_exists_locked(conn, run_id)?;
                let nodes = workflow_run_nodes_locked(conn, run_id)?;
                let edges = workflow_run_edges_locked(conn, run_id)?;
                Ok((nodes, edges))
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                pg_ensure_run_exists(client, run_id)?;
                let nodes = pg_workflow_run_nodes(client, run_id)?;
                let edges = pg_workflow_run_edges(client, run_id)?;
                Ok((nodes, edges))
            }),
        }?;

        let dag_nodes: Vec<crate::workflow::dag_manager::DAGNode> = base_nodes
            .iter()
            .map(|n| crate::workflow::dag_manager::DAGNode {
                node_id: n
                    .get("node_id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                task_id: n.get("task_type").and_then(Value::as_str).map(String::from),
                node_type: n
                    .get("task_type")
                    .and_then(Value::as_str)
                    .unwrap_or("task")
                    .to_string(),
                status: n
                    .get("db_status")
                    .and_then(Value::as_str)
                    .or_else(|| n.get("status").and_then(Value::as_str))
                    .unwrap_or("pending")
                    .to_string(),
                tier: n
                    .get("tier")
                    .and_then(Value::as_str)
                    .unwrap_or("cheap_executor")
                    .to_string(),
                metadata: Default::default(),
            })
            .collect();

        let dag_edges: Vec<crate::workflow::dag_manager::DAGEdge> = base_edges
            .iter()
            .map(|e| crate::workflow::dag_manager::DAGEdge {
                edge_id: e
                    .get("edge_id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                from_node: e
                    .get("from_node_id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                to_node: e
                    .get("to_node_id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                dependency_type: e
                    .get("edge_type")
                    .and_then(Value::as_str)
                    .unwrap_or("dependency")
                    .to_string(),
                status: "pending".to_string(),
            })
            .collect();

        let mut mgr = DAGManager::new(run_id, &self.now());
        {
            let state = mgr.current_state();
            let _ = state;
        }
        for n in &dag_nodes {
            let mut payload = std::collections::HashMap::new();
            payload.insert("node_id".to_string(), json!(n.node_id));
            payload.insert("node_type".to_string(), json!(n.node_type));
            payload.insert("status".to_string(), json!(n.status));
            payload.insert("tier".to_string(), json!(n.tier));
            let proposal = DAGMutationProposal {
                proposal_id: format!("load_node_{}", n.node_id),
                dag_id: run_id.to_string(),
                mutation_type: "add_node".to_string(),
                payload,
                ..Default::default()
            };
            mgr.apply_mutation(&proposal);
        }
        for e in &dag_edges {
            let mut payload = std::collections::HashMap::new();
            payload.insert("edge_id".to_string(), json!(e.edge_id));
            payload.insert("from_node".to_string(), json!(e.from_node));
            payload.insert("to_node".to_string(), json!(e.to_node));
            let proposal = DAGMutationProposal {
                proposal_id: format!("load_edge_{}", e.edge_id),
                dag_id: run_id.to_string(),
                mutation_type: "add_edge".to_string(),
                payload,
                ..Default::default()
            };
            mgr.apply_mutation(&proposal);
        }

        let mut results = Vec::new();
        for proposal in proposals {
            let result = mgr.apply_mutation(proposal);
            if !result.applied {
                results.push(json!({
                    "proposal_id": proposal.proposal_id,
                    "applied": false,
                    "errors": result.errors,
                }));
                continue;
            }
            let persist_result = match proposal.mutation_type.as_str() {
                "add_node" => {
                    let node_value = proposal
                        .payload
                        .get("node_id")
                        .cloned()
                        .map(|nid| {
                            let mut n = json!({
                                "node_id": nid,
                                "task_type": proposal.payload.get("task_type").or_else(|| proposal.payload.get("node_type")).and_then(Value::as_str).unwrap_or("task"),
                                "status": proposal.payload.get("status").and_then(Value::as_str).unwrap_or("pending"),
                            });
                            if let Some(pid) = proposal.payload.get("profile_id") {
                                if let Some(obj) = n.as_object_mut() {
                                    obj.insert("profile_id".to_string(), pid.clone());
                                }
                            }
                            n
                        })
                        .unwrap_or(Value::Null);
                    self.insert_workflow_node(run_id, &node_value, actor, &proposal.reason)
                        .map(Some)
                }
                "remove_node" => {
                    let nid = proposal.target_node_id.as_deref().unwrap_or("");
                    self.remove_workflow_node(run_id, nid, actor, &proposal.reason)
                        .map(Some)
                }
                "add_edge" => {
                    let edge_value = json!({
                        "edge_id": proposal.payload.get("edge_id").and_then(Value::as_str).unwrap_or(""),
                        "from_node_id": proposal.payload.get("from_node").and_then(Value::as_str).unwrap_or(""),
                        "to_node_id": proposal.payload.get("to_node").and_then(Value::as_str).unwrap_or(""),
                    });
                    self.insert_workflow_edge(run_id, &edge_value, actor, &proposal.reason)
                        .map(Some)
                }
                "remove_edge" => {
                    let eid = proposal.target_edge_id.as_deref().unwrap_or("");
                    self.remove_workflow_edge(run_id, eid, actor, &proposal.reason)
                        .map(Some)
                }
                "rewire_edge" => {
                    let eid = proposal.target_edge_id.as_deref().unwrap_or("");
                    let new_from = proposal.payload.get("from_node").and_then(Value::as_str);
                    let new_to = proposal.payload.get("to_node").and_then(Value::as_str);
                    self.rewire_workflow_edge(
                        run_id,
                        eid,
                        new_from,
                        new_to,
                        actor,
                        &proposal.reason,
                    )
                    .map(Some)
                }
                "update_node" => {
                    let nid = proposal.target_node_id.as_deref().unwrap_or("");
                    let status = proposal
                        .payload
                        .get("status")
                        .and_then(Value::as_str)
                        .unwrap_or("pending");
                    self.update_workflow_node_status(run_id, nid, status, actor, &proposal.reason)
                        .map(Some)
                }
                _ => Err(format!(
                    "unsupported mutation type: {}",
                    proposal.mutation_type
                )),
            };
            match persist_result {
                Ok(_) => {
                    match &self.db {
                        DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                            let created_at = self.now();
                            insert_workflow_run_event_locked(
                                conn,
                                run_id,
                                None,
                                "dag.mutation.applied",
                                actor,
                                &json!({
                                    "proposal_id": proposal.proposal_id,
                                    "mutation_type": proposal.mutation_type,
                                    "metadata_only": true,
                                }),
                                &created_at,
                            )?;
                            Ok(())
                        }),
                        #[cfg(feature = "pg")]
                        DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                            let created_at = self.now();
                            pg_insert_workflow_run_event(
                                client,
                                run_id,
                                None,
                                "dag.mutation.applied",
                                actor,
                                &json!({
                                    "proposal_id": proposal.proposal_id,
                                    "mutation_type": proposal.mutation_type,
                                    "metadata_only": true,
                                }),
                                &created_at,
                            )?;
                            Ok(())
                        }),
                    }?;
                    results.push(json!({
                        "proposal_id": proposal.proposal_id,
                        "applied": true,
                        "new_dag_version": result.new_dag_version,
                    }));
                }
                Err(e) => {
                    results.push(json!({
                        "proposal_id": proposal.proposal_id,
                        "applied": false,
                        "errors": [e],
                    }));
                }
            }
        }
        Ok(results)
    }

    pub fn replay_mutation_events(&self, run_id: &str) -> Result<Value, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                ensure_run_exists_locked(conn, run_id)?;
                Ok(())
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                pg_ensure_run_exists(client, run_id)?;
                Ok(())
            }),
        }?;

        let run = self
            .get_workflow_run(run_id)?
            .ok_or_else(|| format!("workflow run not found: {run_id}"))?;

        let base_nodes = run
            .get("nodes")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let base_edges = run
            .get("edges")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        let all_events = match &self.db {
            DatabaseConnection::Sqlite(_) => {
                self.with_conn(|conn| workflow_run_events_locked(conn, run_id, 100_000))?
            }
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => {
                self.with_pg_conn(|client| pg_workflow_run_events(client, run_id, 100_000))?
            }
        };

        let mutation_events: Vec<&Value> = all_events
            .iter()
            .filter(|e| {
                e.get("event_type")
                    .and_then(Value::as_str)
                    .map(|t| t.starts_with("dag.mutation."))
                    .unwrap_or(false)
            })
            .collect();

        let mut nodes: Vec<Value> = base_nodes.clone();
        let mut edges: Vec<Value> = base_edges.clone();
        let mut mutations_replayed = 0u64;

        for event in &mutation_events {
            let event_type = event
                .get("event_type")
                .and_then(Value::as_str)
                .unwrap_or("");
            let details = event.get("details").cloned().unwrap_or(Value::Null);

            match event_type {
                "dag.mutation.node_added" => {
                    if let Some(node_id) = details.get("node_id").and_then(Value::as_str) {
                        if !nodes
                            .iter()
                            .any(|n| n.get("node_id").and_then(Value::as_str) == Some(node_id))
                        {
                            nodes.push(json!({
                                "node_id": node_id,
                                "status": "pending",
                                "task_type": "unknown",
                            }));
                        }
                    }
                    mutations_replayed += 1;
                }
                "dag.mutation.node_removed" => {
                    if let Some(node_id) = details.get("node_id").and_then(Value::as_str) {
                        nodes.retain(|n| n.get("node_id").and_then(Value::as_str) != Some(node_id));
                    }
                    mutations_replayed += 1;
                }
                "dag.mutation.node_status_updated" => {
                    if let Some(node_id) = details.get("node_id").and_then(Value::as_str) {
                        if let Some(new_status) = details.get("new_status").and_then(Value::as_str)
                        {
                            if let Some(node) = nodes
                                .iter_mut()
                                .find(|n| n.get("node_id").and_then(Value::as_str) == Some(node_id))
                            {
                                if let Some(obj) = node.as_object_mut() {
                                    obj.insert("status".to_string(), json!(new_status));
                                }
                            }
                        }
                    }
                    mutations_replayed += 1;
                }
                "dag.mutation.edge_added" => {
                    if let Some(edge_id) = details.get("edge_id").and_then(Value::as_str) {
                        if !edges
                            .iter()
                            .any(|e| e.get("edge_id").and_then(Value::as_str) == Some(edge_id))
                        {
                            edges.push(json!({
                                "edge_id": edge_id,
                            }));
                        }
                    }
                    mutations_replayed += 1;
                }
                "dag.mutation.edge_removed" => {
                    if let Some(edge_id) = details.get("edge_id").and_then(Value::as_str) {
                        edges.retain(|e| e.get("edge_id").and_then(Value::as_str) != Some(edge_id));
                    }
                    mutations_replayed += 1;
                }
                "dag.mutation.edge_rewired" => {
                    if let Some(edge_id) = details.get("edge_id").and_then(Value::as_str) {
                        if let Some(edge) = edges
                            .iter_mut()
                            .find(|e| e.get("edge_id").and_then(Value::as_str) == Some(edge_id))
                        {
                            if let Some(obj) = edge.as_object_mut() {
                                if let Some(new_from) =
                                    details.get("new_from").and_then(Value::as_str)
                                {
                                    obj.insert("from_node_id".to_string(), json!(new_from));
                                }
                                if let Some(new_to) = details.get("new_to").and_then(Value::as_str)
                                {
                                    obj.insert("to_node_id".to_string(), json!(new_to));
                                }
                            }
                        }
                    }
                    mutations_replayed += 1;
                }
                _ => {}
            }
        }

        let protected_completed_nodes: Vec<String> = base_nodes
            .iter()
            .filter(|n| {
                let status = n
                    .get("db_status")
                    .and_then(Value::as_str)
                    .or_else(|| n.get("status").and_then(Value::as_str))
                    .unwrap_or("");
                matches!(status, "completed" | "failed")
            })
            .filter_map(|n| n.get("node_id").and_then(Value::as_str).map(String::from))
            .collect();

        Ok(json!({
            "run_id": run_id,
            "mutations_replayed": mutations_replayed,
            "nodes": nodes,
            "edges": edges,
            "protected_completed_nodes": protected_completed_nodes,
            "metadata_only": true,
        }))
    }
}
