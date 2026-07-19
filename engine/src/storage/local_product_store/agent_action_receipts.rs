use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};

use super::agent_runtime::{
    apply_size_cap, apply_size_cap_and_redact, redact_message_body, MAX_BODY_SUMMARY_BYTES,
    MAX_PROPOSAL_CONTEXT_BYTES, MAX_PROPOSAL_OBJECTIVE_BYTES, MAX_SCRATCHPAD_BYTES,
};
use super::{append_audit_locked, DatabaseConnection, LocalProductStore};
use crate::recursive_execution::RecursiveTree;

fn mark_recursive_proposals_accepted_sqlite(
    conn: &rusqlite::Connection,
    mutation: &AgentActionMutation,
    tree: &RecursiveTree,
    now: &str,
) -> Result<(), String> {
    for proposal_id in &tree.accepted_proposals {
        let status: Option<String> = conn
            .query_row(
                "SELECT status FROM agent_proposals WHERE proposal_id=?1 AND run_id=?2",
                params![proposal_id, mutation.run_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        match status.as_deref() {
            Some("pending") => {
                let updated = conn
                    .execute(
                        "UPDATE agent_proposals SET status='accepted', updated_at=?1
                         WHERE proposal_id=?2 AND run_id=?3 AND status='pending'",
                        params![now, proposal_id, mutation.run_id],
                    )
                    .map_err(|error| error.to_string())?;
                if updated != 1 {
                    return Err("recursive proposal acceptance raced".to_string());
                }
            }
            Some("accepted") => {}
            Some("rejected") => return Err("recursive proposal was rejected".to_string()),
            _ => return Err("recursive proposal record is missing".to_string()),
        }
    }
    Ok(())
}

fn mark_recursive_proposals_rejected_sqlite(
    conn: &rusqlite::Connection,
    mutation: &AgentActionMutation,
    tree: &RecursiveTree,
    now: &str,
) -> Result<(), String> {
    for (proposal_id, evidence) in &tree.rejected_proposals {
        let status: Option<String> = conn
            .query_row(
                "SELECT status FROM agent_proposals WHERE proposal_id=?1 AND run_id=?2",
                params![proposal_id, mutation.run_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        match status.as_deref() {
            Some("pending") => {
                let updated = conn
                    .execute(
                        "UPDATE agent_proposals SET status='rejected', updated_at=?1
                         WHERE proposal_id=?2 AND run_id=?3 AND status='pending'",
                        params![now, proposal_id, mutation.run_id],
                    )
                    .map_err(|error| error.to_string())?;
                if updated != 1 {
                    return Err("recursive proposal rejection raced".to_string());
                }
                append_audit_locked(
                    conn,
                    now,
                    &format!("agent:{}", mutation.agent_id),
                    "agent_step.recursive_proposal_rejected",
                    &format!("agent_proposal/{proposal_id}"),
                    &json!({
                        "run_id": mutation.run_id,
                        "proposal_id": proposal_id,
                        "reason_code": evidence.reason_code,
                        "evidence_refs": evidence.evidence_refs,
                    }),
                )?;
            }
            Some("rejected") => {}
            Some("accepted") => return Err("recursive proposal was already accepted".to_string()),
            _ => return Err("recursive proposal record is missing".to_string()),
        }
    }
    Ok(())
}

#[cfg(feature = "pg")]
fn mark_recursive_proposals_accepted_pg(
    client: &mut impl postgres::GenericClient,
    mutation: &AgentActionMutation,
    tree: &RecursiveTree,
    now: &str,
) -> Result<(), String> {
    for proposal_id in &tree.accepted_proposals {
        let status: Option<String> = client
            .query_opt(
                "SELECT status FROM agent_proposals WHERE proposal_id=$1 AND run_id=$2",
                &[&proposal_id, &mutation.run_id],
            )
            .map_err(|error| error.to_string())?
            .map(|row| row.get(0));
        match status.as_deref() {
            Some("pending") => {
                let updated = client
                    .execute(
                        "UPDATE agent_proposals SET status='accepted', updated_at=$1
                         WHERE proposal_id=$2 AND run_id=$3 AND status='pending'",
                        &[&now, &proposal_id, &mutation.run_id],
                    )
                    .map_err(|error| error.to_string())?;
                if updated != 1 {
                    return Err("recursive proposal acceptance raced".to_string());
                }
            }
            Some("accepted") => {}
            Some("rejected") => return Err("recursive proposal was rejected".to_string()),
            _ => return Err("recursive proposal record is missing".to_string()),
        }
    }
    Ok(())
}

#[cfg(feature = "pg")]
fn mark_recursive_proposals_rejected_pg(
    client: &mut impl postgres::GenericClient,
    mutation: &AgentActionMutation,
    tree: &RecursiveTree,
    now: &str,
) -> Result<(), String> {
    for (proposal_id, evidence) in &tree.rejected_proposals {
        let status: Option<String> = client
            .query_opt(
                "SELECT status FROM agent_proposals WHERE proposal_id=$1 AND run_id=$2",
                &[&proposal_id, &mutation.run_id],
            )
            .map_err(|error| error.to_string())?
            .map(|row| row.get(0));
        match status.as_deref() {
            Some("pending") => {
                let updated = client
                    .execute(
                        "UPDATE agent_proposals SET status='rejected', updated_at=$1
                         WHERE proposal_id=$2 AND run_id=$3 AND status='pending'",
                        &[&now, &proposal_id, &mutation.run_id],
                    )
                    .map_err(|error| error.to_string())?;
                if updated != 1 {
                    return Err("recursive proposal rejection raced".to_string());
                }
                super::workflow_runs::pg_append_audit(
                    client,
                    now,
                    &format!("agent:{}", mutation.agent_id),
                    "agent_step.recursive_proposal_rejected",
                    &format!("agent_proposal/{proposal_id}"),
                    &json!({
                        "run_id": mutation.run_id,
                        "proposal_id": proposal_id,
                        "reason_code": evidence.reason_code,
                        "evidence_refs": evidence.evidence_refs,
                    }),
                )?;
            }
            Some("rejected") => {}
            Some("accepted") => return Err("recursive proposal was already accepted".to_string()),
            _ => return Err("recursive proposal record is missing".to_string()),
        }
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub(crate) struct AgentActionMutation {
    pub run_id: String,
    pub node_id: String,
    pub agent_id: String,
    pub action_sha256: String,
    pub action_type: String,
    pub result_json: String,
    pub operations: Vec<AgentMutationOp>,
}

#[derive(Debug, Clone)]
pub(crate) enum AgentMutationOp {
    UpdateAgentState {
        expected_updated_at: String,
        status: Option<String>,
        scratchpad_summary: Option<String>,
        metadata_patch: Option<Value>,
    },
    AckMessage {
        message_id: String,
    },
    InsertProposal {
        proposal_id: String,
        correlation_id: String,
        parent_node_id: String,
        proposal_type: String,
        objective: String,
        context_summary: String,
        target_agent_id: Option<String>,
        proposed_node_id: Option<String>,
        proposed_edge_id: Option<String>,
    },
    PersistRecursiveTree {
        tree: Box<RecursiveTree>,
        /// The tree version observed before admission. `Some(0)` means the
        /// action is creating the tree and requires it to be absent.
        expected_version: Option<u64>,
    },
    PersistRecursiveWorkflow {
        node: Value,
        edge: Value,
    },
    UpdateProposalStatus {
        proposal_id: String,
        new_status: String,
    },
    UpdateProposalStatusBound {
        proposal_id: String,
        new_status: String,
        expected_proposal_type: String,
        expected_correlation_id: String,
        expected_owner_agent_id: Option<String>,
        expected_target_agent_id: Option<String>,
        expected_review_blocking: Option<bool>,
    },
    UpdateDebateContext {
        proposal_id: String,
        expected_correlation_id: String,
        expected_context_summary: String,
        new_context_summary: String,
    },
    InsertMessage {
        message_id: String,
        from_agent_id: String,
        to_agent_id: String,
        message_type: String,
        body: Option<String>,
        correlation_id: Option<String>,
        reply_to_message_id: Option<String>,
        metadata: Value,
    },
    AppendAudit {
        action: String,
        resource: String,
        details: Value,
    },
}

impl LocalProductStore {
    pub(crate) fn committed_agent_action_result(
        &self,
        run_id: &str,
        node_id: &str,
        agent_id: &str,
    ) -> Result<Option<String>, String> {
        let existing: Option<(String, String)> = match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                conn.query_row(
                    "SELECT agent_id, result_json FROM agent_action_receipts
                     WHERE run_id = ?1 AND node_id = ?2",
                    params![run_id, node_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()
                .map_err(|error| error.to_string())
            })?,
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                client
                    .query_opt(
                        "SELECT agent_id, result_json FROM agent_action_receipts
                         WHERE run_id = $1 AND node_id = $2",
                        &[&run_id, &node_id],
                    )
                    .map_err(|error| error.to_string())
                    .map(|row| row.map(|row| (row.get(0), row.get(1))))
            })?,
        };
        let Some((bound_agent_id, result_json)) = existing else {
            return Ok(None);
        };
        if bound_agent_id != agent_id {
            return Err(format!(
                "agent action receipt agent binding changed for {run_id}/{node_id}"
            ));
        }
        serde_json::from_str::<Value>(&result_json)
            .map_err(|error| format!("invalid stored agent action result JSON: {error}"))?;
        Ok(Some(result_json))
    }

    pub(crate) fn apply_agent_action_once(
        &self,
        mutation: &AgentActionMutation,
    ) -> Result<String, String> {
        validate_mutation(mutation)?;
        let now = self.now();
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = conn
                    .unchecked_transaction()
                    .map_err(|error| error.to_string())?;
                let inserted = tx.execute(
                    "INSERT INTO agent_action_receipts
                     (run_id, node_id, agent_id, action_sha256, action_type, result_json, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        mutation.run_id,
                        mutation.node_id,
                        mutation.agent_id,
                        mutation.action_sha256,
                        mutation.action_type,
                        mutation.result_json,
                        now,
                    ],
                );
                if let Err(insert_error) = inserted {
                    tx.rollback().map_err(|error| error.to_string())?;
                    return existing_receipt_sqlite(conn, mutation)?.ok_or_else(|| {
                        format!("failed to claim agent action receipt: {insert_error}")
                    });
                }
                for operation in &mutation.operations {
                    apply_sqlite_operation(&tx, mutation, operation, &now)?;
                }
                let committed_result: String = tx
                    .query_row(
                        "SELECT result_json FROM agent_action_receipts
                         WHERE run_id=?1 AND node_id=?2",
                        params![mutation.run_id, mutation.node_id],
                        |row| row.get(0),
                    )
                    .map_err(|error| error.to_string())?;
                append_audit_locked(
                    &tx,
                    &now,
                    "agent_step",
                    "agent_action.committed",
                    &format!("agent_action/{}/{}", mutation.run_id, mutation.node_id),
                    &json!({
                        "run_id": mutation.run_id,
                        "node_id": mutation.node_id,
                        "agent_id": mutation.agent_id,
                        "action_sha256": mutation.action_sha256,
                        "action_type": mutation.action_type,
                    }),
                )?;
                tx.commit().map_err(|error| error.to_string())?;
                Ok(committed_result)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|error| error.to_string())?;
                let inserted = tx.execute(
                    "INSERT INTO agent_action_receipts
                     (run_id, node_id, agent_id, action_sha256, action_type, result_json, created_at)
                     VALUES ($1, $2, $3, $4, $5, $6, $7)",
                    &[
                        &mutation.run_id,
                        &mutation.node_id,
                        &mutation.agent_id,
                        &mutation.action_sha256,
                        &mutation.action_type,
                        &mutation.result_json,
                        &now,
                    ],
                );
                if let Err(insert_error) = inserted {
                    tx.rollback().map_err(|error| error.to_string())?;
                    return existing_receipt_pg(client, mutation)?.ok_or_else(|| {
                        format!("failed to claim agent action receipt: {insert_error}")
                    });
                }
                for operation in &mutation.operations {
                    apply_pg_operation(&mut tx, mutation, operation, &now)?;
                }
                let committed_result: String = tx
                    .query_one(
                        "SELECT result_json FROM agent_action_receipts
                         WHERE run_id=$1 AND node_id=$2",
                        &[&mutation.run_id, &mutation.node_id],
                    )
                    .map_err(|error| error.to_string())?
                    .get(0);
                pg_append_audit(
                    &mut tx,
                    &now,
                    "agent_step",
                    "agent_action.committed",
                    &format!("agent_action/{}/{}", mutation.run_id, mutation.node_id),
                    &json!({
                        "run_id": mutation.run_id,
                        "node_id": mutation.node_id,
                        "agent_id": mutation.agent_id,
                        "action_sha256": mutation.action_sha256,
                        "action_type": mutation.action_type,
                    }),
                )?;
                tx.commit().map_err(|error| error.to_string())?;
                Ok(committed_result)
            }),
        }
    }
}

fn validate_mutation(mutation: &AgentActionMutation) -> Result<(), String> {
    if mutation.run_id.is_empty()
        || mutation.node_id.is_empty()
        || mutation.agent_id.is_empty()
        || mutation.action_type.is_empty()
        || mutation.action_sha256.len() != 64
        || !mutation
            .action_sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err("invalid agent action mutation identity or hash".to_string());
    }
    serde_json::from_str::<Value>(&mutation.result_json)
        .map_err(|error| format!("invalid agent action result JSON: {error}"))?;
    Ok(())
}

fn recursive_rejection_result_json(
    mutation: &AgentActionMutation,
    reason_code: &str,
) -> Result<String, String> {
    let mut result: Value = serde_json::from_str(&mutation.result_json)
        .map_err(|error| format!("invalid recursive action result JSON: {error}"))?;
    let object = result
        .as_object_mut()
        .ok_or_else(|| "recursive action result must be an object".to_string())?;
    object.insert("recursive_node_id".to_string(), Value::Null);
    object.insert("decision".to_string(), json!("rejected"));
    object.insert("reason_code".to_string(), json!(reason_code));
    Ok(result.to_string())
}

fn existing_receipt_sqlite(
    conn: &rusqlite::Connection,
    mutation: &AgentActionMutation,
) -> Result<Option<String>, String> {
    let existing = conn
        .query_row(
            "SELECT action_sha256, agent_id, result_json FROM agent_action_receipts
             WHERE run_id = ?1 AND node_id = ?2",
            params![mutation.run_id, mutation.node_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()
        .map_err(|error| error.to_string())?;
    verify_existing_receipt(existing, mutation)
}

#[cfg(feature = "pg")]
fn existing_receipt_pg(
    client: &mut impl postgres::GenericClient,
    mutation: &AgentActionMutation,
) -> Result<Option<String>, String> {
    let existing = client
        .query_opt(
            "SELECT action_sha256, agent_id, result_json FROM agent_action_receipts
             WHERE run_id = $1 AND node_id = $2",
            &[&mutation.run_id, &mutation.node_id],
        )
        .map_err(|error| error.to_string())?
        .map(|row| (row.get(0), row.get(1), row.get(2)));
    verify_existing_receipt(existing, mutation)
}

fn verify_existing_receipt(
    existing: Option<(String, String, String)>,
    mutation: &AgentActionMutation,
) -> Result<Option<String>, String> {
    let Some((action_sha256, agent_id, result_json)) = existing else {
        return Ok(None);
    };
    if action_sha256 != mutation.action_sha256 || agent_id != mutation.agent_id {
        return Err(format!(
            "agent action receipt conflict for {}/{}",
            mutation.run_id, mutation.node_id
        ));
    }
    Ok(Some(result_json))
}

fn apply_sqlite_operation(
    conn: &rusqlite::Connection,
    mutation: &AgentActionMutation,
    operation: &AgentMutationOp,
    now: &str,
) -> Result<(), String> {
    match operation {
        AgentMutationOp::UpdateAgentState {
            expected_updated_at,
            status,
            scratchpad_summary,
            metadata_patch,
        } => {
            let metadata_json = merge_agent_metadata_sqlite(
                conn,
                mutation,
                expected_updated_at,
                metadata_patch.as_ref(),
                now,
            )?;
            let scratchpad_summary = scratchpad_summary
                .as_ref()
                .map(|value| apply_size_cap_and_redact(value, MAX_SCRATCHPAD_BYTES));
            let affected = conn
                .execute(
                    "UPDATE agent_state SET
                         status = COALESCE(?1, status),
                         scratchpad_summary = COALESCE(?2, scratchpad_summary),
                        metadata_json = ?3,
                         updated_at = ?4
                     WHERE agent_id = ?5 AND run_id = ?6 AND updated_at = ?7",
                    params![
                        status,
                        &scratchpad_summary,
                        metadata_json,
                        now,
                        mutation.agent_id,
                        mutation.run_id,
                        expected_updated_at,
                    ],
                )
                .map_err(|error| error.to_string())?;
            require_one(affected, "agent state changed concurrently")
        }
        AgentMutationOp::AckMessage { message_id } => {
            let affected = conn
                .execute(
                    "UPDATE agent_mailbox SET status='acked', ack_at=?1
                     WHERE message_id=?2 AND to_agent_id=?3 AND run_id=?4 AND status='pending'",
                    params![now, message_id, mutation.agent_id, mutation.run_id],
                )
                .map_err(|error| error.to_string())?;
            require_one(affected, "message not found or already acknowledged")
        }
        AgentMutationOp::InsertProposal {
            proposal_id,
            correlation_id,
            parent_node_id,
            proposal_type,
            objective,
            context_summary,
            target_agent_id,
            proposed_node_id,
            proposed_edge_id,
        } => {
            let objective = apply_size_cap_and_redact(objective, MAX_PROPOSAL_OBJECTIVE_BYTES);
            let context = apply_size_cap_and_redact(context_summary, MAX_PROPOSAL_CONTEXT_BYTES);
            conn.execute(
                "INSERT INTO agent_proposals
                 (proposal_id, correlation_id, run_id, parent_node_id, agent_id,
                  target_agent_id, proposed_node_id, proposed_edge_id, proposal_type,
                  objective, context_summary, status, created_at, updated_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,'pending',?12,?13)",
                params![
                    proposal_id,
                    correlation_id,
                    mutation.run_id,
                    parent_node_id,
                    mutation.agent_id,
                    target_agent_id,
                    proposed_node_id,
                    proposed_edge_id,
                    proposal_type,
                    objective,
                    context,
                    now,
                    now,
                ],
            )
            .map_err(|error| error.to_string())?;
            append_audit_locked(
                conn,
                now,
                &format!("agent:{}", mutation.agent_id),
                "agent_proposal.create",
                &format!("agent_proposal/{proposal_id}"),
                &json!({
                    "proposal_id": proposal_id,
                    "correlation_id": correlation_id,
                    "proposal_type": proposal_type,
                    "run_id": mutation.run_id,
                    "agent_id": mutation.agent_id,
                    "parent_node_id": parent_node_id,
                }),
            )?;
            Ok(())
        }
        AgentMutationOp::PersistRecursiveTree {
            tree,
            expected_version,
        } => match super::recursive_execution::persist_recursive_tree_sqlite(
            conn,
            tree,
            now,
            *expected_version,
        ) {
            Ok(()) => {
                mark_recursive_proposals_accepted_sqlite(conn, mutation, tree, now)?;
                mark_recursive_proposals_rejected_sqlite(conn, mutation, tree, now)?;
                Ok(())
            }
            Err(error) if error == "stale_parent" => {
                let rejected =
                    match super::recursive_execution::record_recursive_cas_rejection_sqlite(
                        conn, tree, now,
                    ) {
                        Ok(rejected) => rejected,
                        Err(error) if error == "recursive_tree_missing" => {
                            return Err("recursive_tree_missing".to_string());
                        }
                        Err(error) => return Err(error),
                    };
                if let Some((_, reason_code)) = rejected.first() {
                    let result_json = recursive_rejection_result_json(mutation, reason_code)?;
                    let updated = conn
                        .execute(
                            "UPDATE agent_action_receipts SET result_json=?1
                             WHERE run_id=?2 AND node_id=?3 AND action_sha256=?4",
                            params![
                                result_json,
                                mutation.run_id,
                                mutation.node_id,
                                mutation.action_sha256,
                            ],
                        )
                        .map_err(|error| error.to_string())?;
                    require_one(updated, "recursive rejection receipt update failed")?;
                }
                for (proposal_id, reason_code) in rejected {
                    let resource = format!("agent_proposal/{proposal_id}");
                    conn.execute(
                        "UPDATE agent_proposals SET status='rejected', updated_at=?1
                         WHERE proposal_id=?2 AND run_id=?3 AND status='pending'",
                        params![now, proposal_id, mutation.run_id],
                    )
                    .map_err(|error| error.to_string())?;
                    append_audit_locked(
                        conn,
                        now,
                        &format!("agent:{}", mutation.agent_id),
                        "agent_step.recursive_proposal_rejected",
                        &resource,
                        &json!({
                            "run_id": mutation.run_id,
                            "proposal_id": proposal_id,
                            "reason_code": reason_code,
                            "evidence_persisted": true,
                        }),
                    )?;
                }
                Ok(())
            }
            Err(error) => Err(error),
        },
        AgentMutationOp::PersistRecursiveWorkflow { node, edge } => {
            let Some(proposal_id) = node.get("proposal_id").and_then(Value::as_str) else {
                return Err("recursive proposal identity is missing".to_string());
            };
            {
                let proposal_status: Option<String> = conn
                    .query_row(
                        "SELECT status FROM agent_proposals
                         WHERE proposal_id=?1 AND run_id=?2",
                        params![proposal_id, mutation.run_id],
                        |row| row.get(0),
                    )
                    .optional()
                    .map_err(|error| error.to_string())?;
                if proposal_status.as_deref() == Some("rejected") {
                    return Ok(());
                }
                if proposal_status.as_deref() == Some("accepted") {
                    // The tree mutation may have committed acceptance before
                    // this workflow insert in the same transaction.
                } else if proposal_status.as_deref() != Some("pending") {
                    return Err("recursive proposal is not pending".to_string());
                } else {
                    let updated = conn
                        .execute(
                            "UPDATE agent_proposals SET status='accepted', updated_at=?1
                             WHERE proposal_id=?2 AND run_id=?3 AND status='pending'",
                            params![now, proposal_id, mutation.run_id],
                        )
                        .map_err(|error| error.to_string())?;
                    if updated != 1 {
                        return Err("recursive proposal acceptance raced".to_string());
                    }
                }
            }
            super::recursive_execution::validate_recursive_workflow_mutation_sqlite(
                conn,
                &mutation.run_id,
                node,
                edge,
                &mutation.agent_id,
            )?;
            super::workflow_runs::dag_mutations::insert_workflow_run_node_locked(
                conn,
                &mutation.run_id,
                node,
            )?;
            super::workflow_runs::dag_mutations::insert_workflow_run_edge_locked(
                conn,
                &mutation.run_id,
                edge,
            )?;
            let actor = format!("agent:{}", mutation.agent_id);
            super::workflow_runs::insert_workflow_run_event_locked(
                conn,
                &mutation.run_id,
                node.get("node_id").and_then(Value::as_str),
                "dag.mutation.node_added",
                &actor,
                &json!({"recursive": true, "metadata_only": true}),
                now,
            )?;
            super::workflow_runs::insert_workflow_run_event_locked(
                conn,
                &mutation.run_id,
                None,
                "dag.mutation.edge_added",
                &actor,
                &json!({"recursive": true, "metadata_only": true}),
                now,
            )
            .map(|_| ())
        }
        AgentMutationOp::UpdateProposalStatus {
            proposal_id,
            new_status,
        } => {
            let affected = conn
                .execute(
                    "UPDATE agent_proposals SET status=?1, updated_at=?2
                     WHERE proposal_id=?3 AND run_id=?4 AND status='pending'",
                    params![new_status, now, proposal_id, mutation.run_id],
                )
                .map_err(|error| error.to_string())?;
            require_one(affected, "proposal is no longer pending")
        }
        AgentMutationOp::UpdateProposalStatusBound {
            proposal_id,
            new_status,
            expected_proposal_type,
            expected_correlation_id,
            expected_owner_agent_id,
            expected_target_agent_id,
            expected_review_blocking,
        } => {
            if let Some(expected_blocking) = expected_review_blocking {
                validate_review_mailbox_binding_sqlite(
                    conn,
                    mutation,
                    proposal_id,
                    expected_proposal_type,
                    expected_correlation_id,
                    expected_owner_agent_id.as_deref(),
                    expected_target_agent_id.as_deref(),
                    *expected_blocking,
                )?;
            }
            let affected = conn
                .execute(
                    "UPDATE agent_proposals SET status=?1, updated_at=?2
                     WHERE proposal_id=?3 AND run_id=?4 AND status='pending'
                       AND proposal_type=?5 AND correlation_id=?6
                       AND (?7 IS NULL OR agent_id=?7)
                       AND (?8 IS NULL OR target_agent_id=?8)",
                    params![
                        new_status,
                        now,
                        proposal_id,
                        mutation.run_id,
                        expected_proposal_type,
                        expected_correlation_id,
                        expected_owner_agent_id,
                        expected_target_agent_id,
                    ],
                )
                .map_err(|error| error.to_string())?;
            require_one(
                affected,
                "proposal evidence binding changed or is no longer pending",
            )
        }
        AgentMutationOp::UpdateDebateContext {
            proposal_id,
            expected_correlation_id,
            expected_context_summary,
            new_context_summary,
        } => {
            let context = apply_size_cap(new_context_summary, MAX_PROPOSAL_CONTEXT_BYTES);
            let affected = conn
                .execute(
                    "UPDATE agent_proposals SET context_summary=?1, updated_at=?2
                     WHERE proposal_id=?3 AND run_id=?4 AND status='pending'
                       AND proposal_type='debate_request' AND correlation_id=?5
                       AND context_summary=?6",
                    params![
                        context,
                        now,
                        proposal_id,
                        mutation.run_id,
                        expected_correlation_id,
                        expected_context_summary
                    ],
                )
                .map_err(|error| error.to_string())?;
            require_one(affected, "debate round changed concurrently")
        }
        AgentMutationOp::InsertMessage {
            message_id,
            from_agent_id,
            to_agent_id,
            message_type,
            body,
            correlation_id,
            reply_to_message_id,
            metadata,
        } => insert_message_sqlite(
            conn,
            mutation,
            message_id,
            from_agent_id,
            to_agent_id,
            message_type,
            body.as_deref(),
            correlation_id.as_deref(),
            reply_to_message_id.as_deref(),
            metadata,
            now,
        ),
        AgentMutationOp::AppendAudit {
            action,
            resource,
            details,
        } => append_audit_locked(conn, now, "agent_step", action, resource, details).map(|_| ()),
    }
}

#[allow(clippy::too_many_arguments)]
fn insert_message_sqlite(
    conn: &rusqlite::Connection,
    mutation: &AgentActionMutation,
    message_id: &str,
    from_agent_id: &str,
    to_agent_id: &str,
    message_type: &str,
    body: Option<&str>,
    correlation_id: Option<&str>,
    reply_to_message_id: Option<&str>,
    metadata: &Value,
    now: &str,
) -> Result<(), String> {
    let (safe_body, redaction_status) = redact_message_body(body);
    let body_summary = safe_body
        .as_ref()
        .map(|value| apply_size_cap(value, MAX_BODY_SUMMARY_BYTES));
    let metadata_json = metadata.to_string();
    conn.execute(
        "INSERT INTO agent_mailbox
         (message_id, correlation_id, from_agent_id, to_agent_id, run_id, node_id,
          message_type, status, body, body_summary, redaction_status, created_at,
          reply_to_message_id, metadata_json)
         VALUES (?1,?2,?3,?4,?5,?6,?7,'pending',?8,?9,?10,?11,?12,?13)",
        params![
            message_id,
            correlation_id,
            from_agent_id,
            to_agent_id,
            mutation.run_id,
            mutation.node_id,
            message_type,
            safe_body,
            body_summary,
            redaction_status,
            now,
            reply_to_message_id,
            metadata_json,
        ],
    )
    .map_err(|error| error.to_string())?;
    append_audit_locked(
        conn,
        now,
        &format!("agent:{from_agent_id}"),
        "agent_mailbox.send",
        &format!("agent_mailbox/{message_id}"),
        &json!({
            "message_id": message_id,
            "from_agent_id": from_agent_id,
            "to_agent_id": to_agent_id,
            "message_type": message_type,
            "redaction_status": redaction_status,
        }),
    )?;
    Ok(())
}

fn require_one(affected: usize, message: &str) -> Result<(), String> {
    if affected == 1 {
        Ok(())
    } else {
        Err(message.to_string())
    }
}

#[allow(clippy::too_many_arguments)]
fn validate_review_mailbox_binding_sqlite(
    conn: &rusqlite::Connection,
    mutation: &AgentActionMutation,
    proposal_id: &str,
    expected_proposal_type: &str,
    expected_correlation_id: &str,
    expected_owner_agent_id: Option<&str>,
    expected_target_agent_id: Option<&str>,
    expected_blocking: bool,
) -> Result<(), String> {
    if expected_proposal_type != "review_request" {
        return Err("review mailbox binding requires a review_request proposal".to_string());
    }
    let owner = expected_owner_agent_id
        .ok_or_else(|| "review mailbox binding requires the request owner".to_string())?;
    let target = expected_target_agent_id
        .ok_or_else(|| "review mailbox binding requires the target reviewer".to_string())?;
    let mut statement = conn
        .prepare(
            "SELECT mailbox.metadata_json
             FROM agent_mailbox AS mailbox
             JOIN agent_proposals AS proposal
               ON proposal.proposal_id=?1 AND proposal.run_id=?2
             WHERE proposal.status='pending'
               AND proposal.proposal_type=?3
               AND proposal.correlation_id=?4
               AND proposal.agent_id=?5
               AND proposal.target_agent_id=?6
               AND mailbox.run_id=proposal.run_id
               AND mailbox.node_id=proposal.parent_node_id
               AND mailbox.message_type='review_request'
               AND mailbox.correlation_id=proposal.correlation_id
               AND mailbox.from_agent_id=proposal.agent_id
               AND mailbox.to_agent_id=proposal.target_agent_id",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(
            params![
                proposal_id,
                mutation.run_id,
                expected_proposal_type,
                expected_correlation_id,
                owner,
                target,
            ],
            |row| row.get::<_, String>(0),
        )
        .map_err(|error| error.to_string())?;
    let metadata_rows = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    validate_review_mailbox_metadata(metadata_rows.into_iter(), proposal_id, expected_blocking)
}

#[cfg(feature = "pg")]
#[allow(clippy::too_many_arguments)]
fn validate_review_mailbox_binding_pg(
    client: &mut impl postgres::GenericClient,
    mutation: &AgentActionMutation,
    proposal_id: &str,
    expected_proposal_type: &str,
    expected_correlation_id: &str,
    expected_owner_agent_id: Option<&str>,
    expected_target_agent_id: Option<&str>,
    expected_blocking: bool,
) -> Result<(), String> {
    if expected_proposal_type != "review_request" {
        return Err("review mailbox binding requires a review_request proposal".to_string());
    }
    let owner = expected_owner_agent_id
        .ok_or_else(|| "review mailbox binding requires the request owner".to_string())?;
    let target = expected_target_agent_id
        .ok_or_else(|| "review mailbox binding requires the target reviewer".to_string())?;
    let rows = client
        .query(
            "SELECT mailbox.metadata_json
             FROM agent_mailbox AS mailbox
             JOIN agent_proposals AS proposal
               ON proposal.proposal_id=$1 AND proposal.run_id=$2
             WHERE proposal.status='pending'
               AND proposal.proposal_type=$3
               AND proposal.correlation_id=$4
               AND proposal.agent_id=$5
               AND proposal.target_agent_id=$6
               AND mailbox.run_id=proposal.run_id
               AND mailbox.node_id=proposal.parent_node_id
               AND mailbox.message_type='review_request'
               AND mailbox.correlation_id=proposal.correlation_id
               AND mailbox.from_agent_id=proposal.agent_id
               AND mailbox.to_agent_id=proposal.target_agent_id
             FOR UPDATE OF mailbox, proposal",
            &[
                &proposal_id,
                &mutation.run_id,
                &expected_proposal_type,
                &expected_correlation_id,
                &owner,
                &target,
            ],
        )
        .map_err(|error| error.to_string())?;
    validate_review_mailbox_metadata(
        rows.into_iter().map(|row| row.get::<_, String>(0)),
        proposal_id,
        expected_blocking,
    )
}

fn validate_review_mailbox_metadata(
    metadata_rows: impl Iterator<Item = String>,
    proposal_id: &str,
    expected_blocking: bool,
) -> Result<(), String> {
    let mut matching = 0usize;
    for raw in metadata_rows {
        let metadata: Value = serde_json::from_str(&raw)
            .map_err(|error| format!("invalid stored review mailbox metadata: {error}"))?;
        if metadata.get("proposal_id").and_then(Value::as_str) != Some(proposal_id) {
            continue;
        }
        if metadata.get("blocking").and_then(Value::as_bool) != Some(expected_blocking) {
            return Err("review request blocking evidence binding changed".to_string());
        }
        matching += 1;
    }
    if matching == 1 {
        Ok(())
    } else {
        Err("review request mailbox evidence binding changed".to_string())
    }
}

fn merge_agent_metadata_value(
    raw: &str,
    patch: Option<&Value>,
    now: &str,
) -> Result<String, String> {
    let mut metadata: Value = serde_json::from_str(raw)
        .map_err(|error| format!("invalid stored agent metadata: {error}"))?;
    if !metadata.is_object() {
        return Err("stored agent metadata must be an object".to_string());
    }
    if let Some(patch) = patch {
        let patch = patch
            .as_object()
            .ok_or_else(|| "agent metadata patch must be an object".to_string())?;
        let target = metadata.as_object_mut().expect("checked object");
        for (key, value) in patch {
            target.insert(key.clone(), value.clone());
        }
    }
    if let Some(digest) = metadata
        .get_mut("memory_digest")
        .and_then(Value::as_object_mut)
    {
        digest.insert("updated_at".to_string(), json!(now));
    }
    Ok(metadata.to_string())
}

fn merge_agent_metadata_sqlite(
    conn: &rusqlite::Connection,
    mutation: &AgentActionMutation,
    expected_updated_at: &str,
    patch: Option<&Value>,
    now: &str,
) -> Result<String, String> {
    let raw = conn
        .query_row(
            "SELECT metadata_json FROM agent_state
             WHERE agent_id=?1 AND run_id=?2 AND updated_at=?3",
            params![mutation.agent_id, mutation.run_id, expected_updated_at],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "agent state changed concurrently".to_string())?;
    merge_agent_metadata_value(&raw, patch, now)
}

#[cfg(feature = "pg")]
fn merge_agent_metadata_pg(
    client: &mut impl postgres::GenericClient,
    mutation: &AgentActionMutation,
    expected_updated_at: &str,
    patch: Option<&Value>,
    now: &str,
) -> Result<String, String> {
    let raw: String = client
        .query_opt(
            "SELECT metadata_json FROM agent_state
             WHERE agent_id=$1 AND run_id=$2 AND updated_at=$3 FOR UPDATE",
            &[&mutation.agent_id, &mutation.run_id, &expected_updated_at],
        )
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "agent state changed concurrently".to_string())?
        .get(0);
    merge_agent_metadata_value(&raw, patch, now)
}

#[cfg(feature = "pg")]
fn apply_pg_operation(
    client: &mut impl postgres::GenericClient,
    mutation: &AgentActionMutation,
    operation: &AgentMutationOp,
    now: &str,
) -> Result<(), String> {
    match operation {
        AgentMutationOp::UpdateAgentState {
            expected_updated_at,
            status,
            scratchpad_summary,
            metadata_patch,
        } => {
            let metadata_json = merge_agent_metadata_pg(
                client,
                mutation,
                expected_updated_at,
                metadata_patch.as_ref(),
                now,
            )?;
            let scratchpad_summary = scratchpad_summary
                .as_ref()
                .map(|value| apply_size_cap_and_redact(value, MAX_SCRATCHPAD_BYTES));
            let affected = client
                .execute(
                    "UPDATE agent_state SET
                         status = COALESCE($1, status),
                         scratchpad_summary = COALESCE($2, scratchpad_summary),
                         metadata_json = COALESCE($3, metadata_json), updated_at = $4
                     WHERE agent_id = $5 AND run_id = $6 AND updated_at = $7",
                    &[
                        status,
                        &scratchpad_summary,
                        &metadata_json,
                        &now,
                        &mutation.agent_id,
                        &mutation.run_id,
                        expected_updated_at,
                    ],
                )
                .map_err(|error| error.to_string())?;
            require_one(affected as usize, "agent state changed concurrently")
        }
        AgentMutationOp::AckMessage { message_id } => {
            let affected = client
                .execute(
                    "UPDATE agent_mailbox SET status='acked', ack_at=$1
                     WHERE message_id=$2 AND to_agent_id=$3 AND run_id=$4 AND status='pending'",
                    &[&now, message_id, &mutation.agent_id, &mutation.run_id],
                )
                .map_err(|error| error.to_string())?;
            require_one(
                affected as usize,
                "message not found or already acknowledged",
            )
        }
        AgentMutationOp::InsertProposal {
            proposal_id,
            correlation_id,
            parent_node_id,
            proposal_type,
            objective,
            context_summary,
            target_agent_id,
            proposed_node_id,
            proposed_edge_id,
        } => {
            let objective = apply_size_cap_and_redact(objective, MAX_PROPOSAL_OBJECTIVE_BYTES);
            let context = apply_size_cap_and_redact(context_summary, MAX_PROPOSAL_CONTEXT_BYTES);
            client
                .execute(
                    "INSERT INTO agent_proposals
                     (proposal_id, correlation_id, run_id, parent_node_id, agent_id,
                      target_agent_id, proposed_node_id, proposed_edge_id, proposal_type,
                      objective, context_summary, status, created_at, updated_at)
                     VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,'pending',$12,$13)",
                    &[
                        proposal_id,
                        correlation_id,
                        &mutation.run_id,
                        parent_node_id,
                        &mutation.agent_id,
                        target_agent_id,
                        proposed_node_id,
                        proposed_edge_id,
                        proposal_type,
                        &objective,
                        &context,
                        &now,
                        &now,
                    ],
                )
                .map_err(|error| error.to_string())?;
            pg_append_audit(
                client,
                now,
                &format!("agent:{}", mutation.agent_id),
                "agent_proposal.create",
                &format!("agent_proposal/{proposal_id}"),
                &json!({
                    "proposal_id": proposal_id,
                    "correlation_id": correlation_id,
                    "proposal_type": proposal_type,
                    "run_id": mutation.run_id,
                    "agent_id": mutation.agent_id,
                    "parent_node_id": parent_node_id,
                }),
            )
        }
        AgentMutationOp::PersistRecursiveTree {
            tree,
            expected_version,
        } => match super::recursive_execution::persist_recursive_tree_pg(
            client,
            tree,
            now,
            *expected_version,
        ) {
            Ok(()) => {
                mark_recursive_proposals_accepted_pg(client, mutation, tree, now)?;
                mark_recursive_proposals_rejected_pg(client, mutation, tree, now)?;
                Ok(())
            }
            Err(error) if error == "stale_parent" => {
                let rejected = match super::recursive_execution::record_recursive_cas_rejection_pg(
                    client, tree, now,
                ) {
                    Ok(rejected) => rejected,
                    Err(error) if error == "recursive_tree_missing" => {
                        return Err("recursive_tree_missing".to_string());
                    }
                    Err(error) => return Err(error),
                };
                if let Some((_, reason_code)) = rejected.first() {
                    let result_json = recursive_rejection_result_json(mutation, reason_code)?;
                    let updated = client
                        .execute(
                            "UPDATE agent_action_receipts SET result_json=$1
                             WHERE run_id=$2 AND node_id=$3 AND action_sha256=$4",
                            &[
                                &result_json,
                                &mutation.run_id,
                                &mutation.node_id,
                                &mutation.action_sha256,
                            ],
                        )
                        .map_err(|error| error.to_string())?;
                    require_one(
                        updated as usize,
                        "recursive rejection receipt update failed",
                    )?;
                }
                for (proposal_id, reason_code) in rejected {
                    let resource = format!("agent_proposal/{proposal_id}");
                    client
                        .execute(
                            "UPDATE agent_proposals SET status='rejected', updated_at=$1
                             WHERE proposal_id=$2 AND run_id=$3 AND status='pending'",
                            &[&now, &proposal_id, &mutation.run_id],
                        )
                        .map_err(|error| error.to_string())?;
                    super::workflow_runs::pg_append_audit(
                        client,
                        now,
                        &format!("agent:{}", mutation.agent_id),
                        "agent_step.recursive_proposal_rejected",
                        &resource,
                        &json!({
                            "run_id": mutation.run_id,
                            "proposal_id": proposal_id,
                            "reason_code": reason_code,
                            "evidence_persisted": true,
                        }),
                    )?;
                }
                Ok(())
            }
            Err(error) => Err(error),
        },
        AgentMutationOp::PersistRecursiveWorkflow { node, edge } => {
            let Some(proposal_id) = node.get("proposal_id").and_then(Value::as_str) else {
                return Err("recursive proposal identity is missing".to_string());
            };
            {
                let proposal_status: Option<String> = client
                    .query_opt(
                        "SELECT status FROM agent_proposals
                         WHERE proposal_id=$1 AND run_id=$2",
                        &[&proposal_id, &mutation.run_id],
                    )
                    .map_err(|error| error.to_string())?
                    .map(|row| row.get(0));
                if proposal_status.as_deref() == Some("rejected") {
                    return Ok(());
                }
                if proposal_status.as_deref() == Some("accepted") {
                    // The tree mutation may have committed acceptance before
                    // this workflow insert in the same transaction.
                } else if proposal_status.as_deref() != Some("pending") {
                    return Err("recursive proposal is not pending".to_string());
                } else {
                    let updated = client
                        .execute(
                            "UPDATE agent_proposals SET status='accepted', updated_at=$1
                             WHERE proposal_id=$2 AND run_id=$3 AND status='pending'",
                            &[&now, &proposal_id, &mutation.run_id],
                        )
                        .map_err(|error| error.to_string())?;
                    if updated != 1 {
                        return Err("recursive proposal acceptance raced".to_string());
                    }
                }
            }
            super::recursive_execution::validate_recursive_workflow_mutation_pg(
                client,
                &mutation.run_id,
                node,
                edge,
                &mutation.agent_id,
            )?;
            super::workflow_runs::dag_mutations::pg_insert_workflow_run_node(
                client,
                &mutation.run_id,
                node,
            )?;
            super::workflow_runs::dag_mutations::pg_insert_workflow_run_edge(
                client,
                &mutation.run_id,
                edge,
            )?;
            let actor = format!("agent:{}", mutation.agent_id);
            super::workflow_runs::pg_insert_workflow_run_event(
                client,
                &mutation.run_id,
                node.get("node_id").and_then(Value::as_str),
                "dag.mutation.node_added",
                &actor,
                &json!({"recursive": true, "metadata_only": true}),
                now,
            )?;
            super::workflow_runs::pg_insert_workflow_run_event(
                client,
                &mutation.run_id,
                None,
                "dag.mutation.edge_added",
                &actor,
                &json!({"recursive": true, "metadata_only": true}),
                now,
            )
            .map(|_| ())
        }
        AgentMutationOp::UpdateProposalStatus {
            proposal_id,
            new_status,
        } => {
            let affected = client
                .execute(
                    "UPDATE agent_proposals SET status=$1, updated_at=$2
                     WHERE proposal_id=$3 AND run_id=$4 AND status='pending'",
                    &[new_status, &now, proposal_id, &mutation.run_id],
                )
                .map_err(|error| error.to_string())?;
            require_one(affected as usize, "proposal is no longer pending")
        }
        AgentMutationOp::UpdateProposalStatusBound {
            proposal_id,
            new_status,
            expected_proposal_type,
            expected_correlation_id,
            expected_owner_agent_id,
            expected_target_agent_id,
            expected_review_blocking,
        } => {
            if let Some(expected_blocking) = expected_review_blocking {
                validate_review_mailbox_binding_pg(
                    client,
                    mutation,
                    proposal_id,
                    expected_proposal_type,
                    expected_correlation_id,
                    expected_owner_agent_id.as_deref(),
                    expected_target_agent_id.as_deref(),
                    *expected_blocking,
                )?;
            }
            let affected = client
                .execute(
                    "UPDATE agent_proposals SET status=$1, updated_at=$2
                     WHERE proposal_id=$3 AND run_id=$4 AND status='pending'
                       AND proposal_type=$5 AND correlation_id=$6
                       AND ($7::TEXT IS NULL OR agent_id=$7)
                       AND ($8::TEXT IS NULL OR target_agent_id=$8)",
                    &[
                        new_status,
                        &now,
                        proposal_id,
                        &mutation.run_id,
                        expected_proposal_type,
                        expected_correlation_id,
                        expected_owner_agent_id,
                        expected_target_agent_id,
                    ],
                )
                .map_err(|error| error.to_string())?;
            require_one(
                affected as usize,
                "proposal evidence binding changed or is no longer pending",
            )
        }
        AgentMutationOp::UpdateDebateContext {
            proposal_id,
            expected_correlation_id,
            expected_context_summary,
            new_context_summary,
        } => {
            let context = apply_size_cap(new_context_summary, MAX_PROPOSAL_CONTEXT_BYTES);
            let affected = client
                .execute(
                    "UPDATE agent_proposals SET context_summary=$1, updated_at=$2
                     WHERE proposal_id=$3 AND run_id=$4 AND status='pending'
                       AND proposal_type='debate_request' AND correlation_id=$5
                       AND context_summary=$6",
                    &[
                        &context,
                        &now,
                        proposal_id,
                        &mutation.run_id,
                        expected_correlation_id,
                        expected_context_summary,
                    ],
                )
                .map_err(|error| error.to_string())?;
            require_one(affected as usize, "debate round changed concurrently")
        }
        AgentMutationOp::InsertMessage {
            message_id,
            from_agent_id,
            to_agent_id,
            message_type,
            body,
            correlation_id,
            reply_to_message_id,
            metadata,
        } => insert_message_pg(
            client,
            mutation,
            message_id,
            from_agent_id,
            to_agent_id,
            message_type,
            body.as_deref(),
            correlation_id.as_deref(),
            reply_to_message_id.as_deref(),
            metadata,
            now,
        ),
        AgentMutationOp::AppendAudit {
            action,
            resource,
            details,
        } => pg_append_audit(client, now, "agent_step", action, resource, details),
    }
}

#[cfg(feature = "pg")]
#[allow(clippy::too_many_arguments)]
fn insert_message_pg(
    client: &mut impl postgres::GenericClient,
    mutation: &AgentActionMutation,
    message_id: &str,
    from_agent_id: &str,
    to_agent_id: &str,
    message_type: &str,
    body: Option<&str>,
    correlation_id: Option<&str>,
    reply_to_message_id: Option<&str>,
    metadata: &Value,
    now: &str,
) -> Result<(), String> {
    let (safe_body, redaction_status) = redact_message_body(body);
    let body_summary = safe_body
        .as_ref()
        .map(|value| apply_size_cap(value, MAX_BODY_SUMMARY_BYTES));
    let metadata_json = metadata.to_string();
    client
        .execute(
            "INSERT INTO agent_mailbox
             (message_id, correlation_id, from_agent_id, to_agent_id, run_id, node_id,
              message_type, status, body, body_summary, redaction_status, created_at,
              reply_to_message_id, metadata_json)
             VALUES ($1,$2,$3,$4,$5,$6,$7,'pending',$8,$9,$10,$11,$12,$13)",
            &[
                &message_id,
                &correlation_id,
                &from_agent_id,
                &to_agent_id,
                &mutation.run_id,
                &mutation.node_id,
                &message_type,
                &safe_body,
                &body_summary,
                &redaction_status,
                &now,
                &reply_to_message_id,
                &metadata_json,
            ],
        )
        .map_err(|error| error.to_string())?;
    pg_append_audit(
        client,
        now,
        &format!("agent:{from_agent_id}"),
        "agent_mailbox.send",
        &format!("agent_mailbox/{message_id}"),
        &json!({
            "message_id": message_id,
            "from_agent_id": from_agent_id,
            "to_agent_id": to_agent_id,
            "message_type": message_type,
            "redaction_status": redaction_status,
        }),
    )
}

#[cfg(feature = "pg")]
fn pg_append_audit(
    client: &mut impl postgres::GenericClient,
    now: &str,
    actor: &str,
    action: &str,
    resource: &str,
    details: &Value,
) -> Result<(), String> {
    let details_json = details.to_string();
    client
        .execute(
            "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
             VALUES ($1, $2, $3, $4, $5)",
            &[&now, &actor, &action, &resource, &details_json],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}
